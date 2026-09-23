use crate::adapters::{Adapter, AdapterError};
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, ArtifactKey, ArtifactKind, Attention, Fault, InstanceId, ManagerInstance, OpKind,
    OpStatus, Outcome, Plan, ResourceLock,
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
/// `InvalidName`, `NotActionable` or `IndexUpdating` (brew's `execute`
/// waits for a running `brew update` instead; only its `inventory` and
/// `check_updates` return that). Everything but the two runner errors
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
        | AdapterError::IndexUpdating => Fault::Internal,
    };
    Outcome::CanagerFailed(fault)
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
    pub argv_preview: Vec<String>, // program followed by args
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
                let mut argv_preview = vec![r.plan.program.to_string_lossy().to_string()];
                argv_preview.extend(r.plan.args.iter().cloned());
                OpSummary {
                    id: r.id,
                    kind: r.plan.request.kind,
                    instance_id: r.plan.request.instance_id.clone(),
                    artifact_kind: r.plan.request.artifact_kind,
                    name: r.plan.request.name.clone(),
                    status: r.status,
                    outcome: r.outcome.clone(),
                    argv_preview,
                }
            })
            .collect();
        summaries.sort_by_key(|s| std::cmp::Reverse(s.id));
        summaries
    }

    pub fn cancel(&self, op_id: OpId) {
        let mut records = self.records.lock().unwrap();
        if let Some(r) = records.get_mut(&op_id) {
            // Only a still-pending op can be cancelled. Once it has moved
            // past Running (Verifying/Done) — or is already
            // CancelRequested/Cancelling — cancelling again must be a
            // no-op: forcing it back to CancelRequested here would corrupt
            // a finished record and make `wait()` (which only returns on
            // Done) hang forever.
            if !matches!(r.status, OpStatus::Queued | OpStatus::Running) {
                return;
            }
            r.status = OpStatus::CancelRequested;
            r.cancel.cancel();
            drop(records);
            self.sink.emit(OperationEvent::Status {
                op_id,
                status: OpStatus::CancelRequested,
            });
        }
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
        // `wait_for_update` blocks on Homebrew's own update lock
        // (`update_lock_for`) or a bounded sleep
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

        let exec_result = adapter
            .execute(&plan, self.sink.clone(), op_id, cancel.clone())
            .await;

        if cancel.is_cancelled() {
            self.set_status(op_id, OpStatus::Cancelling);
        }
        self.set_status(op_id, OpStatus::Verifying);

        let key = ArtifactKey {
            instance_id: plan.request.instance_id.clone(),
            kind: plan.request.artifact_kind,
            name: plan.request.name.clone(),
        };
        let reconciled = adapter.reconcile(&instance, &key).await;

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
                    OpKind::Upgrade => {
                        // There is no target version to compare against
                        // here, so presence is the strongest evidence
                        // available: still present after an upgrade that
                        // reported success is as good as it gets.
                        if r.present {
                            Outcome::Succeeded
                        } else {
                            Outcome::NeedsAttention(Attention::GoneAfterUpgrade)
                        }
                    }
                },
            },
            // A cancelled/timed-out execute only tells us the artifact's
            // *current* presence, not whether this op caused it. That is
            // proof of success for Install (wasn't there, now is) and
            // Uninstall (was there, now isn't) — but never for Upgrade,
            // since the artifact was already present before the op ran, so
            // presence afterward proves nothing either way.
            //
            // When the run was stopped by the user's own Cancel (the token
            // is only ever fired by `cancel()`; a timeout never touches
            // it) and reconcile shows the request did *not* take effect,
            // the cancel is what happened, and the user is told so. If the
            // work finished anyway, the presence arms below report
            // `Succeeded`, not `Cancelled`: the race goes to whatever
            // reconcile actually found. Upgrade stays `Unconfirmed` even after a user cancel,
            // for the same reason as above — nothing here can tell whether
            // the new version landed before the kill, and `Cancelled` would
            // read as "it did not".
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
