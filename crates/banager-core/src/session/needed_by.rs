//! What other sources run on the Homebrew package an uninstall would
//! remove, added to its preview by `issue_plan` (`crate::needed_by`), and
//! the refusal `submit` gives a preview that named any. Its own file, like
//! `kept.rs`, so `plans.rs` only calls it.

use super::Session;
use crate::model::{
    ArtifactKind, InstalledArtifact, ManagerInstance, OpKind, OpRequest, Plan, Warning,
};
use crate::needed_by::{self, HOSTED};
use crate::runner::HostEnv;

/// What `needed_by` looks at for one uninstall, copied out of the snapshot
/// under its lock: the Homebrew package, its Homebrew, and the sources
/// that could run on it with their rows.
pub(super) struct Subject {
    package: InstalledArtifact,
    brew: ManagerInstance,
    instances: Vec<ManagerInstance>,
    artifacts: Vec<InstalledArtifact>,
}

/// The `Subject` of `req`, when it uninstalls a formula or cask of a
/// Homebrew the snapshot lists with that row; `None` for anything else.
pub(super) fn subject(
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    req: &OpRequest,
) -> Option<Subject> {
    if req.kind != OpKind::Uninstall
        || !matches!(
            req.artifact_kind,
            ArtifactKind::Formula | ArtifactKind::Cask
        )
    {
        return None;
    }
    let brew = instances
        .iter()
        .find(|i| i.id == req.instance_id && i.adapter_id == "brew")?;
    let package = artifacts.iter().find(|a| {
        a.key.instance_id == req.instance_id
            && a.key.kind == req.artifact_kind
            && a.key.name == req.name
    })?;
    let hosted: Vec<ManagerInstance> = instances
        .iter()
        .filter(|i| HOSTED.contains(&i.adapter_id.as_str()))
        .cloned()
        .collect();
    let artifacts = artifacts
        .iter()
        .filter(|a| hosted.iter().any(|i| i.id == a.key.instance_id))
        .cloned()
        .collect();
    Some(Subject {
        package: package.clone(),
        brew: brew.clone(),
        instances: hosted,
        artifacts,
    })
}

/// Whether `plan`'s preview named a source that runs on its package: such
/// a plan is never run (`Session::submit`).
pub(super) fn names_a_source(plan: &Plan) -> bool {
    plan.warnings
        .iter()
        .any(|warning| matches!(warning, Warning::NeededBySource { .. }))
}

impl Session {
    /// Remembers the `PATH` and home folder the last refresh read, which
    /// `with_needed_by` looks with: which `node` npm is run with, and the
    /// places never looked into. Only for a session that looks at the disk
    /// (`Session::new`, `with_adapters_and_sizes`), as with the data an
    /// uninstall leaves behind, so a test refreshing a fake source never
    /// has the links on its paths followed.
    pub(super) fn note_needed_by_env(&self, env: &HostEnv) {
        if self.sizes.is_some() {
            *self.needed_by_env.lock().unwrap() = Some(env.clone());
        }
    }

