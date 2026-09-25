//! (F9 / M1) Contract test: a panic inside `Adapter::execute` must not leak
//! the resource lock forever. Before this fix, locks were only released by
//! an explicit `finish()` call; a panicking adapter (or event callback)
//! meant the background task's `JoinHandle` was simply dropped, so neither
//! `held` nor the op's record were ever cleaned up — the op's `wait()` would
//! hang forever, and every later op on the same resource would queue behind
//! a lock nobody would ever release.

use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use canager_core::events::{EventSink, OpId, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, Fault, InstalledArtifact, ManagerInstance, OpKind,
    OpRequest, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// `OperationManager::wait` polls forever with no built-in timeout, so if
/// the panic-safety fix regresses and the record never reaches `Done`, a
/// bare `.await` here would hang the test suite instead of failing it.
async fn wait_with_timeout(
    manager: &Arc<OperationManager>,
    op_id: canager_core::events::OpId,
) -> Outcome {
    tokio::time::timeout(Duration::from_secs(5), manager.wait(op_id))
        .await
        .expect("op did not reach Done within 5s — the panic likely leaked its lock/record")
        .expect("wait() returned None — record disappeared")
}

struct FakeAdapter {
    meta: AdapterMeta,
    should_panic: Arc<AtomicBool>,
}

impl FakeAdapter {
    fn new(should_panic: Arc<AtomicBool>) -> FakeAdapter {
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
            should_panic,
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
        _plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        if self.should_panic.load(Ordering::SeqCst) {
            panic!("simulated adapter panic mid-execute");
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
        version: None,
        ..canager_core::testing::manager_instance("fake", id)
    }
}

#[tokio::test]
async fn test_panic_in_execute_reports_a_canager_fault_and_releases_the_lock() {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let should_panic = Arc::new(AtomicBool::new(true));
    let adapter = Arc::new(FakeAdapter::new(should_panic.clone()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/panic");
    manager.register_instance(inst.clone());

    // First op: the adapter panics inside `execute`.
    let req1 = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "a".to_string(),
    };
    let plan1 = adapter.plan(&inst, &req1).await.expect("plan1");
    let id1 = manager.submit(plan1);

    let outcome1 = wait_with_timeout(&manager, id1).await;
    match outcome1 {
        // Canager's own failure, as a reason the front end words -- not an
        // English sentence in `Failed`'s `summary`, which is the tool's.
        Outcome::CanagerFailed(Fault::Panicked) => {}
        other => panic!("expected CanagerFailed(Panicked), got {other:?}"),
    }

    // Second op, same resource lock: if the panic leaked the lock, this
    // would queue forever and `wait` would never return `Some(_)`.
    should_panic.store(false, Ordering::SeqCst);
    let req2 = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "b".to_string(),
    };
    let plan2 = adapter.plan(&inst, &req2).await.expect("plan2");
    let id2 = manager.submit(plan2);
    let outcome2 = wait_with_timeout(&manager, id2).await;
    assert_eq!(outcome2, Outcome::Succeeded);
}
