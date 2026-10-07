use crate::adapters::{Adapter, AdapterError};
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, AlreadyUpdated, ArtifactKey, ArtifactKind, Attention, CancelPolicy, Fault,
    InstanceId, ManagerInstance, OpKind, OpRequest, OpStatus, Outcome, Plan, PlanAction,
    Reconciled, ResourceLock,
};
use crate::runner::RunnerError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

/// Default target for how many operations `records` holds, applied as
/// each operation is submitted (`evict_oldest_done_records`): the oldest
/// finished (`Done`) ones are evicted down to it. An operation still in
/// flight is never evicted, so `records` can sit above it -- and stays
/// there, once those finish, until the next submission. Bounds a
/// long-running session's memory use -- without it, every
/// operation ever submitted in the process's lifetime stays in `records`
/// (and therefore in `summaries()`) forever.
pub const DEFAULT_MAX_RECORDS: usize = 200;

/// Cap on how many evicted operations' `EvictedOp` the manager keeps for
/// the completion notification at once (`Completions::evicted`). Each is
/// a few bytes; they are dropped as the notification accepts the runs
/// that include them (`completions_after`), so this is only ever reached
/// by thousands of operations with no run accepted between them. Past it
/// the oldest are dropped, and `Completions::forgotten_through` says so.
pub const MAX_EVICTED: usize = 10_000;

/// The `Outcome` for an `Err` out of `Adapter::execute`.
///
/// Every `Err` here is a reason of Banager's own, worded by the front end
/// in the user's language; the English `Display` of `AdapterError` never
/// reaches the wire. A tool that ran and failed is not an `Err` at all:
/// `run_plan` turns its exit code and stderr into `Ok(Outcome::Failed)`.
///
/// What today's adapters can return from `execute`: a runner error from
/// `run_plan` (`NotFound` and `Spawn` are states of the Mac and get their
/// own `Fault`; `OutputTooLarge` cannot happen for a transcript run and
/// `NoMock` is a test runner's), brew's root refusal (the gate refuses a
/// root Homebrew first, as `Unavailable::RefusesAsRoot`) and pip's
/// `Unsupported` (no pip `Plan` can exist: every pip instance is read-only
/// by design, so `issue_plan`'s gate refuses before pip's `plan()` would).
/// No `execute` returns `CommandFailed`, `Parse`, `SourceGone`,
/// `InvalidName`, `NotActionable`, `UpdateBlocked`, `UninstallBlocked`
/// (only `issue_plan` builds those two), `UninstallUnsafe` (a `plan()`
/// refusal; at run time the same finding is `Fault::PathChanged`) or
/// `IndexUpdating` (brew's `execute`
/// waits for a running `brew update` instead; only its `inventory`,
/// `check_updates` and uninstall `plan` return that). Everything but the two runner errors
/// is therefore a bug in Banager, and says so as `Fault::Internal` rather
/// than as a sentence of its own that nothing can produce.
fn execute_error_outcome(e: AdapterError) -> Outcome {
    let fault = match e {
        AdapterError::Runner(RunnerError::NotFound(program)) => Fault::ProgramMissing {
            program: program.display().to_string(),
        },
        AdapterError::Runner(RunnerError::Spawn(io)) => Fault::SpawnFailed {
            detail: io.to_string(),
        },
        AdapterError::Runner(RunnerError::NoMock(_))
        | AdapterError::Runner(RunnerError::OutputTooLarge { .. })
        | AdapterError::CommandFailed { .. }
        | AdapterError::Parse(_)
        | AdapterError::Refused(_)
        | AdapterError::InvalidName(_)
        | AdapterError::Unsupported(_)
        | AdapterError::SourceGone { .. }
        | AdapterError::NotListed
        | AdapterError::NotActionable { .. }
        | AdapterError::UpdateBlocked { .. }
        | AdapterError::UninstallBlocked { .. }
        | AdapterError::UninstallUnsafe { .. }
        | AdapterError::IndexUpdating => Fault::Internal,
    };
    Outcome::BanagerFailed(fault)
}

/// What an upgrade whose tool exited 0 did to the installed version, judged
/// from two `reconcile` readings of the same artifact: one taken before the
/// command ran and one after (both in `run_operation`). Only the exit-0 arm
/// asks: for a command that was stopped partway, the tools' version fields
/// prove nothing either way (the `Ok(Outcome::Unconfirmed)` arm says why).
enum VersionChange {
    /// Both readings name a version, and they differ.
    Changed,
    /// Both readings name a version, and it is the same one.
    Unchanged,
    /// There is nothing to compare: the before-reading failed, the package
    /// was absent in either reading, or a reading carries no version (an
    /// adapter says `None` when its version string cannot tell one install
    /// from another -- see `Reconciled::version`). An empty string is
    /// treated the same way: brew's parser falls back to `""` for a formula
    /// with no installed entry to read one from (`parse_info_installed`),
    /// npm's for a package `npm ls` gave no version (`parse_ls_global`),
    /// and two empty strings being equal says nothing.
    Unknown,
}

fn version_change(before: Option<&Reconciled>, after: &Reconciled) -> VersionChange {
    let known = |r: &Reconciled| -> Option<String> {
        if !r.present {
            return None;
        }
        r.version.clone().filter(|v| !v.is_empty())
    };
    match (before.and_then(known), known(after)) {
        (Some(b), Some(a)) if b == a => VersionChange::Unchanged,
        (Some(_), Some(_)) => VersionChange::Changed,
        _ => VersionChange::Unknown,
    }
}

/// Whether an update that ended with `outcome` may have changed what is
/// installed, and so brought a later update's package along
/// (`OperationManager::upgrades_ended`, `already_at_target`): its version
/// moved, or there was nothing to compare (`Succeeded`, but not one that
/// was `already` at its target); it failed, as Homebrew upgrades the
/// dependencies before the formula and a failure on the formula leaves
/// them moved; it was stopped partway (`Unconfirmed`); or the package is
/// gone. Not one the tool skipped (`UnchangedAfterUpgrade`), one cancelled
/// before its command, or one Banager stopped before its command
/// (`BanagerFailed`): those changed nothing (review of r6 y3-batch,
/// finding 1).
fn may_have_moved_others(outcome: &Outcome, already: Option<AlreadyUpdated>) -> bool {
    match outcome {
        Outcome::Succeeded => already.is_none(),
        Outcome::Failed { .. } | Outcome::Unconfirmed => true,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade) => false,
        Outcome::NeedsAttention(_) => true,
        Outcome::Cancelled | Outcome::BanagerFailed(_) => false,
    }
}

/// Whether `after`, an installed version, is at least `target`, the one the
/// confirmed plan aimed for (`OperationManager::submit_toward`): the same
/// string, or -- where both are made only of numbers and the separators
/// versions use (`1.6.59`, Homebrew's `1.11.1_6`, a cask's `5.0,123`) --
/// a later one by the numbers, run by run (`1.6.60`, `1.6.59_1`). Anything
/// with a letter or a hyphen in it is compared only for being the same:
/// "1.7.0-rc1" and npm's "1.7.0-1" are prereleases, both before "1.7.0",
/// though the one sorts after it by its characters and the other by its
/// numbers (r11 F1), and an order that can be wrong would call an update
/// that did not happen done. A hyphen that is no prerelease's
/// (ImageMagick's `7.1.1-47`, a date) is not ordered either: such a
/// version is at its target only when it is the target.
fn reached_target(after: &str, target: &str) -> bool {
    if after == target {
        return true;
    }
    let numeric = |v: &str| {
        !v.is_empty()
            && v.chars().next().is_some_and(|c| c.is_ascii_digit())
            && v.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | '_' | ','))
    };
    if !numeric(after) || !numeric(target) {
        return false;
    }
    let runs = |v: &str| -> Vec<u128> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|run| !run.is_empty())
            .map(|run| run.parse::<u128>().unwrap_or(u128::MAX))
            .collect()
    };
    runs(after) > runs(target)
}

