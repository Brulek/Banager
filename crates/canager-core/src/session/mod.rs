//! `Session`: the facade `canager-core` exposes to a host shell (the Tauri
//! app in this repo, or a test harness). It owns the registered adapters,
//! the last known set of instances, and an in-memory, generation-numbered
//! `Snapshot`; it forwards operation lifecycle calls to an internal
//! `OperationManager`. See
//! `docs/superpowers/plans/2026-09-19-phase-2-ui-shell.md`'s Core
//! Interfaces section — every name and shape here is fixed by that
//! document.

use crate::adapters::brew::BrewAdapter;
use crate::adapters::{Adapter, AdapterError, CheckOptions};
use crate::events::{EventSink, OpId};
use crate::model::{
    AdapterId, InstalledArtifact, InstanceId, ManagerInstance, OpRequest, Plan, ResourceLock,
    UpdateCandidate,
};
use crate::ops::{OpSummary, OperationManager};
use crate::runner::HostEnv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
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

    /// Whether `self` and `other` carry the same *data* — every field
    /// except `generation`, `refreshed_at` and `stale`, which describe the
    /// refresh attempt rather than the fetched data itself.
    ///
    /// Deliberately excludes `refreshed_at`: comparing the *full* struct
    /// (as the design review's M6 suggested) would mean `generation` bumps
    /// on every successful refresh, since `refreshed_at` changes every
    /// time — at which point `generation` stops meaning "the content
    /// changed" and a front end watching it for that reason gets bumped on
    /// every poll for no visible reason. `generation` keeps that meaning by
    /// design; `refresh_seq` below (M5) is the separate counter that
    /// actually solves the concurrent-refresh-coalescing problem M6's
    /// suggestion was trying to fix.
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
/// back from the client (see `Session::submit`).
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
    /// Serialises `refresh()`: whichever caller acquires this first does
    /// the real work; anyone already waiting when it releases just re-reads
    /// `snapshot` (see `refresh`'s doc comment for the exact protocol).
    refresh_gate: tokio::sync::Mutex<()>,
    snapshot: Mutex<Snapshot>,
    /// Bumped every time a refresh actually completes, regardless of
    /// whether its content — and therefore `generation` — changed (M5 in
    /// the design review). `generation` alone cannot tell a waiter "someone
    /// else already finished a refresh while I waited for the gate" apart
    /// from "no one has run since I last checked": two refreshes in a row
    /// can fetch identical data, in which case `generation` does not move
    /// even though a real refresh happened. `refresh_seq` always moves, so
    /// it is what `refresh` actually checks to decide whether to coalesce.
    refresh_seq: AtomicU64,
    /// Plans handed out by `issue_plan` but not yet consumed by `submit`,
    /// keyed by `PlanId`. `submit` removes its entry on use, so each plan
    /// can be submitted at most once; an entry older than 600 seconds is
    /// rejected as expired instead of being proactively swept, since this
    /// only grows by one entry per preview an operator actually looks at.
    issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>,
    next_plan_id: AtomicU64,
    now_fn: Option<fn() -> i64>,
}

impl Session {
    /// Registers the Homebrew adapter with a `RealRunner`. `now_fn` exists
    /// so tests can pin `refreshed_at`; production passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let brew = Arc::new(BrewAdapter::new(Arc::new(crate::runner::RealRunner::new())));
        Session::with_adapters(sink, vec![brew], now_fn)
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

