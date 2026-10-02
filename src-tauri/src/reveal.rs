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
//!
//! A path the scan resolved is a name, and what it names can change after
//! the scan: the program, or a folder on its way, replaced by a link into
//! `~/Documents`. So before Finder is asked, the path is looked up again
//! the way the scan looked it up (`protected::resolve`: one step at a
//! time, never into a protected place), and Finder is asked only when it
//! still leads, with no link anywhere on its way, to the very file the
//! scan found there (`UnknownEntry::seen`: its device and inode). Finder
//! is then asked through AppKit directly (`NSWorkspace
//! activateFileViewerSelectingURLs:`), with that path as it is: nothing
//! resolves it again first -- the opener plugin's `reveal_item_in_dir`
//! would (`std::fs::canonicalize`, which follows any link, into any place).

use banager_core::dirfd::Stat;
use banager_core::protected::{self, Protected, Resolution};
use banager_core::runner::HostEnv;
use banager_core::scan::UnknownScan;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

/// The paths Show in Finder may show -- every `resolved` of the newest
/// scan handed to the window -- each with which file the scan found there
/// (`UnknownEntry::seen`). Managed on the builder in `run()`; in memory
/// only.
#[derive(Debug, Default)]
pub struct Revealable(Mutex<HashMap<PathBuf, Stat>>);

impl Revealable {
    /// From now on, the paths `scan` resolved, and no others: a program
    /// that is gone from the newest scan is gone from the page as well.
    pub fn remember(&self, scan: &UnknownScan) {
        *self.0.lock().unwrap() = scan
            .entries
            .iter()
            .filter_map(|entry| Some((entry.resolved.clone()?, entry.seen?)))
            .collect();
    }

    /// Which file the newest scan found at `path`, when `path` is, exactly,
    /// one it resolved.
    fn seen(&self, path: &Path) -> Option<Stat> {
        self.0.lock().unwrap().get(path).copied()
    }
}

/// The refusal of a path Finder may not be asked about, in the
/// `{"kind": ...}` envelope every command's refusals use.
fn not_revealable_json() -> String {
    serde_json::json!({ "kind": "not_revealable" }).to_string()
}

/// `path`, as Finder may be asked to show it now: one the newest scan
/// resolved (`Revealable`), looked up again one step at a time and never
/// into a place `protected` keeps out (`protected::resolve`), and still
/// leading, with no link anywhere on its way, to the very file the scan
/// found there. Refused as `not_revealable` otherwise: a path the scan did
/// not resolve, before anything is read; one that is gone, that is or
/// leads into or through a protected place, that has a link on its way
/// now, or that names another file than the scan found.
pub(crate) fn still_found(
    revealable: &Revealable,
    path: &Path,
    protected: &Protected,
) -> Result<PathBuf, String> {
    let Some(seen) = revealable.seen(path) else {
        return Err(not_revealable_json());
    };
    match protected::resolve(path, protected, true) {
        // Every name on the way looked at and none of them a link (any
        // link would have been followed elsewhere), and the file at the
        // end the one the scan found.
        Resolution::Found(now, stat) if now == path && stat.same_as(&seen) => Ok(now),
        _ => Err(not_revealable_json()),
    }
}

/// `reveal_in_finder`'s whole effect: `reveal` asked of `path` when it is
/// still the file the newest scan found there (`still_found`), and nothing
/// at all otherwise. `reveal` is a parameter so a test never opens Finder.
pub(crate) fn reveal_impl(
    revealable: &Revealable,
    path: &Path,
    protected: &Protected,
    reveal: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let path = still_found(revealable, path, protected)?;
    reveal(&path).map_err(|detail| {
        serde_json::json!({ "kind": "reveal_failed", "detail": detail }).to_string()
    })
}

/// Finder, on `path`'s folder with `path` selected: one call to AppKit,
/// `NSWorkspace activateFileViewerSelectingURLs:`, with a file URL of the
/// path as it is (`fileURLWithPath:isDirectory:`, told it is a file -- the
/// scan lists no folder -- so Foundation does not look at it to ask).
/// Nothing runs and nothing resolves it first. A path that is not UTF-8
/// is not changed to fit: refused.
///
/// In a pool of its own (`autoreleasepool`), drained when the call
/// returns: the command runs on one of the async runtime's threads, which
/// has none, and whatever Foundation or AppKit autoreleases would
/// otherwise be kept until the thread ends -- a little more with each
/// click.
#[cfg(target_os = "macos")]
fn show_in_finder(path: &Path) -> Result<(), String> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSArray, NSString, NSURL};
    let path = path
        .to_str()
        .ok_or_else(|| "the path is not UTF-8".to_string())?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(path), false);
        let urls = NSArray::from_retained_slice(&[url]);
        NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&urls);
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn show_in_finder(_path: &Path) -> Result<(), String> {
    Err("Finder is only on a Mac".to_string())
}