/// Called once, as an operation finishes, with what it ended as: the
/// history's way in (`Session::submit`, `history::HistoryStore::record`).
/// Called on the operation's own task, with no lock of the manager's held,
/// after its resource locks are released but before the operation is
/// `Done` and before `OperationEvent::Finished` is sent -- so a window that
/// asks for the history on that event finds the record there, and Quit,
/// which waits for every operation to be `Done`, never exits before it. It must return at
/// once: the history only puts the record in memory and wakes its writer.
pub type OnFinish = Box<dyn FnOnce(&crate::history::Ended<'_>) + Send>;

pub struct OpRecord {
    pub id: OpId,
    pub plan: Plan,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
}

/// Releases a specific op's resource locks from the shared `held` set
/// exactly once, no matter how many times `release_once` is called or from
/// where. Two independent things can trigger a release for the same op:
/// `finish()` on its normal completion path, and `LockGuard`'s `Drop` impl
/// as a panic-safety net if the task unwinds before `finish` is ever
/// reached. Routing both through the same `AtomicBool`-guarded instance
/// means whichever fires first does the real work and the other is a no-op,
/// so a panic can never cause a *different*, still-running op's lock to be
/// evicted by a stale second release.
struct LockRelease {
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    locks: Vec<ResourceLock>,
    released: AtomicBool,
}

impl LockRelease {
    fn release_once(&self) {
        if self.released.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut held = self.held.lock().unwrap();
        for l in &self.locks {
            held.remove(l);
        }
    }
}

/// RAII guard that releases its `LockRelease` when dropped. Held as a plain
/// local variable across the rest of `run_operation`, including across the
/// `.await` on `adapter.execute(...)`: if that future panics, unwinding
/// through `run_operation`'s stack drops this guard like any other local,
/// releasing the lock even though `finish()` is never reached. On the
/// normal (non-panicking) path `finish()` already releases the lock via the
/// same `LockRelease`, so this `Drop` just finds `released` already `true`
/// and does nothing.
struct LockGuard(Arc<LockRelease>);

impl Drop for LockGuard {
    fn drop(&mut self) {
        self.0.release_once();
    }
}

/// Internal bookkeeping for one submitted operation. Deliberately a
/// different type from the public `OpRecord` (a Core Interface type whose
/// fields are contractual): `record()` builds a fresh `OpRecord` from this
/// on demand, so extra internal-only state like `lock_release` never leaks
/// into the public shape.
struct OpInternal {
    id: OpId,
    plan: Plan,
    status: OpStatus,
    outcome: Option<Outcome>,
    cancel: CancellationToken,
    /// `Some` once this op has actually acquired its resource locks (set
    /// right after the lock-wait loop below succeeds). `finish()` routes its
    /// own release through this instead of touching `held` directly, so it
    /// can never race with `LockGuard`'s panic-safety release.
    lock_release: Option<Arc<LockRelease>>,
    /// `submit_with`'s callback, taken by `finish`.
    on_finish: Option<OnFinish>,
    /// Set as `finish` begins: the outcome is decided, so a Cancel from
    /// then on is too late (`cancel`), even while the status still says
    /// `Running` for the instant the history's record takes.
    finishing: bool,
    /// Set just before `execute` is called: an operation that ends before
    /// that started nothing.
    started: bool,
    /// An update's two readings of the installed version, before its
    /// command and after it (`run_operation`), for `on_finish`.
    before_version: Option<String>,
    after_version: Option<String>,
    /// The version the confirmed plan of an update aimed for
    /// (`submit_toward`), or `None` where none was known.
    target_version: Option<String>,
    /// How many updates of this op's source had ended (`upgrades_ended`)
    /// when it was submitted.
    upgrades_seen: u64,
    /// Set by `run_operation` for an update already at its target when its
    /// turn came (`OpSummary::already_updated`).
    already_updated: Option<AlreadyUpdated>,
}

pub struct OperationManager {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    instances: Mutex<HashMap<InstanceId, ManagerInstance>>,
    sink: Arc<dyn EventSink>,
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    /// Every submitted operation that has not taken its resource locks yet,
    /// by id, with the locks it needs. `run_operation` lets an op take its
    /// locks only when none of them is held *and* no op with a lower id is
    /// still here needing one of them, so operations that need the same lock
    /// start in the order they were submitted -- the order the user
    /// confirmed them, which a batch uninstall relies on to remove a
    /// Homebrew formula's dependents before the formula
    /// (`src/lib/batchUninstall.ts`). Without it, whichever waiting op
    /// happened to poll first after the lock came free took it. An op
    /// leaves when it takes its locks, or in `finish` if it never does
    /// (cancelled while waiting, or ended by the panic watcher).
    ///
    /// Taken before `held` wherever both are held. A refresh's
    /// `acquire_resource_lock` does not queue: it is not an operation.
    queue: Mutex<BTreeMap<OpId, Vec<ResourceLock>>>,
    records: Arc<Mutex<HashMap<OpId, OpInternal>>>,
    next_id: AtomicU64,
    /// Caps how many operations may be concurrently past the lock-wait stage
    /// (i.e. actually running `execute`) at once, regardless of how many
    /// distinct resources are involved — spec §6: same lock serial,
    /// different locks parallel, at most 3 overall.
    semaphore: Arc<Semaphore>,
    /// Fired every time any operation reaches `Done`, so `wait()` can react
    /// immediately instead of polling `records` every 20 ms. A single
    /// instance shared by every operation: `wait(op_id)` re-checks its own
    /// `op_id`'s status after every wake, so a notification meant for a
    /// different op just costs one extra, harmless status check.
    done_notify: Arc<Notify>,
    /// Caps `records` at this many total entries (Task 13); see
    /// `DEFAULT_MAX_RECORDS`'s doc comment and `with_max_records`.
    max_records: usize,
    /// What the records evicted leave for the completion notification
    /// (`EvictedOp`). Locked only with `records` held (and `queue` before
    /// both, in `completions_after`).
    evicted: Mutex<EvictedLedger>,
    /// Caps `evicted`: `MAX_EVICTED`, smaller in this module's tests.
    max_evicted: usize,
    /// How many updates that reached their command and may have changed
    /// something (`may_have_moved_others`) have ended, by source instance
    /// and kind -- Homebrew brings a formula's dependencies along, which
    /// are formulae, so a cask's update brings no formula -- counted as each one's locks are released (`finish`), so that the
    /// next operation on that source, which waits for them, reads it. An
    /// update whose package is already at its target when its turn comes
    /// (`run_operation`) was brought there by an earlier one of the same
    /// source when this count moved after it was submitted and the source
    /// is one where an update can bring others along
    /// (`AlreadyUpdated::ByEarlierUpdate`). Locked after `records` where
    /// both are held, and never the other way round.
    upgrades_ended: Mutex<HashMap<(InstanceId, ArtifactKind), u64>>,
}

/// A read-only view of one operation for a UI, independent of the
/// operation's own lifetime bookkeeping (`OpRecord`/`OpInternal`). Carries
/// exactly what a list of "current and recent operations" needs to render.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpSummary {
    pub id: OpId,
    pub kind: OpKind,
    pub instance_id: InstanceId,
    pub artifact_kind: ArtifactKind,
    pub name: String,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
    pub argv_preview: Vec<String>, // program followed by args; empty for a plan that runs no command
    /// The variables the plan's command is given on top of Banager's own
    /// environment (`PlanAction::Command`'s `env`), in the plan's order;
    /// empty for a plan that runs no command. With `argv_preview` it is the
    /// whole command as the confirmation showed it (`CommandPreview.tsx`),
    /// so the one a failed operation hands over for Terminal -- where sudo
    /// can ask for the password Banager has no way to
    /// (`PasswordCommand.tsx`) -- keeps `HOMEBREW_NO_AUTOREMOVE=1` and the
    /// rest, and does no more there than it would have done here.
    /// OLLAMA_HOST userinfo is masked; this is not an execution environment.
    pub env_preview: Vec<(String, String)>,
    /// The plan's `cancel_policy`. The front end reads it with `status` to
    /// offer no Cancel button for a Running `NoCancel` op
    /// (`OperationBar.tsx`), the one op `cancel` below refuses by policy.
    pub cancel_policy: CancelPolicy,
    /// For an update that `Succeeded` though its own command changed
    /// nothing, because the version was already at its target when its
    /// turn came: how it got there, as far as Banager saw
    /// (`AlreadyUpdated`). `None` for every other operation. Always sent;
    /// `serde(default)` so a summary from before it existed still reads.
    #[serde(default)]
    pub already_updated: Option<AlreadyUpdated>,
}

/// How a finished operation ended, as the completion notification counts
/// it (`notify_operations::ReportedRuns::accepted`): `Outcome` without
/// what it carries, but for the one thing about a failure the
/// notification says apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// `Outcome::Succeeded`.
    Succeeded,
    /// `Outcome::Failed` and `Outcome::BanagerFailed`: did not happen.
    Failed,
    /// `Outcome::Failed` where sudo wanted the Mac's password with no way
    /// to ask for it (its `cause`, `FailureCause::NeedsPassword`, read off
    /// the last lines the tool wrote to stderr before a login was masked
    /// out of them): did not happen either, and of an update the notification
    /// says it needs the password, as the operation bar does. Decided as
    /// the operation is counted or evicted, so that what eviction keeps
    /// (`EvictedOp`) still says it once the summary has gone.
    NeedsPassword,
    /// `Outcome::NeedsAttention` and `Outcome::Unconfirmed`.
    Attention,
    /// `Outcome::Cancelled`.
    Cancelled,
}

impl Ended {
    pub fn of(outcome: &Outcome) -> Ended {
        use crate::history::FailureCause;
        match outcome {
            Outcome::Succeeded => Ended::Succeeded,
            Outcome::Failed {
                cause: Some(FailureCause::NeedsPassword),
                ..
            } => Ended::NeedsPassword,
            Outcome::Failed { .. } | Outcome::BanagerFailed(_) => Ended::Failed,
            Outcome::NeedsAttention(_) | Outcome::Unconfirmed => Ended::Attention,
            Outcome::Cancelled => Ended::Cancelled,
        }
    }
}

