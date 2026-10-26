//! The Discover page's search of Homebrew (phase 5 step E): what Homebrew
//! lists for a text -- names with it in them, aliases, close spellings --
//! each result kept here
//! under an id of its own, so that Install names a result, never a package
//! (phase 5 proposal §3.4), as Install on a catalog entry names the entry.

use super::{IssuedPlan, Session};
use crate::adapters::AdapterError;
use crate::model::{ArtifactKind, Chip, HitNeeds, OpKind, OpRequest};
use serde::Serialize;
use std::time::{Duration, Instant};

/// How long a result can still be installed by its id: as long as a plan
/// stays submittable (`plans::PLAN_LIFETIME`). After that, search again.
pub(crate) const FOUND_LIFETIME: Duration = Duration::from_secs(600);

/// How many results are kept at most; past it, the oldest go first. A
/// search gives at most `brew::SEARCH_HITS_MAX`, so this is fifty
/// searches' worth: more than ten minutes of typing.
pub(crate) const FOUND_KEPT: usize = 2000;

/// How many searches run at once; the others wait their turn. Each runs
/// three `brew search` commands, up to `CANONICAL_ASKS_MAX` `brew info`
/// asks of the formulae and one of the casks, one after another
/// (`BrewAdapter::search`), and the window can ask
/// for as many searches as it likes (from the independent review of the
/// branch).
pub(crate) const SEARCHES_AT_ONCE: usize = 2;

/// What installs a result, kept under its id.
pub(crate) struct FoundEntry {
    request: OpRequest,
    blocked: Option<FoundBlock>,
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
    /// Homebrew's description: its description search's, or for a formula
    /// it gave none for, `brew info`'s (`BrewAdapter::search`).
    pub description: Option<String>,
    /// The Homebrew it would install into lists it installed now.
    pub installed: bool,
    /// The catalog entry this formula or cask installs, where it is one
    /// (`catalog::id_installing`): the row then says it is one of the
    /// suggestions.
    pub catalog_id: Option<&'static str>,
    /// Why Homebrew would refuse to install it on this Mac, from what its
    /// own `brew info` says it needs (`blocked_on`); the row then says so
    /// in place of Install, and `plan_found` refuses it.
    pub blocked: Option<FoundBlock>,
}

/// Why Homebrew would refuse to install a result on this Mac, before
/// downloading anything (from the review of the areas earlier reviews
/// left: the row offered Install, and the run ended in Homebrew's own
/// refusal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FoundBlock {
    /// Something is already where it puts its app, put there another way:
    /// Homebrew would download it, then stop rather than replace it.
    AppThere,
    /// Homebrew disabled it, and installs it no more.
    Disabled,
    /// It installs on this macOS major version or later only.
    NeedsMacOS { major: u32 },
    /// It installs on the other kind of chip only.
    OtherChip,
}

/// What of `needs` this Mac does not meet, the first of them: its app
/// already there, then disabled, then macOS, then the chip. Nothing where
/// this Mac's macOS could not be read, as Homebrew's own refusal then
/// still says it.
pub(crate) fn blocked_on(needs: &HitNeeds, host: &crate::catalog::Host) -> Option<FoundBlock> {
    if needs.app_there {
        return Some(FoundBlock::AppThere);
    }
    if needs.disabled {
        return Some(FoundBlock::Disabled);
    }
    if let (Some(major), Some(here)) = (needs.macos, host.macos_major) {
        if here < major {
            return Some(FoundBlock::NeedsMacOS { major });
        }
    }
    match needs.chip {
        Some(Chip::AppleSilicon) if !host.apple_silicon => Some(FoundBlock::OtherChip),
        Some(Chip::Intel) if host.apple_silicon => Some(FoundBlock::OtherChip),
        _ => None,
    }
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
    /// It is a suggestion whose catalog entry is not "Not installed" on
    /// this Mac: installed another way (Claude Code from npm, Node.js as
    /// `node@22`), or one this Mac cannot install (Ollama before macOS 14).
    State(crate::catalog::CatalogState),
    /// Homebrew would refuse it on this Mac (`Found::blocked`).
    Blocked(FoundBlock),
    /// The plan was refused (`Session::issue_plan`).
    Adapter(AdapterError),
}

