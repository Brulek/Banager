//! Logins masked out of what a tool prints, before Banager shows, keeps or
//! logs it (F2 of the decisions-round review).
//!
//! The proxy and mirror settings every command is handed
//! (`login_path::IMPORTED`) can hold a login --
//! `http://user:password@proxy:8080` -- and tools print such a setting
//! back, whole, when something about it is wrong. curl, and so Homebrew,
//! says `Unsupported proxy syntax in 'http://user:password@…'`; pip says
//! `Failed to parse: http://user:password@…` as the last line of its
//! traceback. What a command prints is the operation's log, its failure
//! summary and a source's error, so `RealRunner` puts every line it hands
//! on, and every transcript meant for a person, through
//! [`Redactor::redact`] first (runner/real.rs, `StreamBuffer`).
//!
//! Two passes:
//!
//! - The logins in the settings themselves (`Redactor::for_settings`), in
//!   every form a tool may print them: the password as written, decoded
//!   and percent-encoded, and the `user:password` pair as an HTTP Basic
//!   credential (base64), which is how `curl -v` prints a proxy's login.
//!   An http(s) address whose login is a token alone (`https://token@…`)
//!   has the token masked the same way. This is what catches a setting
//!   printed in a shape no pattern knows: curl prints a proxy written
//!   without a scheme (`user:password@host:port`) just as it was written.
//! - Any `scheme://user:password@` in the text (`mask_url_logins`),
//!   whatever setting or file it came from.
//!
//! Both put [`MASK`] where the secret was and leave the rest of the line,
//! the user name included, as the tool wrote it.
use crate::runner::login_path::{LoginEnv, IMPORTED};
use base64::Engine;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

/// What stands where a password or a token was.
pub const MASK: &str = "****";

/// A password this many characters long or longer is masked wherever it
/// appears, unless it is letters alone or digits alone
/// ([`worth_masking_anywhere`]); a shorter one only where it stands as a
/// login, before an `@` (`:abc@`). Masking every `abc` in a build log
/// would make the log unreadable for a secret that short.
pub const SHORTEST_MASKED_ANYWHERE: usize = 4;

/// A mirror's user name or token this many characters long or longer is
/// masked wherever it appears when it looks like a token
/// ([`looks_like_a_token`]); any other only where it stands in the
/// address (`//name@`, `//name:`). An access token is far longer
/// (GitHub's are 40 characters and up); a person's name, which may be the
/// Mac account's and so in every path a tool prints, is not.
pub const SHORTEST_TOKEN: usize = 16;

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
/// login and still masks any `scheme://user:password@`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redactor {
    /// Masked wherever they appear, longest first, so that a longer form
    /// is never left half-masked by a shorter one inside it.
    anywhere: Vec<String>,
    /// Masked where they stand as a login: `(pattern, replacement)`.
    in_login: Vec<(String, String)>,
}

