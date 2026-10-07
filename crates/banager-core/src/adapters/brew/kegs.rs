//! What a Homebrew prefix holds of one formula, read for the upgrade and
//! uninstall previews (the author's decision U9, r6): the versions
//! installed -- the folders in `<prefix>/Cellar/<name>`, its kegs, which
//! Homebrew lists as `rack.subdirs` (`Formula#installed_kegs`, and
//! `resolve_kegs` in `cli/named_args.rb:522-543` in Homebrew 7.0.7-9) --
//! and whether it is pinned, which Homebrew records as a link
//! `<prefix>/var/homebrew/pinned/<name>` (`FormulaPin#pinned?`). Only
//! names are read: no keg's contents, and nothing in or through a
//! protected place (`protected::look`).

use crate::protected::{look, Protected};
use std::cmp::Ordering;
use std::io::ErrorKind;
use std::path::Path;

/// One formula's kegs under a prefix, as `read_kegs` found them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Kegs {
    /// Every installed version, oldest first (`version_order`).
    pub(crate) versions: Vec<String>,
    /// Whether it is pinned -- or may be: a pin record Banager could not
    /// look at counts as one, so that nothing Banager adds to a command
    /// overrides Homebrew's own refusal to uninstall a pinned formula
    /// (`Uninstall.uninstall_kegs`, `uninstall.rb:45-53`).
    pub(crate) pinned: bool,
}

/// The kegs of the formula `name` (a tap's `user/tap/name` is `name` in
/// the Cellar) under `prefix`, or `None` when its folder in the Cellar
/// cannot be listed in full: not there, not a folder, in or through a
/// protected place, or more names than one directory budget
/// (`look::ListingBudget`) -- never the versions of part of it. A version
/// is a folder's name; a file there is none.
pub(crate) fn read_kegs(prefix: &Path, name: &str) -> Option<Kegs> {
    let protected = Protected::of_this_process();
    let short = name.rsplit('/').next().filter(|short| plain(short))?;
    let listing = look::list(&prefix.join("Cellar").join(short), &protected).ok()?;
    let mut versions: Vec<String> = listing
        .names(&mut look::ListingBudget::default())
        .ok()?
        .into_iter()
        .filter_map(|entry| {
            let meta = listing.lstat(&entry).ok()?;
            let version = entry.into_string().ok()?;
            (plain(&version) && meta.is_dir()).then_some(version)
        })
        .collect();
    versions.sort_by(|a, b| version_order(a, b));
    let pin = prefix.join("var/homebrew/pinned").join(short);
    let pinned = match look::lstat(&pin, &protected) {
        Ok(_) => true,
        Err(error) => error.kind() != ErrorKind::NotFound,
    };
    Some(Kegs { versions, pinned })
}

/// Whether `name` is one plain path component: not empty, no `/`, not `.`
/// or `..`.
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// `a` and `b` in the order people read versions in: runs of digits by
/// their value, everything else by its characters -- `1.9` before `1.10`,
/// `2.0` before `2.0_1`. Only for listing them; which ones a `brew cleanup`
/// deletes is Homebrew's own comparison (`Formula#eligible_kegs_for_cleanup`).
pub(crate) fn version_order(a: &str, b: &str) -> Ordering {
    let (mut left, mut right) = (runs(a), runs(b));
    loop {
        match (left.next(), right.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
                let order = if digits(x) && digits(y) {
                    let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                    x.len().cmp(&y.len()).then_with(|| x.cmp(y))
                } else {
                    x.cmp(y)
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// `text` cut into runs of ASCII digits and runs of anything else.
fn runs(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let digit = first.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digit)
            .unwrap_or(rest.len());
        let (run, after) = rest.split_at(end);
        rest = after;
        Some(run)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::read_file::tests::temp_dir;

    #[test]
    fn bounded_kegs_are_unknown_instead_of_incomplete() {
        let prefix = temp_dir("bounded-kegs");
        let rack = prefix.join("Cellar/wget");
        std::fs::create_dir_all(&rack).unwrap();
        for n in 0..4097 {
            std::fs::create_dir(rack.join(n.to_string())).unwrap();
        }
        let (answer, calls) = crate::dirfd::calls::measure(|| read_kegs(&prefix, "wget"));
        assert_eq!(answer, None);
        assert_eq!(calls.entries, 4097);
        std::fs::remove_dir_all(prefix).unwrap();
    }

    #[test]
    fn lists_a_formulas_versions_oldest_first_and_only_its_folders() {
        let prefix = temp_dir("kegs-versions");
        let rack = prefix.join("Cellar/wget");
        for version in ["1.25.0", "1.9.2", "1.25.0_1", "1.10.0"] {
            std::fs::create_dir_all(rack.join(version).join("bin")).unwrap();
        }
        std::fs::write(rack.join(".DS_Store"), b"").unwrap();
        let kegs = read_kegs(&prefix, "wget").expect("listed");
        assert_eq!(kegs.versions, ["1.9.2", "1.10.0", "1.25.0", "1.25.0_1"]);
        assert!(!kegs.pinned);
        // A tap's formula is its last name in the Cellar.
        assert_eq!(read_kegs(&prefix, "someone/tap/wget"), Some(kegs));
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn says_a_formula_is_pinned_when_homebrew_recorded_a_pin() {
        let prefix = temp_dir("kegs-pinned");
        std::fs::create_dir_all(prefix.join("Cellar/jq/1.8.2")).unwrap();
        let pins = prefix.join("var/homebrew/pinned");
        std::fs::create_dir_all(&pins).unwrap();
        assert!(!read_kegs(&prefix, "jq").unwrap().pinned);
        std::os::unix::fs::symlink("../../../Cellar/jq/1.8.2", pins.join("jq")).unwrap();
        assert!(read_kegs(&prefix, "jq").unwrap().pinned);
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn reads_nothing_of_a_formula_with_no_folder_or_in_a_protected_place() {
        let prefix = temp_dir("kegs-none");
        assert_eq!(read_kegs(&prefix, "wget"), None);
        assert_eq!(read_kegs(&prefix, ".."), None);
        std::fs::write(prefix.join("Cellar"), b"").unwrap();
        assert_eq!(read_kegs(&prefix, "wget"), None);
        // A prefix kept in `~/Documents` (`home` as the home folder): never
        // looked into.
        let home = std::fs::canonicalize(&prefix).unwrap();
        let kept = home.join("Documents/homebrew");
        std::fs::create_dir_all(kept.join("Cellar/wget/1.25.0")).unwrap();
        assert!(read_kegs(&kept, "wget").is_some());
        let as_if = crate::protected::as_if_home(&home);
        assert_eq!(read_kegs(&kept, "wget"), None);
        drop(as_if);
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn orders_versions_as_people_read_them() {
        let mut versions = vec![
            "1.10",
            "1.9",
            "2.0_1",
            "2.0",
            "1.9.10",
            "1.9.9",
            "HEAD-abc1234",
            "0.9",
        ];
        versions.sort_by(|a, b| version_order(a, b));
        assert_eq!(
            versions,
            [
                "0.9",
                "1.9",
                "1.9.9",
                "1.9.10",
                "1.10",
                "2.0",
                "2.0_1",
                "HEAD-abc1234"
            ]
        );
        assert_eq!(version_order("1.08", "1.8"), "1.08".cmp("1.8"));
    }
}