    /// Detect instances, then inventory + check updates for each instance
    /// concurrently (each under that instance's own resource lock — see
    /// below). Bumps `generation` only when the resulting data actually
    /// differs from the previous snapshot. Per-instance failures land in
    /// `errors` and set `stale`; they never abort the whole refresh, and a
    /// failing instance's *previous* artifacts/updates are kept rather than
    /// dropped, so a transient failure never makes something the user
    /// installed appear to vanish. Concurrent calls are serialised: a call
    /// that starts while another is already running waits for it, then
    /// returns the snapshot that other call produced instead of running a
    /// second, redundant refresh — see `refresh_seq` on `Session` for why
    /// that check cannot use `generation`.
    pub async fn refresh(self: &Arc<Self>, env: &HostEnv, opts: &CheckOptions) -> Snapshot {
        let seq_before = self.refresh_seq.load(Ordering::SeqCst);
        let _gate = self.refresh_gate.lock().await;
        if self.refresh_seq.load(Ordering::SeqCst) != seq_before {
            // Another call already completed a refresh while we waited for
            // the gate. Its result is exactly what we would produce — even
            // when its content was identical to what came before and so
            // left `generation` unchanged (M5 in the design review): a
            // second, redundant run of the adapters must not happen just
            // because nothing looked different.
            return self.snapshot.lock().unwrap().clone();
        }

        let previous = self.snapshot.lock().unwrap().clone();
        // Owned copy (CheckOptions is Copy): each per-instance spawned task
        // below needs its own 'static value, and the caller's `&opts`
        // reference cannot outlive this function.
        let opts: CheckOptions = *opts;

        if BrewAdapter::refuses_as_root(env) {
            // This refresh ran to completion: it did not fail, it answered
            // "Canager cannot run as root", which is a definitive result
            // about the host and not a missing one. So it stamps
            // `refreshed_at` like any other completed refresh. Carrying
            // `previous.refreshed_at` forward instead left it `None` on a
            // process's first refresh, and the front end reads a null
            // `refreshed_at` with no errors as "no refresh has finished
            // yet" — which, since a process's euid never changes, would
            // have been true forever.
            let refused = Snapshot {
                generation: previous.generation,
                detect: DetectOutcome::RefusedAsRoot,
                instances: Vec::new(),
                artifacts: Vec::new(),
                updates: Vec::new(),
                refreshed_at: Some(self.now()),
                stale: previous.stale,
                errors: Vec::new(),
            };
            return self.commit(previous, refused);
        }

        let mut instances = Vec::new();
        for adapter in self.adapters.values() {
            instances.extend(adapter.detect(env).await);
        }
        for inst in &instances {
            self.ops.register_instance(inst.clone());
        }
        let detect = if instances.is_empty() {
            DetectOutcome::Missing
        } else {
            DetectOutcome::Found
        };

        // M8 in the design review: take the *same* per-instance resource
        // lock a submitted install/upgrade/uninstall holds for the whole
        // inventory+check_updates segment below, so a refresh can never
        // observe a half-updated filesystem while an operation on that
        // instance is running (and vice versa). Each instance's fetch is
        // its own spawned task so that a lock held by a slow or blocked
        // operation on *one* instance only ever delays that instance's
        // fetch — spec §6's "same lock serial, different locks parallel"
        // applies here exactly as it does to operations themselves.
        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                continue;
            };
            let ops = self.ops.clone();
            let previous = previous.clone();
            // `opts` is `Copy`, so the `async move` block below captures its
            // own value rather than borrowing this function's.
            handles.push((
                inst.id.clone(),
                tokio::spawn(async move {
                    let _lock = ops
                        .acquire_resource_lock(ResourceLock(inst.id.clone()))
                        .await;
                    let mut artifacts = Vec::new();
                    let mut updates = Vec::new();
                    let mut errors = Vec::new();
                    let mut stale = false;
                    match adapter.inventory(&inst).await {
                        Ok(items) => artifacts.extend(items),
                        Err(e) => {
                            errors.push(SourceError {
                                instance_id: inst.id.clone(),
                                message: e.to_string(),
                            });
                            stale = true;
                            artifacts.extend(
                                previous
                                    .artifacts
                                    .iter()
                                    .filter(|a| a.key.instance_id == inst.id)
                                    .cloned(),
                            );
                        }
                    }
                    match adapter.check_updates(&inst, &opts).await {
                        Ok(items) => updates.extend(items),
                        Err(e) => {
                            errors.push(SourceError {
                                instance_id: inst.id.clone(),
                                message: e.to_string(),
                            });
                            stale = true;
                            updates.extend(
                                previous
                                    .updates
                                    .iter()
                                    .filter(|u| u.key.instance_id == inst.id)
                                    .cloned(),
                            );
                        }
                    }
                    (artifacts, updates, errors, stale)
                }),
            ));
        }

        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut errors = Vec::new();
        let mut stale = false;
        for (instance_id, handle) in handles {
            match handle.await {
                Ok((a, u, e, s)) => {
                    artifacts.extend(a);
                    updates.extend(u);
                    errors.extend(e);
                    stale = stale || s;
                }
                Err(_join_err) => {
                    errors.push(SourceError {
                        instance_id,
                        message: "internal error refreshing this instance".to_string(),
                    });
                    stale = true;
                }
            }
        }

        let refreshed_at = if stale {
            previous.refreshed_at
        } else {
            Some(self.now())
        };
        let candidate = Snapshot {
            generation: previous.generation,
            detect,
            instances,
            artifacts,
            updates,
            refreshed_at,
            stale,
            errors,
        };
        self.commit(previous, candidate)
    }

    /// Assigns the real generation number (bumping only on a content
    /// change), stores the result as the current snapshot, and marks this
    /// refresh complete via `refresh_seq` regardless of whether `generation`
    /// moved (M5 in the design review — see `refresh_seq`'s field doc).
    fn commit(&self, previous: Snapshot, mut candidate: Snapshot) -> Snapshot {
        if !previous.same_content(&candidate) {
            candidate.generation = previous.generation + 1;
        }
        *self.snapshot.lock().unwrap() = candidate.clone();
        self.refresh_seq.fetch_add(1, Ordering::SeqCst);
        candidate
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    /// Resolves `req` to its owning adapter, asks it to plan the operation,
    /// then stores the resulting `Plan` under a fresh `PlanId` and returns
    /// both as an `IssuedPlan`. The caller previews `issued.plan`; nothing
    /// in it is ever accepted back — `submit` takes only `issued.id`.
    pub async fn issue_plan(&self, req: &OpRequest) -> Result<IssuedPlan, AdapterError> {
        let instance = self
            .snapshot
            .lock()
            .unwrap()
            .instances
            .iter()
            .find(|i| i.id == req.instance_id)
            .cloned()
            .ok_or_else(|| {
                AdapterError::Refused(format!("unknown instance {}", req.instance_id))
            })?;
        let adapter = self.adapters.get(&instance.adapter_id).ok_or_else(|| {
            AdapterError::Refused(format!("no adapter registered for {}", instance.adapter_id))
        })?;
        let plan = adapter.plan(&instance, req).await?;
        let id = self.next_plan_id.fetch_add(1, Ordering::SeqCst);
        let issued = IssuedPlan {
            id,
            plan,
            issued_at: self.now(),
        };
        self.issued_plans.lock().unwrap().insert(id, issued.clone());
        Ok(issued)
    }

    /// Removes (one-time consumption) the issued plan stored under
    /// `plan_id` and submits exactly that stored `Plan`. Fails with
    /// `SubmitError::Unknown` if `plan_id` was never issued or was already
    /// submitted once, and `SubmitError::Expired` if it was issued more
    /// than 600 seconds ago — the client can never influence what actually
    /// runs, since nothing it sends is used except this opaque id.
    pub fn submit(self: &Arc<Self>, plan_id: PlanId) -> Result<OpId, SubmitError> {
        let issued = {
            let mut plans = self.issued_plans.lock().unwrap();
            plans.remove(&plan_id).ok_or(SubmitError::Unknown)?
        };
        if self.now() - issued.issued_at > 600 {
            return Err(SubmitError::Expired);
        }
        Ok(self.ops.submit(issued.plan))
    }

    pub fn cancel(&self, op_id: OpId) {
        self.ops.cancel(op_id)
    }

    pub fn operations(&self) -> Vec<OpSummary> {
        self.ops.summaries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKind, CancelPolicy, InstallReason, OpKind, OpStatus, Outcome, Reconciled,
        ResourceLock, Scope, SearchHit,
    };
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicI64;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    /// Controls one `FakeAdapter`'s behaviour so a single test can flip a
    /// source from healthy to failing mid-run, add an artificial `detect()`
    /// delay to observe refresh coalescing, or block `execute()` on
    /// cancellation. Not shared with any other test file's `FakeAdapter`.
    struct FakeState {
        instances: Vec<ManagerInstance>,
        artifacts: HashMap<InstanceId, Vec<InstalledArtifact>>,
        updates: HashMap<InstanceId, Vec<UpdateCandidate>>,
        /// Instance ids whose `inventory` should fail on the *next* call
        /// only (consumed on use).
        failing: Vec<InstanceId>,
        detect_delay: Duration,
        detect_calls: usize,
        block_execute: bool,
        /// Every instance id `inventory()` was actually called for, in
        /// call order — used by `test_refresh_is_mutually_exclusive_...`
        /// to observe that one instance's fetch proceeded while another's
        /// was still blocked on a resource lock (M8 in the design review).
        inventory_calls: Vec<InstanceId>,
    }

    struct FakeAdapter {
        meta: AdapterMeta,
        state: Arc<Mutex<FakeState>>,
    }

    impl FakeAdapter {
        fn new(id: &str) -> (Arc<FakeAdapter>, Arc<Mutex<FakeState>>) {
            let state = Arc::new(Mutex::new(FakeState {
                instances: Vec::new(),
                artifacts: HashMap::new(),
                updates: HashMap::new(),
                failing: Vec::new(),
                detect_delay: Duration::from_millis(0),
                detect_calls: 0,
                block_execute: false,
                inventory_calls: Vec::new(),
            }));
            let adapter = Arc::new(FakeAdapter {
                meta: AdapterMeta {
                    id: id.to_string(),
                    name: id.to_string(),
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
            let delay = {
                let mut s = self.state.lock().unwrap();
                s.detect_calls += 1;
                s.detect_delay
            };
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.state.lock().unwrap().instances.clone()
        }

        async fn inventory(
            &self,
            inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            let mut s = self.state.lock().unwrap();
            s.inventory_calls.push(inst.id.clone());
            if let Some(pos) = s.failing.iter().position(|id| id == &inst.id) {
                s.failing.remove(pos);
                return Err(AdapterError::CommandFailed {
                    code: Some(1),
                    stderr: format!("{} inventory failed", inst.id),
                });
            }
            Ok(s.artifacts.get(&inst.id).cloned().unwrap_or_default())
        }

        async fn check_updates(
            &self,
            inst: &ManagerInstance,
            _opts: &CheckOptions,
        ) -> Result<Vec<UpdateCandidate>, AdapterError> {
            let s = self.state.lock().unwrap();
            Ok(s.updates.get(&inst.id).cloned().unwrap_or_default())
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
            _key: &crate::model::ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(Reconciled {
                present: true,
                version: None,
            })
        }
    }

    fn make_instance(adapter_id: &str, id: &str) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: adapter_id.to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    fn make_artifact(instance_id: &str, name: &str) -> InstalledArtifact {
        InstalledArtifact {
            key: crate::model::ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: "1.0".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        }
    }

    fn non_root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        }
    }

    fn root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
        }
    }

    #[tokio::test]
    async fn test_refresh_populates_snapshot_from_adapter() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(snapshot.detect, DetectOutcome::Found);
        assert_eq!(snapshot.instances.len(), 1);
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].display_name, "jq");
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty());
        assert!(snapshot.refreshed_at.is_some());
        assert_eq!(snapshot.generation, 1);
    }

    #[tokio::test]
    async fn test_refresh_as_root_refuses_without_calling_adapters() {
        let (adapter, state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;
        assert_eq!(snapshot.detect, DetectOutcome::RefusedAsRoot);
        assert!(snapshot.instances.is_empty());
        assert_eq!(
            state.lock().unwrap().detect_calls,
            0,
            "no adapter should be probed while running as root"
        );
    }

    /// A refresh that ran to completion and definitively answered "Canager
    /// cannot run as root" *is* a completed refresh, so it must stamp
    /// `refreshed_at`. Leaving it `None` (as carrying `previous.refreshed_at`
    /// forward did on a process's first refresh) made the front end's
    /// "no refresh has finished yet" loading branch match forever: a
    /// process's euid never changes, so no later refresh could clear it
    /// either.
    #[tokio::test]
    async fn test_refresh_as_root_stamps_refreshed_at() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;
        assert_eq!(snapshot.detect, DetectOutcome::RefusedAsRoot);
        assert!(
            snapshot.refreshed_at.is_some(),
            "a root refusal is a completed refresh and must set refreshed_at"
        );
    }

    #[tokio::test]
    async fn test_refresh_with_no_instances_yields_missing() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(snapshot.detect, DetectOutcome::Missing);
    }

    #[tokio::test]
    async fn test_refresh_keeps_previous_data_and_flags_stale_on_per_instance_failure() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.artifacts
                .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(first.artifacts.len(), 2);
        assert!(!first.stale);
        let first_refreshed_at = first.refreshed_at;

        state.lock().unwrap().failing.push("fake:1".to_string());
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(second.stale);
        assert_eq!(second.errors.len(), 1);
        assert_eq!(second.errors[0].instance_id, "fake:1");
        assert!(second.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(second.artifacts.iter().any(|a| a.key.name == "wget"));
        assert_eq!(
            second.refreshed_at, first_refreshed_at,
            "a refresh with a per-instance failure must not claim a new successful timestamp"
        );
    }

    #[tokio::test]
    async fn test_refresh_generation_unchanged_when_nothing_changed() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(
            first.generation, second.generation,
            "identical data must not bump the generation"
        );

        state
            .lock()
            .unwrap()
            .artifacts
            .get_mut("fake:1")
            .unwrap()
            .push(make_artifact("fake:1", "wget"));
        let third = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(
            third.generation > second.generation,
            "new data must bump the generation"
        );
    }

    #[tokio::test]
    async fn test_concurrent_refresh_calls_are_coalesced() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.detect_delay = Duration::from_millis(100);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let session_a = session.clone();
        let session_b = session.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move {
                session_a
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
            tokio::spawn(async move {
                session_b
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
        );
        let snap_a = a.expect("task a");
        let snap_b = b.expect("task b");
        assert_eq!(snap_a.generation, snap_b.generation);
        assert_eq!(
            state.lock().unwrap().detect_calls,
            1,
            "two concurrent refreshes must run detect() only once between them"
        );
    }

    #[tokio::test]
    async fn test_concurrent_refresh_calls_with_unchanged_content_are_still_coalesced() {
        // Regression guard for M5 in the design review: `generation` only
        // advances when content actually changes, so by itself it cannot
        // tell "another refresh already completed while I waited for the
        // gate" apart from "no refresh has run since I last checked" — two
        // refreshes back to back that both see identical data must still
        // coalesce into one `detect()` call, not run the adapters twice.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        // Establish a steady-state snapshot first, outside any concurrency.
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let calls_before = state.lock().unwrap().detect_calls;

        // Every refresh from here on sees exactly the same data as above,
        // so `generation` will not advance no matter how many times it
        // runs — that must not be mistaken for "no refresh has happened".
        state.lock().unwrap().detect_delay = Duration::from_millis(100);
        let session_a = session.clone();
        let session_b = session.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move {
                session_a
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
            tokio::spawn(async move {
                session_b
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
        );
        let snap_a = a.expect("task a");
        let snap_b = b.expect("task b");
        assert_eq!(snap_a.generation, snap_b.generation);
        assert_eq!(
            state.lock().unwrap().detect_calls,
            calls_before + 1,
            "two concurrent refreshes over unchanged content must still run detect() only once between them"
        );
    }

    #[tokio::test]
    async fn test_snapshot_returns_cached_value_without_calling_adapters() {
        let (adapter, state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let before = session.snapshot();
        assert_eq!(before.generation, 0);
        assert_eq!(state.lock().unwrap().detect_calls, 0);
        let refreshed = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let after = session.snapshot();
        assert_eq!(after, refreshed);
    }

    #[tokio::test]
    async fn test_issue_plan_delegates_to_the_owning_adapter() {
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
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
        assert_eq!(issued.id, 1, "PlanId numbering starts at 1");
        assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);
    }

    #[tokio::test]
    async fn test_issue_plan_for_unknown_instance_is_refused() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "does-not-exist".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        match session.issue_plan(&req).await {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_submit_cancel_and_operations_forward_to_the_operation_manager() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
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

    #[tokio::test]
    async fn test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others(
    ) {
        // Regression guard for M8 in the design review: refresh() must take
        // the same per-instance resource lock a submitted operation holds,
        // so it can never observe fake:1's filesystem state while an
        // install/upgrade/uninstall on fake:1 is still running — but that
        // must not hold up fake:2's fetch, which uses a different lock.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.artifacts
                .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        state.lock().unwrap().inventory_calls.clear();

        // Submit (and thereby lock) an operation against fake:1 only, and
        // hold it there — `block_execute` makes `execute()` wait on
        // cancellation — until this test releases it below.
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

        let session_for_refresh = session.clone();
        let refresh_task = tokio::spawn(async move {
            session_for_refresh
                .refresh(&non_root_env(), &CheckOptions::default())
                .await
        });

        // Give the refresh time to reach fake:2's inventory (no contention)
        // and to *try* fake:1's (which must still be waiting on the lock
        // fake:1's running operation holds).
        tokio::time::sleep(Duration::from_millis(200)).await;
        {
            let calls = state.lock().unwrap().inventory_calls.clone();
            assert!(
                calls.contains(&"fake:2".to_string()),
                "a different instance's refresh must proceed while fake:1 is locked"
            );
            assert!(
                !calls.contains(&"fake:1".to_string()),
                "fake:1's refresh must not run while fake:1's operation is still holding its lock"
            );
        }

        // Release fake:1's lock; the refresh (and the operation) must now
        // both complete, and the snapshot must reflect both instances.
        session.cancel(op_id);
        let snapshot = tokio::time::timeout(Duration::from_secs(2), refresh_task)
            .await
            .expect("refresh must not hang once the blocking operation is cancelled")
            .expect("refresh task panicked");
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "wget"));
    }

    /// Backing store for `fake_now`. `Session::with_adapters` takes a plain
    /// `fn() -> i64`, which cannot capture state, so the clock the expiry
    /// test winds forward has to live in a static. Only
    /// `test_submit_rejects_a_plan_issued_more_than_600s_ago` reads or
    /// writes it, so the parallel test threads never race on it.
    static FAKE_NOW: AtomicI64 = AtomicI64::new(0);

    fn fake_now() -> i64 {
        FAKE_NOW.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_submit_of_a_never_issued_plan_id_is_unknown_and_runs_nothing() {
        // F1 / spec §6: the only thing a client can send `submit` is an id,
        // and an id this Session never handed out must be rejected outright
        // — it must not start anything, whatever number it is.
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        // Nothing has been issued yet, so every id is a forgery.
        assert_eq!(session.submit(1), Err(SubmitError::Unknown));
        assert_eq!(session.submit(u64::MAX), Err(SubmitError::Unknown));

        // With exactly one plan issued (id 1), its neighbours are still
        // forgeries and the real id is untouched by those failed attempts.
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(issued.id, 1);
        assert_eq!(session.submit(0), Err(SubmitError::Unknown));
        assert_eq!(session.submit(2), Err(SubmitError::Unknown));
        assert!(
            session.operations().is_empty(),
            "a rejected submit must never reach the OperationManager"
        );
        session
            .submit(issued.id)
            .expect("the genuinely issued id is still submittable after the forgeries failed");
        assert_eq!(session.operations().len(), 1);
    }

    #[tokio::test]
    async fn test_submit_consumes_the_plan_so_the_same_id_cannot_be_replayed() {
        // The single-use guarantee: one issue_plan yields at most one
        // operation. Submitting the same id a second time is treated exactly
        // like an id that was never issued.
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
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

        let op_id = session
            .submit(issued.id)
            .expect("first submit of a freshly issued plan");
        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::Unknown),
            "an issued plan is single-use: replaying its id must be rejected"
        );
        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::Unknown),
            "and it stays rejected however many times it is replayed"
        );
        let ops = session.operations();
        assert_eq!(
            ops.len(),
            1,
            "exactly one operation may result from one issued plan"
        );
        assert_eq!(ops[0].id, op_id);

        // Previewing the same request again is a new plan under a new id,
        // which is itself submittable exactly once more.
        let reissued = session.issue_plan(&req).await.expect("issue_plan again");
        assert_ne!(reissued.id, issued.id);
        session
            .submit(reissued.id)
            .expect("a re-issued plan is submittable once");
        assert_eq!(session.submit(reissued.id), Err(SubmitError::Unknown));
        assert_eq!(session.operations().len(), 2);
    }

    #[tokio::test]
    async fn test_submit_rejects_a_plan_issued_more_than_600s_ago() {
        // Pins `issued_at` through the `now_fn` seam, then winds the same
        // clock forward to prove the 600 s limit is enforced on submit —
        // inclusive at exactly 600 s ("more than 600 seconds ago" is the
        // documented contract), exclusive one second later.
        const T0: i64 = 1_758_000_000;
        FAKE_NOW.store(T0, Ordering::SeqCst);
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(fake_now));
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let on_time = session.issue_plan(&req).await.expect("issue_plan");
        let too_late = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(
            on_time.issued_at, T0,
            "issued_at must come from the injected clock"
        );
        assert_eq!(too_late.issued_at, T0);

        // Exactly 600 s old is not "more than 600 seconds ago": still valid.
        FAKE_NOW.store(T0 + 600, Ordering::SeqCst);
        session
            .submit(on_time.id)
            .expect("a plan exactly 600 s old is still submittable");
        assert_eq!(session.operations().len(), 1);

        // One second past the limit: expired, and nothing is submitted.
        FAKE_NOW.store(T0 + 601, Ordering::SeqCst);
        assert_eq!(session.submit(too_late.id), Err(SubmitError::Expired));
        assert_eq!(
            session.operations().len(),
            1,
            "an expired plan must never reach the OperationManager"
        );

        // The failed submit discarded the expired plan rather than leaving
        // it around for a retry: even winding the clock back cannot
        // resurrect it, and the caller has to issue_plan again.
        FAKE_NOW.store(T0, Ordering::SeqCst);
        assert_eq!(session.submit(too_late.id), Err(SubmitError::Unknown));
        assert_eq!(session.operations().len(), 1);
        let fresh = session.issue_plan(&req).await.expect("issue_plan again");
        assert_ne!(fresh.id, too_late.id);
        session
            .submit(fresh.id)
            .expect("a freshly issued plan is submittable");
        assert_eq!(session.operations().len(), 2);
    }
}
