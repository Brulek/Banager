//! Versions: the installed one out of a `--version` line, the published
//! one out of an endpoint's body or a tool's own update check, and how the
//! two compare.
//!
//! Dotted integers, compared component by component. The existing
//! adapters compare with `!=`, because a package registry never reports a
//! version older than the installed one; a standalone tool's channel
//! pointer can (Claude Code's `stable` pointer was 2.1.274 while the
//! installed `latest` was 2.1.282, recorded in
//! `adapters/fixtures/standalone-claude/2.1.282/`), so only `remote > local`
//! is an update (phase 4 spec §4.3) -- unless the tool answers for itself
//! (`parse_update_check`), whose verdict is taken as answered and never
//! compared (the same section). No `semver` crate: these tools' versions
//! can include suffixes; those remain intact and uncheckable. Calver (`2026.9.12`) is right under this
//! rule where a string comparison is wrong.

use super::recipe::VersionParse;
use crate::runner::redact::Redactor;
use std::cmp::Ordering;
use std::path::Path;

/// `^\d+(\.\d+)*$`, by hand (the crate's `validate_package_name` sets the
/// precedent for not pulling in `regex` for one pattern).
pub fn is_dotted_version(s: &str) -> bool {
    !s.is_empty()
        && s.split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// The installed version out of a version command's stdout, per `parse`,
/// retaining the complete token, including prerelease/build suffixes.
/// Only an absent token is `None` (`NotResponding` in detect); only
/// `compare_dotted` decides comparability. Read the first non-empty line.
/// The command's output contract chooses the token; extraction does not
/// silently truncate or reinterpret a version it cannot compare. A token
/// with a control character in it is no version, as an absent one is
/// (`sanity::version_token`).
pub fn parse_version(stdout: &str, parse: VersionParse) -> Option<String> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    let mut tokens = line.split_whitespace();
    let token = match parse {
        VersionParse::FirstToken => tokens.next()?,
        VersionParse::SecondToken => tokens.nth(1)?,
    };
    crate::adapters::sanity::version_token(Some(token.to_string()))
}

/// `local` against `remote` as sequences of integers, shorter-is-less
/// (`1.2` < `1.2.0`); `None` when either is not a dotted version or has a
/// component too large to be an integer, which `check_updates` reports as
/// an uncheckable row naming both strings.
pub fn compare_dotted(local: &str, remote: &str) -> Option<Ordering> {
    fn components(s: &str) -> Option<Vec<u64>> {
        if !is_dotted_version(s) {
            return None;
        }
        s.split('.').map(|part| part.parse::<u64>().ok()).collect()
    }
    Some(components(local)?.cmp(&components(remote)?))
}

/// Claude Code's two release channels, as `autoUpdatesChannel` names them
/// and as the pointer URLs are spelled (claude.md §4, §5: VERIFIED for the
/// setting's values and for both URLs answering).
pub const CHANNEL_LATEST: &str = "latest";
pub const CHANNEL_STABLE: &str = "stable";

/// The channel out of `~/.claude/settings.json`'s text: `stable` when the
/// key `autoUpdatesChannel` is exactly the string `"stable"`, `latest` for
/// everything else -- a missing key, a value of another type, an unknown
/// name, or JSON that does not parse. Never an error: the default is what
/// a fresh install has, and a wrong channel costs a badge whose update
/// then reports `UnchangedAfterUpgrade` (spec §3.1).
pub fn claude_channel_from_json(json: &str) -> &'static str {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value)
            if value
                .get("autoUpdatesChannel")
                .and_then(serde_json::Value::as_str)
                == Some(CHANNEL_STABLE) =>
        {
            CHANNEL_STABLE
        }
        _ => CHANNEL_LATEST,
    }
}

/// The words Claude Code takes as "yes" for a switch in its environment
/// (`isEnvTruthy`: the value as text, trimmed and in lower case, is one of
/// these; Claude Code 2.1.292, read from its binary as bytes, never run).
const CLAUDE_YES: &[&str] = &["1", "true", "yes", "on"];

/// The `env` object of `~/.claude/settings.json`'s text, which Claude Code
/// copies into its environment when it starts, each value as text.
/// `None` when there is no `env` object or the JSON does not parse.
struct ClaudeEnv(serde_json::Map<String, serde_json::Value>);

impl ClaudeEnv {
    fn from_json(json: &str) -> Option<Self> {
        use serde_json::Value;
        let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(json) else {
            return None;
        };
        match settings.remove("env")? {
            Value::Object(env) => Some(Self(env)),
            _ => None,
        }
    }

    /// A value as Claude Code's environment holds it: JSON's `String()`,
    /// so `1` and `true` are `"1"` and `"true"`; an array or object is
    /// none.
    fn text(&self, key: &str) -> Option<String> {
        use serde_json::Value;
        match self.0.get(key)? {
            Value::String(text) => Some(text.clone()),
            Value::Bool(yes) => Some(yes.to_string()),
            Value::Number(n) if n.as_f64() == Some(1.0) => Some("1".to_string()),
            Value::Number(n) => Some(n.to_string()),
            Value::Null => Some("null".to_string()),
            Value::Array(_) | Value::Object(_) => None,
        }
    }

    /// Whether `key` is one of `CLAUDE_YES`.
    fn says_yes(&self, key: &str) -> bool {
        self.text(key)
            .is_some_and(|text| CLAUDE_YES.contains(&text.trim().to_lowercase().as_str()))
    }
}