/// What the manager keeps of a finished operation `records` evicts
/// (`evict_oldest_done_records`): enough for the completion notification
/// to count it all the same, an update that stopped for the password
/// included (`Ended::NeedsPassword`). A run longer than `DEFAULT_MAX_RECORDS`, or
/// one told of after that many newer operations, has lost its oldest
/// records by then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvictedOp {
    pub kind: OpKind,
    pub ended: Ended,
}

/// Every operation the completion notification may count, read at one
/// instant (`OperationManager::completions_after`), so that none is
/// evicted from `operations` without being in `evicted` yet, and none
/// submitted is missing from all three.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completions {
    /// The records, newest first, as `summaries` gives them.
    pub operations: Vec<OpSummary>,
    /// The ids a submission has taken whose record is not in yet
    /// (`reserve` done, `record_and_run` to come): unfinished, though in
    /// neither `operations` nor `evicted`, and never one forgotten.
    pub unrecorded: Vec<OpId>,
    /// The evicted operations still kept (`MAX_EVICTED`), by id.
    pub evicted: BTreeMap<OpId, EvictedOp>,
    /// The newest id dropped from `evicted` past `MAX_EVICTED`, or 0: an
    /// id at or below it that is in neither `operations` nor `evicted`
    /// finished and was evicted, and how it ended is no longer known.
    pub forgotten_through: OpId,
}

/// `Completions::evicted` and `forgotten_through` as the manager keeps
/// them, behind `records`' lock (taken first wherever both are held).
#[derive(Debug, Default)]
struct EvictedLedger {
    ops: BTreeMap<OpId, EvictedOp>,
    forgotten_through: OpId,
}

impl EvictedLedger {
    /// Keeps `id`, dropping the oldest past `max`. A record with no
    /// outcome to keep -- never one `Done` -- is dropped as forgotten.
    fn keep(&mut self, id: OpId, kind: OpKind, outcome: Option<&Outcome>, max: usize) {
        match outcome {
            Some(outcome) => {
                self.ops.insert(
                    id,
                    EvictedOp {
                        kind,
                        ended: Ended::of(outcome),
                    },
                );
            }
            None => self.forgotten_through = self.forgotten_through.max(id),
        }
        while self.ops.len() > max {
            let Some((oldest, _)) = self.ops.pop_first() else {
                break;
            };
            self.forgotten_through = self.forgotten_through.max(oldest);
        }
    }

    /// Drops every operation at or below `through`, which the completion
    /// notification has accepted and never counts again.
    fn forget_through(&mut self, through: OpId) {
        self.ops = match through.checked_add(1) {
            Some(next) => self.ops.split_off(&next),
            None => BTreeMap::new(),
        };
    }
}

/// Why `OperationManager::cancel` changed nothing. The IPC layer
/// (`cancel_operation_impl`, src-tauri/src/ipc.rs) reports `NoCancel` to
/// the front end and keeps `NotPending` silent, as a lost race always was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelRefused {
    /// The op's plan says `CancelPolicy::NoCancel` and it is `Running`, so
    /// its command may be under way: the user cannot stop it. The command
    /// ends on its own or at the plan's `timeout_secs`, which the runner
    /// enforces on a deadline of its own counted from spawn
    /// (`RealRunner::run`, runner/real.rs), never through the token this
    /// refusal leaves unfired; `run_operation` then reconciles the stopped
    /// run like any other. A NoCancel op still `Queued` is not refused:
    /// nothing has been spawned and no timeout is counting, so `cancel`
    /// fires its token as for any plan and its command never starts.
    NoCancel,
    /// No op has this id, or it is past `Running` (`Verifying`/`Done`, or
    /// `finish` has begun and its outcome is decided), or a cancel is
    /// already in flight (`CancelRequested`/`Cancelling`) -- whatever the
    /// plan's policy.
    NotPending,
}

impl OperationManager {
    pub fn new(sink: Arc<dyn EventSink>) -> OperationManager {
        OperationManager {
            adapters: HashMap::new(),
            instances: Mutex::new(HashMap::new()),
            sink,
            held: Arc::new(Mutex::new(HashSet::new())),
            queue: Mutex::new(BTreeMap::new()),
            records: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            semaphore: Arc::new(Semaphore::new(3)),
            done_notify: Arc::new(Notify::new()),
            max_records: DEFAULT_MAX_RECORDS,
            evicted: Mutex::new(EvictedLedger::default()),
            max_evicted: MAX_EVICTED,
            upgrades_ended: Mutex::new(HashMap::new()),
        }
    }

    /// Adapters are registered once, before the manager is shared behind an
    /// `Arc` (see the tests below) — that's why this takes `&mut self`
    /// while every other method takes `&self`.
    pub fn register_adapter(&mut self, adapter: Arc<dyn Adapter>) {
        let id = adapter.meta().id.clone();
        self.adapters.insert(id, adapter);
    }

    pub fn register_instance(&self, inst: ManagerInstance) {
        self.instances.lock().unwrap().insert(inst.id.clone(), inst);
    }

    /// Caps how many finished (`Done`) operations `records` keeps at once;
    /// production uses `DEFAULT_MAX_RECORDS`, tests set a small value to
    /// make eviction observable without submitting hundreds of ops.
    pub fn with_max_records(mut self, max: usize) -> OperationManager {
        self.max_records = max;
        self
    }

    pub fn record(&self, op_id: OpId) -> Option<OpRecord> {
        let records = self.records.lock().unwrap();
        records.get(&op_id).map(|r| OpRecord {
            id: r.id,
            plan: r.plan.clone(),
            status: r.status,
            outcome: r.outcome.clone(),
        })
    }

    /// Newest first (descending op id).
    pub fn summaries(&self) -> Vec<OpSummary> {
        Self::summaries_of(&self.records.lock().unwrap())
    }

    /// What the completion notification may count once it has accepted
    /// every operation through `after` (`ReportedRuns::through`): the
    /// records and what evicted ones left, read under one lock. What the
    /// manager kept of evicted operations at or below `after` is dropped
    /// here, never to be asked for again.
    pub fn completions_after(&self, after: OpId) -> Completions {
        // The queue first, as everywhere: no id can be taken meanwhile, and
        // one taken before is in the queue until its op takes its locks or
        // finishes -- both after its record is in.
        let queue = self.queue.lock().unwrap();
        let records = self.records.lock().unwrap();
        let mut evicted = self.evicted.lock().unwrap();
        evicted.forget_through(after);
        Completions {
            operations: Self::summaries_of(&records),
            unrecorded: queue
                .keys()
                .filter(|id| !records.contains_key(id))
                .copied()
                .collect(),
            evicted: evicted.ops.clone(),
            forgotten_through: evicted.forgotten_through,
        }
    }

    fn summaries_of(records: &HashMap<OpId, OpInternal>) -> Vec<OpSummary> {
        let mut summaries: Vec<OpSummary> = records
            .values()
            .map(|r| {
                let (argv_preview, env_preview) = match &r.plan.action {
                    // A plan of two commands (U9) previews its first: the
                    // upgrade a `brew cleanup` follows, which is what a
                    // failed operation hands over for Terminal.
                    PlanAction::Command { program, args, env }
                    | PlanAction::CommandThen {
                        program, args, env, ..
                    } => {
                        let mut argv = vec![program.to_string_lossy().to_string()];
                        argv.extend(args.iter().cloned());
                        (argv, crate::runner::redact::preview_env(env))
                    }
                    // No command runs, so there is no argv to preview: an
                    // empty list, never an invented one. (The uninstall
                    // dialog shows the paths through the plan's
                    // `WillTrash` warnings.)
                    PlanAction::TrashPaths { .. } => (Vec::new(), Vec::new()),
                };
                OpSummary {
                    id: r.id,
                    kind: r.plan.request.kind,
                    instance_id: r.plan.request.instance_id.clone(),
                    artifact_kind: r.plan.request.artifact_kind,
                    name: r.plan.request.name.clone(),
                    status: r.status,
                    outcome: r.outcome.clone(),
                    argv_preview,
                    env_preview,
                    cancel_policy: r.plan.cancel_policy,
                    already_updated: r.already_updated,
                }
            })
            .collect();
        summaries.sort_by_key(|s| std::cmp::Reverse(s.id));
        summaries
    }

