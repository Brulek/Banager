//! Homebrew's trust list, read only, for two things an uninstall preview
//! says: that `brew uninstall` deletes the entry the list holds for what it
//! uninstalls (`Warning::HomebrewForgetsTrust`), and, for a cask from a tap
//! Homebrew does not trust, that it then runs none of the cask's recorded
//! uninstall steps (`UninstallScope::HomebrewCaskStepsIfTrusted`).
//!
//! Homebrew 7.0.7-9 keeps the list in `$HOMEBREW_USER_CONFIG_HOME/trust.json`
//! (`Homebrew::Trust.trust_file`, `trust.rb:27-43`; the folder `bin/brew`
//! chooses, `brew_env::HomebrewSwitches::user_config_home`): one JSON
//! object whose `trustedtaps`, `trustedcasks` and `trustedformulae` hold
//! names, lowercased as Homebrew reads them (`trust_store`, `trust.rb:426-453`;
//! `SETTING_KEYS`, `:18-23`). No file, or one that is not a JSON object, is
//! an empty list to Homebrew. A tap's entry is its `user/repo` name or,
//! for a tap with a remote of its own, the remote's URL or path, which
//! Homebrew matches against the tap's git remote (`Tap#matches_reference?`,
//! `tap.rb:952-959`) -- something Banager does not read, so where such an
//! entry is in the list, this module says it cannot tell.

use serde_json::Value;
use std::io::ErrorKind;
use std::path::Path;

/// The file's name in `HOMEBREW_USER_CONFIG_HOME` (`trust.rb:39`, `:41`).
pub(crate) const TRUST_FILE: &str = "trust.json";

/// The names Homebrew's trust list holds, lowercased as Homebrew reads
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TrustList {
    pub(crate) taps: Vec<String>,
    pub(crate) casks: Vec<String>,
    pub(crate) formulae: Vec<String>,
}

/// What kind of package a trust entry is for (`trustedcasks`,
/// `trustedformulae`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Cask,
    Formula,
}

