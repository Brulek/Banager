//! (F8 / M9) Contract test for `OperationManager`'s cross-resource
//! concurrency cap: spec §6 requires "same lock serial, different locks
//! parallel, at most 3 at once" — before this fix, `submit` only looked at
//! resource locks, so four operations on four different resources would all
//! run concurrently with no cap at all.

use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions};
use canager_core::events::{EventSink, OpId, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// An adapter whose `execute` blocks in the middle so the test can observe
/// exactly how many ops are concurrently inside `execute`, and control when
/// each one is allowed to finish.
struct BlockingAdapter {
    meta: AdapterMeta,
    in_execute: Arc<AtomicUsize>,
    max_seen: Arc<AtomicUsize>,
    release: Arc<Notify>,
}

impl BlockingAdapter {
    fn new(in_execute: Arc<AtomicUsize>, max_seen: Arc<AtomicUsize>, release: Arc<Notify>) -> Self {
        BlockingAdapter {
            meta: AdapterMeta {
                id: "fake".to_string(),
                name: "fake".to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            in_execute,
            max_seen,
            release,
        }
    }
}

#[async_trait]
impl Adapter for BlockingAdapter {
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
        let current = self.in_execute.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_seen.fetch_max(current, Ordering::SeqCst);
        // Block here until the test explicitly lets this op proceed.
        self.release.notified().await;
        self.in_execute.fetch_sub(1, Ordering::SeqCst);
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
    make_instance_for_adapter(id, "fake")
}

fn make_instance_for_adapter(id: &str, adapter_id: &str) -> ManagerInstance {
    ManagerInstance {
        id: id.to_string(),
        adapter_id: adapter_id.to_string(),
        exe_path: PathBuf::from("/bin/true"),
        prefix: PathBuf::from("/"),
        scope: Scope::User,
        version: None,
        healthy: true,
        unverified_version: None,
        read_only_reason: None,
    }
}

async fn wait_until(condition: impl Fn() -> bool, timeout: Duration, what: &str) {
    let start = tokio::time::Instant::now();
    loop {
        if condition() {
            return;
        }
        if start.elapsed() > timeout {
            panic!("timed out after {:?} waiting for: {}", timeout, what);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn test_at_most_three_concurrent_operations_across_different_locks() {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let in_execute = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let adapter = Arc::new(BlockingAdapter::new(
        in_execute.clone(),
        max_seen.clone(),
        release.clone(),
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    // Four distinct resources: nothing here contends on a `ResourceLock`, so
    // the only thing that can limit concurrency is the semaphore.
    let mut op_ids = Vec::new();
    for name in ["a", "b", "c", "d"] {
        let inst = make_instance(&format!("fake:/{name}"));
        manager.register_instance(inst.clone());
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: name.to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        op_ids.push(manager.submit(plan));
    }

    // Exactly 3 should be able to enter `execute` and no more, however long
    // we wait.
    wait_until(
        || in_execute.load(Ordering::SeqCst) == 3,
        Duration::from_secs(2),
        "3 ops concurrently in execute",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        in_execute.load(Ordering::SeqCst),
        3,
        "a 4th op must not enter execute while 3 are already running"
    );
    assert_eq!(max_seen.load(Ordering::SeqCst), 3);

    // Release exactly one; that frees a permit, letting the 4th op in.
    release.notify_one();
    wait_until(
        || in_execute.load(Ordering::SeqCst) == 3,
        Duration::from_secs(2),
        "the 4th op to take the freed permit",
    )
    .await;

    // Let everything finish.
    release.notify_one();
    release.notify_one();
    release.notify_one();

    for id in op_ids {
        let outcome = manager.wait(id).await;
        assert_eq!(outcome, Some(Outcome::Succeeded));
    }
}

/// A second fake adapter used only by the starvation-regression test below.
/// Unlike `BlockingAdapter`'s `in_execute` (which counts *currently*
/// in-flight executions and can go back down), `entered_count` only ever
/// increases: it records how many distinct operations have *ever* reached
/// `execute`, which is exactly what the test needs to observe an
/// independent 4th operation getting in while two same-lock operations are
/// still stuck queued on their resource lock (never having reached
/// `execute` at all).
struct SerializingAdapter {
    meta: AdapterMeta,
    entered_count: Arc<AtomicUsize>,
    release: Arc<Notify>,
}

impl SerializingAdapter {
    fn new(entered_count: Arc<AtomicUsize>, release: Arc<Notify>) -> Self {
        SerializingAdapter {
            meta: AdapterMeta {
                id: "fake2".to_string(),
                name: "fake2".to_string(),
                kind: "fake2".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            entered_count,
            release,
        }
    }
}

#[async_trait]
impl Adapter for SerializingAdapter {
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
        self.entered_count.fetch_add(1, Ordering::SeqCst);
        // Block here until the test explicitly lets this particular
        // execution proceed.
        self.release.notified().await;
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

/// (Starvation regression) Three operations all target the *same* resource
/// lock, so only one of them can ever be inside `execute` — the other two
/// sit in `run_operation`'s lock-wait poll loop the whole time, having
/// never reached `execute` (and, after the fix, never even requested a
/// concurrency permit). A fourth operation on a completely different,
/// uncontended lock must still be able to start immediately: spec §6 says
/// "same lock serial, different locks parallel, at most 3 concurrent", and
/// the semaphore's own doc comment says it caps operations *actually
/// running* `execute`.
///
/// Before the fix, permits were acquired *before* the lock-wait loop, so
/// the two merely-queued operations each held a permit while doing nothing
/// but waiting — together with the one permit legitimately held by the op
/// that's actually running, all 3 permits are exhausted by operations that
/// are not executing, and the 4th operation (on a completely free lock)
/// can never acquire a permit at all. This test must time out / fail
/// against that code and pass once the permit is acquired after the lock.
#[tokio::test]
async fn test_fourth_op_on_free_lock_is_not_starved_by_two_ops_queued_on_another_lock() {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let entered_count = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let adapter = Arc::new(SerializingAdapter::new(
        entered_count.clone(),
        release.clone(),
    ));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    // Three ops all target the SAME resource lock (same instance id).
    let contended = make_instance_for_adapter("fake2:/contended", "fake2");
    manager.register_instance(contended.clone());
    let mut same_lock_ops = Vec::new();
    for name in ["op1", "op2", "op3"] {
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: contended.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: name.to_string(),
        };
        let plan = adapter.plan(&contended, &req).await.expect("plan");
        same_lock_ops.push(manager.submit(plan));
    }

    // Let the lock race settle: exactly one of the three should get in;
    // the other two are merely waiting for that lock to free up.
    wait_until(
        || entered_count.load(Ordering::SeqCst) == 1,
        Duration::from_secs(2),
        "exactly one same-lock op to reach execute",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        entered_count.load(Ordering::SeqCst),
        1,
        "only one of the three same-lock ops should ever be executing at this point; the \
         other two must still be waiting for the resource lock"
    );

    // A fourth op on a completely different, uncontended lock.
    let free = make_instance_for_adapter("fake2:/free", "fake2");
    manager.register_instance(free.clone());
    let req4 = OpRequest {
        kind: OpKind::Install,
        instance_id: free.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "op4".to_string(),
    };
    let plan4 = adapter.plan(&free, &req4).await.expect("plan");
    let op4 = manager.submit(plan4);

    // The 4th op must reach execute while the other two same-lock ops are
    // still queued (entered_count must reach 2: the original winner plus
    // this independent 4th op) — never blocked behind permits hoarded by
    // operations that are merely waiting for an unrelated lock.
    wait_until(
        || entered_count.load(Ordering::SeqCst) == 2,
        Duration::from_secs(2),
        "the 4th op (different, uncontended lock) to reach execute while 2 ops are still \
         queued on the contended lock",
    )
    .await;

    // Drain everything so the test doesn't leave background tasks hanging.
    // Ops 2 and 3 (whichever two didn't win the initial race) only reach
    // `execute` serially, once the lock ahead of them frees up, so this
    // just keeps nudging `release` and re-checking each op in turn until
    // every one of them reports `Done`.
    let all_ops: Vec<OpId> = same_lock_ops
        .into_iter()
        .chain(std::iter::once(op4))
        .collect();
    for id in all_ops {
        loop {
            release.notify_one();
            match tokio::time::timeout(Duration::from_millis(100), manager.wait(id)).await {
                Ok(outcome) => {
                    assert_eq!(outcome, Some(Outcome::Succeeded));
                    break;
                }
                Err(_) => continue,
            }
        }
    }
}
