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
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use canager_core::events::{EventSink, OpId, OperationEvent, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    OpStatus, Outcome, Plan, Reconciled, ResourceLock, SearchHit,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
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
    /// Reports the execution as unconfirmed straight away, without anyone
    /// having cancelled -- models a run the runner stopped at its timeout.
    /// Only the user's Cancel may ever be reported as `Cancelled`.
    TimedOut,
    /// Simulates real work that ignores cancellation entirely and just
    /// takes some time, then reports success. Used for scenarios where the
    /// op must actually run to completion (or must still be queued while a
    /// sibling op runs).
    Work(Duration),
}

struct FakeAdapter {
    meta: AdapterMeta,
    behavior: ExecuteBehavior,
    /// One reading per `reconcile` call, in order; the last one repeats.
    /// An upgrade reconciles before `execute` and again after.
    readings: Vec<Reconciled>,
    reconcile_calls: AtomicUsize,
    /// How long the first `reconcile` call takes, so a test can cancel
    /// while an upgrade is still taking its before-reading.
    first_reconcile_delay: Duration,
    execute_calls: Arc<AtomicUsize>,
}

impl FakeAdapter {
    fn new(behavior: ExecuteBehavior, reconcile_result: Reconciled) -> FakeAdapter {
        FakeAdapter::with_readings(behavior, vec![reconcile_result])
    }

    fn with_readings(behavior: ExecuteBehavior, readings: Vec<Reconciled>) -> FakeAdapter {
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
            readings,
            reconcile_calls: AtomicUsize::new(0),
            first_reconcile_delay: Duration::ZERO,
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
            ExecuteBehavior::TimedOut => Ok(Outcome::Unconfirmed),
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
        let nth = self.reconcile_calls.fetch_add(1, Ordering::SeqCst);
        if nth == 0 {
            tokio::time::sleep(self.first_reconcile_delay).await;
        }
        Ok(self.readings[nth.min(self.readings.len() - 1)].clone())
    }
}