    /// Asks the op to stop. `Ok(())` means the request went through: the
    /// record is now `CancelRequested` and its token has fired, which
    /// `run_operation` and the runner act on. An `Err` means nothing
    /// changed, and says why.
    pub fn cancel(&self, op_id: OpId) -> Result<(), CancelRefused> {
        let mut records = self.records.lock().unwrap();
        let Some(r) = records.get_mut(&op_id) else {
            return Err(CancelRefused::NotPending);
        };
        // Only a still-pending op can be cancelled. Once it has moved
        // past Running (Verifying/Done) — or is already
        // CancelRequested/Cancelling — cancelling again must be a
        // no-op: forcing it back to CancelRequested here would corrupt
        // a finished record and make `wait()` (which only returns on
        // Done) hang forever. One `finish` has begun on is past it too,
        // though it says `Running` while its history record is kept: its
        // outcome is decided, and a `CancelRequested` from it would come
        // after its `Finished`.
        if r.finishing || !matches!(r.status, OpStatus::Queued | OpStatus::Running) {
            return Err(CancelRefused::NotPending);
        }
        // The policy is read only for an op that is Running, whose command
        // may be under way: that is what NoCancel protects. A Queued op has
        // spawned nothing (`run_operation` sets Running before it calls
        // `execute`), so it takes the token path below whatever its plan
        // says and its command never starts: `run_operation` finishes it
        // `Cancelled` before `execute`, or -- for a token fired in the
        // instant before `set_status(Running)` -- `RealRunner::run` sees
        // the fired token and spawns nothing. `OperationBar.tsx` reads the
        // same two fields off `OpSummary` and offers no Cancel button for
        // the one case this refuses by policy, a Running NoCancel op.
        if r.plan.cancel_policy == CancelPolicy::NoCancel && r.status == OpStatus::Running {
            return Err(CancelRefused::NoCancel);
        }
        r.status = OpStatus::CancelRequested;
        r.cancel.cancel();
        drop(records);
        self.sink.emit(OperationEvent::Status {
            op_id,
            status: OpStatus::CancelRequested,
        });
        Ok(())
    }

    pub async fn wait(&self, op_id: OpId) -> Option<Outcome> {
        loop {
            // Register interest in the next notification *before* checking
            // `records`: `Notify::notified()`'s returned future remembers a
            // notification that lands between this line and the `.await`
            // below, so a `finish()` racing with this check can never be
            // missed — the lost-wakeup a naive "check, then await" would
            // have. This replaces the previous 20ms-poll implementation.
            let notified = self.done_notify.notified();
            {
                let records = self.records.lock().unwrap();
                match records.get(&op_id) {
                    Some(r) if r.status == OpStatus::Done => return r.outcome.clone(),
                    None => return None,
                    _ => {}
                }
            }
            notified.await;
        }
    }

    pub fn submit(self: &Arc<Self>, plan: Plan) -> OpId {
        self.submit_with(plan, None)
    }

    /// `submit`, with a callback for when the operation finishes
    /// (`OnFinish`).
    pub fn submit_with(self: &Arc<Self>, plan: Plan, on_finish: Option<OnFinish>) -> OpId {
        self.submit_toward(plan, None, on_finish)
    }

    /// `submit_with`, for an update whose confirmed plan aimed at
    /// `target_version`: the version the check offered and the
    /// confirmation showed (`Session::submit` passes the candidate's
    /// `target`). An update whose package is already at least there when
    /// its turn comes, and still is after its command, is done
    /// (`run_operation`, `AlreadyUpdated`); with `None` it is judged as it
    /// always was.
    pub fn submit_toward(
        self: &Arc<Self>,
        plan: Plan,
        target_version: Option<String>,
        on_finish: Option<OnFinish>,
    ) -> OpId {
        let op_id = self.reserve(&plan);
        self.record_and_run(op_id, plan, target_version, on_finish);
        op_id
    }

    /// How many updates of `request`'s source and kind have ended
    /// (`upgrades_ended`).
    fn upgrades_ended_on(&self, request: &OpRequest) -> u64 {
        self.upgrades_ended
            .lock()
            .unwrap()
            .get(&(request.instance_id.clone(), request.artifact_kind))
            .copied()
            .unwrap_or(0)
    }

    /// Takes the next id, and the op joins the queue, in one step, so ids
    /// enter it in order: an op can never find the queue missing an
    /// earlier op that has its id but has not joined yet. Its record comes
    /// after (`record_and_run`), outside the queue's lock.
    fn reserve(&self, plan: &Plan) -> OpId {
        let mut queue = self.queue.lock().unwrap();
        let op_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        queue.insert(op_id, plan.locks.clone());
        op_id
    }

