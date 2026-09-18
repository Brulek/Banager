//! Contract tests for `OperationManager::cancel`.
//!
//! These are deliberately self-contained: the `FakeAdapter` here is not
//! shared with `tests/ops_lock_test.rs` even though its shape rhymes with
//! that file's `FakeAdapter` (same `Adapter` boilerplate is unavoidable).
//!
//! Timing: the fake adapter's "real work" path sleeps 200 ms; `wait()` and
//! our own polling helper check every 20 ms, so there is a wide margin
//! between "op is definitely still running/queued" and "op is definitely
//! done" — the tests should be deterministic on any reasonably-scheduled
//! CI runner.

use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};
use canager_core::events::{EventSink, OpId, OperationEvent, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    OpStatus, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Controls what `FakeAdapter::execute` does, so each scenario below can
/// pick the shape of "work" it needs without a dozen near-duplicate
/// adapters.
#[derive(Clone)]
enum ExecuteBehavior {
    /// Blocks until the op's cancellation token fires, then reports the
    /// execution as unconfirmed — models a `brew` invocation that was
    /// killed mid-flight (cancelled or timed out), whose real effect is
    /// only known after reconciliation.
    WaitForCancel,
    /// Simulates real work that ignores cancellation entirely and just
    /// takes some time, then reports success. Used for scenarios where the
    /// op must actually run to completion (or must still be queued while a
    /// sibling op runs).
    Work(Duration),
}

struct FakeAdapter {
    meta: AdapterMeta,
    behavior: ExecuteBehavior,
    reconcile_result: Reconciled,
    execute_calls: Arc<AtomicUsize>,
}

impl FakeAdapter {
    fn new(behavior: ExecuteBehavior, reconcile_result: Reconciled) -> FakeAdapter {
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
            behavior,
            reconcile_result,
            execute_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Number of times `execute` has actually been invoked — used to prove a
    /// cancelled-before-running op never starts the underlying command.
    fn execute_calls(&self) -> usize {
        self.execute_calls.load(Ordering::SeqCst)
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
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        match &self.behavior {
            ExecuteBehavior::WaitForCancel => {
                cancel.cancelled().await;
                Ok(Outcome::Unconfirmed)
            }
            ExecuteBehavior::Work(duration) => {
                tokio::time::sleep(*duration).await;
                Ok(Outcome::Succeeded)
            }
        }
    }

    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        Ok(self.reconcile_result.clone())
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

fn make_request(kind: OpKind, instance_id: &str, name: &str) -> OpRequest {
    OpRequest {
        kind,
        instance_id: instance_id.to_string(),
        artifact_kind: ArtifactKind::Formula,
        name: name.to_string(),
    }
}

/// Polls `manager.record(op_id)` (not the event sink) every 20 ms until its
/// status equals `target`, or panics after `timeout`.
async fn wait_for_status(
    manager: &Arc<OperationManager>,
    op_id: OpId,
    target: OpStatus,
    timeout: Duration,
) {
    let start = Instant::now();
    loop {
        if let Some(record) = manager.record(op_id) {
            if record.status == target {
                return;
            }
        }
        if start.elapsed() > timeout {
            panic!(
                "timed out after {:?} waiting for op {} to reach status {:?}",
                timeout, op_id, target
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Flattens the events emitted for a single op into a status trace. A
/// `Finished` event counts as reaching `Done`, since the manager only ever
/// marks a record `Done` at the same time it emits `Finished` (there is no
/// separate `Status { status: Done }` event) — this is what "the op is
/// Done" looks like from the event stream.
fn status_trace(events: &[OperationEvent], op_id: OpId) -> Vec<OpStatus> {
    events
        .iter()
        .filter_map(|e| match e {
            OperationEvent::Status { op_id: id, status } if *id == op_id => Some(*status),
            OperationEvent::Finished { op_id: id, .. } if *id == op_id => Some(OpStatus::Done),
            _ => None,
        })
        .collect()
}

/// Asserts `expected` appears as an in-order subsequence of `actual` (not
/// necessarily contiguous, and `actual` may contain other entries too).
fn assert_contains_in_order(actual: &[OpStatus], expected: &[OpStatus]) {
    let mut idx = 0;
    for status in actual {
        if idx < expected.len() && *status == expected[idx] {
            idx += 1;
        }
    }
    assert_eq!(
        idx,
        expected.len(),
        "expected {:?} to appear in order within {:?}",
        expected,
        actual
    );
}

/// Runs a single op of `kind` whose execute blocks until cancelled, cancels
/// it once it is confirmed `Running`, and returns the final outcome plus
/// the full status trace.
async fn run_cancelled_mid_execute(
    kind: OpKind,
    reconciled: Reconciled,
) -> (Outcome, Vec<OpStatus>) {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink.clone());
    let adapter = Arc::new(FakeAdapter::new(ExecuteBehavior::WaitForCancel, reconciled));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/mid-execute");
    manager.register_instance(inst.clone());
    let req = make_request(kind, &inst.id, "pkg");
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);

    wait_for_status(
        &manager,
        op_id,
        OpStatus::Running,
        Duration::from_millis(1000),
    )
    .await;
    manager.cancel(op_id);

    let outcome = manager
        .wait(op_id)
        .await
        .expect("op must reach Done and report an outcome");
    let trace = status_trace(&sink.snapshot(), op_id);
    (outcome, trace)
}

// (a) Cancel mid-execute: the full status machine runs end to end and
// reaches Done, in the right order.
#[tokio::test]
async fn test_cancel_mid_execute_emits_full_status_sequence() {
    let (outcome, trace) = run_cancelled_mid_execute(
        OpKind::Install,
        Reconciled {
            present: false,
            version: None,
        },
    )
    .await;

    assert_contains_in_order(
        &trace,
        &[
            OpStatus::Queued,
            OpStatus::Running,
            OpStatus::CancelRequested,
            OpStatus::Cancelling,
            OpStatus::Verifying,
            OpStatus::Done,
        ],
    );
    // Install + present:false after a cancelled execute must not be
    // reported as a silent success.
    assert_eq!(outcome, Outcome::Unconfirmed);
}

// (b) Outcome mapping per OpKind after a cancelled execute.
#[tokio::test]
async fn test_cancelled_install_reports_succeeded_when_present() {
    let (outcome, _trace) = run_cancelled_mid_execute(
        OpKind::Install,
        Reconciled {
            present: true,
            version: None,
        },
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_cancelled_uninstall_reports_succeeded_when_absent() {
    let (outcome, _trace) = run_cancelled_mid_execute(
        OpKind::Uninstall,
        Reconciled {
            present: false,
            version: None,
        },
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_cancelled_upgrade_never_reports_succeeded() {
    // Upgrade means the artifact was already present before the op ran, so
    // presence after a cancelled/timed-out execute proves nothing: it must
    // stay Unconfirmed whether the reconcile finds it present or absent.
    let (present_outcome, _) = run_cancelled_mid_execute(
        OpKind::Upgrade,
        Reconciled {
            present: true,
            version: None,
        },
    )
    .await;
    assert_eq!(present_outcome, Outcome::Unconfirmed);

    let (absent_outcome, _) = run_cancelled_mid_execute(
        OpKind::Upgrade,
        Reconciled {
            present: false,
            version: None,
        },
    )
    .await;
    assert_eq!(absent_outcome, Outcome::Unconfirmed);
}

// (c) Cancel while still Queued (waiting for a lock held by a sibling op)
// must report NoChange and never let the op run.
#[tokio::test]
async fn test_cancel_while_queued_reports_no_change_and_never_runs() {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink.clone());
    let adapter = Arc::new(FakeAdapter::new(
        ExecuteBehavior::Work(Duration::from_millis(200)),
        Reconciled {
            present: true,
            version: None,
        },
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/queued");
    manager.register_instance(inst.clone());

    let plan_a = adapter
        .plan(&inst, &make_request(OpKind::Install, &inst.id, "a"))
        .await
        .expect("plan a");
    let plan_b = adapter
        .plan(&inst, &make_request(OpKind::Install, &inst.id, "b"))
        .await
        .expect("plan b");

    // Both plans share inst's ResourceLock, so op_b must sit Queued for the
    // whole 200 ms that op_a is Running.
    let id_a = manager.submit(plan_a);
    let id_b = manager.submit(plan_b);

    wait_for_status(
        &manager,
        id_a,
        OpStatus::Running,
        Duration::from_millis(1000),
    )
    .await;
    manager.cancel(id_b);

    let outcome_b = manager.wait(id_b).await;
    assert_eq!(outcome_b, Some(Outcome::NoChange));

    let trace_b = status_trace(&sink.snapshot(), id_b);
    assert!(
        !trace_b.contains(&OpStatus::Running),
        "an op cancelled while queued must never run: {:?}",
        trace_b
    );

    // The sibling op that actually held the lock is unaffected.
    let outcome_a = manager.wait(id_a).await;
    assert_eq!(outcome_a, Some(Outcome::Succeeded));
}

// (d) Cancel after Done is a no-op: no new events, status stays Done, wait()
// still returns the original outcome.
#[tokio::test]
async fn test_cancel_after_done_is_a_no_op() {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink.clone());
    let adapter = Arc::new(FakeAdapter::new(
        ExecuteBehavior::Work(Duration::from_millis(200)),
        Reconciled {
            present: true,
            version: None,
        },
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/done");
    manager.register_instance(inst.clone());
    let plan = adapter
        .plan(&inst, &make_request(OpKind::Install, &inst.id, "pkg"))
        .await
        .expect("plan");
    let op_id = manager.submit(plan);

    let outcome = manager.wait(op_id).await;
    assert_eq!(outcome, Some(Outcome::Succeeded));

    let events_before = sink.snapshot();
    let cancel_requested_before = events_before
        .iter()
        .filter(|e| {
            matches!(
                e,
                OperationEvent::Status {
                    op_id: id,
                    status: OpStatus::CancelRequested
                } if *id == op_id
            )
        })
        .count();

    manager.cancel(op_id);
    // Give any (incorrect) async side effect a moment to land before we
    // assert its absence.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let events_after = sink.snapshot();
    assert_eq!(
        events_after.len(),
        events_before.len(),
        "cancel() on a Done op must not emit any new events: before={:?} after={:?}",
        events_before,
        events_after
    );
    let cancel_requested_after = events_after
        .iter()
        .filter(|e| {
            matches!(
                e,
                OperationEvent::Status {
                    op_id: id,
                    status: OpStatus::CancelRequested
                } if *id == op_id
            )
        })
        .count();
    assert_eq!(
        cancel_requested_after, cancel_requested_before,
        "cancel() on a Done op must not emit a new CancelRequested status"
    );

    let record = manager.record(op_id).expect("record still present");
    assert_eq!(record.status, OpStatus::Done);
    assert_eq!(manager.wait(op_id).await, Some(Outcome::Succeeded));
}

// (F1) Cancelling immediately after submit, with the resource lock free, must
// not let the command start at all: cancellation is checked again right
// after the lock is acquired and before the op is marked Running, not only
// while it is still waiting for a lock held by someone else.
#[tokio::test]
async fn test_cancel_immediately_after_submit_never_calls_execute() {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let adapter = Arc::new(FakeAdapter::new(
        ExecuteBehavior::Work(Duration::from_millis(200)),
        Reconciled {
            present: true,
            version: None,
        },
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/immediate-cancel");
    manager.register_instance(inst.clone());
    let plan = adapter
        .plan(&inst, &make_request(OpKind::Install, &inst.id, "pkg"))
        .await
        .expect("plan");

    // Nothing else holds this resource's lock, so `submit` will find it free
    // on the very first poll.
    let op_id = manager.submit(plan);
    manager.cancel(op_id);

    let outcome = manager.wait(op_id).await;
    assert_eq!(outcome, Some(Outcome::NoChange));
    assert_eq!(
        adapter.execute_calls(),
        0,
        "a cancelled-before-running op must never invoke execute"
    );
}
