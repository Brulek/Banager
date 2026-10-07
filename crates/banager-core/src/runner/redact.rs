//! Logins masked out of what a tool prints, before Banager shows, keeps or
//! logs it (F2 of the decisions-round review, and R1–R3 of its re-check).
//!
//! The proxy and mirror settings every command is handed
//! (`login_path::IMPORTED`) can hold a login --
//! `http://user:password@proxy:8080` -- and tools print such a setting
//! back, whole or in part, when something about it is wrong. curl, and so
//! Homebrew, says `Unsupported proxy syntax in 'http://user:password@…'`;
//! pip says `Failed to parse: http://user:password@…` as the last line of
//! its traceback; git, refused, says it `could not read Password for
//! 'https://user@host'`; npm, given a proxy written with no scheme, says
//! ``Invalid protocol `user:` ``. What a command prints is the operation's
//! log, its failure summary and a source's error, so `RealRunner` puts
//! every line it hands on, and every transcript meant for a person,
//! through [`Redactor::redact`] first (runner/real.rs, `StreamBuffer`).
//!
//! Two passes:
//!
//! - The logins in the settings themselves (`Redactor::for_settings`).
//!   Each value is read by fixed rules, never by what its parts look like
//!   (`Address::of`): a scheme only where the value starts with one; a
//!   proxy's login is all before the last `@` of the value; a mirror's
//!   all before the last `@` of its authority, so an `@` in a path is the
//!   path's -- unless what follows that `@` is no host and port, when the
//!   rules cannot read the value, and its login is all before its last
//!   `@`, as a proxy's is. With a login, the user name and the password
//!   are both secrets, whatever they look like: a name may itself be a
//!   token (`https://TOKEN:x-oauth-basic@github.com/…`). Each is masked
//!   wherever it appears -- as written, decoded and percent-encoded -- and
//!   so are the whole login, the whole value and the pair as an HTTP Basic
//!   credential, each in any case (npm prints a proxy's user name
//!   lowercased). A part shorter than [`SHORTEST_MASKED_ANYWHERE`], or one
//!   of [`COMMON_WORDS`], is masked only where it stands in its login.
//!
//! Why a command failed is read before any of this, off what it wrote
//! (`CommandOutput::failure_cause`): the mask can take the words that say
//! it.
//! - Any `scheme://user:password@` in the text (`mask_url_logins`),
//!   or `scheme://user@` in the text, whatever its source: the whole login.
//!
//! Both put [`MASK`] where a secret was, and leave the rest of the line as
//! the tool wrote it. Where a rule cannot tell, it masks too much rather
//! than too little.
use crate::runner::login_path::{LoginEnv, IMPORTED};
use base64::Engine;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

/// An Ollama instance id, with URL userinfo removed. Detection uses this
/// for public ids while retaining authentication privately; history and
/// settings also scrub legacy ids. Preserve every other byte so
/// credential-free ids keep matching. Mirrored by `artifactKeyId` in TS.
pub(crate) fn without_ollama_login(id: &str) -> Cow<'_, str> {
    static LOGIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(ollama:https?://)[^/?#|]*@").expect("a valid pattern"));
    LOGIN.replace(id, "${1}")
}

/// A complete endpoint, rather than a URL embedded in prose: quote characters
/// can be valid userinfo here and must not terminate the match.
pub fn mask_ollama_host(host: &str) -> Cow<'_, str> {
    static LOGIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^((?:[A-Za-z][A-Za-z0-9+.\-]*://)?)([^/?#]*)@").unwrap());
    LOGIN.replace(host, |c: &regex::Captures<'_>| {
        format!(
            "{}{}@",
            &c[1],
            if c[2].contains(':') {
                "****:****"
            } else {
                MASK
            }
        )
    })
}

/// A window-facing environment. Keep the execution environment untouched.
/// Normalized hosts have a scheme; accepting the bare form here also protects
/// older callers and test/mock data, including one-character usernames.
pub fn preview_env(env: &[(String, String)]) -> Vec<(String, String)> {
    env.iter()
        .map(|(name, value)| {
            let shown = if name == "OLLAMA_HOST" {
                mask_ollama_host(value).into_owned()
            } else {
                value.clone()
            };
            (name.clone(), shown)
        })
        .collect()
}

/// Serialization is a preview boundary, never a source for execution.
pub(crate) fn serialize_preview_env<S: serde::Serializer>(
    env: &[(String, String)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&preview_env(env), serializer)
}

/// What stands where a secret was.
pub const MASK: &str = "****";

/// A user name or a password this many characters long or longer
/// (decoded) is masked wherever it appears; a shorter one only where it
/// stands in its login (`:ab@`, the whole login, the whole value).
/// Masking every `ab` in a build log would make the log unreadable for a
/// secret that short.
pub const SHORTEST_MASKED_ANYWHERE: usize = 3;

/// User names and passwords masked only where they stand in their login,
/// never wherever they appear, compared ignoring case (ASCII). Each is a
/// word tools print for their own reasons, and none is a secret:
///
/// - the names a host has everyone write before a token, the token being
///   the password, or the word GitHub has everyone write after one;
/// - account names that are also words in tools' output (`git`, as in
///   `git@github.com:…`, which names an ssh user);
/// - sudo's words in "sudo: a password is required", so that sudo's lines,
///   shown with the steps for Terminal, stay readable. Why the operation
///   failed does not depend on them: the runner reads it before masking
///   (`CommandOutput::failure_cause`, re-check 2's N1), so a password that
///   is part of one of them (`pass`) masks that part and the cause stands.
///
/// The list is in docs/what-we-run.md, word for word (what_we_run_test).
pub const COMMON_WORDS: &[&str] = &[
    // Before or after a token.
    "x-oauth-basic",
    "x-access-token",
    "x-token-auth",
    "oauth2",
    "oauth",
    "gitlab-ci-token",
    "__token__",
    "token",
    // Account names.
    "git",
    "user",
    "username",
    "admin",
    "root",
    "guest",
    "anonymous",
    "proxy",
    "login",
    "test",
    "default",
    // sudo's words.
    "password",
    "required",
    "terminal",
    "sudo",
];

/// How much of the word next to a cut [`before_cut`] and [`after_cut`]
/// mask at most: far longer than any login, short enough that a runaway
/// line keeps nearly all of what is left of it.
pub const CUT_WINDOW: usize = 1024;

/// What percent-encoding leaves alone: RFC 3986's unreserved characters.
const NOT_UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The logins to mask in a command's output. `Default` knows no setting's
/// login and still masks any URL's complete userinfo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redactor {
    /// `(what, replacement)`: every `what` in the text is replaced, the
    /// longest first, so that a longer form is never left half-masked by a
    /// shorter one inside it. `what` is kept in lowercase (ASCII) and
    /// found ignoring case (`replace_ignoring_case`): a tool may print a
    /// secret in another case than it was written -- npm prints a proxy's
    /// user name lowercased, read as the scheme of an address (re-check
    /// 2's N3).
    rules: Vec<(String, String)>,
}

