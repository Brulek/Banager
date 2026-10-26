//! The Discover page's search of Homebrew (phase 5 step E): what Homebrew
//! has whose name or description has a text in it, each result kept here
//! under an id of its own, so that Install names a result, never a package
//! (phase 5 proposal §3.4), as Install on a catalog entry names the entry.

use super::{IssuedPlan, Session};
use crate::adapters::AdapterError;
use crate::model::{ArtifactKind, OpKind, OpRequest};
use serde::Serialize;
use std::time::{Duration, Instant};

/// How long a result can still be installed by its id: as long as a plan
/// stays submittable (`plans::PLAN_LIFETIME`). After that, search again.
pub(crate) const FOUND_LIFETIME: Duration = Duration::from_secs(600);

/// How many results are kept at most; past it, the oldest go first. A
/// search gives at most `brew::SEARCH_HITS_MAX`, so this is several
/// searches' worth.
pub(crate) const FOUND_KEPT: usize = 400;

/// What installs a result, kept under its id.
pub(crate) struct FoundEntry {
    request: OpRequest,
    found_at: Instant,
}

/// One result as the window lists it (`search_homebrew` in
/// src-tauri/src/ipc.rs; `Found` in src/lib/types.ts).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Found {
    /// What Install sends back (`Session::plan_found`): random, and only
    /// good for `FOUND_LIFETIME`.
    pub id: String,
    pub kind: ArtifactKind,
    pub name: String,
    /// Homebrew's description, where its description search gave one.
    pub description: Option<String>,
    /// The Homebrew it would install into lists it installed now.
    pub installed: bool,
    /// The catalog entry this formula or cask installs, where it is one
    /// (`catalog::id_installing`): the row then says it is one of the
    /// suggestions.
    pub catalog_id: Option<&'static str>,
}

/// Why a search gave nothing to list.
#[derive(Debug)]
pub enum SearchError {
    /// No Homebrew that answered and can be written to is there.
    NoHomebrew,
    /// The text was refused (`brew::validate_search_query`), or Homebrew
    /// could not search.
    Adapter(AdapterError),
}

/// Why a result's install was not planned.
#[derive(Debug)]
pub enum FoundError {
    /// No result has this id, or it is older than `FOUND_LIFETIME`.
    Unknown,
    /// The check since lists it installed.
    Installed,
    /// The plan was refused (`Session::issue_plan`).
    Adapter(AdapterError),
}

impl Session {
    /// Homebrew's formulae and casks whose name or description has
    /// `query` in it (`BrewAdapter::search`), from the first Homebrew of the
    /// snapshot that answered and can be written to -- the one a catalog
    /// entry installs into -- each kept under a fresh id for
    /// `plan_found`. Every search also forgets the results older than
    /// `FOUND_LIFETIME`.
    pub async fn search_homebrew(&self, query: &str) -> Result<Vec<Found>, SearchError> {
        let query = query.trim();
        crate::adapters::brew::validate_search_query(query).map_err(SearchError::Adapter)?;
        let instance = self
            .snapshot
            .lock()
            .unwrap()
            .instances
            .iter()
            .find(|instance| crate::catalog::homebrew_can_install(instance))
            .cloned()
            .ok_or(SearchError::NoHomebrew)?;
        let adapter = self
            .adapters
            .get(&instance.adapter_id)
            .ok_or(SearchError::NoHomebrew)?;
        let hits = adapter
            .search(&instance, query)
            .await
            .map_err(SearchError::Adapter)?;
        let artifacts = self.snapshot.lock().unwrap().artifacts.clone();
        let mut found = self.found.lock().unwrap();
        found.retain(|_, entry| entry.found_at.elapsed() <= FOUND_LIFETIME);
        let now = Instant::now();
        let results: Vec<Found> = hits
            .into_iter()
            .map(|hit| {
                let id = super::plans::random_plan_id();
                let installed = artifacts.iter().any(|artifact| {
                    artifact.key.instance_id == instance.id
                        && artifact.key.kind == hit.kind
                        && artifact.key.name == hit.name
                });
                found.insert(
                    id.clone(),
                    FoundEntry {
                        request: OpRequest {
                            kind: OpKind::Install,
                            instance_id: instance.id.clone(),
                            artifact_kind: hit.kind,
                            name: hit.name.clone(),
                        },
                        found_at: now,
                    },
                );
                Found {
                    id,
                    kind: hit.kind,
                    catalog_id: crate::catalog::id_installing(hit.kind, &hit.name),
                    name: hit.name,
                    description: hit.description,
                    installed,
                }
            })
            .collect();
        while found.len() > FOUND_KEPT {
            let oldest = found
                .iter()
                .min_by_key(|(_, entry)| entry.found_at)
                .map(|(id, _)| id.clone());
            match oldest {
                Some(id) => found.remove(&id),
                None => break,
            };
        }
        Ok(results)
    }