/// Has Finder show `path` -- a window on its folder, with it selected --
/// when it is still the file the newest scan found there (`still_found`,
/// against the places kept out for the home folder the scan is given,
/// `HostEnv::discover`'s), through AppKit (`show_in_finder`), and runs
/// nothing. Any other path is refused as `not_revealable`; one the newest
/// scan did not resolve before anything is read.
#[tauri::command]
pub async fn reveal_in_finder(
    revealable: State<'_, Revealable>,
    path: PathBuf,
) -> Result<(), String> {
    let protected = Protected::new(&HostEnv::discover().home);
    reveal_impl(&revealable, &path, &protected, show_in_finder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const REFUSED: &str = r#"{"kind":"not_revealable"}"#;

    /// A folder of a test's own, standing in for `/`: a home folder at
    /// `home`. Removed when dropped. Canonical, as a scan's paths are
    /// (`/var` is a link).
    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Temp {
            let dir = std::env::temp_dir().join(format!(
                "banager-reveal-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Temp(std::fs::canonicalize(&dir).unwrap())
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }

        /// An executable file at `relative`, its folders made.
        fn program(&self, relative: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// A link at `relative` to `target`, its folders made.
        fn link(&self, relative: &str, target: &Path) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, &path).unwrap();
            path
        }

        /// The places kept out for its home folder (and the account's).
        fn protected(&self) -> Protected {
            Protected::new(&self.path("home"))
        }

        /// The scan of `bin` alone, for its home folder, as
        /// `ipc::scan_unknown` hands the window one.
        fn scan(&self, bin: &str) -> UnknownScan {
            use std::os::unix::fs::MetadataExt;
            let home = self.path("home");
            std::fs::create_dir_all(&home).unwrap();
            let env = HostEnv {
                path_dirs: Vec::new(),
                home: home.clone(),
                euid: std::fs::metadata(&home).unwrap().uid(),
                cargo_home: None,
                rustup_home: None,
                zdotdir: None,
                ollama_host: None,
            };
            banager_core::scan::scan_dirs(
                &[self.path(bin)],
                &env,
                &[],
                &[],
                &[],
                banager_core::scan::ScanBudget::default(),
            )
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_only_a_path_the_newest_scan_resolved_is_shown_in_finder() {
        let temp = Temp::new("only");
        let tool = temp.program("home/bin/tool");
        let helper = temp.program("Applications/Helper.app/helper");
        temp.link("home/bin/helper", &helper);
        temp.link("home/bin/broken", Path::new("/nonexistent/broken"));
        temp.program("home/Documents/secret");
        let protected = temp.protected();
        let revealable = Revealable::default();
        let shown = RefCell::new(Vec::new());
        let reveal = |path: &Path| {
            shown.borrow_mut().push(path.to_path_buf());
            Ok(())
        };
        // Before any scan, nothing.
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );

        let scan = temp.scan("home/bin");
        assert_eq!(scan.entries.len(), 3, "{scan:?}");
        revealable.remember(&scan);
        assert_eq!(reveal_impl(&revealable, &tool, &protected, reveal), Ok(()));
        assert_eq!(
            reveal_impl(&revealable, &helper, &protected, reveal),
            Ok(())
        );
        // What the row shows rather than what the scan resolved, a broken
        // link's own path, a folder holding a program, a path dressed up to
        // lead into a protected place, a file in one, and any other place
        // on the Mac: refused.
        for refused in [
            temp.path("home/bin/helper"),
            temp.path("home/bin/broken"),
            temp.path("home/bin"),
            temp.path("home/bin/tool/../../Documents/secret"),
            temp.path("home/Documents/secret"),
            PathBuf::from("/bin/sh"),
            PathBuf::new(),
        ] {
            assert_eq!(
                reveal_impl(&revealable, &refused, &protected, reveal),
                Err(REFUSED.to_string()),
                "{refused:?}"
            );
        }
        assert_eq!(*shown.borrow(), [tool.clone(), helper.clone()]);

        // A newer scan without it: no longer shown.
        let other = temp.program("home/other/other");
        revealable.remember(&temp.scan("home/other"));
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );
        assert_eq!(reveal_impl(&revealable, &other, &protected, reveal), Ok(()));
    }

    #[test]
    fn test_a_program_replaced_after_the_scan_is_not_shown_in_finder() {
        // The scan finds a program in `~/bin`; then the program -- or the
        // folder it is in -- is replaced by a link into `~/Documents`. The
        // path the window still shows is no longer the file the scan
        // found, and leads into a place Banager never looks into: Finder
        // is not asked, and nothing there is looked at.
        let temp = Temp::new("swap");
        let tool = temp.program("home/bin/tool");
        temp.program("home/Documents/private/tool");
        let scan = temp.scan("home/bin");
        assert_eq!(scan.entries.len(), 1, "{scan:?}");
        assert_eq!(scan.entries[0].resolved.as_deref(), Some(tool.as_path()));
        let protected = temp.protected();
        let revealable = Revealable::default();
        revealable.remember(&scan);
        let shown = RefCell::new(Vec::new());
        let reveal = |path: &Path| {
            shown.borrow_mut().push(path.to_path_buf());
            Ok(())
        };
        assert_eq!(reveal_impl(&revealable, &tool, &protected, reveal), Ok(()));

        // The program, replaced by a link into `~/Documents`.
        std::fs::remove_file(&tool).unwrap();
        temp.link("home/bin/tool", &temp.path("home/Documents/private/tool"));
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );
        // The folder it is in, replaced by a link into `~/Documents`.
        std::fs::remove_file(&tool).unwrap();
        std::fs::remove_dir(temp.path("home/bin")).unwrap();
        temp.link("home/bin", &temp.path("home/Documents/private"));
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );
        assert_eq!(*shown.borrow(), [tool]);
    }

    #[test]
    fn test_a_program_shown_is_the_file_the_scan_found_by_the_path_it_found() {
        let temp = Temp::new("same");
        let tool = temp.program("home/bin/tool");
        let protected = temp.protected();
        let revealable = Revealable::default();
        revealable.remember(&temp.scan("home/bin"));
        let shown = RefCell::new(Vec::new());
        let reveal = |path: &Path| {
            shown.borrow_mut().push(path.to_path_buf());
            Ok(())
        };
        // Its folder moved, and a link left in its place: the very file,
        // outside every protected place, but by a link now -- refused,
        // until a scan finds it again.
        std::fs::rename(temp.path("home/bin"), temp.path("home/moved")).unwrap();
        temp.link("home/bin", &temp.path("home/moved"));
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );
        // Moved back: the path and the file the scan found, so shown.
        std::fs::remove_file(temp.path("home/bin")).unwrap();
        std::fs::rename(temp.path("home/moved"), temp.path("home/bin")).unwrap();
        assert_eq!(reveal_impl(&revealable, &tool, &protected, reveal), Ok(()));
        // Another program put there under the same name: not the file the
        // scan found.
        std::fs::remove_file(&tool).unwrap();
        temp.program("home/bin/tool");
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );
        // Gone: refused.
        std::fs::remove_file(&tool).unwrap();
        assert_eq!(
            reveal_impl(&revealable, &tool, &protected, reveal),
            Err(REFUSED.to_string())
        );
        assert_eq!(*shown.borrow(), [tool]);
    }

    #[test]
    fn test_a_scan_read_back_from_json_shows_nothing() {
        // Which file the scan found is never sent to the window, so a scan
        // that went through JSON -- as the window's copy has -- names none,
        // and nothing in it is shown.
        let temp = Temp::new("json");
        let tool = temp.program("home/bin/tool");
        let scan = temp.scan("home/bin");
        let json = serde_json::to_string(&scan).unwrap();
        let back: UnknownScan = serde_json::from_str(&json).unwrap();
        let revealable = Revealable::default();
        revealable.remember(&back);
        assert_eq!(
            reveal_impl(&revealable, &tool, &temp.protected(), |_| Ok(())),
            Err(REFUSED.to_string())
        );
        revealable.remember(&scan);
        assert_eq!(
            reveal_impl(&revealable, &tool, &temp.protected(), |_| Ok(())),
            Ok(())
        );
    }

    #[test]
    fn test_a_finder_failure_comes_back_in_the_envelope() {
        let temp = Temp::new("failure");
        let tool = temp.program("home/bin/tool");
        let revealable = Revealable::default();
        revealable.remember(&temp.scan("home/bin"));
        let err = reveal_impl(&revealable, &tool, &temp.protected(), |_| {
            Err("the path is not UTF-8".to_string())
        })
        .unwrap_err();
        let v: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!(v["kind"], "reveal_failed");
        assert_eq!(v["detail"], "the path is not UTF-8");
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