    /// The second half of `submit_with`: the record of the op `reserve`
    /// queued, and the task that runs it.
    fn record_and_run(
        self: &Arc<Self>,
        op_id: OpId,
        plan: Plan,
        target_version: Option<String>,
        on_finish: Option<OnFinish>,
    ) {
        let cancel = CancellationToken::new();
        let upgrades_seen = self.upgrades_ended_on(&plan.request);
        let record = OpInternal {
            id: op_id,
            plan: plan.clone(),
            status: OpStatus::Queued,
            outcome: None,
            cancel: cancel.clone(),
            lock_release: None,
            on_finish,
            started: false,
            finishing: false,
            before_version: None,
            after_version: None,
            target_version: target_version.filter(|v| !v.is_empty()),
            upgrades_seen,
            already_updated: None,
        };
        {
            let mut records = self.records.lock().unwrap();
            records.insert(op_id, record);
            Self::evict_oldest_done_records(
                &mut records,
                self.max_records,
                &mut self.evicted.lock().unwrap(),
                self.max_evicted,
            );
        }
        self.sink.emit(OperationEvent::Status {
            op_id,
            status: OpStatus::Queued,
        });

        let manager = Arc::clone(self);
        let handle = tokio::spawn(async move {
            manager.run_operation(op_id, plan, cancel).await;
        });

        // `run_operation`'s `JoinHandle` is otherwise unobserved, so if the
        // task panics (e.g. a misbehaving adapter), tokio catches the panic
        // at the task boundary and nothing would ever mark this op `Done` —
        // `wait()` would hang forever. This small watcher task is the other
        // half of the panic-safety fix: `LockGuard`'s `Drop` (inside
        // `run_operation`) already released any resource lock during the
        // unwind; this just finishes the bookkeeping so `wait()` returns.
        let manager_for_panic = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(join_err) = handle.await {
                if join_err.is_panic() {
                    manager_for_panic.finish(op_id, Outcome::BanagerFailed(Fault::Panicked), true);
                }
            }
        });
    }

    /// Evicts the oldest (lowest op id) `Done` records, oldest first, until
    /// either the cap is met or no `Done` record remains. Work that is still
    /// Queued/Running/CancelRequested/Cancelling/Verifying is never evicted,
    /// so `max_records` is a target, not a hard bound: with enough operations
    /// in flight at once, `records` can legitimately sit above it. Each
    /// one evicted leaves its kind and how it ended in `evicted`
    /// (`EvictedOp`), for the completion notification.
    fn evict_oldest_done_records(
        records: &mut HashMap<OpId, OpInternal>,
        max_records: usize,
        evicted: &mut EvictedLedger,
        max_evicted: usize,
    ) {
        if records.len() <= max_records {
            return;
        }
        let mut done_ids: Vec<OpId> = records
            .iter()
            .filter(|(_, r)| r.status == OpStatus::Done)
            .map(|(id, _)| *id)
            .collect();
        done_ids.sort_unstable();
        let mut overflow = records.len() - max_records;
        for id in done_ids {
            if overflow == 0 {
                break;
            }
            if let Some(r) = records.remove(&id) {
                evicted.keep(id, r.plan.request.kind, r.outcome.as_ref(), max_evicted);
            }
            overflow -= 1;
        }
    }

    async fn run_operation(self: Arc<Self>, op_id: OpId, plan: Plan, cancel: CancellationToken) {
        // Wait for every lock this plan needs, polling every 50 ms, and for
        // this op's turn: no op submitted before it may still be waiting for
        // one of the same locks (`queue`). Only mark `acquired` once every
        // lock in `plan.locks` was free and has now been inserted into
        // `held` — otherwise a later step could release a lock this op never
        // actually took.
        let mut acquired = false;
        loop {
            {
                let mut queue = self.queue.lock().unwrap();
                let mut held = self.held.lock().unwrap();
                let free = plan.locks.iter().all(|l| !held.contains(l));
                let earlier_waits = queue
                    .range(..op_id)
                    .any(|(_, locks)| locks.iter().any(|l| plan.locks.contains(l)));
                if free && !earlier_waits {
                    for l in &plan.locks {
                        held.insert(l.clone());
                    }
                    queue.remove(&op_id);
                    acquired = true;
                }
            }
            if acquired {
                break;
            }
            if cancel.is_cancelled() {
                // Never actually started (still waiting for a lock held by
                // another op), so nothing on the system changed: the user's
                // cancel is the whole story.
                self.finish(op_id, Outcome::Cancelled, false);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // The lock is now ours. Wire up panic-safe release before doing
        // anything else: stash a `LockRelease` in this op's record (so
        // `finish()` can route its normal-path release through the same
        // idempotent object) and keep a `LockGuard` as a local variable for
        // the rest of this function. If anything below panics — including
        // inside `adapter.execute(...).await` — unwinding drops `_lock_guard`
        // and releases the lock even though `finish` is never reached.
        let lock_release = Arc::new(LockRelease {
            held: self.held.clone(),
            locks: plan.locks.clone(),
            released: AtomicBool::new(false),
        });
        if let Some(r) = self.records.lock().unwrap().get_mut(&op_id) {
            r.lock_release = Some(lock_release.clone());
        }
        let _lock_guard = LockGuard(lock_release);

        // If the op was cancelled in the window between "the lock became
        // free" and this check (e.g. it was cancelled the instant after
        // `submit`, before the poll loop above ever ran), the command must
        // never actually start. Unlike the still-queued branch above, this
        // op *did* acquire its locks, so they must be released here —
        // `release_locks: true` is required, not `false` (which would leak
        // them forever since nothing else will ever release them).
        if cancel.is_cancelled() {
            self.finish(op_id, Outcome::Cancelled, true);
            return;
        }

        // Acquire a concurrency permit only *now*, after the resource lock
        // is already ours — this is the cross-resource cap (max 3
        // concurrently active ops of any kind), and per the `semaphore`
        // field's doc comment it must cap operations *actually running*
        // `execute`, not operations merely queued behind another op's
        // resource lock. Acquiring it earlier (before the lock-wait loop
        // above) would let two ops stuck waiting for the same lock each
        // hoard a permit while doing nothing, starving a third, unrelated
        // op on a completely different, free lock — spec §6 requires same
        // lock serial, different locks parallel, at most 3 concurrent, not
        // "at most 3 queued".
        //
        // An op can now hold its resource lock while waiting here for a
        // permit. This cannot deadlock: permits are only ever held by ops
        // that are actively executing (never by an op blocked waiting on a
        // lock or another permit), so every permit holder is guaranteed to
        // finish and release it independently of anything this waiting op
        // holds. The wait graph is a DAG — lock-wait → lock-held →
        // permit-wait → permit-held → release — with no cycle back through
        // a resource this op already owns.
        //
        // "Actively executing" covers more than the tool's own process now,
        // and is worth naming: the `adapter.execute(...)` call below can
        // itself block for a while before a brew formula or cask ever
        // spawns, inside `BrewAdapter::wait_for_update`
        // (`crates/banager-core/src/adapters/brew/mod.rs`) — up to
        // `op_update_wait` (`OP_UPDATE_WAIT` outside tests, ten minutes),
        // while it waits for a `brew update` a refresh left running to
        // finish. That is a permit held doing nothing but waiting, for as
        // long as ten minutes, not a bug in this reasoning: the DAG claim
        // above is about what a permit holder can be *blocked on*, and
        // `wait_for_update` blocks on Banager's own per-instance update lock
        // (`update_lock_for` -- an in-process `tokio::sync::Mutex`, not
        // Homebrew's file-based `var/homebrew/locks/update`;
        // `wait_for_update`'s doc in
        // `crates/banager-core/src/adapters/brew/mod.rs` distinguishes the
        // two) or a bounded sleep
        // (`tokio::time::sleep(self.op_update_wait)`), never on this
        // semaphore or on any `ResourceLock` another op here might be
        // holding. So it still cannot deadlock; it can only make the other
        // two permits this cap allows matter more while it lasts.
        //
        // Cancellation must still be honored while waiting here: an op
        // cancelled at this stage already holds its resource lock, so
        // — unlike the pre-lock cancellation branches above, which never
        // touched anything — it must release that lock before finishing.
        // Routing through `finish(..., true)` delegates the actual release
        // to this op's `LockRelease` (already stashed above), the same
        // exactly-once mechanism `LockGuard`'s panic-safety `Drop` uses, so
        // this can never race or double-release.
        let _permit: OwnedSemaphorePermit = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                self.finish(op_id, Outcome::Cancelled, true);
                return;
            }
            permit = self.semaphore.clone().acquire_owned() => {
                match permit {
                    Ok(p) => p,
                    Err(_) => {
                        // `acquire_owned` only fails on a closed semaphore,
                        // and nothing ever closes this one — so this arm is
                        // not expected to run. If it ever does, the command
                        // never started, but nobody asked for that: it is
                        // not the user's cancellation, so it must not be
                        // reported as one.
                        self.finish(
                            op_id,
                            Outcome::BanagerFailed(Fault::Internal),
                            true,
                        );
                        return;
                    }
                }
            }
        };

        self.set_status(op_id, OpStatus::Running);

        let instance = {
            let instances = self.instances.lock().unwrap();
            instances.get(&plan.request.instance_id).cloned()
        };
        // Always found in production: `Session::submit` is the only caller
        // of `submit`, it only runs a plan `issue_plan` built from an
        // instance in the snapshot, every instance a refresh commits to the
        // snapshot is registered here first (`refresh`, before it commits),
        // and `instances` is insert-only. A source that has gone since the
        // preview is refused by `Session::submit` itself, as
        // `SubmitError::SourceGone`. So a miss is Banager's own bug.
        let instance = match instance {
            Some(i) => i,
            None => {
                self.finish(op_id, Outcome::BanagerFailed(Fault::Internal), true);
                return;
            }
        };

        let adapter = self.adapters.get(&instance.adapter_id).cloned();
        let adapter = match adapter {
            Some(a) => a,
            None => {
                self.finish(op_id, Outcome::BanagerFailed(Fault::Internal), true);
                return;
            }
        };

        let key = ArtifactKey {
            instance_id: plan.request.instance_id.clone(),
            kind: plan.request.artifact_kind,
            name: plan.request.name.clone(),
        };

        // An upgrade's package is installed before the command and after
        // it, so being present afterwards proves nothing about whether it
        // was upgraded. What does, once the tool has exited 0, is its
        // version changing, and for that there has to be a reading from
        // before. (It decides nothing for a command that was stopped: see
        // the `Ok(Outcome::Unconfirmed)` arm below.) It is taken with the
        // same `reconcile` the after-reading below uses, so the two come
        // from one parser reading one source the same way, and only a real
        // change in what is installed can make them differ. The update
        // check's `UpdateCandidate.current` / `.target` are not used for
        // this: they come from a different command and parser, and two
        // parsers for one tool have already spelled one package's name
        // differently (brew's `full_name` against `name`), which versions
        // can do too.
        //
        // An `Err` here keeps `before` `None`, and every arm below then
        // decides exactly as it did before this reading existed. On brew,
        // that is what happens while a `brew update` a refresh left
        // running is still going: `inventory` refuses with
        // `IndexUpdating` (`BrewAdapter::inventory`). Its
        // `join_running_update` also marks that update `announced`, which
        // it already is: only `maybe_update` starts one, only from a
        // refresh's `check_updates` holding this instance's `ResourceLock`
        // (`refresh_round` in session/refresh.rs), which this op now holds;
        // and one it leaves running is marked there, the moment the
        // refresh stops waiting for it. (A refresh dropped during that wait
        // would leave it unmarked, and this would then mark it, costing one
        // extra refresh when it ends; nothing drops a refresh today.)
        //
        // A Cancel while this reads stops here: nothing was started.
        // Dropping the read's future kills its command, which for a query
        // is harmless (the `CommandRunner::run` doc).
        let before = if plan.request.kind == OpKind::Upgrade {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    self.finish(op_id, Outcome::Cancelled, true);
                    return;
                }
                read = adapter.reconcile(&instance, &key) => read.ok(),
            }
        } else {
            None
        };

        // npm, Cargo and Ollama upgrades can install a missing name.
        // A successful locked reading of absence must never reach execute.
        // A failed reading remains None (including brew's IndexUpdating).
        if before.as_ref().is_some_and(|reading| !reading.present) {
            self.finish(
                op_id,
                Outcome::NeedsAttention(Attention::GoneBeforeUpgrade),
                true,
            );
            return;
        }

        // A NoCancel plan gets the same live token as any other. Once the
        // op is Running, `cancel` refuses to fire it, so its `execute` ends
        // when the command does or at `plan.timeout_secs`, which the runner
        // keeps on a deadline of its own from spawn (see
        // `CancelRefused::NoCancel`). A token `cancel` fired while the op
        // was still Queued reaches here only if it fired between acquiring
        // the permit and `set_status(Running)` above; `RealRunner::run`
        // then checks it before spawning and starts nothing.
        if let Some(r) = self.records.lock().unwrap().get_mut(&op_id) {
            r.started = true;
            r.before_version = before
                .as_ref()
                .filter(|b| b.present)
                .and_then(|b| b.version.clone());
        }
        let exec_result = adapter
            .execute(&plan, self.sink.clone(), op_id, cancel.clone())
            .await;

        if cancel.is_cancelled() {
            self.set_status(op_id, OpStatus::Cancelling);
        }
        self.set_status(op_id, OpStatus::Verifying);

        // After an uninstall only presence decides anything below, and an
        // adapter may answer that when it cannot answer what version is
        // installed (`Adapter::reconcile_after_uninstall`, handed the plan
        // `execute` just carried out: a path-list uninstall's reading asks
        // which paths it moved); everything else keeps the full reading.
        //
        // After a link, whether Homebrew says it is linked now
        // (`reconcile_link`): `brew link` exits 0 having linked nothing
        // too. `linked_after` is that reading -- `Ok(None)`, not installed --
        // and `reconciled` its presence, for the arms that read only that.
        let linked_after = match plan.request.kind {
            OpKind::Link => Some(adapter.reconcile_link(&instance, &key).await),
            _ => None,
        };
        let reconciled = match plan.request.kind {
            OpKind::Uninstall => {
                adapter
                    .reconcile_after_uninstall(&instance, &key, &plan)
                    .await
            }
            OpKind::Install | OpKind::Upgrade => adapter.reconcile(&instance, &key).await,
            OpKind::Link => match &linked_after {
                Some(Ok(linked)) => Ok(Reconciled {
                    present: linked.is_some(),
                    version: None,
                }),
                _ => Err(AdapterError::Refused(
                    "the reading after the link failed".to_string(),
                )),
            },
        };
        if plan.request.kind == OpKind::Upgrade {
            if let Ok(r) = reconciled.as_ref() {
                if r.present {
                    if let Some(rec) = self.records.lock().unwrap().get_mut(&op_id) {
                        rec.after_version = r.version.clone();
                    }
                }
            }
        }

        let final_outcome = match exec_result {
            // A command that reported success is not proof of success on
            // its own — spec §6 requires the `Verifying` reconcile to gate
            // the final outcome even on the "normal" exit-0 path, not just
            // after a cancelled/timed-out execute. Never fabricate Succeeded
            // or Failed when reconcile itself could not be trusted (`Err`):
            // report Unconfirmed instead.
            Ok(Outcome::Succeeded) => match reconciled {
                Err(_) => Outcome::Unconfirmed,
                Ok(r) => match plan.request.kind {
                    OpKind::Install => {
                        if r.present {
                            Outcome::Succeeded
                        } else {
                            Outcome::NeedsAttention(Attention::NotInstalledAfterInstall)
                        }
                    }
                    OpKind::Uninstall => {
                        if !r.present {
                            Outcome::Succeeded
                        } else {
                            Outcome::NeedsAttention(Attention::StillInstalledAfterUninstall)
                        }
                    }
                    // `brew link` exited 0: linked, if Homebrew says so
                    // now; it also exits 0 having refused ("Refusing to
                    // link macOS provided/shadowed software", cmd/link.rb
                    // in Homebrew 7.0.8). Gone after it, what happened is
                    // not known. Whether the source that needed it answers
                    // now is the next refresh's to say (`NoAnswer`).
                    OpKind::Link => match (r.present, &linked_after) {
                        (true, Some(Ok(Some(true)))) => Outcome::Succeeded,
                        (true, Some(Ok(Some(false)))) => {
                            Outcome::NeedsAttention(Attention::NotLinkedAfterLink)
                        }
                        _ => Outcome::Unconfirmed,
                    },
                    // The tool exited 0: it says it ran to the end, so
                    // the reading after is of a finished run, and the two
                    // readings say what that run did.
                    // - Moved: it upgraded the package.
                    // - Did not move: it skipped the package and exited 0
                    //   anyway. Every known way to get here is such a
                    //   tool: a locked pipx tool, a uv tool installed with
                    //   `==`, a disabled Homebrew cask, a cask whose
                    //   installed recipe Homebrew cannot load (one test
                    //   each in tests/ops_upgrade_version_test.rs).
                    // - Nothing to compare (`Unknown`): presence is all
                    //   there is, and it is taken as success, as it was
                    //   before the reading existed.
                    //
                    // One that did not move but reads at least the version
                    // its confirmed plan aimed for (`submit_toward`) was
                    // already there before its command: an earlier update
                    // of the batch upgraded it as a dependency, say. That
                    // is an update done, not one skipped -- `Succeeded`,
                    // with `already_updated` saying how. Below the target,
                    // or with no target, it is `UnchangedAfterUpgrade`.
                    OpKind::Upgrade => {
                        if !r.present {
                            Outcome::NeedsAttention(Attention::GoneAfterUpgrade)
                        } else {
                            match version_change(before.as_ref(), &r) {
                                VersionChange::Unchanged => {
                                    match self.already_at_target(
                                        op_id,
                                        &plan,
                                        &r,
                                        adapter.one_update_can_update_others(),
                                    ) {
                                        Some(how) => {
                                            if let Some(rec) =
                                                self.records.lock().unwrap().get_mut(&op_id)
                                            {
                                                rec.already_updated = Some(how);
                                            }
                                            Outcome::Succeeded
                                        }
                                        None => Outcome::NeedsAttention(
                                            Attention::UnchangedAfterUpgrade,
                                        ),
                                    }
                                }
                                VersionChange::Changed | VersionChange::Unknown => {
                                    Outcome::Succeeded
                                }
                            }
                        }
                    }
                },
            },
            // The command did not reach its exit: `run_plan`
            // (adapters/mod.rs) turns a run the runner cancelled or timed
            // out, or one a signal the run did not send ended (Activity
            // Monitor, `kill`, a crash), into this; a path-list uninstall
            // (adapters/standalone/removal.rs) returns it for a Cancel
            // after its first move or a spent budget before an item, and
            // for an item whose move
            // panicked -- and also once every item moved, when its last
            // look cannot tell whether a listed path is there; brew's
            // own Cancel, while it waits for a `brew update`, is
            // `Cancelled` before its command starts. The
            // reading after it tells us the artifact's *current* state,
            // not whether this op caused it. Presence is proof enough for
            // Install (wasn't there, now is) and Uninstall (was there, now
            // isn't).
            //
            // When the run was stopped by the user's own Cancel (the token
            // is only ever fired by `cancel()`; neither a timeout nor a
            // signal from outside touches it) and reconcile shows an
            // install did *not* take effect, the cancel is what happened,
            // and the user is told so: an install that left nothing behind
            // changed nothing that matters. An uninstall is different:
            // finding the package still there does not prove the command
            // removed nothing (a cask's app can be gone while its saved
            // metadata still lists it), so an interrupted uninstall that
            // reads as present stays `Unconfirmed`. A Cancel that landed
            // before the command started never reaches this arm:
            // `run_plan` answers `Cancelled` itself, which the `Ok(other)`
            // arm lets stand, and so does a path-list uninstall stopped
            // before its first move. If the work finished anyway, the arms
            // below report `Succeeded`, not `Cancelled`: the race goes to
            // whatever reconcile actually found. A run a signal ended with
            // the token unfired stays `Unconfirmed` here, never
            // `Cancelled`: nobody pressed Cancel.
            //
            // An upgrade stopped here is `Unconfirmed`, whatever its two
            // version readings say. Do not let the comparison turn it into
            // `Succeeded` or `Cancelled`: c6ecf5b did, and was wrong both
            // ways. The runner stops a command with SIGTERM to its process
            // group (`ProcessGroup::terminate`, runner/real.rs). The tools
            // write the version this reads partway through an upgrade, not
            // at its end, and nothing in them runs on SIGTERM to undo the
            // part already done. (Line numbers are Homebrew 7.0.6 and pipx
            // 1.17.3.)
            //
            // A moved version does not mean the upgrade finished:
            // - Homebrew upgrades a formula by unlinking the old keg,
            //   pouring the new one, and only then linking it, in `finish`
            //   (install.rb:634-637). Homebrew handles INT (brew.rb:22) but
            //   not TERM, so SIGTERM raises `SignalException`, which its
            //   `rescue => e` rollbacks do not catch; the `ensure` relinks
            //   the old keg only `unless formula.latest_version_installed?`
            //   (install.rb:645), which is true once the new keg is poured.
            //   Stopped between pour and link, nothing is linked and
            //   `post_install` never ran, and the reading shows the new keg:
            //   with `linked_keg` null, `parse_info_installed` takes
            //   `installed.last()`.
            // - A cask upgrade writes the new version's metadata in `stage`
            //   (cask/upgrade.rb:460; `save_caskfile`,
            //   cask/installer.rb:589-603), which is where
            //   `Cask#installed_version` reads it from (cask/cask.rb:312-315),
            //   before `install_artifacts` puts the new app in place
            //   (cask/upgrade.rb:462). Stopped there, the reading shows the
            //   new version while the old app is out of /Applications and
            //   the new one is not in.
            //
            // An unchanged version does not mean nothing happened:
            // - pipx installs the new package first (`venv.py:778`) and
            //   writes the version `pipx list --json` reports last
            //   (`update_package_metadata`, `venv.py:790`). pipx sets no
            //   signal handler, so Python dies on SIGTERM between the two,
            //   leaving the venv new or half-swapped and the reading at the
            //   old version.
            // - A cask upgrade first moves the old app out of /Applications
            //   (`start_upgrade`, cask/upgrade.rb:455) and puts it back only
            //   from `rescue => e` (cask/upgrade.rb:502). Stopped before
            //   `stage`, the reading is the old version and the app is gone.
            //
            // Exit 0 is different: the tool itself said it finished, so
            // the comparison on the `Ok(Outcome::Succeeded)` arm above is
            // of a finished run. And a Cancel during the before-reading is
            // `Cancelled`, because `execute` never started (above).
            Ok(Outcome::Unconfirmed) => {
                let user_cancelled = cancel.is_cancelled();
                match plan.request.kind {
                    // A link stopped partway may have linked some of the
                    // formula's files: Homebrew takes them back only on its
                    // own error, not on a stop.
                    OpKind::Upgrade | OpKind::Link => Outcome::Unconfirmed,
                    OpKind::Install => match reconciled {
                        Ok(r) if r.present => Outcome::Succeeded,
                        Ok(_) if user_cancelled => Outcome::Cancelled,
                        _ => Outcome::Unconfirmed,
                    },
                    OpKind::Uninstall => match reconciled {
                        Ok(r) if !r.present => Outcome::Succeeded,
                        // Presence cannot prove that an interrupted removal
                        // left the package intact (e.g. cask metadata remains).
                        _ => Outcome::Unconfirmed,
                    },
                }
            }
            // Anything else `execute` answered stands as it is, whatever
            // the reading after says -- a tool's own `Failed`, Banager's
            // `BanagerFailed`, brew's `Cancelled` before its command starts,
            // `run_plan`'s `Cancelled` for a Cancel before its command
            // started, a path-list uninstall's `Cancelled` before its first
            // move, and its `NeedsAttention(BackAfterUninstall)`, which its
            // own last look found (adapters/standalone/removal.rs).
            Ok(other) => other,
            Err(e) => execute_error_outcome(e),
        };

        self.finish(op_id, final_outcome, true);
    }

    fn set_status(&self, op_id: OpId, status: OpStatus) {
        if let Some(r) = self.records.lock().unwrap().get_mut(&op_id) {
            r.status = status;
        }
        self.sink.emit(OperationEvent::Status { op_id, status });
    }

    /// For an update whose two readings are equal (`after`): how it was
    /// already at its confirmed target (`submit_toward`), or `None` where
    /// there is no target or the reading is below it. By an earlier update
    /// when one of the same source and kind that may have changed something
    /// has ended since this one was submitted (`upgrades_ended`), which on a
    /// source's lock means before its turn -- and only on a source where
    /// one update can update another package at all
    /// (`Adapter::one_update_can_update_others`, `others_move`).
    fn already_at_target(
        &self,
        op_id: OpId,
        plan: &Plan,
        after: &Reconciled,
        others_move: bool,
    ) -> Option<AlreadyUpdated> {
        let (target, seen) = {
            let records = self.records.lock().unwrap();
            let r = records.get(&op_id)?;
            (r.target_version.clone()?, r.upgrades_seen)
        };
        let version = after.version.as_deref().filter(|v| !v.is_empty())?;
        if !reached_target(version, &target) {
            return None;
        }
        if others_move && self.upgrades_ended_on(&plan.request) > seen {
            Some(AlreadyUpdated::ByEarlierUpdate)
        } else {
            Some(AlreadyUpdated::BeforeItsTurn)
        }
    }

    /// `release_locks` must be `false` when this op never actually acquired
    /// its locks (cancelled while still waiting for them, or for a
    /// concurrency permit) — passing `true` in that case would be
    /// harmless in itself (there is no `lock_release` yet to act on), but
    /// stays `false` there for clarity. When `true` and the op *did*
    /// acquire locks, the actual removal is delegated to that op's
    /// `LockRelease` (shared with its `LockGuard`), so a panic-triggered
    /// release racing with this one can never double-release — whichever
    /// runs first wins and the other is a no-op.
    fn finish(&self, op_id: OpId, outcome: Outcome, release_locks: bool) {
        // An op that never took its locks is still in the queue; left there,
        // every later op needing one of its locks would wait for it for
        // ever. (One that took them left it then; this is a no-op for it.)
        self.queue.lock().unwrap().remove(&op_id);
        // First the locks and the history: the operation is not `Done` --
        // nor has it an outcome to show -- until its record is kept, so
        // that Quit, which asks whether any operation is unfinished and
        // then flushes what the history owes, can never find it finished
        // with its record still to come.
        let ended = {
            let mut records = self.records.lock().unwrap();
            if let Some(r) = records.get_mut(&op_id) {
                r.finishing = true;
                // Counted before the locks are let go: the next operation
                // on this source waits for them, and reads the count to
                // tell whether an update ended before its turn
                // (`already_at_target`). Only an update that may have
                // changed something counts (`may_have_moved_others`).
                if r.plan.request.kind == OpKind::Upgrade
                    && r.started
                    && may_have_moved_others(&outcome, r.already_updated)
                {
                    *self
                        .upgrades_ended
                        .lock()
                        .unwrap()
                        .entry((
                            r.plan.request.instance_id.clone(),
                            r.plan.request.artifact_kind,
                        ))
                        .or_insert(0) += 1;
                }
                if release_locks {
                    if let Some(lr) = &r.lock_release {
                        lr.release_once();
                    }
                }
                r.on_finish.take().map(|on_finish| {
                    (
                        on_finish,
                        r.plan.request.clone(),
                        r.started,
                        r.before_version.clone(),
                        r.after_version.clone(),
                        r.already_updated,
                    )
                })
            } else {
                None
            }
        };
        if let Some((on_finish, request, started, before, after, already_updated)) = ended {
            let key = ArtifactKey {
                instance_id: request.instance_id,
                kind: request.artifact_kind,
                name: request.name,
            };
            let ended = crate::history::Ended {
                op_id,
                key: &key,
                op_kind: request.kind,
                outcome: &outcome,
                started,
                before: before.as_deref(),
                after: after.as_deref(),
                already_updated,
            };
            // A callback that panicked must not leave the operation
            // unfinished for good: it ends all the same, with no record.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| on_finish(&ended)));
        }
        if let Some(r) = self.records.lock().unwrap().get_mut(&op_id) {
            r.status = OpStatus::Done;
            r.outcome = Some(outcome.clone());
        }
        self.done_notify.notify_waiters();
        self.sink.emit(OperationEvent::Finished { op_id, outcome });
    }
}

