//! `Session`: the facade `banager-core` exposes to a host shell (the Tauri
//! app in this repo, or a test harness). It owns the registered adapters,
//! the last known set of instances, and an in-memory, generation-numbered
//! `Snapshot`; it forwards operation lifecycle calls to an internal
//! `OperationManager`. Split (Task 14) into this facade, `refresh.rs`
//! (detection and the inventory/updates fetch) and `plans.rs`
//! (preview-then-confirm); no behaviour changed in the split itself.

mod icon;
mod kept;
mod needed_by;
mod plans;
mod refresh;
mod scan;
mod sizes;
/// Re-exported for `crate::testing::expire_issued_plans` alone: how long
/// an issued plan stays submittable, so that helper can age one past it
/// without duplicating the number. Gated the same way that function is --
/// see its doc comment -- so this re-export does not sit unused (and
/// `-D warnings`-fail the build) once it is compiled out.
#[cfg(any(test, feature = "test-support"))]
pub(crate) use plans::PLAN_LIFETIME;
/// Shared `#[cfg(test)]` scaffolding (`non_root_env`/`root_env`, common
/// `FakeAdapter` boilerplate) for the test modules in this file, `plans.rs`
/// and `refresh.rs`. See its module doc for why it exists.
#[cfg(test)]
mod test_support;

// `CheckOptions` and `AdapterError` are deliberately absent from this list:
// `refresh` (which took `CheckOptions`) now lives in `refresh.rs`, and
// `issue_plan`/`submit` (which took `AdapterError`) now live in `plans.rs`;
// each imports what it needs itself. Leaving either here would be an unused
// import under `-D warnings` -- the facade's own top-level code no longer
// touches either type.
use crate::adapters::brew::BrewAdapter;
use crate::adapters::cargo::CargoAdapter;
use crate::adapters::npm::NpmAdapter;
use crate::adapters::ollama::OllamaAdapter;
use crate::adapters::pip::PipAdapter;
use crate::adapters::pipx::PipxAdapter;
use crate::adapters::standalone;
use crate::adapters::uv::UvAdapter;
use crate::adapters::Adapter;
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, RealHttpClient};
use crate::model::{
    AdapterId, InstalledArtifact, InstanceId, ManagerInstance, OpStatus, Plan, ReadOnlyReason,
    Unavailable, UninstallBlocked, UpdateBlocked, UpdateCandidate,
};
use crate::ops::{CancelRefused, Completions, OpSummary, OperationManager};
use crate::runner::{CommandRunner, RealRunner};
use crate::trash::RealTrasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceError {
    pub instance_id: InstanceId,
    pub message: String,
}