impl Redactor {
    /// The logins in `settings`, as `(name, value)`: of the values, only
    /// those that are an address with a login count.
    pub fn for_settings<'a>(settings: impl IntoIterator<Item = (&'a str, &'a str)>) -> Redactor {
        let mut redactor = Redactor::default();
        for (name, value) in settings {
            match Login::in_setting(name, value) {
                Some(Login::Password {
                    user,
                    password,
                    token,
                }) => {
                    for form in forms_of(password) {
                        redactor.mask_password(form);
                    }
                    if token {
                        for form in forms_of(user) {
                            redactor.mask_name(form, ':');
                        }
                    }
                    // How a tool that sends the login prints it: HTTP Basic,
                    // the pair decoded as it is sent, and as written.
                    redactor.mask_anywhere(basic(&format!(
                        "{}:{}",
                        decoded(user).unwrap_or_else(|| user.to_string()),
                        decoded(password).unwrap_or_else(|| password.to_string())
                    )));
                    redactor.mask_anywhere(basic(&format!("{user}:{password}")));
                }
                Some(Login::Token(token)) => {
                    for form in forms_of(token) {
                        redactor.mask_name(form, '@');
                    }
                    let token = decoded(token).unwrap_or_else(|| token.to_string());
                    redactor.mask_anywhere(basic(&format!("{token}:")));
                    redactor.mask_anywhere(basic(&token));
                }
                None => {}
            }
        }
        redactor
            .anywhere
            .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        redactor.anywhere.dedup();
        redactor
    }

    /// Masks a password's `form` where it stands as one (`:{form}@`), and
    /// anywhere at all when that is worth it ([`worth_masking_anywhere`]).
    fn mask_password(&mut self, form: String) {
        self.mask_in_login(format!(":{form}@"), format!(":{MASK}@"));
        if worth_masking_anywhere(&form) {
            self.mask_anywhere(form);
        }
    }

    /// Masks a name's `form` where it stands in an address
    /// (`//{form}{then}`), and anywhere at all when it looks like a token
    /// ([`looks_like_a_token`]).
    fn mask_name(&mut self, form: String, then: char) {
        self.mask_in_login(format!("//{form}{then}"), format!("//{MASK}{then}"));
        if looks_like_a_token(&form) {
            self.mask_anywhere(form);
        }
    }

    fn mask_in_login(&mut self, login: String, masked: String) {
        let login = (login, masked);
        if !self.in_login.contains(&login) {
            self.in_login.push(login);
        }
    }

    fn mask_anywhere(&mut self, secret: String) {
        if !secret.is_empty() && secret != MASK {
            self.anywhere.push(secret);
        }
    }

    /// What a command Banager runs is handed: the settings read from the
    /// login shell (`accepted`, from `login_path::accepted_env`), and the
    /// same-named settings of Banager's own environment, which a command
    /// inherits when the login shell did not set them.
    pub fn for_commands(accepted: Option<&LoginEnv>) -> Redactor {
        let mut settings: Vec<(String, String)> = accepted
            .map(|found| found.imported.clone())
            .unwrap_or_default();
        settings.extend(
            IMPORTED
                .iter()
                .filter_map(|name| Some((name.to_string(), std::env::var(name).ok()?))),
        );
        Redactor::for_settings(
            settings
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        )
    }

    /// `text` with every login this knows, and every
    /// `scheme://user:password@`'s password, replaced by [`MASK`].
    /// Borrowed when there was nothing to mask.
    pub fn redact<'t>(&self, text: &'t str) -> Cow<'t, str> {
        let mut out = Cow::Borrowed(text);
        for secret in &self.anywhere {
            if out.contains(secret.as_str()) {
                out = Cow::Owned(out.replace(secret.as_str(), MASK));
            }
        }
        for (login, masked) in &self.in_login {
            if out.contains(login.as_str()) {
                out = Cow::Owned(out.replace(login.as_str(), masked));
            }
        }
        match mask_url_logins(&out) {
            Cow::Owned(masked) => Cow::Owned(masked),
            Cow::Borrowed(_) => out,
        }
    }
}

/// A login as a setting's value holds it: what sits before an `@` of the
/// address (what follows `scheme://`, or the value as a whole when it has
/// no scheme, as curl accepts a proxy written that way) -- which `@`, by
/// the setting's name (`Login::in_setting`).
#[derive(Debug, PartialEq, Eq)]
enum Login<'v> {
    /// `user:password@`, with a password: the password is the secret,
    /// and on a mirror's or a remote's http(s) address (`token`), the
    /// name too, which may be a token: GitHub's documented form is
    /// `https://TOKEN:x-oauth-basic@github.com/…`.
    Password {
        user: &'v str,
        password: &'v str,
        token: bool,
    },
    /// `token@` on an http(s) address: no password, so the name is the
    /// secret (a mirror's access token). Elsewhere a name alone is no
    /// secret: `git@github.com:…` names an ssh user.
    Token(&'v str),
}

