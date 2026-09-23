use crate::events::UiEvent;
use crate::state::AppState;
use canager_core::adapters::CheckOptions;
use canager_core::model::OpRequest;
use canager_core::ops::OpSummary;
use canager_core::runner::HostEnv;
use canager_core::session::{IssuedPlan, Snapshot};
use canager_core::settings::Settings;
use std::sync::atomic::Ordering;
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
/// whenever the refreshed snapshot's `generation` is newer than any this
/// process has already announced (M9 in the design review; see
/// `claim_broadcast` below for exactly what that means under concurrent
/// callers). `canager-core` must never depend on `tauri`, so
/// `Session::refresh` itself cannot send this — the shell is the only
/// layer that can, and this is the only place in the whole plan that does
/// so outside a test.
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    let opts = CheckOptions {
        include_self_updating: state.get_settings().include_self_updating,
    };
    let snapshot = state.session.refresh(&HostEnv::discover(), &opts).await;
    let generation = snapshot.generation;
    if claim_broadcast(&state.last_broadcast_generation, generation) {
        state
            .channel_sink
            .broadcast(UiEvent::SnapshotChanged { generation });
    }
    Ok(snapshot)
}

/// Whether this caller is the one that must announce `generation`, and
/// the only place `last_broadcast_generation` is written.
///
/// Two `refresh_impl` calls that coalesce inside `Session::refresh` (its
/// `refresh_gate`) both receive the *same* resulting `Snapshot`. Comparing
/// each call's own "before" reading against that shared result would let
/// both independently decide the generation moved and broadcast -- a
/// spurious duplicate for what was one refresh (Task 13). Claiming the
/// generation against a single counter is what keeps that to one
/// announcement per generation, however many callers coalesced.
///
/// `fetch_max` rather than a load followed by a compare-and-swap, which is
/// what this was: two callers that read the counter before either wrote it
/// left the loser with a failed swap and no retry, so the *newer* of the
/// two generations could be the one that went unannounced while the
/// counter sat at the older one -- and a later refresh that changed
/// nothing would then broadcast the number the UI had already been
/// waiting for. The comment here used to claim a caller that lost the swap
/// had nothing left to do "since whoever won it (or a still-newer
/// generation) already has this one covered"; the winner could be older,
/// so that was simply false.
///
/// What this does guarantee, for any interleaving:
/// - the counter ends at the highest generation any caller offered, since
///   `fetch_max` cannot move it backwards and cannot be lost;
/// - exactly one caller sees a return value below its own generation, so
///   each generation is announced at most once;
/// - a caller whose generation is higher than every generation claimed
///   before it always announces.
///
/// What it does not guarantee, and no counter can: the *order* the
/// announcements reach a subscriber in. A winner can be descheduled
/// between claiming and sending, so `SnapshotChanged` events can arrive
/// out of order. `src/lib/events.ts` is where that has to be tolerated --
/// it invalidates its snapshot query rather than trusting the number on
/// the event, and its cache rejects an older snapshot than the one it
/// holds.
fn claim_broadcast(
    last_broadcast_generation: &std::sync::atomic::AtomicU64,
    generation: u64,
) -> bool {
    generation > last_broadcast_generation.fetch_max(generation, Ordering::SeqCst)
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
/// Maps `Session::issue_plan`'s error to the string every `#[tauri::command]`
/// in this file rejects with. Every variant but one keeps `AdapterError`'s
/// own `Display` verbatim, exactly as before -- those are refusals that
/// should never happen outside a bug (an unknown instance, an unregistered
/// adapter) and are shown as-is on the assumption nobody but a developer
/// will ever read them.
///
/// `NotActionable` is different: both `InstalledPage` and `UpdatesPage`
/// hide every control for an instance that fails this gate, so a real
/// person sees it only through a stale snapshot or a genuine TOCTOU -- and
/// when that happens, `{:?}` of two Rust enums is the worst possible thing
/// to show someone who does not read Rust. It goes out as a small JSON
/// object instead, so `src/lib/sources.ts`'s `parseNotActionable` can turn
/// it into the same localised copy the source's own notice already uses
/// (`READ_ONLY_NOTICE_KEYS`, `sourceNotice.notRunning`,
/// `sourceNotice.unreachable`) instead of showing it verbatim.
///
/// This is a narrower change than it looks: `plan_operation` still returns
/// `Result<IssuedPlan, String>`, identical to every other command, so
/// nothing about the IPC boundary's *type* widens and `src/lib/api.ts`'s
/// single `call()` choke point needs no special case. Only the *content* of
/// the string differs for this one refusal.
fn plan_operation_error(e: canager_core::adapters::AdapterError) -> String {
    match e {
        canager_core::adapters::AdapterError::NotActionable {
            read_only,
            unavailable,
        } => not_actionable_json(read_only, unavailable),
        other => other.to_string(),
    }
}

/// The one payload shape both refusals of the actionability gate go out
/// as, whether the gate refused at `issue_plan` (an `AdapterError`) or at
/// `submit` (a `SubmitError`): the front end has exactly one decoder for
/// it (`parseNotActionable` in `src/lib/sources.ts`) and both must feed it
/// the same thing.
fn not_actionable_json(
    read_only: Option<canager_core::model::ReadOnlyReason>,
    unavailable: Option<canager_core::model::Unavailable>,
) -> String {
    serde_json::json!({
        "kind": "not_actionable",
        "read_only": read_only,
        "unavailable": unavailable,
    })
    .to_string()
}

/// `plan_operation_error`'s counterpart for `Session::submit`.
///
/// `Unknown` and `Expired` keep `SubmitError`'s own `Display`, which is
/// already a sentence a person can read. The two refusals the submit-time
/// actionability re-check produces do not: `NotActionable`'s `Display`
/// is a `{:?}` of two Rust enums, and this is the refusal a real person is
/// *most* likely to see -- both pages hide controls for a source that
/// fails the gate, but nothing can hide a source that failed it in the
/// second between rendering the preview and clicking Confirm. It goes out
/// as the same JSON `plan_operation_error` produces, so the front end
/// renders it with the same localised copy the source's own notice uses.
/// `SourceGone` has no instance left to carry a reason, so it gets its own
/// kind rather than a `not_actionable` with two nulls, which would decode
/// to an empty message.
fn submit_operation_error(e: canager_core::session::SubmitError) -> String {
    match e {
        canager_core::session::SubmitError::NotActionable {
            read_only,
            unavailable,
        } => not_actionable_json(read_only, unavailable),
        canager_core::session::SubmitError::SourceGone => {
            serde_json::json!({ "kind": "source_gone" }).to_string()
        }
        other => other.to_string(),
    }
}

pub(crate) async fn plan_operation_impl(
    state: &AppState,
    request: OpRequest,
) -> Result<IssuedPlan, String> {
    state
        .session
        .issue_plan(&request)
        .await
        .map_err(plan_operation_error)
}

#[tauri::command]
pub async fn plan_operation(
    state: State<'_, AppState>,
    request: OpRequest,
) -> Result<IssuedPlan, String> {
    plan_operation_impl(&state, request).await
}

/// Consumes the plan stored under `plan_id` (one-time use) and submits
/// exactly that stored `Plan`. Rejects an unknown, already-submitted or
/// expired `plan_id`, and one whose source no longer passes the
/// actionability gate (`Session::submit`'s `SubmitError`), without ever
/// constructing or accepting a `Plan` from the caller.
pub(crate) fn submit_operation_impl(state: &AppState, plan_id: String) -> Result<u64, String> {
    state
        .session
        .submit(plan_id)
        .map_err(submit_operation_error)
}

#[tauri::command]
pub async fn submit_operation(state: State<'_, AppState>, plan_id: String) -> Result<u64, String> {
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

/// How long `open_ollama_app_impl_with` waits for `/usr/bin/open` to
/// report what happened before giving up on an answer.
///
/// `open -a` returns as soon as LaunchServices has accepted (or refused)
/// the request -- it does not wait for the app to finish starting, which
/// is what `-W` would do -- so in practice this is milliseconds. The
/// bound exists so that a wedged LaunchServices cannot leave the Open
/// Ollama button spinning forever; past it the launch is reported as
/// under way, which is what the front end's own poll then confirms or
/// does not.
const OPEN_ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// The payload the Open Ollama button's failures go out as, in the same
/// shape and for the same reason as `not_actionable_json` above: a
/// `kind` the front end can recognise and a `reason` it can localise,
/// rather than an English sentence written in Rust that a zh-CN user
/// would read in English. Decoded by `parseOpenOllamaFailure` in
/// `src/lib/sources.ts`.
fn open_ollama_failed_json(reason: &str) -> String {
    serde_json::json!({ "kind": "ollama_open_failed", "reason": reason }).to_string()
}

/// Launches (or focuses) Ollama.app for the "Ollama isn't running"
/// notice's button. Takes no input at all, so there is nothing here for
/// the front end to build an argv from or for a caller to influence --
/// unlike a package operation, this never goes through Session/Plan
/// because it is not a package management action.
///
/// It used to be fire-and-forget: spawn, reap on a detached thread,
/// return `Ok(())` without ever reading the exit status. The front end
/// treats `Ok` as success and renders nothing, so a user whose `open -a
/// Ollama` failed -- the commonest cause being `brew install ollama`,
/// which installs the CLI and no app -- pressed a button that did
/// nothing, forever, with no message. The exit status is read now, and a
/// failure comes back as something the notice can say out loud.
///
/// The three standard streams stay nulled. Nothing here parses them: the
/// two outcomes this reports apart are decided by whether Ollama.app
/// exists (`open_ollama_app_impl`) and by the exit status, not by
/// matching English against `open`'s stderr, which would break on a
/// non-English Mac.
///
/// `program` is a parameter purely so tests can point it at an inert
/// binary: `cargo test --workspace` runs on the developer's machine and
/// on CI, and a test that really ran `open -a Ollama` would launch a GUI
/// app on both.
fn open_ollama_app_impl_with(program: &std::path::Path) -> Result<(), String> {
    let (_default_program, args) = open_ollama_app_argv();
    // `spawn()` inherits the parent's stdin/stdout/stderr by default, which
    // hands this child Canager's own console and pipes for no reason -- it
    // takes no input and nothing here ever reads its output. Nulling all
    // three is the same "share nothing it does not need" rule `run_plan`
    // already applies to every package-manager command; this is the one
    // launch in the app that bypasses `run_plan` and so had been missed.
    let mut child = std::process::Command::new(program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| open_ollama_failed_json("launch_failed"))?;
    // Waited for on a background thread rather than inline, so the bound
    // above is a bound: `recv_timeout` returns whether or not the child
    // ever does, and the thread still reaps it either way instead of
    // leaving a zombie.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait().map(|status| status.success()));
    });
    match rx.recv_timeout(OPEN_ANSWER_TIMEOUT) {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) | Ok(Err(_)) => Err(open_ollama_failed_json("launch_failed")),
        // No answer within the bound. Reported as success, on purpose,
        // and this is the reasoning rather than a default:
        //
        // - The case that made this button silently useless -- no
        //   Ollama.app at all -- never gets here. `open_ollama_app_impl`
        //   answers `not_installed` before spawning, and `open -a` for an
        //   app LaunchServices cannot find refuses in milliseconds, which
        //   the arm above reports. So a slow answer is not the bug this
        //   function was rewritten to surface.
        // - What can make `open` slow is macOS being busy with a launch
        //   that is working: a cold start on a loaded machine, or the
        //   first open of a freshly downloaded app, which Gatekeeper
        //   verifies and then asks the user about ("downloaded from the
        //   internet, open it?") before the launch completes. Saying
        //   "couldn't open Ollama" beside that dialog would be the wrong
        //   message at the worst moment.
        // - Success is not the last word. The front end's `onSuccess`
        //   polls for the daemon, and if it never answers the "isn't
        //   running" notice simply stays up with its button, so the user
        //   still sees that nothing has changed. What is given up is only
        //   the reason, for a failure that arrives after 20 s; the thread
        //   still reaps the child when it does.
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        // The waiting thread dropped its sender without sending, which
        // only a panic inside `Child::wait` could do. There is then no
        // exit status to read either way; treated like the timeout for
        // the same last reason -- the poll, not this, decides whether the
        // notice clears.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(()),
    }
}