/// Held while `refresh` is fetching one instance's inventory/updates, over
/// the *same* `held` set `run_operation`'s locks use. Releases on drop, the
/// same idempotent-by-construction shape as the internal `LockGuard`.
pub struct ResourceLockGuard {
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    lock: ResourceLock,
}

impl Drop for ResourceLockGuard {
    fn drop(&mut self) {
        self.held.lock().unwrap().remove(&self.lock);
    }
}

/// A detection owns every resource it may execute, atomically acquired
/// against operations and released even when the detection is aborted.
pub(crate) struct DetectionGuard {
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    locks: Vec<ResourceLock>,
}

impl Drop for DetectionGuard {
    fn drop(&mut self) {
        let mut held = self.held.lock().unwrap();
        for lock in &self.locks {
            held.remove(lock);
        }
    }
}

impl OperationManager {
    pub(crate) fn try_detection_locks(&self, locks: Vec<ResourceLock>) -> Option<DetectionGuard> {
        let queue = self.queue.lock().unwrap();
        let mut held = self.held.lock().unwrap();
        if locks
            .iter()
            .any(|lock| held.contains(lock) || queue.values().any(|q| q.contains(lock)))
        {
            return None;
        }
        held.extend(locks.iter().cloned());
        Some(DetectionGuard {
            held: self.held.clone(),
            locks,
        })
    }

