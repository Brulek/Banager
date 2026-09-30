//! The icon Finder shows for the app a Homebrew cask installed, for that
//! cask's row in the window. Read-only, and read through macOS itself:
//! `RealIconRenderer` asks `NSWorkspace iconForFile:` for the icon and has
//! AppKit draw it and encode it as PNG. Nothing here runs a command, opens
//! a file, writes anything or touches the network (docs/what-we-run.md,
//! "App icons").
//!
//! Which folder's icon is drawn is decided on this side, never by the
//! window. The window sends an `ArtifactKey` and nothing else
//! (`ipc::artifact_icon` in src-tauri); `Session::artifact_icon`
//! (session/icon.rs) compares that key, whole, with the rows of the current
//! snapshot, and hands the one it matches to `cask_app_bundle`, which
//! accepts only a cask whose `path` -- the `.app` Homebrew reported moving
//! it to (`parse_info_installed` in adapters/brew/parse.rs) -- is absolute
//! and ends in `.app`. `AppIcons::bundle_icon` then draws it only while
//! that path is a folder, not a link to one. `bundle_icon` takes a path,
//! and is `pub(crate)`: from outside this crate, `AppIcons` draws only
//! through `Session::artifact_icon`, which takes a key. (A renderer's
//! `render_png` takes a path too; `bundle_icon` is its only caller.)
//!
//! `IconRenderer` is the seam, like `Trasher` and `CommandRunner`:
//! `RealIconRenderer` is AppKit, and `MockIconRenderer` -- compiled only
//! for tests, like `trash::MockTrasher` -- records what it is asked to draw
//! and draws nothing, so everything here but the drawing itself is tested
//! without it.

use crate::model::{ArtifactKind, InstalledArtifact};
use base64::Engine as _;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod real;

#[cfg(any(test, feature = "test-support"))]
pub use mock::MockIconRenderer;
pub use real::RealIconRenderer;

/// The side of the square an icon is drawn in, in pixels: sharp at up to
/// 64 points on a Retina (2x) screen. docs/what-we-run.md states it
/// (`what_we_run_test.rs` checks that).
pub const ICON_PIXELS: u32 = 128;

/// Draws an app's icon. Implemented by `RealIconRenderer` (AppKit) and, in
/// tests, `MockIconRenderer`; called only by `AppIcons::bundle_icon`, one
/// call at a time.
pub trait IconRenderer: Send + Sync {
    /// The icon Finder shows for the app at `bundle`, drawn `ICON_PIXELS`
    /// pixels square and encoded as PNG; `None` when the system gives none.
    /// `bundle` has passed every check in this module: an absolute path
    /// ending in `.app` that the snapshot holds for a cask, and a folder,
    /// not a link, a moment before this call.
    fn render_png(&self, bundle: &Path) -> Option<Vec<u8>>;
}

/// The `.app` whose icon `artifact`'s row shows, or `None` for every row
/// that shows none: anything but a cask; a cask with no `path` (a font, a
/// `pkg`, one whose `brew info` entry names no `app` target); and a `path`
/// that is not absolute or does not end in `.app`, which Homebrew does not
/// write for an `app` stanza. Looks at the value alone, not at the disk:
/// whether the folder is there is `AppIcons::bundle_icon`'s question.
pub(crate) fn cask_app_bundle(artifact: &InstalledArtifact) -> Option<&Path> {
    if artifact.key.kind != ArtifactKind::Cask {
        return None;
    }
    let path = artifact.path.as_deref()?;
    (path.is_absolute() && path.extension().is_some_and(|ext| ext == "app")).then_some(path)
}

/// What a folder looked like when its icon was drawn: its modification
/// time, and the device and inode it is, so that a folder replaced by
/// another -- `brew upgrade` moves the new version's `.app` into place --
/// counts as new even when the two carry the same time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    dev: u64,
    ino: u64,
}

