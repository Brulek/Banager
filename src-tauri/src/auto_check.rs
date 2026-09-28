//! The daily check's task (`Settings::auto_check`, off by default). For the
//! life of the app, at every `canager_core::auto_check::TICK` it asks
//! `canager_core::auto_check::tick` whether to check, and when the answer
//! is `Tick::Check` runs the refresh the window's Check again runs
//! (`ipc::refresh_as`), recorded as `RoundTrigger::Automatic`. That is all
//! it does: the refresh runs what every refresh runs, which installs
//! nothing, and the task ends with the app.

use crate::ipc;
use crate::state::AppState;
use canager_core::auto_check::{self, RoundTrigger, Tick};
use std::time::Duration;

/// Spawned once at startup (`run()` in lib.rs).
pub(crate) async fn check_automatically(state: &AppState) {
    check_every(state, auto_check::TICK, auto_check::wall_clock_now).await
}

/// The body of `check_automatically`, with the tick and the wall clock
/// handed in, so a test can run it on milliseconds and on a clock it sets.
///
/// The first tick comes one `tick` after the start, not at it: the
/// window's check at launch is the day's first, and a tick at launch would
/// only race it. A tick that passes while a round runs is skipped, not
/// made up in a burst after it.
async fn check_every(state: &AppState, tick: Duration, now: fn() -> i64) {
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + tick, tick);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticks.tick().await;
        if tick_at(state, now()) == Tick::Check {
            // Never `Err` (`refresh_as`); a failed round is on screen
            // through the snapshot's own `errors`, like any other.
            let _ = ipc::refresh_as(state, RoundTrigger::Automatic).await;
        }
    }
}

