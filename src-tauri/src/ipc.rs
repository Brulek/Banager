use crate::events::UiEvent;
use crate::state::AppState;
use canager_core::adapters::CheckOptions;
use canager_core::auto_check::RoundTrigger;
use canager_core::model::OpRequest;
use canager_core::ops::{CancelRefused, OpSummary};
use canager_core::runner::HostEnv;
use canager_core::scan::UnknownScan;
use canager_core::session::{IssuedPlan, Session, Snapshot};
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

/// The window's refresh, the `refresh` command's body: `refresh_as` for
/// `RoundTrigger::Window`.
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    refresh_as(state, RoundTrigger::Window).await
}

/// A refresh asked for as `trigger`: the window's (`refresh_impl`) or the
/// daily check's (`auto_check::check_automatically`), through
/// `refresh_for`.
pub(crate) async fn refresh_as(
    state: &AppState,
    trigger: RoundTrigger,
) -> Result<Snapshot, String> {
    refresh_for(state, Asker::Trigger(trigger)).await
}

/// Who a refresh the shell runs is recorded as asked for by.
#[derive(Clone, Copy)]
enum Asker {
    /// The window or the daily check (`refresh_as`).
    Trigger(RoundTrigger),
    /// Whoever asked for the round that started the `brew update` whose end
    /// set this refresh off (`refresh_on_background_change`), read as its
    /// round is recorded (`RoundLog::record_follow_up`).
    FollowUp,
}

/// Every refresh the shell runs goes through here: the window's
/// (`refresh_impl`), the daily check's (`auto_check::check_automatically`)
/// and the one a finished `brew update` sets off
/// (`refresh_on_background_change`). Records who asked for the round that
/// answered (`RoundLog::record`, or `RoundLog::record_follow_up` for the
/// last of the three) before that round's snapshot is committed
/// (`Session::refresh_recording`) -- so before the page can fetch it
/// (`get_snapshot`, which it may do at any time, a refetch as the window
/// comes back included) and report it (`notify::report_update_set`, which
/// looks up who asked) -- then broadcasts
/// `UiEvent::SnapshotChanged` on `state.channel_sink` whenever the
/// refreshed snapshot's `generation` is newer than any this process has
/// already announced (M9 in the design review; see `claim_broadcast` below
/// for exactly what that means under concurrent callers), and after every
/// round asked for as `RoundTrigger::Automatic` whatever its generation
/// (`announce`). `canager-core` must never depend on `tauri`, so
/// `Session::refresh` itself cannot send this — the shell is the only
/// layer that can, and `announce` below is the only place that does so
/// outside a test.
async fn refresh_for(state: &AppState, asker: Asker) -> Result<Snapshot, String> {
    // Who this call asks as, which `announce` reads: for a follow-up, what
    // the record callback reads from the log.
    let mut trigger = match asker {
        Asker::Trigger(trigger) => trigger,
        Asker::FollowUp => RoundTrigger::Window,
    };
    let (_, snapshot) = state
        .session
        .refresh_recording(
            &HostEnv::discover(),
            &check_options(state),
            |round, snapshot| {
                let mut rounds = state.rounds.lock().unwrap();
                match asker {
                    Asker::Trigger(asked) => rounds.record(round, asked, snapshot),
                    Asker::FollowUp => trigger = rounds.record_follow_up(round, snapshot),
                }
            },
        )
        .await;
    Ok(announce(state, snapshot, trigger))
}

fn check_options(state: &AppState) -> CheckOptions {
    CheckOptions {
        include_self_updating: state.get_settings().include_self_updating,
        ..CheckOptions::default()
    }
}