    /// The resource locks held this instant: by operations from the
    /// moment `run_operation` acquires theirs until `finish` releases
    /// them, and by a refresh's per-instance fetches
    /// (`acquire_resource_lock`). A snapshot of the same `held` set both
    /// of those use, read by `Session::refresh_round` before its
    /// detection fan-out so that an adapter whose instance an operation is
    /// working on is neither detected nor inventoried that round (phase 4
    /// step E: `rustup self update` replaces the binary that both
    /// `rustup --version` and, through the `cargo` proxy, `cargo
    /// --version` would run). It decides nothing about acquisition, which
    /// stays in `run_operation`'s own loop.
    pub fn locks_held(&self) -> HashSet<ResourceLock> {
        self.held.lock().unwrap().clone()
    }

    /// Waits (polling every 50ms, the same cadence `run_operation` already
    /// uses for its own lock-wait loop) until `lock` is free, then holds it
    /// until the returned guard drops. `refresh` uses this to take the same
    /// per-instance lock a submitted install/upgrade/uninstall holds, so the
    /// two can never read/write that instance's filesystem state at once —
    /// while a *different* instance's lock is untouched, so refreshing one
    /// instance never waits on an operation running against another.
    pub async fn acquire_resource_lock(self: &Arc<Self>, lock: ResourceLock) -> ResourceLockGuard {
        loop {
            {
                let mut held = self.held.lock().unwrap();
                if !held.contains(&lock) {
                    held.insert(lock.clone());
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        ResourceLockGuard {
            held: self.held.clone(),
            lock,
        }
    }
}

#[cfg(test)]
mod already_updated_tests {
    use super::*;

    #[test]
    fn test_reached_target_orders_only_versions_made_of_numbers() {
        assert!(reached_target("1.6.59", "1.6.59"));
        assert!(reached_target("1.6.60", "1.6.59"));
        assert!(
            reached_target("1.10.0", "1.9.9"),
            "by value, not by characters"
        );
        assert!(
            reached_target("1.11.1_6", "1.11.1_5"),
            "a Homebrew revision"
        );
        assert!(reached_target("1.6.59_1", "1.6.59"));
        assert!(
            reached_target("5.0,124", "5.0,123"),
            "a cask's build after a comma"
        );
        assert!(!reached_target("2026-08-14", "2026-08-13"));
        assert!(!reached_target("1.2.3-1", "1.2.3"));
        assert!(reached_target("1.2.3-1", "1.2.3-1"));
        assert!(!reached_target("1.6.58", "1.6.59"));
        assert!(!reached_target("1.9.9", "1.10.0"));
        // A letter anywhere: only the same string is at least it.
        assert!(!reached_target("1.7.0-rc1", "1.7.0"));
        assert!(!reached_target("1.7.1-beta", "1.7.0"));
        assert!(reached_target("latest", "latest"));
        assert!(!reached_target("sha256:bbbb", "sha256:aaaa"));
        assert!(!reached_target("", "1.0"));
    }

    #[test]
    fn test_op_summary_wire_shape_carries_already_updated_and_reads_without_it() {
        // `OpSummary` in src/lib/types.ts; src/lib/types.test.ts round-trips
        // the same shape.
        let summary = OpSummary {
            id: 10,
            kind: OpKind::Upgrade,
            instance_id: "brew:/opt/homebrew".into(),
            artifact_kind: ArtifactKind::Formula,
            name: "libpng".into(),
            status: OpStatus::Done,
            outcome: Some(Outcome::Succeeded),
            argv_preview: vec![],
            env_preview: vec![],
            cancel_policy: CancelPolicy::KillThenReconcile,
            already_updated: Some(AlreadyUpdated::ByEarlierUpdate),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert_eq!(
            json,
            r#"{"id":10,"kind":"Upgrade","instance_id":"brew:/opt/homebrew","artifact_kind":"Formula","name":"libpng","status":"Done","outcome":"Succeeded","argv_preview":[],"env_preview":[],"cancel_policy":"KillThenReconcile","already_updated":"ByEarlierUpdate"}"#
        );
        assert_eq!(
            serde_json::to_string(&AlreadyUpdated::BeforeItsTurn).unwrap(),
            r#""BeforeItsTurn""#
        );
        let older = json.replace(r#","already_updated":"ByEarlierUpdate""#, "");
        let back: OpSummary = serde_json::from_str(&older).unwrap();
        assert_eq!(back.already_updated, None);
    }
}

#[cfg(test)]
mod evicted_tests {
    use super::*;

    #[test]
    fn regression_an_evicted_failure_keeps_that_it_stopped_for_the_password() {
        // Astra's final review, F1: eviction kept only `Ended::Failed`, so
        // an update that stopped where sudo wanted the Mac's password was
        // told of as one that could not be updated once its record went.
        let mut ledger = EvictedLedger::default();
        let stopped = Outcome::Failed {
            exit_code: Some(1),
            summary: "==> Upgrading tool\nsudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper".into(),
            cause: Some(crate::history::FailureCause::NeedsPassword),
        };
        let failed = Outcome::Failed {
            exit_code: Some(1),
            summary: "Error: Download failed".into(),
            cause: None,
        };
        ledger.keep(1, OpKind::Upgrade, Some(&stopped), 3);
        ledger.keep(2, OpKind::Upgrade, Some(&failed), 3);
        ledger.keep(
            3,
            OpKind::Upgrade,
            Some(&Outcome::BanagerFailed(Fault::Panicked)),
            3,
        );
        assert_eq!(
            ledger.ops.values().map(|op| op.ended).collect::<Vec<_>>(),
            [Ended::NeedsPassword, Ended::Failed, Ended::Failed]
        );
    }

    #[test]
    fn test_evicted_operations_are_kept_until_accepted_and_past_the_cap_are_forgotten() {
        let mut ledger = EvictedLedger::default();
        let failed = Outcome::Failed {
            exit_code: Some(1),
            summary: "no".into(),
            cause: None,
        };
        ledger.keep(1, OpKind::Upgrade, Some(&Outcome::Succeeded), 3);
        ledger.keep(2, OpKind::Uninstall, Some(&failed), 3);
        ledger.keep(3, OpKind::Upgrade, Some(&Outcome::Unconfirmed), 3);
        assert_eq!(
            ledger.ops.values().map(|op| op.ended).collect::<Vec<_>>(),
            [Ended::Succeeded, Ended::Failed, Ended::Attention]
        );
        assert_eq!(ledger.forgotten_through, 0);
        ledger.keep(4, OpKind::Upgrade, Some(&Outcome::Cancelled), 3);
        assert_eq!(ledger.ops.keys().copied().collect::<Vec<_>>(), [2, 3, 4]);
        assert_eq!(ledger.forgotten_through, 1, "the oldest, past the cap");
        ledger.keep(5, OpKind::Upgrade, None, 3);
        assert_eq!(ledger.forgotten_through, 5, "nothing to keep is forgotten");
        ledger.forget_through(3);
        assert_eq!(ledger.ops.keys().copied().collect::<Vec<_>>(), [4]);
        ledger.forget_through(OpId::MAX);
        assert!(ledger.ops.is_empty());
    }

    /// An adapter whose every operation succeeds at once: enough to put
    /// operations through `run_operation` to `Done`.
    struct Instant(crate::adapters::AdapterMeta);

    #[async_trait::async_trait]
    impl Adapter for Instant {
        fn meta(&self) -> &crate::adapters::AdapterMeta {
            &self.0
        }
        async fn detect(&self, _env: &crate::runner::HostEnv) -> Vec<ManagerInstance> {
            Vec::new()
        }
        async fn inventory(
            &self,
            _inst: &ManagerInstance,
        ) -> Result<Vec<crate::model::InstalledArtifact>, AdapterError> {
            Ok(Vec::new())
        }
        async fn check_updates(
            &self,
            _inst: &ManagerInstance,
            _opts: &crate::adapters::CheckOptions,
        ) -> Result<crate::adapters::CheckOutcome, AdapterError> {
            Ok(Default::default())
        }
        async fn search(
            &self,
            _inst: &ManagerInstance,
            _query: &str,
        ) -> Result<Vec<crate::model::SearchHit>, AdapterError> {
            Ok(Vec::new())
        }
        async fn plan(
            &self,
            _inst: &ManagerInstance,
            _req: &crate::model::OpRequest,
        ) -> Result<Plan, AdapterError> {
            Err(AdapterError::Unsupported("not here".into()))
        }
        async fn execute(
            &self,
            _plan: &Plan,
            _sink: Arc<dyn EventSink>,
            _op_id: OpId,
            _cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
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

    fn install(inst: &ManagerInstance, name: &str, lock: &str) -> Plan {
        Plan {
            request: crate::model::OpRequest {
                kind: OpKind::Install,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Formula,
                name: name.into(),
            },
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args: vec!["install".into(), name.into()],
                env: vec![],
            },
            needs_password: false,
            locks: vec![ResourceLock(lock.into())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: vec![],
            affected: vec![],
            timeout_secs: 60,
        }
    }

    /// A submission that has taken its id and joined the queue but whose
    /// record is not in yet (`reserve` without `record_and_run`, the seam
    /// `submit_with` is made of), while later operations on another
    /// resource finish and overflow the evicted ledger past its id. The
    /// overflow proves nothing about it: no report may cross it until it
    /// has finished, and then it is told of.
    #[tokio::test]
    async fn regression_overflow_recovery_never_crosses_an_id_reserved_but_not_recorded() {
        use crate::notify_operations::ReportedRuns;
        let mut manager =
            OperationManager::new(Arc::new(crate::events::VecSink::new())).with_max_records(1);
        manager.max_evicted = 1;
        manager.register_adapter(Arc::new(Instant(crate::adapters::AdapterMeta {
            id: "fake".into(),
            name: "fake".into(),
            kind: "fake".into(),
            platforms: vec!["macos".into()],
            homepage: "https://example.invalid".into(),
            schema_version: 1,
            verified_versions: vec![],
        })));
        let manager = Arc::new(manager);
        let inst = crate::testing::manager_instance("fake", "fake:1");
        manager.register_instance(inst.clone());

        let held_back = install(&inst, "aaa", "a");
        let first = manager.reserve(&held_back);
        let mut last = first;
        for name in ["bbb", "ccc", "ddd"] {
            last = manager.submit(install(&inst, name, "b"));
            manager.wait(last).await;
        }
        let known = manager.completions_after(0);
        assert!(
            known.forgotten_through >= first,
            "the ledger has overflowed past the reserved id"
        );
        let reported = ReportedRuns::default();
        assert_eq!(reported.completed(first, &known), None);
        assert_eq!(reported.completed(last, &known), None);

        manager.record_and_run(first, held_back, None, None);
        assert_eq!(manager.wait(first).await, Some(Outcome::Succeeded));
        let told = reported
            .completed(first, &manager.completions_after(reported.through()))
            .expect("told of once it has finished");
        assert_eq!(told.succeeded, 1);
    }
}
