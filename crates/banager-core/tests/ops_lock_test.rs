use async_trait::async_trait;
use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use banager_core::events::{EventSink, OpId, VecSink};
use banager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    OpStatus, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
};
use banager_core::ops::OperationManager;
use banager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// What `FakeAdapter::execute` ran: the plan's name, when it started, when
/// it ended.
type RunLog = Arc<Mutex<Vec<(String, Instant, Instant)>>>;

/// A minimal `Adapter` whose `execute` just sleeps and records when it ran,
/// so the tests below can prove same-lock plans never overlap while
/// different-lock plans do. A plan whose name starts with "hold" also waits
/// for `gate` before it ends, so a test can keep its lock taken for as long
/// as it needs.
struct FakeAdapter {
    meta: AdapterMeta,
    log: Arc<Mutex<Vec<(String, Instant, Instant)>>>,
    delay: Duration,
    gate: CancellationToken,
}

impl FakeAdapter {
    fn new(id: &str, log: Arc<Mutex<Vec<(String, Instant, Instant)>>>) -> FakeAdapter {
        FakeAdapter::with_delay(id, log, Duration::from_millis(200))
    }

    fn with_delay(
        id: &str,
        log: Arc<Mutex<Vec<(String, Instant, Instant)>>>,
        delay: Duration,
    ) -> FakeAdapter {
        FakeAdapter {
            meta: AdapterMeta {
                id: id.to_string(),
                name: id.to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            log,
            delay,
            gate: CancellationToken::new(),
        }
    }
}

#[async_trait]
impl Adapter for FakeAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
        Vec::new()
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

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args: vec![],
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
        plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        let start = Instant::now();
        tokio::time::sleep(self.delay).await;
        if plan.request.name.starts_with("hold") {
            self.gate.cancelled().await;
        }
        let end = Instant::now();
        self.log
            .lock()
            .unwrap()
            .push((plan.request.name.clone(), start, end));
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

fn make_instance(id: &str, prefix: &str) -> ManagerInstance {
    ManagerInstance {
        prefix: PathBuf::from(prefix),
        version: None,
        ..banager_core::testing::manager_instance("fake", id)
    }
}

fn overlaps(a_start: &Instant, a_end: &Instant, b_start: &Instant, b_end: &Instant) -> bool {
    a_start < b_end && b_start < a_end
}

#[tokio::test]
async fn test_same_lock_runs_serially() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let adapter = Arc::new(FakeAdapter::new("fake", log.clone()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/shared", "/shared");
    manager.register_instance(inst.clone());

    let req_a = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "a".to_string(),
    };
    let req_b = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "b".to_string(),
    };

    let plan_a = adapter.plan(&inst, &req_a).await.expect("plan a");
    let plan_b = adapter.plan(&inst, &req_b).await.expect("plan b");

    let id_a = manager.submit(plan_a);
    let id_b = manager.submit(plan_b);

    manager.wait(id_a).await;
    manager.wait(id_b).await;

    let entries = log.lock().unwrap().clone();
    assert_eq!(entries.len(), 2);
    let (_, start_a, end_a) = &entries[0];
    let (_, start_b, end_b) = &entries[1];
    assert!(
        !overlaps(start_a, end_a, start_b, end_b),
        "same-lock operations overlapped: {:?}",
        entries
    );
}