/// Whether any adapter detected a usable instance. There used to be a
/// third variant, `RefusedAsRoot`, standing for "every adapter is disabled
/// because Banager is running as root" -- but that was never true: the
/// root objection belongs to `BrewAdapter` alone, and npm, pipx, uv, pip,
/// cargo and ollama have no objection to root at all.
///
/// Under root `BrewAdapter::detect` still contributes an instance, marked
/// `Unavailable::RefusesAsRoot`, so a Mac that has Homebrew reports `Found`
/// and the UI explains the refusal against that one source -- naming the
/// action that fixes it, which is to quit and reopen without `sudo`. An
/// earlier arrangement returned no instance under root, which is
/// indistinguishable from Homebrew not being installed and left such a
/// machine reading "none of them are set up on this Mac yet" while
/// Homebrew sat there installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectOutcome {
    Found,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: u64,
    /// The number of the refresh round that committed this snapshot, the
    /// one `refresh_with_round` hands back with it: 0 for
    /// `Snapshot::empty()`, before any round has. Each round has a higher
    /// one than the last, whether or not it moved `generation`, so unlike
    /// `generation` it names the round a snapshot came from -- the number
    /// `auto_check::RoundLog` records who asked for each round by. The page
    /// reports the updates it offers with it after each snapshot
    /// (`report_update_set` in src-tauri/src/notify.rs), which looks up
    /// whether the daily check asked for that round, and tells by it which
    /// of two snapshots is the later (`isNewerSnapshot` in
    /// src/lib/events.ts): rounds commit in the order they are numbered,
    /// whatever the clock stamped on them as `refreshed_at`.
    pub round: u64,
    pub detect: DetectOutcome,
    pub instances: Vec<ManagerInstance>,
    pub artifacts: Vec<InstalledArtifact>,
    pub updates: Vec<UpdateCandidate>,
    /// Unix seconds of the last refresh that *ran*, successful or not
    /// (spec §2.4-1) -- `stale` is what says whether it came back clean.
    /// Gated on success, one permanently broken source left this null for
    /// the life of the machine, and `SnapshotStatus` reads a null
    /// timestamp as "Banager has never finished a check". `None` now means
    /// only that: `Snapshot::empty()`, before the first refresh commits.
    pub refreshed_at: Option<i64>,
    /// True when part of the newest refresh attempt failed, so this data
    /// is older than it looks (spec §3: keep old data, mark it possibly
    /// stale). For an instance an operation is holding, "newest" means
    /// the last attempt that reached it: a refresh that skips it carries
    /// its errors forward with its rows (`refresh.rs`), having retried
    /// nothing. Exactly `!errors.is_empty()`: `errors` says which sources
    /// and why, this says whether to say anything at all, and
    /// `SnapshotStatus` renders the one banner over both.
    ///
    /// Deliberately *not* "or some source is unavailable". That is not a
    /// failed refresh -- the source answered, with the news that it cannot
    /// answer -- and it is already on screen in that source's own words,
    /// on both pages, via `sourceNoticesFor`.
    pub stale: bool,
    pub errors: Vec<SourceError>,
    /// When the daily check is next due, Unix seconds on the wall clock
    /// (`auto_check::next_check_due`): what Settings shows under its
    /// switch, while the switch is on. Never set by `Session`, which knows
    /// nothing of who asked for a round: `None` in every snapshot it
    /// commits, and filled in by the shell from its `auto_check::RoundLog`
    /// as it hands a snapshot to the window (`get_snapshot`, `refresh`).
    /// `None` there too while no round has counted as a check -- the check
    /// is then due at the next look. Not data a round fetched, so not part
    /// of `same_content`.
    #[serde(default)]
    pub next_auto_check_at: Option<i64>,
}

impl Snapshot {
    fn empty() -> Snapshot {
        Snapshot {
            generation: 0,
            round: 0,
            detect: DetectOutcome::Missing,
            instances: Vec::new(),
            artifacts: Vec::new(),
            updates: Vec::new(),
            refreshed_at: None,
            stale: false,
            errors: Vec::new(),
            next_auto_check_at: None,
        }
    }

    /// Whether `self` and `other` carry the same *data* -- every field
    /// except `generation`, `round`, `refreshed_at` and `stale`, which
    /// describe the refresh attempt rather than the fetched data itself --
    /// and, for the same reason, each instance's `answered_at`
    /// (`ManagerInstance::same_content`). A round in which every source
    /// answered just as before still stamps a new time on each, and is
    /// committed with it (its `round` moves on), but it does not move
    /// `generation`: counting the clock as news would announce a changed
    /// snapshot after every check (`SnapshotChanged`), and make every plan
    /// previewed before it go through the actionability gate again on
    /// submit (`Session::submit`).
    ///
    /// So the window can hold an older time than this snapshot's for a
    /// source that keeps answering, and that is never on screen: a time is
    /// shown only for a source that did not answer (`sourceNoticesFor` in
    /// src/lib/sources.ts). Such a source's `answered_at` cannot move --
    /// it is not asked -- and the round it went quiet in changed its
    /// `status`, which is content: that round moves `generation`, and the
    /// window receives the snapshot with the latest time in it.
    fn same_content(&self, other: &Snapshot) -> bool {
        self.detect == other.detect
            && self.instances.len() == other.instances.len()
            && self
                .instances
                .iter()
                .zip(&other.instances)
                .all(|(a, b)| a.same_content(b))
            && self.artifacts == other.artifacts
            && self.updates == other.updates
            && self.errors == other.errors
    }
}

