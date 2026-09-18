use crate::adapters::Adapter;
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, ArtifactKey, InstanceId, ManagerInstance, OpKind, OpStatus, Outcome, Plan,
    ResourceLock,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

pub struct OpRecord {
    pub id: OpId,
    pub plan: Plan,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
    pub cancel: CancellationToken,
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

    pub fn record(&self, op_id: OpId) -> Option<OpRecord> {
        let records = self.records.lock().unwrap();
        records.get(&op_id).map(|r| OpRecord {
            id: r.id,
            plan: r.plan.clone(),
            status: r.status,
            outcome: r.outcome.clone(),
            cancel: r.cancel.clone(),
        })
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
            {
                let records = self.records.lock().unwrap();
                match records.get(&op_id) {
                    Some(r) if r.status == OpStatus::Done => return r.outcome.clone(),
                    None => return None,
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
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
        self.records.lock().unwrap().insert(op_id, record);
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
                    manager_for_panic.finish(
                        op_id,
                        Outcome::Failed {
                            exit_code: None,
                            summary: "operation panicked".to_string(),
                        },
                        true,
                    );
                }
            }
        });

        op_id
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
                // another op), so nothing on the system changed.
                self.finish(op_id, Outcome::NoChange, false);
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
            self.finish(op_id, Outcome::NoChange, true);
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
                self.finish(op_id, Outcome::NoChange, true);
                return;
            }
            permit = self.semaphore.clone().acquire_owned() => {
                match permit {
                    Ok(p) => p,
                    Err(_) => {
                        // The semaphore was closed (manager torn down);
                        // treat exactly like a cancellation.
                        self.finish(op_id, Outcome::NoChange, true);
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
        let instance = match instance {
            Some(i) => i,
            None => {
                self.finish(
                    op_id,
                    Outcome::Failed {
                        exit_code: None,
                        summary: format!("unknown instance {}", plan.request.instance_id),
                    },
                    true,
                );
                return;
            }
        };

        let adapter = self.adapters.get(&instance.adapter_id).cloned();
        let adapter = match adapter {
            Some(a) => a,
            None => {
                self.finish(
                    op_id,
                    Outcome::Failed {
                        exit_code: None,
                        summary: format!("no adapter registered for {}", instance.adapter_id),
                    },
                    true,
                );
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
                            Outcome::NeedsAttention(
                                "command succeeded but the package is not installed".to_string(),
                            )
                        }
                    }
                    OpKind::Uninstall => {
                        if !r.present {
                            Outcome::Succeeded
                        } else {
                            Outcome::NeedsAttention(
                                "command succeeded but the package is still installed".to_string(),
                            )
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
                            Outcome::NeedsAttention("package disappeared after upgrade".to_string())
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
            Ok(Outcome::Unconfirmed) => match plan.request.kind {
                OpKind::Upgrade => Outcome::Unconfirmed,
                OpKind::Install => match reconciled {
                    Ok(r) if r.present => Outcome::Succeeded,
                    _ => Outcome::Unconfirmed,
                },
                OpKind::Uninstall => match reconciled {
                    Ok(r) if !r.present => Outcome::Succeeded,
                    _ => Outcome::Unconfirmed,
                },
            },
            Ok(other) => other,
            Err(e) => Outcome::Failed {
                exit_code: None,
                summary: e.to_string(),
            },
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
        self.sink.emit(OperationEvent::Finished { op_id, outcome });
    }
}