impl<'v> Login<'v> {
    fn in_setting(name: &str, value: &'v str) -> Option<Login<'v>> {
        let (scheme, rest) = match value.find("://") {
            Some(at) => (&value[..at], &value[at + 3..]),
            None => ("", value),
        };
        // A password may hold a `/`, `?` or `#` as written: an address
        // read by the rules, authority up to the first of them, would cut
        // the login off before its `@`, and the tool that cannot read it
        // either prints it back whole (curl: "Unsupported proxy syntax").
        let userinfo = if is_proxy(name) {
            // A proxy's address has no path: all before its last `@`.
            &rest[..rest.rfind('@')?]
        } else {
            mirror_userinfo(rest)?
        };
        let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
        let scheme = scheme.to_ascii_lowercase();
        let http = ["http", "https"]
            .iter()
            .any(|name| scheme == *name || scheme.ends_with(&format!("+{name}")));
        // A proxy's name is the account's (NTLM, Kerberos), never a token.
        let token = http && !user.is_empty() && !is_proxy(name);
        if !password.is_empty() {
            return Some(Login::Password {
                user,
                password,
                token,
            });
        }
        token.then_some(Login::Token(user))
    }
}

/// Whether a password's form is worth masking wherever it appears: long
/// enough ([`SHORTEST_MASKED_ANYWHERE`]), and not letters alone or digits
/// alone. A password that is a plain word or number (`password`,
/// `required`, `2026`) stands in sudo's "a password is required", which
/// Banager reads to say that an operation needs Terminal
/// (`history::failure_cause`, src/lib/failureCause.ts), and in dates and
/// sizes: masked there, the reading fails and the log loses its words.
/// Such a password is masked where it stands as a login (`:password@`),
/// which is where tools print it.
fn worth_masking_anywhere(secret: &str) -> bool {
    secret.chars().count() >= SHORTEST_MASKED_ANYWHERE
        && !secret.chars().all(char::is_alphabetic)
        && !secret.chars().all(char::is_numeric)
}

/// Whether a mirror's user name or token looks like a token, and so is
/// masked wherever it appears: long ([`SHORTEST_TOKEN`]), worth masking
/// as a password would be, and with no `.` or `@`, which a person's name
/// or address has and a token does not.
fn looks_like_a_token(name: &str) -> bool {
    name.chars().count() >= SHORTEST_TOKEN
        && worth_masking_anywhere(name)
        && !name.contains(['.', '@'])
}

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

/// The login of a mirror's or a remote's address, `rest` being what follows
/// its `scheme://`: what stands before the last `@` that a host follows
/// (`/`, `?`, `#` and `@` in a password included), when what stands
/// before it reads as a login -- a name with no `/`, `?` or `#`, then
/// `:` and the password; failing that, before the last `@` of the
/// authority (up to the first `/`, `?` or `#`), as the rules read it. An
/// `@` in a path (`/npm/@scope`, `/foo@1.2`, `/x/user@example.com`) is
/// the path's.
fn mirror_userinfo(rest: &str) -> Option<&str> {
    let before_a_host = rest.match_indices('@').map(|(at, _)| at).rev().find(|&at| {
        let name = rest[..at].split(':').next().unwrap_or_default();
        HOST_FIRST.is_match(&rest[at + 1..]) && !name.contains(['/', '?', '#'])
    });
    if let Some(at) = before_a_host {
        return Some(&rest[..at]);
    }
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    Some(&authority[..authority.rfind('@')?])
}

/// A host, and its port if it has one, then the end of the address or
/// of its authority: a domain name of two or more labels, the last
/// starting with a letter (so not a version, `1.2`), an IPv4 or bracketed
/// IPv6 address, or `localhost`.
static HOST_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)^
        (?: (?:[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?\.)+ [A-Za-z][A-Za-z0-9-]*
          | (?:[0-9]{1,3}\.){3}[0-9]{1,3}
          | \[[0-9A-Fa-f:.]+\]
          | (?i:localhost) )
        (?: :[0-9]{1,5} )?
        (?: [/?\#] | $ )",
    )
    .expect("a valid pattern")
});

