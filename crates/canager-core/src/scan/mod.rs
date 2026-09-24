//! The unknown-source scan: the command-line programs on this Mac that
//! none of the registered sources installed (design spec §4.2; phase 4
//! spec §八).
//!
//! Not an `Adapter`, on purpose. An adapter has a `plan`, an `execute`, a
//! `reconcile`, an instance and a fixture directory of recorded output,
//! and this has none of them; registering it as one would trip the
//! fixture-set equality test, put it in the operation manager's registry
//! and make the plan gate answer "is this actionable?" for something
//! that can never be. The decisive reason is simpler: deciding who owns a
//! program needs *every other* source's instances and artifacts, and an
//! adapter's `inventory(&self, inst)` sees only its own. So this is a
//! pure function over a clone of the snapshot (`Session::scan_unknown`),
//! run on demand from the Unknown page -- never from a refresh, never
//! into the `Snapshot`, never under a lock.
//!
//! Read-only in the strictest sense: `read_dir`, `symlink_metadata`,
//! `metadata`, `read_link` and `canonicalize`, one level deep, over a
//! fixed list of bin directories. No `CommandRunner`, so nothing it finds
//! is ever run; no write of any kind.
//!
//! Under `scan/`, not `adapters/unknown.rs` as the design spec's §3 drew
//! it: someone reading `adapters/` should not find a module there with no
//! `impl Adapter` (phase 4 spec §8.1, Q12; its appendix C records the
//! deviation).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// How much of the file system one scan may look at before it stops and
/// says so. Runtime values rather than constants, so the two numbers the
/// user reads in the "this list may be incomplete" banner come from the
/// same place the scan enforced them (`ScanStop` carries them out;
/// `Fault::HomebrewStillUpdating { minutes }` in model.rs is the
/// precedent for putting the number in the payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanBudget {
    /// Directory entries examined, counting the ones a known source
    /// claimed and the ones skipped as subdirectories or non-executables.
    pub max_entries: usize,
    /// Wall-clock time from the start of the walk -- the clock starts
    /// after the known sources are indexed, so canonicalising their paths
    /// is not charged to it -- checked before every `read_dir` and before
    /// every entry.
    pub max_duration: Duration,
}

impl Default for ScanBudget {
    /// The design spec's §4.2 numbers. Sized for a `~/bin` of a few
    /// thousand files; the seven directories on the research machine held
    /// 26 entries between them and took 64 ms.
    fn default() -> ScanBudget {
        ScanBudget {
            max_entries: 2000,
            max_duration: Duration::from_secs(10),
        }
    }
}

/// Why a scan stopped before it had looked at everything. Read by the
/// Unknown page's banner (`unknown.stopped.FileLimit` /
/// `unknown.stopped.TimeLimit`), which prints the number carried here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanStop {
    /// `ScanBudget::max_entries` entries had been examined and there was
    /// another.
    FileLimit { max_entries: u32 },
    /// `ScanBudget::max_duration` had elapsed before the next `read_dir`
    /// or the next entry.
    TimeLimit { max_secs: u32 },
}

/// One directory the scan actually read, and how many of its entries it
/// examined (claimed, listed or skipped alike). The page's "Looked in:"
/// footer, so an empty list reads as "looked in seven places", not
/// "didn't look".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedDir {
    /// With the home directory abbreviated to `~`, on this side, for the
    /// reason `Warning`'s `{{path}}` payloads are: the front end has no
    /// `HOME` to strip, and this is data, not a sentence.
    pub path: PathBuf,
    pub entries: u32,
}

/// What one listed entry is. Read by the page's kind badge
/// (`unknown.kind.*`, through a `Record<EntryKind, string>` so a variant
/// added here without copy fails `tsc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Symlink,
    /// A symlink whose target `canonicalize` could not reach. `link_target`
    /// still carries what it says; the research machine had one pointing
    /// into an app that had since been deleted.
    BrokenSymlink,
}

