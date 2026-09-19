//! `Session`: the facade `canager-core` exposes to a host shell (the Tauri
//! app in this repo, or a test harness). It owns the registered adapters,
//! the last known set of instances, and an in-memory, generation-numbered
//! `Snapshot`; it forwards operation lifecycle calls to an internal
//! `OperationManager`. Split (Task 14) into this facade, `refresh.rs`
//! (detection and the inventory/updates fetch) and `plans.rs`
//! (preview-then-confirm); no behaviour changed in the split itself.

mod plans;
mod refresh;

// `CheckOptions` and `AdapterError` are deliberately absent from this list:
// `refresh` (which took `CheckOptions`) now lives in `refresh.rs`, and
// `issue_plan`/`submit` (which took `AdapterError`) now live in `plans.rs`;
// each imports what it needs itself. Leaving either here would be an unused
// import under `-D warnings` -- the facade's own top-level code no longer
// touches either type.
use crate::adapters::brew::BrewAdapter;
use crate::adapters::cargo::CargoAdapter;
use crate::adapters::npm::NpmAdapter;
use crate::adapters::ollama::OllamaAdapter;
use crate::adapters::pip::PipAdapter;
use crate::adapters::pipx::PipxAdapter;
use crate::adapters::uv::UvAdapter;
use crate::adapters::Adapter;
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, RealHttpClient};
use crate::model::{
    AdapterId, InstalledArtifact, InstanceId, ManagerInstance, Plan, UpdateCandidate,
};
use crate::ops::{OpSummary, OperationManager};
use crate::runner::{CommandRunner, RealRunner};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceError {
    pub instance_id: InstanceId,
    pub message: String,
}

/// Why an adapter reported no usable instance. `Missing` is the ordinary
/// "Homebrew is not installed" case; `RefusedAsRoot` must be surfaced
/// differently in the UI (spec §7 empty states).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectOutcome {
    Found,
    Missing,
    RefusedAsRoot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: u64,
    pub detect: DetectOutcome,
    pub instances: Vec<ManagerInstance>,
    pub artifacts: Vec<InstalledArtifact>,
    pub updates: Vec<UpdateCandidate>,
    /// Unix seconds of the last fully successful refresh, if any.
    pub refreshed_at: Option<i64>,
    /// True when the newest refresh attempt failed and this data is older
    /// than it looks (spec §3: keep old data, mark it possibly stale).
    pub stale: bool,
    pub errors: Vec<SourceError>,
}

impl Snapshot {
    fn empty() -> Snapshot {
        Snapshot {
            generation: 0,
            detect: DetectOutcome::Missing,
            instances: Vec::new(),
            artifacts: Vec::new(),
            updates: Vec::new(),
            refreshed_at: None,
            stale: false,
            errors: Vec::new(),
        }
    }

    /// Whether `self` and `other` carry the same *data* -- every field
    /// except `generation`, `refreshed_at` and `stale`, which describe the
    /// refresh attempt rather than the fetched data itself.
    fn same_content(&self, other: &Snapshot) -> bool {
        self.detect == other.detect
            && self.instances == other.instances
            && self.artifacts == other.artifacts
            && self.updates == other.updates
            && self.errors == other.errors
    }
}

/// Opaque handle to a plan `Session` has issued and is holding server-side.
/// The front end never constructs one; it only ever echoes back the `id` it
/// was given.
pub type PlanId = u64;

/// A `Plan` the server has already computed and stored, returned to the
/// caller for preview. Submitting requires only the `id`; the `plan` field
/// is for display (exact command preview, spec §6) and is never accepted
/// back from the client (see `Session::submit`, in `plans.rs`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedPlan {
    pub id: PlanId,
    pub plan: Plan,
    /// Unix seconds when this plan was issued, used to decide expiry.
    pub issued_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SubmitError {
    #[error("no such plan, or it was already submitted")]
    Unknown,
    #[error("this plan is older than 10 minutes; preview it again")]
    Expired,
}

pub struct Session {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    ops: Arc<OperationManager>,
    /// Serialises `refresh()` (in `refresh.rs`): whichever caller acquires
    /// this first does the real work; anyone already waiting when it
    /// releases just re-reads `snapshot`.
    refresh_gate: tokio::sync::Mutex<()>,
    snapshot: Mutex<Snapshot>,
    /// Bumped every time a refresh actually completes, regardless of
    /// whether its content -- and therefore `generation` -- changed. See
    /// `refresh.rs`'s doc comment for why a waiter needs this instead of
    /// `generation` alone.
    refresh_seq: AtomicU64,
    /// Plans handed out by `issue_plan` (in `plans.rs`) but not yet
    /// consumed by `submit`, keyed by `PlanId`.
    issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>,
    next_plan_id: AtomicU64,
    now_fn: Option<fn() -> i64>,
}

