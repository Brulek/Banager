//! `MockTrasher`: the Trash as a temporary directory, for tests only --
//! `trash/mod.rs` compiles this module under `cfg(test)` or the
//! `test-support` feature, never into a release build.

use super::{TrashError, Trasher};
use crate::model::ItemKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Renames each item into a temporary directory of its own -- a symbolic
/// link is renamed as a link, exactly as `trashItemAtURL:` moves one --
/// records every call in order, with the kind it was told the item is, and
/// can be told to refuse the n-th call or to fire a cancellation token
/// after it: the two ways an uninstall stops partway. Its bin is removed on
/// drop -- a permanent delete of whatever was moved into it, which is why
/// it is test-only. Read by the removal and adapter tests
/// (adapters/standalone/), tests/ops_upgrade_version_test.rs and
/// tests/standalone_uninstall_test.rs.
pub struct MockTrasher {
    bin: PathBuf,
    calls: Mutex<Vec<(PathBuf, ItemKind)>>,
    refuse: Mutex<Option<(usize, String)>>,
    cancel_after: Mutex<Option<(usize, CancellationToken)>>,
}

impl MockTrasher {
    pub fn new() -> MockTrasher {
        // Every `adapter()` in the standalone tests builds one, and those
        // tests run in parallel in one process: two can read the same time
        // (macOS's realtime clock counts whole microseconds), so a sequence
        // number keeps each bin its own -- a shared bin would be deleted
        // under one test by the other one's drop.
        static NEXT_BIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let raw = std::env::temp_dir().join(format!(
            "banager-mock-trash-{}-{}-{}",
            std::process::id(),
            NEXT_BIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create the mock trash directory");
        MockTrasher {
            // Canonical, so the paths `trash` returns compare equal to what
            // a test builds from `bin()` (macOS's `/var/folders` is
            // `/private/var/…`).
            bin: std::fs::canonicalize(&raw).expect("canonical mock trash directory"),
            calls: Mutex::new(Vec::new()),
            refuse: Mutex::new(None),
            cancel_after: Mutex::new(None),
        }
    }

    /// Where trashed items land.
    pub fn bin(&self) -> &Path {
        &self.bin
    }

    /// Every path handed to `trash`, in order -- a refused one included.
    pub fn calls(&self) -> Vec<PathBuf> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// The kind each call was told its item is, in the same order as
    /// `calls`: what the check immediately before the call saw.
    pub fn kinds(&self) -> Vec<ItemKind> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(_, kind)| *kind)
            .collect()
    }

    /// The `nth` call (0-based, counted over this trasher's whole life)
    /// fails with `Refused { detail }` and moves nothing; every other call
    /// proceeds.
    pub fn refuse_call(&self, nth: usize, detail: &str) {
        *self.refuse.lock().unwrap() = Some((nth, detail.to_string()));
    }

    /// Fires `token` right after the `nth` call (0-based) has moved its
    /// item and before the call returns: a user pressing Cancel while that
    /// move is still being reported.
    pub fn cancel_after_call(&self, nth: usize, token: CancellationToken) {
        *self.cancel_after.lock().unwrap() = Some((nth, token));
    }
}

impl Default for MockTrasher {
    fn default() -> Self {
        MockTrasher::new()
    }
}

impl Trasher for MockTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        let nth = {
            let mut calls = self.calls.lock().unwrap();
            calls.push((path.to_path_buf(), kind));
            calls.len() - 1
        };
        let refused = self
            .refuse
            .lock()
            .unwrap()
            .clone()
            .filter(|(refused, _)| *refused == nth);
        if let Some((_, detail)) = refused {
            return Err(TrashError::Refused { detail });
        }
        let name = path.file_name().ok_or_else(|| TrashError::Refused {
            detail: format!("{} has no file name", path.display()),
        })?;
        let mut dest = self.bin.join(name);
        if std::fs::symlink_metadata(&dest).is_ok() {
            // The system suffixes a colliding name with the time of day; the
            // call index does the same job here and is predictable.
            dest = self.bin.join(format!("{} {nth}", name.to_string_lossy()));
        }
        std::fs::rename(path, &dest).map_err(|error| TrashError::Refused {
            detail: error.to_string(),
        })?;
        if let Some((_, token)) = self
            .cancel_after
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(after, _)| *after == nth)
        {
            token.cancel();
        }
        Ok(dest)
    }
}