impl Redactor {
    /// The logins in `settings`, as `(name, value)`: of the values, only
    /// those that hold a login count (`Address::of`).
    pub fn for_settings<'a>(settings: impl IntoIterator<Item = (&'a str, &'a str)>) -> Redactor {
        let mut redactor = Redactor::default();
        for (name, value) in settings {
            if let Some(address) = Address::of(name, value) {
                redactor.mask_login(value, &address, name == "OLLAMA_HOST");
            }
        }
        // `MASK` itself, or any run of `*`, is no secret: replacing it
        // would mask the masks.
        redactor
            .rules
            .retain(|(what, _)| !what.is_empty() && !what.chars().all(|c| c == '*'));
        for (what, _) in &mut redactor.rules {
            what.make_ascii_lowercase();
        }
        redactor
            .rules
            .sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
        redactor.rules.dedup_by(|later, kept| later.0 == kept.0);
        redactor
    }

    /// The rules for one setting's login: `value` read as `address`.
    fn mask_login(&mut self, value: &str, address: &Address<'_>, force: bool) {
        let (user, password) = address.user_and_password();
        let user_anywhere = !user.is_empty() && masked_anywhere(user);
        let has_password = password.is_some_and(|password| !password.is_empty());
        if !force && !user_anywhere && !has_password {
            // A name alone that is no secret -- `git@github.com:…` names
            // an ssh user -- or no login at all (`http://:@proxy`).
            return;
        }
        let login = masked_login(user, password);

        // The whole value, as written and decoded: masked as the address
        // it is, so the host stays readable.
        let whole = format!("{}{login}@{}", address.scheme, address.rest);
        self.replace(value, &whole);
        if let Some(decoded) = decoded(value) {
            self.replace(decoded, &whole);
        }
        // The whole login, in every form.
        let login_anywhere = user_anywhere || password.is_some_and(masked_anywhere);
        for form in forms_of(address.login) {
            if login_anywhere {
                self.replace(form, &login);
            } else {
                self.replace(format!("{form}@"), &format!("{login}@"));
            }
        }
        // Each part, in every form.
        if user_anywhere {
            for form in forms_of(user) {
                self.replace(form, MASK);
            }
        }
        if let Some(password) = password.filter(|password| !password.is_empty()) {
            for form in forms_of(password) {
                if masked_anywhere(password) {
                    self.replace(form, MASK);
                } else {
                    self.replace(format!(":{form}@"), &format!(":{MASK}@"));
                }
            }
        }
        // How a tool that sends the login prints it: HTTP Basic, the pair
        // decoded as it is sent, and as written (`curl -v`'s
        // `Proxy-Authorization: Basic …`).
        let user_sent = decoded(user).unwrap_or_else(|| user.to_string());
        match password {
            Some(password) => {
                let password_sent = decoded(password).unwrap_or_else(|| password.to_string());
                self.replace(basic(&format!("{user_sent}:{password_sent}")), MASK);
                self.replace(basic(&format!("{user}:{password}")), MASK);
            }
            None => {
                self.replace(basic(&format!("{user_sent}:")), MASK);
                self.replace(basic(&user_sent), MASK);
            }
        }
    }

    fn replace(&mut self, what: impl Into<String>, with: &str) {
        self.rules.push((what.into(), with.to_string()));
    }

    /// What a command Banager runs is handed: the settings read from the
    /// login shell (`accepted`, from `login_path::accepted_env`), and the
    /// same-named settings of Banager's own environment, which a command
    /// inherits when the login shell did not set them.
    pub fn for_commands(accepted: Option<&LoginEnv>) -> Redactor {
        Self::for_command_env(accepted, &[])
    }

    /// Include command overrides and inherited OLLAMA_HOST in output masking.
    pub fn for_command_env(accepted: Option<&LoginEnv>, env: &[(String, String)]) -> Redactor {
        let mut settings: Vec<(String, String)> = accepted
            .map(|found| found.imported.clone())
            .unwrap_or_default();
        settings.extend(
            IMPORTED
                .iter()
                .filter_map(|name| Some((name.to_string(), std::env::var(name).ok()?))),
        );
        if let Ok(host) = std::env::var("OLLAMA_HOST") {
            settings.push(("OLLAMA_HOST".to_string(), host));
        }
        settings.extend(
            env.iter()
                .filter(|(name, _)| name == "OLLAMA_HOST")
                .cloned(),
        );
        Redactor::for_settings(
            settings
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        )
    }

    /// `text` with every login this knows, in any case, and every
    /// URL's complete userinfo, replaced by [`MASK`].
    /// Borrowed when there was nothing to mask.
    pub fn redact<'t>(&self, text: &'t str) -> Cow<'t, str> {
        let mut out = Cow::Borrowed(text);
        if self.rules.is_empty() {
            return mask_url_logins(text);
        }
        let mut lowered = text.to_ascii_lowercase();
        for (what, with) in &self.rules {
            if lowered.contains(what.as_str()) {
                out = Cow::Owned(replace_ignoring_case(&out, &lowered, what, with));
                lowered = out.to_ascii_lowercase();
            }
        }
        match mask_url_logins(&out) {
            Cow::Owned(masked) => Cow::Owned(masked),
            Cow::Borrowed(_) => out,
        }
    }
}

/// `text` with every `what` replaced by `with`, found ignoring case
/// (ASCII): `lowered` is `text` in lowercase, `what` is in lowercase.
/// Lowercasing ASCII changes no byte's position, so where `what` stands in
/// `lowered` is where it stands in `text`.
fn replace_ignoring_case(text: &str, lowered: &str, what: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    for (at, _) in lowered.match_indices(what) {
        out.push_str(&text[done..at]);
        out.push_str(with);
        done = at + what.len();
    }
    out.push_str(&text[done..]);
    out
}

/// A setting's value read as an address with a login, by fixed rules:
/// `{scheme}{login}@{rest}`.
#[derive(Debug, PartialEq, Eq)]
struct Address<'v> {
    /// `scheme://` when the value starts with one (`SCHEME`), else empty:
    /// a `://` further in is part of the login (a password `rev://secret`
    /// in a proxy written with no scheme, which curl accepts).
    scheme: &'v str,
    /// All before the `@` that ends the login.
    login: &'v str,
    /// All after it: the host, its port, and a mirror's path.
    rest: &'v str,
}

impl<'v> Address<'v> {
    /// `value`, the setting `name`'s, read as an address with a login; or
    /// `None` when it has none. A proxy's address has no path, so its
    /// login is all before the last `@` of the value, a `/`, `?`, `#` or
    /// `@` in its password included; a mirror's or a remote's ends where
    /// `mirror_login_end` says.
    fn of(name: &str, value: &'v str) -> Option<Address<'v>> {
        let scheme = SCHEME.find(value).map_or("", |found| found.as_str());
        let after = &value[scheme.len()..];
        let at = if is_proxy(name) {
            after.rfind('@')?
        } else {
            mirror_login_end(after)?
        };
        Some(Address {
            scheme,
            login: &after[..at],
            rest: &after[at + 1..],
        })
    }

    /// The user name, and the password when the login has a `:` (the
    /// first: a user name has none, a password may).
    fn user_and_password(&self) -> (&'v str, Option<&'v str>) {
        match self.login.split_once(':') {
            Some((user, password)) => (user, Some(password)),
            None => (self.login, None),
        }
    }
}

/// A scheme, at the very start of a value only.
static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*://").expect("a valid pattern"));

/// Whether the setting `name` is a proxy's address (`http_proxy`,
/// `https_proxy`, `all_proxy`, in either case), not a mirror's or a
/// remote's. `no_proxy` lists hosts, and is read as a mirror's would be:
/// it holds no `@`.
fn is_proxy(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "http_proxy" | "https_proxy" | "all_proxy"
    )
}