/// `written`, and the forms a tool may print it in instead: decoded, and
/// percent-encoded with upper- and lowercase hex digits.
fn forms_of(written: &str) -> Vec<String> {
    let mut forms = vec![written.to_string()];
    if let Some(decoded) = decoded(written) {
        let upper = utf8_percent_encode(&decoded, NOT_UNRESERVED).to_string();
        forms.push(lowercase_hex(&upper));
        forms.push(upper);
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

/// `encoded` with the two hex digits after each `%` in lowercase, and
/// nothing else changed.
fn lowercase_hex(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len());
    let mut hex_left = 0;
    for c in encoded.chars() {
        if hex_left > 0 {
            out.push(c.to_ascii_lowercase());
            hex_left -= 1;
        } else {
            out.push(c);
            if c == '%' {
                hex_left = 2;
            }
        }
    }
    out
}

/// `pair` as an HTTP Basic credential.
fn basic(pair: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(pair)
}

/// `scheme://`, a user name (no `@` or `:`), `:`, and then -- greedy, so
/// up to the last `@` before the authority ends -- the password. The
/// authority ends at `/`, `?`, `#`, white space, a quote or `<`/`>`, as
/// it does where tools print an address inside a sentence.
static URL_LOGIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([A-Za-z][A-Za-z0-9+.\-]*://[^\s/?#@:'"`<>]*:)[^\s/?#'"`<>]+@"#)
        .expect("a valid pattern")
});