pub(crate) fn open_ollama_app_impl() -> Result<(), String> {
    // Asked before spawning, because `open`'s own refusal cannot be told
    // apart from any other failure without matching English text against
    // its stderr. This is the case worth telling apart: `brew install
    // ollama` -- Homebrew being the first source this project's README
    // lists -- installs the CLI and no app, and "there is no Ollama app
    // on this Mac" is a different thing to say than "it would not
    // start". `detect` asks the same question and withholds the button
    // entirely when the answer is no, so reaching this is either a TOCTOU
    // (the app was removed since the last refresh) or a stale snapshot.
    if canager_core::adapters::ollama::ollama_app_path(&HostEnv::discover().home).is_none() {
        return Err(open_ollama_failed_json("not_installed"));
    }
    let (program, _args) = open_ollama_app_argv();
    open_ollama_app_impl_with(program)
}

#[tauri::command]
pub async fn open_ollama_app() -> Result<(), String> {
    // On the blocking pool, not inline: `open_ollama_app_impl` now waits
    // (up to `OPEN_ANSWER_TIMEOUT`) for `open` to answer, and doing that
    // inside an async command would hold one of the async runtime's
    // worker threads -- the ones every other command and the refresh run
    // on -- for as long as LaunchServices takes.
    tauri::async_runtime::spawn_blocking(open_ollama_app_impl)
        .await
        .unwrap_or_else(|_| Err(open_ollama_failed_json("launch_failed")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use async_trait::async_trait;
    use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use canager_core::events::{EventSink, OpId, OperationEvent};
    use canager_core::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
        Outcome, Plan, Reconciled, ResourceLock, SearchHit,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
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
        detect_delay: std::time::Duration,
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            if !self.detect_delay.is_zero() {
                tokio::time::sleep(self.detect_delay).await;
            }
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
        ) -> Result<CheckOutcome, AdapterError> {
            self.check_options_calls.lock().unwrap().push(*opts);
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
        let instance = canager_core::testing::manager_instance("fake", "fake:1");
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
            detect_delay: std::time::Duration::ZERO,
        });
        let sink = ChannelSink::new();
        let session =
            canager_core::session::Session::with_adapters(sink.clone(), vec![adapter], now_fn);
        let state = AppState {
            session,
            settings_path: temp_settings_path("appstate"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        };
        (state, execute_calls, check_options_calls)
    }

    /// Like `state_with_fake_adapter_and_now`, but the fake adapter's
    /// `detect()` sleeps for `detect_delay` first -- long enough to widen
    /// the race window so two concurrent `refresh_impl` calls reliably
    /// coalesce inside `Session::refresh`'s `refresh_gate`, mirroring
    /// `session::tests::test_concurrent_refresh_calls_are_coalesced`'s own
    /// use of an artificial delay for the same reason.
    fn state_with_slow_fake_adapter(detect_delay: std::time::Duration) -> Arc<AppState> {
        let instance = canager_core::testing::manager_instance("fake", "fake:1");
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec![],
        };
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: Arc::new(AtomicUsize::new(0)),
            check_options_calls: Arc::new(Mutex::new(Vec::new())),
            detect_delay,
        });
        let sink = ChannelSink::new();
        let session =
            canager_core::session::Session::with_adapters(sink.clone(), vec![adapter], None);
        Arc::new(AppState {
            session,
            settings_path: temp_settings_path("ipc-slow"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        })
    }

    #[test]
    fn test_claim_broadcast_announces_each_generation_once_and_never_an_older_one() {
        use std::sync::atomic::AtomicU64;

        let counter = AtomicU64::new(0);
        assert!(
            claim_broadcast(&counter, 1),
            "the first caller past the post announces"
        );
        assert!(
            !claim_broadcast(&counter, 1),
            "a coalesced caller holding the same result must not announce it twice"
        );
        assert!(claim_broadcast(&counter, 2), "a newer generation announces");
        assert!(
            !claim_broadcast(&counter, 1),
            "an older generation arriving late is already covered by the newer one"
        );
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        assert!(
            claim_broadcast(&counter, 5),
            "and a jump forward still announces"
        );
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_claim_broadcast_cannot_lose_the_newest_generation_to_a_racing_caller() {
        // The interleaving this replaces, with a plain load + CAS:
        //   the counter is 0; A commits generation 1 and loads 0; B
        //   commits generation 2 and loads 0; A swaps 0 -> 1 and
        //   announces 1; B's swap 0 -> 2 fails and B returns without
        //   retrying.
        // Nothing ever carried generation 2, and the counter sat at 1 --
        // so a later refresh that changed *nothing* would announce 2, and
        // the UI would sit waiting for a change that had already
        // happened.
        //
        // Reproducing it needs the two claimers inside the same handful
        // of instructions. Releasing two freshly spawned threads from a
        // `Barrier` does not: they come out tens of microseconds apart,
        // an eternity next to a load and the compare-exchange after it,
        // and the old code passed that test 500 rounds out of 500. Two
        // long-lived threads already spinning on `gate` do: measured
        // against the old load+CAS, this schedule loses the newer
        // generation in roughly 70% of rounds.
        use std::sync::atomic::{AtomicU64, AtomicUsize};

        const ROUNDS: usize = 2_000;
        const CLAIMERS: usize = 2;

        // Per-round state, allocated up front so no round has to
        // allocate (or lock) inside the window being raced.
        let counters: Arc<Vec<AtomicU64>> =
            Arc::new((0..ROUNDS).map(|_| AtomicU64::new(0)).collect());
        let announced: Arc<Vec<Mutex<Vec<u64>>>> =
            Arc::new((0..ROUNDS).map(|_| Mutex::new(Vec::new())).collect());
        let gate = Arc::new(AtomicUsize::new(usize::MAX));
        let arrived = Arc::new(AtomicUsize::new(0));

        let claimers: Vec<_> = (1..=CLAIMERS as u64)
            .map(|generation| {
                let counters = counters.clone();
                let announced = announced.clone();
                let gate = gate.clone();
                let arrived = arrived.clone();
                std::thread::spawn(move || {
                    for round in 0..ROUNDS {
                        arrived.fetch_add(1, Ordering::SeqCst);
                        while gate.load(Ordering::Acquire) != round {
                            std::hint::spin_loop();
                        }
                        if claim_broadcast(&counters[round], generation) {
                            announced[round].lock().unwrap().push(generation);
                        }
                    }
                })
            })
            .collect();

        for round in 0..ROUNDS {
            // Every claimer has finished the previous round and is
            // spinning on this one before the gate opens.
            while arrived.load(Ordering::SeqCst) < (round + 1) * CLAIMERS {
                std::hint::spin_loop();
            }
            gate.store(round, Ordering::Release);
        }
        for claimer in claimers {
            claimer.join().expect("claimer thread");
        }

        let newest = CLAIMERS as u64;
        for round in 0..ROUNDS {
            let announced = announced[round].lock().unwrap();
            assert!(
                announced.contains(&newest),
                "the newest generation must always be announced by someone \
                 (round {round}, announced {announced:?})"
            );
            assert_eq!(
                counters[round].load(Ordering::SeqCst),
                newest,
                "and the counter must not be left below it, or a later unchanged \
                 refresh would announce it instead (round {round})"
            );
        }
    }

    #[tokio::test]
    async fn test_refresh_impl_broadcasts_snapshot_changed_exactly_once_when_two_calls_coalesce() {
        // Two refresh_impl calls that coalesce inside Session::refresh (its
        // refresh_gate) both receive the *same* resulting Snapshot. Each
        // independently comparing that result's generation against its own
        // "before" reading used to make both of them decide the generation
        // moved and broadcast -- a spurious duplicate for what was really
        // one refresh.
        let state = state_with_slow_fake_adapter(std::time::Duration::from_millis(100));
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let state_a = state.clone();
        let state_b = state.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { refresh_impl(&state_a).await }),
            tokio::spawn(async move { refresh_impl(&state_b).await }),
        );
        let snap_a = a.expect("task a").expect("refresh_impl a");
        let snap_b = b.expect("task b").expect("refresh_impl b");
        assert_eq!(
            snap_a.generation, snap_b.generation,
            "precondition: both calls must see the same coalesced result"
        );

        let events = received.lock().unwrap();
        let broadcasts: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                UiEvent::SnapshotChanged { generation } => Some(*generation),
                UiEvent::Operation(_) => None,
            })
            .collect();
        assert_eq!(
            broadcasts,
            vec![snap_a.generation],
            "exactly one SnapshotChanged must reach the subscriber even though two refresh_impl calls coalesced, got: {events:?}"
        );
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

    /// Builds an `AppState` around a single fake instance, letting the
    /// caller control its writability/availability -- unlike
    /// `state_with_fake_adapter`, which always builds a writable, available
    /// one. The only way to reach `plan_operation`'s `NotActionable`
    /// mapping is through an instance the gate in `Session::issue_plan`
    /// refuses.
    fn state_with_instance(instance: ManagerInstance) -> AppState {
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec![],
        };
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: Arc::new(AtomicUsize::new(0)),
            check_options_calls: Arc::new(Mutex::new(Vec::new())),
            detect_delay: std::time::Duration::ZERO,
        });
        let sink = ChannelSink::new();
        let session =
            canager_core::session::Session::with_adapters(sink.clone(), vec![adapter], None);
        AppState {
            session,
            settings_path: temp_settings_path("not-actionable"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    #[tokio::test]
    async fn test_plan_operation_impl_maps_not_actionable_to_structured_json_not_a_debug_dump() {
        // Both pages hide every control for an instance the actionability
        // gate (spec §2.5) would refuse, so a real person only ever sees
        // this through a stale snapshot or a genuine TOCTOU -- and even
        // then, `AdapterError::NotActionable`'s own `Display` (a `{:?}` of
        // two Rust enums) must never be what reaches them. `plan_operation`
        // stays `Result<IssuedPlan, String>` like every other command (no
        // widened IPC type); what changes is *what the string holds* for
        // this one case: JSON the front end can localise, not English
        // prose it can only show verbatim.
        let instance = canager_core::testing::read_only_instance(
            "fake",
            "fake:1",
            canager_core::model::ReadOnlyReason::PrefixNotWritable,
        );
        let state = state_with_instance(instance);
        refresh_impl(&state).await.expect("refresh_impl");

        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let err = plan_operation_impl(&state, req)
            .await
            .expect_err("a read-only instance must be refused");

        assert!(
            !err.contains("Some(") && !err.contains("None"),
            "must not be a Rust Debug dump of the two reasons: {err}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&err)
            .unwrap_or_else(|e| panic!("expected JSON, got {err:?} ({e})"));
        assert_eq!(parsed["kind"], "not_actionable");
        assert_eq!(parsed["read_only"], "PrefixNotWritable");
        assert_eq!(parsed["unavailable"], serde_json::Value::Null);
    }

    #[test]
    fn test_submit_operation_error_localisable_for_the_gate_and_verbatim_for_the_rest() {
        use canager_core::session::SubmitError;

        // The refusal a real person is most likely to meet: they were
        // looking at a preview when the source stopped answering. What
        // reaches them must be the same payload the plan-time refusal
        // sends, because the front end has one decoder for it.
        let err = submit_operation_error(SubmitError::NotActionable {
            read_only: None,
            unavailable: Some(canager_core::model::Unavailable::NotRunning),
        });
        assert!(
            !err.contains("Some(") && !err.contains("None"),
            "must not be a Rust Debug dump of the two reasons: {err}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&err)
            .unwrap_or_else(|e| panic!("expected JSON, got {err:?} ({e})"));
        assert_eq!(parsed["kind"], "not_actionable");
        assert_eq!(parsed["read_only"], serde_json::Value::Null);
        assert_eq!(parsed["unavailable"], "NotRunning");

        let gone = submit_operation_error(SubmitError::SourceGone);
        let parsed: serde_json::Value = serde_json::from_str(&gone)
            .unwrap_or_else(|e| panic!("expected JSON, got {gone:?} ({e})"));
        assert_eq!(
            parsed["kind"], "source_gone",
            "a vanished source has no reason to name, so it cannot go out as \
             not_actionable with two nulls -- that decodes to an empty message"
        );

        // The two that were already sentences stay sentences: wrapping
        // them in JSON would put braces on screen for no gain.
        assert_eq!(
            submit_operation_error(SubmitError::Expired),
            SubmitError::Expired.to_string()
        );
        assert_eq!(
            submit_operation_error(SubmitError::Unknown),
            SubmitError::Unknown.to_string()
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
        let err = submit_operation_impl(&state, "forged-plan-id".to_string())
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
        submit_operation_impl(&state, issued.id.clone()).expect("the first submit must succeed");
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
        // blind. A plan's lifetime is measured on the monotonic clock --
        // moving the injected `now_fn` forward used to simulate this and
        // deliberately no longer can, since a system clock the user (or
        // NTP) can step must not decide whether a destructive preview is
        // still live. `testing::expire_issued_plans` ages what the
        // session is holding instead.
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

        canager_core::testing::expire_issued_plans(&state.session);

        let err = submit_operation_impl(&state, issued.id)
            .expect_err("a plan older than its lifetime must be rejected");
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
        assert_eq!(err, open_ollama_failed_json("launch_failed"));
    }

    #[test]
    fn test_open_ollama_app_impl_with_reports_a_program_that_exits_nonzero() {
        // The whole finding: this used to spawn, reap on a detached
        // thread and return `Ok(())` without ever looking at the exit
        // status, so a failed `open -a Ollama` -- stderr nulled, nothing
        // read -- reached the front end as success and rendered nothing.
        // A user with the CLI and no app pressed a button that did
        // nothing at all, forever, with no message.
        let err = open_ollama_app_impl_with(std::path::Path::new("/usr/bin/false"))
            .expect_err("a non-zero exit must not be reported as success");
        assert_eq!(err, open_ollama_failed_json("launch_failed"));
    }

    #[test]
    fn test_open_ollama_failure_payloads_are_the_two_the_front_end_decodes() {
        // Locked here because the copy on the other side keys off these
        // exact strings, and `src/lib/sources.ts` has a matching test.
        assert_eq!(
            open_ollama_failed_json("not_installed"),
            r#"{"kind":"ollama_open_failed","reason":"not_installed"}"#
        );
        assert_eq!(
            open_ollama_failed_json("launch_failed"),
            r#"{"kind":"ollama_open_failed","reason":"launch_failed"}"#
        );
    }

    #[test]
    fn test_open_ollama_app_impl_with_nulls_all_three_stdio_streams() {
        // "Open Ollama" spawns with inherited stdio today: a Canager built
        // and launched from a Terminal window hands that child process the
        // app's own stdin/stdout/stderr, which it has no business sharing
        // (spec's step D, item 3). This runs a shell script in place of
        // `/usr/bin/open` that inspects its *own* three standard streams --
        // by comparing each `/dev/fd/N` against `/dev/null` with `-ef`,
        // which compares device and inode, not content -- and records what
        // it found to a file passed in its own source rather than to
        // stdout, since a genuinely-nulled stdout could not carry the
        // answer back.
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "canager-stdio-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let script_path = dir.join("probe.sh");
        let out_path = dir.join("out.txt");

        // Each stream's `-ef` check must run *before* the final block
        // redirects fd 1 to `out_path` to collect the answer -- checking
        // `/dev/fd/1` from inside that block would just compare `out_path`
        // against itself and report "null" whatever the original stdout
        // was, since by then fd 1 no longer points at it.
        let script = format!(
            "#!/bin/sh\n\
             r0=inherited; [ /dev/fd/0 -ef /dev/null ] && r0=null\n\
             r1=inherited; [ /dev/fd/1 -ef /dev/null ] && r1=null\n\
             r2=inherited; [ /dev/fd/2 -ef /dev/null ] && r2=null\n\
             {{\n\
             echo stdin=$r0\n\
             echo stdout=$r1\n\
             echo stderr=$r2\n\
             }} > {out}\n",
            out = out_path.display()
        );
        {
            let mut f = std::fs::File::create(&script_path).expect("create script");
            f.write_all(script.as_bytes()).expect("write script");
        }
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod +x");

        open_ollama_app_impl_with(&script_path).expect("spawning the probe script must succeed");

        // Fire-and-forget: the script runs on its own timeline, so poll
        // briefly for its output rather than assuming it has finished the
        // instant `spawn()` returns.
        let mut contents = String::new();
        for _ in 0..250 {
            if let Ok(s) = std::fs::read_to_string(&out_path) {
                if !s.is_empty() {
                    contents = s;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            contents.contains("stdin=null"),
            "stdin must be null, got: {contents:?}"
        );
        assert!(
            contents.contains("stdout=null"),
            "stdout must be null, got: {contents:?}"
        );
        assert!(
            contents.contains("stderr=null"),
            "stderr must be null, got: {contents:?}"
        );
    }
}