/// One program no registered source accounts for. Every field is read by
/// the Unknown page (`src/pages/UnknownPage.tsx`); the comment on each
/// says where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownEntry {
    /// The entry as found, `~`-abbreviated like `ScannedDir::path`. The
    /// row's name is its last component; the row's first line is the path.
    pub path: PathBuf,
    /// The badge.
    pub kind: EntryKind,
    /// `canonicalize` of the entry: every link hop followed, absolute, never
    /// abbreviated. `None` for a broken link, and for the rare regular
    /// file whose parent cannot be resolved. Shown under technical details
    /// as "Links to …" for a `Symlink`.
    pub resolved: Option<PathBuf>,
    /// `readlink`'s text, verbatim, for links only -- relative or absolute
    /// as the installer wrote it. The `{{target}}` of the broken-link
    /// sentence.
    pub link_target: Option<String>,
    /// The target's size. `None` for a broken link: there is no target to
    /// measure. Formatted by `formatBytes` into the size · date subtitle.
    pub size_bytes: Option<u64>,
    /// The target's modification time, unix seconds. `None` for a broken
    /// link, whose own `mtime` would only say when the link was made.
    /// Formatted with `Intl.DateTimeFormat` (an absolute date; this
    /// repository deliberately has no relative-time formatter).
    pub modified_at: Option<i64>,
    /// Whether the entry itself belongs to the user Canager runs as
    /// (`st_uid == euid`, of the entry, not its target: the question is
    /// who put it here). `false` renders "Put here by an installer with
    /// administrator rights".
    pub owned_by_me: bool,
    /// The `.app` bundle any component of the path runs inside, without
    /// the `.app`; tried on `resolved`, then on a link's own text (the
    /// only path a broken link has), then on the entry's path. Renders
    /// "Part of {{app}}".
    pub app_bundle: Option<String>,
}

/// The result of one scan. Not the `Snapshot`'s: produced on demand by
/// `Session::scan_unknown`, returned by the `scan_unknown` IPC command,
/// held only by the Unknown page's query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownScan {
    /// Every directory actually read, in scan order. Directories that do
    /// not exist are not here (29 of the research machine's 36 candidates
    /// did not).
    pub scanned: Vec<ScannedDir>,
    /// The programs nobody claimed: the rows.
    pub entries: Vec<UnknownEntry>,
    /// How many examined programs a registered source accounted for and
    /// are therefore not listed. The page's "N more programs came from
    /// sources Canager knows" sentence.
    pub attributed: u32,
    /// `Some` when the scan hit its budget; the page's banner. What was
    /// scanned before that point is still in the fields above.
    pub stopped: Option<ScanStop>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_budget_default_is_the_spec_numbers() {
        let budget = ScanBudget::default();
        assert_eq!(budget.max_entries, 2000);
        assert_eq!(budget.max_duration, Duration::from_secs(10));
    }

    #[test]
    fn test_scan_wire_shapes_match_the_hand_written_ts_mirror() {
        // `src/lib/types.ts` spells `EntryKind` as bare strings and
        // `ScanStop` as externally tagged single-key objects carrying the
        // limit the scan really enforced -- so `unknown.stopped.*` can
        // print that number rather than a copy typed into the locale
        // files (`Fault::HomebrewStillUpdating { minutes }` is the
        // precedent, model.rs).
        assert_eq!(
            serde_json::to_string(&EntryKind::File).unwrap(),
            r#""File""#
        );
        assert_eq!(
            serde_json::to_string(&EntryKind::Symlink).unwrap(),
            r#""Symlink""#
        );
        assert_eq!(
            serde_json::to_string(&EntryKind::BrokenSymlink).unwrap(),
            r#""BrokenSymlink""#
        );
        assert_eq!(
            serde_json::to_string(&ScanStop::FileLimit { max_entries: 2000 }).unwrap(),
            r#"{"FileLimit":{"max_entries":2000}}"#
        );
        assert_eq!(
            serde_json::to_string(&ScanStop::TimeLimit { max_secs: 10 }).unwrap(),
            r#"{"TimeLimit":{"max_secs":10}}"#
        );

        let scan = UnknownScan {
            scanned: vec![ScannedDir {
                path: PathBuf::from("~/.local/bin"),
                entries: 5,
            }],
            entries: vec![UnknownEntry {
                path: PathBuf::from("~/.local/bin/old-script"),
                kind: EntryKind::BrokenSymlink,
                resolved: None,
                link_target: Some(
                    "/Applications/Removed.app/Contents/Resources/index.js".to_string(),
                ),
                size_bytes: None,
                modified_at: None,
                owned_by_me: true,
                app_bundle: Some("Removed".to_string()),
            }],
            attributed: 4,
            stopped: None,
        };
        let json = serde_json::to_string(&scan).expect("serialize");
        assert!(
            json.contains(r#""stopped":null"#),
            "a complete scan carries an explicit null, not a missing key: {json}"
        );
        assert!(json.contains(r#""kind":"BrokenSymlink""#), "{json}");
        assert!(json.contains(r#""resolved":null"#), "{json}");
        assert!(json.contains(r#""owned_by_me":true"#), "{json}");
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            scan
        );

        let stopped = UnknownScan {
            stopped: Some(ScanStop::TimeLimit { max_secs: 10 }),
            ..scan
        };
        let json = serde_json::to_string(&stopped).expect("serialize");
        assert!(
            json.contains(r#""stopped":{"TimeLimit":{"max_secs":10}}"#),
            "{json}"
        );
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            stopped
        );
    }
}