/// What the first refresh round since launch has found installed, handed
/// out (`refresh_recording`'s `preview`, in `refresh.rs`) as soon as every
/// source it asked has listed its packages -- while the update checks, the
/// slow half of a round, still run: Homebrew's `brew update` alone can take
/// two minutes. The window shows the Installed list from it meanwhile,
/// every Update and Uninstall off.
///
/// Deliberately not a `Snapshot`. It is never committed: `snapshot()`,
/// `issue_plan` and every caller waiting on the refresh gate go on seeing
/// what was there before -- on the only round that previews, the startup
/// placeholder -- until the round itself commits. Committing it would mark
/// the round done and hand callers waiting on that round a snapshot with
/// no updates in it. And it says nothing about updates, errors or
/// staleness, so it has no fields for them that a reader could take for
/// "none".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryPreview {
    /// The round it is from, still running: the number that round's
    /// snapshot will carry as `Snapshot::round` when it commits.
    pub round: u64,
    /// Every source this round detected, as detection described it: no
    /// note an update check adds is on them yet, and no `answered_at` --
    /// no round before this one committed one, and this one's are written
    /// as each source's task is joined, after its update check, before the
    /// round commits.
    pub instances: Vec<ManagerInstance>,
    /// What each source that read its list this round listed, in the order
    /// the round asked them. A source whose read failed, or declined
    /// because its catalogue is being rewritten, is not in it.
    pub artifacts: Vec<InstalledArtifact>,
}

/// Opaque handle to a plan `Session` has issued and is holding server-side.
/// The front end never constructs one; it only ever echoes back the `id` it
/// was given.
///
/// A random 128-bit token (32 hex chars via `plans::random_plan_id`), not a
/// sequential counter. The previous `AtomicU64` starting at 1 gave a plan
/// the user previewed and then declined a guessable id that `submit` would
/// still fire on request. That guessability is the whole gap this closes:
/// it does nothing against a fully compromised renderer, which can call
/// `issue_plan` itself and read back the id it was handed, but it does mean
/// a *declined* preview cannot be fired by guessing.
pub type PlanId = String;

/// A `Plan` the server has already computed and stored, returned to the
/// caller for preview. Submitting requires only the `id`; the `plan` field
/// is for display (exact command preview, spec §6) and is never accepted
/// back from the client (see `Session::submit`, in `plans.rs`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedPlan {
    pub id: PlanId,
    pub plan: Plan,
    /// Unix seconds when this plan was issued, used to decide expiry.
    pub issued_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SubmitError {
    /// `plan_id` was never issued, or was already consumed by an earlier
    /// `submit` of the same preview. This `Display` is for developer-facing
    /// contexts (logs, panics, other tests in this crate); the shell never
    /// sends it to the front end verbatim -- `submit_operation_error` in
    /// `src-tauri/src/ipc.rs` turns it into `{"kind": "unknown"}` so
    /// `src/lib/sources.ts` can show `planRefused.unknown` in the user's
    /// own language instead of this project's own English.
    #[error("no such plan, or it was already submitted")]
    Unknown,
    /// `plan_id` is still stored but was issued more than the plan's
    /// lifetime ago. Same note as `Unknown` above: `submit_operation_error`
    /// sends `{"kind": "expired"}`, not this `Display`.
    #[error("this plan is older than 10 minutes; preview it again")]
    Expired,
    /// The snapshot moved between issuing this plan and submitting it, and
    /// the instance it targets no longer passes the actionability gate
    /// (spec §2.5) that `issue_plan` checked. Carries the same two axes
    /// `AdapterError::NotActionable` does, so the shell can render it
    /// through the same localised copy rather than as a Rust enum -- see
    /// `submit_operation_error` in `src-tauri/src/ipc.rs`.
    #[error("that source cannot run this any more: read_only={read_only:?}, unavailable={unavailable:?}")]
    NotActionable {
        read_only: Option<ReadOnlyReason>,
        unavailable: Option<Unavailable>,
    },
    /// The per-package half of the same re-check: a refresh since the
    /// preview says the tool will now refuse to update this package (it was
    /// pinned in the meantime). Carries the same reason
    /// `AdapterError::UpdateBlocked` does, and goes out through the same
    /// `update_blocked` payload (`submit_operation_error` in
    /// src-tauri/src/ipc.rs).
    #[error("the tool will refuse to update this package now ({reason:?})")]
    UpdateBlocked { reason: UpdateBlocked },
    /// The same re-check for an `Uninstall`: a refresh since the preview
    /// says the tool will now refuse to uninstall this package (it was
    /// pinned in the meantime). Goes out as the `uninstall_blocked`
    /// payload `AdapterError::UninstallBlocked` uses
    /// (`submit_operation_error` in src-tauri/src/ipc.rs).
    #[error("the tool will refuse to uninstall this package now ({reason:?})")]
    UninstallBlocked { reason: UninstallBlocked },
    /// The same check, for the case where the instance is not in the
    /// current snapshot at all: the source was uninstalled, or the last
    /// detection stopped reporting it. There is no read-only/unavailable
    /// reason to give because there is no instance left to ask.
    #[error("the source this was prepared for is no longer there")]
    SourceGone,
}