/// Broadcasts `SnapshotChanged` for `snapshot` if this caller is the one
/// that claims its generation (`claim_broadcast`), and hands it back.
///
/// A round of the daily check (`RoundTrigger::Automatic`) is announced
/// whatever its generation: one that found nothing new keeps the
/// generation it read, and without the event the window would never
/// fetch it. With it, the page takes the snapshot of that round -- the
/// same generation, a later `refreshed_at`, which `isNewerSnapshot` in
/// src/lib/events.ts lets in -- so its header's last check moves, and it
/// reports the updates the round offers with the round's number
/// (`notify::report_update_set`), which is how a notification that
/// failed, or pairs nobody has seen, are tried again at the next daily
/// check.
fn announce(state: &AppState, snapshot: Snapshot, trigger: RoundTrigger) -> Snapshot {
    let generation = snapshot.generation;
    let claimed = claim_broadcast(&state.last_broadcast_generation, generation);
    if claimed || trigger == RoundTrigger::Automatic {
        state
            .channel_sink
            .broadcast(UiEvent::SnapshotChanged { generation });
    }
    snapshot
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
///   each generation is announced at most once on its account -- a round
///   of the daily check is announced besides, whatever its generation
///   (`announce`);
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

/// Refreshes each time `Session::background_change` says something a
/// refresh reported has changed by itself -- a `brew update` a refresh
/// stopped waiting for has ended -- for the life of the app. Spawned once
/// at startup.
///
/// Through `announce`, as `refresh_impl` is, so the window learns of the
/// new snapshot the way it learns of any other, by `SnapshotChanged`: the
/// "still downloading" notice goes and the fresh catalogue shows without
/// the user pressing anything. Nothing new crosses to the front end.
///
/// Through `refresh_for`, so `Session::refresh`, like any other refresh.
/// The refresh in flight when this wakes can be the one that reported the
/// update as running, still waiting on a slow source; `Session::refresh`
/// never answers a call with a round that started before the call
/// arrived, so this gets a round that starts after the wake-up, and so
/// after the update ended. `refresh_for` never returns `Err`; a failed
/// refresh is on screen through the snapshot's own `errors`.
///
/// Recorded as asked for by whoever asked for the round that started the
/// `brew update` that ended, read as this refresh's round is recorded
/// (`RoundLog::record_follow_up`): the follow-up of a daily check's
/// update is the daily check's, and that of a check of the window's is
/// the window's. It used to be read as the wake-up came. The round that
/// started the update is recorded only as it commits, and a slow source
/// can keep it going after the update has ended, so the wake-up could
/// find it not yet recorded and count the daily check's follow-up as the
/// window's, and the daily check's notification was then never posted.
/// This refresh's round starts after the wake-up, and one round runs at a
/// time, so the round in flight at the wake-up has committed, and been
/// recorded, before this one is.
pub(crate) async fn refresh_on_background_change(state: &AppState) {
    loop {
        state.session.background_change().await;
        let _ = refresh_for(state, Asker::FollowUp).await;
    }
}

/// Resolves and plans `request` through `Session::issue_plan`, returning
/// the server-issued `IssuedPlan` for the caller to preview. Nothing in
/// the returned `Plan` is ever accepted back from the client — F1 in the
/// design review: IPC accepts only known operations and server-issued
/// object IDs, never a client-supplied `Plan`. `submit_operation_impl`
/// below is the only way to actually run it, and takes only the id.
/// Maps `Session::issue_plan`'s error to the string every `#[tauri::command]`
/// in this file rejects with: always a small `{"kind": ...}` JSON object,
/// never `AdapterError`'s own `Display`, which is this project's English.
/// `src/lib/sources.ts`'s `planErrorMessage` turns each kind into a
/// sentence in the user's language.
///
/// This used to send every variant but `NotActionable` as its `Display`,
/// on the theory that they were refusals only a developer would ever see.
/// Some are (an unregistered adapter), but not all: a source removed after
/// the last refresh, a name the validator will not pass to a tool, Homebrew
/// gone between the check and the dependents scan -- each put an English
/// sentence inside a Chinese frame. The kinds split along whose words the
/// detail is:
///
/// - **Canager's own words** go out with no prose at all, only data the
///   front end can interpolate into its own sentence: `source_gone`,
///   `invalid_name` (the name), `program_missing` (the path),
///   `output_too_large`, `uninstall_unsafe` (the path a path-list
///   uninstall preview refused, and which check refused it),
///   `index_updating` (brew's uninstall preview
///   would not read Homebrew's catalogue while `brew update` rewrites
///   it, and the user should try again shortly), and `refused` for
///   everything that is a bug in
///   Canager rather than a state of the Mac (an instance/request mismatch,
///   an unregistered adapter, a test-only `NoMock`). `Refused` carries an
///   English string in Rust; it is dropped here on purpose -- it is for
///   logs.
/// - **Another program's words** are kept verbatim, for the front end to
///   quote inside a translated sentence that says what happened:
///   `spawn_failed` carries the operating system's reason it could not
///   start the tool.
///
/// `Parse`, `CommandFailed` and `Unsupported` have no kind of their own
/// because no `plan()` returns them: brew's is the only one that runs a
/// command (`brew uses`), and it turns that command's failure into
/// `Warning::DependentsUnknown` rather than an error; the others run
/// nothing. pip's `plan()` does refuse with `Unsupported`, but every pip
/// instance is read-only by design, so `issue_plan`'s gate refuses first
/// with `not_actionable`. They go out as `refused` -- if one ever arrives,
/// an adapter broke that contract, which is Canager's bug. A kind of their
/// own would be copy in two locales that nothing can make appear.
/// `IndexUpdating` is not one of them: brew's uninstall `plan()` returns it
/// (the `catalogue_stamp` checks around its `brew uses`) while a `brew
/// update` is running, a state of the Mac that passes by itself.
///
/// `plan_operation` still returns `Result<IssuedPlan, String>`, identical to
/// every other command, so `src/lib/api.ts`'s single `call()` choke point
/// needs no special case. Only the *content* of the string is structured.
fn plan_operation_error(e: canager_core::adapters::AdapterError) -> String {
    use canager_core::adapters::AdapterError;
    use canager_core::runner::RunnerError;
    match e {
        AdapterError::NotActionable {
            read_only,
            unavailable,
        } => not_actionable_json(read_only, unavailable),
        AdapterError::UpdateBlocked { reason } => update_blocked_json(reason),
        AdapterError::UninstallBlocked { reason } => uninstall_blocked_json(reason),
        // A path-list uninstall's preview refused one of its checks (phase
        // 4 step C): the path, home folder abbreviated, and the reason
        // spelled by hand -- the one producer of these strings, which
        // `UNINSTALL_UNSAFE_KEYS` in src/lib/sources.ts indexes by.
        AdapterError::UninstallUnsafe { path, reason } => {
            use canager_core::model::UninstallUnsafeReason;
            let reason = match reason {
                UninstallUnsafeReason::OutsideHome => "outside_home",
                UninstallUnsafeReason::SharedFolder => "shared_folder",
                UninstallUnsafeReason::Missing => "missing",
                UninstallUnsafeReason::NotOwnedByYou => "not_owned_by_you",
                UninstallUnsafeReason::NotWhatInstructionsExpect => "not_what_instructions_expect",
                UninstallUnsafeReason::OverlapsKept => "overlaps_kept",
            };
            serde_json::json!({ "kind": "uninstall_unsafe", "path": path, "reason": reason })
                .to_string()
        }
        // The same bare kind `submit_operation_error` sends for
        // `SubmitError::SourceGone`: one situation, one sentence.
        AdapterError::SourceGone { .. } => serde_json::json!({ "kind": "source_gone" }).to_string(),
        AdapterError::InvalidName(name) => {
            serde_json::json!({ "kind": "invalid_name", "name": name }).to_string()
        }
        AdapterError::Runner(RunnerError::NotFound(program)) => serde_json::json!({
            "kind": "program_missing",
            "program": program.display().to_string(),
        })
        .to_string(),
        AdapterError::Runner(RunnerError::Spawn(io)) => {
            serde_json::json!({ "kind": "spawn_failed", "detail": io.to_string() }).to_string()
        }
        AdapterError::Runner(RunnerError::OutputTooLarge { .. }) => {
            serde_json::json!({ "kind": "output_too_large" }).to_string()
        }
        AdapterError::IndexUpdating => serde_json::json!({ "kind": "index_updating" }).to_string(),
        AdapterError::Runner(RunnerError::NoMock(_))
        | AdapterError::Refused(_)
        | AdapterError::Parse(_)
        | AdapterError::CommandFailed { .. }
        | AdapterError::Unsupported(_) => serde_json::json!({ "kind": "refused" }).to_string(),
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

/// The per-package refusal of the same gate (`UpdateCandidate.blocked`),
/// from `issue_plan` or `submit` alike: the reason as its bare serde
/// spelling (`"Pinned"`), which `parseUpdateBlocked` in
/// `src/lib/sources.ts` reads back into its own copy.
fn update_blocked_json(reason: canager_core::model::UpdateBlocked) -> String {
    serde_json::json!({ "kind": "update_blocked", "reason": reason }).to_string()
}

/// `update_blocked_json`'s twin for an uninstall the tool will refuse
/// (`InstalledArtifact.uninstall_blocked`), from `issue_plan` or `submit`
/// alike. A kind of its own, not `update_blocked` with a flag, because the
/// front end words the two differently: `parseUninstallBlocked` in
/// `src/lib/sources.ts` reads this one, for the uninstall dialog.
fn uninstall_blocked_json(reason: canager_core::model::UninstallBlocked) -> String {
    serde_json::json!({ "kind": "uninstall_blocked", "reason": reason }).to_string()
}

/// `plan_operation_error`'s counterpart for `Session::submit`.
///
/// Every variant goes out as the same small JSON envelope
/// `not_actionable_json` uses (`{"kind": "..."}`, plus whatever extra
/// fields that one kind carries), so `src/lib/sources.ts`'s
/// `planErrorMessage` never shows a bare English sentence to someone who
/// does not read English. `Unknown` and `Expired` used to keep
/// `SubmitError`'s own `Display` verbatim on the theory that it was
/// "already a sentence a person can read" -- but it is a sentence in this
/// project's *source* language, and a zh-CN user previewing an uninstall
/// that outlives the 10-minute window read this project's own English
/// back at them. That is the same class of bug `NotActionable`'s JSON
/// treatment fixed below: a `{:?}` of two Rust enums and a hardcoded
/// English sentence are both "not this user's language" from the front
/// end's point of view. `SourceGone`, `Expired` and `Unknown` all carry no
/// extra fields, so each gets its own bare `kind` rather than a
/// `not_actionable` with two nulls, which would decode to an empty
/// message.
fn submit_operation_error(e: canager_core::session::SubmitError) -> String {
    match e {
        canager_core::session::SubmitError::NotActionable {
            read_only,
            unavailable,
        } => not_actionable_json(read_only, unavailable),
        canager_core::session::SubmitError::UpdateBlocked { reason } => update_blocked_json(reason),
        canager_core::session::SubmitError::UninstallBlocked { reason } => {
            uninstall_blocked_json(reason)
        }
        canager_core::session::SubmitError::SourceGone => {
            serde_json::json!({ "kind": "source_gone" }).to_string()
        }
        canager_core::session::SubmitError::Expired => {
            serde_json::json!({ "kind": "expired" }).to_string()
        }
        canager_core::session::SubmitError::Unknown => {
            serde_json::json!({ "kind": "unknown" }).to_string()
        }
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

/// A `NoCancel` op that is Running refuses, as `{"kind":"no_cancel"}` in
/// the JSON shape `submit_operation_error` uses; one still Queued is
/// cancelled like any other. The front end's only Cancel control
/// (`OperationBar.tsx`, the one caller of `useCancelOperation`) is not
/// offered for a Running `NoCancel` op, reading `OpSummary.cancel_policy`
/// and `status`, so this is the backstop and nothing there words it. A
/// cancel that finds nothing pending (the op finished first, or a cancel
/// is already in flight) stays `Ok(())`, as it always was: losing that
/// race is not an error.
pub(crate) fn cancel_operation_impl(state: &AppState, op_id: u64) -> Result<(), String> {
    match state.session.cancel(op_id) {
        Ok(()) | Err(CancelRefused::NotPending) => Ok(()),
        Err(CancelRefused::NoCancel) => Err(serde_json::json!({ "kind": "no_cancel" }).to_string()),
    }
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
    state.set_settings(settings).map_err(settings_save_error)
}

/// Why writing the settings file failed, as a `{"kind":
/// "settings_save_failed", "reason": ...}` payload `src/lib/sources.ts`'s
/// `settingsSaveErrorMessage` reads -- the same envelope every other
/// refusal in this file uses. It used to be the `io::Error`'s `Display`,
/// which reached a zh-CN user as English inside the translated "couldn't
/// save that change" frame.
///
/// The three reasons a person can do something about get a `reason` the
/// front end words itself. Anything else is `other`, with the operating
/// system's own description kept verbatim in `detail` for the front end to
/// quote: that is the OS's text, not Canager's, and there is no honest way
/// to translate a reason Canager did not anticipate.
fn settings_save_error(e: std::io::Error) -> String {
    use std::io::ErrorKind;
    let reason = match e.kind() {
        ErrorKind::PermissionDenied => "permission_denied",
        ErrorKind::StorageFull => "disk_full",
        ErrorKind::ReadOnlyFilesystem => "read_only",
        _ => "other",
    };
    let mut payload = serde_json::json!({ "kind": "settings_save_failed", "reason": reason });
    if reason == "other" {
        payload["detail"] = serde_json::Value::String(e.to_string());
    }
    payload.to_string()
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

/// The unknown-source scan over the session's current snapshot
/// (`Session::scan_unknown`), for the `HostEnv` the caller read. The
/// command reads it fresh, as `refresh_impl` does, so the directory list
/// follows the `PATH` this process was launched with; the test passes one
/// that keeps the scan off the developer's own home.
pub(crate) fn scan_unknown_impl(session: &Session, env: &HostEnv) -> UnknownScan {
    session.scan_unknown(env)
}

#[tauri::command]
pub async fn scan_unknown(state: State<'_, AppState>) -> Result<UnknownScan, String> {
    // On the blocking pool, as `open_ollama_app` is: the scan is
    // synchronous file-system work bounded by `ScanBudget::default()` --
    // up to ten seconds by design -- and running it inline would hold one
    // of the async runtime's worker threads, the ones every other command
    // and the refresh run on, for that long. `State` cannot move into the
    // task; the `Arc<Session>` inside it can.
    let session = state.session.clone();
    let env = HostEnv::discover();
    tauri::async_runtime::spawn_blocking(move || scan_unknown_impl(&session, &env))
        .await
        // Only a panic inside the scan reaches this arm. The text is the
        // front end's to show verbatim, the way a failed load shows the
        // backend's own words under `emptyStates.loadFailed`; it is the
        // runtime's sentence, not one of Canager's to translate.
        .map_err(|e| e.to_string())
}

/// The icon Finder shows for the app a Homebrew cask installed, for that
/// cask's row: a `data:image/png;base64,...` URL, or `None` for any other
/// row and whenever there is no icon to show (`Session::artifact_icon`).
///
/// Takes a key and nothing else: the window sends no path, here as in
/// every command in this file (README, "What makes it safe"). Which
/// folder's icon is drawn is decided on this side, from the current
/// snapshot's own row for that key -- the `.app` Homebrew reported -- and
/// no field of the key is ever read as a path. `icons` is the in-memory
/// cache `run()` manages beside `AppState`; the tests hand in one that
/// draws with a mock.
pub(crate) fn artifact_icon_impl(
    session: &Session,
    icons: &canager_core::icon::AppIcons,
    key: &canager_core::model::ArtifactKey,
) -> Option<String> {
    session.artifact_icon(icons, key)
}

#[tauri::command]
pub async fn artifact_icon(
    state: State<'_, AppState>,
    icons: State<'_, std::sync::Arc<canager_core::icon::AppIcons>>,
    key: canager_core::model::ArtifactKey,
) -> Result<Option<String>, String> {
    // On the blocking pool, as `scan_unknown` is: an `lstat`, and on a
    // folder's first request an AppKit drawing (`icon::RealIconRenderer`,
    // which says why that is allowed off the main thread), neither of which
    // may hold one of the async runtime's worker threads.
    let session = state.session.clone();
    let icons = icons.inner().clone();
    tauri::async_runtime::spawn_blocking(move || artifact_icon_impl(&session, &icons, &key))
        .await
        // Only a panic while drawing reaches this arm: the runtime's
        // sentence, as in `scan_unknown`, and no icon for that row.
        .map_err(|e| e.to_string())
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
        OpStatus, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
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
        /// How long `execute()` takes before it reports success, so a
        /// test can act on an op while it is still `Running` (or, with two
        /// ops on the one instance, while the second is still `Queued`).
        /// Only the two NoCancel cancel tests set it; every other fixture
        /// keeps it zero and `execute()` returns at once.
        execute_delay: std::time::Duration,
        /// What every plan this adapter builds says about Cancel. Only
        /// `test_cancel_operation_impl_refuses_a_running_no_cancel_op_such_as_rustup_self_update`
        /// and `test_cancel_operation_impl_cancels_a_queued_no_cancel_op_such_as_rustup_self_update`
        /// set `NoCancel`; every other fixture keeps `KillThenReconcile`.
        cancel_policy: CancelPolicy,
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
                action: PlanAction::Command {
                    program: inst.exe_path.clone(),
                    args: vec!["do".to_string(), req.name.clone()],
                    env: vec![],
                },
                needs_password: false,
                locks: vec![ResourceLock(inst.id.clone())],
                cancel_policy: self.cancel_policy,
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
            if !self.execute_delay.is_zero() {
                tokio::time::sleep(self.execute_delay).await;
            }
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

    #[tokio::test]
    async fn test_scan_unknown_impl_reads_the_session_and_never_refreshes_it() {
        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh");
        let generation = state.session.snapshot().generation;
        // No `PATH` entries and a home that does not exist: of the scan's
        // candidate directories only `/usr/local/bin` can be read on the
        // machine running this, and reading is all that happens to it.
        // The command itself passes `HostEnv::discover()`.
        let env = HostEnv {
            path_dirs: Vec::new(),
            home: std::env::temp_dir().join(format!("canager-ipc-scan-{}", std::process::id())),
            euid: 0,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };

        let scan = scan_unknown_impl(&state.session, &env);

        // The fake instance's executable is `/bin/true`, which no scanned
        // directory holds, so attribution is not the subject here --
        // `session/scan.rs` proves that on a synthetic home. This proves
        // the shell-level contract: a scan is a read of the session, never
        // a refresh, and everything it lists or claims it also examined.
        assert_eq!(state.session.snapshot().generation, generation);
        let examined: u64 = scan.scanned.iter().map(|dir| u64::from(dir.entries)).sum();
        assert!(
            u64::from(scan.attributed) + scan.entries.len() as u64 <= examined,
            "{scan:?}"
        );
        assert!(
            scan.scanned.iter().all(|dir| !dir.path.starts_with("~")),
            "nothing under the non-existent home was read: {:?}",
            scan.scanned
        );
    }

    /// Like `state_with_fake_adapter`, but also returns a counter of how
    /// many times the fake adapter's `execute()` actually ran, and accepts
    /// an injectable clock — needed only by the plan-rejection tests below,
    /// which must prove a rejected `submit` never reaches the runner and
    /// must simulate a plan aging past its expiry window (F1 in the design
    /// review). `state_with_fake_adapter` above delegates to this with
    /// `None`, and this delegates to `state_with_fake_adapter_and_policy`
    /// with `KillThenReconcile`, so there is exactly one place that builds
    /// this fixture.
    fn state_with_fake_adapter_and_now(
        now_fn: Option<fn() -> i64>,
    ) -> (AppState, Arc<AtomicUsize>, Arc<Mutex<Vec<CheckOptions>>>) {
        state_with_fake_adapter_and_policy(
            now_fn,
            CancelPolicy::KillThenReconcile,
            std::time::Duration::ZERO,
        )
    }

    /// Where `state_with_fake_adapter` and `state_with_fake_adapter_and_now`
    /// both end up; see the latter for the counter and the clock.
    /// `cancel_policy` is what every plan the fake builds will say about
    /// Cancel, and `execute_delay` how long the fake's `execute()` takes
    /// (see the field).
    ///
    /// `session` and `channel_sink` share the *same* `ChannelSink` (N1 in
    /// the design review): the two used to be built from separate
    /// `ChannelSink::new()` calls, which meant a real operation's events —
    /// emitted into `session`'s sink — could never reach a Channel
    /// registered through `AppState.channel_sink`, and no test caught it
    /// because every test only ever broadcast directly on `channel_sink`
    /// rather than checking that a *real* operation's events arrive.
    fn state_with_fake_adapter_and_policy(
        now_fn: Option<fn() -> i64>,
        cancel_policy: CancelPolicy,
        execute_delay: std::time::Duration,
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
            execute_delay,
            cancel_policy,
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
            rounds: std::sync::Mutex::new(Default::default()),
            notified: std::sync::Mutex::new(Default::default()),
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
            execute_delay: std::time::Duration::ZERO,
            cancel_policy: CancelPolicy::KillThenReconcile,
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
            rounds: std::sync::Mutex::new(Default::default()),
            notified: std::sync::Mutex::new(Default::default()),
        })
    }

    /// Like `state_with_fake_adapter_and_now`, but the `Session`'s own
    /// `background_change` -- the one `ipc::refresh_on_background_change`
    /// loops on -- is wired to `background_change` itself, via
    /// `canager_core::testing::session_with_background_change`
    /// (`Session::with_adapters` wires it to a `Notify` nobody outside the
    /// session ever gets a handle to). A test can then wake it directly
    /// with `background_change.notify_one()`, standing in for a real
    /// `BrewAdapter`'s own clone of the same `Notify` firing when a `brew
    /// update` a refresh left running ends.
    ///
    /// `detect_delay` is how long every refresh's detection takes, so a
    /// test can wake the loop while a refresh is still in flight.
    fn state_with_fake_adapter_and_background_change(
        background_change: Arc<tokio::sync::Notify>,
        detect_delay: std::time::Duration,
    ) -> (Arc<AppState>, Arc<Mutex<Vec<CheckOptions>>>) {
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
        let check_options_calls = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: Arc::new(AtomicUsize::new(0)),
            check_options_calls: check_options_calls.clone(),
            detect_delay,
            execute_delay: std::time::Duration::ZERO,
            cancel_policy: CancelPolicy::KillThenReconcile,
        });
        let sink = ChannelSink::new();
        let session = canager_core::testing::session_with_background_change(
            sink.clone(),
            vec![adapter],
            background_change,
        );
        let state = Arc::new(AppState {
            session,
            settings_path: temp_settings_path("ipc-background-change"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
            rounds: std::sync::Mutex::new(Default::default()),
            notified: std::sync::Mutex::new(Default::default()),
        });
        (state, check_options_calls)
    }

    #[tokio::test]
    async fn test_refresh_on_background_change_refreshes_exactly_once_and_broadcasts_snapshot_changed(
    ) {
        // `ipc::refresh_on_background_change` had no test at all. This
        // proves the one thing it exists for: a single background change
        // -- standing in for a real `brew update` a refresh left running
        // finally ending -- makes the loop refresh exactly once (not zero,
        // the loop never having woken; not more than once, an extra spurious
        // wake or a loop that free-runs instead of re-`.await`ing
        // `background_change()`), and that the refresh reaches the window
        // the same way any other one does: a `SnapshotChanged` broadcast.
        let background_change = Arc::new(tokio::sync::Notify::new());
        let (state, check_options_calls) = state_with_fake_adapter_and_background_change(
            background_change.clone(),
            std::time::Duration::ZERO,
        );

        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let loop_state = state.clone();
        let handle = tokio::spawn(async move { refresh_on_background_change(&loop_state).await });

        // No race to guard here: `Notify::notify_one` stores a permit when
        // nobody is waiting yet, which the loop's first
        // `background_change().await` then consumes immediately -- so this
        // works whether or not the spawned task has reached that await
        // point by the time this runs.
        background_change.notify_one();

        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if !check_options_calls.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("a background change must make refresh_on_background_change refresh");

        // Give any *unwanted* extra wake a chance to land too, so "exactly
        // one" below is not just "at least one, measured too early".
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(
            check_options_calls.lock().unwrap().len(),
            1,
            "one background change must refresh exactly once"
        );
        let events = received.lock().unwrap().clone();
        assert_eq!(
            events.len(),
            1,
            "and broadcast exactly one SnapshotChanged: {events:?}"
        );
        match &events[0] {
            UiEvent::SnapshotChanged { .. } => {}
            other => panic!("expected SnapshotChanged, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_refresh_on_background_change_runs_its_own_refresh_when_one_is_in_flight() {
        // Rereview F1. `Session::refresh` used to merge a call that
        // arrives while a refresh is running into that refresh. When the
        // wake-up (a `brew update` ending) lands mid-refresh, the refresh
        // in flight is the one that saw the update running, so the loop
        // got its stale snapshot back and never read the source again.
        // Here a refresh is in flight (detection takes 600 ms) when the
        // loop is woken; the loop must still get a round that starts after
        // the wake-up.
        let background_change = Arc::new(tokio::sync::Notify::new());
        let (state, check_options_calls) = state_with_fake_adapter_and_background_change(
            background_change.clone(),
            std::time::Duration::from_millis(600),
        );
        let loop_state = state.clone();
        let handle = tokio::spawn(async move { refresh_on_background_change(&loop_state).await });

        let in_flight_state = state.clone();
        let in_flight = tokio::spawn(async move { refresh_impl(&in_flight_state).await });
        // Well inside the 600 ms detection: the refresh holds the gate.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            check_options_calls.lock().unwrap().is_empty(),
            "the first refresh must still be in flight when the loop wakes"
        );
        background_change.notify_one();
        in_flight
            .await
            .expect("in-flight task")
            .expect("in-flight refresh");

        let reached_two = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if check_options_calls.lock().unwrap().len() >= 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await;
        // Room for an unwanted third round to show up too.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let rounds = check_options_calls.lock().unwrap().len();
        assert!(
            reached_two.is_ok() && rounds == 2,
            "the in-flight refresh and exactly one of the loop's own: got {rounds} \
             (1 means the loop's refresh was merged into the one in flight)"
        );

        handle.abort();
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

        // Two calls coalesce when both arrive while an older round is in
        // flight: `Session::refresh` answers neither with that round (it
        // started before they arrived), and the first of them to take the
        // gate runs the round both then share.
        let state_leader = state.clone();
        let leader = tokio::spawn(async move { refresh_impl(&state_leader).await });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let state_a = state.clone();
        let state_b = state.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { refresh_impl(&state_a).await }),
            tokio::spawn(async move { refresh_impl(&state_b).await }),
        );
        leader
            .await
            .expect("leader task")
            .expect("refresh_impl leader");
        let snap_a = a.expect("task a").expect("refresh_impl a");
        let snap_b = b.expect("task b").expect("refresh_impl b");
        assert_eq!(
            snap_a, snap_b,
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
        // M9 in the design review: `refresh_for`, which `refresh_impl` runs
        // for the window, is the only production code path in the whole
        // plan that ever sends `UiEvent::SnapshotChanged`, and for the
        // window only when the refresh actually moved `generation`. Subscribe *first*
        // (every other test that refreshes either has no subscriber or
        // subscribes after its last refresh, which is why inverting or
        // dropping the generation-diff branch used to leave the whole
        // suite green), then refresh a fresh
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
    async fn test_a_round_of_the_daily_check_is_announced_even_when_its_generation_is_unchanged() {
        // The exception to the test above: the page must fetch every round
        // of the daily check, one that found nothing new included, to
        // report the updates it offers with its round's number
        // (`notify::report_update_set`).
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

        let daily = refresh_as(&state, RoundTrigger::Automatic)
            .await
            .expect("the daily check's refresh");
        assert_eq!(
            first.generation, daily.generation,
            "precondition: identical content must not move the generation"
        );
        assert!(daily.round > first.round, "a round of its own");
        // And a round of the window's that changes nothing is still quiet.
        refresh_impl(&state).await.expect("third refresh_impl");

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
            vec![daily.generation],
            "the daily check's round, and it alone, announced: {events:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_refresh_as_records_who_asked_before_the_round_can_be_seen() {
        // The page may fetch the snapshot at any moment (`get_snapshot`)
        // and report its round (`notify::report_update_set`), which looks
        // up who asked for it. So the round is recorded before it is
        // committed: while nothing can record it -- another thread holds
        // the log -- nothing can see it either.
        let state = Arc::new(state_with_fake_adapter());
        let (locked, is_locked) = std::sync::mpsc::channel::<()>();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let holder = {
            let state = state.clone();
            std::thread::spawn(move || {
                let _log = state.rounds.lock().unwrap();
                locked.send(()).expect("the test is waiting");
                released.recv().ok();
            })
        };
        is_locked.recv().expect("the log is held");

        let refreshing = {
            let state = state.clone();
            tokio::spawn(async move { refresh_as(&state, RoundTrigger::Automatic).await })
        };
        // Long past the fake source's answer: the round has run as far as
        // it can.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(
            state.session.snapshot().round,
            0,
            "the round was committed before who asked for it was recorded"
        );

        release.send(()).expect("the holder is waiting");
        holder.join().expect("holder thread");
        let snapshot = refreshing.await.expect("refresh task").expect("refresh_as");
        assert_eq!(snapshot.round, 1);
        assert_eq!(state.session.snapshot().round, 1);
        assert_eq!(
            state.rounds.lock().unwrap().trigger_of(1),
            Some(RoundTrigger::Automatic)
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
        assert_eq!(
            canager_core::testing::command_args(&issued.plan),
            vec!["do".to_string(), "jq".to_string()]
        );
    }

    #[tokio::test]
    async fn test_plan_operation_impl_maps_an_unknown_instance_to_source_gone() {
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
        // The same bare kind `submit_operation_error` sends for a source
        // that vanished, so the front end has one sentence for both -- and
        // never the Rust `Display` ("unknown instance does-not-exist").
        assert_eq!(err, r#"{"kind":"source_gone"}"#);
    }

    #[test]
    fn test_plan_operation_error_never_sends_canagers_own_english() {
        use canager_core::runner::RunnerError;
        let parse = |e: AdapterError| -> serde_json::Value {
            let raw = plan_operation_error(e);
            serde_json::from_str(&raw).unwrap_or_else(|_| panic!("not JSON: {raw}"))
        };

        // Canager's own words: the kind and data only, no prose.
        let v = parse(AdapterError::InvalidName("-rf".to_string()));
        assert_eq!(
            v,
            serde_json::json!({ "kind": "invalid_name", "name": "-rf" })
        );
        let v = parse(AdapterError::Refused(
            "no adapter registered for fake".to_string(),
        ));
        assert_eq!(v, serde_json::json!({ "kind": "refused" }));
        let v = parse(AdapterError::Runner(RunnerError::NoMock(vec![])));
        assert_eq!(v, serde_json::json!({ "kind": "refused" }));
        let v = parse(AdapterError::Runner(RunnerError::NotFound(
            std::path::PathBuf::from("/opt/homebrew/bin/brew"),
        )));
        assert_eq!(
            v,
            serde_json::json!({ "kind": "program_missing", "program": "/opt/homebrew/bin/brew" })
        );
        let v = parse(AdapterError::Runner(RunnerError::OutputTooLarge {
            limit: 1024,
        }));
        assert_eq!(v, serde_json::json!({ "kind": "output_too_large" }));
        let v = parse(AdapterError::SourceGone {
            instance_id: "brew:/opt/homebrew".to_string(),
        });
        assert_eq!(v, serde_json::json!({ "kind": "source_gone" }));
        // brew's uninstall preview while `brew update` runs: a state of
        // the Mac the dialog words itself, not Canager's bug.
        let v = parse(AdapterError::IndexUpdating);
        assert_eq!(v, serde_json::json!({ "kind": "index_updating" }));
        // A pinned package on a stale Updates page: the reason as data,
        // for `parseUpdateBlocked` in src/lib/sources.ts to word.
        let v = parse(AdapterError::UpdateBlocked {
            reason: canager_core::model::UpdateBlocked::Pinned,
        });
        assert_eq!(
            v,
            serde_json::json!({ "kind": "update_blocked", "reason": "Pinned" })
        );
        // A pinned package on a stale Installed page: a kind of its own,
        // for `parseUninstallBlocked` in src/lib/sources.ts to word.
        let v = parse(AdapterError::UninstallBlocked {
            reason: canager_core::model::UninstallBlocked::Pinned,
        });
        assert_eq!(
            v,
            serde_json::json!({ "kind": "uninstall_blocked", "reason": "Pinned" })
        );
        // A path-list uninstall's preview refused one of its checks (phase
        // 4 step C): the path (home folder abbreviated) and the reason, as
        // snake_case data for `parseUninstallUnsafe` in src/lib/sources.ts.
        let v = parse(AdapterError::UninstallUnsafe {
            path: "~/.local/bin/claude".to_string(),
            reason: canager_core::model::UninstallUnsafeReason::NotWhatInstructionsExpect,
        });
        assert_eq!(
            v,
            serde_json::json!({
                "kind": "uninstall_unsafe",
                "path": "~/.local/bin/claude",
                "reason": "not_what_instructions_expect"
            })
        );

        // Errors no `plan()` returns: a broken adapter contract, so
        // Canager's own bug, and none of their text reaches the wire.
        for e in [
            AdapterError::Parse("unexpected token".to_string()),
            AdapterError::CommandFailed {
                code: Some(1),
                stderr: "Error: No such keg".to_string(),
            },
            AdapterError::Unsupported("pip is read-only in Canager".to_string()),
        ] {
            assert_eq!(parse(e), serde_json::json!({ "kind": "refused" }));
        }

        // Another program's words: kept verbatim for the front end to quote.
        let v = parse(AdapterError::Runner(RunnerError::Spawn(
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        )));
        assert_eq!(v["kind"], "spawn_failed");
        assert!(
            v["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "spawn_failed must carry the OS's reason, got {v}"
        );
    }

    #[test]
    fn test_plan_operation_error_spells_each_uninstall_unsafe_reason_in_snake_case() {
        use canager_core::model::UninstallUnsafeReason;
        // Written out by hand in `plan_operation_error`, not derived:
        // `UNINSTALL_UNSAFE_KEYS` in src/lib/sources.ts indexes its copy by
        // these exact strings, and the `match` is exhaustive, so a reason
        // added in Rust fails to compile there until it has a spelling --
        // and the TS `Record` fails `tsc` until it has copy.
        for (reason, spelling) in [
            (UninstallUnsafeReason::OutsideHome, "outside_home"),
            (UninstallUnsafeReason::SharedFolder, "shared_folder"),
            (UninstallUnsafeReason::Missing, "missing"),
            (UninstallUnsafeReason::NotOwnedByYou, "not_owned_by_you"),
            (
                UninstallUnsafeReason::NotWhatInstructionsExpect,
                "not_what_instructions_expect",
            ),
            (UninstallUnsafeReason::OverlapsKept, "overlaps_kept"),
        ] {
            let raw = plan_operation_error(AdapterError::UninstallUnsafe {
                path: "~/.claude/downloads".to_string(),
                reason,
            });
            let v: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
            assert_eq!(v["kind"], "uninstall_unsafe");
            assert_eq!(v["path"], "~/.claude/downloads");
            assert_eq!(v["reason"], spelling, "{reason:?}");
        }
    }

    #[test]
    fn test_settings_save_error_names_the_reason_and_keeps_only_the_oss_text() {
        use std::io::{Error, ErrorKind};
        let parse = |e: Error| -> serde_json::Value {
            serde_json::from_str(&settings_save_error(e)).unwrap()
        };
        assert_eq!(
            parse(Error::from(ErrorKind::PermissionDenied)),
            serde_json::json!({ "kind": "settings_save_failed", "reason": "permission_denied" })
        );
        assert_eq!(
            parse(Error::from(ErrorKind::StorageFull)),
            serde_json::json!({ "kind": "settings_save_failed", "reason": "disk_full" })
        );
        assert_eq!(
            parse(Error::from(ErrorKind::ReadOnlyFilesystem)),
            serde_json::json!({ "kind": "settings_save_failed", "reason": "read_only" })
        );
        assert_eq!(
            parse(Error::other("Input/output error (os error 5)")),
            serde_json::json!({
                "kind": "settings_save_failed",
                "reason": "other",
                "detail": "Input/output error (os error 5)",
            })
        );
    }

    #[test]
    fn test_set_settings_impl_rejects_with_the_structured_payload() {
        // A settings path whose parent is a regular file: `create_dir_all`
        // cannot succeed, whatever user runs the test.
        let blocker = temp_settings_path("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let state = AppState::new(blocker.join("settings.json"), ChannelSink::new());
        let err = set_settings_impl(&state, Settings::default())
            .expect_err("saving under a file must fail");
        let v: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!(v["kind"], "settings_save_failed");
        std::fs::remove_file(&blocker).ok();
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
            execute_delay: std::time::Duration::ZERO,
            cancel_policy: CancelPolicy::KillThenReconcile,
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
            rounds: std::sync::Mutex::new(Default::default()),
            notified: std::sync::Mutex::new(Default::default()),
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
    fn test_submit_operation_error_sends_structured_json_for_every_variant() {
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

        // The per-package half: the package was pinned after the preview.
        // Same payload as the plan-time refusal, for the same one decoder.
        let blocked = submit_operation_error(SubmitError::UpdateBlocked {
            reason: canager_core::model::UpdateBlocked::Pinned,
        });
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&blocked)
                .unwrap_or_else(|e| panic!("expected JSON, got {blocked:?} ({e})")),
            serde_json::json!({ "kind": "update_blocked", "reason": "Pinned" })
        );
        let blocked = submit_operation_error(SubmitError::UninstallBlocked {
            reason: canager_core::model::UninstallBlocked::Pinned,
        });
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&blocked)
                .unwrap_or_else(|e| panic!("expected JSON, got {blocked:?} ({e})")),
            serde_json::json!({ "kind": "uninstall_blocked", "reason": "Pinned" })
        );

        let gone = submit_operation_error(SubmitError::SourceGone);
        let parsed: serde_json::Value = serde_json::from_str(&gone)
            .unwrap_or_else(|e| panic!("expected JSON, got {gone:?} ({e})"));
        assert_eq!(
            parsed["kind"], "source_gone",
            "a vanished source has no reason to name, so it cannot go out as \
             not_actionable with two nulls -- that decodes to an empty message"
        );

        // `Expired` and `Unknown` used to keep `SubmitError`'s own
        // `Display` -- this project's own English, unlocalised, reaching
        // whichever locale the user is running in. They now go out as the
        // same bare-kind envelope `SourceGone` does, so
        // `src/lib/sources.ts` can render them in the user's language.
        let expired = submit_operation_error(SubmitError::Expired);
        assert!(
            !expired.contains("older than 10 minutes"),
            "must not be the hardcoded English Display any more: {expired}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&expired)
            .unwrap_or_else(|e| panic!("expected JSON, got {expired:?} ({e})"));
        assert_eq!(parsed["kind"], "expired");

        let unknown = submit_operation_error(SubmitError::Unknown);
        assert!(
            !unknown.contains("already submitted"),
            "must not be the hardcoded English Display any more: {unknown}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&unknown)
            .unwrap_or_else(|e| panic!("expected JSON, got {unknown:?} ({e})"));
        assert_eq!(parsed["kind"], "unknown");
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
        assert_eq!(summaries[0].cancel_policy, CancelPolicy::KillThenReconcile);

        // The op finished before this Cancel: a cancel that lost that race
        // is silent, as it always was, not an error.
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

    /// Polls `list_operations_impl` every 10 ms until `op_id`'s summary
    /// says `target`, or panics after `timeout`. The fake's `execute()`
    /// returns at once unless the fixture was given an `execute_delay`, so
    /// a test that needs an op still `Running` or `Queued` sets one and
    /// waits here rather than sleeping a guessed interval.
    async fn wait_for_op_status(
        state: &AppState,
        op_id: u64,
        target: OpStatus,
        timeout: std::time::Duration,
    ) {
        let start = std::time::Instant::now();
        loop {
            let summaries = list_operations_impl(state).expect("list_operations_impl");
            if summaries
                .iter()
                .any(|s| s.id == op_id && s.status == target)
            {
                return;
            }
            assert!(
                start.elapsed() < timeout,
                "timed out after {timeout:?} waiting for op {op_id} to reach {target:?}: {summaries:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn test_cancel_operation_impl_refuses_a_running_no_cancel_op_such_as_rustup_self_update()
    {
        // The summary says `NoCancel`, which `OperationBar.tsx` reads with
        // the status to offer no Cancel button once the op is Running; and
        // if a cancel arrives anyway while it is Running, it is refused as
        // `{"kind":"no_cancel"}`. The op must still be Running when the
        // cancel lands: the fake's `execute()` is held for 300 ms and the
        // status is polled, since a cancel that finds the op finished is
        // `Ok(())` as in the round-trip test above, whatever the plan said.
        let (state, _execute_calls, _check_options_calls) = state_with_fake_adapter_and_policy(
            None,
            CancelPolicy::NoCancel,
            std::time::Duration::from_millis(300),
        );
        refresh_impl(&state).await.expect("refresh_impl");

        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "claude".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        let op_id = submit_operation_impl(&state, issued.id).expect("submit_operation_impl");
        wait_for_op_status(
            &state,
            op_id,
            OpStatus::Running,
            std::time::Duration::from_millis(1000),
        )
        .await;

        let summaries = list_operations_impl(&state).expect("list_operations_impl");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].cancel_policy, CancelPolicy::NoCancel);

        let err = cancel_operation_impl(&state, op_id)
            .expect_err("a Running NoCancel op's cancel must be refused");
        let parsed: serde_json::Value = serde_json::from_str(&err)
            .unwrap_or_else(|e| panic!("expected JSON, got {err:?} ({e})"));
        assert_eq!(parsed["kind"], "no_cancel");
    }

    #[tokio::test]
    async fn test_cancel_operation_impl_cancels_a_queued_no_cancel_op_such_as_rustup_self_update() {
        // A NoCancel op that is still Queued has spawned nothing, so its
        // cancel goes through like any other op's -- `Ok(())`, not
        // `{"kind":"no_cancel"}` -- and it ends `Cancelled` without its
        // command ever running. Two ops on the one instance share its
        // resource lock, so the second sits Queued while the first is held
        // Running by the fake's 300 ms `execute()`.
        let (state, execute_calls, _check_options_calls) = state_with_fake_adapter_and_policy(
            None,
            CancelPolicy::NoCancel,
            std::time::Duration::from_millis(300),
        );
        refresh_impl(&state).await.expect("refresh_impl");

        let mut ids = Vec::new();
        for name in ["claude", "codex"] {
            let req = OpRequest {
                kind: OpKind::Upgrade,
                instance_id: "fake:1".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: name.to_string(),
            };
            let issued = plan_operation_impl(&state, req)
                .await
                .expect("plan_operation_impl");
            ids.push(submit_operation_impl(&state, issued.id).expect("submit_operation_impl"));
        }
        let (first, second) = (ids[0], ids[1]);
        wait_for_op_status(
            &state,
            first,
            OpStatus::Running,
            std::time::Duration::from_millis(1000),
        )
        .await;
        let queued = list_operations_impl(&state)
            .expect("list_operations_impl")
            .into_iter()
            .find(|s| s.id == second)
            .expect("the second op is listed");
        assert_eq!(queued.status, OpStatus::Queued);
        assert_eq!(queued.cancel_policy, CancelPolicy::NoCancel);

        cancel_operation_impl(&state, second).expect("a Queued NoCancel op's cancel goes through");

        wait_for_op_status(
            &state,
            second,
            OpStatus::Done,
            std::time::Duration::from_millis(1000),
        )
        .await;
        let done = list_operations_impl(&state)
            .expect("list_operations_impl")
            .into_iter()
            .find(|s| s.id == second)
            .expect("the second op is still listed");
        assert_eq!(done.outcome, Some(Outcome::Cancelled));
        // Only the first op's command ran; the first is still Running or
        // has since finished, either way its one call is the total.
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            1,
            "a NoCancel op cancelled while Queued must never run its command"
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
        let parsed: serde_json::Value = serde_json::from_str(&err)
            .unwrap_or_else(|e| panic!("expected JSON, got {err:?} ({e})"));
        assert_eq!(
            parsed["kind"], "unknown",
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
        let parsed: serde_json::Value = serde_json::from_str(&err)
            .unwrap_or_else(|e| panic!("expected JSON, got {err:?} ({e})"));
        assert_eq!(
            parsed["kind"], "unknown",
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
        let parsed: serde_json::Value = serde_json::from_str(&err)
            .unwrap_or_else(|e| panic!("expected JSON, got {err:?} ({e})"));
        assert_eq!(
            parsed["kind"], "expired",
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

    /// `artifact_icon`'s inputs, pinned: Tauri's two `State`s and one
    /// `ArtifactKey`, the only thing the window's payload carries -- no
    /// path, no URL, nothing a drawing could be pointed at. Adding one
    /// fails to compile here.
    #[test]
    fn test_artifact_icon_takes_only_a_key_from_the_window() {
        use canager_core::icon::AppIcons;

        fn command_inputs<F, Fut>(_command: F)
        where
            F: Fn(State<'static, AppState>, State<'static, Arc<AppIcons>>, ArtifactKey) -> Fut,
            Fut: std::future::Future<Output = Result<Option<String>, String>>,
        {
        }
        command_inputs(artifact_icon);
        let _: fn(&Session, &AppIcons, &ArtifactKey) -> Option<String> = artifact_icon_impl;
    }

    /// A key that names a path, in any of its fields, is a key the
    /// snapshot has no row for: nothing is drawn, whatever is at that path.
    /// `session::icon`'s tests draw what a cask's own row holds.
    #[tokio::test]
    async fn test_artifact_icon_impl_never_draws_a_path_the_key_names() {
        use canager_core::icon::{AppIcons, MockIconRenderer};

        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh");
        let dir = std::env::temp_dir().join(format!(
            "canager-ipc-icon-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = dir.join("Here.app");
        std::fs::create_dir_all(&app).expect("create an .app folder");
        let renderer = Arc::new(MockIconRenderer::answering(b"\x89PNG\r\n\x1a\n"));
        let icons = AppIcons::new(renderer.clone());

        let mut named = Vec::new();
        for path in [
            app.display().to_string(),
            format!("file://{}", app.display()),
            "/System/Applications/Calculator.app".to_string(),
        ] {
            named.push(ArtifactKey {
                instance_id: "fake:1".to_string(),
                kind: ArtifactKind::Cask,
                name: path.clone(),
            });
            named.push(ArtifactKey {
                instance_id: path.clone(),
                kind: ArtifactKind::Cask,
                name: path,
            });
        }
        for key in named {
            assert_eq!(
                artifact_icon_impl(&state.session, &icons, &key),
                None,
                "{key:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(renderer.calls().is_empty(), "{:?}", renderer.calls());
    }
}
