use crate::events::ChannelSink;
use banager_core::auto_check::RoundLog;
use banager_core::notify_updates::Notified;
use banager_core::runner::login_path::LoginPath;
use banager_core::runner::HostEnv;
use banager_core::session::Session;
use banager_core::settings::{self, Settings};
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
    /// broadcasts -- never both. The one exception is `ipc::announce`'s:
    /// a caller that ran for the daily check broadcasts whether or not it
    /// won.
    pub last_broadcast_generation: std::sync::atomic::AtomicU64,
    /// Who asked for each refresh round: the window or the daily check.
    /// Written by `ipc::refresh_for` for every round before the round's
    /// snapshot is committed, and by each call that shares it as it takes
    /// it (`Session::refresh_recording`) -- for the refresh a finished
    /// `brew update` sets off (`ipc::refresh_on_background_change`), with
    /// whose round started that update read then
    /// (`RoundLog::record_follow_up`), and for the daily check's own, with
    /// the look that started it (`RoundLog::record_daily`); read by
    /// `notify::report` for the round the page reports, and by
    /// `auto_check::tick_at` for when the last round that counts as a check
    /// ended and the daily checks in which every source failed since. In
    /// memory only.
    pub rounds: Mutex<RoundLog>,
    /// The (row, version) pairs this run has told the user about in the
    /// update notification, or that the user saw in the window: what
    /// `notify::report` goes by. In memory only.
    pub notified: Mutex<Notified>,
    /// The login shell's `PATH`, read in the background (`LoginPath`):
    /// set by `run()` as the app starts, which starts the first read; never
    /// in a test, whose refreshes then read nothing (`read_login_path`).
    pub login_path: std::sync::OnceLock<std::sync::Arc<LoginPath>>,
}

impl AppState {
    /// Loads settings from `settings_path` (falling back to defaults per
    /// `banager_core::settings::load`'s contract) and builds a `Session`
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
            rounds: Mutex::new(RoundLog::default()),
            notified: Mutex::new(Notified::default()),
            login_path: std::sync::OnceLock::new(),
        }
    }

    /// Waits for the login shell's `PATH` -- the read the app started as
    /// it opened, or, when the last read failed or ran out of time, one
    /// more (`LoginPath::ensure`) -- and tells the session whether `PATH`
    /// is now the login shell's (`Session::note_login_path`). Every
    /// refresh does this before it looks for sources (`ipc::refresh_for`),
    /// so Check Again reads a shell that failed again. Nothing to wait for
    /// where nothing set one up (a test).
    pub async fn read_login_path(&self) {
        let Some(probe) = self.login_path.get() else {
            return;
        };
        probe.ensure().await;
        // Whether any read has worked, not this call's answer: a caller
        // that waited on a failed read must not take back a later success.
        self.session.note_login_path(probe.is_read());
    }

    /// What a refresh round looks along and whether that `PATH` is the
    /// login shell's, as one value, after `read_login_path`: from one look
    /// at the `PATH` read (`runner::login_path::round_env`), so a read that
    /// works while the round runs changes neither for it
    /// (`Session::refresh_recording_on`). Where nothing set a read up (a
    /// test), the process's own `PATH` and what the session was told.
    pub async fn round_env(&self) -> (HostEnv, bool) {
        self.read_login_path().await;
        if self.login_path.get().is_some() {
            banager_core::runner::login_path::round_env()
        } else {
            (HostEnv::discover(), self.session.login_path_restored())
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
    ///
    /// `welcome_seen` is kept true once it is (`Settings::keep_welcome_seen`):
    /// a page holding settings read before the welcome sheet closed cannot
    /// bring the sheet back by saving over them.
    pub fn set_settings(&self, new_settings: Settings) -> std::io::Result<()> {
        let mut settings = self.settings.lock().unwrap();
        let new_settings = new_settings.keep_welcome_seen(&settings);
        settings::save(&self.settings_path, &new_settings)?;
        *settings = new_settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use banager_core::model::{ArtifactKey, ArtifactKind};
    use std::sync::Arc;

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "banager-appstate-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// Opus review finding 2: a read that ran out of time leaves the
    /// session told so, and the next refresh -- Check Again -- reads again
    /// and tells it the PATH is the login shell's. Through a mock runner:
    /// no shell runs, and the read `PATH` goes nowhere but the test.
    #[tokio::test]
    async fn test_a_refresh_after_a_failed_read_reads_the_login_shell_again() {
        use banager_core::runner::{CommandOutput, MockRunner};
        let state = AppState::new(temp_settings_path("login-path"), ChannelSink::new());
        // No read set up (a test's state): nothing to wait for.
        state.read_login_path().await;
        assert!(state.session.login_path_restored());

        let runner = Arc::new(MockRunner::new());
        let argv = vec![
            "/test-shell",
            "-ilc",
            "echo -n \"_SHELL_ENV_DELIMITER_\"; env; echo -n \"_SHELL_ENV_DELIMITER_\"; exit",
        ];
        runner.respond(
            argv.clone(),
            CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: true,
                cancelled: false,
            },
        );
        let published = Arc::new(Mutex::new(Vec::new()));
        let to = published.clone();
        let _ = state.login_path.set(Arc::new(LoginPath::new(
            runner.clone(),
            "/test-shell".into(),
            "/tmp".into(),
            banager_core::runner::login_path::TIMEOUT,
            move |path| to.lock().unwrap().push(path.to_string()),
        )));
        state.read_login_path().await;
        assert!(!state.session.login_path_restored());
        assert!(published.lock().unwrap().is_empty());
        // A round begun now -- itself a refresh, so it reads once more, and
        // fails again -- goes along the process's own PATH, taken as not
        // the login shell's (Astra's j2 review, finding 2).
        let (_, known) = state.round_env().await;
        assert!(!known);
        assert_eq!(runner.calls().len(), 2);

        runner.respond(
            argv,
            CommandOutput {
                exit_code: Some(0),
                stdout:
                    "_SHELL_ENV_DELIMITER_PATH=/opt/homebrew/bin:/usr/bin\n_SHELL_ENV_DELIMITER_"
                        .to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        state.read_login_path().await;
        assert!(state.session.login_path_restored());
        assert_eq!(*published.lock().unwrap(), ["/opt/homebrew/bin:/usr/bin"]);
        assert_eq!(runner.calls().len(), 3);
        // Read: no shell runs again.
        state.read_login_path().await;
        assert_eq!(runner.calls().len(), 3);
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
    fn test_set_settings_never_turns_welcome_seen_back_off() {
        let path = temp_settings_path("welcome");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path.clone(), ChannelSink::new());
        assert!(
            !state.get_settings().welcome_seen,
            "shown at the first launch"
        );
        state
            .set_settings(Settings {
                welcome_seen: true,
                ..Settings::default()
            })
            .expect("the sheet closes");
        // Settings a page read before the sheet closed, saved with a change.
        state
            .set_settings(Settings {
                show_technical_details: true,
                ..Settings::default()
            })
            .expect("a stale save");
        let saved = settings::load(&path);
        assert!(saved.welcome_seen && saved.show_technical_details);
        assert_eq!(state.get_settings(), saved);
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