impl Session {
    /// Registers all seven adapters over a shared `RealRunner` and
    /// `RealHttpClient` (network-touching adapters only: pipx, cargo,
    /// ollama). `now_fn` exists so tests can pin `refreshed_at`; production
    /// passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::new());
        let http: Arc<dyn HttpClient> = Arc::new(RealHttpClient::new());
        let adapters: Vec<Arc<dyn Adapter>> = vec![
            Arc::new(BrewAdapter::new(runner.clone())),
            Arc::new(NpmAdapter::new(runner.clone())),
            Arc::new(PipxAdapter::new(runner.clone(), http.clone())),
            Arc::new(UvAdapter::new(runner.clone())),
            Arc::new(PipAdapter::new(runner.clone())),
            Arc::new(CargoAdapter::new(runner.clone(), http.clone())),
            Arc::new(OllamaAdapter::new(runner, http)),
        ];
        Session::with_adapters(sink, adapters, now_fn)
    }

    /// Test seam: build a Session over arbitrary adapters.
    pub fn with_adapters(
        sink: Arc<dyn EventSink>,
        adapters: Vec<Arc<dyn Adapter>>,
        now_fn: Option<fn() -> i64>,
    ) -> Arc<Session> {
        let mut ops = OperationManager::new(sink);
        let mut by_id = HashMap::new();
        for adapter in adapters {
            ops.register_adapter(adapter.clone());
            by_id.insert(adapter.meta().id.clone(), adapter);
        }
        Arc::new(Session {
            adapters: by_id,
            ops: Arc::new(ops),
            refresh_gate: tokio::sync::Mutex::new(()),
            snapshot: Mutex::new(Snapshot::empty()),
            refresh_seq: AtomicU64::new(0),
            issued_plans: Mutex::new(HashMap::new()),
            next_plan_id: AtomicU64::new(1),
            now_fn,
        })
    }

    fn now(&self) -> i64 {
        match self.now_fn {
            Some(f) => f(),
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub fn cancel(&self, op_id: OpId) {
        self.ops.cancel(op_id)
    }

    pub fn operations(&self) -> Vec<OpSummary> {
        self.ops.summaries()
    }

    /// Sorted ids of every adapter this Session has registered, regardless
    /// of whether that adapter currently detects any instance on the host.
    /// A test seam (Task 11) so registration itself is verifiable without
    /// depending on which tools happen to be installed on the machine
    /// running the test.
    pub fn adapter_ids(&self) -> Vec<AdapterId> {
        let mut ids: Vec<AdapterId> = self.adapters.keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, OpKind, OpRequest, OpStatus,
        Outcome, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    };
    use crate::runner::HostEnv;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct FakeState {
        instances: Vec<ManagerInstance>,
        block_execute: bool,
    }

    struct FakeAdapter {
        meta: AdapterMeta,
        state: Arc<StdMutex<FakeState>>,
    }

    impl FakeAdapter {
        fn new() -> (Arc<FakeAdapter>, Arc<StdMutex<FakeState>>) {
            let state = Arc::new(StdMutex::new(FakeState {
                instances: Vec::new(),
                block_execute: false,
            }));
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
                state: state.clone(),
            });
            (adapter, state)
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
                background_check: true,
                cancel_safe: true,
            }
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.state.lock().unwrap().instances.clone()
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

        async fn plan(
            &self,
            inst: &ManagerInstance,
            req: &OpRequest,
        ) -> Result<Plan, AdapterError> {
            Ok(Plan {
                request: req.clone(),
                program: inst.exe_path.clone(),
                args: vec!["do".to_string(), req.name.clone()],
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
            let block = self.state.lock().unwrap().block_execute;
            if block {
                cancel.cancelled().await;
                Ok(Outcome::Unconfirmed)
            } else {
                Ok(Outcome::Succeeded)
            }
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
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    fn non_root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        }
    }

    #[test]
    fn test_new_registers_all_seven_adapters() {
        let sink = Arc::new(VecSink::new());
        let session = Session::new(sink, None);
        assert_eq!(
            session.adapter_ids(),
            vec![
                "brew".to_string(),
                "cargo".to_string(),
                "npm".to_string(),
                "ollama".to_string(),
                "pip".to_string(),
                "pipx".to_string(),
                "uv".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_submit_cancel_and_operations_forward_to_the_operation_manager() {
        let (adapter, state) = FakeAdapter::new();
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake:1")];
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if session
                .operations()
                .iter()
                .any(|o| o.id == op_id && o.status == OpStatus::Running)
            {
                break;
            }
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(session.operations().len(), 1);

        session.cancel(op_id);

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(summary) = session.operations().into_iter().find(|o| o.id == op_id) {
                if summary.status == OpStatus::Done {
                    assert_eq!(summary.outcome, Some(Outcome::Succeeded));
                    break;
                }
            }
            assert!(Instant::now() < deadline, "operation never finished");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
