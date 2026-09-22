//! `Session::refresh`: detect every registered adapter's instances, then
//! fetch inventory + updates per instance, merging failures into `errors`
//! and `stale` without ever aborting the whole refresh. Split out of
//! `session/mod.rs` (Task 14); no behaviour change from what shipped there.

use super::{DetectOutcome, Session, Snapshot, SourceError};
use crate::adapters::CheckOptions;
use crate::model::{InstanceNote, ManagerInstance, ResourceLock};
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
    /// `generation`. An instance whose `status.unavailable` is set is
    /// skipped by the per-instance fetch -- that is a *reported state*, not
    /// a failed refresh (Task 11) -- but it keeps the previous round's
    /// artifacts and updates, so the "here is what Canager saw last time"
    /// copy its notice carries is true rather than a promise over an empty
    /// group.
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

        // No root check here: it used to short-circuit the entire refresh
        // to `RefusedAsRoot` behind `BrewAdapter::refuses_as_root(env)`, but
        // that predicate is nothing but `env.euid == 0` -- a fact about
        // *brew*, not about npm, pipx, uv, pip, cargo or ollama, none of
        // which object to root. A root user with no Homebrew objection saw
        // every one of the other six adapters disabled and was told
        // Homebrew refused to run, whether or not Homebrew was even
        // installed. The decision now lives solely in
        // `BrewAdapter::detect`, which under root contributes an instance
        // marked `Unavailable::RefusesAsRoot` -- so the refusal is reported
        // against brew alone, with the action that resolves it, instead of
        // silently disabling the other six. Every adapter, brew included,
        // is simply fanned out to below like any other refresh.
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

        // Seeded before the fan-out because a skipped instance contributes
        // its carried-forward rows from inside the loop below.
        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            // Task 11: a source that already told us it is not answering is
            // a reported state, not a failed refresh, so it is never fanned
            // out to. It stays in `snapshot.instances` so the UI can render
            // its notice and offer to start it -- and it keeps whatever it
            // reported last time, exactly as the error paths below already
            // do. Dropping those rows is what made the unreachable notice's
            // "below is what Canager saw last time" a lie: a stopped Ollama
            // rendered a group header, that sentence, and no rows at all.
            // `issue_plan`'s gate (spec §2.5) is what stops those rows
            // offering an Uninstall button that could not possibly work.
            if inst.status.unavailable.is_some() {
                artifacts.extend(
                    previous
                        .artifacts
                        .iter()
                        .filter(|a| a.key.instance_id == inst.id)
                        .cloned(),
                );
                updates.extend(
                    previous
                        .updates
                        .iter()
                        .filter(|u| u.key.instance_id == inst.id)
                        .cloned(),
                );
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
                    // What this source said about *itself* while checking,
                    // as opposed to about any one package: `brew update`
                    // failing means "no updates" may simply be wrong, and
                    // no package row can carry that. The join below merges
                    // these back into `instances` by id, because
                    // `instances` was cloned *before* this fan-out --
                    // writing a note onto `inst` here would write it onto
                    // the clone and lose it, the same trap `SourceError`'s
                    // `instance_id` already fell into once.
                    let mut notes: Vec<InstanceNote> = Vec::new();
                    match adapter.check_updates(&inst, &opts).await {
                        Ok(outcome) => {
                            updates.extend(outcome.candidates);
                            notes.extend(outcome.notes);
                        }
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
                    (artifacts, updates, errors, stale, notes)
                }),
            ));
        }

        let mut errors = detect_errors;
        // Exactly "a refresh attempt failed", which is all any reader does
        // with it: `SnapshotStatus` turns it into the one page-wide "some
        // of this may be out of date, try again" banner, over a count of
        // `errors`.
        //
        // It briefly also meant "or some source is unavailable". Nothing
        // could observe that half -- the banner's own condition ruled it
        // out -- and widening it for its own sake would have been worse
        // than useless: an unavailable source already says so itself, in
        // its own words, with its own action, on both pages, through
        // `sourceNoticesFor`. A second page-wide banner saying something
        // vaguer about the same fact is noise, and its copy ("the last
        // refresh couldn't finish for {count} sources") would have been
        // false with a count of zero.
        let mut stale = !errors.is_empty();
        for (instance_id, handle) in handles {
            match handle.await {
                Ok((a, u, e, s, notes)) => {
                    artifacts.extend(a);
                    updates.extend(u);
                    errors.extend(e);
                    stale = stale || s;
                    merge_instance_notes(&mut instances, &instance_id, notes);
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

        // Stamped because a refresh *ran*, not because it came back
        // perfect. Gated on `stale`, a Mac with one permanently unavailable
        // source carried `refreshed_at: None` for the rest of its life, and
        // `SnapshotStatus` reads a null timestamp as "Canager has never
        // finished a check" -- six healthy sources' worth of real data
        // described as no data at all. What "some of this may be old" means
        // is `stale`, and that is the flag that carries it.
        // `Snapshot::empty()` still has `refreshed_at: None`, which is what
        // keeps the startup branch in `SnapshotStatus` working: nothing but
        // an uncommitted snapshot can have a null timestamp now.
        let refreshed_at = Some(self.now());
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

/// Merge notes a fan-out task produced back onto the instance it belongs
/// to, matched by id.
///
/// `refresh` clones `instances` before spawning, so every per-instance task
/// holds its own copy and anything it learns about the source has to travel
/// back by id or be lost. An id with no matching instance is dropped on
/// purpose: instances only ever shrink between the clone and the join if
/// something removed one, and inventing an entry for it would put a source
/// in the snapshot that no `detect()` reported.
fn merge_instance_notes(
    instances: &mut [ManagerInstance],
    instance_id: &str,
    notes: Vec<InstanceNote>,
) {
    if notes.is_empty() {
        return;
    }
    if let Some(inst) = instances.iter_mut().find(|i| i.id == instance_id) {
        inst.status.notes.extend(notes);
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::brew::BrewAdapter;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceId, InstanceNote,
        ManagerInstance, OpKind, OpRequest, OpStatus, Outcome, Plan, Reconciled, SearchHit,
        Unavailable, UpdateCandidate, UpdateChannel,
    };
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use crate::session::test_support::{make_instance, non_root_env, root_env};
    use crate::session::{DetectOutcome, Session, Snapshot};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct FakeState {
        instances: Vec<ManagerInstance>,
        artifacts: HashMap<InstanceId, Vec<InstalledArtifact>>,
        updates: HashMap<InstanceId, Vec<UpdateCandidate>>,
        /// What `check_updates` reports *about the source itself*, as
        /// opposed to about a package -- the `CheckOutcome.notes` channel
        /// brew fills when `brew update` failed.
        notes: HashMap<InstanceId, Vec<InstanceNote>>,
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
                notes: HashMap::new(),
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
        ) -> Result<CheckOutcome, AdapterError> {
            let s = self.state.lock().unwrap();
            Ok(CheckOutcome {
                candidates: s.updates.get(&inst.id).cloned().unwrap_or_default(),
                notes: s.notes.get(&inst.id).cloned().unwrap_or_default(),
            })
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

    /// The root decision lives entirely in `BrewAdapter::detect` now (spec:
    /// one adapter's root policy must not disable the other six). Session
    /// no longer special-cases root at all -- it just runs every adapter's
    /// `detect()` concurrently, exactly as for any other host state, and
    /// brew alone comes back unavailable. A real `BrewAdapter` is used
    /// (over a `MockRunner`, the same pattern `adapters/brew/mod.rs`'s own
    /// detect tests use) rather than a second `FakeAdapter`, because the
    /// whole point under test is brew's *own* root check, not a stand-in
    /// for it.
    #[tokio::test]
    async fn test_refresh_as_root_reports_brew_unavailable_and_leaves_the_others_alone() {
        let runner = Arc::new(MockRunner::new());
        // Real Homebrew is assumed installed at /opt/homebrew, as
        // `adapters/brew/mod.rs`'s own detect tests already assume for
        // CI's macos-latest runners; this response would only be used if
        // brew's root refusal failed to stop `detect` short of asking the
        // runner for a version at all.
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 7.0.3\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let brew = Arc::new(BrewAdapter::new(runner));
        let (fake, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![brew, fake], None);

        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;

        assert_eq!(
            snapshot.detect,
            DetectOutcome::Found,
            "the fake adapter still found an instance, so this is an ordinary Found, not a whole-app refusal"
        );
        let brew_instance = snapshot
            .instances
            .iter()
            .find(|i| i.adapter_id == "brew")
            .expect("brew is installed here, and saying otherwise is the whole bug");
        assert_eq!(
            brew_instance.status.unavailable,
            Some(crate::model::Unavailable::RefusesAsRoot),
            "brew is listed with its reason, not dropped"
        );
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:1"),
            "an adapter with no root objection must detect normally, got {:?}",
            snapshot.instances
        );
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
    }

    /// When brew is the *only* registered adapter, running as root is
    /// still not a whole-app state -- but it is not "nothing detected"
    /// either. `DetectOutcome::Missing` is what `SnapshotStatus` renders
    /// as "none of them are set up on this Mac yet", which on a Mac with
    /// Homebrew installed is simply false; the source is `Found`, and
    /// unavailable with a reason that names what to do.
    #[tokio::test]
    async fn test_refresh_as_root_with_only_brew_registered_still_finds_it() {
        let runner = Arc::new(MockRunner::new());
        let brew = Arc::new(BrewAdapter::new(runner));
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![brew], None);

        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;

        assert_eq!(snapshot.detect, DetectOutcome::Found);
        assert_eq!(snapshot.instances.len(), 1, "got {:?}", snapshot.instances);
        assert_eq!(
            snapshot.instances[0].status.unavailable,
            Some(crate::model::Unavailable::RefusesAsRoot)
        );
        assert!(
            snapshot.errors.is_empty(),
            "a source that reported why it cannot answer is not a refresh error"
        );
        assert!(
            snapshot.refreshed_at.is_some(),
            "a refresh that ran is stamped even when every source came back empty"
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
        // A pinned, *moving* clock: the second refresh must be able to
        // stamp a different timestamp from the first, which a real
        // wall clock only does if the two land either side of a second.
        static NOW: AtomicI64 = AtomicI64::new(1_700_000_000);
        let session = Session::with_adapters(
            sink,
            vec![adapter],
            Some(|| NOW.fetch_add(100, Ordering::SeqCst)),
        );
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
        assert!(
            second.refreshed_at > first_refreshed_at,
            "a refresh that ran is stamped even when a source failed (spec §2.4-1): \
             {:?} should be newer than {:?}",
            second.refreshed_at,
            first_refreshed_at
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
    async fn test_an_unavailable_instance_is_a_reported_state_not_a_failed_refresh() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:up"),
                crate::testing::unavailable_instance("fake", "fake:down", Unavailable::NotRunning),
            ];
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
            "a refresh that ran has a timestamp, whatever the sources said"
        );
        assert!(
            snapshot.errors.is_empty(),
            "a source that reported it is not running is not a refresh error"
        );
        assert!(
            !snapshot.stale,
            "a source that said why it cannot answer is not a failed refresh: it says so \
             itself, on both pages, through its own notice. `stale` means the refresh \
             attempt failed, which is the only thing any reader of it does with it"
        );
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:down"),
            "the unavailable instance stays in the snapshot so the UI can offer to start it"
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

    fn make_update(instance_id: &str, name: &str) -> UpdateCandidate {
        UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: name.to_string(),
            },
            current: "1.0".to_string(),
            target: "1.1".to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_a_source_that_stops_answering_keeps_its_rows_but_offers_no_operations_on_them() {
        // Spec §2.4-3 and §2.5 together, and neither half is optional. The
        // notice a stopped source renders says "below is what Canager saw
        // last time"; before the carry-forward that sentence sat above an
        // empty group. With the rows back, every one of them would sprout
        // an Uninstall button that cannot possibly work -- so the gate in
        // `issue_plan` is what the second half of this test pins down.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.updates
                .insert("fake:1".to_string(), vec![make_update("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(first.artifacts.len(), 1, "precondition: one artifact known");
        assert_eq!(first.updates.len(), 1, "precondition: one update known");

        {
            let mut s = state.lock().unwrap();
            s.instances = vec![crate::testing::unavailable_instance(
                "fake",
                "fake:1",
                Unavailable::NotRunning,
            )];
            s.inventory_calls.clear();
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(
            second.artifacts.iter().any(|a| a.key.name == "jq"),
            "an unavailable source keeps the artifacts it reported last time, got {:?}",
            second.artifacts
        );
        assert!(
            second.updates.iter().any(|u| u.key.name == "jq"),
            "and the updates too, got {:?}",
            second.updates
        );
        assert!(
            !state
                .lock()
                .unwrap()
                .inventory_calls
                .contains(&"fake:1".to_string()),
            "carrying data forward must not mean asking the dead source again"
        );

        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        match session.issue_plan(&req).await {
            Err(crate::adapters::AdapterError::NotActionable {
                read_only,
                unavailable,
            }) => {
                assert_eq!(read_only, None);
                assert_eq!(unavailable, Some(Unavailable::NotRunning));
            }
            other => panic!("a carried-forward row must offer no Uninstall, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_refresh_stamps_refreshed_at_even_when_a_source_failed() {
        // Spec §2.4-1. Gated on `stale`, one permanently broken source left
        // `refreshed_at` null for the life of the machine, and a null
        // timestamp is how `SnapshotStatus` recognises "nothing has ever
        // been checked" -- six good sources described as no data at all.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.failing.push("fake:1".to_string());
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(|| 1_700_000_000));

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(snapshot.stale, "precondition: the fetch failed");
        assert_eq!(snapshot.errors.len(), 1, "precondition: and said so");
        assert_eq!(
            snapshot.refreshed_at,
            Some(1_700_000_000),
            "a refresh that ran is stamped; `stale` is what says the data may be old"
        );
    }

    #[test]
    fn test_snapshot_empty_still_has_no_timestamp() {
        // The one snapshot that may carry `refreshed_at: None` now that
        // every completed refresh stamps one. `SnapshotStatus`'s startup
        // branch reads exactly this to tell "still loading" from "checked
        // and found nothing".
        assert_eq!(Snapshot::empty().refreshed_at, None);
    }

    #[tokio::test]
    async fn test_a_note_a_source_reports_while_checking_reaches_the_snapshot() {
        // The whole point of `CheckOutcome`: `brew update` failing is a
        // fact about the *source*, not about any one package, and the only
        // place the user can be told is the source's own notice. The
        // instance the fan-out task holds is a clone (`refresh` clones
        // `instances` before spawning), so a note that is not merged back
        // by id is silently lost -- the snapshot the UI renders would carry
        // an empty `notes` and the Updates page would go back to saying
        // "Everything is up to date" over a catalogue it could not
        // download.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.notes
                .insert("fake:1".to_string(), vec![InstanceNote::IndexMayBeStale]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        let noted = snapshot
            .instances
            .iter()
            .find(|i| i.id == "fake:1")
            .expect("the instance that reported the note is still in the snapshot");
        assert_eq!(
            noted.status.notes,
            vec![InstanceNote::IndexMayBeStale],
            "a note from `check_updates` has to travel back to its instance by id"
        );
        let quiet = snapshot
            .instances
            .iter()
            .find(|i| i.id == "fake:2")
            .expect("the other instance is still there");
        assert!(
            quiet.status.notes.is_empty(),
            "and only to that instance, got {:?}",
            quiet.status.notes
        );
        assert!(
            snapshot.errors.is_empty(),
            "a note is not an error: the source answered, its answer just has a caveat"
        );
    }

    #[test]
    fn test_merge_instance_notes_merges_by_id_and_ignores_the_rest() {
        // `instances` is cloned before the fan-out, so a note a spawned
        // task produced has to travel back by id or be lost -- the same
        // trap `SourceError`'s `instance_id` fell into once already.
        let mut instances = vec![
            crate::testing::manager_instance("fake", "fake:1"),
            crate::testing::manager_instance("fake", "fake:2"),
        ];
        super::merge_instance_notes(
            &mut instances,
            "fake:2",
            vec![InstanceNote::IndexMayBeStale],
        );
        assert!(instances[0].status.notes.is_empty());
        assert_eq!(
            instances[1].status.notes,
            vec![InstanceNote::IndexMayBeStale]
        );

        super::merge_instance_notes(&mut instances, "fake:2", vec![]);
        assert_eq!(
            instances[1].status.notes,
            vec![InstanceNote::IndexMayBeStale],
            "an empty batch of notes changes nothing"
        );
        super::merge_instance_notes(
            &mut instances,
            "fake:gone",
            vec![InstanceNote::IndexMayBeStale],
        );
        assert!(
            instances.iter().all(|i| i.id != "fake:gone"),
            "a note for an instance nobody detected must not invent one"
        );
    }
}