/// Where the login of a mirror's or a remote's address ends, `after`
/// being what follows its scheme: at the last `@` of its authority -- up
/// to the first `/`, `?` or `#`, as the rules read an address -- so that
/// an `@` in a path (`/npm/@scope`, `/x/user@example.com`) is the path's,
/// after a port too (`https://mirror.example:8443/x/user@example.com`).
///
/// When what follows that `@` -- or the authority as a whole, with no `@`
/// in it -- is no host and port (`reads_as_host`), the rules cannot read
/// the value: a `/`, `?` or `#` written into a password cut the authority
/// short (`https://user:pass/word@host/…`), and a tool that cannot read it
/// either prints it back whole. Then the login is all before the last `@`
/// of the value, as a proxy's is: which masks too much rather than too
/// little when such a value also has an `@` in its path.
fn mirror_login_end(after: &str) -> Option<usize> {
    let authority = &after[..after.find(['/', '?', '#']).unwrap_or(after.len())];
    if authority.is_empty() {
        // `file:///…`, or a path: no host, so no login either.
        return None;
    }
    match authority.rfind('@') {
        Some(at) if reads_as_host(&authority[at + 1..], true) => Some(at),
        None if reads_as_host(authority, false) => None,
        _ => after.rfind('@'),
    }
}

/// Whether `text` is a host, and its port if it has one, and nothing else
/// (`HOST_AND_PORT`): a domain name of two labels or more, the last
/// starting with a letter (so not a version, `1.2`), with or without the
/// final `.` of an absolute name (`mirror.example.:8443`); an IPv4 or
/// bracketed IPv6 address; or a name of one label with a letter in it --
/// an intranet's `nexus:8081`, `localhost` (re-check 2's N2).
///
/// `after_a_login`: what follows an `@` in the authority. There, a name of
/// one label is a host only with a port, or as `localhost`: in
/// `https://user:p@ss/word@mirror.example/`, a password with an `@` and a
/// `/` written into it, the rules read `ss` as the host, and the value is
/// read by its last `@` instead.
fn reads_as_host(text: &str, after_a_login: bool) -> bool {
    let Some(found) = HOST_AND_PORT.captures(text) else {
        return false;
    };
    let Some(label) = found.name("label") else {
        return true;
    };
    let label = label.as_str();
    label.chars().any(|c| c.is_ascii_alphabetic())
        && (!after_a_login
            || found.name("port").is_some()
            || label.eq_ignore_ascii_case("localhost"))
}

/// A host, and its port if it has one (`reads_as_host`).
static HOST_AND_PORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)^
        (?: (?:[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?\.)+ [A-Za-z][A-Za-z0-9-]* \.?
          | (?:[0-9]{1,3}\.){3}[0-9]{1,3}
          | \[[0-9A-Fa-f:.]+\]
          | (?P<label>[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?) )
        (?P<port> :[0-9]{1,5} )?
        $",
    )
    .expect("a valid pattern")
});

/// Whether a user name or a password is masked wherever it appears: as
/// decoded, [`SHORTEST_MASKED_ANYWHERE`] characters or longer, and not one
/// of [`COMMON_WORDS`]. Whatever it looks like otherwise: letters alone,
/// digits alone, with a `.` or not -- a token can be any of them.
fn masked_anywhere(part: &str) -> bool {
    let part = decoded(part).unwrap_or_else(|| part.to_string());
    part.chars().count() >= SHORTEST_MASKED_ANYWHERE
        && !COMMON_WORDS
            .iter()
            .any(|word| word.eq_ignore_ascii_case(&part))
}

/// A login with each part that is there masked: `****:****`, `****`
/// (a name alone), `:****` (a password alone).
fn masked_login(user: &str, password: Option<&str>) -> String {
    let user = if user.is_empty() { "" } else { MASK };
    match password {
        None => user.to_string(),
        Some("") => format!("{user}:"),
        Some(_) => format!("{user}:{MASK}"),
    }
}

/// `written`, and the forms a tool may print it in instead: decoded, and
/// percent-encoded (its hex digits in either case, as every form is found
/// ignoring case).
fn forms_of(written: &str) -> Vec<String> {
    let mut forms = vec![written.to_string()];
    if let Some(decoded) = decoded(written) {
        forms.push(utf8_percent_encode(&decoded, NOT_UNRESERVED).to_string());
        forms.push(decoded);
    }
    forms.retain(|form| !form.is_empty());
    forms.sort();
    forms.dedup();
    forms
}

/// `written` percent-decoded, when that is text.
fn decoded(written: &str) -> Option<String> {
    percent_decode_str(written)
        .decode_utf8()
        .ok()
        .map(|text| text.into_owned())
}

/// `pair` as an HTTP Basic credential.
fn basic(pair: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(pair)
}

/// The complete userinfo before the last `@` of a URL authority.
/// A path, query, fragment or surrounding punctuation ends the authority.
static URL_LOGIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([A-Za-z][A-Za-z0-9+.\-]*://)([^\s/?#'"`<>]*)@"#).expect("a valid pattern")
});

/// Masks both parts of any URL login, including a username alone: Git
/// configuration can supply a token there without an imported setting.
pub fn mask_url_logins(text: &str) -> Cow<'_, str> {
    URL_LOGIN.replace_all(text, |captures: &regex::Captures<'_>| {
        let pair = captures[2].contains(':');
        format!("{}{}@", &captures[1], if pair { "****:****" } else { MASK })
    })
}

/// Not part of a word for [`before_cut`] and [`after_cut`]: white space and
/// what tools put around an address.
fn ends_a_word(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\'' | '"' | '`' | '<' | '>')
}

/// `text` is followed by a cut -- bytes dropped to keep a runaway
/// transcript bounded -- so its last word may be the first half of a
/// login: masks it (its last [`CUT_WINDOW`] bytes at most).
pub fn before_cut(text: &str) -> Cow<'_, str> {
    let mut floor = text.len().saturating_sub(CUT_WINDOW);
    while !text.is_char_boundary(floor) {
        floor += 1;
    }
    let start = text[floor..]
        .char_indices()
        .rev()
        .take_while(|(_, c)| !ends_a_word(*c))
        .last()
        .map_or(text.len(), |(at, _)| floor + at);
    if start == text.len() {
        return Cow::Borrowed(text);
    }
    Cow::Owned(format!("{}{MASK}", &text[..start]))
}