impl Session {
    /// What Homebrew's search lists for `query` among its formulae and
    /// casks (`BrewAdapter::search`), from the first Homebrew of the
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
        // Its turn among the searches under way (`SEARCHES_AT_ONCE`). The
        // semaphore is never closed.
        let Ok(_turn) = self.searching.acquire().await else {
            return Err(SearchError::NoHomebrew);
        };
        let hits = adapter
            .search(&instance, query)
            .await
            .map_err(SearchError::Adapter)?;
        let artifacts = self.snapshot.lock().unwrap().artifacts.clone();
        let mut host = crate::catalog::Host::of_this_mac();
        // Its chip as this Homebrew runs: one in /usr/local runs as Intel,
        // under Rosetta on Apple silicon, and refuses an Apple silicon
        // cask (`brew::bottles::this_mac`; from the review of the check).
        if instance.prefix == std::path::Path::new("/usr/local") {
            host.apple_silicon = false;
        }
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
                // An app Homebrew put there itself is no other one in the
                // way: once uninstalled, the result installs again (from
                // the review of the check).
                let needs = HitNeeds {
                    app_there: hit.needs.app_there && !installed,
                    ..hit.needs.clone()
                };
                let blocked = blocked_on(&needs, &host);
                found.insert(
                    id.clone(),
                    FoundEntry {
                        request: OpRequest {
                            kind: OpKind::Install,
                            instance_id: instance.id.clone(),
                            artifact_kind: hit.kind,
                            name: hit.name.clone(),
                        },
                        blocked,
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
                    blocked,
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
    /// any adapter is asked where no result has that id any more, where
    /// the snapshot now lists it installed there, and, for a suggestion,
    /// where its catalog entry is not "Not installed" (`FoundError::State`),
    /// and where Homebrew would refuse it on this Mac (`FoundError::Blocked`).
    pub async fn plan_found(&self, id: &str) -> Result<IssuedPlan, FoundError> {
        let (request, blocked) = {
            let mut found = self.found.lock().unwrap();
            found.retain(|_, entry| entry.found_at.elapsed() <= FOUND_LIFETIME);
            found
                .get(id)
                .map(|entry| (entry.request.clone(), entry.blocked))
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
        if let Some(block) = blocked {
            return Err(FoundError::Blocked(block));
        }
        // A suggestion, found by searching, is installed only where its
        // entry would be: not a second copy of a tool the Mac has another
        // way, nor one this Mac cannot install (`catalog::install_request`).
        let state = crate::catalog::state_installing(
            &self.snapshot(),
            &crate::catalog::Host::of_this_mac(),
            request.artifact_kind,
            &request.name,
        );
        if let Some(state) =
            state.filter(|state| *state != crate::catalog::CatalogState::NotInstalled)
        {
            return Err(FoundError::State(state));
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
        /// What it lists installed.
        listed: Vec<(ArtifactKind, &'static str)>,
        hits: Mutex<Vec<SearchHit>>,
        searched: Mutex<Vec<String>>,
        plans: AtomicUsize,
        /// How long each search takes, and how many were under way at once
        /// at most: for the test of `SEARCHES_AT_ONCE`.
        search_delay: Duration,
        searching: AtomicUsize,
        most_searching: AtomicUsize,
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
            Ok(self
                .listed
                .iter()
                .map(|(kind, name)| crate::testing::installed_artifact(&inst.id, *kind, name))
                .collect())
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
            let now = self.searching.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_searching.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.search_delay).await;
            self.searching.fetch_sub(1, Ordering::SeqCst);
            Ok(self.hits.lock().unwrap().clone())
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
            needs: Default::default(),
        }
    }

    async fn session_with(instance: ManagerInstance) -> (Arc<Session>, Arc<FakeBrew>) {
        session_listing(instance, vec![(ArtifactKind::Formula, "jq")]).await
    }

    async fn session_listing(
        instance: ManagerInstance,
        listed: Vec<(ArtifactKind, &'static str)>,
    ) -> (Arc<Session>, Arc<FakeBrew>) {
        let brew = Arc::new(FakeBrew {
            meta: test_support::fake_adapter_meta("brew"),
            instance,
            listed,
            hits: Mutex::new(vec![
                hit(ArtifactKind::Formula, "jq"),
                hit(ArtifactKind::Formula, "jq-lsp"),
                hit(ArtifactKind::Cask, "claude-code"),
            ]),
            searched: Mutex::new(Vec::new()),
            plans: AtomicUsize::new(0),
            search_delay: Duration::ZERO,
            searching: AtomicUsize::new(0),
            most_searching: AtomicUsize::new(0),
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
                "catalog_id": "claude-code",
                "blocked": null
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
        // claude-code is a suggestion: where the Mac has Claude Code another
        // way, it is not installed again (here, the fake's jq is no such
        // thing, so the cask is still planned).
        assert!(session.plan_found(&found[2].id).await.is_ok());
        for id in ["", "jq-lsp", "00000000000000000000000000000000"] {
            assert!(
                matches!(session.plan_found(id).await, Err(FoundError::Unknown)),
                "{id:?}"
            );
        }
        assert_eq!(brew.plans.load(Ordering::SeqCst), 2);
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
        assert!(brew.searched.lock().unwrap().is_empty());
        // Refused before the Homebrew that could search is asked (from the
        // independent review of the branch: this read the view-only one's).
        let (session, brew) = session_with(crate::testing::manager_instance(
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

    #[tokio::test]
    async fn test_a_result_older_than_its_lifetime_is_unknown() {
        let (session, brew) = session_with(crate::testing::manager_instance(
            "brew",
            "brew:/opt/homebrew",
        ))
        .await;
        let found = session.search_homebrew("jq").await.expect("search");
        let past = Instant::now()
            .checked_sub(FOUND_LIFETIME + Duration::from_secs(1))
            .expect("an instant that long ago");
        for entry in session.found.lock().unwrap().values_mut() {
            entry.found_at = past;
        }
        assert!(matches!(
            session.plan_found(&found[1].id).await,
            Err(FoundError::Unknown)
        ));
        assert_eq!(brew.plans.load(Ordering::SeqCst), 0);
        // And a search forgets those past their lifetime, of its own accord:
        // expired ones put in after plan_found, which forgot the others
        // (from the review of the fix batch: the search's own sweep was
        // untested).
        {
            let mut kept = session.found.lock().unwrap();
            for n in 0..3 {
                kept.insert(
                    format!("expired-{n}"),
                    FoundEntry {
                        request: OpRequest {
                            kind: OpKind::Install,
                            instance_id: "brew:/opt/homebrew".to_string(),
                            artifact_kind: ArtifactKind::Formula,
                            name: format!("expired{n}"),
                        },
                        blocked: None,
                        found_at: past,
                    },
                );
            }
        }
        session.search_homebrew("jq").await.expect("search");
        let kept = session.found.lock().unwrap();
        assert!(!kept.keys().any(|id| id.starts_with("expired-")));
        assert_eq!(kept.len(), found.len());
    }

    #[tokio::test]
    async fn test_a_result_homebrew_would_refuse_says_why_and_is_not_planned() {
        // From the review of the areas earlier reviews left: a disabled
        // cask was offered Install, and the run ended in Homebrew's refusal.
        let (session, brew) = session_with(crate::testing::manager_instance(
            "brew",
            "brew:/opt/homebrew",
        ))
        .await;
        let mut alacritty = hit(ArtifactKind::Cask, "alacritty");
        alacritty.needs.disabled = true;
        *brew.hits.lock().unwrap() = vec![alacritty, hit(ArtifactKind::Formula, "jq-lsp")];
        let found = session.search_homebrew("ala").await.expect("search");
        assert_eq!(found[0].blocked, Some(FoundBlock::Disabled));
        assert_eq!(found[1].blocked, None);
        assert_eq!(
            serde_json::to_value(found[0].blocked).unwrap(),
            serde_json::json!("Disabled")
        );
        assert!(matches!(
            session.plan_found(&found[0].id).await,
            Err(FoundError::Blocked(FoundBlock::Disabled))
        ));
        assert_eq!(brew.plans.load(Ordering::SeqCst), 0);
        session.plan_found(&found[1].id).await.expect("plan");
    }

    #[tokio::test]
    async fn test_an_app_homebrew_installed_itself_does_not_block_its_own_cask() {
        // From the review of the check: the cask's own app, in
        // /Applications because Homebrew put it there, blocked the result,
        // and after an uninstall the row still said "Already on this Mac".
        let (session, brew) = session_listing(
            crate::testing::manager_instance("brew", "brew:/opt/homebrew"),
            vec![(ArtifactKind::Cask, "iterm2")],
        )
        .await;
        let mut iterm = hit(ArtifactKind::Cask, "iterm2");
        iterm.needs.app_there = true;
        let mut chrome = hit(ArtifactKind::Cask, "google-chrome");
        chrome.needs.app_there = true;
        *brew.hits.lock().unwrap() = vec![iterm, chrome];
        let found = session.search_homebrew("it").await.expect("search");
        assert!(found[0].installed);
        assert_eq!(found[0].blocked, None);
        assert_eq!(found[1].blocked, Some(FoundBlock::AppThere));
    }

    #[tokio::test]
    async fn test_a_homebrew_in_usr_local_is_taken_to_run_as_intel() {
        // From the review of the check: on Apple silicon, /usr/local's
        // Homebrew runs under Rosetta, installs an Intel-only cask, and
        // refuses one for Apple silicon -- whatever chip the Mac has.
        let mut instance = crate::testing::manager_instance("brew", "brew:/usr/local");
        instance.prefix = std::path::PathBuf::from("/usr/local");
        let (session, brew) = session_with(instance).await;
        let mut intel = hit(ArtifactKind::Cask, "intel-only");
        intel.needs.chip = Some(Chip::Intel);
        let mut arm = hit(ArtifactKind::Cask, "arm-only");
        arm.needs.chip = Some(Chip::AppleSilicon);
        *brew.hits.lock().unwrap() = vec![intel, arm];
        let found = session.search_homebrew("only").await.expect("search");
        assert_eq!(found[0].blocked, None);
        assert_eq!(found[1].blocked, Some(FoundBlock::OtherChip));
    }

    #[test]
    fn test_what_a_result_needs_blocks_it_only_where_this_mac_lacks_it() {
        fn host(macos_major: Option<u32>, apple_silicon: bool) -> crate::catalog::Host {
            crate::catalog::Host {
                macos_major,
                apple_silicon,
                memory_bytes: None,
                free_bytes: None,
                ollama_app: false,
            }
        }
        let needs = |macos, chip| HitNeeds {
            disabled: false,
            macos,
            chip,
            app_there: false,
        };
        let tahoe_arm = host(Some(26), true);
        assert_eq!(blocked_on(&HitNeeds::default(), &tahoe_arm), None);
        assert_eq!(blocked_on(&needs(Some(26), None), &tahoe_arm), None);
        assert_eq!(
            blocked_on(&needs(Some(15), None), &host(Some(14), true)),
            Some(FoundBlock::NeedsMacOS { major: 15 })
        );
        // macOS unread: Homebrew's own refusal says it.
        assert_eq!(blocked_on(&needs(Some(15), None), &host(None, true)), None);
        assert_eq!(
            blocked_on(
                &needs(None, Some(Chip::AppleSilicon)),
                &host(Some(15), false)
            ),
            Some(FoundBlock::OtherChip)
        );
        assert_eq!(
            blocked_on(&needs(None, Some(Chip::Intel)), &tahoe_arm),
            Some(FoundBlock::OtherChip)
        );
        assert_eq!(
            blocked_on(&needs(None, Some(Chip::AppleSilicon)), &tahoe_arm),
            None
        );
        // Its app there first, then disabled, then macOS, then the chip.
        let all = HitNeeds {
            disabled: true,
            macos: Some(30),
            chip: Some(Chip::Intel),
            app_there: true,
        };
        assert_eq!(blocked_on(&all, &tahoe_arm), Some(FoundBlock::AppThere));
        let all = HitNeeds {
            app_there: false,
            ..all
        };
        assert_eq!(blocked_on(&all, &tahoe_arm), Some(FoundBlock::Disabled));
        assert_eq!(
            blocked_on(
                &HitNeeds {
                    disabled: false,
                    ..all
                },
                &tahoe_arm
            ),
            Some(FoundBlock::NeedsMacOS { major: 30 })
        );
    }

    #[tokio::test]
    async fn test_past_the_results_kept_the_oldest_go_first() {
        let (session, _) = session_with(crate::testing::manager_instance(
            "brew",
            "brew:/opt/homebrew",
        ))
        .await;
        // FOUND_KEPT results from earlier, within their lifetime, each a
        // millisecond after the one before.
        let start = Instant::now()
            .checked_sub(Duration::from_secs(300))
            .expect("an instant that long ago");
        {
            let mut found = session.found.lock().unwrap();
            for n in 0..FOUND_KEPT {
                found.insert(
                    format!("old-{n}"),
                    FoundEntry {
                        request: OpRequest {
                            kind: OpKind::Install,
                            instance_id: "brew:/opt/homebrew".to_string(),
                            artifact_kind: ArtifactKind::Formula,
                            name: format!("old{n}"),
                        },
                        blocked: None,
                        found_at: start + Duration::from_millis(n as u64),
                    },
                );
            }
        }
        let found = session.search_homebrew("jq").await.expect("search");
        let kept = session.found.lock().unwrap();
        assert_eq!(kept.len(), FOUND_KEPT);
        // The newest search's all kept; the oldest earlier ones gone.
        assert!(found.iter().all(|f| kept.contains_key(&f.id)));
        for n in 0..found.len() {
            assert!(!kept.contains_key(&format!("old-{n}")), "old-{n}");
        }
        assert!(kept.contains_key(&format!("old-{}", found.len())));
    }

    #[tokio::test]
    async fn test_no_more_than_searches_at_once_run_together() {
        let brew = Arc::new(FakeBrew {
            meta: test_support::fake_adapter_meta("brew"),
            instance: crate::testing::manager_instance("brew", "brew:/opt/homebrew"),
            listed: Vec::new(),
            hits: Mutex::new(vec![hit(ArtifactKind::Formula, "jq")]),
            searched: Mutex::new(Vec::new()),
            plans: AtomicUsize::new(0),
            search_delay: Duration::from_millis(40),
            searching: AtomicUsize::new(0),
            most_searching: AtomicUsize::new(0),
        });
        let adapter: Arc<dyn Adapter> = brew.clone();
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let searches: Vec<_> = (0..6)
            .map(|n| {
                let session = session.clone();
                tokio::spawn(async move { session.search_homebrew(&format!("jq{n}")).await })
            })
            .collect();
        for search in searches {
            assert!(search.await.expect("joined").is_ok());
        }
        assert_eq!(brew.searched.lock().unwrap().len(), 6);
        assert!(
            brew.most_searching.load(Ordering::SeqCst) <= SEARCHES_AT_ONCE,
            "{} at once",
            brew.most_searching.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn test_a_suggestion_found_by_searching_is_installed_only_where_its_entry_would_be() {
        // Claude Code from its `claude-code@latest` cask: the catalog says
        // Installed, so the search's `claude-code` is not a second copy.
        let (session, brew) = session_listing(
            crate::testing::manager_instance("brew", "brew:/opt/homebrew"),
            vec![(ArtifactKind::Cask, "claude-code@latest")],
        )
        .await;
        let found = session.search_homebrew("claude").await.expect("search");
        let cask = found.iter().find(|f| f.name == "claude-code").unwrap();
        assert!(matches!(
            session.plan_found(&cask.id).await,
            Err(FoundError::State(crate::catalog::CatalogState::Installed))
        ));
        // Not a suggestion: planned.
        let other = found.iter().find(|f| f.name == "jq-lsp").unwrap();
        assert!(session.plan_found(&other.id).await.is_ok());
        assert_eq!(brew.plans.load(Ordering::SeqCst), 1);
    }
}