/// Whether `~/.claude/settings.json`'s text turns Claude Code's own
/// background updater off, by the switches Claude Code itself checks
/// before it updates (2.1.292, read from its binary as bytes: r39 S2):
/// its `env` (`ClaudeEnv`) sets `DISABLE_UPDATES` or `DISABLE_AUTOUPDATER`
/// to one of `CLAUDE_YES`, or sets
/// `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` to anything but empty text,
/// which Claude Code also takes as off. `false` for anything else -- no
/// `env`, another value, an array or object as a value, JSON that does
/// not parse: the updater is on by default. Claude Code's one other such
/// check, the older `autoUpdates: false` in `~/.claude.json`, is in a
/// file Banager does not open; Claude Code moves it into this file's
/// `env` as `DISABLE_AUTOUPDATER: "1"`.
pub fn claude_updater_off_from_json(json: &str) -> bool {
    let Some(env) = ClaudeEnv::from_json(json) else {
        return false;
    };
    env.says_yes("DISABLE_UPDATES")
        || env.says_yes("DISABLE_AUTOUPDATER")
        || env
            .text("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .is_some_and(|text| !text.is_empty())
}

/// Whether `~/.claude/settings.json`'s text turns off every update of
/// Claude Code, not only the background one: its `env` sets
/// `DISABLE_UPDATES` to one of `CLAUDE_YES`. `claude update`, the command
/// Banager's Update runs, checks that before anything else, and then
/// prints that updates are disabled by an administrator and exits 0
/// having installed nothing (2.1.292, read from its binary as bytes: r39
/// S2, skeptic 2). The other two switches `claude_updater_off_from_json`
/// reads stop only the background updater, and `claude update` still
/// installs. `false` for anything else, as there.
pub fn claude_updates_refused_from_json(json: &str) -> bool {
    ClaudeEnv::from_json(json).is_some_and(|env| env.says_yes("DISABLE_UPDATES"))
}

/// `<home>/.claude/settings.json`'s text, the one file Banager reads for
/// Claude Code (`docs/what-we-run.md`, "Files Banager reads"), read-only
/// and bounded (`read_file`): a named pipe there is not waited on, and a
/// `~/.claude` kept in a protected place is not read. `Some(None)` when it
/// cannot be read -- not there, not a regular file, too large, not UTF-8;
/// `None` when it is in, or reached through, a protected place (a
/// `~/.claude` kept in iCloud Drive): Banager does not read it there,
/// Claude Code does, and what it says is not known.
fn claude_settings(home: &Path) -> Option<Option<String>> {
    match crate::adapters::read_file::read_text(
        &home.join(".claude").join("settings.json"),
        &crate::protected::Protected::new(home),
    ) {
        Ok(json) => Some(Some(json)),
        Err(error) if crate::protected::look::is_protected(&error) => None,
        Err(_) => Some(None),
    }
}

/// `claude_channel_from_json` over `claude_settings`: `latest` when the
/// file cannot be read -- Claude Code's own default, which it follows too
/// when it cannot read the file; `None` when it is in a protected place,
/// where which channel it names is not known. Read by the update check
/// (`StandaloneAdapter::published`).
pub fn claude_channel(home: &Path) -> Option<&'static str> {
    claude_settings(home)
        .map(|json| json.map_or(CHANNEL_LATEST, |json| claude_channel_from_json(&json)))
}

/// `claude_updater_off_from_json` over `claude_settings`: `false` when the
/// file cannot be read -- Claude Code's own default, the updater on;
/// `None` when it is in a protected place, where whether it turns the
/// updater off is not known. Read by the inventory
/// (`StandaloneAdapter::rows`, `SelfUpdates::UnlessOffInClaudeSettings`).
pub fn claude_updater_off(home: &Path) -> Option<bool> {
    claude_settings(home).map(|json| json.is_some_and(|json| claude_updater_off_from_json(&json)))
}

/// `claude_updates_refused_from_json` over `claude_settings`: `false` when
/// the file cannot be read -- Claude Code's own default, updates on;
/// `None` when it is in a protected place, where whether it turns them off
/// is not known. Read by the update check (`StandaloneAdapter::check_updates`,
/// `UpdateBlocked::UpdatesTurnedOff`).
pub fn claude_updates_refused(home: &Path) -> Option<bool> {
    claude_settings(home)
        .map(|json| json.is_some_and(|json| claude_updates_refused_from_json(&json)))
}

/// The version a channel pointer answered with, trimmed; `Err` with a
/// short reason for a body that is not one version, which becomes an
/// uncheckable row's description -- so the reason quotes only the first
/// few characters, never a page of HTML. A control character -- a NUL,
/// an escape -- makes it no version either.
pub fn parse_channel_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if crate::adapters::sanity::is_name(trimmed) && !trimmed.chars().any(char::is_whitespace) {
        return Ok(trimmed.to_string());
    }
    let shown: String = trimmed.chars().take(40).collect();
    Err(format!(
        "the channel endpoint did not answer with a version (got {shown:?})"
    ))
}