#[tokio::test]
async fn test_different_locks_run_concurrently() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let adapter = Arc::new(FakeAdapter::new("fake", log.clone()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst_a = make_instance("fake:/a", "/a");
    let inst_b = make_instance("fake:/b", "/b");
    manager.register_instance(inst_a.clone());
    manager.register_instance(inst_b.clone());

    let req_a = OpRequest {
        kind: OpKind::Install,
        instance_id: inst_a.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "a".to_string(),
    };
    let req_b = OpRequest {
        kind: OpKind::Install,
        instance_id: inst_b.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "b".to_string(),
    };

    let plan_a = adapter.plan(&inst_a, &req_a).await.expect("plan a");
    let plan_b = adapter.plan(&inst_b, &req_b).await.expect("plan b");

    let id_a = manager.submit(plan_a);
    let id_b = manager.submit(plan_b);

    manager.wait(id_a).await;
    manager.wait(id_b).await;

    let entries = log.lock().unwrap().clone();
    assert_eq!(entries.len(), 2);
    let (_, start_a, end_a) = &entries[0];
    let (_, start_b, end_b) = &entries[1];
    assert!(
        overlaps(start_a, end_a, start_b, end_b),
        "different-lock operations did not overlap: {:?}",
        entries
    );
}

// ---- Same-lock operations start in the order they were submitted ----
//
// `run_operation` waits for its locks by polling every 50 ms. Before the
// queue existed, whichever waiting op happened to poll first after a lock
// came free took it, so two uninstalls confirmed on one Homebrew in the
// order "pipx, then python@3.13" could start the other way round. These
// tests pin the order (`OperationManager`'s `queue`).

fn request(inst: &ManagerInstance, name: &str) -> OpRequest {
    OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: name.to_string(),
    }
}

/// A plan of `adapter`'s for `name` on `inst` that takes exactly `locks`.
async fn plan_with_locks(
    adapter: &FakeAdapter,
    inst: &ManagerInstance,
    name: &str,
    locks: &[&str],
) -> Plan {
    let mut plan = adapter
        .plan(inst, &request(inst, name))
        .await
        .expect("plan");
    plan.locks = locks.iter().map(|l| ResourceLock(l.to_string())).collect();
    plan
}

/// The names `execute` ran, in the order they started.
fn start_order(log: &RunLog) -> Vec<String> {
    let mut entries = log.lock().unwrap().clone();
    entries.sort_by_key(|(_, start, _)| *start);
    entries.into_iter().map(|(name, _, _)| name).collect()
}

/// Waits until `op_id` has taken its locks and is running.
async fn until_running(manager: &Arc<OperationManager>, op_id: OpId) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while manager.record(op_id).map(|r| r.status) != Some(OpStatus::Running) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the op never started running");
}

async fn wait_done(manager: &Arc<OperationManager>, op_id: OpId) -> Outcome {
    tokio::time::timeout(Duration::from_secs(5), manager.wait(op_id))
        .await
        .expect("the op did not finish within 5s -- left waiting behind a stale queue entry?")
        .expect("the op's record disappeared")
}