    /// The install of the result `id` (`search_homebrew`), planned as every
    /// operation is (`issue_plan`): its formula or cask, by the name
    /// Homebrew gave, into the Homebrew it was found in. Refused before
    /// any adapter is asked where no result has that id any more, and where
    /// the snapshot now lists it installed there.
    pub async fn plan_found(&self, id: &str) -> Result<IssuedPlan, FoundError> {
        let request = {
            let mut found = self.found.lock().unwrap();
            found.retain(|_, entry| entry.found_at.elapsed() <= FOUND_LIFETIME);
            found.get(id).map(|entry| entry.request.clone())
        }
        .ok_or(FoundError::Unknown)?;
        let installed = self
            .snapshot
            .lock()
            .unwrap()
            .artifacts
            .iter()
            .any(|artifact| {
                artifact.key.instance_id == request.instance_id
                    && artifact.key.kind == request.artifact_kind
                    && artifact.key.name == request.name
            });
        if installed {
            return Err(FoundError::Installed);
        }
        self.issue_plan(&request).await.map_err(FoundError::Adapter)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;
    use crate::adapters::{Adapter, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, InstalledArtifact, ManagerInstance, Outcome, Plan, Reconciled, SearchHit,
    };
    use crate::runner::HostEnv;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    /// A Homebrew with jq installed, whose search finds what it is given.
    struct FakeBrew {
        meta: AdapterMeta,
        instance: ManagerInstance,
        hits: Vec<SearchHit>,
        searched: Mutex<Vec<String>>,
        plans: AtomicUsize,
    }

    #[async_trait]
    impl Adapter for FakeBrew {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            vec![self.instance.clone()]
        }

        async fn inventory(
            &self,
            inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            Ok(vec![crate::testing::installed_artifact(
                &inst.id,
                ArtifactKind::Formula,
                "jq",
            )])
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
            query: &str,
        ) -> Result<Vec<SearchHit>, AdapterError> {
            self.searched.lock().unwrap().push(query.to_string());
            Ok(self.hits.clone())
        }

        async fn plan(
            &self,
            inst: &ManagerInstance,
            req: &OpRequest,
        ) -> Result<Plan, AdapterError> {
            self.plans.fetch_add(1, Ordering::SeqCst);
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

    fn hit(kind: ArtifactKind, name: &str) -> SearchHit {
        SearchHit {
            adapter_id: "brew".to_string(),
            kind,
            name: name.to_string(),
            description: Some(format!("{name}, described")),
        }
    }

    async fn session_with(instance: ManagerInstance) -> (Arc<Session>, Arc<FakeBrew>) {
        let brew = Arc::new(FakeBrew {
            meta: test_support::fake_adapter_meta("brew"),
            instance,
            hits: vec![
                hit(ArtifactKind::Formula, "jq"),
                hit(ArtifactKind::Formula, "jq-lsp"),
                hit(ArtifactKind::Cask, "claude-code"),
            ],
            searched: Mutex::new(Vec::new()),
            plans: AtomicUsize::new(0),
        });
        let adapter: Arc<dyn Adapter> = brew.clone();
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        (session, brew)
    }

    #[tokio::test]
    async fn test_a_search_lists_each_result_under_an_id_of_its_own_and_says_what_is_installed() {
        let (session, brew) = session_with(crate::testing::manager_instance(
            "brew",
            "brew:/opt/homebrew",
        ))
        .await;
        let found = session.search_homebrew("  jq ").await.expect("search");
        assert_eq!(*brew.searched.lock().unwrap(), ["jq"], "searched trimmed");
        let named: Vec<(&str, bool, Option<&str>)> = found
            .iter()
            .map(|f| (f.name.as_str(), f.installed, f.catalog_id))
            .collect();
        assert_eq!(
            named,
            [
                ("jq", true, None),
                ("jq-lsp", false, None),
                ("claude-code", false, Some("claude-code")),
            ]
        );
        assert_eq!(found[2].kind, ArtifactKind::Cask);
        assert_eq!(found[0].description.as_deref(), Some("jq, described"));
        // Ids: random, 32 hex characters, all different.
        assert!(found.iter().all(|f| f.id.len() == 32));
        assert_ne!(found[0].id, found[1].id);
        assert_eq!(
            serde_json::to_value(&found[2]).unwrap(),
            serde_json::json!({
                "id": found[2].id,
                "kind": "Cask",
                "name": "claude-code",
                "description": "claude-code, described",
                "installed": false,
                "catalog_id": "claude-code"
            })
        );

        // Install plans that result, by its id, and nothing else.
        let issued = session.plan_found(&found[1].id).await.expect("plan");
        assert_eq!(
            issued.plan.request,
            OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/opt/homebrew".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq-lsp".to_string(),
            }
        );
        assert_eq!(brew.plans.load(Ordering::SeqCst), 1);
        // Installed already: refused, as is an id no search gave.
        assert!(matches!(
            session.plan_found(&found[0].id).await,
            Err(FoundError::Installed)
        ));
        for id in ["", "jq-lsp", "00000000000000000000000000000000"] {
            assert!(
                matches!(session.plan_found(id).await, Err(FoundError::Unknown)),
                "{id:?}"
            );
        }
        assert_eq!(brew.plans.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_a_search_needs_a_homebrew_that_can_install_and_a_text_brew_may_be_given() {
        let view_only = crate::testing::read_only_instance(
            "brew",
            "brew:/opt/homebrew",
            crate::model::ReadOnlyReason::PrefixNotWritable,
        );
        let (session, brew) = session_with(view_only).await;
        assert!(matches!(
            session.search_homebrew("jq").await,
            Err(SearchError::NoHomebrew)
        ));
        let (session, _) = session_with(crate::testing::manager_instance(
            "brew",
            "brew:/opt/homebrew",
        ))
        .await;
        for query in ["", "--macports", "a/b"] {
            assert!(
                matches!(
                    session.search_homebrew(query).await,
                    Err(SearchError::Adapter(AdapterError::InvalidName(_)))
                ),
                "{query:?}"
            );
        }
        assert!(brew.searched.lock().unwrap().is_empty());
    }
}