/// The `version` of a release file such as rustup's `release-stable.toml`
/// (`schema-version = '1'` / `version = '1.29.1'`), trimmed; `Err` with a
/// short reason for a body that is not TOML, has no top-level `version`
/// string, or whose version is not a dotted version. The reason becomes
/// an uncheckable row's description, so it quotes at most a few
/// characters of the body, never a page of HTML. Read by
/// `StandaloneAdapter::published` for `Latest::HttpTomlVersion`.
pub fn parse_release_stable_toml(body: &str) -> Result<String, String> {
    let shown = || -> String { body.trim().chars().take(40).collect() };
    let table: toml::Value = toml::from_str(body)
        .map_err(|_| format!("the release file is not TOML (got {:?})", shown()))?;
    let version = table
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            format!(
                "the release file has no `version` string (got {:?})",
                shown()
            )
        })?;
    let version = version.trim();
    if is_dotted_version(version) {
        Ok(version.to_string())
    } else {
        Err(format!(
            "the release file's version is not a version (got {version:?})"
        ))
    }
}

/// The version a JSON manifest names in its top-level `field` -- agy's
/// `manifests/darwin_arm64.json` answers `{"version":"1.2.9","url":…,
/// "sha512":…}` (agy.md §4, VERIFIED live) -- trimmed; `Err` with a short
/// reason for a body that is not JSON, has no such string, or names
/// something that is not a dotted version. The reason becomes an
/// uncheckable row's description, so it quotes at most a few characters
/// of the body, never a page of HTML. Read by
/// `StandaloneAdapter::published` for `Latest::HttpJsonField`.
pub fn parse_json_field(body: &str, field: &str) -> Result<String, String> {
    let shown = || -> String { body.trim().chars().take(40).collect() };
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| format!("the manifest is not JSON (got {:?})", shown()))?;
    let version = value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the manifest has no `{field}` string (got {:?})", shown()))?
        .trim();
    if is_dotted_version(version) {
        Ok(version.to_string())
    } else {
        Err(format!(
            "the manifest's `{field}` is not a version (got {version:?})"
        ))
    }
}

/// What a tool's own read-only update check answered (grok's `update
/// --check --json`, grok.md §3, VERIFIED on this Mac): the newest version
/// it knows of, and whether it calls that an update. Read by
/// `StandaloneAdapter::check_updates`, which trusts `available` and never
/// compares (spec §4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCheck {
    pub latest: String,
    pub available: bool,
}

/// `UpdateCheck` out of the check command's stdout: a JSON object whose
/// `latest_field` is a non-empty string, whose `available_field` is a
/// boolean, and whose `error_field` (when the recipe names one) is absent
/// or `null` -- `{"currentVersion":"1.0.41","latestVersion":"1.0.41",
/// "updateAvailable":false,…,"error":null}`. `Err` with a short reason
/// otherwise; a non-null error is quoted, since `updateAvailable: false`
/// beside it means the tool could not find out, not that nothing is newer
/// (ruling 10 of the phase 4 step D plan). The tool's `latest` is taken as
/// it is, suffix and all: it is shown, not compared. Read by
/// `StandaloneAdapter::published` for `Latest::Command`.
///
/// A reason quotes what the tool printed, and is shown on the source's
/// row and in the diagnostics; the tool ran with the proxy and mirror
/// settings, which can hold a login. So what it quotes goes through `mask`
/// first (`Redactor::for_commands`, the runner's), and is cut to length
/// after (F2 of the decisions-round review).
pub fn parse_update_check(
    stdout: &str,
    latest_field: &str,
    available_field: &str,
    error_field: Option<&str>,
    mask: &Redactor,
) -> Result<UpdateCheck, String> {
    let shown = || -> String { mask.redact(stdout.trim()).chars().take(40).collect() };
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|_| format!("the update check did not print JSON (got {:?})", shown()))?;
    if let Some(error) = error_field.and_then(|field| value.get(field)) {
        if !error.is_null() {
            let text = match error.as_str() {
                Some(text) => text.trim().to_string(),
                None => error.to_string(),
            };
            let text: String = mask.redact(&text).chars().take(80).collect();
            return Err(format!("the update check reported: {text}"));
        }
    }
    let latest = value
        .get(latest_field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            format!(
                "the update check has no `{latest_field}` string (got {:?})",
                shown()
            )
        })?
        .trim()
        .to_string();
    let available = value
        .get(available_field)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            format!(
                "the update check has no `{available_field}` boolean (got {:?})",
                shown()
            )
        })?;
    if latest.is_empty() {
        return Err(format!("the update check's `{latest_field}` is empty"));
    }
    if !crate::adapters::sanity::is_name(&latest) {
        let shown: String = mask.redact(&latest).chars().take(40).collect();
        return Err(format!(
            "the update check's `{latest_field}` is not a version (got {shown:?})"
        ));
    }
    Ok(UpdateCheck { latest, available })
}

/// The CPU architectures a `Latest::HttpJsonField` manifest URL has been
/// verified for: agy's `darwin_arm64.json` was fetched live (agy.md §4);
/// the `darwin_amd64.json` the install script's `${os}_${arch}` rule
/// implies was not (spec §3.5, §十一). Spelled as `std::env::consts::ARCH`
/// spells it -- `aarch64`, never `arm64`. Read by `manifest_arch_allowed`.
pub const MANIFEST_VERIFIED_ARCHES: [&str; 1] = ["aarch64"];

