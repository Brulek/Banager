//! `Session::refresh`: detect every registered adapter's instances, then
//! fetch inventory + updates per instance, merging failures into `errors`
//! and `stale` without ever aborting the whole refresh. Split out of
//! `session/mod.rs` (Task 14); no behaviour change from what shipped there.

use super::{DetectOutcome, Session, Snapshot, SourceError};
use crate::adapters::brew::BrewAdapter;
use crate::adapters::CheckOptions;
use crate::model::ResourceLock;
use crate::runner::HostEnv;
use std::sync::atomic::Ordering;

impl Session {
    /// Detect every registered adapter's instances concurrently (Task 11: a
    /// slow or failing adapter's `detect()` must not delay any other
    /// adapter's), then inventory + check updates for each resulting
    /// instance concurrently (each under that instance's own resource lock
    /// -- see below). Bumps `generation` only when the resulting data
    /// actually differs from the previous snapshot. Per-instance and
    /// per-adapter failures land in `errors` and set `stale`; they never
    /// abort the whole refresh, and a failing instance's *previous*
    /// artifacts/updates are kept rather than dropped, so a transient
    /// failure never makes something the user installed appear to vanish.
    /// Concurrent calls are serialised: a call that starts while another is
    /// already running waits for it, then returns the snapshot that other
    /// call produced instead of running a second, redundant refresh -- see
    /// `refresh_seq` on `Session` for why that check cannot use
    /// `generation`. An instance the adapter reported as `healthy: false` is
    /// skipped by the per-instance fetch: that is a *reported state*, not a
    /// failed refresh (Task 11).
    pub async fn refresh(
        self: &std::sync::Arc<Self>,
        env: &HostEnv,
        opts: &CheckOptions,
    ) -> Snapshot {
        let seq_before = self.refresh_seq.load(Ordering::SeqCst);
        let _gate = self.refresh_gate.lock().await;
        if self.refresh_seq.load(Ordering::SeqCst) != seq_before {
            return self.snapshot.lock().unwrap().clone();
        }

        let previous = self.snapshot.lock().unwrap().clone();
        // Owned copy (CheckOptions is Copy): each per-instance spawned task
        // below needs its own 'static value, and the caller's `&opts`
        // reference cannot outlive this function.
        let opts: CheckOptions = *opts;

        if BrewAdapter::refuses_as_root(env) {
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

        let mut detect_handles = Vec::with_capacity(self.adapters.len());
        for adapter in self.adapters.values() {
            // Cloned into the task because `tokio::spawn` needs a 'static
            // future: iterating `values()` by reference would tie it to
            // `&self`. (Written as an explicit clone rather than
            // `.values().cloned()` only because clippy's
            // `unnecessary_to_owned` misreads the latter here.)
            let adapter = adapter.clone();
            let env = env.clone();
            detect_handles.push((
                adapter.meta().id.clone(),
                tokio::spawn(async move { adapter.detect(&env).await }),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, handle) in detect_handles {
            match handle.await {
                Ok(found) => instances.extend(found),
                Err(_join_err) => {
                    detect_errors.push(SourceError {
                        instance_id: adapter_id,
                        message: "internal error detecting this source".to_string(),
                    });
                }
            }
        }
        for inst in &instances {
            self.ops.register_instance(inst.clone());
        }
        let detect = if instances.is_empty() {
            DetectOutcome::Missing
        } else {
            DetectOutcome::Found
        };

        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            // Task 11: a source the adapter already reported as not running
            // is a reported state, not a failed refresh. Fanning out to it
            // would push a SourceError and set `stale`, which carries
            // `refreshed_at` forward instead of stamping it -- leaving the
            // snapshot permanently stale on a machine where, say, Ollama is
            // installed but not running. It stays in `snapshot.instances` so
            // the UI can render its notice and offer to start it.
            if !inst.healthy {
                continue;
            }
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
        let mut errors = detect_errors;
        let mut stale = !errors.is_empty();
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
    /// moved (M5 in the design review -- see `refresh_seq`'s field doc).
    fn commit(&self, previous: Snapshot, mut candidate: Snapshot) -> Snapshot {
        if !previous.same_content(&candidate) {
            candidate.generation = previous.generation + 1;
        }
        *self.snapshot.lock().unwrap() = candidate.clone();
        self.refresh_seq.fetch_add(1, Ordering::SeqCst);
        candidate
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKind, InstallReason, InstalledArtifact, InstanceId, ManagerInstance, OpKind,
        OpRequest, OpStatus, Outcome, Plan, Reconciled, SearchHit, UpdateCandidate,
    };
    use crate::runner::HostEnv;
    use crate::session::test_support::{make_instance, non_root_env, root_env};
    use crate::session::{DetectOutcome, Session};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct FakeState {
        instances: Vec<ManagerInstance>,
        artifacts: HashMap<InstanceId, Vec<InstalledArtifact>>,
        updates: HashMap<InstanceId, Vec<UpdateCandidate>>,
        failing: Vec<InstanceId>,
        detect_delay: Duration,
        detect_calls: usize,
        block_execute: bool,
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
                meta: crate::session::test_support::fake_adapter_meta(id),
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
            Ok(crate::session::test_support::fake_plan(inst, req))
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
            Ok(crate::session::test_support::fake_reconciled())
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
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let calls_before = state.lock().unwrap().detect_calls;

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
    async fn test_refresh_detects_across_adapters_concurrently_so_a_slow_source_does_not_block_others(
    ) {
        let (slow_a, state_a) = FakeAdapter::new("slow-a");
        let (slow_b, state_b) = FakeAdapter::new("slow-b");
        state_a.lock().unwrap().detect_delay = Duration::from_millis(200);
        state_a.lock().unwrap().instances = vec![make_instance("slow-a", "slow-a:1")];
        state_b.lock().unwrap().detect_delay = Duration::from_millis(200);
        state_b.lock().unwrap().instances = vec![make_instance("slow-b", "slow-b:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![slow_a, slow_b], None);

        let started = Instant::now();
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let elapsed = started.elapsed();

        assert!(snapshot.instances.iter().any(|i| i.id == "slow-a:1"));
        assert!(snapshot.instances.iter().any(|i| i.id == "slow-b:1"));
        assert!(
            elapsed < Duration::from_millis(350),
            "two 200ms detects must overlap, not run back to back (took {elapsed:?})"
        );
    }

    #[tokio::test]
    async fn test_an_unhealthy_instance_is_a_reported_state_not_a_failed_refresh() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            let mut down = make_instance("fake", "fake:down");
            down.healthy = false;
            s.instances = vec![make_instance("fake", "fake:up"), down];
            s.artifacts
                .insert("fake:up".to_string(), vec![make_artifact("fake:up", "jq")]);
            s.failing.push("fake:down".to_string());
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(
            snapshot.refreshed_at.is_some(),
            "a refresh whose only complaint is a source known not to be running has completed"
        );
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty());
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:down"),
            "the unhealthy instance stays in the snapshot so the UI can offer to start it"
        );
        assert!(
            !state
                .lock()
                .unwrap()
                .inventory_calls
                .contains(&"fake:down".to_string()),
            "an instance reported as not running must never be inventoried"
        );
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
    }

    #[tokio::test]
    async fn test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others(
    ) {
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

        session.cancel(op_id);
        let snapshot = tokio::time::timeout(Duration::from_secs(2), refresh_task)
            .await
            .expect("refresh must not hang once the blocking operation is cancelled")
            .expect("refresh task panicked");
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "wget"));
    }
}
