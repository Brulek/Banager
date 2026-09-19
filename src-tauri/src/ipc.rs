use crate::events::UiEvent;
use crate::state::AppState;
use canager_core::adapters::CheckOptions;
use canager_core::model::OpRequest;
use canager_core::ops::OpSummary;
use canager_core::runner::HostEnv;
use canager_core::session::{IssuedPlan, Snapshot};
use canager_core::settings::Settings;
use tauri::ipc::Channel;
use tauri::State;

pub(crate) fn get_snapshot_impl(state: &AppState) -> Result<Snapshot, String> {
    Ok(state.session.snapshot())
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    get_snapshot_impl(&state)
}

/// Also broadcasts `UiEvent::SnapshotChanged` on `state.channel_sink`
/// whenever the refreshed snapshot's `generation` differs from the one
/// before this call (M9 in the design review). `canager-core` must never
/// depend on `tauri`, so `Session::refresh` itself cannot send this — the
/// shell is the only layer that can, and this is the only place in the
/// whole plan that does so outside a test.
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    let generation_before = state.session.snapshot().generation;
    let opts = CheckOptions {
        include_self_updating: state.get_settings().include_self_updating,
    };
    let snapshot = state.session.refresh(&HostEnv::discover(), &opts).await;
    if snapshot.generation != generation_before {
        state.channel_sink.broadcast(UiEvent::SnapshotChanged {
            generation: snapshot.generation,
        });
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn refresh(state: State<'_, AppState>) -> Result<Snapshot, String> {
    refresh_impl(&state).await
}

/// Resolves and plans `request` through `Session::issue_plan`, returning
/// the server-issued `IssuedPlan` for the caller to preview. Nothing in
/// the returned `Plan` is ever accepted back from the client — F1 in the
/// design review: IPC accepts only known operations and server-issued
/// object IDs, never a client-supplied `Plan`. `submit_operation_impl`
/// below is the only way to actually run it, and takes only the id.
pub(crate) async fn plan_operation_impl(
    state: &AppState,
    request: OpRequest,
) -> Result<IssuedPlan, String> {
    state
        .session
        .issue_plan(&request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn plan_operation(
    state: State<'_, AppState>,
    request: OpRequest,
) -> Result<IssuedPlan, String> {
    plan_operation_impl(&state, request).await
}

/// Consumes the plan stored under `plan_id` (one-time use) and submits
/// exactly that stored `Plan`. Rejects an unknown, already-submitted, or
/// expired `plan_id` (`Session::submit`'s `SubmitError`) without ever
/// constructing or accepting a `Plan` from the caller.
pub(crate) fn submit_operation_impl(state: &AppState, plan_id: u64) -> Result<u64, String> {
    state.session.submit(plan_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn submit_operation(state: State<'_, AppState>, plan_id: u64) -> Result<u64, String> {
    submit_operation_impl(&state, plan_id)
}

pub(crate) fn cancel_operation_impl(state: &AppState, op_id: u64) -> Result<(), String> {
    state.session.cancel(op_id);
    Ok(())
}

#[tauri::command]
pub async fn cancel_operation(state: State<'_, AppState>, op_id: u64) -> Result<(), String> {
    cancel_operation_impl(&state, op_id)
}

pub(crate) fn list_operations_impl(state: &AppState) -> Result<Vec<OpSummary>, String> {
    Ok(state.session.operations())
}

#[tauri::command]
pub async fn list_operations(state: State<'_, AppState>) -> Result<Vec<OpSummary>, String> {
    list_operations_impl(&state)
}

pub(crate) fn get_settings_impl(state: &AppState) -> Result<Settings, String> {
    Ok(state.get_settings())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    get_settings_impl(&state)
}

pub(crate) fn set_settings_impl(state: &AppState, settings: Settings) -> Result<(), String> {
    state.set_settings(settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    set_settings_impl(&state, settings)
}

pub(crate) fn subscribe_events_impl(
    state: &AppState,
    channel: Channel<UiEvent>,
) -> Result<(), String> {
    state.channel_sink.register(channel);
    Ok(())
}

#[tauri::command]
pub async fn subscribe_events(
    state: State<'_, AppState>,
    channel: Channel<UiEvent>,
) -> Result<(), String> {
    subscribe_events_impl(&state, channel)
}

/// The exact program and argv this command runs. A pure builder so a test
/// can assert the contract -- launch Ollama.app, nothing else -- without
/// starting a process.
fn open_ollama_app_argv() -> (&'static std::path::Path, Vec<String>) {
    (
        std::path::Path::new("/usr/bin/open"),
        vec!["-a".to_string(), "Ollama".to_string()],
    )
}

/// Fire-and-forget: launches (or focuses) the Ollama.app the user already
/// has installed, for the "Ollama isn't running" notice's button. Takes no
/// input at all, so there is nothing here for the front end to build an
/// argv from or for a caller to influence -- unlike a package operation,
/// this never goes through Session/Plan because it is not a package
/// management action.
///
/// `program` is a parameter purely so tests can point it at an inert binary:
/// `cargo test --workspace` runs on the developer's machine and on CI, and a
/// test that really ran `open -a Ollama` would launch a GUI app on both.
fn open_ollama_app_impl_with(program: &std::path::Path) -> Result<(), String> {
    let (_default_program, args) = open_ollama_app_argv();
    let mut child = std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    // Reap on a background thread instead of leaving a zombie: `open` exits
    // almost immediately once it has handed off to (or failed to find)
    // Ollama.app, and this command must return without waiting for that.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub(crate) fn open_ollama_app_impl() -> Result<(), String> {
    let (program, _args) = open_ollama_app_argv();
    open_ollama_app_impl_with(program)
}

#[tauri::command]
pub async fn open_ollama_app() -> Result<(), String> {
    open_ollama_app_impl()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use async_trait::async_trait;
    use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
    use canager_core::events::{EventSink, OpId, OperationEvent};
    use canager_core::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
        Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    struct FakeAdapter {
        meta: AdapterMeta,
        instance: ManagerInstance,
        /// How many times `execute()` actually ran. Used only by the
        /// plan-rejection tests below to prove a rejected `submit` never
        /// reaches the runner (F1 in the design review); every other test
        /// in this module ignores it.
        execute_calls: Arc<AtomicUsize>,
        /// Every `CheckOptions` this adapter's `check_updates` was handed,
        /// in order. Proves the Settings toggle really reaches the adapter
        /// on a refresh instead of only round-tripping through
        /// get_settings/set_settings.
        check_options_calls: Arc<Mutex<Vec<CheckOptions>>>,
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                search: false,
                per_item_upgrade: true,
                upgrade_all: false,
                uninstall: true,
                background_check: true,
                cancel_safe: true,
            }
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            vec![self.instance.clone()]
        }

        async fn inventory(
            &self,
            _inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            Ok(Vec::new())
        }

        async fn check_updates(
            &self,
            _inst: &ManagerInstance,
            opts: &CheckOptions,
        ) -> Result<Vec<UpdateCandidate>, AdapterError> {
            self.check_options_calls.lock().unwrap().push(*opts);
            Ok(Vec::new())
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
                program: inst.exe_path.clone(),
                args: vec!["do".to_string(), req.name.clone()],
                env: vec![],
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
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
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

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-ipc-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn state_with_fake_adapter() -> AppState {
        let (state, _execute_calls, _check_options_calls) = state_with_fake_adapter_and_now(None);
        state
    }

    /// Like `state_with_fake_adapter`, but also returns a counter of how
    /// many times the fake adapter's `execute()` actually ran, and accepts
    /// an injectable clock — needed only by the plan-rejection tests below,
    /// which must prove a rejected `submit` never reaches the runner and
    /// must simulate a plan aging past its expiry window (F1 in the design
    /// review). `state_with_fake_adapter` above delegates to this with
    /// `None`, so there is exactly one place that builds this fixture.
    ///
    /// `session` and `channel_sink` share the *same* `ChannelSink` (N1 in
    /// the design review): the two used to be built from separate
    /// `ChannelSink::new()` calls, which meant a real operation's events —
    /// emitted into `session`'s sink — could never reach a Channel
    /// registered through `AppState.channel_sink`, and no test caught it
    /// because every test only ever broadcast directly on `channel_sink`
    /// rather than checking that a *real* operation's events arrive.
    fn state_with_fake_adapter_and_now(
        now_fn: Option<fn() -> i64>,
    ) -> (AppState, Arc<AtomicUsize>, Arc<Mutex<Vec<CheckOptions>>>) {
        let instance = ManagerInstance {
            id: "fake:1".to_string(),
            adapter_id: "fake".to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        };
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec![],
        };
        let execute_calls = Arc::new(AtomicUsize::new(0));
        let check_options_calls = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: execute_calls.clone(),
            check_options_calls: check_options_calls.clone(),
        });
        let sink = ChannelSink::new();
        let session =
            canager_core::session::Session::with_adapters(sink.clone(), vec![adapter], now_fn);
        let state = AppState {
            session,
            settings_path: temp_settings_path("appstate"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
        };
        (state, execute_calls, check_options_calls)
    }

    #[tokio::test]
    async fn test_get_snapshot_impl_returns_the_sessions_current_snapshot() {
        let state = state_with_fake_adapter();
        let snapshot = get_snapshot_impl(&state).expect("get_snapshot_impl");
        assert_eq!(snapshot.generation, 0);
    }

    #[tokio::test]
    async fn test_refresh_impl_detects_the_fake_instance() {
        // `refresh_impl` calls `HostEnv::discover()` internally (there is no
        // way to inject an env from the command layer — the real command
        // never has one to inject either), so this test relies on the test
        // process itself not running as root, same as every other
        // integration test in this workspace that calls real `detect()`
        // logic.
        let state = state_with_fake_adapter();
        let snapshot = refresh_impl(&state).await.expect("refresh_impl");
        assert_eq!(snapshot.instances.len(), 1);
        assert_eq!(snapshot.instances[0].id, "fake:1");
    }

    #[tokio::test]
    async fn test_refresh_impl_broadcasts_snapshot_changed_when_the_generation_moves() {
        // M9 in the design review: `refresh_impl` is the only production
        // code path in the whole plan that ever sends
        // `UiEvent::SnapshotChanged`, and only when the refresh actually
        // moved `generation`. Subscribe *first* (every other test that
        // refreshes either has no subscriber or subscribes after its last
        // refresh, which is why inverting or dropping the generation-diff
        // branch used to leave the whole suite green), then refresh a fresh
        // session: its first refresh always changes the content (no
        // instances -> the fake instance), so `generation` must move and
        // exactly one SnapshotChanged carrying the new value must reach the
        // subscriber through AppState.channel_sink.
        let state = state_with_fake_adapter();
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let before = get_snapshot_impl(&state)
            .expect("get_snapshot_impl")
            .generation;
        let snapshot = refresh_impl(&state).await.expect("refresh_impl");
        assert_ne!(
            snapshot.generation, before,
            "precondition: the first refresh must change the generation"
        );

        let events = received.lock().unwrap();
        let generations: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                UiEvent::SnapshotChanged { generation } => Some(*generation),
                UiEvent::Operation(_) => None,
            })
            .collect();
        assert_eq!(
            generations,
            vec![snapshot.generation],
            "exactly one SnapshotChanged carrying the new generation must reach the subscriber, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_refresh_impl_does_not_rebroadcast_when_the_generation_is_unchanged() {
        // Companion to the test above. The fake adapter always reports the
        // same instance and empty inventory/updates, so a second refresh
        // yields identical content and `Session::refresh` leaves
        // `generation` alone (M5: `refreshed_at` is deliberately excluded
        // from that comparison). `refresh_impl` must then stay silent — a
        // front end that re-fetches on every SnapshotChanged would
        // otherwise refetch identical data on every poll.
        let state = state_with_fake_adapter();
        let first = refresh_impl(&state).await.expect("first refresh_impl");

        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let second = refresh_impl(&state).await.expect("second refresh_impl");
        assert_eq!(
            first.generation, second.generation,
            "precondition: identical content must not move the generation"
        );

        let events = received.lock().unwrap();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, UiEvent::SnapshotChanged { .. })),
            "an unchanged refresh must not re-broadcast SnapshotChanged, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_plan_operation_impl_delegates_to_session_issue_plan() {
        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh_impl");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);
    }

    #[tokio::test]
    async fn test_plan_operation_impl_maps_session_error_to_string() {
        let state = state_with_fake_adapter();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "does-not-exist".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let err = plan_operation_impl(&state, req)
            .await
            .expect_err("expected an error for an unknown instance");
        assert!(
            err.contains("does-not-exist"),
            "error string should name the unknown instance, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_submit_and_list_and_cancel_operations_impl_round_trip() {
        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh_impl");

        // N1 in the design review: state_with_fake_adapter now wires
        // `session` and `channel_sink` to the *same* ChannelSink, so a real
        // subscriber registered here — through the same `subscribe_events_impl`
        // path the real `subscribe_events` command uses — proves that
        // wiring is actually connected end to end, not merely that
        // ChannelSink::broadcast works when called directly on a sink no
        // Session ever emits into.
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        let op_id = submit_operation_impl(&state, issued.id).expect("submit_operation_impl");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let summaries = list_operations_impl(&state).expect("list_operations_impl");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, op_id);

        cancel_operation_impl(&state, op_id).expect("cancel_operation_impl on a finished op");

        let events = received.lock().unwrap();
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Operation(OperationEvent::Status { op_id: id, .. }) if *id == op_id
            )),
            "the real operation's Status events must reach a subscriber through AppState.channel_sink"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Operation(OperationEvent::Finished { op_id: id, .. }) if *id == op_id
            )),
            "the real operation's Finished event must reach a subscriber through AppState.channel_sink"
        );
    }

    #[tokio::test]
    async fn test_submit_operation_impl_rejects_an_unissued_plan_id() {
        // F1 in the design review: submit must accept only a server-issued
        // PlanId, never anything the caller invents — including a
        // tampered or forged id nothing ever issued. The runner must never
        // be reached.
        let (state, execute_calls, _check_options_calls) = state_with_fake_adapter_and_now(None);
        refresh_impl(&state).await.expect("refresh_impl");
        let err = submit_operation_impl(&state, 999_999)
            .expect_err("an unissued plan id must be rejected");
        assert!(
            err.contains("no such plan"),
            "expected the Unknown-plan error, got: {err}"
        );
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            0,
            "a rejected submit must never reach the runner"
        );
    }

    #[tokio::test]
    async fn test_submit_operation_impl_rejects_the_same_plan_id_submitted_twice() {
        // F1: each issued plan is single-use. Resubmitting the same id —
        // e.g. a replayed IPC call — must be rejected the second time, not
        // silently run the operation again.
        let (state, execute_calls, _check_options_calls) = state_with_fake_adapter_and_now(None);
        refresh_impl(&state).await.expect("refresh_impl");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        submit_operation_impl(&state, issued.id).expect("the first submit must succeed");
        let err = submit_operation_impl(&state, issued.id)
            .expect_err("resubmitting the same plan id must be rejected");
        assert!(
            err.contains("no such plan"),
            "expected the Unknown-plan error, got: {err}"
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            1,
            "exactly one execute() call, from the first legitimate submit — not two"
        );
    }

    #[tokio::test]
    async fn test_submit_operation_impl_rejects_an_expired_plan() {
        // F1: a plan previewed too long ago must be re-previewed, not run
        // blind. Simulates 601 seconds passing between issue_plan and
        // submit via an injectable clock — `Session::{new,with_adapters}`'s
        // `now_fn` seam exists specifically so tests like this one do not
        // need to actually wait 10 minutes.
        static EXPIRED_PLAN_TEST_NOW: AtomicI64 = AtomicI64::new(1_700_000_000);
        fn expired_plan_test_now() -> i64 {
            EXPIRED_PLAN_TEST_NOW.load(Ordering::SeqCst)
        }

        let (state, execute_calls, _check_options_calls) =
            state_with_fake_adapter_and_now(Some(expired_plan_test_now));
        refresh_impl(&state).await.expect("refresh_impl");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");

        EXPIRED_PLAN_TEST_NOW.fetch_add(601, Ordering::SeqCst);

        let err = submit_operation_impl(&state, issued.id)
            .expect_err("a plan older than 600 seconds must be rejected");
        assert!(
            err.contains("older than 10 minutes"),
            "expected the Expired-plan error, got: {err}"
        );
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            0,
            "an expired submit must never reach the runner"
        );
    }

    #[test]
    fn test_get_and_set_settings_impl_round_trip() {
        let path = temp_settings_path("settings-roundtrip");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        let mut settings = get_settings_impl(&state).expect("get_settings_impl");
        assert_eq!(settings, Settings::default());
        settings.show_technical_details = true;
        set_settings_impl(&state, settings.clone()).expect("set_settings_impl");
        assert_eq!(
            get_settings_impl(&state).expect("get_settings_impl again"),
            settings
        );
    }

    #[tokio::test]
    async fn test_refresh_impl_passes_include_self_updating_from_settings_to_check_updates() {
        // Backend end of the backlog's `greedy_casks` item: the Settings
        // toggle must actually reach `Adapter::check_updates` on every
        // refresh, not just round-trip through get_settings/set_settings.
        let (state, _execute_calls, check_options_calls) = state_with_fake_adapter_and_now(None);
        let mut settings = get_settings_impl(&state).expect("get_settings_impl");
        settings.include_self_updating = true;
        set_settings_impl(&state, settings).expect("set_settings_impl");

        refresh_impl(&state).await.expect("refresh_impl");

        let calls = check_options_calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].include_self_updating,
            "refresh_impl must read the persisted setting and thread it through, got {calls:?}"
        );
    }

    #[tokio::test]
    async fn test_refresh_impl_defaults_include_self_updating_to_false() {
        let (state, _execute_calls, check_options_calls) = state_with_fake_adapter_and_now(None);
        refresh_impl(&state).await.expect("refresh_impl");
        let calls = check_options_calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].include_self_updating);
    }

    #[test]
    fn test_subscribe_events_impl_registers_a_channel_that_receives_broadcasts() {
        let path = temp_settings_path("subscribe");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");
        state
            .channel_sink
            .broadcast(UiEvent::SnapshotChanged { generation: 42 });
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_open_ollama_app_argv_is_exactly_open_dash_a_ollama() {
        // The argv is the whole contract of this command: it must launch
        // Ollama.app and nothing else, and it takes no input, so there is
        // nothing a caller could steer. Asserted from a pure builder so the
        // test never starts a process.
        let (program, args) = open_ollama_app_argv();
        assert_eq!(program, std::path::Path::new("/usr/bin/open"));
        assert_eq!(args, vec!["-a".to_string(), "Ollama".to_string()]);
    }

    #[test]
    fn test_open_ollama_app_impl_with_spawns_and_reaps_the_program_it_is_given() {
        // Deliberately /bin/echo, not /usr/bin/open: `cargo test --workspace`
        // is this plan's definition of done for every task, so a test that
        // really ran `open -a Ollama` would launch Ollama.app on the
        // developer's machine and on CI -- exactly the side effect this
        // phase's own constraint ("a background refresh never launches an
        // application") exists to prevent -- while asserting nothing beyond
        // "spawn did not error", which is true of any existing binary.
        open_ollama_app_impl_with(std::path::Path::new("/bin/echo"))
            .expect("spawning an existing program must succeed");
    }

    #[test]
    fn test_open_ollama_app_impl_with_reports_a_missing_program_instead_of_panicking() {
        let err = open_ollama_app_impl_with(std::path::Path::new("/definitely/not/a/program"))
            .expect_err("a missing program must be an Err, not a panic");
        assert!(!err.is_empty());
    }
}
