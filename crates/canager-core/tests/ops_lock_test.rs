use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
use canager_core::events::{EventSink, OpId, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// A minimal `Adapter` whose `execute` just sleeps and records when it ran,
/// so the tests below can prove same-lock plans never overlap while
/// different-lock plans do.
struct FakeAdapter {
    meta: AdapterMeta,
    log: Arc<Mutex<Vec<(String, Instant, Instant)>>>,
}

impl FakeAdapter {
    fn new(id: &str, log: Arc<Mutex<Vec<(String, Instant, Instant)>>>) -> FakeAdapter {
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
            args: vec![],
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
        plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_millis(200)).await;
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
        id: id.to_string(),
        adapter_id: "fake".to_string(),
        exe_path: PathBuf::from("/bin/true"),
        prefix: PathBuf::from(prefix),
        scope: Scope::User,
        version: None,
        healthy: true,
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