/// What the tick at `now` does: `auto_check::tick` over the end of the
/// last round that counts as a check (`RoundLog::last_check_ended`: a round
/// of any trigger, but a daily one in which every source failed), whether
/// a refresh or an operation is under way (`Session::busy`), and the
/// setting as it is saved now.
fn tick_at(state: &AppState, now: i64) -> Tick {
    let last_check_ended = state.rounds.lock().unwrap().last_check_ended();
    auto_check::tick(
        now,
        last_check_ended,
        state.session.busy(),
        state.get_settings().auto_check,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use async_trait::async_trait;
    use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use canager_core::events::{EventSink, OpId};
    use canager_core::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
        OpRequest, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
    };
    use canager_core::runner::HostEnv;
    use canager_core::session::Session;
    use canager_core::settings::Settings;
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    /// The tick the tests run the task on.
    const TICK: Duration = Duration::from_millis(20);
    const DAY: i64 = auto_check::DUE_AFTER_SECS;
    /// 2026-09-28 09:00 UTC.
    const T0: i64 = 1_790_586_000;

    /// One source, answering at once. Counts its rounds by its `detect`,
    /// which every round calls once; says its catalogue is being rewritten
    /// -- a `brew update` Canager started still running -- while
    /// `index_updating` is set; fails to read its packages while `failing`
    /// is; and holds every operation until `release`.
    struct Fake {
        meta: AdapterMeta,
        rounds: AtomicUsize,
        index_updating: AtomicBool,
        failing: AtomicBool,
        release: tokio::sync::Notify,
    }

    impl Fake {
        fn new() -> Arc<Fake> {
            Arc::new(Fake {
                meta: AdapterMeta {
                    id: "fake".to_string(),
                    name: "fake".to_string(),
                    kind: "fake".to_string(),
                    platforms: vec!["macos".to_string()],
                    homepage: "https://example.invalid".to_string(),
                    schema_version: 1,
                    verified_versions: vec![],
                },
                rounds: AtomicUsize::new(0),
                index_updating: AtomicBool::new(false),
                failing: AtomicBool::new(false),
                release: tokio::sync::Notify::new(),
            })
        }

        fn rounds(&self) -> usize {
            self.rounds.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Adapter for Fake {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.rounds.fetch_add(1, Ordering::SeqCst);
            vec![canager_core::testing::manager_instance("fake", "fake:1")]
        }

        async fn inventory(
            &self,
            _inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            if self.index_updating.load(Ordering::SeqCst) {
                return Err(AdapterError::IndexUpdating);
            }
            if self.failing.load(Ordering::SeqCst) {
                return Err(AdapterError::Parse("no network".to_string()));
            }
            Ok(Vec::new())
        }

        async fn check_updates(
            &self,
            _inst: &ManagerInstance,
            _opts: &CheckOptions,
        ) -> Result<CheckOutcome, AdapterError> {
            Ok(CheckOutcome::default())
        }

        async fn search(
            &self,
            _inst: &ManagerInstance,
            _query: &str,
        ) -> Result<Vec<SearchHit>, AdapterError> {
            Ok(Vec::new())
        }

        async fn plan(
            &self,
            inst: &ManagerInstance,
            req: &OpRequest,
        ) -> Result<Plan, AdapterError> {
            Ok(Plan {
                request: req.clone(),
                action: PlanAction::Command {
                    program: inst.exe_path.clone(),
                    args: vec!["install".to_string(), req.name.clone()],
                    env: vec![],
                },
                needs_password: false,
                locks: vec![ResourceLock(inst.id.clone())],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: vec![],
                affected: vec![],
                timeout_secs: 60,
            })
        }

        async fn execute(
            &self,
            _plan: &Plan,
            _sink: Arc<dyn EventSink>,
            _op_id: OpId,
            _cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
            self.release.notified().await;
            Ok(Outcome::Succeeded)
        }

        async fn reconcile(
            &self,
            _inst: &ManagerInstance,
            _key: &ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(Reconciled {
                present: true,
                version: None,
            })
        }
    }

    fn app_state(session: Arc<Session>, sink: Arc<ChannelSink>, auto_check: bool) -> Arc<AppState> {
        Arc::new(AppState {
            session,
            settings_path: std::env::temp_dir().join(format!(
                "canager-auto-check-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )),
            settings: Mutex::new(Settings {
                auto_check,
                ..Settings::default()
            }),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
            rounds: Mutex::new(Default::default()),
            notified: Mutex::new(Default::default()),
        })
    }

    /// A state over `fake` whose session stamps its rounds with `clock`,
    /// the wall clock the task under test is handed too.
    fn state_on(fake: &Arc<Fake>, clock: fn() -> i64, auto_check: bool) -> Arc<AppState> {
        let sink = ChannelSink::new();
        let adapter: Arc<dyn Adapter> = fake.clone();
        let session = Session::with_adapters(sink.clone(), vec![adapter], Some(clock));
        app_state(session, sink, auto_check)
    }

    fn start(
        state: &Arc<AppState>,
        tick: Duration,
        clock: fn() -> i64,
    ) -> tokio::task::JoinHandle<()> {
        let state = state.clone();
        tokio::spawn(async move { check_every(&state, tick, clock).await })
    }

    async fn wait_for_rounds(fake: &Fake, rounds: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.rounds() < rounds {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{rounds} rounds never ran: {}", fake.rounds()));
    }

    /// Who asked for round `round`, once `refresh_as` has recorded it --
    /// after the round's snapshot is in, a moment after its `detect`.
    async fn recorded(state: &AppState, round: u64) -> RoundTrigger {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(trigger) = state.rounds.lock().unwrap().trigger_of(round) {
                    return trigger;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("round {round} was never recorded"))
    }

    // One wall clock per test: the tests run in parallel, and a session's
    // clock is a plain `fn`.
    static CLOCK_DUE: AtomicI64 = AtomicI64::new(T0);
    fn clock_due() -> i64 {
        CLOCK_DUE.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_a_day_after_the_last_round_ended_the_task_runs_one_round_recorded_as_automatic() {
        let fake = Fake::new();
        let state = state_on(&fake, clock_due, true);
        // The window's check at launch, ending at T0.
        ipc::refresh_impl(&state).await.expect("refresh");
        let task = start(&state, TICK, clock_due);

        tokio::time::sleep(TICK * 6).await;
        assert_eq!(fake.rounds(), 1, "within the day, no tick checks");

        CLOCK_DUE.store(T0 + DAY, Ordering::SeqCst);
        wait_for_rounds(&fake, 2).await;
        assert_eq!(recorded(&state, 2).await, RoundTrigger::Automatic);
        assert_eq!(recorded(&state, 1).await, RoundTrigger::Window);
        tokio::time::sleep(TICK * 6).await;
        assert_eq!(
            fake.rounds(),
            2,
            "one round: it stamps its end, and the next is a day after that"
        );
        assert_eq!(state.session.snapshot().refreshed_at, Some(T0 + DAY));
        task.abort();
    }

    static CLOCK_OFFLINE: AtomicI64 = AtomicI64::new(T0);
    fn clock_offline() -> i64 {
        CLOCK_OFFLINE.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_a_daily_round_in_which_every_source_failed_is_run_again_at_the_next_tick() {
        let fake = Fake::new();
        let state = state_on(&fake, clock_offline, true);
        // The window's check at launch, ending at T0.
        ipc::refresh_impl(&state).await.expect("refresh");
        let task = start(&state, TICK, clock_offline);

        // Two days on, the Mac wakes before its network: the daily check's
        // round fails for its one source, and does not count.
        fake.failing.store(true, Ordering::SeqCst);
        CLOCK_OFFLINE.store(T0 + 2 * DAY, Ordering::SeqCst);
        wait_for_rounds(&fake, 3).await;
        assert_eq!(recorded(&state, 2).await, RoundTrigger::Automatic);
        assert_eq!(recorded(&state, 3).await, RoundTrigger::Automatic);
        assert!(
            !state.session.snapshot().errors.is_empty(),
            "precondition: the source failed"
        );
        assert_eq!(state.rounds.lock().unwrap().last_check_ended(), Some(T0));

        // The network is back: the next round counts, and is the last.
        fake.failing.store(false, Ordering::SeqCst);
        let answered = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if state.rounds.lock().unwrap().last_check_ended() == Some(T0 + 2 * DAY) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(answered.is_ok(), "a round that reached the source counted");
        let after = fake.rounds();
        tokio::time::sleep(TICK * 6).await;
        assert_eq!(
            fake.rounds(),
            after,
            "a counted check: no more rounds today"
        );
        task.abort();
    }

    static CLOCK_OFF: AtomicI64 = AtomicI64::new(T0);
    fn clock_off() -> i64 {
        CLOCK_OFF.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_the_task_checks_nothing_while_the_setting_is_off_and_reads_it_at_every_tick() {
        let fake = Fake::new();
        let state = state_on(&fake, clock_off, false);
        // Nothing has ever been checked in this run: due, if it were on.
        let task = start(&state, TICK, clock_off);
        tokio::time::sleep(TICK * 8).await;
        assert_eq!(fake.rounds(), 0, "off: no round, however due");

        // Turned on, as the Settings page saves it: the next tick checks.
        state
            .set_settings(Settings {
                auto_check: true,
                ..Settings::default()
            })
            .expect("save settings");
        wait_for_rounds(&fake, 1).await;
        assert_eq!(recorded(&state, 1).await, RoundTrigger::Automatic);
        task.abort();
        let _ = std::fs::remove_file(&state.settings_path);
    }

    static CLOCK_FIRST: AtomicI64 = AtomicI64::new(T0);
    fn clock_first() -> i64 {
        CLOCK_FIRST.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_the_first_tick_is_one_tick_after_the_start_not_at_it() {
        // Due from the start (no round yet), so only the tick's timing
        // decides when the first round runs: not before the window's own
        // check at launch has had its tick's worth of time.
        let fake = Fake::new();
        let state = state_on(&fake, clock_first, true);
        let tick = Duration::from_millis(400);
        let task = start(&state, tick, clock_first);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(fake.rounds(), 0, "no tick at the start");
        wait_for_rounds(&fake, 1).await;
        task.abort();
    }

    static CLOCK_BUSY: AtomicI64 = AtomicI64::new(T0);
    fn clock_busy() -> i64 {
        CLOCK_BUSY.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_a_due_check_waits_for_an_operation_and_runs_at_the_first_tick_after_it() {
        let fake = Fake::new();
        let state = state_on(&fake, clock_busy, true);
        ipc::refresh_impl(&state).await.expect("refresh");
        let issued = state
            .session
            .issue_plan(&OpRequest {
                kind: OpKind::Install,
                instance_id: "fake:1".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".to_string(),
            })
            .await
            .expect("issue_plan");
        state.session.submit(issued.id).expect("submit");
        assert!(
            state.session.busy(),
            "precondition: the operation is under way"
        );

        CLOCK_BUSY.store(T0 + 2 * DAY, Ordering::SeqCst);
        let task = start(&state, TICK, clock_busy);
        tokio::time::sleep(TICK * 8).await;
        assert_eq!(fake.rounds(), 1, "due, but busy: every tick skipped");

        fake.release.notify_one();
        wait_for_rounds(&fake, 2).await;
        assert_eq!(recorded(&state, 2).await, RoundTrigger::Automatic);
        task.abort();
    }

    #[tokio::test]
    async fn test_the_refresh_a_finished_brew_update_sets_off_belongs_to_the_round_that_started_it()
    {
        let fake = Fake::new();
        let background_change = Arc::new(tokio::sync::Notify::new());
        let sink = ChannelSink::new();
        let adapter: Arc<dyn Adapter> = fake.clone();
        let session = canager_core::testing::session_with_background_change(
            sink.clone(),
            vec![adapter],
            background_change.clone(),
        );
        let state = app_state(session, sink, true);
        let follow_ups = {
            let state = state.clone();
            tokio::spawn(async move { ipc::refresh_on_background_change(&state).await })
        };

        // The daily check's round leaves its `brew update` running.
        fake.index_updating.store(true, Ordering::SeqCst);
        ipc::refresh_as(&state, RoundTrigger::Automatic)
            .await
            .expect("refresh");
        // It ends; the refresh it sets off is the daily check's.
        fake.index_updating.store(false, Ordering::SeqCst);
        background_change.notify_one();
        wait_for_rounds(&fake, 2).await;
        assert_eq!(recorded(&state, 2).await, RoundTrigger::Automatic);

        // The same for a check of the window's: its follow-up is the window's.
        fake.index_updating.store(true, Ordering::SeqCst);
        ipc::refresh_impl(&state).await.expect("refresh");
        fake.index_updating.store(false, Ordering::SeqCst);
        background_change.notify_one();
        wait_for_rounds(&fake, 4).await;
        assert_eq!(recorded(&state, 3).await, RoundTrigger::Window);
        assert_eq!(recorded(&state, 4).await, RoundTrigger::Window);
        follow_ups.abort();
    }
}