/// `Ok` when a manifest lookup may be made on `arch`; otherwise the reason
/// the row says "could not check" -- an Intel Mac, or a universal build
/// under Rosetta, which reports `x86_64`: the safe direction, no request
/// to an unverified URL. Read by `StandaloneAdapter::published`.
pub fn manifest_arch_allowed(arch: &str) -> Result<(), String> {
    if MANIFEST_VERIFIED_ARCHES.contains(&arch) {
        Ok(())
    } else {
        Err(format!(
            "not yet verified on Intel Macs: this Banager runs as {arch:?}, and the manifest URL is verified for Apple silicon only"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_claude_channel_does_not_wait_on_a_named_pipe() {
        use crate::adapters::read_file::tests::{finishes, make_fifo, temp_dir};
        let home = temp_dir("claude-channel-fifo");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        make_fifo(&home.join(".claude").join("settings.json"));
        let read = home.clone();
        assert_eq!(
            finishes(move || claude_channel(&read)),
            Some(CHANNEL_LATEST)
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_version_readers_refuse_control_characters() {
        assert_eq!(parse_version("1.2.11\0\n", VersionParse::FirstToken), None);
        assert_eq!(
            parse_version(
                "grok 1.0.4\u{11} (4220f3b224a6)\n",
                VersionParse::SecondToken
            ),
            None
        );
        assert_eq!(
            parse_version("grok 1.0.41 (4220f3b224a6)\n", VersionParse::SecondToken),
            Some("1.0.41".to_string())
        );
        assert!(parse_channel_body("2.1.\u{0}274\n").is_err());
        assert_eq!(parse_channel_body("2.1.274\n"), Ok("2.1.274".to_string()));
        let check = r#"{"latestVersion":"1.0.\u001b42","updateAvailable":true,"error":null}"#;
        assert!(parse_update_check(
            check,
            "latestVersion",
            "updateAvailable",
            Some("error"),
            &Redactor::default()
        )
        .is_err());
    }
    use std::cmp::Ordering;

    #[test]
    fn test_is_dotted_version_accepts_integers_joined_by_dots_and_nothing_else() {
        for ok in ["2.1.281", "1.0.41", "2026.9.12", "7", "0.0.0"] {
            assert!(is_dotted_version(ok), "{ok:?} is a dotted version");
        }
        for bad in [
            "",
            "v2.1.281",
            "2.1.281-beta",
            "2..1",
            ".1",
            "1.",
            "abc",
            "2.1.281 (Claude Code)",
            "１.2",
        ] {
            assert!(!is_dotted_version(bad), "{bad:?} is not a dotted version");
        }
    }

    #[test]
    fn test_parse_version_reads_the_first_token_of_claudes_recorded_line() {
        // `claude --version` on this Mac, 2026-09-24 (claude.md §1,
        // VERIFIED): `2.1.281 (Claude Code)`. The recorded line is
        // `adapters/fixtures/standalone-claude/<version>/version.txt`, which
        // `mod.rs`'s fixture tests read; the shape is pinned here from the
        // research record.
        assert_eq!(
            parse_version("2.1.281 (Claude Code)\n", VersionParse::FirstToken),
            Some("2.1.281".to_string())
        );
    }

    #[test]
    fn test_parse_version_skips_leading_blank_lines_and_trailing_space() {
        // A tool that prints a blank line first, or a version with a
        // trailing tab, still has a version: the alternative is a working
        // install shown as "not responding".
        assert_eq!(
            parse_version("\n\n2.1.281 (Claude Code)  \n", VersionParse::FirstToken),
            Some("2.1.281".to_string())
        );
        assert_eq!(
            parse_version("2.1.281\t\n", VersionParse::FirstToken),
            Some("2.1.281".to_string())
        );
    }

    #[test]
    fn test_parse_version_is_none_only_when_no_token_is_available() {
        for stdout in ["", "\n", " \t\n"] {
            assert_eq!(
                parse_version(stdout, VersionParse::FirstToken),
                None,
                "{stdout:?}"
            );
        }
    }

    #[test]
    fn test_version_extraction_preserves_the_complete_token() {
        for token in ["2.1.281-beta", "2.1.281+build.7", "v2.1.281", "abc"] {
            assert_eq!(
                parse_version(
                    &format!("{token} (Claude Code)\n"),
                    VersionParse::FirstToken
                ),
                Some(token.to_string())
            );
            assert_eq!(
                parse_channel_body(&format!(" {token}\n")),
                Ok(token.to_string())
            );
            assert_eq!(compare_dotted(token, "2.1.290"), None);
            assert_eq!(compare_dotted("2.1.281", token), None);
        }
    }

    #[test]
    fn test_compare_dotted_compares_integers_component_by_component() {
        // Spec §4.3's table, plus the two shapes a string comparison gets
        // wrong: `9` vs `12`, and a shorter version against a longer one.
        for (local, remote, expected) in [
            ("2.1.273", "2.1.281", Ordering::Less),
            ("2.1.281", "2.1.273", Ordering::Greater),
            ("1.0.41", "1.0.41", Ordering::Equal),
            ("2026.9.9", "2026.9.12", Ordering::Less),
            ("1.2.9", "1.2.10", Ordering::Less),
            ("1.2.10", "1.2.9", Ordering::Greater),
            ("1.2", "1.2.0", Ordering::Less),
            ("2.01", "2.1", Ordering::Equal),
        ] {
            assert_eq!(
                compare_dotted(local, remote),
                Some(expected),
                "{local} vs {remote}"
            );
        }
    }

    #[test]
    fn test_compare_dotted_is_none_when_either_side_is_not_a_version() {
        assert_eq!(compare_dotted("abc", "2.1.281"), None);
        assert_eq!(compare_dotted("2.1.281", "latest"), None);
        assert_eq!(compare_dotted("", ""), None);
        // A component too large for an integer is not a version Banager
        // will reason about either.
        assert_eq!(compare_dotted("1.99999999999999999999999", "2"), None);
    }

    #[test]
    fn test_claude_channel_from_json_reads_stable_and_defaults_to_latest() {
        // The two documented values (claude.md §5, VERIFIED): "latest"
        // (the default) and "stable".
        assert_eq!(
            claude_channel_from_json(r#"{"autoUpdatesChannel":"stable"}"#),
            CHANNEL_STABLE
        );
        assert_eq!(
            claude_channel_from_json(r#"{"autoUpdatesChannel":"latest"}"#),
            CHANNEL_LATEST
        );
        assert_eq!(
            claude_channel_from_json(r#"{"model":"opus","permissions":{}}"#),
            CHANNEL_LATEST
        );
    }

    #[test]
    fn test_claude_channel_from_json_defaults_to_latest_for_anything_but_stable() {
        // Malformed JSON, a value that is not a string, an unknown channel
        // name, a different case: never a guess, never an error -- the
        // failure mode of a wrong guess is a harmless `UnchangedAfterUpgrade`
        // (spec §3.1), and the default is what a fresh install has.
        for json in [
            "",
            "{",
            "[]",
            r#"{"autoUpdatesChannel":1}"#,
            r#"{"autoUpdatesChannel":null}"#,
            r#"{"autoUpdatesChannel":"Stable"}"#,
            r#"{"autoUpdatesChannel":"nightly"}"#,
        ] {
            assert_eq!(claude_channel_from_json(json), CHANNEL_LATEST, "{json:?}");
        }
    }

    #[test]
    fn test_claude_channel_reads_the_settings_file_under_home_and_defaults_when_absent() {
        let home = crate::testing::unique_temp_path("standalone-channel");
        std::fs::create_dir_all(home.join(".claude")).expect("create .claude");
        assert_eq!(
            claude_channel(&home),
            Some(CHANNEL_LATEST),
            "no settings.json yet"
        );
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_channel(&home), Some(CHANNEL_STABLE));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_claude_channel_never_reads_a_settings_file_kept_in_a_protected_place() {
        // `~/.claude` a link into `~/Documents` or iCloud Drive: its
        // settings are not read there, and the channel is not known --
        // not taken as the default, since Claude Code reads the file and
        // may follow `stable`. Kept anywhere else, it is read.
        for (keep, expected) in [
            ("claude-settings", Some(CHANNEL_STABLE)),
            ("Documents/claude-settings", None),
            (
                "Library/Mobile Documents/com~apple~CloudDocs/claude-settings",
                None,
            ),
        ] {
            let raw = crate::testing::unique_temp_path("standalone-channel-kept");
            std::fs::create_dir_all(raw.join(keep)).expect("create the kept folder");
            let home = std::fs::canonicalize(&raw).unwrap();
            std::fs::write(
                home.join(keep).join("settings.json"),
                r#"{"autoUpdatesChannel":"stable"}"#,
            )
            .expect("write settings.json");
            std::os::unix::fs::symlink(home.join(keep), home.join(".claude")).unwrap();
            assert_eq!(claude_channel(&home), expected, "{keep}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    #[test]
    fn test_claude_updater_off_from_json_reads_the_switches_claude_code_checks() {
        // r39 S2: Claude Code's documented switch (`DISABLE_AUTOUPDATER`,
        // which it also migrates the older `autoUpdates: false` into),
        // `DISABLE_UPDATES`, and the "no nonessential traffic" switch, in
        // the forms Claude Code takes as yes once it has copied `env` into
        // its environment as text.
        for json in [
            r#"{"env":{"DISABLE_AUTOUPDATER":"1"}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":"true"}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":" Yes "}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":"ON"}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":1}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":1.0}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":true}}"#,
            r#"{"env":{"DISABLE_UPDATES":"1"}}"#,
            r#"{"env":{"DISABLE_UPDATES":"true"}}"#,
            r#"{"env":{"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC":"1"}}"#,
            // Taken as set whatever it says, as Claude Code takes it.
            r#"{"env":{"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC":"0"}}"#,
            r#"{"model":"opus","env":{"FOO":"bar","DISABLE_AUTOUPDATER":"1"},"autoUpdatesChannel":"stable"}"#,
        ] {
            assert!(claude_updater_off_from_json(json), "{json}");
        }
    }

    #[test]
    fn test_claude_updater_off_from_json_is_false_for_anything_else() {
        // The updater is on by default: no `env`, a switch Claude Code
        // does not take as yes, a value no one writes there, keys it does
        // not read here (the older `autoUpdates` belongs in
        // `~/.claude.json`), JSON that does not parse.
        for json in [
            "",
            "{",
            "[]",
            "null",
            r#"{}"#,
            r#"{"env":{}}"#,
            r#"{"env":"DISABLE_AUTOUPDATER=1"}"#,
            r#"{"env":["DISABLE_AUTOUPDATER"]}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":"0"}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":"false"}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":""}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":"no"}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":false}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":0}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":2}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":null}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":["1"]}}"#,
            r#"{"env":{"DISABLE_AUTOUPDATER":{"value":"1"}}}"#,
            r#"{"env":{"DISABLE_UPDATES":"off"}}"#,
            r#"{"env":{"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC":""}}"#,
            r#"{"env":{"disable_autoupdater":"1"}}"#,
            r#"{"DISABLE_AUTOUPDATER":"1"}"#,
            r#"{"autoUpdates":false}"#,
            r#"{"env":{"FORCE_AUTOUPDATE_PLUGINS":"1"}}"#,
        ] {
            assert!(!claude_updater_off_from_json(json), "{json}");
        }
    }

    #[test]
    fn test_claude_updates_refused_from_json_reads_disable_updates_alone() {
        // r39 S2, skeptic 2: `DISABLE_UPDATES` is the one switch that
        // `claude update` obeys too (it prints that updates are disabled
        // and exits 0 having installed nothing; Claude Code 2.1.292, read
        // from its binary as bytes). The other two stop only the
        // background updater, and `claude update` still installs.
        for (json, refused) in [
            (r#"{"env":{"DISABLE_UPDATES":"1"}}"#, true),
            (r#"{"env":{"DISABLE_UPDATES":"true"}}"#, true),
            (r#"{"env":{"DISABLE_UPDATES":" Yes "}}"#, true),
            (r#"{"env":{"DISABLE_UPDATES":"ON"}}"#, true),
            (r#"{"env":{"DISABLE_UPDATES":1}}"#, true),
            (r#"{"env":{"DISABLE_UPDATES":true}}"#, true),
            (
                r#"{"autoUpdatesChannel":"stable","env":{"FOO":"bar","DISABLE_UPDATES":"1"}}"#,
                true,
            ),
            (r#"{"env":{"DISABLE_UPDATES":"0"}}"#, false),
            (r#"{"env":{"DISABLE_UPDATES":"off"}}"#, false),
            (r#"{"env":{"DISABLE_UPDATES":""}}"#, false),
            (r#"{"env":{"DISABLE_UPDATES":false}}"#, false),
            (r#"{"env":{"DISABLE_UPDATES":null}}"#, false),
            (r#"{"env":{"DISABLE_UPDATES":["1"]}}"#, false),
            (r#"{"env":{"disable_updates":"1"}}"#, false),
            (r#"{"DISABLE_UPDATES":"1"}"#, false),
            (r#"{"env":{"DISABLE_AUTOUPDATER":"1"}}"#, false),
            (
                r#"{"env":{"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC":"1"}}"#,
                false,
            ),
            (r#"{}"#, false),
            ("{ not json", false),
            ("", false),
        ] {
            assert_eq!(claude_updates_refused_from_json(json), refused, "{json}");
            if refused {
                // Every update off is the background updater off too.
                assert!(claude_updater_off_from_json(json), "{json}");
            }
        }
    }

    #[test]
    fn test_claude_updates_refused_reads_the_same_file_and_never_a_protected_one() {
        let home = crate::testing::unique_temp_path("standalone-updates-refused");
        std::fs::create_dir_all(home.join(".claude")).expect("create .claude");
        assert_eq!(
            claude_updates_refused(&home),
            Some(false),
            "no settings.json yet: Claude Code's default, updates on"
        );
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"env":{"DISABLE_UPDATES":"1"}}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_updates_refused(&home), Some(true));
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"env":{"DISABLE_AUTOUPDATER":"1"}}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_updates_refused(&home), Some(false));
        let _ = std::fs::remove_dir_all(&home);

        for (keep, expected) in [
            ("claude-settings", Some(true)),
            ("Documents/claude-settings", None),
            (
                "Library/Mobile Documents/com~apple~CloudDocs/claude-settings",
                None,
            ),
        ] {
            let raw = crate::testing::unique_temp_path("standalone-updates-refused-kept");
            std::fs::create_dir_all(raw.join(keep)).expect("create the kept folder");
            let home = std::fs::canonicalize(&raw).unwrap();
            std::fs::write(
                home.join(keep).join("settings.json"),
                r#"{"env":{"DISABLE_UPDATES":"1"}}"#,
            )
            .expect("write settings.json");
            std::os::unix::fs::symlink(home.join(keep), home.join(".claude")).unwrap();
            assert_eq!(claude_updates_refused(&home), expected, "{keep}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    #[test]
    fn test_claude_updater_off_reads_the_settings_file_under_home_and_defaults_when_absent() {
        let home = crate::testing::unique_temp_path("standalone-updater-off");
        std::fs::create_dir_all(home.join(".claude")).expect("create .claude");
        assert_eq!(
            claude_updater_off(&home),
            Some(false),
            "no settings.json yet: Claude Code's default, the updater on"
        );
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"env":{"DISABLE_AUTOUPDATER":"1"}}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_updater_off(&home), Some(true));
        // The same read the channel makes: one file, both keys.
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable","env":{"DISABLE_UPDATES":"1"}}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_updater_off(&home), Some(true));
        assert_eq!(claude_channel(&home), Some(CHANNEL_STABLE));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_claude_updater_off_never_reads_a_settings_file_kept_in_a_protected_place() {
        // `~/.claude` a link into `~/Documents` or iCloud Drive: not read
        // there, so whether it turns the updater off is not known. Kept
        // anywhere else, it is read.
        for (keep, expected) in [
            ("claude-settings", Some(true)),
            ("Documents/claude-settings", None),
            (
                "Library/Mobile Documents/com~apple~CloudDocs/claude-settings",
                None,
            ),
        ] {
            let raw = crate::testing::unique_temp_path("standalone-updater-off-kept");
            std::fs::create_dir_all(raw.join(keep)).expect("create the kept folder");
            let home = std::fs::canonicalize(&raw).unwrap();
            std::fs::write(
                home.join(keep).join("settings.json"),
                r#"{"env":{"DISABLE_AUTOUPDATER":"1"}}"#,
            )
            .expect("write settings.json");
            std::os::unix::fs::symlink(home.join(keep), home.join(".claude")).unwrap();
            assert_eq!(claude_updater_off(&home), expected, "{keep}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    #[test]
    fn test_parse_channel_body_trims_and_accepts_a_bare_version() {
        // `curl -sS https://downloads.claude.ai/claude-code-releases/latest`
        // answers a bare version (claude.md §4, VERIFIED); whether or not it
        // ends in a newline must not matter.
        assert_eq!(parse_channel_body("2.1.281"), Ok("2.1.281".to_string()));
        assert_eq!(parse_channel_body("2.1.273\n"), Ok("2.1.273".to_string()));
        assert_eq!(
            parse_channel_body("  2.1.281 \r\n"),
            Ok("2.1.281".to_string())
        );
    }

    #[test]
    fn test_parse_channel_body_refuses_anything_that_is_not_a_version() {
        // A multi-token HTML error page or an empty body: an uncheckable
        // row with the reason, never a candidate built from it. The reason
        // quotes at most a few characters of the body, so a page of HTML
        // does not become the row's description.
        for body in [
            "",
            " \n",
            "<html><body>503 Service Unavailable</body></html>",
            "2.1.281 2.1.290",
        ] {
            let err = parse_channel_body(body).expect_err(body);
            assert!(err.contains("did not answer with a version"), "{err}");
            assert!(err.len() < 120, "the reason stays short: {err}");
        }
    }

    #[test]
    fn test_parse_version_reads_the_second_token_of_rustups_recorded_line() {
        // `rustup --version` stdout on this Mac, 2026-09-25 (rustup.md §3;
        // recorded as `adapters/fixtures/standalone-rustup/<v>/version.txt`):
        // `rustup 1.29.1 (d95a37b6a 2026-08-13)`.
        assert_eq!(
            parse_version(
                "rustup 1.29.1 (d95a37b6a 2026-08-13)\n",
                VersionParse::SecondToken
            ),
            Some("1.29.1".to_string())
        );
        // The two `info:` lines rustup writes go to stderr, which
        // `read_version` never hands to this function. Had it been handed
        // one, the second token "This" would come back as the version --
        // this function keeps whatever token is there and leaves
        // comparability to `compare_dotted`
        // (`test_version_extraction_preserves_the_complete_token`) -- which
        // is why the read passes stdout alone.
        assert_eq!(
            parse_version(
                "info: This is the version for the rustup toolchain manager, not the rustc compiler.\n",
                VersionParse::SecondToken
            ),
            Some("This".to_string())
        );
        // A line with one token has no second.
        assert_eq!(parse_version("1.29.1\n", VersionParse::SecondToken), None);
    }

    #[test]
    fn test_parse_release_stable_toml_reads_the_version_string() {
        // The release file byte for byte (rustup.md §6, VERIFIED by curl;
        // recorded as `adapters/fixtures/standalone-rustup/<v>/
        // release-stable.toml`), and TOML's other string quote.
        assert_eq!(
            parse_release_stable_toml("schema-version = '1'\nversion = '1.29.1'\n"),
            Ok("1.29.1".to_string())
        );
        assert_eq!(
            parse_release_stable_toml("version = \"1.30.0\"\nschema-version = \"1\"\n"),
            Ok("1.30.0".to_string())
        );
    }

    #[test]
    fn test_parse_release_stable_toml_refuses_anything_that_is_not_a_versioned_release_file() {
        // HTML answered with status 200 (a captive portal), a file with
        // no version, a version that is not one, a version of the wrong
        // type: an uncheckable row with a short reason, never a candidate.
        for body in [
            "<html><body>Sign in to the network</body></html>",
            "",
            "schema-version = '1'\n",
            "version = 'latest'\n",
            "version = 1\n",
            "version = ['1.29.1']\n",
        ] {
            let err = parse_release_stable_toml(body).expect_err(body);
            assert!(err.contains("release file"), "{body:?}: {err}");
            assert!(err.len() < 140, "the reason stays short: {err}");
        }
    }

    #[test]
    fn test_parse_json_field_reads_agys_manifest_version_and_nothing_else() {
        // agy.md §4, VERIFIED live 2026-09-24: the manifest is one JSON
        // object with `version`, `url`, `sha512`. Only `version` is read;
        // the other two are the installer's business.
        let manifest = r#"{"version":"1.2.9","url":"https://storage.googleapis.com/antigravity-public/antigravity-cli/1.2.9-5905287731871744/darwin-arm/cli_mac_arm64.tar.gz","sha512":"8a96"}"#;
        assert_eq!(
            parse_json_field(manifest, "version"),
            Ok("1.2.9".to_string())
        );
        assert_eq!(
            parse_json_field(r#"{ "version" : " 1.2.10 " }"#, "version"),
            Ok("1.2.10".to_string())
        );
        for (body, needle) in [
            ("", "not JSON"),
            ("<html>Sign in</html>", "not JSON"),
            (r#"{"url":"x"}"#, "no `version` string"),
            (r#"{"version":12}"#, "no `version` string"),
            (r#"{"version":"latest"}"#, "not a version"),
            (r#"{"version":"1.2.9-beta"}"#, "not a version"),
        ] {
            let err = parse_json_field(body, "version").expect_err(body);
            assert!(err.contains(needle), "{body:?}: {err}");
            assert!(err.len() < 120, "the reason stays short: {err}");
        }
    }

    #[test]
    fn test_parse_update_check_reads_groks_answer_and_nothing_else() {
        // grok.md §3, VERIFIED on this Mac: `grok update --check --json`
        // prints one JSON object. Only the three fields the recipe names are
        // read; `latest` is taken as printed, since it is never compared;
        // a non-null `error` makes the whole answer a failure, since
        // `updateAvailable: false` beside an error is "could not check",
        // never "up to date" (ruling 10 of the phase 4 step D plan).
        let answer = r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false,"installer":"internal","channel":"stable","autoUpdate":true,"error":null}"#;
        assert_eq!(
            parse_update_check(
                answer,
                "latestVersion",
                "updateAvailable",
                Some("error"),
                &Redactor::default()
            ),
            Ok(UpdateCheck {
                latest: "1.0.41".to_string(),
                available: false,
            })
        );
        assert_eq!(
            parse_update_check(
                r#"{"latestVersion":"1.0.42-alpha.1","updateAvailable":true}"#,
                "latestVersion",
                "updateAvailable",
                Some("error"),
                &Redactor::default()
            ),
            Ok(UpdateCheck {
                latest: "1.0.42-alpha.1".to_string(),
                available: true,
            })
        );
        // No error field named: the key is not looked at.
        assert!(parse_update_check(
            r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":"ignored"}"#,
            "latestVersion",
            "updateAvailable",
            None,
            &Redactor::default()
        )
        .is_ok());
        for (body, needle) in [
            ("", "did not print JSON"),
            ("Checking for updates...\n", "did not print JSON"),
            (r#"{"updateAvailable":true}"#, "no `latestVersion` string"),
            (
                r#"{"latestVersion":"1.0.42"}"#,
                "no `updateAvailable` boolean",
            ),
            (
                r#"{"latestVersion":"1.0.42","updateAvailable":"yes"}"#,
                "no `updateAvailable` boolean",
            ),
            (r#"{"latestVersion":"","updateAvailable":true}"#, "is empty"),
            (
                r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":"network unreachable"}"#,
                "reported: network unreachable",
            ),
            (
                r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":{"code":7}}"#,
                r#"reported: {"code":7}"#,
            ),
        ] {
            let err = parse_update_check(
                body,
                "latestVersion",
                "updateAvailable",
                Some("error"),
                &Redactor::default(),
            )
            .expect_err(body);
            assert!(err.contains(needle), "{body:?}: {err}");
            assert!(err.len() < 140, "the reason stays short: {err}");
        }
    }

    #[test]
    fn test_parse_update_check_quotes_what_it_was_given_with_logins_masked() {
        // F2 of the decisions-round review: the reason is shown on the
        // source's row and in diagnostics, and the tool ran with the proxy
        // settings read from the login shell. Masked before it is cut to
        // length, so no part of a secret is left at the cut.
        let mask =
            Redactor::for_settings([("http_proxy", "review-user:rev/secret@127.0.0.1:8080")]);
        for body in [
            "review-user:rev/secret@127.0.0.1:8080 is not a proxy",
            r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":"proxy review-user:rev/secret@127.0.0.1:8080 refused"}"#,
            // Not a version: a control character in it.
            r#"{"latestVersion":"review-user:rev/secret@127.0.0.1:8080\u0007","updateAvailable":true}"#,
        ] {
            let err = parse_update_check(
                body,
                "latestVersion",
                "updateAvailable",
                Some("error"),
                &mask,
            )
            .expect_err(body);
            assert!(!err.contains("rev/s"), "{body:?}: {err}");
            // The user name too: a name can be a token (R1 of the
            // decisions-round re-check).
            assert!(!err.contains("review-user"), "{body:?}: {err}");
            assert!(err.contains("****:****@"), "{body:?}: {err}");
        }
    }

    #[test]
    fn test_manifest_arch_allowed_only_on_apple_silicon() {
        // Spec §3.1/§3.5: only the darwin_arm64 manifest was fetched; an
        // Intel Mac, or a universal build under Rosetta (which reports
        // x86_64), gets "could not check" with the reason, not a request
        // to an unverified URL. `std::env::consts::ARCH` spells it
        // `aarch64`, never `arm64`.
        assert_eq!(MANIFEST_VERIFIED_ARCHES, ["aarch64"]);
        assert_eq!(manifest_arch_allowed("aarch64"), Ok(()));
        for arch in ["x86_64", "arm64", ""] {
            let err = manifest_arch_allowed(arch).expect_err(arch);
            assert!(err.contains("Intel"), "{arch:?}: {err}");
            assert!(err.contains(arch), "{arch:?}: {err}");
        }
    }
}
