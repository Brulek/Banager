use crate::adapters::{Adapter, AdapterError};
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, ArtifactKey, ArtifactKind, Attention, CancelPolicy, Fault, InstanceId,
    ManagerInstance, OpKind, OpStatus, Outcome, Plan, PlanAction, Reconciled, ResourceLock,
};
use crate::runner::RunnerError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

/// Default cap on how many finished (`Done`) operations `records` keeps at
/// once; an operation still in flight is never evicted regardless of this
/// bound. Bounds a long-running session's memory use -- without it, every
/// operation ever submitted in the process's lifetime stays in `records`
/// (and therefore in `summaries()`) forever.
const DEFAULT_MAX_RECORDS: usize = 200;

/// The `Outcome` for an `Err` out of `Adapter::execute`.
///
/// Every `Err` here is a reason of Canager's own, worded by the front end
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
/// is therefore a bug in Canager, and says so as `Fault::Internal` rather
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
        | AdapterError::NotActionable { .. }
        | AdapterError::UpdateBlocked { .. }
        | AdapterError::UninstallBlocked { .. }
        | AdapterError::UninstallUnsafe { .. }
        | AdapterError::IndexUpdating => Fault::Internal,
    };
    Outcome::CanagerFailed(fault)
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
}

pub struct OperationManager {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    instances: Mutex<HashMap<InstanceId, ManagerInstance>>,
    sink: Arc<dyn EventSink>,
    held: Arc<Mutex<HashSet<ResourceLock>>>,
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
    /// The plan's `cancel_policy`. The front end reads it with `status` to
    /// offer no Cancel button for a Running `NoCancel` op
    /// (`OperationBar.tsx`), the one op `cancel` below refuses by policy.
    pub cancel_policy: CancelPolicy,
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
    /// No op has this id, or it is past `Running` (`Verifying`/`Done`), or
    /// a cancel is already in flight (`CancelRequested`/`Cancelling`) --
    /// whatever the plan's policy.
    NotPending,
}

