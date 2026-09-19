use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
use canager_core::events::{EventSink, OpId, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    OpStatus, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

struct FakeAdapter {
    meta: AdapterMeta,
    /// When `gated` is set, `execute()` waits for `release.notified()`
    /// before returning instead of completing immediately. Lets a test hold
    /// an operation in `Running` for as long as it likes — with no reliance
    /// on real time — so it can register several `wait()` callers before
    /// choosing the exact moment the operation finishes. Unset by default,
    /// so every test that does not opt in still completes immediately.
    gated: std::sync::atomic::AtomicBool,
    release: Arc<tokio::sync::Notify>,
}

impl FakeAdapter {
    fn new() -> FakeAdapter {
        FakeAdapter {
            meta: AdapterMeta {
                id: "fake".to_string(),
                name: "fake".to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            gated: std::sync::atomic::AtomicBool::new(false),
            release: Arc::new(tokio::sync::Notify::new()),
        }
    }
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
            background_check: false,
            cancel_safe: true,
        }
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
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        Ok(Vec::new())
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
            program: inst.exe_path.clone(),
            args: vec!["install".to_string(), req.name.clone()],
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
        if self.gated.load(std::sync::atomic::Ordering::SeqCst) {
            self.release.notified().await;
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

fn make_instance(id: &str) -> ManagerInstance {
    ManagerInstance {
        id: id.to_string(),
        adapter_id: "fake".to_string(),
        exe_path: PathBuf::from("/bin/true"),
        prefix: PathBuf::from("/"),
        scope: Scope::User,
        version: None,
        healthy: true,
        unverified_version: None,
    }
}

fn make_request(name: &str, instance_id: &str) -> OpRequest {
    OpRequest {
        kind: OpKind::Install,
        instance_id: instance_id.to_string(),
        artifact_kind: ArtifactKind::Formula,
        name: name.to_string(),
    }
}

#[tokio::test]
async fn test_summaries_is_empty_before_anything_is_submitted() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(Arc::new(FakeAdapter::new()));
    let manager = Arc::new(manager);
    assert!(manager.summaries().is_empty());
}

#[tokio::test]
async fn test_summaries_reflects_a_submitted_operation_and_its_argv_preview() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let req = make_request("jq", "fake:1");
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    let outcome = manager.wait(op_id).await;
    assert_eq!(outcome, Some(Outcome::Succeeded));

    let summaries = manager.summaries();
    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    assert_eq!(summary.id, op_id);
    assert_eq!(summary.kind, OpKind::Install);
    assert_eq!(summary.instance_id, "fake:1");
    assert_eq!(summary.artifact_kind, ArtifactKind::Formula);
    assert_eq!(summary.name, "jq");
    assert_eq!(summary.status, OpStatus::Done);
    assert_eq!(summary.outcome, Some(Outcome::Succeeded));
    assert_eq!(
        summary.argv_preview,
        vec![
            "/bin/true".to_string(),
            "install".to_string(),
            "jq".to_string()
        ]
    );
}

#[tokio::test]
async fn test_summaries_are_ordered_newest_first() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan_a = adapter
        .plan(&inst, &make_request("aaa", "fake:1"))
        .await
        .unwrap();
    let id_a = manager.submit(plan_a);
    manager.wait(id_a).await;

    let plan_b = adapter
        .plan(&inst, &make_request("bbb", "fake:1"))
        .await
        .unwrap();
    let id_b = manager.submit(plan_b);
    manager.wait(id_b).await;

    let summaries = manager.summaries();
    assert_eq!(summaries.len(), 2);
    assert_eq!(
        summaries[0].id, id_b,
        "the more recently submitted op must come first"
    );
    assert_eq!(summaries[1].id, id_a);
}

#[tokio::test]
async fn test_wait_does_not_hang_after_finish() {
    // This only guards against an unbounded stall (e.g. a regression to a
    // dropped notification that never wakes `wait()` at all). It does
    // *not* prove the 20ms poll loop is gone — the old poll-based
    // implementation would pass this same assertion, just slower — so it
    // must not be read as a performance regression test. That property is
    // covered by `test_multiple_waiters_all_wake_once_the_op_finishes`
    // below, which uses a controlled synchronization point instead of a
    // wall-clock bound and so cannot flake under CI load the way tightening
    // this timeout would.
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan = adapter
        .plan(&inst, &make_request("jq", "fake:1"))
        .await
        .unwrap();
    let op_id = manager.submit(plan);
    let outcome = tokio::time::timeout(Duration::from_millis(200), manager.wait(op_id))
        .await
        .expect("wait() should not hang");
    assert_eq!(outcome, Some(Outcome::Succeeded));
}

#[tokio::test]
async fn test_multiple_waiters_all_wake_once_the_op_finishes() {
    // Exercises the actual race `wait()`'s "create `notified()` before
    // checking status" ordering exists to prevent, with several concurrent
    // waiters instead of one. The synchronization is entirely deterministic
    // — a gate on `execute()` plus cooperative yielding, no sleeps or
    // timing thresholds — so this cannot be flaky under CI load the way a
    // tightened wall-clock bound would be.
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    adapter
        .gated
        .store(true, std::sync::atomic::Ordering::SeqCst);
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan = adapter
        .plan(&inst, &make_request("jq", "fake:1"))
        .await
        .unwrap();
    let op_id = manager.submit(plan);

    // Wait until `execute()` has actually been entered and is blocked on
    // the gate (status == Running), so there is no window in which the op
    // could finish before any waiter is spawned.
    loop {
        if manager.record(op_id).map(|r| r.status) == Some(OpStatus::Running) {
            break;
        }
        tokio::task::yield_now().await;
    }

    let waiters: Vec<_> = (0..5)
        .map(|_| {
            let manager = manager.clone();
            tokio::spawn(async move { manager.wait(op_id).await })
        })
        .collect();

    // Give every spawned waiter a chance to run up to its `notified().await`
    // point before the op is allowed to finish, so this test actually
    // exercises concurrent registered waiters rather than each one simply
    // observing an already-`Done` status.
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    // `notify_one`, not `notify_waiters`: exactly one `execute()` call is
    // ever blocked on this gate, and `notify_one` (unlike `notify_waiters`)
    // stores its permit if `execute()` has not reached the await yet, so
    // this can never race the gate itself.
    adapter.release.notify_one();

    for w in waiters {
        assert_eq!(
            w.await.expect("waiter task panicked"),
            Some(Outcome::Succeeded)
        );
    }
}