/// The list in `config_home`'s `trust.json`, or `None` when there is a file
/// Banager cannot read: not a regular file, too big, or one it may not
/// open. No file at all is an empty list, as it is to Homebrew
/// (`return {} unless trust_path.exist?`), and so is one that does not
/// parse as a JSON object (`rescue Errno::ENOENT, JSON::ParserError`,
/// `return {} unless parsed_store.is_a?(Hash)`). Opened without waiting,
/// at most `read_file::LIMIT` bytes of it, as the `brew.env` files are.
pub(crate) fn read_trust_list(config_home: &Path) -> Option<TrustList> {
    match crate::adapters::read_file::read_bytes(&config_home.join(TRUST_FILE)) {
        Ok(bytes) => Some(parse(&bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Some(TrustList::default()),
        Err(_) => None,
    }
}

/// `trust_store`'s reading of the file's bytes: each value as Ruby's
/// `Array(entries).map { |entry| entry.to_s.downcase }` makes it -- a
/// string alone is a list of one, `null` none -- of which only strings and
/// numbers can name anything; the rest are left out.
fn parse(bytes: &[u8]) -> TrustList {
    let Ok(Value::Object(store)) = serde_json::from_slice::<Value>(bytes) else {
        return TrustList::default();
    };
    let names = |key: &str| -> Vec<String> {
        let entry = |value: &Value| match value {
            Value::String(text) => Some(text.to_lowercase()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        };
        match store.get(key) {
            Some(Value::Array(entries)) => entries.iter().filter_map(entry).collect(),
            Some(value) => entry(value).into_iter().collect(),
            None => Vec::new(),
        }
    };
    TrustList {
        taps: names("trustedtaps"),
        casks: names("trustedcasks"),
        formulae: names("trustedformulae"),
    }
}

/// `Tap.remote_reference?` (`tap.rb:141-143`): an entry that names a tap by
/// its remote -- a URL, an `scp`-style address or a path -- not by name.
fn remote_reference(entry: &str) -> bool {
    let scheme = entry
        .split_once(':')
        .is_some_and(|(head, tail)| !head.is_empty() && !head.contains('/') && !tail.is_empty());
    scheme || entry.starts_with(['/', '.', '~'])
}

/// A package's full name split into its tap (`user/repo`) and its own
/// name, as `Utils.tap_from_full_name` and `name_from_full_name` split it;
/// `None` for a name with no tap in it.
pub(crate) fn split_full_name(full_name: &str) -> Option<(&str, &str)> {
    let mut parts = full_name.splitn(3, '/');
    let (user, repo, name) = (parts.next()?, parts.next()?, parts.next()?);
    let tap_len = user.len() + 1 + repo.len();
    (!user.is_empty() && !repo.is_empty() && !name.is_empty() && !name.contains('/'))
        .then(|| (&full_name[..tap_len], name))
}

/// Whether `tap` is one of Homebrew's own (`Tap#official?`, `tap.rb:477-479`:
/// its user is `Homebrew`), which Homebrew trusts without a list entry
/// (`Tap#implicitly_trusted?`, `tap.rb:1520-1522`) -- as long as its remote
/// is Homebrew's, which Banager takes it to be.
pub(crate) fn official(tap: &str) -> bool {
    tap.split('/')
        .next()
        .is_some_and(|user| user.eq_ignore_ascii_case("homebrew"))
}

impl TrustList {
    fn entries(&self, kind: Kind) -> &[String] {
        match kind {
            Kind::Cask => &self.casks,
            Kind::Formula => &self.formulae,
        }
    }

    /// `Homebrew::Trust.trusted?(:tap, tap)` (`trust.rb:183-193`) as far as
    /// the list alone can tell: `Some(true)` when an entry names the tap,
    /// `Some(false)` when no entry could match it, `None` when an entry
    /// names a tap by its remote, which might be this one's.
    fn tap_listed(&self, tap: &str) -> Option<bool> {
        let tap = tap.to_lowercase();
        if self.taps.contains(&tap) {
            return Some(true);
        }
        if self.taps.iter().any(|entry| remote_reference(entry)) {
            return None;
        }
        Some(false)
    }

    /// Whether `brew uninstall` will delete this list's entry for
    /// `full_name`, a `kind` from the tap it names: after the uninstall,
    /// for each package it names whose tap is not on the list itself
    /// (`Trust.trusted?(:tap, …)`), Homebrew deletes that package's own
    /// entry (`cmd/uninstall.rb:122-127`, `Trust.untrust!`,
    /// `trust.rb:65-90`), and writes the file only when there was one.
    /// True only when the list holds that entry and surely does not hold
    /// the tap; false where it cannot tell.
    pub(crate) fn uninstall_forgets(&self, kind: Kind, full_name: &str) -> bool {
        let Some((tap, _)) = split_full_name(full_name) else {
            return false;
        };
        let name = full_name.to_lowercase();
        self.entries(kind).contains(&name) && self.tap_listed(tap) == Some(false)
    }

    /// Whether Homebrew surely trusts the cask `token` from `tap`
    /// (`Trust.trusted?(:cask, "tap/token")`, `trust.rb:183-209`): its tap
    /// is one of Homebrew's own, or the list names the cask or its tap.
    /// False where it cannot tell.
    pub(crate) fn trusts_cask(&self, tap: &str, token: &str) -> bool {
        if official(tap) {
            return true;
        }
        let name = format!("{tap}/{token}").to_lowercase();
        self.casks.contains(&name) || self.tap_listed(tap) == Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(json: &str) -> TrustList {
        parse(json.as_bytes())
    }

    #[test]
    fn reads_each_list_lowercased_and_a_lone_name_as_a_list_of_one() {
        let read = list(
            r#"{"trustedtaps":["Gautham-V/Tap"],"trustedcasks":"gautham-v/tap/ClaudeBar","trustedformulae":null}"#,
        );
        assert_eq!(read.taps, ["gautham-v/tap"]);
        assert_eq!(read.casks, ["gautham-v/tap/claudebar"]);
        assert!(read.formulae.is_empty());
    }

    #[test]
    fn a_file_that_is_not_a_json_object_is_an_empty_list() {
        for text in ["", "not json", "[]", "\"x\"", "\u{0}"] {
            assert_eq!(list(text), TrustList::default(), "{text:?}");
        }
    }

    #[test]
    fn no_file_is_an_empty_list_and_a_file_banager_cannot_read_is_unknown() {
        let dir = crate::adapters::read_file::tests::temp_dir("trust-list");
        assert_eq!(read_trust_list(&dir), Some(TrustList::default()));
        std::fs::write(
            dir.join(TRUST_FILE),
            r#"{"trustedcasks":["someone/tap/thing"]}"#,
        )
        .unwrap();
        assert_eq!(read_trust_list(&dir).unwrap().casks, ["someone/tap/thing"]);
        // A folder where the file should be: not read.
        let other = crate::adapters::read_file::tests::temp_dir("trust-list-dir");
        std::fs::create_dir(other.join(TRUST_FILE)).unwrap();
        assert_eq!(read_trust_list(&other), None);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&other).unwrap();
    }

    #[test]
    fn an_uninstall_forgets_the_packages_own_entry_unless_its_tap_is_listed() {
        let only_the_cask = list(r#"{"trustedcasks":["gautham-v/tap/claudebar"]}"#);
        assert!(only_the_cask.uninstall_forgets(Kind::Cask, "gautham-v/tap/claudebar"));
        assert!(only_the_cask.uninstall_forgets(Kind::Cask, "Gautham-V/tap/ClaudeBar"));
        // Another kind's list, another cask, a name with no tap in it.
        assert!(!only_the_cask.uninstall_forgets(Kind::Formula, "gautham-v/tap/claudebar"));
        assert!(!only_the_cask.uninstall_forgets(Kind::Cask, "gautham-v/tap/codexbar"));
        assert!(!only_the_cask.uninstall_forgets(Kind::Cask, "claudebar"));
        // Its tap on the list: Homebrew leaves the entry.
        let with_tap =
            list(r#"{"trustedtaps":["gautham-v/tap"],"trustedcasks":["gautham-v/tap/claudebar"]}"#);
        assert!(!with_tap.uninstall_forgets(Kind::Cask, "gautham-v/tap/claudebar"));
        // A tap named by its remote might be this one: not said.
        let by_remote = list(
            r#"{"trustedtaps":["https://github.com/someone/homebrew-tap"],"trustedformulae":["someone/tap/thing"]}"#,
        );
        assert!(!by_remote.uninstall_forgets(Kind::Formula, "someone/tap/thing"));
        // An official tap is not on the list either, so its entry goes too.
        let core = list(r#"{"trustedformulae":["homebrew/core/wget"]}"#);
        assert!(core.uninstall_forgets(Kind::Formula, "homebrew/core/wget"));
    }

    #[test]
    fn a_cask_is_surely_trusted_from_an_official_tap_or_when_the_list_names_it_or_its_tap() {
        let empty = TrustList::default();
        assert!(empty.trusts_cask("homebrew/cask", "firefox"));
        assert!(!empty.trusts_cask("gautham-v/tap", "claudebar"));
        assert!(list(r#"{"trustedcasks":["gautham-v/tap/claudebar"]}"#)
            .trusts_cask("gautham-v/tap", "claudebar"));
        assert!(
            list(r#"{"trustedtaps":["gautham-v/tap"]}"#).trusts_cask("gautham-v/tap", "claudebar")
        );
        // Another cask of the tap, or a tap named by a remote: not surely.
        assert!(!list(r#"{"trustedcasks":["gautham-v/tap/codexbar"]}"#)
            .trusts_cask("gautham-v/tap", "claudebar"));
        assert!(
            !list(r#"{"trustedtaps":["git@github.com:gautham-v/homebrew-tap.git"]}"#)
                .trusts_cask("gautham-v/tap", "claudebar")
        );
    }

    #[test]
    fn remote_references_are_told_from_tap_names_as_homebrew_tells_them() {
        for entry in [
            "https://github.com/a/homebrew-b",
            "git@github.com:a/homebrew-b.git",
            "/srv/taps/b",
            "./b",
            "~/b",
        ] {
            assert!(remote_reference(entry), "{entry}");
        }
        for entry in ["a/b", "homebrew/cask"] {
            assert!(!remote_reference(entry), "{entry}");
        }
    }

    #[test]
    fn a_full_name_splits_into_its_tap_and_its_own_name() {
        assert_eq!(
            split_full_name("gautham-v/tap/claudebar"),
            Some(("gautham-v/tap", "claudebar"))
        );
        assert_eq!(split_full_name("wget"), None);
        assert_eq!(split_full_name("a/b"), None);
        assert_eq!(split_full_name("a/b/c/d"), None);
    }
}