fn make_instance(id: &str) -> ManagerInstance {
    ManagerInstance {
        version: None,
        ..canager_core::testing::manager_instance("fake", id)
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
    run_cancelled_mid_execute_with_readings(kind, vec![reconciled]).await
}

/// `run_cancelled_mid_execute` with one reading per `reconcile` call (see
/// `FakeAdapter::readings`).
async fn run_cancelled_mid_execute_with_readings(
    kind: OpKind,
    readings: Vec<Reconciled>,
) -> (Outcome, Vec<OpStatus>) {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink.clone());
    let adapter = Arc::new(FakeAdapter::with_readings(
        ExecuteBehavior::WaitForCancel,
        readings,
    ));
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
    // `Running` is set before an upgrade's before-reading; cancel only once
    // `execute` is under way, or this would test a cancel of the reading.
    let start = Instant::now();
    while adapter.execute_calls() == 0 {
        assert!(
            start.elapsed() < Duration::from_millis(1000),
            "execute never started"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    manager.cancel(op_id);

    let outcome = manager
        .wait(op_id)
        .await
        .expect("op must reach Done and report an outcome");
    let trace = status_trace(&sink.snapshot(), op_id);
    (outcome, trace)
}

fn at(version: &str) -> Reconciled {
    Reconciled {
        present: true,
        version: Some(version.to_string()),
    }
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
    // Install + present:false after the user's cancel: nothing was
    // installed, and the user is told it was their cancel -- never a silent
    // success, and never "unconfirmed" when reconcile confirmed it.
    assert_eq!(outcome, Outcome::Cancelled);
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
async fn test_cancelled_uninstall_reports_cancelled_when_still_present() {
    let (outcome, _trace) = run_cancelled_mid_execute(
        OpKind::Uninstall,
        Reconciled {
            present: true,
            version: None,
        },
    )
    .await;
    assert_eq!(outcome, Outcome::Cancelled);
}

#[tokio::test]
async fn test_cancelled_upgrade_without_versions_to_compare_stays_unconfirmed() {
    // Upgrade means the artifact was already present before the op ran, so
    // presence after a cancelled/timed-out execute proves nothing. With no
    // version in either reading there is nothing else to go on: it must
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

// A run that ended unconfirmed because it timed out, not because the user
// cancelled, must never be reported as the user's cancellation -- even when
// reconcile shows the request did not take effect.
#[tokio::test]
async fn test_timed_out_run_is_unconfirmed_not_cancelled() {
    for (kind, present) in [(OpKind::Install, false), (OpKind::Uninstall, true)] {
        let sink = Arc::new(VecSink::new());
        let mut manager = OperationManager::new(sink);
        let adapter = Arc::new(FakeAdapter::new(
            ExecuteBehavior::TimedOut,
            Reconciled {
                present,
                version: None,
            },
        ));
        manager.register_adapter(adapter.clone());
        let manager = Arc::new(manager);

        let inst = make_instance("fake:/timed-out");
        manager.register_instance(inst.clone());
        let plan = adapter
            .plan(&inst, &make_request(kind, &inst.id, "pkg"))
            .await
            .expect("plan");
        let op_id = manager.submit(plan);

        assert_eq!(
            manager.wait(op_id).await,
            Some(Outcome::Unconfirmed),
            "{kind:?} that timed out"
        );
    }
}

// (c) Cancel while still Queued (waiting for a lock held by a sibling op)
// must report Cancelled and never let the op run.
#[tokio::test]
async fn test_cancel_while_queued_reports_cancelled_and_never_runs() {
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
    assert_eq!(outcome_b, Some(Outcome::Cancelled));

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
    assert_eq!(outcome, Some(Outcome::Cancelled));
    assert_eq!(
        adapter.execute_calls(),
        0,
        "a cancelled-before-running op must never invoke execute"
    );
}

// An upgrade whose command was stopped partway -- the user's Cancel or the
// timeout -- is `Unconfirmed`, whatever its version reads before and after.
// The tools write that version partway through an upgrade, so a stopped run
// can leave it moved with the upgrade unfinished, or unmoved with the
// package already changed (the `Ok(Outcome::Unconfirmed)` arm of
// `run_operation` cites the Homebrew and pipx lines). Only the tool's own
// exit 0 lets the two readings decide (tests/ops_outcome_test.rs).

#[tokio::test]
async fn test_cancelled_upgrade_whose_version_moved_is_unconfirmed_not_succeeded() {
    // A Homebrew formula stopped after its new keg is poured and before it
    // is linked reads as the new version; so does a cask stopped after
    // `stage` wrote the new metadata and before its app is installed.
    // Neither upgrade finished.
    let (outcome, _) =
        run_cancelled_mid_execute_with_readings(OpKind::Upgrade, vec![at("1.7.1"), at("1.8.0")])
            .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_cancelled_upgrade_whose_version_did_not_move_is_unconfirmed_not_cancelled() {
    // pipx writes the version it reports after its installer has already
    // changed the venv; a Homebrew cask has moved the old app out of
    // /Applications before it writes the new version. Stopped in between, the version
    // reads as before and the package has changed, so "You cancelled this"
    // is not known to be all that happened.
    let (outcome, _) =
        run_cancelled_mid_execute_with_readings(OpKind::Upgrade, vec![at("1.7.1"), at("1.7.1")])
            .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_timed_out_upgrade_is_unconfirmed_whether_or_not_the_version_moved() {
    // The timeout stops a command the same way a Cancel does, so the same
    // holds: neither reading is evidence of how far the tool got.
    for after in [at("1.8.0"), at("1.7.1")] {
        let sink = Arc::new(VecSink::new());
        let mut manager = OperationManager::new(sink);
        let adapter = Arc::new(FakeAdapter::with_readings(
            ExecuteBehavior::TimedOut,
            vec![at("1.7.1"), after.clone()],
        ));
        manager.register_adapter(adapter.clone());
        let manager = Arc::new(manager);

        let inst = make_instance("fake:/timed-out-upgrade");
        manager.register_instance(inst.clone());
        let plan = adapter
            .plan(&inst, &make_request(OpKind::Upgrade, &inst.id, "pkg"))
            .await
            .expect("plan");
        let op_id = manager.submit(plan);

        assert_eq!(
            manager.wait(op_id).await,
            Some(Outcome::Unconfirmed),
            "after {after:?}"
        );
    }
}

#[tokio::test]
async fn test_cancel_during_an_upgrades_before_reading_never_calls_execute() {
    // The before-reading is an inventory read (`brew info --installed`
    // took 0.75 s on the Mac this was written on). A Cancel pressed during
    // it must stop the upgrade there, before its command starts.
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let mut fake = FakeAdapter::with_readings(ExecuteBehavior::WaitForCancel, vec![at("1.7.1")]);
    fake.first_reconcile_delay = Duration::from_millis(500);
    let adapter = Arc::new(fake);
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/cancel-before-reading");
    manager.register_instance(inst.clone());
    let plan = adapter
        .plan(&inst, &make_request(OpKind::Upgrade, &inst.id, "pkg"))
        .await
        .expect("plan");
    let op_id = manager.submit(plan);

    wait_for_status(
        &manager,
        op_id,
        OpStatus::Running,
        Duration::from_millis(1000),
    )
    .await;
    manager.cancel(op_id);

    assert_eq!(manager.wait(op_id).await, Some(Outcome::Cancelled));
    assert_eq!(
        adapter.execute_calls(),
        0,
        "an upgrade cancelled during its before-reading must never invoke execute"
    );
}
