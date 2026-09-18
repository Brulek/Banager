//! (F8 / M9) Contract test for `OperationManager`'s cross-resource
//! concurrency cap: spec §6 requires "same lock serial, different locks
//! parallel, at most 3 at once" — before this fix, `submit` only looked at
//! resource locks, so four operations on four different resources would all
//! run concurrently with no cap at all.

use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};
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
