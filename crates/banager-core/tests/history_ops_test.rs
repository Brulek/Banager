//! The history's way in from `OperationManager`: what `submit_with`'s
//! callback is handed as an operation finishes, and what the history then
//! keeps -- an update's two readings of the version, nothing for an
//! operation cancelled before it reached its adapter, and `Cancelled` for
//! one its adapter gave up on before starting the tool's command.

use async_trait::async_trait;
use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use banager_core::events::{EventSink, LogNote, OpId, OperationEvent, VecSink};
use banager_core::follow_up::FollowUpWarning;
use banager_core::history::{HistoryKind, HistoryResult, HistoryStore, Started};
use banager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    OpStatus, Outcome, Plan, PlanAction, Reconciled, ResourceLock, SearchHit,
};
use banager_core::ops::{OnFinish, OperationManager};
use banager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Reads `1.0` before an update and `2.0` after it; its `execute` answers
/// `outcome` without running anything.
struct FakeAdapter {
    meta: AdapterMeta,
    reconciles: Mutex<usize>,
    outcome: Outcome,
    notes: Mutex<Vec<LogNote>>,
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
                args: vec!["--secret-flag".to_string()],
                env: vec![],
            },
            needs_password: false,
            locks: vec![ResourceLock(inst.id.clone())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: vec![],
            affected: vec![],
            basis: None,
            timeout_secs: 60,
        })
    }
    async fn execute(
        &self,
        _plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        for note in self.notes.lock().unwrap().iter() {
            sink.emit(OperationEvent::Note {
                op_id,
                note: note.clone(),
            });
        }
        Ok(self.outcome.clone())
    }
    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let mut n = self.reconciles.lock().unwrap();
        *n += 1;
        Ok(Reconciled {
            present: true,
            version: Some(if *n == 1 { "1.0" } else { "2.0" }.to_string()),
        })
    }
}

struct Fixture {
    manager: Arc<OperationManager>,
    adapter: Arc<FakeAdapter>,
    instance: ManagerInstance,
    store: Arc<HistoryStore>,
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture(tag: &str) -> Fixture {
    fixture_with(tag, Outcome::Succeeded)
}

fn fixture_with(tag: &str, outcome: Outcome) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "banager-history-ops-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter {
        meta: AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec![],
        },
        reconciles: Mutex::new(0),
        outcome,
        notes: Mutex::new(Vec::new()),
    });
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let instance = banager_core::testing::manager_instance("fake", "fake:/history");
    manager.register_instance(instance.clone());
    let store = HistoryStore::open(dir.join("history.json"));
    Fixture {
        manager,
        adapter,
        instance,
        store,
        dir,
    }
}

fn on_finish(store: &Arc<HistoryStore>) -> OnFinish {
    let store = store.clone();
    let started = Started {
        display_name: "cmake".to_string(),
        adapter_id: "fake".to_string(),
        listed_version: Some("1.0".to_string()),
    };
    Box::new(move |ended| store.record(ended, &started))
}

