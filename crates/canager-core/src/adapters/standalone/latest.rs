//! Versions: the installed one out of a `--version` line, the published
//! one out of an endpoint's body, and how the two compare.
//!
//! Dotted integers, compared component by component. The existing
//! adapters compare with `!=`, because a package registry never reports a
//! version older than the installed one; a standalone tool's channel
//! pointer can (Claude Code's `stable` pointer was 2.1.274 while the
//! installed `latest` was 2.1.282, recorded in
//! `adapters/fixtures/standalone-claude/2.1.282/`), so only `remote > local`
//! is an update (phase 4 spec §4.3). No `semver` crate: these tools' versions
//! can include suffixes; those remain intact and uncheckable. Calver (`2026.9.12`) is right under this
//! rule where a string comparison is wrong.

use super::recipe::VersionParse;
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
/// silently truncate or reinterpret a version it cannot compare.
pub fn parse_version(stdout: &str, parse: VersionParse) -> Option<String> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    let mut tokens = line.split_whitespace();
    let token = match parse {
        VersionParse::FirstToken => tokens.next()?,
        VersionParse::SecondToken => tokens.nth(1)?,
    };
    Some(token.to_string())
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

/// `claude_channel_from_json` over `<home>/.claude/settings.json`, the one
/// file Canager reads for Claude Code (`docs/what-we-run.md`, "Files
/// Canager reads"): read-only, and `latest` when it cannot be read.
pub fn claude_channel(home: &Path) -> &'static str {
    match std::fs::read_to_string(home.join(".claude").join("settings.json")) {
        Ok(json) => claude_channel_from_json(&json),
        Err(_) => CHANNEL_LATEST,
    }
}

/// The version a channel pointer answered with, trimmed; `Err` with a
/// short reason for a body that is not one version, which becomes an
/// uncheckable row's description -- so the reason quotes only the first
/// few characters, never a page of HTML.
pub fn parse_channel_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if !trimmed.is_empty() && !trimmed.chars().any(char::is_whitespace) {
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
/// `StandaloneAdapter::latest_version` for `Latest::HttpTomlVersion`.
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

#[cfg(test)]
mod tests {
    use super::*;
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
        // A component too large for an integer is not a version Canager
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
        let home = std::env::temp_dir().join(format!(
            "canager-standalone-channel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(home.join(".claude")).expect("create .claude");
        assert_eq!(
            claude_channel(&home),
            CHANNEL_LATEST,
            "no settings.json yet"
        );
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_channel(&home), CHANNEL_STABLE);
        let _ = std::fs::remove_dir_all(&home);
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
}