pub struct Session {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    ops: Arc<OperationManager>,
    /// Serialises refresh rounds (in `refresh.rs`): `refresh_round` takes
    /// the guard by value and holds it through its commit. A caller waiting
    /// here either runs the next round or, once it holds the gate, re-reads
    /// `snapshot` -- only when `last_committed_round` says a round that
    /// started after it arrived has committed.
    refresh_gate: tokio::sync::Mutex<()>,
    snapshot: Mutex<Snapshot>,
    /// How many refresh rounds have begun. Bumped by `refresh_round`, under
    /// `refresh_gate`, before the round reads anything; `refresh` reads it
    /// on arrival, so a round numbered above that reading began after the
    /// caller arrived.
    rounds_started: AtomicU64,
    /// The number `rounds_started` gave the round whose snapshot is
    /// `snapshot` now. Written only by `commit`, under `refresh_gate`, and
    /// on every commit whether or not `generation` moved: a waiter compares
    /// it with its arrival reading of `rounds_started`, and comparing
    /// `generation` instead would miss a round that found nothing new (M5
    /// in the design review) and make the waiter run a redundant one.
    last_committed_round: AtomicU64,
    /// How many `refresh` calls are under way: counted from the moment one
    /// arrives, before it queues on `refresh_gate`, until it returns or is
    /// dropped (`UnderWay`, in `refresh.rs`). Read by `busy`.
    refreshes_under_way: AtomicUsize,
    /// Plans handed out by `issue_plan` (in `plans.rs`) but not yet
    /// consumed by `submit`, keyed by `PlanId`. The stored value is
    /// `plans::StoredPlan`, not the `IssuedPlan` the caller previews: the
    /// snapshot generation a plan was built against is server-side
    /// bookkeeping that the wire type has no business carrying.
    ///
    /// `pub(crate)` only under `cfg(any(test, feature = "test-support"))`,
    /// so `crate::testing::expire_issued_plans` (gated the same way) can
    /// age entries in test builds: expiry is monotonic on purpose, so a
    /// test in another crate has no clock it can move instead. In a
    /// release build neither that function nor this widened visibility
    /// exist -- the field is private to `session` and its submodules,
    /// which is all production code needs.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) issued_plans: Mutex<HashMap<PlanId, plans::StoredPlan>>,
    #[cfg(not(any(test, feature = "test-support")))]
    issued_plans: Mutex<HashMap<PlanId, plans::StoredPlan>>,
    now_fn: Option<fn() -> i64>,
    /// Turns among the plans being worked out (`plans::PLANS_AT_ONCE`).
    planning: tokio::sync::Semaphore,
    /// See `background_change`. Handed to the adapters that can change
    /// state on their own (Homebrew's background `brew update`) by
    /// `Session::new`.
    background_change: Arc<tokio::sync::Notify>,
    /// Whether the `PATH` this process has is the one the user's login
    /// shell exports: `note_login_path`. Read by every refresh round,
    /// which says which copy of a command runs only while it holds.
    login_path: std::sync::atomic::AtomicBool,
    /// Set while the blocking half of a round's command check is running
    /// (`commands::start_reading`, `commands::finish`), so that one stuck
    /// on a folder that stopped answering is not joined by another each
    /// round.
    commands_in_flight: Arc<std::sync::atomic::AtomicBool>,
    /// What the last round made of `PATH`'s folders (`commands::finish`),
    /// for `get_system_facts`: `None` until a round read them in full.
    path_folders: Mutex<Option<crate::diagnostics::PathFolders>>,
    /// Measures how much disk each installed thing takes after every round
    /// commits (`refresh_recording`, `sizes.rs`), or `None`: on in
    /// `Session::new` and `with_adapters_and_sizes`, off for every other
    /// test seam, so that a test refreshing a fake source never walks a
    /// folder it did not make.
    sizes: Option<Arc<crate::size::SizeMeter>>,
    /// The home folder the last refresh read, where an uninstall preview
    /// looks for the data the uninstall leaves behind (`kept.rs`); `None`
    /// before the first refresh, and always in a session that does not
    /// measure sizes.
    kept_data_home: Mutex<Option<std::path::PathBuf>>,
    /// The `PATH` and home folder the last refresh read, which a Homebrew
    /// uninstall's preview looks with for the sources that run on the
    /// package (`needed_by.rs`); `None` before the first refresh, and always
    /// in a session that does not measure sizes.
    needed_by_env: Mutex<Option<crate::runner::HostEnv>>,
    /// Where each finished update and uninstall is kept across launches
    /// (`attach_history`), or nothing: the shell attaches the one in
    /// Banager's application data directory as it starts; tests attach
    /// their own or none.
    history: std::sync::OnceLock<Arc<crate::history::HistoryStore>>,
}