/// `text` follows a cut, so its first word may be the second half of a
/// login: masks it (its first [`CUT_WINDOW`] bytes at most).
pub fn after_cut(text: &str) -> Cow<'_, str> {
    let mut ceiling = text.len().min(CUT_WINDOW);
    while !text.is_char_boundary(ceiling) {
        ceiling -= 1;
    }
    let end = text[..ceiling]
        .char_indices()
        .find(|(_, c)| ends_a_word(*c))
        .map_or(ceiling, |(at, _)| at);
    if end == 0 {
        return Cow::Borrowed(text);
    }
    Cow::Owned(format!("{MASK}{}", &text[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only an Ollama instance id's URL login goes: every other id, an `@`
    /// in a path among them, is handed back as it is (`Cow::Borrowed`), so
    /// a settings or history file without a login is never rewritten at
    /// load. The ids are built as the adapters build them
    /// (`model::instance_id`), an Ollama one from an `OLLAMA_HOST` with a
    /// scheme as `path_env::normalize_ollama_host` turns it into a URL:
    /// parsed by `url`, which percent-encodes an `@` in a password, and
    /// with no trailing `/`.
    #[test]
    fn test_without_ollama_login_changes_only_an_ollama_url_login() {
        let ollama = |host: &str| {
            let url = url::Url::parse(host).expect("a URL");
            crate::model::instance_id("ollama", Some(url.as_str().trim_end_matches('/')))
        };
        let id = |adapter: &str, path: &str| crate::model::instance_id(adapter, Some(path));
        for (login, plain) in [
            (
                ollama("http://alice:secret@server:11434"),
                "ollama:http://server:11434",
            ),
            (
                ollama("http://alice:p@ss@server:11434/ollama"),
                "ollama:http://server:11434/ollama",
            ),
            (
                ollama("http://token@[::1]:11434"),
                "ollama:http://[::1]:11434",
            ),
            (ollama("http://:secret@server"), "ollama:http://server"),
        ] {
            assert_eq!(without_ollama_login(&login), plain, "{login}");
        }
        for unchanged in [
            ollama("http://127.0.0.1:11434"),
            ollama("http://server:11434/a@b"),
            ollama("http://server:11434/?who=a@b"),
            id("pip", "/opt/homebrew/opt/python@3.13/bin/python3.13"),
            id("npm", "/opt/homebrew/opt/node@22"),
            id("brew", "/opt/homebrew"),
            crate::model::instance_id("standalone-claude", None),
        ] {
            assert!(
                matches!(without_ollama_login(&unchanged), Cow::Borrowed(same) if same == unchanged),
                "{unchanged} must be left as it is"
            );
        }
    }

    /// What curl 8.7.1 (macOS 27's `/usr/bin/curl`) printed for this
    /// setting, run read-only against a closed port on this Mac: it fails
    /// before connecting anywhere (exit 5).
    const CURL_PROXY: &str = "http://review-user:review-secret@127.0.0.1:invalid";
    const CURL_SAID: &str = "curl: (5) Unsupported proxy syntax in \
        'http://review-user:review-secret@127.0.0.1:invalid': \
        Port number was not a decimal number between 0 and 65535";

    /// The same setting written without a scheme, as curl printed it back:
    /// as written, with no `scheme://` for a pattern to find.
    const CURL_SAID_NO_SCHEME: &str = "curl: (5) Unsupported proxy syntax in \
        'review-user:review-secret@127.0.0.1:invalid': \
        Port number was not a decimal number between 0 and 65535";

    /// The last two lines of what pip 26.2.1 printed (`pip index versions`
    /// with the setting above, which fails before any request).
    const PIP_SAID: &str = "pip._vendor.urllib3.exceptions.LocationParseError: \
        Failed to parse: http://review-user:review-secret@127.0.0.1:invalid\n\
        pip._vendor.requests.exceptions.InvalidURL: \
        Failed to parse: http://review-user:review-secret@127.0.0.1:invalid";

    fn redactor(settings: &[(&str, &str)]) -> Redactor {
        Redactor::for_settings(settings.iter().copied())
    }

    /// What curl 8.7.1 printed on this Mac (exit 5, before connecting
    /// anywhere) for a proxy setting it cannot read: the setting back,
    /// whole, as written.
    fn curl_refuses(setting: &str) -> String {
        format!(
            "curl: (5) Unsupported proxy syntax in '{setting}': \
             Port number was not a decimal number between 0 and 65535"
        )
    }

    #[test]
    fn test_a_value_is_read_by_fixed_rules() {
        let read = |name: &str, value: &'static str| {
            Address::of(name, value).map(|address| (address.scheme, address.login, address.rest))
        };
        for (name, value, expected) in [
            // A scheme at the start only.
            (
                "http_proxy",
                "http://u:pw@proxy.lan:3128",
                Some(("http://", "u:pw", "proxy.lan:3128")),
            ),
            (
                "http_proxy",
                "u:rev://pw@proxy.lan:3128",
                Some(("", "u:rev://pw", "proxy.lan:3128")),
            ),
            (
                "ALL_PROXY",
                "socks5h://u:p@ss/w0rd@127.0.0.1:7891",
                Some(("socks5h://", "u:p@ss/w0rd", "127.0.0.1:7891")),
            ),
            ("https_proxy", "http://127.0.0.1:7890", None),
            // A mirror's authority ends at the first `/`, `?` or `#`.
            (
                "PIP_INDEX_URL",
                "https://u:pw@mirror.example/a@b",
                Some(("https://", "u:pw", "mirror.example/a@b")),
            ),
            (
                "PIP_INDEX_URL",
                "https://mirror.example:8443/x/user@example.com/simple",
                None,
            ),
            (
                "HOMEBREW_BREW_GIT_REMOTE",
                "git@github.com:Homebrew/brew.git",
                Some(("", "git", "github.com:Homebrew/brew.git")),
            ),
            // No host and port where the rules read one: the last `@`.
            (
                "PIP_INDEX_URL",
                "https://u:pass/word@mirror.example/a@b",
                Some(("https://", "u:pass/word@mirror.example/a", "b")),
            ),
            (
                "PIP_INDEX_URL",
                "https://u:p@ss/w0rd@mirror.example/",
                Some(("https://", "u:p@ss/w0rd", "mirror.example/")),
            ),
            ("RUSTUP_UPDATE_ROOT", "file:///Users/me@corp/rustup", None),
            ("no_proxy", "localhost,127.0.0.1,.corp", None),
            // A host of one label, or an absolute name, is a host
            // (re-check 2's N2) ...
            (
                "npm_config_registry",
                "https://nexus:8081/repository/npm/@scope/pkg",
                None,
            ),
            (
                "PIP_INDEX_URL",
                "https://mirror.example.:8443/x/user@example.com/simple",
                None,
            ),
            (
                "npm_config_registry",
                "https://u:pw@nexus:8081/repository/npm/@scope/pkg",
                Some(("https://", "u:pw", "nexus:8081/repository/npm/@scope/pkg")),
            ),
            (
                "PIP_INDEX_URL",
                "https://u:pw@localhost/x@y",
                Some(("https://", "u:pw", "localhost/x@y")),
            ),
            // ... but after an `@`, one with no port is a password's tail.
            (
                "PIP_INDEX_URL",
                "https://u:p@ss/w0rd@nexus:8081/",
                Some(("https://", "u:p@ss/w0rd", "nexus:8081/")),
            ),
        ] {
            assert_eq!(read(name, value), expected, "{name}={value}");
        }
    }

    #[test]
    fn test_curls_proxy_error_has_the_login_masked() {
        let said = redactor(&[("https_proxy", CURL_PROXY)])
            .redact(CURL_SAID)
            .into_owned();
        // The user name too, since the re-check's R1: a name can be a
        // token, and nothing in it says whether it is.
        assert_eq!(said, curl_refuses("http://****:****@127.0.0.1:invalid"));
        // With no setting known, the pattern masks both parts too:
        // either part may be a token supplied by Git configuration.
        assert_eq!(
            Redactor::default().redact(CURL_SAID),
            curl_refuses("http://****:****@127.0.0.1:invalid")
        );
    }

    #[test]
    fn test_pips_proxy_error_has_the_login_masked() {
        let said = redactor(&[("https_proxy", CURL_PROXY)])
            .redact(PIP_SAID)
            .into_owned();
        assert!(!said.contains("review-"), "{said}");
        assert_eq!(said.matches("//****:****@127.0.0.1:invalid").count(), 2);
        assert!(!Redactor::default()
            .redact(PIP_SAID)
            .contains("review-secret"));
        // And pip's line for a percent-encoded password (pip 26.2.1, this
        // Mac, before any request): the password as written.
        let r = redactor(&[(
            "http_proxy",
            "http://someone:p%40ss%2Fword@127.0.0.1:invalid",
        )]);
        assert_eq!(
            r.redact(
                "pip._vendor.requests.exceptions.InvalidURL: \
                 Failed to parse: http://someone:p%40ss%2Fword@127.0.0.1:invalid"
            ),
            "pip._vendor.requests.exceptions.InvalidURL: \
             Failed to parse: http://****:****@127.0.0.1:invalid"
        );
    }

    #[test]
    fn test_a_setting_printed_without_its_scheme_is_masked_by_its_own_login() {
        // No `scheme://` in the text, so only knowing the setting finds it.
        assert!(Redactor::default()
            .redact(CURL_SAID_NO_SCHEME)
            .contains("review-secret"));
        let said = redactor(&[("http_proxy", "review-user:review-secret@127.0.0.1:invalid")])
            .redact(CURL_SAID_NO_SCHEME)
            .into_owned();
        assert_eq!(said, curl_refuses("****:****@127.0.0.1:invalid"));
    }

    /// The re-check's R2: a proxy written with no scheme whose password
    /// holds `://`. curl 8.7.1 printed this on this Mac, exit 5, before
    /// connecting anywhere; npm 10.9.9 (a scratch `HOME`, a closed local
    /// registry, nothing sent) printed the second line.
    const R2_PROXY: &str = "review-user:rev://secret@127.0.0.1:invalid";
    const NPM_SAID: &str = "npm error Invalid protocol `review-user:` connecting to proxy ``";

    #[test]
    fn test_a_proxy_with_no_scheme_whose_password_holds_a_scheme_separator_is_masked() {
        // A scheme only at the very start: `review-user:rev://` is none,
        // so all before the last `@` is the login.
        assert_eq!(
            Address::of("https_proxy", R2_PROXY),
            Some(Address {
                scheme: "",
                login: "review-user:rev://secret",
                rest: "127.0.0.1:invalid",
            })
        );
        for name in ["http_proxy", "https_proxy", "ALL_PROXY"] {
            let r = redactor(&[(name, R2_PROXY)]);
            assert_eq!(
                r.redact(&curl_refuses(R2_PROXY)),
                curl_refuses("****:****@127.0.0.1:invalid"),
                "{name}"
            );
            assert_eq!(
                r.redact(NPM_SAID),
                "npm error Invalid protocol `****:` connecting to proxy ``",
                "{name}"
            );
            assert_eq!(r.redact("secret rev://secret"), "secret ****", "{name}");
        }
        // Read as a mirror's, the same: its authority is no host and
        // port, so all before the last `@`.
        assert_eq!(
            redactor(&[("PIP_INDEX_URL", R2_PROXY)]).redact(&curl_refuses(R2_PROXY)),
            curl_refuses("****:****@127.0.0.1:invalid")
        );
    }

    #[test]
    fn test_a_proxy_password_holding_a_slash_hash_or_question_mark_is_masked() {
        for (name, setting, password) in [
            (
                "https_proxy",
                "http://review-user:rev/secret@127.0.0.1:8080",
                "rev/secret",
            ),
            (
                "HTTPS_PROXY",
                "http://review-user:rev#secret@127.0.0.1:8080",
                "rev#secret",
            ),
            (
                "ALL_PROXY",
                "http://review-user:rev?secret@127.0.0.1:8080",
                "rev?secret",
            ),
            // No scheme, as curl also accepts and prints back as written.
            (
                "http_proxy",
                "review-user:rev/secret@127.0.0.1:8080",
                "rev/secret",
            ),
            // `@` and `/` both: all before the last `@` is the login.
            (
                "all_proxy",
                "socks5h://review-user:p@ss/w0rd@127.0.0.1:7891",
                "p@ss/w0rd",
            ),
        ] {
            let said = curl_refuses(setting);
            let masked = redactor(&[(name, setting)]).redact(&said).into_owned();
            assert_eq!(
                masked,
                said.replace(password, MASK).replace("review-user", MASK),
                "{name}={setting}"
            );
            assert!(masked.contains("'****:****@") || masked.contains("//****:****@"));
        }
    }

    #[test]
    fn test_a_mirror_password_holding_a_slash_hash_question_mark_or_at_is_masked() {
        // The authority, as the rules read it, ends at the first `/`, `?`
        // or `#`; here what follows its last `@` (or the authority, with
        // none) is no host and port, so the login is all before the last
        // `@` of the value.
        for (name, setting, password) in [
            (
                "HOMEBREW_CORE_GIT_REMOTE",
                "https://review-user:rev/secret@github.example/Homebrew/homebrew-core",
                "rev/secret",
            ),
            (
                "PIP_INDEX_URL",
                "https://review-user:rev#secret@pypi.mirror.example/simple",
                "rev#secret",
            ),
            (
                "UV_INDEX_URL",
                "https://review-user:rev?secret@pypi.mirror.example:8443/simple",
                "rev?secret",
            ),
            (
                "npm_config_registry",
                "https://review-user:p@ss/w0rd@registry.mirror.example/",
                "p@ss/w0rd",
            ),
            // An `@` in the path after the host is the path's.
            (
                "HOMEBREW_BOTTLE_DOMAIN",
                "https://review-user:pw4mirror@mirror.example/pkgs/foo@1.2",
                "pw4mirror",
            ),
            (
                "UV_DEFAULT_INDEX",
                "https://bot:s3cr3t-pw@mirror.example:8443/x/user@example.com/simple",
                "s3cr3t-pw",
            ),
            // R2's password in a mirror's address: its `//` ends the
            // authority as the rules read it.
            (
                "PIP_INDEX_URL",
                "https://review-user:rev://secret@pypi.mirror.example/simple",
                "rev://secret",
            ),
        ] {
            let said = format!("fatal: unable to access '{setting}/': URL rejected");
            let masked = redactor(&[(name, setting)]).redact(&said).into_owned();
            let user = setting["https://".len()..].split(':').next().unwrap();
            let expected = said
                .replace(password, MASK)
                .replace(&format!("//{user}:"), "//****:");
            assert_eq!(masked, expected, "{name}={setting}");
        }
    }

    #[test]
    fn test_an_at_in_a_mirrors_path_query_or_fragment_is_no_login() {
        // The re-check's R3: a mirror with a port and an address in its
        // path holds no login, and nothing of it is masked.
        for (name, mirror) in [
            (
                "PIP_INDEX_URL",
                "https://mirror.example:8443/x/user@example.com/simple",
            ),
            ("UV_INDEX_URL", "https://mirror.example/x/user@example.com"),
            ("npm_config_registry", "https://mirror.example/npm/@scope/"),
            ("HOMEBREW_API_DOMAIN", "https://[::1]:8443/a@b"),
            ("HOMEBREW_BOTTLE_DOMAIN", "http://localhost:8080/x@y"),
            ("RUSTUP_DIST_SERVER", "https://192.0.2.7/dist/foo@1.2.3.4/"),
            ("PIP_INDEX_URL", "https://mirror.example?mail=a@b.example"),
            ("PIP_INDEX_URL", "https://mirror.example#a@b.example"),
            // Re-check 2's N2: an intranet's host of one label, with a
            // port or without, and an absolute name's final `.`.
            (
                "npm_config_registry",
                "https://nexus:8081/repository/npm/@scope/pkg",
            ),
            ("npm_config_registry", "http://nexus/repository/npm/@scope/"),
            (
                "PIP_INDEX_URL",
                "https://mirror.example.:8443/x/user@example.com/simple",
            ),
            ("UV_INDEX_URL", "https://mirror.example./x/user@example.com"),
        ] {
            let r = redactor(&[(name, mirror)]);
            assert_eq!(r, Redactor::default(), "{mirror}");
            let said = format!("Looking in indexes: {mirror}");
            assert_eq!(r.redact(&said), said);
        }
    }

    #[test]
    fn test_every_form_of_a_percent_encoded_password_is_masked() {
        // The password is `p@ss/w0rd`, written percent-encoded as it has to
        // be in an address.
        let r = redactor(&[(
            "https_proxy",
            "http://someone:p%40ss%2Fw0rd@proxy.corp:3128",
        )]);
        for (said, gone) in [
            (
                "in 'http://someone:p%40ss%2Fw0rd@proxy.corp:3128'",
                "p%40ss%2Fw0rd",
            ),
            ("password p@ss/w0rd rejected", "p@ss/w0rd"),
            ("re-encoded p%40ss%2fw0rd here", "p%40ss%2fw0rd"),
            ("the login someone:p@ss/w0rd", "someone"),
            // In another case too (re-check 2's N3): masking too much
            // rather than missing a tool's own lowercasing.
            ("upper P%40ss%2Fw0rd differs", "P%40ss%2Fw0rd"),
        ] {
            let masked = r.redact(said).into_owned();
            assert!(!masked.contains(gone), "{said} -> {masked}");
            assert!(masked.contains(MASK), "{said} -> {masked}");
        }
        assert_eq!(
            r.redact("in 'http://someone:p%40ss%2Fw0rd@proxy.corp:3128'"),
            "in 'http://****:****@proxy.corp:3128'"
        );
        assert_eq!(
            r.redact("the login someone:p@ss/w0rd"),
            "the login ****:****"
        );
    }

    #[test]
    fn test_every_form_of_a_percent_encoded_user_name_is_masked() {
        // The name is `me@corp.example`, written `me%40corp.example`.
        let r = redactor(&[("https_proxy", "http://me%40corp.example:pw@proxy.corp:3128")]);
        for (said, expected) in [
            (
                "proxy auth for me@corp.example refused",
                "proxy auth for **** refused",
            ),
            ("as me%40corp.example", "as ****"),
            (
                "'http://me%40corp.example:pw@proxy.corp:3128'",
                "'http://****:****@proxy.corp:3128'",
            ),
        ] {
            assert_eq!(r.redact(said), expected, "{said}");
        }
    }

    #[test]
    fn test_a_basic_credential_for_the_login_is_masked() {
        // `curl -v` prints the header it sends a proxy:
        // base64("review-user:review-secret").
        let header = "> Proxy-Authorization: Basic cmV2aWV3LXVzZXI6cmV2aWV3LXNlY3JldA==";
        assert_eq!(
            redactor(&[("https_proxy", CURL_PROXY)]).redact(header),
            "> Proxy-Authorization: Basic ****"
        );
    }

    #[test]
    fn test_a_token_alone_before_the_at_of_an_https_mirror_is_masked() {
        let r = redactor(&[(
            "HOMEBREW_BOTTLE_DOMAIN",
            "https://ghp_mirrortoken42@mirror.example/homebrew-bottles",
        )]);
        let said = r
            .redact("fatal: unable to access 'https://ghp_mirrortoken42@mirror.example/x': 403")
            .into_owned();
        assert_eq!(
            said,
            "fatal: unable to access 'https://****@mirror.example/x': 403"
        );
        assert_eq!(
            r.redact("token ghp_mirrortoken42 refused"),
            "token **** refused"
        );
    }

    /// What git 2.54 (Apple Git-157) printed on this Mac, asked for the
    /// password of a login whose name is a token, with no credential
    /// helper and no prompt allowed (`git credential fill`: exit 128, no
    /// request made). The re-check's R1: git drops the password and prints
    /// the name before an `@`, not before a `:`.
    fn git_asks_again(name: &str) -> String {
        format!("fatal: could not read Password for 'https://{name}@github.com': terminal prompts disabled")
    }

    #[test]
    fn test_a_token_in_the_user_name_slot_is_masked_whatever_it_looks_like() {
        for token in [
            // Letters alone, which the old rules left be.
            "abcdefghijklmnopqrstuvwx",
            // A JWT's dots, which the old rules took for a person's name.
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJyZXZpZXcifQ.c2lnbmF0dXJl",
            "ghp_reviewtoken42xoauth",
        ] {
            let remote = format!("https://{token}:x-oauth-basic@github.com/Homebrew/brew");
            let r = redactor(&[("HOMEBREW_BREW_GIT_REMOTE", &remote)]);
            assert_eq!(
                r.redact(&git_asks_again(token)),
                git_asks_again(MASK),
                "{token}"
            );
            assert_eq!(
                r.redact(&format!(
                    "fatal: unable to access '{remote}/': The requested URL returned error: 403"
                )),
                "fatal: unable to access 'https://****:****@github.com/Homebrew/brew/': \
                 The requested URL returned error: 403",
                "{token}"
            );
            assert_eq!(
                r.redact(&format!("token {token} refused")),
                "token **** refused"
            );
            // `x-oauth-basic` is everyone's: a word, left be on its own.
            assert_eq!(r.redact("use x-oauth-basic"), "use x-oauth-basic");
        }
        // A proxy's name, the same: npm 10.9.9 printed it as the
        // "protocol" of a proxy written with no scheme.
        let r = redactor(&[(
            "https_proxy",
            "abcdefghijklmnopqrstuvwx:x-oauth-basic@127.0.0.1:8080",
        )]);
        assert_eq!(
            r.redact(
                "npm error Invalid protocol `abcdefghijklmnopqrstuvwx:` connecting to proxy ``"
            ),
            "npm error Invalid protocol `****:` connecting to proxy ``"
        );
    }

    /// What npm 10.9.9 printed on this Mac for a proxy written with no
    /// scheme (`npm view`, a scratch `HOME`, a closed local registry,
    /// nothing sent; the message is `@npmcli/agent`'s `getProxyAgent`):
    /// it reads the user name as the address's scheme, which a URL parser
    /// lowercases.
    fn npm_refuses(lowered_user: &str) -> String {
        format!("npm error Invalid protocol `{lowered_user}:` connecting to proxy ``")
    }

    #[test]
    fn test_a_secret_is_masked_in_whatever_case_a_tool_prints_it() {
        // Re-check 2's N3: the user name, mixed case or dotted, comes back
        // lowercased.
        for (proxy, lowered) in [
            (
                "ProxyTokenAbCdEfGhIjKlMn:x-oauth-basic@127.0.0.1:8080",
                "proxytokenabcdefghijklmn",
            ),
            (
                "Eyj.Token.AbCdEf:x-oauth-basic@127.0.0.1:8080",
                "eyj.token.abcdef",
            ),
            ("Review-User:Rev://Secret@127.0.0.1:invalid", "review-user"),
        ] {
            let r = redactor(&[("https_proxy", proxy)]);
            assert_eq!(
                r.redact(&npm_refuses(lowered)),
                npm_refuses(MASK),
                "{proxy}"
            );
        }
        // And any secret in any case: upper, lower, or mixed otherwise.
        let r = redactor(&[(
            "HOMEBREW_BREW_GIT_REMOTE",
            "https://AbCdEfGhIjKlMnOpQrStUvWx:x-oauth-basic@github.com/Homebrew/brew",
        )]);
        for said in [
            "token abcdefghijklmnopqrstuvwx",
            "token ABCDEFGHIJKLMNOPQRSTUVWX",
            "token aBcDeFgHiJkLmNoPqRsTuVwX",
        ] {
            assert_eq!(r.redact(said), "token ****", "{said}");
        }
        // A secret that is not ASCII keeps its case: only ASCII letters
        // are folded, which every scheme, host and token is made of.
        let r = redactor(&[("https_proxy", "http://me:Ünïcode-pw@proxy.lan:3128")]);
        assert_eq!(r.redact("Ünïcode-pw ünïcode-pw"), "**** ünïcode-pw");
    }

    #[test]
    fn test_a_user_name_is_masked_wherever_it_appears_the_macs_account_name_too() {
        // The old rules left a name that did not look like a token -- a
        // word, or one with a `.` -- since it may be the Mac account's,
        // and so in every path a tool prints. A name can be a token of
        // any shape (R1), so it is masked in those paths too: more than
        // needed, never less.
        for (name, setting) in [
            ("http_proxy", "http://brulek@proxy.lan:3128"),
            ("https_proxy", "http://brulek:s3cret-pw@proxy.lan:3128"),
            (
                "HOMEBREW_BOTTLE_DOMAIN",
                "https://brulek@mirror.example/homebrew-bottles",
            ),
            (
                "PIP_INDEX_URL",
                "https://brulek:apikey-0042@artifactory.corp.example/api/pypi/simple",
            ),
        ] {
            let r = redactor(&[(name, setting)]);
            assert_eq!(
                r.redact("==> Pouring /Users/brulek/Library/Caches/Homebrew/downloads/jq.tar.gz"),
                "==> Pouring /Users/****/Library/Caches/Homebrew/downloads/jq.tar.gz",
                "{setting}"
            );
        }
        let r = redactor(&[(
            "PIP_INDEX_URL",
            "https://john.doe:apikey-0042@artifactory.corp.example/api/pypi/simple",
        )]);
        assert_eq!(
            r.redact("Looking in indexes: https://john.doe:apikey-0042@artifactory.corp.example/api/pypi/simple"),
            "Looking in indexes: https://****:****@artifactory.corp.example/api/pypi/simple"
        );
        assert_eq!(
            r.redact("/Users/john.doe/.cache/pip"),
            "/Users/****/.cache/pip"
        );
    }

    #[test]
    fn test_a_name_alone_that_is_short_or_a_common_word_is_no_login() {
        // `git@github.com:Homebrew/brew.git` names an ssh user, not a
        // secret: masking every `git` would wreck Homebrew's own output.
        for (name, setting) in [
            (
                "HOMEBREW_BREW_GIT_REMOTE",
                "git@github.com:Homebrew/brew.git",
            ),
            (
                "HOMEBREW_CORE_GIT_REMOTE",
                "ssh://git@github.com/Homebrew/homebrew-core",
            ),
            ("HOMEBREW_BOTTLE_DOMAIN", "https://Token@mirror.example/x"),
            ("http_proxy", "http://ab@proxy.lan:3128"),
            ("https_proxy", "http://:@proxy.lan:8080"),
        ] {
            let r = redactor(&[(name, setting)]);
            assert_eq!(r, Redactor::default(), "{setting}");
        }
        let said = "==> git fetch ssh://git@github.com/Homebrew/brew";
        assert_eq!(
            redactor(&[(
                "HOMEBREW_BREW_GIT_REMOTE",
                "git@github.com:Homebrew/brew.git"
            )])
            .redact(said),
            "==> git fetch ssh://****@github.com/Homebrew/brew"
        );
    }

    #[test]
    fn test_a_short_part_is_masked_only_where_it_stands_in_its_login() {
        let r = redactor(&[("http_proxy", "http://u:ab@proxy.lan:8080")]);
        assert_eq!(
            r.redact("in 'u:ab@proxy.lan:8080' and abcdef, ab and u"),
            "in '****:****@proxy.lan:8080' and abcdef, ab and u"
        );
        assert_eq!(
            r.redact(&curl_refuses("http://u:ab@proxy.lan:8080")),
            curl_refuses("http://****:****@proxy.lan:8080")
        );
        // Three characters and up: wherever it appears.
        let r = redactor(&[("http_proxy", "http://u:abc@proxy.lan:8080")]);
        assert_eq!(r.redact("abcdef"), "****def");
    }

    /// What sudo 1.9 prints when it cannot ask for a password, as Homebrew
    /// passes it on (`needsPassword` in src/lib/failureCause.ts).
    const SUDO_SAID: &str = "sudo: a terminal is required to read the password; \
        either use the -S option to read from standard input or configure an askpass helper\n\
        sudo: a password is required";

    #[test]
    fn test_a_part_that_is_a_common_word_is_masked_only_where_it_stands_in_its_login() {
        use crate::history::{failure_cause, FailureCause};
        for (user, password) in [
            ("me", "password"),
            ("me", "Required"),
            ("admin", "terminal"),
            ("token", "sudo"),
        ] {
            let setting = format!("http://{user}:{password}@proxy.lan:3128");
            let r = redactor(&[("https_proxy", &setting)]);
            // sudo's words: left as written, and still read as sudo
            // needing a password.
            assert_eq!(r.redact(SUDO_SAID), SUDO_SAID, "{setting}");
            assert_eq!(
                failure_cause(&r.redact(SUDO_SAID)),
                Some(FailureCause::NeedsPassword),
                "{setting}"
            );
            // Where it stands in its login it is masked all the same:
            // the whole value, the whole login, `:password@`.
            assert_eq!(
                r.redact(&curl_refuses(&setting)),
                curl_refuses("http://****:****@proxy.lan:3128"),
                "{setting}"
            );
            assert_eq!(
                r.redact(&format!("'{user}:{password}@proxy.lan:3128'")),
                "'****:****@proxy.lan:3128'",
                "{setting}"
            );
            assert_eq!(
                r.redact(&format!("socks5://x:{password}@other.lan:1080")),
                "socks5://****:****@other.lan:1080",
                "{setting}"
            );
        }
        // A number, or letters that are no listed word, is masked
        // wherever it appears, a date's year included: more than needed,
        // never less.
        let r = redactor(&[("https_proxy", "http://me:2026@proxy.lan:3128")]);
        assert_eq!(
            r.redact("==> jq 1.8.1 was released 2026-10-01"),
            "==> jq 1.8.1 was released ****-10-01"
        );
        let r = redactor(&[("https_proxy", "http://me:hunter@proxy.lan:3128")]);
        assert_eq!(r.redact("sent hunter"), "sent ****");
    }

    #[test]
    fn test_common_words_are_lowercase_and_listed_once() {
        let mut seen = std::collections::BTreeSet::new();
        for word in COMMON_WORDS {
            assert_eq!(*word, word.to_ascii_lowercase());
            assert!(seen.insert(*word), "{word} twice");
            assert!(word.chars().count() >= SHORTEST_MASKED_ANYWHERE, "{word}");
        }
    }

    #[test]
    fn test_git_configuration_tokens_are_masked_without_setting_discovery() {
        for r in [
            Redactor::default(),
            redactor(&[("https_proxy", "http://proxy.example:3128")]),
        ] {
            for login in [
                "ReviewTapTokenAbCdEfGhIjKlMn",
                "ReviewTapTokenAbCdEfGhIjKlMn:dummy",
            ] {
                let input = format!("fatal: could not read Password for 'https://{login}@github.com': terminal prompts disabled");
                let output = r.redact(&input);
                assert!(!output.contains("ReviewTapToken"), "{output}");
                assert!(!output.contains("dummy"), "{output}");
                assert!(output.contains("@github.com"));
            }
            for public in [
                "https://github.com/team/user@example.com",
                "https://nexus:8081/npm/@scope/pkg",
                "https://example.com?email=a@b",
                "https://example.com#a@b",
            ] {
                assert_eq!(r.redact(public), public);
            }
        }
    }

    #[test]
    fn test_the_pattern_masks_any_url_login_and_nothing_else() {
        let r = Redactor::default();
        for (said, expected) in [
            (
                "Collecting x @ git+https://bot:hunter22@git.example/x.git",
                "Collecting x @ git+https://****:****@git.example/x.git",
            ),
            (
                "proxy socks5h://a:b@c@127.0.0.1:7891 failed",
                "proxy socks5h://****:****@127.0.0.1:7891 failed",
            ),
            (
                "ProxyError('http://u:pw@host:1')",
                "ProxyError('http://****:****@host:1')",
            ),
            (
                "https://example.com:8080/a@b",
                "https://example.com:8080/a@b",
            ),
            (
                "https://mirror.example:8443/x/user@example.com/simple",
                "https://mirror.example:8443/x/user@example.com/simple",
            ),
            ("ssh://git@github.com/x", "ssh://****@github.com/x"),
            ("mailto:someone@example.com", "mailto:someone@example.com"),
            ("http://u:****@host", "http://****:****@host"),
            ("no address here", "no address here"),
        ] {
            assert_eq!(r.redact(said), expected, "{said}");
        }
    }

    #[test]
    fn test_masking_twice_changes_nothing_more() {
        let r = redactor(&[
            ("https_proxy", CURL_PROXY),
            ("http_proxy", R2_PROXY),
            (
                "HOMEBREW_BREW_GIT_REMOTE",
                "https://abcdefghijklmnopqrstuvwx:x-oauth-basic@github.com/Homebrew/brew",
            ),
        ]);
        let git = git_asks_again("abcdefghijklmnopqrstuvwx");
        for said in [CURL_SAID, PIP_SAID, NPM_SAID, git.as_str()] {
            let once = r.redact(said).into_owned();
            assert_eq!(r.redact(&once), once);
        }
    }

    #[test]
    fn test_text_with_nothing_to_mask_is_not_copied() {
        let r = redactor(&[("https_proxy", CURL_PROXY)]);
        assert!(matches!(
            r.redact("==> Upgrading jq 1.7 -> 1.8\nhttps://ghcr.io/v2/x"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn test_settings_with_no_login_add_nothing() {
        assert_eq!(
            redactor(&[
                ("http_proxy", "http://127.0.0.1:7890"),
                ("no_proxy", "localhost,127.0.0.1,.corp"),
                (
                    "HOMEBREW_BOTTLE_DOMAIN",
                    "https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles",
                ),
                ("all_proxy", "socks5://127.0.0.1:7891"),
                (
                    "HOMEBREW_API_DOMAIN",
                    "https://mirror.example/path/with@sign"
                ),
                ("https_proxy", "http://:@proxy.lan:8080"),
                // An `@` in a mirror's path is no login, after a port too.
                ("PIP_INDEX_URL", "https://example.com:8080/a@b"),
                (
                    "npm_config_registry",
                    "https://mirror.example/npm/@scope/pkg"
                ),
                ("UV_INDEX_URL", "https://mirror.example/x/user@example.com"),
                (
                    "UV_DEFAULT_INDEX",
                    "https://mirror.example:8443/x/user@example.com/simple"
                ),
                (
                    "RUSTUP_DIST_SERVER",
                    "https://mirror.example/dist/foo@1.2.3.4/"
                ),
                ("RUSTUP_UPDATE_ROOT", "file:///Users/me@corp/rustup"),
            ]),
            Redactor::default()
        );
    }

    #[test]
    fn test_for_commands_knows_what_the_login_shell_set() {
        let found = LoginEnv {
            path: "/usr/bin:/bin".into(),
            imported: vec![("https_proxy".into(), CURL_PROXY.into())],
        };
        let said = Redactor::for_commands(Some(&found))
            .redact(CURL_SAID_NO_SCHEME)
            .into_owned();
        assert!(!said.contains("review-secret"), "{said}");
        assert!(!said.contains("review-user"), "{said}");
    }

    #[test]
    fn test_the_word_on_each_side_of_a_cut_is_masked() {
        assert_eq!(
            before_cut("Unsupported proxy syntax in 'http://u:sec"),
            "Unsupported proxy syntax in '****"
        );
        assert_eq!(after_cut("ret@127.0.0.1:invalid': Port"), "****': Port");
        assert_eq!(before_cut("ends a line\n"), "ends a line\n");
        assert_eq!(after_cut("\nstarts a line"), "\nstarts a line");
        assert_eq!(before_cut(""), "");
        assert_eq!(after_cut(""), "");
        // A runaway word loses at most the window next to the cut.
        let long = "x".repeat(3 * CUT_WINDOW);
        let before = before_cut(&long);
        assert_eq!(before.len(), 2 * CUT_WINDOW + MASK.len());
        assert!(before.ends_with(MASK));
        let after = after_cut(&long);
        assert_eq!(after.len(), 2 * CUT_WINDOW + MASK.len());
        assert!(after.starts_with(MASK));
        // Never inside a character.
        let wide = "é".repeat(CUT_WINDOW);
        assert!(before_cut(&wide).ends_with(MASK));
        assert!(after_cut(&wide).starts_with(MASK));
    }
}

#[cfg(test)]
mod ollama_preview_tests {
    use super::*;
    use crate::model::PlanAction;

    #[test]
    fn ollama_preview_masks_all_login_shapes_and_preserves_other_values() {
        for (raw, shown) in [
            (
                "http://alice:s%40cret@server:11434",
                "http://****:****@server:11434",
            ),
            ("https://a@server", "https://****@server"),
            ("http://:pw@[::1]:11434", "http://****:****@[::1]:11434"),
            ("a:b@server:80", "****:****@server:80"),
            ("http://a'@server", "http://****@server"),
            (
                "http://server/path@name?q=a@b",
                "http://server/path@name?q=a@b",
            ),
            ("http://server:11434", "http://server:11434"),
        ] {
            let env = vec![
                ("OLLAMA_HOST".into(), raw.into()),
                ("OTHER".into(), raw.into()),
            ];
            let preview = preview_env(&env);
            assert_eq!(preview[0].1, shown);
            assert_eq!(preview[1].1, raw);
            assert_eq!(env[0].1, raw);
            assert_eq!(preview_env(&preview), preview);
        }
    }

    #[test]
    fn both_command_variants_serialize_masked_but_keep_execution_environment() {
        let env = vec![(
            "OLLAMA_HOST".into(),
            "http://alice:secret@server:11434".into(),
        )];
        for action in [
            PlanAction::Command {
                program: "/mock/ollama".into(),
                args: vec!["pull".into()],
                env: env.clone(),
            },
            PlanAction::CommandThen {
                program: "/mock/ollama".into(),
                args: vec!["pull".into()],
                env: env.clone(),
                then: vec![vec!["rm".into()]],
            },
        ] {
            let wire = serde_json::to_string(&action).unwrap();
            assert!(!wire.contains("alice"));
            assert!(!wire.contains("secret"));
            assert!(wire.contains("http://****:****@server:11434"));
            let back: PlanAction = serde_json::from_str(&wire).unwrap();
            assert_eq!(serde_json::to_string(&back).unwrap(), wire);
            match action {
                PlanAction::Command { env: actual, .. }
                | PlanAction::CommandThen { env: actual, .. } => assert_eq!(actual, env),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn command_output_masks_ollama_login_from_explicit_environment() {
        let env = vec![("OLLAMA_HOST".into(), "alice:s%40cret@server".into())];
        let redactor = Redactor::for_command_env(None, &env);
        for text in [
            "alice:s%40cret@server",
            "failed for alice with s@cret",
            "s%40cret",
        ] {
            let shown = redactor.redact(text);
            assert!(!shown.contains("alice"));
            assert!(!shown.contains("s@cret"));
            assert!(!shown.contains("s%40cret"));
        }
        let short = Redactor::for_settings([("OLLAMA_HOST", "http://a'@server")]);
        assert!(!short
            .redact("GET http://a'@server/api/tags")
            .contains("a'@"));
    }
}
