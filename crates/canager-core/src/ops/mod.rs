use crate::adapters::Adapter;
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, ArtifactKey, InstanceId, ManagerInstance, OpKind, OpStatus, Outcome, Plan,
    ResourceLock,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct OpRecord {
    pub id: OpId,
    pub plan: Plan,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
    pub cancel: CancellationToken,
}

pub struct OperationManager {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    instances: Mutex<HashMap<InstanceId, ManagerInstance>>,
    sink: Arc<dyn EventSink>,
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    records: Arc<Mutex<HashMap<OpId, OpRecord>>>,
    next_id: AtomicU64,
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
        let record = OpRecord {
            id: op_id,
            plan: plan.clone(),
            status: OpStatus::Queued,
            outcome: None,
            cancel: cancel.clone(),
        };
        self.records.lock().unwrap().insert(op_id, record);
        self.sink.emit(OperationEvent::Status {
            op_id,
            status: OpStatus::Queued,
        });

        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.run_operation(op_id, plan, cancel).await;
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
                self.finish(op_id, Outcome::Unconfirmed, false);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

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
            Ok(Outcome::Unconfirmed) => match reconciled {
                Ok(r) => {
                    let present_means_success = plan.request.kind != OpKind::Uninstall;
                    if r.present == present_means_success {
                        Outcome::Succeeded
                    } else {
                        Outcome::Unconfirmed
                    }
                }
                Err(_) => Outcome::Unconfirmed,
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
    /// its locks (cancelled while still waiting for them) — otherwise this
    /// would release locks a *different*, still-running op is holding.
    fn finish(&self, op_id: OpId, outcome: Outcome, release_locks: bool) {
        {
            let mut records = self.records.lock().unwrap();
            if let Some(r) = records.get_mut(&op_id) {
                r.status = OpStatus::Done;
                r.outcome = Some(outcome.clone());
                if release_locks {
                    let mut held = self.held.lock().unwrap();
                    for l in &r.plan.locks {
                        held.remove(l);
                    }
                }
            }
        }
        self.sink.emit(OperationEvent::Finished { op_id, outcome });
    }
}
