//! `RealTrasher`: macOS's own "move to Trash".
//!
//! `NSFileManager trashItemAtURL:resultingItemURL:error:` is what Finder
//! calls. Observed on 2026-09-25 from an ad-hoc-signed bundle launched
//! through LaunchServices *without* Full Disk Access (the same process
//! could not list `~/.Trash`): a file, a directory and a symbolic link
//! whose target existed each moved into `~/.Trash` in every run, the link
//! as a link with its target untouched (a dangling link -- the launcher,
//! moved last in every uninstall -- is `tests/standalone_uninstall_test.rs`'s
//! smoke test), and a name collision suffixed by the system. Finder's
//! "Put Back" record was written for every item when the calls were at
//! least two seconds apart, and for only the first when they came back to
//! back -- an observation, not a documented behaviour
//! (`removal::PUT_BACK_SETTLE`). `docs/what-we-run.md` ("Moving files to
//! the Trash") carries the results.

use super::{TrashError, Trasher};
use crate::model::ItemKind;
use std::path::{Path, PathBuf};

/// The system's Trash. Stateless: `NSFileManager`'s shared manager is safe
/// to use from any thread, and there is nothing to configure. Built once,
/// by `Session::new`, for every standalone adapter.
#[derive(Debug, Default)]
pub struct RealTrasher;

impl RealTrasher {
    pub fn new() -> RealTrasher {
        RealTrasher
    }
}

#[cfg(target_os = "macos")]
impl Trasher for RealTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        use objc2::rc::{autoreleasepool, Retained};
        use objc2_foundation::{NSFileManager, NSString, NSURL};

        // `NSString` carries UTF-8, and macOS's file systems do not create
        // a name that is not: Canager's limitation, not the system's
        // answer, so `Unsupported` rather than a `Refused` in its own words.
        let Some(utf8) = path.to_str() else {
            return Err(TrashError::Unsupported);
        };
        // What the item is comes from the caller's last check, the `lstat`
        // made immediately before this call -- not from a look of this
        // function's own, which would sit between that check and the move.
        // A symbolic link is never a directory here, so its URL gets no
        // trailing slash that could resolve through it: the link itself is
        // what moves, never what it points at. (`fileURLWithPath:` alone
        // would `stat` through the link to decide.) Building the URL reads
        // nothing from the disk; the next thing that touches this path is
        // the system's move.
        let is_dir = kind == ItemKind::Dir;
        // A pool of its own: this runs on a thread of tokio's blocking pool,
        // which has none, and the URL, the path and the error description
        // come back autoreleased.
        let moved: Result<PathBuf, TrashError> = autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(utf8), is_dir);
            let mut resulting: Option<Retained<NSURL>> = None;
            NSFileManager::defaultManager()
                .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
                .map_err(|error| TrashError::Refused {
                    detail: error.localizedDescription().to_string(),
                })?;
            Ok(resulting
                .and_then(|url| url.path())
                .map(|trashed| PathBuf::from(trashed.to_string()))
                // The API fills the URL on success (Apple's contract, and
                // every spike run); the fallback names the home volume's
                // Trash so the log line still says where to look.
                .unwrap_or_else(|| PathBuf::from("~/.Trash")))
        });
        let trashed = moved?;
        #[cfg(debug_assertions)]
        report_trash_access(&trashed);
        Ok(trashed)
    }
}

/// Debug builds only (`cfg(debug_assertions)`: `pnpm tauri build --debug`,
/// `cargo test`; never a release build): whether this process may list the
/// Trash it has just moved an item into, printed to stderr. Without Full
/// Disk Access macOS refuses that listing with `Operation not permitted` --
/// how the Trash spike proved a run had no Full Disk Access -- so the
/// author's pre-merge check reads this line from a Finder-launched debug
/// build (`open --stderr`) to learn what that very process was allowed.
/// It runs after the move, never between an item's check and its move.
#[cfg(all(target_os = "macos", debug_assertions))]
fn report_trash_access(trashed: &Path) {
    let Some(trash) = trashed.parent() else {
        return;
    };
    match std::fs::read_dir(trash) {
        Ok(_) => eprintln!(
            "[canager] debug: read_dir({}) -> Ok: this process can list the Trash (it has Full Disk Access)",
            trash.display()
        ),
        Err(error) => eprintln!("[canager] debug: read_dir({}) -> Err: {error}", trash.display()),
    }
}

#[cfg(not(target_os = "macos"))]
impl Trasher for RealTrasher {
    fn trash(&self, _path: &Path, _kind: ItemKind) -> Result<PathBuf, TrashError> {
        Err(TrashError::Unsupported)
    }
}
