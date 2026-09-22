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

impl Session {
    /// Resolves `req` to its owning adapter, asks it to plan the operation,
    /// then stores the resulting `Plan` under a fresh `PlanId` and returns
    /// both as an `IssuedPlan`. The caller previews `issued.plan`; nothing
    /// in it is ever accepted back -- `submit` takes only `issued.id`. Every
    /// call also sweeps any entry in `issued_plans` older than 600 seconds
    /// (Task 13), so a plan the operator previewed and then never submitted
    /// does not sit in the map forever.
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
            return Err(AdapterError::Refused(format!(
                "{} is not something Canager can act on (read-only: {:?}, unavailable: {:?})",
                instance.id, instance.read_only_reason, instance.status.unavailable
            )));
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
        plans.retain(|_, p| issued_at - p.issued_at <= 600);
        plans.insert(id, issued.clone());
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
    pub fn submit(self: &std::sync::Arc<Self>, plan_id: PlanId) -> Result<OpId, SubmitError> {
        let issued = {
            let mut plans = self.issued_plans.lock().unwrap();
            plans.remove(&plan_id).ok_or(SubmitError::Unknown)?
        };
        if self.now() - issued.issued_at > 600 {
            return Err(SubmitError::Expired);
        }
        Ok(self.ops.submit(issued.plan))
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

    struct FakeAdapter {
        meta: AdapterMeta,
        instances: Vec<ManagerInstance>,
    }

    impl FakeAdapter {
        fn new(instances: Vec<ManagerInstance>) -> Arc<FakeAdapter> {
            Arc::new(FakeAdapter {
                meta: test_support::fake_adapter_meta("fake"),
                instances,
            })
        }
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.instances.clone()
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
                    Err(AdapterError::Refused(message)) => {
                        assert!(
                            message.contains("fake:1"),
                            "the refusal must name the instance: {message}"
                        );
                    }
                    other => panic!("expected Refused for {reason:?}/{kind:?}, got {other:?}"),
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
                    Err(AdapterError::Refused(message)) => {
                        assert!(
                            message.contains("fake:1"),
                            "the refusal must name the instance: {message}"
                        );
                    }
                    other => {
                        panic!("expected Refused for {unavailable:?}/{kind:?}, got {other:?}")
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