/// One round: "hold" takes the lock, four more ops on the same lock are
/// submitted at spread-out moments (so their 50 ms polls fall at different
/// points of the cycle, which is what let the old loop start them in any
/// order), then the lock comes free.
async fn same_lock_round(round: u64) -> Vec<String> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::with_delay(
        "fake",
        log.clone(),
        Duration::from_millis(5),
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:/order", "/order");
    manager.register_instance(inst.clone());

    let hold = manager.submit(plan_with_locks(&adapter, &inst, "hold", &["L"]).await);
    until_running(&manager, hold).await;
    let mut ids = vec![hold];
    for n in 2..=5u64 {
        ids.push(manager.submit(plan_with_locks(&adapter, &inst, &n.to_string(), &["L"]).await));
        tokio::time::sleep(Duration::from_millis((round * 17 + n * 11) % 45)).await;
    }
    tokio::time::sleep(Duration::from_millis((round * 7) % 50)).await;
    adapter.gate.cancel();
    for id in ids {
        assert_eq!(wait_done(&manager, id).await, Outcome::Succeeded);
    }
    start_order(&log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_same_lock_ops_start_in_submission_order() {
    // Twenty rounds at once, each on its own manager, so timing luck in
    // any one of them cannot make the order come out right by chance.
    let mut rounds = tokio::task::JoinSet::new();
    for round in 0..20u64 {
        rounds.spawn(same_lock_round(round));
    }
    while let Some(order) = rounds.join_next().await {
        assert_eq!(
            order.expect("round panicked"),
            vec!["hold", "2", "3", "4", "5"],
            "same-lock operations did not start in the order they were submitted"
        );
    }
}

#[tokio::test]
async fn test_queue_does_not_hold_back_other_locks() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::with_delay(
        "fake",
        log.clone(),
        Duration::from_millis(5),
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:/q", "/q");
    manager.register_instance(inst.clone());

    let hold = manager.submit(plan_with_locks(&adapter, &inst, "hold", &["L1"]).await);
    until_running(&manager, hold).await;
    // Waits behind "hold" on L1 ...
    let waiting = manager.submit(plan_with_locks(&adapter, &inst, "waiting", &["L1"]).await);
    // ... which must not keep an op on L2 from starting.
    let other = manager.submit(plan_with_locks(&adapter, &inst, "other", &["L2"]).await);
    assert_eq!(wait_done(&manager, other).await, Outcome::Succeeded);
    assert_eq!(
        manager.record(waiting).map(|r| r.status),
        Some(OpStatus::Queued),
        "the op on L1 should still be waiting for \"hold\""
    );

    adapter.gate.cancel();
    assert_eq!(wait_done(&manager, hold).await, Outcome::Succeeded);
    assert_eq!(wait_done(&manager, waiting).await, Outcome::Succeeded);
    assert_eq!(start_order(&log), vec!["hold", "other", "waiting"]);
}

#[tokio::test]
async fn test_cancelled_waiting_op_leaves_the_queue() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::with_delay(
        "fake",
        log.clone(),
        Duration::from_millis(5),
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:/c", "/c");
    manager.register_instance(inst.clone());

    let hold = manager.submit(plan_with_locks(&adapter, &inst, "hold", &["L"]).await);
    until_running(&manager, hold).await;
    let cancelled = manager.submit(plan_with_locks(&adapter, &inst, "cancelled", &["L"]).await);
    let after = manager.submit(plan_with_locks(&adapter, &inst, "after", &["L"]).await);

    assert_eq!(manager.cancel(cancelled), Ok(()));
    assert_eq!(wait_done(&manager, cancelled).await, Outcome::Cancelled);

    adapter.gate.cancel();
    assert_eq!(wait_done(&manager, hold).await, Outcome::Succeeded);
    // Had the cancelled op stayed in the queue, "after" would wait for it
    // for ever.
    assert_eq!(wait_done(&manager, after).await, Outcome::Succeeded);
    assert_eq!(start_order(&log), vec!["hold", "after"]);
}

#[tokio::test]
async fn test_multi_lock_op_keeps_its_turn() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::with_delay(
        "fake",
        log.clone(),
        Duration::from_millis(5),
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:/m", "/m");
    manager.register_instance(inst.clone());

    let hold = manager.submit(plan_with_locks(&adapter, &inst, "hold", &["L1"]).await);
    until_running(&manager, hold).await;
    // Needs L1 (taken) and L2 (free): it waits for "hold" ...
    let both = manager.submit(plan_with_locks(&adapter, &inst, "both", &["L1", "L2"]).await);
    // ... and an op confirmed after it on L2 waits its turn, though L2 is
    // free this whole time.
    let later = manager.submit(plan_with_locks(&adapter, &inst, "later", &["L2"]).await);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        manager.record(later).map(|r| r.status),
        Some(OpStatus::Queued),
        "the op on L2 started before the earlier op that also needs L2"
    );

    adapter.gate.cancel();
    for id in [hold, both, later] {
        assert_eq!(wait_done(&manager, id).await, Outcome::Succeeded);
    }
    assert_eq!(start_order(&log), vec!["hold", "both", "later"]);
    let entries = log.lock().unwrap().clone();
    let end_of = |name: &str| entries.iter().find(|(n, _, _)| n == name).unwrap().2;
    let start_of = |name: &str| entries.iter().find(|(n, _, _)| n == name).unwrap().1;
    assert!(
        start_of("later") >= end_of("both"),
        "ops sharing L2 overlapped"
    );
}