fn upgrade(instance: &ManagerInstance) -> OpRequest {
    OpRequest {
        kind: OpKind::Upgrade,
        instance_id: instance.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "cmake".to_string(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_a_finished_update_is_kept_with_both_readings_and_as_verified() {
    let f = fixture("update");
    let plan = f
        .adapter
        .plan(&f.instance, &upgrade(&f.instance))
        .await
        .unwrap();
    let op_id = f.manager.submit_with(plan, Some(on_finish(&f.store)));
    assert_eq!(f.manager.wait(op_id).await, Some(Outcome::Succeeded));

    // In memory by the time the operation is Done, before any write.
    let view = f.store.view();
    let [record] = view.records.as_slice() else {
        panic!("one record: {:?}", view.records);
    };
    assert_eq!(record.op_id, op_id);
    assert_eq!(record.kind, HistoryKind::Update);
    assert_eq!(record.result, HistoryResult::Succeeded);
    assert!(record.verified);
    assert_eq!(record.from_version.as_deref(), Some("1.0"));
    assert_eq!(record.to_version.as_deref(), Some("2.0"));

    assert!(f.store.flush(Duration::from_secs(5)));
    let written = std::fs::read_to_string(f.dir.join("history.json")).unwrap();
    assert!(written.contains("\"cmake\""), "{written}");
    assert!(
        !written.contains("--secret-flag") && !written.contains("/bin/true"),
        "no command line in the file: {written}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_an_update_cancelled_while_it_waited_for_its_lock_is_not_kept() {
    let f = fixture("cancel");
    // Another operation holds the source: this one waits, and is cancelled
    // before its command starts.
    let held = f
        .manager
        .acquire_resource_lock(ResourceLock(f.instance.id.clone()))
        .await;
    let plan = f
        .adapter
        .plan(&f.instance, &upgrade(&f.instance))
        .await
        .unwrap();
    let op_id = f.manager.submit_with(plan, Some(on_finish(&f.store)));
    f.manager
        .cancel(op_id)
        .expect("a queued op can be cancelled");
    assert_eq!(f.manager.wait(op_id).await, Some(Outcome::Cancelled));
    drop(held);

    assert_eq!(f.store.view().records, vec![]);
    assert!(f.store.flush(Duration::from_secs(1)));
    assert!(
        !f.dir.join("history.json").exists(),
        "nothing to write, so no file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_a_plain_submit_has_no_callback_and_still_finishes() {
    let f = fixture("plain");
    let plan = f
        .adapter
        .plan(&f.instance, &upgrade(&f.instance))
        .await
        .unwrap();
    let op_id = f.manager.submit(plan);
    assert_eq!(f.manager.wait(op_id).await, Some(Outcome::Succeeded));
    assert_eq!(f.store.view().records, vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_an_update_its_adapter_gave_up_before_starting_the_command_is_kept_as_cancelled() {
    // Brew's `execute` answers `Cancelled` when a Cancel lands while it
    // waits for a `brew update` to end: no command of the tool's ran. The
    // operation had reached its adapter, so it is kept, as `Cancelled` --
    // which is all `HistoryResult::Cancelled` and docs/what-we-run.md
    // claim: handed to the adapter, not that the command started.
    let f = fixture_with("adapter-cancel", Outcome::Cancelled);
    let plan = f
        .adapter
        .plan(&f.instance, &upgrade(&f.instance))
        .await
        .unwrap();
    let op_id = f.manager.submit_with(plan, Some(on_finish(&f.store)));
    assert_eq!(f.manager.wait(op_id).await, Some(Outcome::Cancelled));

    let view = f.store.view();
    let [record] = view.records.as_slice() else {
        panic!("one record: {:?}", view.records);
    };
    assert_eq!(record.result, HistoryResult::Cancelled);
    assert!(!record.verified);
    assert_eq!(
        record.to_version, None,
        "no new version for a stopped update"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_an_operation_is_not_done_until_its_record_is_kept() {
    // Quit asks whether any operation is unfinished, then flushes what the
    // history owes. An operation that already said Done before its record
    // was in the history could let Banager exit with the record lost.
    let f = fixture("before-done");
    let plan = f
        .adapter
        .plan(&f.instance, &upgrade(&f.instance))
        .await
        .unwrap();
    let seen: Arc<Mutex<Option<(OpStatus, bool)>>> = Arc::new(Mutex::new(None));
    let record = on_finish(&f.store);
    let callback: OnFinish = {
        let manager = f.manager.clone();
        let seen = seen.clone();
        Box::new(move |ended| {
            let status = manager
                .summaries()
                .into_iter()
                .find(|op| op.id == ended.op_id)
                .map(|op| (op.status, op.outcome.is_some()));
            *seen.lock().unwrap() = status;
            record(ended);
        })
    };
    let op_id = f.manager.submit_with(plan, Some(callback));
    assert_eq!(f.manager.wait(op_id).await, Some(Outcome::Succeeded));
    let (status, has_outcome) = seen.lock().unwrap().expect("the callback ran");
    assert_ne!(
        status,
        OpStatus::Done,
        "the operation said Done before its record was kept"
    );
    assert!(!has_outcome, "an outcome is shown only with Done");
    // And once it is Done, the record is there.
    assert_eq!(f.store.view().records.len(), 1);
}

#[tokio::test]
async fn test_a_cancel_that_arrives_while_the_record_is_kept_changes_nothing() {
    // The outcome is decided before the record is kept; a Cancel pressed in
    // that instant is too late, and must neither be accepted nor say
    // CancelRequested after the operation has ended.
    // An operation that ends while still `Running` -- its source is not
    // one the manager knows, so it ends `BanagerFailed` before its command
    // -- is the one a cancel could still be accepted for.
    let f = fixture("cancel-while-kept");
    let gone = banager_core::testing::manager_instance("fake", "fake:/gone");
    let plan = f.adapter.plan(&gone, &upgrade(&gone)).await.unwrap();
    let answer: Arc<Mutex<Option<(OpStatus, bool)>>> = Arc::new(Mutex::new(None));
    let record = on_finish(&f.store);
    let callback: OnFinish = {
        let manager = f.manager.clone();
        let answer = answer.clone();
        Box::new(move |ended| {
            let status = manager
                .summaries()
                .into_iter()
                .find(|op| op.id == ended.op_id)
                .map(|op| op.status)
                .unwrap();
            let accepted = manager.cancel(ended.op_id).is_ok();
            *answer.lock().unwrap() = Some((status, accepted));
            record(ended);
        })
    };
    let op_id = f.manager.submit_with(plan, Some(callback));
    assert!(matches!(
        f.manager.wait(op_id).await,
        Some(Outcome::BanagerFailed(_))
    ));
    let (status, accepted) = answer.lock().unwrap().expect("the callback ran");
    assert!(
        !accepted,
        "a cancel was accepted after the outcome was decided (status {status:?})"
    );
    let summary = f
        .manager
        .summaries()
        .into_iter()
        .find(|op| op.id == op_id)
        .unwrap();
    assert_eq!(summary.status, OpStatus::Done);
}

#[tokio::test]
async fn test_follow_up_warnings_cross_operation_wire_and_history_without_changing_success() {
    let f = fixture("follow-up");
    *f.adapter.notes.lock().unwrap() = vec![
        LogNote::OldVersionsNotCleanedUp {
            name: "cmake".into(),
            exit_code: Some(1),
        },
        LogNote::NoLongerLinked {
            name: "cmake".into(),
            commands: vec!["cmake".into()],
        },
    ];
    let plan = f
        .adapter
        .plan(&f.instance, &upgrade(&f.instance))
        .await
        .unwrap();
    let id = f.manager.submit_with(plan, Some(on_finish(&f.store)));
    assert_eq!(f.manager.wait(id).await, Some(Outcome::Succeeded));
    let summary = f
        .manager
        .summaries()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap();
    let expected = vec![
        FollowUpWarning::OldVersionsNotCleanedUp {
            name: "cmake".into(),
            exit_code: Some(1),
        },
        FollowUpWarning::NoLongerLinked {
            name: "cmake".into(),
            commands: vec!["cmake".into()],
        },
    ];
    assert_eq!(summary.follow_up_warnings, expected);
    let wire = serde_json::to_value(&summary).unwrap();
    assert_eq!(wire["outcome"], "Succeeded");
    assert_eq!(
        wire["follow_up_warnings"][1]["NoLongerLinked"]["commands"][0],
        "cmake"
    );
    let back: banager_core::ops::OpSummary = serde_json::from_value(wire).unwrap();
    assert_eq!(back.follow_up_warnings, expected);
    assert!(f.store.flush(Duration::from_secs(5)));
    let reopened = HistoryStore::open(f.dir.join("history.json"));
    let record = &reopened.view().records[0];
    assert_eq!(record.result, HistoryResult::Succeeded);
    assert!(record.verified);
    assert_eq!(record.follow_up_warnings, expected);
}
