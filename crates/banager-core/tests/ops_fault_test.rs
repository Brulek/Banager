//! Contract tests for how an `Err` out of `Adapter::execute` reaches the
//! front end: always as `Outcome::BanagerFailed` (a reason of Banager's
//! own, worded by the front end in the user's language), never as
//! `Outcome::Failed`, whose `summary` is only ever another program's own
//! words (a tool's stderr, or macOS's reason for refusing a move to the
//! Trash).
//!
//! Before this split, `run_operation` put English sentences of its own
//! ("unknown instance ...", "runner: program not found: ...") into
//! `Failed`'s `summary` -- the same string that carries a tool's stderr --
//! so a Chinese user read Banager's English inside a translated frame, and
//! the front end had no way to tell the two apart.

use async_trait::async_trait;
use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use banager_core::events::{EventSink, OpId, VecSink};
use banager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, Fault, InstalledArtifact, ManagerInstance, OpKind,
    OpRequest, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
};
use banager_core::ops::OperationManager;
use banager_core::runner::{HostEnv, RunnerError};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

type MakeError = Box<dyn Fn() -> AdapterError + Send + Sync>;

/// An adapter whose `execute` always fails with the error `make_error`
/// builds (a fresh one per call: `std::io::Error` is not `Clone`).
struct FailingAdapter {
    meta: AdapterMeta,
    make_error: MakeError,
}

impl FailingAdapter {
    fn new(make_error: MakeError) -> FailingAdapter {
        FailingAdapter {
            meta: AdapterMeta {
                id: "fake".to_string(),
                name: "fake".to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            make_error,
        }
    }
}

#[async_trait]
impl Adapter for FailingAdapter {
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
        Err((self.make_error)())
    }

    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        Ok(Reconciled {
            present: false,
            version: None,
        })
    }
}

fn instance(id: &str, adapter_id: &str) -> ManagerInstance {
    ManagerInstance {
        version: None,
        ..banager_core::testing::manager_instance(adapter_id, id)
    }
}

fn request(instance_id: &str) -> OpRequest {
    OpRequest {
        kind: OpKind::Install,
        instance_id: instance_id.to_string(),
        artifact_kind: ArtifactKind::Formula,
        name: "pkg".to_string(),
    }
}

/// Registers `adapter` and `inst`, submits one install planned by `adapter`
/// for the instance id `planned_for`, and returns the outcome.
async fn run(adapter: Arc<FailingAdapter>, inst: ManagerInstance, planned_for: &str) -> Outcome {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    let plan = adapter
        .plan(&inst, &request(planned_for))
        .await
        .expect("plan");
    let op_id = manager.submit(plan);
    manager.wait(op_id).await.expect("op must reach Done")
}

async fn run_execute_error(make_error: MakeError) -> Outcome {
    let inst = instance("fake:/faults", "fake");
    let id = inst.id.clone();
    run(Arc::new(FailingAdapter::new(make_error)), inst, &id).await
}

#[tokio::test]
async fn test_a_missing_program_is_a_fault_carrying_its_path_not_an_english_summary() {
    let outcome = run_execute_error(Box::new(|| {
        AdapterError::Runner(RunnerError::NotFound(PathBuf::from(
            "/opt/homebrew/bin/brew",
        )))
    }))
    .await;
    assert_eq!(
        outcome,
        Outcome::BanagerFailed(Fault::ProgramMissing {
            program: "/opt/homebrew/bin/brew".to_string()
        })
    );
}

#[tokio::test]
async fn test_a_program_macos_would_not_start_carries_only_the_systems_reason() {
    let outcome = run_execute_error(Box::new(|| {
        AdapterError::Runner(RunnerError::Spawn(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )))
    }))
    .await;
    match outcome {
        Outcome::BanagerFailed(Fault::SpawnFailed { detail }) => {
            // The operating system's words, with none of Banager's own
            // ("runner: spawn failed: ") in front of them.
            assert!(!detail.contains("runner"), "{detail}");
            assert!(!detail.contains("spawn failed"), "{detail}");
            assert!(!detail.is_empty());
        }
        other => panic!("expected BanagerFailed(SpawnFailed), got {other:?}"),
    }
}

#[tokio::test]
async fn test_every_error_but_a_missing_or_unstartable_program_is_banagers_own_bug() {
    // A tool that ran and failed never gets here: `run_plan` makes that an
    // `Ok(Outcome::Failed)`. What does arrive as an `Err` is Banager's own,
    // and none of its English reaches the wire.
    let errors: Vec<MakeError> = vec![
        Box::new(|| AdapterError::Refused("refusing to run Homebrew as root".to_string())),
        Box::new(|| AdapterError::Unsupported("pip is read-only in Banager".to_string())),
        Box::new(|| AdapterError::CommandFailed {
            code: Some(1),
            stderr: "Error: No such keg\n".to_string(),
        }),
        Box::new(|| AdapterError::Parse("unexpected token".to_string())),
    ];
    for make_error in errors {
        assert_eq!(
            run_execute_error(make_error).await,
            Outcome::BanagerFailed(Fault::Internal)
        );
    }
}

#[tokio::test]
async fn test_a_plan_for_an_unregistered_instance_is_an_internal_fault() {
    // Cannot happen through `Session::submit` (every instance in the
    // snapshot is registered first, and a source that has gone since the
    // preview is refused there as `SubmitError::SourceGone`), so reaching
    // it at all is a bug in Banager -- what used to be "unknown instance
    // fake:/gone".
    let inst = instance("fake:/present", "fake");
    let adapter = Arc::new(FailingAdapter::new(Box::new(|| {
        AdapterError::Refused("execute must not be reached".to_string())
    })));
    let outcome = run(adapter, inst, "fake:/gone").await;
    assert_eq!(outcome, Outcome::BanagerFailed(Fault::Internal));
}

#[tokio::test]
async fn test_an_instance_with_no_registered_adapter_is_an_internal_fault() {
    // What used to be "no adapter registered for other".
    let inst = instance("other:/x", "other");
    let id = inst.id.clone();
    let adapter = Arc::new(FailingAdapter::new(Box::new(|| {
        AdapterError::Refused("execute must not be reached".to_string())
    })));
    let outcome = run(adapter, inst, &id).await;
    assert_eq!(outcome, Outcome::BanagerFailed(Fault::Internal));
}