impl Drop for MockTrasher {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.bin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory beside the mock's own, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "banager-trash-mock-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).expect("create scratch dir");
            Scratch(std::fs::canonicalize(&dir).expect("canonical scratch dir"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_mock_trasher_moves_a_file_a_directory_and_a_link_as_a_link_and_records_each() {
        let scratch = Scratch::new("moves");
        let file = scratch.0.join("file.txt");
        std::fs::write(&file, b"x").expect("write file");
        let dir = scratch.0.join("dir");
        std::fs::create_dir(&dir).expect("create dir");
        std::fs::write(dir.join("inner"), b"y").expect("write inner");
        let link = scratch.0.join("link");
        std::os::unix::fs::symlink(&file, &link).expect("symlink");

        let trasher = MockTrasher::new();
        let moved_link = trasher
            .trash(&link, ItemKind::Symlink)
            .expect("trash the link");
        let moved_dir = trasher.trash(&dir, ItemKind::Dir).expect("trash the dir");
        let moved_file = trasher
            .trash(&file, ItemKind::File)
            .expect("trash the file");

        // Each landed in the mock's bin under its own name.
        assert_eq!(moved_link, trasher.bin().join("link"));
        assert_eq!(moved_dir, trasher.bin().join("dir"));
        assert_eq!(moved_file, trasher.bin().join("file.txt"));
        // The link was moved as a link, exactly as `trashItemAtURL:` moves
        // one; its target was still in place when it moved.
        assert!(std::fs::symlink_metadata(&moved_link)
            .expect("moved link")
            .file_type()
            .is_symlink());
        assert!(moved_dir.join("inner").is_file());
        assert!(moved_file.is_file());
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(std::fs::symlink_metadata(&dir).is_err());
        assert!(std::fs::symlink_metadata(&file).is_err());
        assert_eq!(trasher.calls(), vec![link, dir, file]);
        assert_eq!(
            trasher.kinds(),
            vec![ItemKind::Symlink, ItemKind::Dir, ItemKind::File]
        );
    }

    #[test]
    fn test_mock_trasher_gives_a_second_item_of_the_same_name_a_suffixed_name() {
        // Claude Code's launcher and program directory are both named
        // `claude`. The system suffixes the second with the time of day;
        // the mock suffixes it with the call index, so a test can name it.
        let scratch = Scratch::new("collision");
        let dir = scratch.0.join("share/claude");
        std::fs::create_dir_all(&dir).expect("create dir");
        let link = scratch.0.join("bin/claude");
        std::fs::create_dir_all(link.parent().unwrap()).expect("create bin");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");

        let trasher = MockTrasher::new();
        assert_eq!(
            trasher.trash(&dir, ItemKind::Dir).expect("dir"),
            trasher.bin().join("claude")
        );
        assert_eq!(
            trasher.trash(&link, ItemKind::Symlink).expect("link"),
            trasher.bin().join("claude 1")
        );
        assert!(trasher.bin().join("claude").is_dir());
        assert!(std::fs::symlink_metadata(trasher.bin().join("claude 1"))
            .expect("moved link")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn test_mock_trasher_refuses_the_nth_call_and_moves_nothing_for_it() {
        let scratch = Scratch::new("refuse");
        let first = scratch.0.join("first");
        let second = scratch.0.join("second");
        std::fs::write(&first, b"1").expect("first");
        std::fs::write(&second, b"2").expect("second");

        let trasher = MockTrasher::new();
        trasher.refuse_call(1, "“second” couldn’t be moved to the Trash.");
        trasher.trash(&first, ItemKind::File).expect("first moves");
        let err = trasher
            .trash(&second, ItemKind::File)
            .expect_err("second is refused");
        assert_eq!(err.to_string(), "“second” couldn’t be moved to the Trash.");
        assert!(second.is_file(), "a refused item stays where it was");
        // Refused calls are recorded too: the adapter's tests assert the
        // whole sequence of what was attempted.
        assert_eq!(trasher.calls(), vec![first, second]);
    }

    #[test]
    fn test_mock_trasher_fires_a_token_after_the_nth_call() {
        let scratch = Scratch::new("cancel");
        let first = scratch.0.join("first");
        std::fs::write(&first, b"1").expect("first");
        let token = CancellationToken::new();

        let trasher = MockTrasher::new();
        trasher.cancel_after_call(0, token.clone());
        assert!(!token.is_cancelled());
        trasher.trash(&first, ItemKind::File).expect("first moves");
        assert!(
            token.is_cancelled(),
            "the user pressed Cancel after the first item"
        );
    }

    #[test]
    fn test_mock_trasher_removes_its_bin_on_drop() {
        let bin = {
            let trasher = MockTrasher::new();
            trasher.bin().to_path_buf()
        };
        assert!(!bin.exists());
    }
}
