//! `MockIconRenderer`: an icon renderer that draws nothing, for tests only
//! -- `icon/mod.rs` compiles this module under `cfg(test)` or the
//! `test-support` feature, never into a release build.

use super::IconRenderer;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Records every folder it is asked to draw, in order, and answers each
/// with the same bytes, or with none. Counts how many drawings overlapped,
/// so a test can see `AppIcons` draw one at a time. Touches nothing on
/// disk. Read by the tests in icon/mod.rs and session/icon.rs, and by the
/// Tauri shell's `ipc` tests.
pub struct MockIconRenderer {
    png: Option<Vec<u8>>,
    calls: Mutex<Vec<PathBuf>>,
    drawing: AtomicUsize,
    most_at_once: AtomicUsize,
}

impl MockIconRenderer {
    /// Answers every drawing with `png` (not checked to be one).
    pub fn answering(png: &[u8]) -> MockIconRenderer {
        MockIconRenderer::with(Some(png.to_vec()))
    }

    /// Answers every drawing with no icon, as the system may.
    pub fn answering_nothing() -> MockIconRenderer {
        MockIconRenderer::with(None)
    }

    fn with(png: Option<Vec<u8>>) -> MockIconRenderer {
        MockIconRenderer {
            png,
            calls: Mutex::new(Vec::new()),
            drawing: AtomicUsize::new(0),
            most_at_once: AtomicUsize::new(0),
        }
    }

    /// Every folder asked for so far, in order.
    pub fn calls(&self) -> Vec<PathBuf> {
        self.calls.lock().unwrap().clone()
    }

    /// The most drawings that were ever under way at once.
    pub fn most_at_once(&self) -> usize {
        self.most_at_once.load(Ordering::SeqCst)
    }
}

impl IconRenderer for MockIconRenderer {
    fn render_png(&self, bundle: &Path) -> Option<Vec<u8>> {
        let now = self.drawing.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_at_once.fetch_max(now, Ordering::SeqCst);
        // A real drawing takes milliseconds; this one takes a few too, so
        // a caller that did not wait its turn would overlap it.
        std::thread::sleep(Duration::from_millis(5));
        self.calls.lock().unwrap().push(bundle.to_path_buf());
        self.drawing.fetch_sub(1, Ordering::SeqCst);
        self.png.clone()
    }
}
