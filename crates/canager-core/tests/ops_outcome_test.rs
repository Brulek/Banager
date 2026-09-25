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
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use canager_core::events::{EventSink, OpId, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, Attention, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
    OpRequest, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// What `FakeAdapter::reconcile` should report for a given scenario.
#[derive(Clone)]
enum ReconcileBehavior {
    Present(bool),
    Err,
    /// One reading per call, in order; the last one repeats. An upgrade
    /// reconciles twice -- before `execute` and after -- so a two-entry
    /// script is "what was installed before, what is installed after".
    Readings(Vec<Option<Reconciled>>),
}

struct FakeAdapter {
    meta: AdapterMeta,
    reconcile_behavior: ReconcileBehavior,
    /// Every `reconcile` and `execute` call, in the order they happened.
    calls: Mutex<Vec<&'static str>>,
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
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
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
        self.calls.lock().unwrap().push("execute");
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
        let nth = {
            let mut calls = self.calls.lock().unwrap();
            let nth = calls.iter().filter(|c| **c == "reconcile").count();
            calls.push("reconcile");
            nth
        };
        match &self.reconcile_behavior {
            ReconcileBehavior::Present(present) => Ok(Reconciled {
                present: *present,
                version: None,
            }),
            ReconcileBehavior::Err => Err(AdapterError::Refused("reconcile failed".to_string())),
            ReconcileBehavior::Readings(readings) => readings[nth.min(readings.len() - 1)]
                .clone()
                .ok_or_else(|| AdapterError::Refused("reconcile failed".to_string())),
        }
    }
}

fn make_instance(id: &str) -> ManagerInstance {
    ManagerInstance {
        version: None,
        ..canager_core::testing::manager_instance("fake", id)
    }
}

/// Submits one op of `kind` against a fresh manager/adapter/instance whose
/// `execute` reports `Succeeded` and whose `reconcile` behaves as given,
/// then returns the final outcome.
async fn run_case(kind: OpKind, reconcile_behavior: ReconcileBehavior) -> Outcome {
    run_case_with_calls(kind, reconcile_behavior).await.0
}

/// `run_case`, also returning the order `reconcile` and `execute` ran in.
async fn run_case_with_calls(
    kind: OpKind,
    reconcile_behavior: ReconcileBehavior,
) -> (Outcome, Vec<&'static str>) {
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
    let outcome = manager
        .wait(op_id)
        .await
        .expect("op must reach Done and report an outcome");
    (outcome, adapter.calls())
}

fn at(version: &str) -> Option<Reconciled> {
    Some(Reconciled {
        present: true,
        version: Some(version.to_string()),
    })
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
        Outcome::NeedsAttention(Attention::NotInstalledAfterInstall)
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
        Outcome::NeedsAttention(Attention::StillInstalledAfterUninstall)
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
        Outcome::NeedsAttention(Attention::GoneAfterUpgrade)
    );
}

#[tokio::test]
async fn test_succeeded_upgrade_reconcile_err_is_unconfirmed() {
    let outcome = run_case(OpKind::Upgrade, ReconcileBehavior::Err).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

// An upgrade whose tool exits 0 used to be `Succeeded` whenever the package
// was still there afterwards -- which it always is, since it was there
// before. These compare the version read before the command with the one
// read after.

#[tokio::test]
async fn test_succeeded_upgrade_whose_version_moved_is_succeeded() {
    let (outcome, calls) = run_case_with_calls(
        OpKind::Upgrade,
        ReconcileBehavior::Readings(vec![at("1.7.1"), at("1.8.0")]),
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
    // The first reading has to be taken before the command runs, or it is
    // not a "before".
    assert_eq!(calls, vec!["reconcile", "execute", "reconcile"]);
}

#[tokio::test]
async fn test_succeeded_upgrade_whose_version_did_not_move_needs_attention() {
    // The tool reported success and nothing changed: a locked pipx tool, a
    // uv tool installed with `==`, a disabled Homebrew cask. It must not be
    // reported as `Succeeded`.
    let outcome = run_case(
        OpKind::Upgrade,
        ReconcileBehavior::Readings(vec![at("1.7.1"), at("1.7.1")]),
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_succeeded_upgrade_with_no_before_reading_falls_back_to_presence() {
    // The before-reading failed: there is nothing to compare, so the
    // outcome is exactly what it was before that reading existed --
    // never a stronger claim than the evidence.
    let outcome = run_case(
        OpKind::Upgrade,
        ReconcileBehavior::Readings(vec![None, at("1.7.1")]),
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_succeeded_upgrade_whose_versions_are_unknown_or_empty_falls_back_to_presence() {
    // `None` is how an adapter says its version string cannot tell one
    // install from another (a Homebrew `version :latest` cask); `""` is
    // what brew's and npm's parsers fall back to when there is no version
    // to read. Two equal non-versions prove nothing.
    for version in [None, Some(String::new())] {
        let reading = Some(Reconciled {
            present: true,
            version: version.clone(),
        });
        let outcome = run_case(
            OpKind::Upgrade,
            ReconcileBehavior::Readings(vec![reading.clone(), reading]),
        )
        .await;
        assert_eq!(outcome, Outcome::Succeeded, "version {version:?}");
    }
}

#[tokio::test]
async fn test_succeeded_upgrade_that_removed_the_package_is_still_gone_after_upgrade() {
    let outcome = run_case(
        OpKind::Upgrade,
        ReconcileBehavior::Readings(vec![
            at("1.7.1"),
            Some(Reconciled {
                present: false,
                version: None,
            }),
        ]),
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::GoneAfterUpgrade)
    );
}

#[tokio::test]
async fn test_install_and_uninstall_take_no_before_reading() {
    // The before-reading is for upgrades only: presence already decides
    // an install or an uninstall, so they cost no extra inventory read.
    for kind in [OpKind::Install, OpKind::Uninstall] {
        let (_, calls) = run_case_with_calls(kind, ReconcileBehavior::Present(true)).await;
        assert_eq!(calls, vec!["execute", "reconcile"], "{kind:?}");
    }
}
