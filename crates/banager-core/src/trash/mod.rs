//! Moving files to the Trash: the one change Canager makes to a file in
//! its own process besides its settings (phase 4 spec §6.2, 附录 B).
//! `Trasher` is the seam, like `CommandRunner` and `HttpClient`:
//! `RealTrasher` is macOS's own `NSFileManager trashItemAtURL:` -- the call
//! Finder makes, which needs no Full Disk Access -- and `MockTrasher`
//! renames into a temporary directory and records every call, so an
//! uninstall can be tested end to end without touching anyone's Trash.
//! Whether Finder can later "Put Back" an item this call moved is Finder's
//! own record, observed but not promised (`removal::PUT_BACK_SETTLE`, and
//! docs/what-we-run.md, "Moving files to the Trash").
//!
//! The trait has one method on purpose: in a release build the only thing
//! this module can do to a file is move it to the Trash (`RealTrasher`).
//! `MockTrasher` -- which creates a temporary directory, renames items into
//! it and deletes it when dropped -- is compiled only for tests
//! (`cfg(test)`, or the `test-support` feature, which only dev-dependencies
//! turn on: this crate's own `tests/` through its dev-dependency on
//! itself), so no release build contains a delete, a rename or a directory
//! creation here.

use crate::model::ItemKind;
use std::path::{Path, PathBuf};

#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod real;

#[cfg(any(test, feature = "test-support"))]
pub use mock::MockTrasher;
pub use real::RealTrasher;

/// Why an item was not moved.
#[derive(Debug, thiserror::Error)]
pub enum TrashError {
    /// The system refused to move this item; `detail` is its own words (an
    /// `NSError`'s localized description; `MockTrasher` stands in for it
    /// with a detail of its own), shown as-is like a tool's stderr --
    /// `removal::execute_removal` puts it in `Outcome::Failed`'s summary
    /// and in a `LogNote::TrashFailed`.
    #[error("{detail}")]
    Refused { detail: String },
    /// Canager could not hand the item to the system at all: not macOS,
    /// where nothing implements the move (Canager v0.1 ships for macOS
    /// only, the crate doc in lib.rs; a build for another Unix reaches this
    /// at run time, honestly, rather than failing to compile), or a path
    /// that is not valid UTF-8, which `NSString` cannot carry (macOS's file
    /// systems do not create such names). Canager's own limitation, with no
    /// words of the Mac's to quote: `execute_removal` reports it as
    /// `Fault::Internal`, never as `Failed` or `TrashFailed`. Produced by
    /// `RealTrasher`: off macOS for every path, and on macOS only for a
    /// path whose home folder's own name is not UTF-8 -- so, either way,
    /// for every path of one uninstall alike, from the first on.
    #[error("Canager could not hand this item to the system's Trash")]
    Unsupported,
}

/// The seam. Implemented by `RealTrasher` (macOS's Trash) and
/// `MockTrasher` (a temporary directory, for tests); read by
/// `removal::execute_removal`, which calls it once per path of a
/// `PlanAction::TrashPaths` plan, in order, each immediately after that
/// item's last check.
pub trait Trasher: Send + Sync {
    /// Moves the item at `path` -- a file, a directory, or a symbolic link
    /// (the link itself, never its target) -- to the Trash, and returns
    /// where it now is (for the operation log). `kind` is what the check
    /// made immediately before this call found at `path` (its last
    /// `lstat`): the caller's, so an implementation never needs a second
    /// look of its own between that check and the move.
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError>;
}
