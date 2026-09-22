//! `Session::issue_plan` and `Session::submit`: preview-then-confirm for a
//! destructive operation. Split out of `session/mod.rs` (Task 14); no
//! behaviour change from what shipped there.

use super::{IssuedPlan, PlanId, Session, SubmitError};
use crate::adapters::AdapterError;
use crate::events::OpId;
use crate::model::OpRequest;

/// A fresh, unpredictable `PlanId`: 16 bytes from the OS CSPRNG, hex-encoded
/// to 32 characters. Replaces the previous sequential `AtomicU64` counter,
/// whose next value for a plan the user had just been issued was always
/// exactly one guess away.
///
/// `getrandom::fill`'s only failure mode is the OS RNG itself being
/// unavailable, which is not a condition this process can recover from or
/// meaningfully report through `AdapterError` -- every caller of
/// `issue_plan` is mid-`await` on a plan it already asked for, with nowhere
/// sensible to route "the operating system cannot hand out random bytes".
fn random_plan_id() -> PlanId {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS RNG must be available to issue a PlanId");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What `issue_plan` actually stores and `submit` consumes: the
/// `IssuedPlan` the caller previewed, plus the bookkeeping that decides
/// whether it may still run.
///
/// `generation` is the `Snapshot::generation` the plan was built against,
/// read *before* `adapter.plan()` is awaited. Reading it afterwards would
/// stamp a plan built against instance state that a refresh had already
/// replaced during the await with the number of the snapshot that replaced
/// it, which is precisely the reading `submit` uses to decide nothing has
/// changed.
///
/// It is deliberately not a field on `IssuedPlan`: that type crosses the
/// IPC boundary to the front end, which neither needs this nor may
/// influence it.
pub(super) struct StoredPlan {
    pub(super) issued: IssuedPlan,
    pub(super) generation: u64,
}

impl Session {
    /// Resolves `req` to its owning adapter, asks it to plan the operation,
    /// then stores the resulting `Plan` under a fresh `PlanId` and returns
    /// both as an `IssuedPlan`. The caller previews `issued.plan`; nothing
    /// in it is ever accepted back -- `submit` takes only `issued.id`. Every
    /// call also sweeps any entry in `issued_plans` older than 600 seconds
    /// (Task 13), so a plan the operator previewed and then never submitted
    /// does not sit in the map forever.
    pub async fn issue_plan(&self, req: &OpRequest) -> Result<IssuedPlan, AdapterError> {
        // Generation and instance are read under one lock, so the number
        // stored below really is the generation this exact instance came
        // from. Reading them separately would let a refresh land in
        // between and stamp the plan with a generation belonging to a
        // different instance.
        let (generation, instance) = {
            let snapshot = self.snapshot.lock().unwrap();
            (
                snapshot.generation,
                snapshot
                    .instances
                    .iter()
                    .find(|i| i.id == req.instance_id)
                    .cloned(),
            )
        };
        let instance = instance.ok_or_else(|| {
            AdapterError::Refused(format!("unknown instance {}", req.instance_id))
        })?;
        // The actionability gate (spec §2.5), both halves: an operation may
        // be offered only when
        // `read_only_reason.is_none() && status.unavailable.is_none()`.
        //
        // The two axes are independent and both are load-bearing. A stopped
        // Ollama is perfectly *writable* -- nothing about its permissions
        // changed -- and since `refresh` now carries an unavailable
        // source's previous artifacts forward rather than dropping them,
        // those rows are on screen. Without the availability half each of
        // them would carry an Uninstall button whose `ollama rm` cannot
        // possibly succeed against a daemon that is not listening: the
        // "offer it, then refuse it" bug this phase exists to remove,
        // grown back inside its own fix.
        //
        // It lives here rather than in seven `plan()` implementations
        // because this is the one point every operation in the workspace
        // passes through, and because five-adapter verbatim duplication is
        // exactly what the last review round flagged twice. npm's own
        // `Refused` check stays regardless: detect and the click are
        // seconds to hours apart and permissions change in between.
        if !instance.writable() || !instance.available() {
            return Err(AdapterError::NotActionable {
                read_only: instance.read_only_reason,
                unavailable: instance.status.unavailable,
            });
        }
        let adapter = self.adapters.get(&instance.adapter_id).ok_or_else(|| {
            AdapterError::Refused(format!("no adapter registered for {}", instance.adapter_id))
        })?;
        let plan = adapter.plan(&instance, req).await?;
        let id = random_plan_id();
        let issued_at = self.now();
        let issued = IssuedPlan {
            id: id.clone(),
            plan,
            issued_at,
        };
        let mut plans = self.issued_plans.lock().unwrap();
        plans.retain(|_, p| issued_at - p.issued.issued_at <= 600);
        plans.insert(
            id,
            StoredPlan {
                issued: issued.clone(),
                generation,
            },
        );
        Ok(issued)
    }

    /// Removes (one-time consumption) the issued plan stored under
    /// `plan_id` and submits exactly that stored `Plan`. Fails with
    /// `SubmitError::Unknown` if `plan_id` was never issued, was already
    /// submitted once, or was already swept out by a later `issue_plan`
    /// call, and `SubmitError::Expired` if it is still present but was
    /// issued more than 600 seconds ago -- the client can never influence
    /// what actually runs, since nothing it sends is used except this
    /// opaque id.
    ///
    /// It also re-runs `issue_plan`'s actionability gate (spec §2.5)
    /// before handing anything to the `OperationManager`, because
    /// `issue_plan` alone cannot enforce an invariant about the *current*
    /// state of a source: a plan issued in generation N can be submitted
    /// after generation N+1 has made its instance read-only, unavailable
    /// or absent, and everything the gate refuses to offer would then be
    /// reachable simply by having previewed it first. The user is clicking
    /// a button on a row rendered from a snapshot that no longer exists.
    ///
    /// The re-check is skipped when `generation` has not moved since the
    /// plan was issued, and only then: an unchanged generation means
    /// `Snapshot::same_content` held on every commit in between, so the
    /// instance the gate already passed is byte-for-byte the instance that
    /// would be re-resolved. When it has moved, the instance is looked up
    /// again and re-tested rather than the plan being rejected outright --
    /// the generation is global and this invariant is per instance, so
    /// rejecting on any change would invalidate a preview the user is
    /// reading because some unrelated source gained a package.
    ///
    /// This is the last gate `Session` owns, not the last gate there
    /// should be: the operation still queues behind its resource lock, and
    /// anything that changes between here and the adapter actually running
    /// belongs to `OperationManager` and the adapter's own checks (npm's
    /// `Refused` is the existing example).
    pub fn submit(self: &std::sync::Arc<Self>, plan_id: PlanId) -> Result<OpId, SubmitError> {
        let stored = {
            let mut plans = self.issued_plans.lock().unwrap();
            plans.remove(&plan_id).ok_or(SubmitError::Unknown)?
        };
        if self.now() - stored.issued.issued_at > 600 {
            return Err(SubmitError::Expired);
        }
        self.recheck_actionable(&stored)?;
        Ok(self.ops.submit(stored.issued.plan))
    }

    /// `issue_plan`'s gate, re-asked of the snapshot that is current now.
    /// See `submit` for why this exists and why an unchanged generation is
    /// allowed to skip it.
    fn recheck_actionable(&self, stored: &StoredPlan) -> Result<(), SubmitError> {
        let snapshot = self.snapshot.lock().unwrap();
        if snapshot.generation == stored.generation {
            return Ok(());
        }
        // The id the plan was *planned for*, not one the client sent:
        // `submit` takes nothing but an opaque token, and every adapter's
        // `plan()` echoes the request it was given back into `Plan`.
        let instance_id = &stored.issued.plan.request.instance_id;
        let instance = snapshot
            .instances
            .iter()
            .find(|i| &i.id == instance_id)
            .ok_or(SubmitError::SourceGone)?;
        if !instance.writable() || !instance.available() {
            return Err(SubmitError::NotActionable {
                read_only: instance.read_only_reason,
                unavailable: instance.status.unavailable,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstalledArtifact, ManagerInstance, OpKind, OpRequest, Outcome,
        Plan, ReadOnlyReason, Reconciled, SearchHit, Unavailable,
    };
    use crate::runner::HostEnv;
    use crate::session::test_support;
    use crate::session::{Session, SubmitError};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    /// Lets a test suspend `FakeAdapter::plan` exactly where the real
    /// thing suspends -- inside the `await` in `issue_plan`, after the
    /// actionability gate has passed and before anything is stored. The
    /// adapter announces on `entered` that it is in there, then waits on
    /// `resume`, so the test can commit a whole refresh in between and
    /// reproduce the issuance race deterministically rather than by
    /// sleeping and hoping.
    struct PlanGate {
        entered: tokio::sync::oneshot::Sender<()>,
        resume: tokio::sync::oneshot::Receiver<()>,
    }

    struct FakeAdapter {
        meta: AdapterMeta,
        instances: std::sync::Mutex<Vec<ManagerInstance>>,
        plan_gate: std::sync::Mutex<Option<PlanGate>>,
    }

    impl FakeAdapter {
        fn new(instances: Vec<ManagerInstance>) -> Arc<FakeAdapter> {
            Arc::new(FakeAdapter {
                meta: test_support::fake_adapter_meta("fake"),
                instances: std::sync::Mutex::new(instances),
                plan_gate: std::sync::Mutex::new(None),
            })
        }

        /// What the next `detect()` reports -- the host changing under a
        /// plan the user is still looking at.
        fn set_instances(&self, instances: Vec<ManagerInstance>) {
            *self.instances.lock().unwrap() = instances;
        }

        /// Arms the gate for the next `plan()` call. Returns the handle
        /// that resolves once `plan()` has been entered, and the sender
        /// that lets it finish.
        fn gate_next_plan(
            &self,
        ) -> (
            tokio::sync::oneshot::Receiver<()>,
            tokio::sync::oneshot::Sender<()>,
        ) {
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
            let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
            *self.plan_gate.lock().unwrap() = Some(PlanGate {
                entered: entered_tx,
                resume: resume_rx,
            });
            (entered_rx, resume_tx)
        }
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.instances.lock().unwrap().clone()
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

        async fn plan(
            &self,
            inst: &ManagerInstance,
            req: &OpRequest,
        ) -> Result<Plan, AdapterError> {
            // Taken out from under the lock before awaiting: holding a
            // std::sync guard across an await point is exactly what
            // clippy's `await_holding_lock` is for.
            let gate = self.plan_gate.lock().unwrap().take();
            if let Some(gate) = gate {
                let _ = gate.entered.send(());
                let _ = gate.resume.await;
            }
            Ok(test_support::fake_plan(inst, req))
        }

        async fn execute(
            &self,
            _plan: &Plan,
            _sink: Arc<dyn EventSink>,
            _op_id: OpId,
            _cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
            Ok(Outcome::Succeeded)
        }

        async fn reconcile(
            &self,
            _inst: &ManagerInstance,
            _key: &ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(test_support::fake_reconciled())
        }
    }

    fn install_request(instance_id: &str) -> OpRequest {
        OpRequest {
            kind: OpKind::Install,
            instance_id: instance_id.to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        }
    }

    #[tokio::test]
    async fn test_issue_plan_delegates_to_the_owning_adapter() {
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let req = install_request("fake:1");
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(
            issued.id.len(),
            32,
            "a PlanId is a random 128-bit token, 32 hex chars: {}",
            issued.id
        );
        assert!(
            issued.id.chars().all(|c| c.is_ascii_hexdigit()),
            "a PlanId is hex-encoded: {}",
            issued.id
        );
        assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);
    }

    #[tokio::test]
    async fn test_issue_plan_ids_are_random_tokens_not_a_guessable_sequence() {
        // A plan the user previewed and declined used to have a guessable
        // next-in-sequence id (`AtomicU64` starting at 1) that
        // `submit_operation` would still fire. This does not defend against
        // a fully compromised renderer -- which can call `plan_operation`
        // itself and read the id it was handed back -- but it does mean
        // nothing short of that can fire a *declined* preview by guessing.
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let req = install_request("fake:1");

        let mut ids = Vec::new();
        for _ in 0..8 {
            ids.push(session.issue_plan(&req).await.expect("issue_plan").id);
        }

        let unique: std::collections::HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "every issued id must be distinct: {ids:?}"
        );
        assert!(
            ids.iter()
                .all(|id| id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())),
            "every id must be a 32-char hex token: {ids:?}"
        );
        // The old scheme produced "1", "2", "3", ...; guard against a
        // regression to anything sequence-like by checking these do not
        // sort into the order they were issued in.
        let mut sorted = ids.clone();
        sorted.sort();
        assert_ne!(
            sorted, ids,
            "ids must not come back in issuance order -- that is what a sequence looks like: {ids:?}"
        );
    }

    #[tokio::test]
    async fn test_issue_plan_refuses_every_operation_on_a_read_only_instance() {
        // The actionability gate (spec §2.5). `FakeAdapter::plan` happily
        // plans anything, so a refusal here can only have come from the
        // gate in `issue_plan` -- which is the point: the invariant holds
        // for every adapter, including the six whose own `plan()` has no
        // writability check of its own.
        for reason in [ReadOnlyReason::ByDesign, ReadOnlyReason::PrefixNotWritable] {
            let adapter = FakeAdapter::new(vec![test_support::make_read_only_instance(
                "fake", "fake:1", reason,
            )]);
            let sink = Arc::new(VecSink::new());
            let session = Session::with_adapters(sink, vec![adapter], None);
            session
                .refresh(&test_support::non_root_env(), &CheckOptions::default())
                .await;

            for kind in [OpKind::Install, OpKind::Uninstall, OpKind::Upgrade] {
                let req = OpRequest {
                    kind,
                    instance_id: "fake:1".to_string(),
                    artifact_kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                };
                match session.issue_plan(&req).await {
                    Err(AdapterError::NotActionable {
                        read_only,
                        unavailable,
                    }) => {
                        assert_eq!(read_only, Some(reason));
                        assert_eq!(unavailable, None);
                    }
                    other => {
                        panic!("expected NotActionable for {reason:?}/{kind:?}, got {other:?}")
                    }
                }
            }
            assert!(
                session.operations().is_empty(),
                "a refused plan must never reach the OperationManager"
            );
        }
    }

    #[tokio::test]
    async fn test_issue_plan_refuses_every_operation_on_an_unavailable_instance() {
        // The other half of the same gate (spec §2.5), and the reason it
        // has to be a conjunction: this instance is perfectly *writable*.
        // Nothing about a stopped Ollama's permissions changed, so a
        // writability-only gate lets every row `refresh` carried forward
        // from it keep an Uninstall button that cannot possibly succeed.
        for unavailable in [Unavailable::NotRunning, Unavailable::NotResponding] {
            let instance = test_support::make_unavailable_instance("fake", "fake:1", unavailable);
            assert!(
                instance.writable(),
                "precondition: unavailability is not a permissions problem"
            );
            let adapter = FakeAdapter::new(vec![instance]);
            let sink = Arc::new(VecSink::new());
            let session = Session::with_adapters(sink, vec![adapter], None);
            session
                .refresh(&test_support::non_root_env(), &CheckOptions::default())
                .await;

            for kind in [OpKind::Install, OpKind::Uninstall, OpKind::Upgrade] {
                let req = OpRequest {
                    kind,
                    instance_id: "fake:1".to_string(),
                    artifact_kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                };
                match session.issue_plan(&req).await {
                    Err(AdapterError::NotActionable {
                        read_only,
                        unavailable: got_unavailable,
                    }) => {
                        assert_eq!(read_only, None);
                        assert_eq!(got_unavailable, Some(unavailable));
                    }
                    other => {
                        panic!("expected NotActionable for {unavailable:?}/{kind:?}, got {other:?}")
                    }
                }
            }
            assert!(
                session.operations().is_empty(),
                "a refused plan must never reach the OperationManager"
            );
        }
    }

    #[tokio::test]
    async fn test_issue_plan_for_unknown_instance_is_refused() {
        let adapter = FakeAdapter::new(vec![]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let req = install_request("does-not-exist");
        match session.issue_plan(&req).await {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_submit_of_a_never_issued_plan_id_is_unknown_and_runs_nothing() {
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;

        assert_eq!(session.submit("1".to_string()), Err(SubmitError::Unknown));
        assert_eq!(session.submit("f".repeat(32)), Err(SubmitError::Unknown));

        let req = install_request("fake:1");
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(issued.id.len(), 32, "a PlanId is a random 128-bit token");
        assert_eq!(session.submit("0".to_string()), Err(SubmitError::Unknown));
        assert_eq!(session.submit("2".to_string()), Err(SubmitError::Unknown));
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
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let req = install_request("fake:1");
        let issued = session.issue_plan(&req).await.expect("issue_plan");

        let op_id = session
            .submit(issued.id.clone())
            .expect("first submit of a freshly issued plan");
        assert_eq!(
            session.submit(issued.id.clone()),
            Err(SubmitError::Unknown),
            "an issued plan is single-use: replaying its id must be rejected"
        );
        assert_eq!(
            session.submit(issued.id.clone()),
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

        let reissued = session.issue_plan(&req).await.expect("issue_plan again");
        assert_ne!(reissued.id, issued.id);
        session
            .submit(reissued.id.clone())
            .expect("a re-issued plan is submittable once");
        assert_eq!(session.submit(reissued.id), Err(SubmitError::Unknown));
        assert_eq!(session.operations().len(), 2);
    }

    /// Refreshes `session` and asserts the refresh moved the generation,
    /// so a test that means "the world changed under this plan" cannot
    /// quietly stop testing that.
    async fn refresh_and_expect_a_new_generation(session: &Arc<Session>, before: u64) -> u64 {
        let snapshot = session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        assert!(
            snapshot.generation > before,
            "precondition: this refresh must move the generation ({} -> {})",
            before,
            snapshot.generation
        );
        snapshot.generation
    }

    #[tokio::test]
    async fn test_submit_is_refused_once_the_source_stopped_answering() {
        // The gate in `issue_plan` only ever looked at the snapshot that
        // was current when the preview was *issued*. A plan for a source
        // that has since stopped answering used to submit anyway: the
        // invariant this phase exists to establish -- an operation may be
        // offered only when its instance is writable and available -- was
        // bypassed by the plan simply being old enough to predate the bad
        // news, and young enough not to have expired.
        for unavailable in [Unavailable::NotRunning, Unavailable::NotResponding] {
            let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
            let sink = Arc::new(VecSink::new());
            let session = Session::with_adapters(sink, vec![adapter.clone()], None);
            let generation = session
                .refresh(&test_support::non_root_env(), &CheckOptions::default())
                .await
                .generation;
            let issued = session
                .issue_plan(&install_request("fake:1"))
                .await
                .expect("issue_plan while the source was answering");

            adapter.set_instances(vec![test_support::make_unavailable_instance(
                "fake",
                "fake:1",
                unavailable,
            )]);
            refresh_and_expect_a_new_generation(&session, generation).await;

            assert_eq!(
                session.submit(issued.id),
                Err(SubmitError::NotActionable {
                    read_only: None,
                    unavailable: Some(unavailable),
                }),
            );
            assert!(
                session.operations().is_empty(),
                "a plan whose source stopped answering must never reach the OperationManager"
            );
        }
    }

    #[tokio::test]
    async fn test_submit_is_refused_once_the_source_became_read_only() {
        // The other half of the same gate, re-checked at the same point:
        // npm's prefix can stop being writable between the preview and
        // the click, and `issue_plan`'s verdict is then simply out of date.
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter.clone()], None);
        let generation = session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await
            .generation;
        let issued = session
            .issue_plan(&install_request("fake:1"))
            .await
            .expect("issue_plan while the source was writable");

        adapter.set_instances(vec![test_support::make_read_only_instance(
            "fake",
            "fake:1",
            ReadOnlyReason::PrefixNotWritable,
        )]);
        refresh_and_expect_a_new_generation(&session, generation).await;

        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::NotActionable {
                read_only: Some(ReadOnlyReason::PrefixNotWritable),
                unavailable: None,
            }),
        );
        assert!(session.operations().is_empty());
    }

    #[tokio::test]
    async fn test_submit_is_refused_once_the_source_is_gone_from_the_snapshot() {
        // `issue_plan` refuses an instance it cannot find; submitting a
        // plan for one that has disappeared since has to refuse for the
        // same reason, and say so differently from "not actionable" --
        // there is no instance left to carry a reason.
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter.clone()], None);
        let generation = session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await
            .generation;
        let issued = session
            .issue_plan(&install_request("fake:1"))
            .await
            .expect("issue_plan while the source was there");

        adapter.set_instances(vec![]);
        refresh_and_expect_a_new_generation(&session, generation).await;

        assert_eq!(session.submit(issued.id), Err(SubmitError::SourceGone));
        assert!(session.operations().is_empty());
    }

    #[tokio::test]
    async fn test_submit_survives_a_refresh_that_changed_something_else() {
        // The deliberate other side of the rule. Rejecting on any
        // generation change at all would throw away a preview the user is
        // reading because an unrelated source gained a package -- the
        // generation is global, the invariant is per instance. What
        // matters is whether *this* instance still passes the gate.
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter.clone()], None);
        let generation = session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await
            .generation;
        let issued = session
            .issue_plan(&install_request("fake:1"))
            .await
            .expect("issue_plan");

        adapter.set_instances(vec![
            test_support::make_instance("fake", "fake:1"),
            test_support::make_instance("fake", "fake:2"),
        ]);
        refresh_and_expect_a_new_generation(&session, generation).await;

        session
            .submit(issued.id)
            .expect("a refresh that left this instance alone must not invalidate its preview");
        assert_eq!(session.operations().len(), 1);
    }

    #[tokio::test]
    async fn test_a_refresh_that_lands_while_planning_is_still_caught_at_submit() {
        // The issuance race, with the interleaving pinned rather than
        // slept for:
        //   1. `issue_plan` passes the gate against generation N and
        //      suspends inside `adapter.plan()`;
        //   2. a refresh commits generation N+1, in which the instance is
        //      no longer answering;
        //   3. planning resumes and stores a plan stamped *now*.
        // Capturing the generation after the await would stamp this plan
        // with N+1 -- the very snapshot that invalidated it -- and submit
        // would wave it through.
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter.clone()], None);
        let generation = session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await
            .generation;

        let (entered, resume) = adapter.gate_next_plan();
        let planning_session = session.clone();
        let planning = tokio::spawn(async move {
            planning_session
                .issue_plan(&install_request("fake:1"))
                .await
        });
        entered.await.expect("plan() must be entered");

        adapter.set_instances(vec![test_support::make_unavailable_instance(
            "fake",
            "fake:1",
            Unavailable::NotRunning,
        )]);
        refresh_and_expect_a_new_generation(&session, generation).await;

        resume.send(()).expect("plan() is still waiting");
        let issued = planning
            .await
            .expect("planning task")
            .expect("planning itself succeeds: it was gated, not failed");

        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::NotActionable {
                read_only: None,
                unavailable: Some(Unavailable::NotRunning),
            }),
            "a plan built against a snapshot that was replaced while it was being built \
             must not be submittable"
        );
        assert!(session.operations().is_empty());
    }

    static FAKE_NOW: AtomicI64 = AtomicI64::new(0);

    fn fake_now() -> i64 {
        FAKE_NOW.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_submit_rejects_a_plan_issued_more_than_600s_ago() {
        const T0: i64 = 1_758_000_000;
        FAKE_NOW.store(T0, Ordering::SeqCst);
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(fake_now));
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let req = install_request("fake:1");
        let on_time = session.issue_plan(&req).await.expect("issue_plan");
        let too_late = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(
            on_time.issued_at, T0,
            "issued_at must come from the injected clock"
        );
        assert_eq!(too_late.issued_at, T0);

        FAKE_NOW.store(T0 + 600, Ordering::SeqCst);
        session
            .submit(on_time.id)
            .expect("a plan exactly 600 s old is still submittable");
        assert_eq!(session.operations().len(), 1);

        FAKE_NOW.store(T0 + 601, Ordering::SeqCst);
        assert_eq!(
            session.submit(too_late.id.clone()),
            Err(SubmitError::Expired)
        );
        assert_eq!(
            session.operations().len(),
            1,
            "an expired plan must never reach the OperationManager"
        );

        FAKE_NOW.store(T0, Ordering::SeqCst);
        assert_eq!(
            session.submit(too_late.id.clone()),
            Err(SubmitError::Unknown)
        );
        assert_eq!(session.operations().len(), 1);
        let fresh = session.issue_plan(&req).await.expect("issue_plan again");
        assert_ne!(fresh.id, too_late.id);
        session
            .submit(fresh.id)
            .expect("a freshly issued plan is submittable");
        assert_eq!(session.operations().len(), 2);
    }

    static SWEEP_TEST_NOW: AtomicI64 = AtomicI64::new(2_000_000_000);

    fn sweep_test_now() -> i64 {
        SWEEP_TEST_NOW.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_issue_plan_sweeps_previously_expired_entries_so_the_map_does_not_grow_unbounded()
    {
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(sweep_test_now));
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let req = install_request("fake:1");

        let stale = session.issue_plan(&req).await.expect("issue_plan (stale)");
        SWEEP_TEST_NOW.fetch_add(601, Ordering::SeqCst);
        let _fresh = session
            .issue_plan(&req)
            .await
            .expect("issue_plan (fresh, triggers sweep)");

        assert_eq!(
            session.submit(stale.id),
            Err(SubmitError::Unknown),
            "a swept entry must read back as Unknown, not Expired"
        );
    }
}
