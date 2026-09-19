//! (F2) Contract tests for how `OperationManager::run_operation` combines a
//! `Succeeded` `execute()` result with the post-execution `reconcile()`
//! check into a final `Outcome`.
//!
//! Before this fix, `reconcile`'s result was only consulted when `execute`
//! returned `Unconfirmed` — a `Succeeded` exit code was always taken at face
//! value, so `brew install` exiting 0 without the package actually present
//! (or `brew uninstall` exiting 0 with the package still present) was
//! reported as a silent `Succeeded`, even though `Verifying` had just proven
//! otherwise.

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
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// What `FakeAdapter::reconcile` should report for a given scenario.
#[derive(Clone)]
enum ReconcileBehavior {
    Present(bool),
    Err,
}

struct FakeAdapter {
    meta: AdapterMeta,
    reconcile_behavior: ReconcileBehavior,
}

impl FakeAdapter {
    fn new(reconcile_behavior: ReconcileBehavior) -> FakeAdapter {
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
            reconcile_behavior,
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
        _plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        // Every scenario in this file models a command that reported
        // success; the interesting variable is what `reconcile` finds
        // afterward.
        Ok(Outcome::Succeeded)
    }

    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        match self.reconcile_behavior {
            ReconcileBehavior::Present(present) => Ok(Reconciled {
                present,
                version: None,
            }),
            ReconcileBehavior::Err => Err(AdapterError::Refused("reconcile failed".to_string())),
        }
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
    }
}

/// Submits one op of `kind` against a fresh manager/adapter/instance whose
/// `execute` reports `Succeeded` and whose `reconcile` behaves as given,
/// then returns the final outcome.
async fn run_case(kind: OpKind, reconcile_behavior: ReconcileBehavior) -> Outcome {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let adapter = Arc::new(FakeAdapter::new(reconcile_behavior));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/outcome-matrix");
    manager.register_instance(inst.clone());
    let req = OpRequest {
        kind,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "pkg".to_string(),
    };
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    manager
        .wait(op_id)
        .await
        .expect("op must reach Done and report an outcome")
}

#[tokio::test]
async fn test_succeeded_install_present_is_succeeded() {
    let outcome = run_case(OpKind::Install, ReconcileBehavior::Present(true)).await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_succeeded_install_absent_needs_attention() {
    let outcome = run_case(OpKind::Install, ReconcileBehavior::Present(false)).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention("command succeeded but the package is not installed".to_string())
    );
}

#[tokio::test]
async fn test_succeeded_install_reconcile_err_is_unconfirmed() {
    let outcome = run_case(OpKind::Install, ReconcileBehavior::Err).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_succeeded_uninstall_absent_is_succeeded() {
    let outcome = run_case(OpKind::Uninstall, ReconcileBehavior::Present(false)).await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_succeeded_uninstall_present_needs_attention() {
    let outcome = run_case(OpKind::Uninstall, ReconcileBehavior::Present(true)).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention("command succeeded but the package is still installed".to_string())
    );
}

#[tokio::test]
async fn test_succeeded_uninstall_reconcile_err_is_unconfirmed() {
    let outcome = run_case(OpKind::Uninstall, ReconcileBehavior::Err).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_succeeded_upgrade_present_is_succeeded() {
    let outcome = run_case(OpKind::Upgrade, ReconcileBehavior::Present(true)).await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_succeeded_upgrade_absent_needs_attention() {
    let outcome = run_case(OpKind::Upgrade, ReconcileBehavior::Present(false)).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention("package disappeared after upgrade".to_string())
    );
}

#[tokio::test]
async fn test_succeeded_upgrade_reconcile_err_is_unconfirmed() {
    let outcome = run_case(OpKind::Upgrade, ReconcileBehavior::Err).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}