impl OperationManager {
    pub fn new(sink: Arc<dyn EventSink>) -> OperationManager {
        OperationManager {
            adapters: HashMap::new(),
            instances: Mutex::new(HashMap::new()),
            sink,
            held: Arc::new(Mutex::new(HashSet::new())),
            records: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            semaphore: Arc::new(Semaphore::new(3)),
            done_notify: Arc::new(Notify::new()),
            max_records: DEFAULT_MAX_RECORDS,
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
        let records = self.records.lock().unwrap();
        let mut summaries: Vec<OpSummary> = records
            .values()
            .map(|r| {
                let argv_preview = match &r.plan.action {
                    PlanAction::Command { program, args, .. } => {
                        let mut argv = vec![program.to_string_lossy().to_string()];
                        argv.extend(args.iter().cloned());
                        argv
                    }
                    // No command runs, so there is no argv to preview: an
                    // empty list, never an invented one. (`src/` renders
                    // no argv_preview today; the dialog shows the paths
                    // through the plan's `WillTrash` warnings.)
                    PlanAction::TrashPaths { .. } => Vec::new(),
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
                    cancel_policy: r.plan.cancel_policy,
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
        // Done) hang forever.
        if !matches!(r.status, OpStatus::Queued | OpStatus::Running) {
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
        let op_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cancel = CancellationToken::new();
        let record = OpInternal {
            id: op_id,
            plan: plan.clone(),
            status: OpStatus::Queued,
            outcome: None,
            cancel: cancel.clone(),
            lock_release: None,
        };
        {
            let mut records = self.records.lock().unwrap();
            records.insert(op_id, record);
            Self::evict_oldest_done_records(&mut records, self.max_records);
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
                    manager_for_panic.finish(op_id, Outcome::CanagerFailed(Fault::Panicked), true);
                }
            }
        });

        op_id
    }

    /// Evicts the oldest (lowest op id) `Done` records, oldest first, until
    /// either the cap is met or no `Done` record remains. Work that is still
    /// Queued/Running/CancelRequested/Cancelling/Verifying is never evicted,
    /// so `max_records` is a target, not a hard bound: with enough operations
    /// in flight at once, `records` can legitimately sit above it.
    fn evict_oldest_done_records(records: &mut HashMap<OpId, OpInternal>, max_records: usize) {
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
            records.remove(&id);
            overflow -= 1;
        }
    }

    async fn run_operation(self: Arc<Self>, op_id: OpId, plan: Plan, cancel: CancellationToken) {
        // Wait for every lock this plan needs, polling every 50 ms. Only
        // mark `acquired` once every lock in `plan.locks` was free and has
        // now been inserted into `held` — otherwise a later step could
        // release a lock this op never actually took.
        let mut acquired = false;
        loop {
            {
                let mut held = self.held.lock().unwrap();
                if plan.locks.iter().all(|l| !held.contains(l)) {
                    for l in &plan.locks {
                        held.insert(l.clone());
                    }
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
        // (`crates/canager-core/src/adapters/brew/mod.rs`) — up to
        // `op_update_wait` (`OP_UPDATE_WAIT` outside tests, ten minutes),
        // while it waits for a `brew update` a refresh left running to
        // finish. That is a permit held doing nothing but waiting, for as
        // long as ten minutes, not a bug in this reasoning: the DAG claim
        // above is about what a permit holder can be *blocked on*, and
        // `wait_for_update` blocks on Canager's own per-instance update lock
        // (`update_lock_for` -- an in-process `tokio::sync::Mutex`, not
        // Homebrew's file-based `var/homebrew/locks/update`;
        // `wait_for_update`'s doc in
        // `crates/canager-core/src/adapters/brew/mod.rs` distinguishes the
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
                            Outcome::CanagerFailed(Fault::Internal),
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
        // `SubmitError::SourceGone`. So a miss is Canager's own bug.
        let instance = match instance {
            Some(i) => i,
            None => {
                self.finish(op_id, Outcome::CanagerFailed(Fault::Internal), true);
                return;
            }
        };

        let adapter = self.adapters.get(&instance.adapter_id).cloned();
        let adapter = match adapter {
            Some(a) => a,
            None => {
                self.finish(op_id, Outcome::CanagerFailed(Fault::Internal), true);
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

        // A NoCancel plan gets the same live token as any other. Once the
        // op is Running, `cancel` refuses to fire it, so its `execute` ends
        // when the command does or at `plan.timeout_secs`, which the runner
        // keeps on a deadline of its own from spawn (see
        // `CancelRefused::NoCancel`). A token `cancel` fired while the op
        // was still Queued reaches here only if it fired between acquiring
        // the permit and `set_status(Running)` above; `RealRunner::run`
        // then checks it before spawning and starts nothing.
        let exec_result = adapter
            .execute(&plan, self.sink.clone(), op_id, cancel.clone())
            .await;

        if cancel.is_cancelled() {
            self.set_status(op_id, OpStatus::Cancelling);
        }
        self.set_status(op_id, OpStatus::Verifying);

        // After an uninstall only presence decides anything below, and an
        // adapter may answer that when it cannot answer what version is
        // installed (`Adapter::reconcile_after_uninstall`); everything
        // else keeps the full reading.
        let reconciled = match plan.request.kind {
            OpKind::Uninstall => adapter.reconcile_after_uninstall(&instance, &key).await,
            OpKind::Install | OpKind::Upgrade => adapter.reconcile(&instance, &key).await,
        };

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
                    OpKind::Upgrade => {
                        if !r.present {
                            Outcome::NeedsAttention(Attention::GoneAfterUpgrade)
                        } else {
                            match version_change(before.as_ref(), &r) {
                                VersionChange::Unchanged => {
                                    Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
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
            // (adapters/standalone/removal.rs) returns it for a Cancel or
            // a spent budget before an item and for an item whose move
            // panicked; brew's own Cancel, while it waits for a `brew
            // update`, is `Cancelled` before its command starts. The
            // reading after it tells us the artifact's *current* state,
            // not whether this op caused it. Presence is proof enough for
            // Install (wasn't there, now is) and Uninstall (was there, now
            // isn't).
            //
            // When the run was stopped by the user's own Cancel (the token
            // is only ever fired by `cancel()`; neither a timeout nor a
            // signal from outside touches it) and reconcile shows the
            // install or uninstall did *not* take effect, the cancel is
            // what happened, and the user is told so. If the work finished
            // anyway, the arms below report `Succeeded`, not `Cancelled`:
            // the race goes to whatever reconcile actually found. A run a
            // signal ended with the token unfired stays `Unconfirmed`
            // here, never `Cancelled`: nobody pressed Cancel.
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
                    OpKind::Upgrade => Outcome::Unconfirmed,
                    OpKind::Install => match reconciled {
                        Ok(r) if r.present => Outcome::Succeeded,
                        Ok(_) if user_cancelled => Outcome::Cancelled,
                        _ => Outcome::Unconfirmed,
                    },
                    OpKind::Uninstall => match reconciled {
                        Ok(r) if !r.present => Outcome::Succeeded,
                        Ok(_) if user_cancelled => Outcome::Cancelled,
                        _ => Outcome::Unconfirmed,
                    },
                }
            }
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
        {
            let mut records = self.records.lock().unwrap();
            if let Some(r) = records.get_mut(&op_id) {
                r.status = OpStatus::Done;
                r.outcome = Some(outcome.clone());
                if release_locks {
                    if let Some(lr) = &r.lock_release {
                        lr.release_once();
                    }
                }
            }
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

impl OperationManager {
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