/// A drawn icon, or the answer that there is none, for one folder as it
/// was (`stamp`).
struct Cached {
    stamp: Stamp,
    icon: Option<Arc<str>>,
}

/// Every cask icon drawn since Banager started, kept in memory only: one
/// entry per `.app` path, each for the folder as it was when drawn (its
/// modification time, device and inode). A row asking again for a folder
/// that has not changed gets the icon already drawn, so a list of fifty
/// casks draws each once, not on every look. An entry is replaced when its
/// folder changes; entries are never removed, and there is one per path a
/// cask has had since launch, a few dozen on a Mac with many casks. A
/// system that gave no icon is remembered as that answer too, until the
/// folder changes. Nothing is written to disk: the next launch draws
/// afresh.
///
/// One per app: the Tauri shell manages it beside `AppState` and hands it
/// to `Session::artifact_icon` on every call.
pub struct AppIcons {
    renderer: Arc<dyn IconRenderer>,
    cache: Mutex<HashMap<PathBuf, Cached>>,
    /// Held for each drawing, so there is one at a time however many rows
    /// ask at once, and a second request for the same folder waits for the
    /// first's icon rather than drawing it again.
    render_gate: Mutex<()>,
}

impl AppIcons {
    pub fn new(renderer: Arc<dyn IconRenderer>) -> AppIcons {
        AppIcons {
            renderer,
            cache: Mutex::new(HashMap::new()),
            render_gate: Mutex::new(()),
        }
    }

    /// `AppIcons` drawing with macOS's own icons (`RealIconRenderer`): the
    /// one the app uses.
    pub fn real() -> AppIcons {
        AppIcons::new(Arc::new(RealIconRenderer::new()))
    }

    /// `bundle`'s icon as a `data:image/png;base64,...` URL, drawn now or
    /// earlier; `None` when `bundle` is not a folder -- missing, a file, or
    /// a symbolic link, which is never followed -- or when the system gave
    /// no icon for it. `bundle` is a path `cask_app_bundle` accepted from
    /// the current snapshot: `pub(crate)`, so nothing outside this crate
    /// can hand it another.
    ///
    /// Blocking: an `lstat`, and on a miss a drawing. The folder is looked
    /// at once, here, and its icon is then read by macOS from the same
    /// path; a folder swapped for another in that instant would have the
    /// newcomer's icon remembered under the old one's stamp, and drawn
    /// again at the next request, which finds a stamp that does not match.
    pub(crate) fn bundle_icon(&self, bundle: &Path) -> Option<String> {
        let stamp = folder_stamp(bundle)?;
        if let Some(known) = self.cached(bundle, stamp) {
            return known;
        }
        // A poisoned gate guards nothing but the one-at-a-time rule: a
        // renderer that panicked leaves no state behind to distrust.
        let _gate = self
            .render_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Drawn by the request this one waited for, perhaps.
        if let Some(known) = self.cached(bundle, stamp) {
            return known;
        }
        let icon: Option<Arc<str>> = self
            .renderer
            .render_png(bundle)
            .filter(|png| !png.is_empty())
            .map(|png| Arc::from(data_url(&png)));
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                bundle.to_path_buf(),
                Cached {
                    stamp,
                    icon: icon.clone(),
                },
            );
        icon.map(|icon| icon.to_string())
    }

    /// What is remembered for `bundle` as it is now (`stamp`): `None` when
    /// nothing is, or only something for the folder as it was before;
    /// `Some(None)` when the system gave no icon for it.
    fn cached(&self, bundle: &Path, stamp: Stamp) -> Option<Option<String>> {
        let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let known = cache.get(bundle).filter(|known| known.stamp == stamp)?;
        Some(known.icon.as_deref().map(str::to_string))
    }
}

/// `path`'s stamp when it is a folder -- `lstat`, so a symbolic link is
/// never followed and never counts as one -- and `None` otherwise.
fn folder_stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_dir() {
        return None;
    }
    Some(Stamp {
        modified: meta.modified().ok(),
        dev: meta.dev(),
        ino: meta.ino(),
    })
}

fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtifactKey, InstallReason};

    /// A fresh, canonical folder under the system temp dir, removed when
    /// the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let raw = std::env::temp_dir().join(format!(
                "banager-icon-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&raw).expect("create temp dir");
            TempDir(std::fs::canonicalize(&raw).expect("canonical temp dir"))
        }

        /// A folder named `name` in it, like the `.app` Homebrew moves
        /// into `/Applications`.
        fn folder(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("create folder");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn artifact(kind: ArtifactKind, path: Option<&Path>) -> InstalledArtifact {
        InstalledArtifact {
            key: ArtifactKey {
                instance_id: "brew:/opt/homebrew".to_string(),
                kind,
                name: "iterm2".to_string(),
            },
            display_name: "iTerm2".to_string(),
            version: "3.6.4".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: path.map(Path::to_path_buf),
            auto_updates: false,
            uninstall_blocked: None,
        }
    }

    fn icons(renderer: &Arc<MockIconRenderer>) -> AppIcons {
        AppIcons::new(renderer.clone())
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnot really an image";

    #[test]
    fn test_cask_app_bundle_accepts_an_absolute_app_path_of_a_cask() {
        let row = artifact(
            ArtifactKind::Cask,
            Some(Path::new("/Applications/iTerm.app")),
        );
        assert_eq!(
            cask_app_bundle(&row),
            Some(Path::new("/Applications/iTerm.app"))
        );
        // A name with spaces and dots is still one `.app`.
        let row = artifact(
            ArtifactKind::Cask,
            Some(Path::new("/Applications/Visual Studio Code.app")),
        );
        assert!(cask_app_bundle(&row).is_some());
    }

    #[test]
    fn test_cask_app_bundle_refuses_every_row_but_a_cask() {
        let app = Some(Path::new("/Applications/iTerm.app"));
        for kind in [
            ArtifactKind::Formula,
            ArtifactKind::Package,
            ArtifactKind::Tool,
            ArtifactKind::Model,
            ArtifactKind::Binary,
        ] {
            assert_eq!(cask_app_bundle(&artifact(kind, app)), None, "{kind:?}");
        }
    }

    #[test]
    fn test_cask_app_bundle_refuses_a_cask_with_no_path() {
        assert_eq!(cask_app_bundle(&artifact(ArtifactKind::Cask, None)), None);
    }

    #[test]
    fn test_cask_app_bundle_refuses_a_relative_path_and_one_not_ending_in_app() {
        for path in [
            "iTerm.app",
            "Applications/iTerm.app",
            "./iTerm.app",
            "/Applications/iTerm",
            "/Applications/iTerm.app.zip",
            "/Applications/iTerm.APP",
            "/Applications/.app",
            "/usr/local/Caskroom/iterm2/3.6.4",
            "/",
        ] {
            let row = artifact(ArtifactKind::Cask, Some(Path::new(path)));
            assert_eq!(cask_app_bundle(&row), None, "{path}");
        }
    }

    #[test]
    fn test_bundle_icon_draws_a_folder_once_and_answers_a_png_data_url() {
        let dir = TempDir::new("draws");
        let app = dir.folder("iTerm.app");
        let renderer = Arc::new(MockIconRenderer::answering(PNG));
        let icons = icons(&renderer);

        let first = icons.bundle_icon(&app).expect("an icon");
        assert_eq!(
            first,
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(PNG)
            )
        );
        // Fifty rows looking again: drawn once.
        for _ in 0..50 {
            assert_eq!(icons.bundle_icon(&app).as_deref(), Some(first.as_str()));
        }
        assert_eq!(renderer.calls(), vec![app]);
    }

    #[test]
    fn test_bundle_icon_draws_again_once_the_folder_changes() {
        let dir = TempDir::new("changes");
        let app = dir.folder("iTerm.app");
        let renderer = Arc::new(MockIconRenderer::answering(PNG));
        let icons = icons(&renderer);
        assert!(icons.bundle_icon(&app).is_some());

        // The same folder, modified.
        let later = SystemTime::now() + std::time::Duration::from_secs(3600);
        let set_modified = |path: &Path| {
            std::fs::File::open(path)
                .expect("open the folder")
                .set_modified(later)
                .expect("set its modification time");
        };
        set_modified(&app);
        assert!(icons.bundle_icon(&app).is_some());
        assert_eq!(renderer.calls().len(), 2, "a changed folder is drawn again");

        // Another folder moved into its place, as `brew upgrade` moves the
        // new version's `.app`, carrying the very same time: made while
        // the first still exists, so it is another inode for certain.
        let incoming = dir.folder("incoming.app");
        std::fs::remove_dir(&app).expect("remove the old folder");
        std::fs::rename(&incoming, &app).expect("move the new one into place");
        set_modified(&app);
        assert!(icons.bundle_icon(&app).is_some());
        assert_eq!(
            renderer.calls().len(),
            3,
            "a replaced folder is drawn again"
        );
        assert!(icons.bundle_icon(&app).is_some());
        assert_eq!(renderer.calls().len(), 3, "and then remembered");
    }

    #[test]
    fn test_bundle_icon_refuses_a_missing_path_a_file_and_a_link_without_drawing() {
        let dir = TempDir::new("refuses");
        let renderer = Arc::new(MockIconRenderer::answering(PNG));
        let icons = icons(&renderer);

        let missing = dir.0.join("Gone.app");
        assert_eq!(icons.bundle_icon(&missing), None);

        let file = dir.0.join("File.app");
        std::fs::write(&file, b"not a bundle").expect("write file");
        assert_eq!(icons.bundle_icon(&file), None);

        // A link to a real folder: never followed.
        let target = dir.folder("Real.app");
        let link = dir.0.join("Link.app");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        assert_eq!(icons.bundle_icon(&link), None);

        assert!(renderer.calls().is_empty(), "{:?}", renderer.calls());
    }

    #[test]
    fn test_bundle_icon_remembers_that_the_system_gave_no_icon() {
        let dir = TempDir::new("none");
        let app = dir.folder("Blank.app");
        let renderer = Arc::new(MockIconRenderer::answering_nothing());
        let icons = icons(&renderer);
        assert_eq!(icons.bundle_icon(&app), None);
        assert_eq!(icons.bundle_icon(&app), None);
        assert_eq!(renderer.calls().len(), 1, "no icon is an answer too");
    }

    #[test]
    fn test_bundle_icon_treats_an_empty_png_as_no_icon() {
        let dir = TempDir::new("empty");
        let app = dir.folder("Empty.app");
        let renderer = Arc::new(MockIconRenderer::answering(b""));
        assert_eq!(icons(&renderer).bundle_icon(&app), None);
    }

    #[test]
    fn test_bundle_icon_draws_one_at_a_time_and_each_folder_once_when_asked_at_once() {
        let dir = TempDir::new("at-once");
        let apps: Vec<PathBuf> = (0..4).map(|i| dir.folder(&format!("App{i}.app"))).collect();
        let renderer = Arc::new(MockIconRenderer::answering(PNG));
        let icons = Arc::new(icons(&renderer));
        let threads: Vec<_> = (0..32)
            .map(|i| {
                let icons = icons.clone();
                let app = apps[i % apps.len()].clone();
                std::thread::spawn(move || icons.bundle_icon(&app))
            })
            .collect();
        for thread in threads {
            assert!(thread.join().expect("no panic").is_some());
        }
        let mut drawn = renderer.calls();
        drawn.sort();
        assert_eq!(drawn, apps, "each folder drawn exactly once");
        assert_eq!(renderer.most_at_once(), 1, "one drawing at a time");
    }
}
