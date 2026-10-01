//! The Other Programs page's Show in Finder (`reveal_in_finder`): Finder
//! shows a program the last unknown-source scan found, and nothing else.
//!
//! The window used to call the opener plugin's own `reveal_item_in_dir`
//! (`opener:allow-reveal-item-in-dir`), which takes any path -- the
//! permission has no scope -- resolves it, and so answers whether anything
//! is there, anywhere on the Mac, protected folders included, and has
//! Finder open on it. The window now sends the path to this command, which
//! asks Finder only for a path the scan itself resolved
//! (`UnknownEntry::resolved`), as the newest scan the window was handed
//! found it (`Revealable::remember`, from `ipc::scan_unknown`), and refuses
//! every other path before anything is read.

use banager_core::scan::UnknownScan;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

/// The paths Show in Finder may show: every `resolved` of the newest scan
/// handed to the window. Managed on the builder in `run()`; in memory only.
#[derive(Debug, Default)]
pub struct Revealable(Mutex<HashSet<PathBuf>>);

impl Revealable {
    /// From now on, the paths `scan` resolved, and no others: a program
    /// that is gone from the newest scan is gone from the page as well.
    pub fn remember(&self, scan: &UnknownScan) {
        *self.0.lock().unwrap() = scan
            .entries
            .iter()
            .filter_map(|entry| entry.resolved.clone())
            .collect();
    }

    /// Whether `path` is, exactly, one the newest scan resolved.
    pub fn allows(&self, path: &Path) -> bool {
        self.0.lock().unwrap().contains(path)
    }
}

/// The refusal of a path the newest scan did not resolve, in the
/// `{"kind": ...}` envelope every command's refusals use.
fn not_revealable_json() -> String {
    serde_json::json!({ "kind": "not_revealable" }).to_string()
}

/// `reveal_in_finder`'s whole effect: `reveal` asked of `path` when the
/// newest scan resolved it, and nothing at all otherwise. `reveal` is a
/// parameter so a test never opens Finder.
pub(crate) fn reveal_impl(
    revealable: &Revealable,
    path: &Path,
    reveal: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if !revealable.allows(path) {
        return Err(not_revealable_json());
    }
    reveal(path).map_err(|detail| {
        serde_json::json!({ "kind": "reveal_failed", "detail": detail }).to_string()
    })
}

/// Has Finder show `path` -- a window on its folder, with it selected --
/// through the opener plugin's `reveal_item_in_dir` function, which
/// resolves it again (`realpath`) and makes one call, `NSWorkspace
/// activateFileViewerSelectingURLs:`, and runs nothing. Only a path the
/// newest scan resolved (`Revealable`); any other is refused as
/// `not_revealable` and nothing is read.
#[tauri::command]
pub async fn reveal_in_finder(
    revealable: State<'_, Revealable>,
    path: PathBuf,
) -> Result<(), String> {
    reveal_impl(&revealable, &path, |path| {
        tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use banager_core::scan::{EntryKind, UnknownEntry};
    use std::cell::RefCell;

    fn entry(path: &str, resolved: Option<&str>) -> UnknownEntry {
        UnknownEntry {
            path: PathBuf::from(path),
            kind: EntryKind::File,
            resolved: resolved.map(PathBuf::from),
            link_target: None,
            size_bytes: None,
            modified_at: None,
            owned_by_me: true,
            app_bundle: None,
        }
    }

    fn scan(entries: Vec<UnknownEntry>) -> UnknownScan {
        UnknownScan {
            scanned: Vec::new(),
            protected_dirs: Vec::new(),
            entries,
            attributed: 0,
            stopped: None,
        }
    }

    #[test]
    fn test_only_a_path_the_newest_scan_resolved_is_shown_in_finder() {
        let revealable = Revealable::default();
        let shown = RefCell::new(Vec::new());
        let reveal = |path: &Path| {
            shown.borrow_mut().push(path.to_path_buf());
            Ok(())
        };
        // Before any scan, nothing.
        assert_eq!(
            reveal_impl(&revealable, Path::new("/usr/local/bin/tool"), reveal),
            Err(r#"{"kind":"not_revealable"}"#.to_string())
        );

        revealable.remember(&scan(vec![
            entry("/usr/local/bin/tool", Some("/usr/local/bin/tool")),
            entry(
                "~/.local/bin/helper",
                Some("/Applications/Helper.app/helper"),
            ),
            entry("/usr/local/bin/broken", None),
        ]));
        assert_eq!(
            reveal_impl(&revealable, Path::new("/usr/local/bin/tool"), reveal),
            Ok(())
        );
        assert_eq!(
            reveal_impl(
                &revealable,
                Path::new("/Applications/Helper.app/helper"),
                reveal
            ),
            Ok(())
        );
        // What the row shows rather than what the scan resolved, a broken
        // link's own path, a folder holding a program, a path dressed up to
        // lead into one, and any other place on the Mac: refused.
        for refused in [
            "~/.local/bin/helper",
            "/usr/local/bin/broken",
            "/usr/local/bin",
            "/usr/local/bin/tool/../../../../Users/someone/Documents",
            "/Users/someone/Documents/secret.pdf",
            "",
        ] {
            assert_eq!(
                reveal_impl(&revealable, Path::new(refused), reveal),
                Err(r#"{"kind":"not_revealable"}"#.to_string()),
                "{refused:?}"
            );
        }
        assert_eq!(
            *shown.borrow(),
            [
                PathBuf::from("/usr/local/bin/tool"),
                PathBuf::from("/Applications/Helper.app/helper")
            ]
        );

        // A newer scan without it: no longer shown.
        revealable.remember(&scan(vec![entry(
            "/usr/local/bin/other",
            Some("/usr/local/bin/other"),
        )]));
        assert!(!revealable.allows(Path::new("/usr/local/bin/tool")));
        assert!(revealable.allows(Path::new("/usr/local/bin/other")));
    }

    #[test]
    fn test_a_finder_failure_comes_back_in_the_envelope() {
        let revealable = Revealable::default();
        revealable.remember(&scan(vec![entry(
            "/usr/local/bin/tool",
            Some("/usr/local/bin/tool"),
        )]));
        let err = reveal_impl(&revealable, Path::new("/usr/local/bin/tool"), |_| {
            Err("No such file or directory (os error 2)".to_string())
        })
        .unwrap_err();
        let v: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!(v["kind"], "reveal_failed");
        assert_eq!(v["detail"], "No such file or directory (os error 2)");
    }

    #[test]
    fn test_the_window_is_given_no_command_of_the_opener_plugin() {
        // Show in Finder goes through `reveal_in_finder`; a permission of
        // the plugin's would let the page ask Finder about any path again.
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let opener: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
            .filter(|p| p.starts_with("opener:"))
            .collect();
        assert_eq!(opener, Vec::<&str>::new());
    }
}
