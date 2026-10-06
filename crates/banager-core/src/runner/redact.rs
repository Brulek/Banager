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

/// A password (or token) this many characters long or longer is masked
/// wherever it appears; a shorter one only where it stands as a login,
/// before an `@` (`:abc@`, `//abc@`). Masking every `abc` in a build log
/// would make the log unreadable for a secret that short.
pub const SHORTEST_MASKED_ANYWHERE: usize = 4;

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
        for (_name, value) in settings {
            match Login::in_value(value) {
                Some(Login::Password { user, password }) => {
                    for form in forms_of(password) {
                        redactor.mask_login(":", form);
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
                        redactor.mask_login("//", form);
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

    /// Masks `form` where it stands as a login (`{lead}{form}@`), and
    /// anywhere at all when it is long enough to be worth it.
    fn mask_login(&mut self, lead: &str, form: String) {
        let login = (format!("{lead}{form}@"), format!("{lead}{MASK}@"));
        if !self.in_login.contains(&login) {
            self.in_login.push(login);
        }
        if form.chars().count() >= SHORTEST_MASKED_ANYWHERE {
            self.mask_anywhere(form);
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

/// A login as a setting's value holds it: what sits before the last `@`
/// of the address's authority (`scheme://` and what follows up to the
/// first `/`, `?` or `#`; the value as a whole when it has no scheme, as
/// curl accepts a proxy written that way).
#[derive(Debug, PartialEq, Eq)]
enum Login<'v> {
    /// `user:password@`, with a password: the password is the secret.
    Password { user: &'v str, password: &'v str },
    /// `token@` on an http(s) address: no password, so the name is the
    /// secret (a mirror's access token). Elsewhere a name alone is no
    /// secret: `git@github.com:…` names an ssh user.
    Token(&'v str),
}

impl<'v> Login<'v> {
    fn in_value(value: &'v str) -> Option<Login<'v>> {
        let (scheme, rest) = match value.find("://") {
            Some(at) => (&value[..at], &value[at + 3..]),
            None => ("", value),
        };
        let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
        let userinfo = &authority[..authority.rfind('@')?];
        let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
        if !password.is_empty() {
            return Some(Login::Password { user, password });
        }
        let scheme = scheme.to_ascii_lowercase();
        let http = ["http", "https"]
            .iter()
            .any(|name| scheme == *name || scheme.ends_with(&format!("+{name}")));
        (http && !user.is_empty()).then_some(Login::Token(user))
    }
}

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