    /// `plan`, with a `Warning::NeededBySource` after its own warnings for
    /// each source that runs on `subject`'s package (`needed_by::needed_by`,
    /// on a blocking thread, within `needed_by::BUDGET`), and with
    /// `Warning::DependentsUnknown` when that look did not finish and the
    /// plan does not say it already. `plan` as it is with no subject, and
    /// before any refresh.
    pub(super) async fn with_needed_by(&self, mut plan: Plan, subject: Option<Subject>) -> Plan {
        let Some(subject) = subject else {
            return plan;
        };
        let Some(env) = self.needed_by_env.lock().unwrap().clone() else {
            return plan;
        };
        let found = tokio::task::spawn_blocking(move || {
            needed_by::needed_by(
                &subject.package,
                &subject.brew,
                &subject.instances,
                &subject.artifacts,
                &env,
                needed_by::BUDGET,
            )
        })
        .await;
        let complete = match found {
            Ok(found) => {
                plan.warnings.extend(found.warnings);
                found.complete
            }
            Err(_) => false,
        };
        if !complete && !plan.warnings.contains(&Warning::DependentsUnknown) {
            plan.warnings.push(Warning::DependentsUnknown);
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpKind,
        OpRequest, Outcome, Plan, Reconciled, SearchHit, UninstallBlocked, Warning,
    };
    use crate::runner::HostEnv;
    use crate::session::{Session, SubmitError};
    use async_trait::async_trait;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    /// A source of a test's own, under a real adapter id, listing `rows`.
    struct Fake {
        meta: AdapterMeta,
        instance: ManagerInstance,
        rows: Vec<(ArtifactKind, &'static str)>,
    }

    #[async_trait]
    impl Adapter for Fake {
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
                .rows
                .iter()
                .map(|(kind, name)| InstalledArtifact {
                    key: ArtifactKey {
                        instance_id: inst.id.clone(),
                        kind: *kind,
                        name: name.to_string(),
                    },
                    display_name: name.to_string(),
                    version: "1.0.0".to_string(),
                    reason: InstallReason::Requested,
                    description: None,
                    homepage: None,
                    size_bytes: None,
                    installed_at: None,
                    path: None,
                    auto_updates: false,
                    uninstall_blocked: None,
                    facts: Default::default(),
                })
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

    /// A folder of a test's own standing in for `/`, removed when dropped:
    /// a Homebrew in `opt/homebrew` with a keg-only `node@22` and a linked
    /// `node`, and a home folder.
    struct Root(PathBuf);

    impl Root {
        fn new(tag: &str) -> Root {
            let dir = std::env::temp_dir().join(format!(
                "banager-needed-by-session-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(dir.join("home")).unwrap();
            let root = Root(std::fs::canonicalize(&dir).unwrap());
            for program in [
                "opt/homebrew/Cellar/node@22/22.23.3/bin/node",
                "opt/homebrew/Cellar/node@22/22.23.3/lib/node_modules/npm/bin/npm-cli.js",
                "opt/homebrew/Cellar/node/24.9.0/bin/node",
                "opt/homebrew/Cellar/node/24.9.0/lib/node_modules/npm/bin/npm-cli.js",
            ] {
                let path = root.0.join(program);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, b"#!/bin/sh\n").unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            for keg in ["node@22/22.23.3", "node/24.9.0"] {
                root.link(
                    &format!("opt/homebrew/Cellar/{keg}/bin/npm"),
                    "../lib/node_modules/npm/bin/npm-cli.js",
                );
            }
            root
        }

        fn link(&self, relative: &str, target: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(target, path).unwrap();
        }

        /// `brew link` of `keg` (`node@22/22.23.3`): its `node` and `npm`
        /// in the prefix's `bin`.
        fn link_into_bin(&self, keg: &str) {
            for name in ["node", "npm"] {
                self.link(
                    &format!("opt/homebrew/bin/{name}"),
                    &format!("../Cellar/{keg}/bin/{name}"),
                );
            }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const BREW: &str = "brew:/opt/homebrew";
    const NPM: &str = "npm:/opt/homebrew";

    fn fake(
        adapter_id: &str,
        id: &str,
        exe_path: PathBuf,
        prefix: PathBuf,
        rows: Vec<(ArtifactKind, &'static str)>,
    ) -> Arc<dyn Adapter> {
        Arc::new(Fake {
            meta: test_support::fake_adapter_meta(adapter_id),
            instance: ManagerInstance {
                exe_path,
                prefix,
                ..test_support::make_instance(adapter_id, id)
            },
            rows,
        })
    }

    /// A session over this root's Homebrew (`node@22` and `node`) and an
    /// npm whose `npm` is the one in the prefix's `bin`, with three
    /// packages of the user's and npm's own; refreshed once, with `PATH`
    /// the prefix's `bin`.
    async fn session_over(root: &Root, looks_at_disk: bool) -> Arc<Session> {
        let prefix = root.path("opt/homebrew");
        let adapters = vec![
            fake(
                "brew",
                BREW,
                prefix.join("bin/brew"),
                prefix.clone(),
                vec![
                    (ArtifactKind::Formula, "node"),
                    (ArtifactKind::Formula, "node@22"),
                ],
            ),
            fake(
                "npm",
                NPM,
                prefix.join("bin/npm"),
                prefix.clone(),
                vec![
                    (ArtifactKind::Package, "@openai/codex"),
                    (ArtifactKind::Package, "corepack"),
                    (ArtifactKind::Package, "npm"),
                    (ArtifactKind::Package, "prettier"),
                    (ArtifactKind::Package, "typescript"),
                ],
            ),
        ];
        let sink = Arc::new(VecSink::new());
        let session = if looks_at_disk {
            Session::with_adapters_and_sizes(sink, adapters, None)
        } else {
            Session::with_adapters(sink, adapters, None)
        };
        let env = HostEnv {
            path_dirs: vec![prefix.join("bin")],
            home: root.path("home"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        session.refresh(&env, &CheckOptions::default()).await;
        session
    }

    fn uninstall(name: &str) -> OpRequest {
        OpRequest {
            kind: OpKind::Uninstall,
            instance_id: BREW.to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: name.to_string(),
        }
    }

    fn needed(plan: &Plan) -> Vec<Warning> {
        plan.warnings
            .iter()
            .filter(|warning| matches!(warning, Warning::NeededBySource { .. }))
            .cloned()
            .collect()
    }

    #[tokio::test]
    async fn test_the_preview_of_the_node_npm_runs_on_names_npm_and_submit_refuses_it() {
        let root = Root::new("linked");
        root.link_into_bin("node@22/22.23.3");
        let session = session_over(&root, true).await;

        let issued = session.issue_plan(&uninstall("node@22")).await.unwrap();
        assert_eq!(
            needed(&issued.plan),
            vec![Warning::NeededBySource {
                instance_id: NPM.to_string(),
                program: true,
                tools: 3,
            }]
        );
        assert!(!issued.plan.warnings.contains(&Warning::DependentsUnknown));
        // Whatever the page sends, this preview is never run.
        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::UninstallBlocked {
                reason: UninstallBlocked::NeededBySource
            })
        );
        assert!(session.operations().is_empty(), "nothing was queued");

        // `node`, not linked, is nobody's: its preview names nothing, and
        // it is run as any other.
        let issued = session.issue_plan(&uninstall("node")).await.unwrap();
        assert_eq!(needed(&issued.plan), Vec::new());
        assert!(session.submit(issued.id).is_ok());
    }

    #[tokio::test]
    async fn test_with_node_linked_instead_node_22_is_free_to_go() {
        let root = Root::new("unlinked");
        root.link_into_bin("node/24.9.0");
        let session = session_over(&root, true).await;
        let issued = session.issue_plan(&uninstall("node@22")).await.unwrap();
        assert_eq!(needed(&issued.plan), Vec::new());
        let issued = session.issue_plan(&uninstall("node")).await.unwrap();
        assert_eq!(
            needed(&issued.plan),
            vec![Warning::NeededBySource {
                instance_id: NPM.to_string(),
                program: true,
                tools: 3,
            }]
        );
    }

    #[tokio::test]
    async fn test_a_session_that_does_not_look_at_the_disk_adds_nothing() {
        let root = Root::new("off");
        root.link_into_bin("node@22/22.23.3");
        let session = session_over(&root, false).await;
        let issued = session.issue_plan(&uninstall("node@22")).await.unwrap();
        assert_eq!(needed(&issued.plan), Vec::new());
        assert!(!issued.plan.warnings.contains(&Warning::DependentsUnknown));
    }

    #[tokio::test]
    async fn test_an_update_or_an_npm_uninstall_is_never_looked_at() {
        let root = Root::new("kinds");
        root.link_into_bin("node@22/22.23.3");
        let session = session_over(&root, true).await;
        // The fake lists no update, so an upgrade plan is asked of it as the
        // window never would (`issue_plan`, not `issue_listed_plan`).
        let upgrade = OpRequest {
            kind: OpKind::Upgrade,
            ..uninstall("node@22")
        };
        let issued = session.issue_plan(&upgrade).await.unwrap();
        assert_eq!(needed(&issued.plan), Vec::new());
        let package = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: NPM.to_string(),
            artifact_kind: ArtifactKind::Package,
            name: "prettier".to_string(),
        };
        let issued = session.issue_plan(&package).await.unwrap();
        assert_eq!(needed(&issued.plan), Vec::new());
    }

    #[test]
    fn test_the_subject_copies_only_the_sources_that_can_run_on_a_package() {
        let brew = ManagerInstance {
            prefix: Path::new("/opt/homebrew").to_path_buf(),
            ..test_support::make_instance("brew", BREW)
        };
        let npm = test_support::make_instance("npm", NPM);
        let claude = test_support::make_instance("standalone-claude", "standalone-claude");
        let row = |instance_id: &str, kind: ArtifactKind, name: &str| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: "1".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
            facts: Default::default(),
        };
        let instances = vec![brew, npm, claude];
        let artifacts = vec![
            row(BREW, ArtifactKind::Formula, "node@22"),
            row(NPM, ArtifactKind::Package, "prettier"),
            row("standalone-claude", ArtifactKind::Binary, "claude"),
        ];
        let subject = super::subject(&instances, &artifacts, &uninstall("node@22")).unwrap();
        assert_eq!(subject.package.key.name, "node@22");
        assert_eq!(subject.brew.id, BREW);
        let ids: Vec<&str> = subject.instances.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec![NPM]);
        let names: Vec<&str> = subject
            .artifacts
            .iter()
            .map(|a| a.key.name.as_str())
            .collect();
        assert_eq!(names, vec!["prettier"]);
        // Not a row the snapshot lists, not Homebrew's, not an uninstall.
        assert!(super::subject(&instances, &artifacts, &uninstall("node")).is_none());
        let npm_uninstall = OpRequest {
            instance_id: NPM.to_string(),
            ..uninstall("node@22")
        };
        assert!(super::subject(&instances, &artifacts, &npm_uninstall).is_none());
        let upgrade = OpRequest {
            kind: OpKind::Upgrade,
            ..uninstall("node@22")
        };
        assert!(super::subject(&instances, &artifacts, &upgrade).is_none());
    }
}