/// Masks the password of every `scheme://user:password@` in `text`.
pub fn mask_url_logins(text: &str) -> Cow<'_, str> {
    URL_LOGIN.replace_all(text, format!("${{1}}{MASK}@").as_str())
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

    #[test]
    fn test_curls_proxy_error_has_the_password_masked() {
        let said = redactor(&[("https_proxy", CURL_PROXY)])
            .redact(CURL_SAID)
            .into_owned();
        assert_eq!(
            said,
            "curl: (5) Unsupported proxy syntax in \
             'http://review-user:****@127.0.0.1:invalid': \
             Port number was not a decimal number between 0 and 65535"
        );
        // And with no setting known at all, by the pattern alone.
        assert_eq!(Redactor::default().redact(CURL_SAID), said);
    }

    #[test]
    fn test_pips_proxy_error_has_the_password_masked() {
        let said = redactor(&[("https_proxy", CURL_PROXY)])
            .redact(PIP_SAID)
            .into_owned();
        assert!(!said.contains("review-secret"), "{said}");
        assert_eq!(
            said.matches("review-user:****@127.0.0.1:invalid").count(),
            2
        );
        assert!(!Redactor::default()
            .redact(PIP_SAID)
            .contains("review-secret"));
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
        assert!(!said.contains("review-secret"), "{said}");
        assert!(
            said.contains("'review-user:****@127.0.0.1:invalid'"),
            "{said}"
        );
    }

    /// What curl 8.7.1 printed on this Mac (exit 5, before connecting
    /// anywhere) for a proxy whose password holds a `/`, `#` or `?` as
    /// written, not percent-encoded: it reads the address as ending there,
    /// fails on the "port", and prints the setting back whole.
    fn curl_refuses(setting: &str) -> String {
        format!(
            "curl: (5) Unsupported proxy syntax in '{setting}': \
             Port number was not a decimal number between 0 and 65535"
        )
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
            assert_eq!(masked, said.replace(password, MASK), "{name}={setting}");
            assert!(
                masked.contains("'review-user:****@") || masked.contains("//review-user:****@")
            );
        }
    }

    #[test]
    fn test_a_mirror_password_holding_a_slash_hash_question_mark_or_at_is_masked() {
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
        ] {
            let said = format!("fatal: unable to access '{setting}/': URL rejected");
            let masked = redactor(&[(name, setting)]).redact(&said).into_owned();
            // A mirror's name is masked where it stands too: it may be a
            // token (`test_a_token_in_the_user_name_slot_of_a_remote_is_masked`).
            let expected = said
                .replace(password, MASK)
                .replace("//review-user:", "//****:");
            assert_eq!(masked, expected, "{name}={setting}");
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
            ("upper P%40ss%2Fw0rd differs", ""),
        ] {
            let masked = r.redact(said).into_owned();
            if gone.is_empty() {
                // `P` is not `p`: another string, left alone.
                assert_eq!(masked, said);
            } else {
                assert!(!masked.contains(gone), "{said} -> {masked}");
                assert!(masked.contains(MASK), "{said} -> {masked}");
            }
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

    #[test]
    fn test_a_token_in_the_user_name_slot_of_a_remote_is_masked() {
        // GitHub's documented token-as-user-name form: the "password" is
        // a fixed word, the token stands where a user name would.
        let r = redactor(&[(
            "HOMEBREW_BREW_GIT_REMOTE",
            "https://ghp_reviewtoken42xoauth:x-oauth-basic@github.com/Homebrew/brew",
        )]);
        assert_eq!(
            r.redact(
                "fatal: unable to access \
                 'https://ghp_reviewtoken42xoauth:x-oauth-basic@github.com/Homebrew/brew/': \
                 The requested URL returned error: 403"
            ),
            "fatal: unable to access 'https://****:****@github.com/Homebrew/brew/': \
             The requested URL returned error: 403"
        );
        assert_eq!(
            r.redact("token ghp_reviewtoken42xoauth refused"),
            "token **** refused"
        );
        // A name that is no token is masked in the address only: it may be
        // the Mac account's.
        let r = redactor(&[(
            "PIP_INDEX_URL",
            "https://jdoe:apikey-0042@artifactory.corp.example/api/pypi/simple",
        )]);
        assert_eq!(
            r.redact("Looking in indexes: https://jdoe:apikey-0042@artifactory.corp.example/api/pypi/simple"),
            "Looking in indexes: https://****:****@artifactory.corp.example/api/pypi/simple"
        );
        let path = "/Users/jdoe/.cache/pip";
        assert_eq!(r.redact(path), path);
        // A proxy's user name stays as written: a proxy is not given a
        // token that way.
        let r = redactor(&[("https_proxy", "http://jdoe:s3cret-pw@proxy.lan:3128")]);
        assert_eq!(
            r.redact(&curl_refuses("http://jdoe:s3cret-pw@proxy.lan:3128")),
            curl_refuses("http://jdoe:****@proxy.lan:3128")
        );
    }

    #[test]
    fn test_a_git_remote_user_name_is_no_login() {
        // `git@github.com:Homebrew/brew.git` names an ssh user, not a
        // secret: masking every `git` would wreck Homebrew's own output.
        let r = redactor(&[(
            "HOMEBREW_BREW_GIT_REMOTE",
            "git@github.com:Homebrew/brew.git",
        )]);
        assert_eq!(r, Redactor::default());
        let said = "==> git fetch ssh://git@github.com/Homebrew/brew";
        assert_eq!(r.redact(said), said);
    }

    #[test]
    fn test_a_short_password_is_masked_only_where_it_stands_as_a_login() {
        let r = redactor(&[("http_proxy", "http://u:abc@proxy.lan:8080")]);
        assert_eq!(
            r.redact("in 'u:abc@proxy.lan:8080' and abcdef"),
            "in 'u:****@proxy.lan:8080' and abcdef"
        );
    }

    #[test]
    fn test_a_proxy_given_a_user_name_alone_masks_nothing() {
        // NTLM or Kerberos style: the name is the account's, no secret.
        // Taken for a token, it masked the account name in every path.
        let r = redactor(&[("http_proxy", "http://brulek@proxy.lan:3128")]);
        assert_eq!(r, Redactor::default());
        let said = "==> Pouring /Users/brulek/Library/Caches/Homebrew/downloads/jq.tar.gz";
        assert_eq!(r.redact(said), said);
    }

    /// What sudo 1.9 prints when it cannot ask for a password, as Homebrew
    /// passes it on (`needsPassword` in src/lib/failureCause.ts).
    const SUDO_SAID: &str = "sudo: a terminal is required to read the password; \
        either use the -S option to read from standard input or configure an askpass helper\n\
        sudo: a password is required";

    #[test]
    fn test_a_password_that_is_a_plain_word_or_number_is_masked_only_as_a_login() {
        use crate::history::{failure_cause, FailureCause};
        for password in ["password", "required", "terminal", "2026", "12345678"] {
            let setting = format!("http://me:{password}@proxy.lan:3128");
            let r = redactor(&[("https_proxy", &setting)]);
            // sudo's words, a date: left as written, and still read as
            // sudo needing a password.
            assert_eq!(r.redact(SUDO_SAID), SUDO_SAID, "{password}");
            assert_eq!(
                failure_cause(&r.redact(SUDO_SAID)),
                Some(FailureCause::NeedsPassword),
                "{password}"
            );
            let dated = "==> jq 1.8.1 was released 2026-10-01 (12345678 bytes)";
            assert_eq!(r.redact(dated), dated, "{password}");
            // Where it stands as a login it is masked all the same.
            assert_eq!(
                r.redact(&curl_refuses(&setting)),
                curl_refuses("http://me:****@proxy.lan:3128"),
                "{password}"
            );
        }
        // One that mixes letters with digits or signs is no word of a
        // tool's, and is masked wherever it appears.
        let r = redactor(&[("https_proxy", "http://me:passw0rd@proxy.lan:3128")]);
        assert_eq!(r.redact("sent passw0rd"), "sent ****");
    }

    #[test]
    fn test_a_mirror_name_is_masked_wherever_it_appears_only_when_it_looks_like_a_token() {
        // A name that is a word or has a `.` may be the account's own.
        for name in ["brulek", "john.doe"] {
            let setting = format!("https://{name}@mirror.example/homebrew-bottles");
            let r = redactor(&[("HOMEBREW_BOTTLE_DOMAIN", &setting)]);
            let path = format!("==> Pouring /Users/{name}/Library/Caches/jq.tar.gz");
            assert_eq!(r.redact(&path), path, "{name}");
            assert_eq!(
                r.redact(&format!("fatal: unable to access '{setting}/x'")),
                "fatal: unable to access 'https://****@mirror.example/homebrew-bottles/x'",
                "{name}"
            );
        }
    }

    #[test]
    fn test_the_pattern_masks_any_url_login_and_nothing_else() {
        let r = Redactor::default();
        for (said, expected) in [
            (
                "Collecting x @ git+https://bot:hunter22@git.example/x.git",
                "Collecting x @ git+https://bot:****@git.example/x.git",
            ),
            (
                "proxy socks5h://a:b@c@127.0.0.1:7891 failed",
                "proxy socks5h://a:****@127.0.0.1:7891 failed",
            ),
            (
                "ProxyError('http://u:pw@host:1')",
                "ProxyError('http://u:****@host:1')",
            ),
            (
                "https://example.com:8080/a@b",
                "https://example.com:8080/a@b",
            ),
            ("ssh://git@github.com/x", "ssh://git@github.com/x"),
            ("mailto:someone@example.com", "mailto:someone@example.com"),
            ("http://u:****@host", "http://u:****@host"),
            ("no address here", "no address here"),
        ] {
            assert_eq!(r.redact(said), expected, "{said}");
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
                    "RUSTUP_DIST_SERVER",
                    "https://mirror.example/dist/foo@1.2.3.4/"
                ),
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