impl Session {
    /// Registers all nine adapters over a shared `RealRunner` and
    /// `RealHttpClient` (network-touching adapters only: pipx, cargo,
    /// ollama, and the standalone tools' update checks), and gives the
    /// standalone tools the real Trash (`RealTrasher`) for their path-list
    /// uninstalls. `now_fn` exists
    /// so tests can pin `refreshed_at`; production passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::new());
        let http: Arc<dyn HttpClient> = Arc::new(RealHttpClient::new());
        let background_change = Arc::new(tokio::sync::Notify::new());
        let mut adapters: Vec<Arc<dyn Adapter>> = vec![
            Arc::new(
                BrewAdapter::new(runner.clone()).with_background_change(background_change.clone()),
            ),
            Arc::new(NpmAdapter::new(runner.clone())),
            Arc::new(PipxAdapter::new(runner.clone(), http.clone())),
            Arc::new(UvAdapter::new(runner.clone())),
            Arc::new(PipAdapter::new(runner.clone())),
            Arc::new(CargoAdapter::new(runner.clone(), http.clone())),
            Arc::new(OllamaAdapter::new(runner.clone(), http.clone())),
        ];
        // The tools with their own installer: one adapter per recipe
        // (`standalone-claude` in phase 4 step B), over the same runner and
        // client. Listed after the seven package managers only as reading
        // order; the refresh fans out alphabetically by id regardless
        // (`refresh_round`).
        // The one thing in this crate that moves a file itself: macOS's own
        // "move to Trash", for a confirmed path-list uninstall
        // (`removal::execute_removal`; docs/what-we-run.md).
        adapters.extend(standalone::all(runner, http, Arc::new(RealTrasher::new())));
        Session::build_with(sink, adapters, now_fn, background_change, true)
    }

    /// Test seam: build a Session over arbitrary adapters. Its
    /// `background_change` is wired to nothing: an adapter a test builds
    /// is given its own `Notify` by the test if it needs one. A test that
    /// instead needs *this* session's own `background_change()` to resolve
    /// -- proving something that waits on it, rather than on an adapter's
    /// copy -- wants `crate::testing::session_with_background_change`.
    pub fn with_adapters(
        sink: Arc<dyn EventSink>,
        adapters: Vec<Arc<dyn Adapter>>,
        now_fn: Option<fn() -> i64>,
    ) -> Arc<Session> {
        Session::build(sink, adapters, now_fn, Arc::new(tokio::sync::Notify::new()))
    }

    /// Test seam: `with_adapters`, with the size measurement on, as
    /// `Session::new` has it -- for a test whose adapters report folders
    /// of its own (`sizes.rs`, and the shell's `get_sizes`).
    pub fn with_adapters_and_sizes(
        sink: Arc<dyn EventSink>,
        adapters: Vec<Arc<dyn Adapter>>,
        now_fn: Option<fn() -> i64>,
    ) -> Arc<Session> {
        Session::build_with(
            sink,
            adapters,
            now_fn,
            Arc::new(tokio::sync::Notify::new()),
            true,
        )
    }

    /// `pub(crate)`, not private: `crate::testing::session_with_background_change`
    /// (a separate module, but the same crate) is the only other caller,
    /// and exists for exactly the reason `with_adapters`'s own doc comment
    /// gives for not wiring a test's `background_change` to anything --
    /// except that the Tauri shell's tests need it wired to a `Notify`
    /// *they* hold, to prove `ipc::refresh_on_background_change` reacts to
    /// it, and `src-tauri` is a different crate that cannot see a private
    /// item here at all, `#[cfg(test)]` or not.
    pub(crate) fn build(
        sink: Arc<dyn EventSink>,
        adapters: Vec<Arc<dyn Adapter>>,
        now_fn: Option<fn() -> i64>,
        background_change: Arc<tokio::sync::Notify>,
    ) -> Arc<Session> {
        Session::build_with(sink, adapters, now_fn, background_change, false)
    }

    /// `build`, with the size measurement on or off (`Session::sizes`):
    /// its rounds tell `sink` as they move (`EventSink::sizes_changed`).
    fn build_with(
        sink: Arc<dyn EventSink>,
        adapters: Vec<Arc<dyn Adapter>>,
        now_fn: Option<fn() -> i64>,
        background_change: Arc<tokio::sync::Notify>,
        measure_sizes: bool,
    ) -> Arc<Session> {
        let sizes = measure_sizes.then(|| {
            let sink = sink.clone();
            crate::size::SizeMeter::new(crate::size::SizeBudget::default(), move |round| {
                sink.sizes_changed(round)
            })
        });
        let mut ops = OperationManager::new(sink);
        let mut by_id = HashMap::new();
        for adapter in adapters {
            let id = adapter.meta().id.clone();
            // Both halves of what `model::instance_id` relies on to keep
            // instance ids unique across adapters. A duplicate adapter id
            // used to replace the earlier adapter here without a word;
            // this is a programming error in the fixed registration list
            // (or a test's), never something a user's machine can cause,
            // so it fails at construction rather than at some later
            // refresh.
            assert!(
                !id.contains(':'),
                "adapter id {id:?} must not contain ':' (see model::instance_id)"
            );
            ops.register_adapter(adapter.clone());
            let replaced = by_id.insert(id.clone(), adapter);
            assert!(
                replaced.is_none(),
                "two adapters registered with the same id {id:?}"
            );
        }
        Arc::new(Session {
            adapters: by_id,
            ops: Arc::new(ops),
            refresh_gate: tokio::sync::Mutex::new(()),
            snapshot: Mutex::new(Snapshot::empty()),
            rounds_started: AtomicU64::new(0),
            last_committed_round: AtomicU64::new(0),
            refreshes_under_way: AtomicUsize::new(0),
            issued_plans: Mutex::new(HashMap::new()),
            now_fn,
            planning: tokio::sync::Semaphore::new(plans::PLANS_AT_ONCE),
            background_change,
            login_path: std::sync::atomic::AtomicBool::new(true),
            commands_in_flight: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            path_folders: Mutex::new(None),
            sizes,
            kept_data_home: Mutex::new(None),
            needed_by_env: Mutex::new(None),
            history: std::sync::OnceLock::new(),
        })
    }

    /// Keeps every operation submitted from now on in `store`
    /// (`plans.rs`, `submit`). Once: a second store is ignored.
    pub fn attach_history(&self, store: Arc<crate::history::HistoryStore>) {
        let _ = self.history.set(store);
    }

    /// The history the window lists (`get_history`), or an empty one when
    /// none is attached.
    pub fn history(&self) -> crate::history::HistoryView {
        match self.history.get() {
            Some(store) => store.view(),
            None => crate::history::HistoryView {
                run: String::new(),
                cleared_before: None,
                records: Vec::new(),
            },
        }
    }

    /// Waits, at most `timeout`, for the history file to have every record
    /// so far (`HistoryStore::flush`, which tries a failed write once more);
    /// true when none is attached. Called once, as Banager exits.
    pub fn flush_history(&self, timeout: std::time::Duration) -> bool {
        match self.history.get() {
            Some(store) => store.flush(timeout),
            None => true,
        }
    }

    /// The Updates page's Clear, kept (`HistoryStore::clear`).
    pub fn clear_history(&self) -> crate::history::HistoryView {
        match self.history.get() {
            Some(store) => store.clear(),
            None => self.history(),
        }
    }

    /// Says whether the login shell's `PATH` has been read
    /// (`runner::login_path::LoginPath::ensure`, which the Tauri shell asks
    /// before every refresh, `AppState::read_login_path`); the process's
    /// own small `PATH` stands in while it has not. While it has not, no
    /// refresh says which copy of a command runs
    /// (`commands::judge`): against the `PATH` an app opened from Finder
    /// starts with, nearly every tool would read as "not found". Until a
    /// host says otherwise, the `PATH` is taken to be the login shell's --
    /// what a host that never restores it, such as a test, hands in itself.
    pub fn note_login_path(&self, restored: bool) {
        self.login_path.store(restored, Ordering::SeqCst);
    }

    /// What `note_login_path` was last told: whether `PATH` is the login
    /// shell's. Read by `get_system_facts` for the diagnostic text.
    pub fn login_path_restored(&self) -> bool {
        self.login_path.load(Ordering::SeqCst)
    }

    /// What the last refresh round made of `PATH`'s folders when it read
    /// them to say which copy of a command runs: how many it read, and
    /// those it left unread (`commands::finish`). `None` before a round
    /// has, and after one that did not read them in full. Read by
    /// `get_system_facts` for the window's tool setup check.
    pub fn path_folders(&self) -> Option<crate::diagnostics::PathFolders> {
        self.path_folders.lock().unwrap().clone()
    }

    /// Resolves when something a refresh reported has since changed by
    /// itself, so the snapshot is out of date and only a refresh will say
    /// so. Today that is one thing: a `brew update` that a refresh stopped
    /// waiting for, and reported as still running
    /// (`InstanceNote::IndexUpdating`), has ended.
    ///
    /// Nothing in the core refreshes on its own, and the snapshot only
    /// reaches the window through the shell's `refresh`, which is what
    /// announces `SnapshotChanged`. So the shell waits on this in a loop and
    /// refreshes each time it resolves: the notice goes, the new catalogue
    /// shows, and the window hears about it the way it hears about every
    /// other change. A wake-up that arrives while nobody is waiting is
    /// kept, one deep (`Notify::notify_one` stores a permit), so one that
    /// lands mid-refresh is not lost. The refresh it sets off is an
    /// ordinary `refresh`, which never answers with a round that started
    /// before the call arrived -- so not with the round in flight that may
    /// be the very one reporting the update as still running.
    pub async fn background_change(&self) {
        self.background_change.notified().await
    }

    fn now(&self) -> i64 {
        Self::clock(self.now_fn)
    }

    /// `now`, for a task that holds only the clock: a refresh's per-source
    /// task stamps when it asked its source (`answered_at`, refresh.rs).
    fn clock(now_fn: Option<fn() -> i64>) -> i64 {
        match now_fn {
            Some(f) => f(),
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub fn cancel(&self, op_id: OpId) -> Result<(), CancelRefused> {
        self.ops.cancel(op_id)
    }

    pub fn operations(&self) -> Vec<OpSummary> {
        self.ops.summaries()
    }

    /// What the completion notification may count past the operations it
    /// has accepted, `after` (`OperationManager::completions_after`).
    pub fn completions_after(&self, after: OpId) -> Completions {
        self.ops.completions_after(after)
    }

    /// Whether a refresh or an operation is under way: a `refresh` call
    /// running a round or queued behind one, or an operation that is not
    /// `Done` -- queued, running, being cancelled or being verified. The
    /// daily check does nothing while this holds and asks again at its next
    /// tick (`auto_check::tick`). It claims nothing stronger: a refresh can
    /// arrive the moment after this answered.
    pub fn busy(&self) -> bool {
        self.refreshes_under_way.load(Ordering::SeqCst) > 0
            || self
                .ops
                .summaries()
                .iter()
                .any(|op| op.status != OpStatus::Done)
    }

    /// Sorted ids of every adapter this Session has registered, regardless
    /// of whether that adapter currently detects any instance on the host.
    /// A test seam (Task 11) so registration itself is verifiable without
    /// depending on which tools happen to be installed on the machine
    /// running the test.
    pub fn adapter_ids(&self) -> Vec<AdapterId> {
        let mut ids: Vec<AdapterId> = self.adapters.keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::test_support;
    use super::*;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstalledArtifact, OpKind, OpRequest, OpStatus, Outcome,
        Reconciled, SearchHit,
    };
    use crate::runner::HostEnv;
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct FakeState {
        instances: Vec<ManagerInstance>,
        block_execute: bool,
    }

    struct FakeAdapter {
        meta: AdapterMeta,
        state: Arc<StdMutex<FakeState>>,
    }

    impl FakeAdapter {
        fn new() -> (Arc<FakeAdapter>, Arc<StdMutex<FakeState>>) {
            let state = Arc::new(StdMutex::new(FakeState {
                instances: Vec::new(),
                block_execute: false,
            }));
            let adapter = Arc::new(FakeAdapter {
                meta: test_support::fake_adapter_meta("fake"),
                state: state.clone(),
            });
            (adapter, state)
        }
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.state.lock().unwrap().instances.clone()
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
            Ok(test_support::fake_plan(inst, req))
        }

        async fn execute(
            &self,
            _plan: &Plan,
            _sink: Arc<dyn EventSink>,
            _op_id: OpId,
            cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
            let block = self.state.lock().unwrap().block_execute;
            if block {
                cancel.cancelled().await;
                Ok(Outcome::Unconfirmed)
            } else {
                Ok(Outcome::Succeeded)
            }
        }

        async fn reconcile(
            &self,
            _inst: &ManagerInstance,
            _key: &ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(test_support::fake_reconciled())
        }
    }

    #[test]
    fn test_new_registers_all_thirteen_adapters() {
        let sink = Arc::new(VecSink::new());
        let session = Session::new(sink, None);
        assert_eq!(
            session.adapter_ids(),
            vec![
                "brew".to_string(),
                "cargo".to_string(),
                "npm".to_string(),
                "ollama".to_string(),
                "pip".to_string(),
                "pipx".to_string(),
                "standalone-agy".to_string(),
                "standalone-claude".to_string(),
                "standalone-codex".to_string(),
                "standalone-grok".to_string(),
                "standalone-opencode".to_string(),
                "standalone-rustup".to_string(),
                "uv".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_submit_cancel_and_operations_forward_to_the_operation_manager() {
        let (adapter, state) = FakeAdapter::new();
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![test_support::make_instance("fake", "fake:1")];
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if session
                .operations()
                .iter()
                .any(|o| o.id == op_id && o.status == OpStatus::Running)
            {
                break;
            }
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(session.operations().len(), 1);

        session.cancel(op_id).expect("cancel a Running op");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(summary) = session.operations().into_iter().find(|o| o.id == op_id) {
                if summary.status == OpStatus::Done {
                    assert_eq!(summary.outcome, Some(Outcome::Succeeded));
                    break;
                }
            }
            assert!(Instant::now() < deadline, "operation never finished");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn test_busy_while_an_operation_is_not_done() {
        // The daily check skips its tick while this holds
        // (`auto_check::tick`): an operation queued, running, being
        // cancelled or verified counts, a finished one does not.
        let (adapter, state) = FakeAdapter::new();
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![test_support::make_instance("fake", "fake:1")];
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        assert!(
            !session.busy(),
            "the refresh returned and nothing else runs"
        );

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert!(!session.busy(), "a plan being previewed is not under way");
        let op_id = session.submit(issued.id).expect("submit");
        assert!(
            session.busy(),
            "queued or running from the moment it is submitted"
        );

        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Running)
        {
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(session.busy(), "running");

        session.cancel(op_id).expect("cancel a Running op");
        let deadline = Instant::now() + Duration::from_secs(2);
        while session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status != OpStatus::Done)
        {
            assert!(Instant::now() < deadline, "operation never finished");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!session.busy(), "the operation is done");
    }
}
