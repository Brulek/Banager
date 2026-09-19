use crate::events::ChannelSink;
use canager_core::session::Session;
use canager_core::settings::{self, Settings};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub session: std::sync::Arc<Session>,
    pub settings_path: PathBuf,
    pub settings: Mutex<Settings>,
    pub channel_sink: std::sync::Arc<ChannelSink>,
    /// The last `Snapshot::generation` this process has ever broadcast as a
    /// `SnapshotChanged` event (Task 13). `refresh_impl` compares against
    /// this with a compare-and-swap instead of each call's own "before"
    /// reading of `session.snapshot()`, so that when two `refresh_impl`
    /// calls coalesce inside `Session::refresh` and both receive the same
    /// resulting Snapshot, only one of them ever wins the swap and
    /// broadcasts -- never both.
    pub last_broadcast_generation: std::sync::atomic::AtomicU64,
}

impl AppState {
    /// Loads settings from `settings_path` (falling back to defaults per
    /// `canager_core::settings::load`'s contract) and builds a `Session`
    /// wired to `channel_sink` as its event sink.
    pub fn new(settings_path: PathBuf, channel_sink: std::sync::Arc<ChannelSink>) -> AppState {
        let loaded = settings::load(&settings_path);
        let session = Session::new(channel_sink.clone(), None);
        AppState {
            session,
            settings_path,
            settings: Mutex::new(loaded),
            channel_sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Persists `new_settings` to disk, then updates the in-memory copy —
    /// holding `settings`'s lock across *both*, not just the final
    /// assignment (M7 in the design review). Without this, two overlapping
    /// calls could each save to disk unlocked and then briefly lock memory
    /// only for the assignment, letting them interleave into "disk holds
    /// caller B's settings, memory holds caller A's": e.g. A saves, pauses;
    /// B saves (disk now B) and updates memory (memory now B); A resumes
    /// and updates memory (memory now A) — disk and memory now disagree
    /// even though both calls "succeeded". Holding the lock for the whole
    /// method serialises the two callers instead, so whichever one's write
    /// actually lands on disk last is also the one left in memory. This
    /// stays synchronous throughout (no `.await` inside), so holding a
    /// `std::sync::Mutex` guard across it is safe.
    pub fn set_settings(&self, new_settings: Settings) -> std::io::Result<()> {
        let mut settings = self.settings.lock().unwrap();
        settings::save(&self.settings_path, &new_settings)?;
        *settings = new_settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canager_core::model::{ArtifactKey, ArtifactKind};
    use std::sync::Arc;

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-appstate-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn test_new_loads_defaults_when_settings_file_is_missing() {
        let path = temp_settings_path("missing");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        assert_eq!(state.get_settings(), Settings::default());
    }

    #[test]
    // Deviation from the brief (recorded in the task report): the brief's
    // verbatim `let mut new_settings = Settings::default(); new_settings
    // .show_technical_details = true;` trips clippy::field_reassign_with_default
    // under this workspace's `-D warnings` gate. `#[allow]` keeps the test's
    // exact shape and intent rather than restructuring it into a struct
    // literal.
    #[allow(clippy::field_reassign_with_default)]
    fn test_set_settings_persists_and_updates_the_in_memory_copy() {
        let path = temp_settings_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path.clone(), ChannelSink::new());
        let mut new_settings = Settings::default();
        new_settings.show_technical_details = true;
        state
            .set_settings(new_settings.clone())
            .expect("set_settings");
        assert_eq!(state.get_settings(), new_settings);
        assert_eq!(settings::load(&path), new_settings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_new_builds_a_working_session() {
        let path = temp_settings_path("session");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        // A freshly built Session has an empty, generation-0 snapshot until
        // something calls refresh() — proves `session` is a real, usable
        // Session rather than left unconstructed.
        assert_eq!(state.session.snapshot().generation, 0);
    }

    #[test]
    fn test_concurrent_set_settings_calls_leave_disk_and_memory_consistent() {
        // Regression guard for M7 in the design review: set_settings used
        // to save to disk unlocked and only briefly lock memory for the
        // final assignment, so two overlapping calls could finish with
        // disk holding one caller's settings and memory holding the
        // other's. The whole save-then-update sequence is now one critical
        // section, so no matter how many threads race here, whichever
        // write actually lands on disk last must also be the one left in
        // memory.
        let path = temp_settings_path("concurrent");
        let _ = std::fs::remove_file(&path);
        let state = Arc::new(AppState::new(path.clone(), ChannelSink::new()));

        let handles: Vec<_> = (0..8u32)
            .map(|i| {
                let state = state.clone();
                std::thread::spawn(move || {
                    let settings = Settings {
                        show_technical_details: i % 2 == 0,
                        ignored_updates: vec![ArtifactKey {
                            instance_id: "brew:/opt/homebrew".to_string(),
                            kind: ArtifactKind::Formula,
                            name: format!("pkg-{i}"),
                        }],
                        ..Settings::default()
                    };
                    state.set_settings(settings).expect("set_settings");
                })
            })
            .collect();
        for h in handles {
            h.join().expect("writer thread panicked");
        }

        let on_disk = settings::load(&path);
        let in_memory = state.get_settings();
        assert_eq!(
            on_disk, in_memory,
            "whichever write actually landed on disk must also be the one left in memory"
        );
        let _ = std::fs::remove_file(&path);
    }
}
