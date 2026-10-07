//! What other sources run on the Homebrew package an uninstall would
//! remove, added to its preview by `issue_plan` (`crate::needed_by`), and
//! the refusal `submit` gives a preview that named any. Its own file, like
//! `kept.rs`, so `plans.rs` only calls it.

use super::Session;
use crate::model::{
    ArtifactKey, ArtifactKind, InstalledArtifact, InstanceId, ManagerInstance, OpKind, OpRequest,
    Plan, Warning,
};
use crate::needed_by::{self, HOSTED};
use crate::runner::HostEnv;
use std::path::PathBuf;

/// What `needed_by` looks at for one uninstall, copied out of the snapshot
/// under its lock: the Homebrew package, its Homebrew, and the sources
/// that could run on it with their rows.
pub(super) struct Subject {
    package: InstalledArtifact,
    brew: ManagerInstance,
    instances: Vec<ManagerInstance>,
    artifacts: Vec<InstalledArtifact>,
}

/// What `needed_by` reads of a `Subject` that a later snapshot can change,
/// and nothing else: the package's key and a cask's app (`own_folders`),
/// its Homebrew's id and prefix, and of each source with a tool that
/// counts (`needed_by::counts`) its kind, its program and those tools, each
/// with its environment (pipx's and uv's, `InstalledArtifact::path`). Kept
/// with a Homebrew uninstall's preview, so that `Session::submit` can tell
/// whether the snapshot it is confirmed against could give the package a
/// dependent the preview did not look for (`adds_no_dependent`). Not the
/// rows themselves: a version, a description or when a source answered is
/// nothing `needed_by` reads, and a preview held for ten minutes keeps no
/// copy of every hosted source's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Inputs {
    package: (ArtifactKey, Option<PathBuf>),
    brew: (InstanceId, PathBuf),
    sources: Vec<Source>,
}

/// One source of `Inputs`: one with at least one tool that counts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    id: InstanceId,
    adapter_id: String,
    exe_path: PathBuf,
    tools: Vec<(ArtifactKey, Option<PathBuf>)>,
}

impl Subject {
    pub(super) fn inputs(&self) -> Inputs {
        let sources = self
            .instances
            .iter()
            .filter_map(|source| {
                let tools: Vec<_> = self
                    .artifacts
                    .iter()
                    .filter(|tool| {
                        tool.key.instance_id == source.id
                            && needed_by::counts(&source.adapter_id, tool)
                    })
                    .map(|tool| (tool.key.clone(), tool.path.clone()))
                    .collect();
                (!tools.is_empty()).then(|| Source {
                    id: source.id.clone(),
                    adapter_id: source.adapter_id.clone(),
                    exe_path: source.exe_path.clone(),
                    tools,
                })
            })
            .collect();
        Inputs {
            package: (self.package.key.clone(), self.package.path.clone()),
            brew: (self.brew.id.clone(), self.brew.prefix.clone()),
            sources,
        }
    }
}

/// Whether a preview whose look was given `before` still says all that
/// runs on its package when the snapshot gives `now`: nothing `needed_by`
/// reads of the package or its Homebrew has changed, and every tool that
/// counts now was there to be looked at -- in the same source, run by the
/// same program, with the same environment. A source's first such tool,
/// or one that has come to count (a pip package no longer a dependency),
/// is one the preview never looked at: a fresh preview has to. Fewer tools
/// cannot add a dependent -- a source with none is not looked at, a pipx
/// or uv tool gone is one fewer on the package -- so an uninstall or an
/// update in another source, a source gone quiet with its rows carried,
/// or a Homebrew whose catalogue update ended, leaves the preview the one
/// to confirm. A preview with no subject (an uninstall `needed_by` does
/// not look at) stays so only while the request still has none.
pub(super) fn adds_no_dependent(before: Option<&Inputs>, now: Option<&Inputs>) -> bool {
    let (Some(before), Some(now)) = (before, now) else {
        return before.is_none() && now.is_none();
    };
    before.package == now.package
        && before.brew == now.brew
        && now.sources.iter().all(|source| {
            before.sources.iter().any(|was| {
                was.id == source.id
                    && was.adapter_id == source.adapter_id
                    && was.exe_path == source.exe_path
                    && source.tools.iter().all(|tool| was.tools.contains(tool))
            })
        })
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
    /// `Warning::DependentsUnknown` when that look did not finish -- or did
    /// not come back within `GRACE` of its budget -- and the plan does not
    /// say it already. `plan` as it is with no subject, and before any
    /// refresh.
    pub(super) async fn with_needed_by(&self, mut plan: Plan, subject: Option<Subject>) -> Plan {
        let Some(subject) = subject else {
            return plan;
        };
        let Some(env) = self.needed_by_env.lock().unwrap().clone() else {
            return plan;
        };
        let budget = needed_by::BUDGET;
        let found = bounded(
            move || {
                needed_by::needed_by(
                    &subject.package,
                    &subject.brew,
                    &subject.instances,
                    &subject.artifacts,
                    &env,
                    budget,
                )
            },
            budget.max_duration + GRACE,
        )
        .await;
        let complete = match found {
            Some(found) => {
                plan.warnings.extend(found.warnings);
                found.complete
            }
            None => false,
        };
        if !complete && !plan.warnings.contains(&Warning::DependentsUnknown) {
            plan.warnings.push(Warning::DependentsUnknown);
        }
        plan
    }
}

/// How long past its own budget the look may take before the preview stops
/// waiting for it, as the command check's halves are given
/// (`commands::GRACE`): a step stuck in the kernel -- a folder on a disk
/// that stopped answering -- never comes back to look at the clock. The
/// look then goes on alone, and the preview says it did not finish.
const GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// What `look` found, run on the blocking pool; `None` when it panicked or
/// did not end within `wait`.
async fn bounded<F>(look: F, wait: std::time::Duration) -> Option<needed_by::NeededBy>
where
    F: FnOnce() -> needed_by::NeededBy + Send + 'static,
{
    tokio::time::timeout(wait, tokio::task::spawn_blocking(look))
        .await
        .ok()?
        .ok()
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use crate::adapters::brew::BrewAdapter;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpKind,
        OpRequest, Outcome, Plan, Reconciled, SearchHit, UninstallBlocked, Warning,
    };
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use crate::session::{Session, SubmitError};
    use async_trait::async_trait;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    /// A source of a test's own, under a real adapter id, listing `rows`.
    struct Fake {
        meta: AdapterMeta,
        instance: ManagerInstance,
        rows: Mutex<Vec<(ArtifactKind, &'static str)>>,
        brew: Option<BrewAdapter>,
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
                .lock()
                .unwrap()
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
            if let Some(brew) = &self.brew {
                return brew.plan(inst, req).await;
            }
            Ok(test_support::fake_plan(inst, req))
        }

        async fn execute(
            &self,
            plan: &Plan,
            sink: Arc<dyn EventSink>,
            op_id: OpId,
            cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
            if let Some(brew) = &self.brew {
                return brew.execute(plan, sink, op_id, cancel).await;
            }
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
            rows: Mutex::new(rows),
            brew: None,
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
    async fn test_unresolved_aliases_keep_dependents_unknown_in_the_uninstall_preview() {
        // A launcher whose name is not what it leads to: pip's `python3`
        // into python@3.13's `bin/python3.13` (a keg with no `python3`),
        // uv's `uv` into its keg's `libexec`. Followed at the refresh,
        // then rerouted through `~/Documents` or a link loop before the
        // preview, as an install changing between rounds can do. The
        // package it could lead into -- a Python for pip's, the formula
        // named `uv` for uv's -- says it could not check; jq, which it
        // could not lead into, says nothing of it.
        for (adapter, launcher, package, target) in [
            ("pip", "python3", "python@3.13", "3.13.9/bin/python3.13"),
            ("uv", "uv", "uv", "0.9.2/libexec/vendor-launcher"),
        ] {
            for refused in [false, true] {
                let root = Root::new("renamed-source");
                let prefix = root.path("opt/homebrew");
                let real = prefix.join("Cellar").join(package).join(target);
                std::fs::create_dir_all(real.parent().unwrap()).unwrap();
                std::fs::write(&real, b"#!/bin/sh\n").unwrap();
                std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
                let keg = target.split('/').next().unwrap();
                root.link(
                    &format!("opt/homebrew/opt/{package}"),
                    &format!("../Cellar/{package}/{keg}"),
                );
                let jq = prefix.join("Cellar/jq/1.8.1/bin/jq");
                std::fs::create_dir_all(jq.parent().unwrap()).unwrap();
                std::fs::write(&jq, b"#!/bin/sh\n").unwrap();
                root.link("opt/homebrew/opt/jq", "../Cellar/jq/1.8.1");
                let relative = format!("home/bin/{launcher}");
                root.link(&relative, real.to_str().unwrap());
                let source_path = root.path(&relative);
                let adapters = vec![
                    fake(
                        "brew",
                        BREW,
                        prefix.join("bin/brew"),
                        prefix.clone(),
                        vec![
                            (ArtifactKind::Formula, package),
                            (ArtifactKind::Formula, "jq"),
                        ],
                    ),
                    fake(
                        adapter,
                        "source",
                        source_path.clone(),
                        root.path("home/bin"),
                        vec![(ArtifactKind::Package, "requests")],
                    ),
                ];
                let session =
                    Session::with_adapters_and_sizes(Arc::new(VecSink::new()), adapters, None);
                let env = HostEnv {
                    path_dirs: vec![root.path("home/bin")],
                    home: root.path("home"),
                    euid: 501,
                    cargo_home: None,
                    rustup_home: None,
                    zdotdir: None,
                    ollama_host: None,
                };
                session.refresh(&env, &CheckOptions::default()).await;
                let known = session.issue_plan(&uninstall(package)).await.unwrap();
                assert_eq!(needed(&known.plan).len(), 1);
                assert!(!known.plan.warnings.contains(&Warning::DependentsUnknown));
                std::fs::remove_file(&source_path).unwrap();
                if refused {
                    root.link(&relative, launcher); // Refused: a link loop.
                } else {
                    root.link("home/Documents/launcher", real.to_str().unwrap());
                    root.link(
                        &relative,
                        root.path("home/Documents/launcher").to_str().unwrap(),
                    );
                }
                let unknown = session.issue_plan(&uninstall(package)).await.unwrap();
                assert!(needed(&unknown.plan).is_empty());
                assert!(
                    unknown.plan.warnings.contains(&Warning::DependentsUnknown),
                    "{adapter} refused={refused}"
                );
                let bystander = session.issue_plan(&uninstall("jq")).await.unwrap();
                assert!(
                    !bystander
                        .plan
                        .warnings
                        .contains(&Warning::DependentsUnknown),
                    "{adapter} refused={refused}: jq"
                );
                // A known missing launcher is not an unresolved one.
                std::fs::remove_file(&source_path).unwrap();
                let gone = session.issue_plan(&uninstall(package)).await.unwrap();
                assert!(
                    !gone.plan.warnings.contains(&Warning::DependentsUnknown),
                    "{adapter}"
                );
                assert!(
                    session.operations().is_empty(),
                    "no uninstall was submitted"
                );
            }
        }
    }

    /// `session_over(root, true)`, with its Homebrew's and npm's fakes
    /// kept so a test can change what their next inventory lists, and the
    /// environment it refreshes with.
    async fn session_with_fakes(root: &Root) -> (Arc<Session>, Arc<Fake>, Arc<Fake>, HostEnv) {
        let prefix = root.path("opt/homebrew");
        let fake = |adapter_id: &str, id: &str, rows: Vec<(ArtifactKind, &'static str)>| {
            Arc::new(Fake {
                meta: test_support::fake_adapter_meta(adapter_id),
                instance: ManagerInstance {
                    exe_path: prefix.join("bin").join(adapter_id),
                    prefix: prefix.clone(),
                    ..test_support::make_instance(adapter_id, id)
                },
                rows: Mutex::new(rows),
                brew: None,
            })
        };
        let brew = fake(
            "brew",
            BREW,
            vec![
                (ArtifactKind::Formula, "node"),
                (ArtifactKind::Formula, "node@22"),
            ],
        );
        let npm = fake(
            "npm",
            NPM,
            vec![
                (ArtifactKind::Package, "@openai/codex"),
                (ArtifactKind::Package, "corepack"),
                (ArtifactKind::Package, "npm"),
                (ArtifactKind::Package, "prettier"),
                (ArtifactKind::Package, "typescript"),
            ],
        );
        let session = Session::with_adapters_and_sizes(
            Arc::new(VecSink::new()),
            vec![brew.clone(), npm.clone()],
            None,
        );
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
        (session, brew, npm, env)
    }

    #[tokio::test]
    async fn test_refreshes_that_add_no_dependent_leave_runtime_preview_usable() {
        // npm runs on the linked `node` (24.9.0), so node@22's preview
        // names nothing. A refresh that commits while it is open, and can
        // make nothing run on node@22, leaves it the one to confirm: the
        // round after an operation in another source finished (the
        // frontend refreshes after each), after `brew update` ended, or
        // after a source went quiet. Through the fakes' own answers where
        // they can give it, else as such a round commits it.
        type Listed = fn(&Fake, &Fake);
        type Committed = fn(&mut crate::session::Snapshot);
        // Each: what the fakes list before the preview, then after it.
        let listed: [(&str, Listed, Listed); 3] = [
            (
                "another Homebrew package",
                |_, _| {},
                |brew, _| {
                    brew.rows
                        .lock()
                        .unwrap()
                        .push((ArtifactKind::Formula, "jq"))
                },
            ),
            (
                "an npm tool uninstalled",
                |_, _| {},
                |_, npm| {
                    npm.rows
                        .lock()
                        .unwrap()
                        .retain(|(_, name)| *name != "prettier")
                },
            ),
            // npm's own, which runs on whatever npm does: not a tool.
            (
                "npm's own corepack listed",
                |_, npm| {
                    npm.rows
                        .lock()
                        .unwrap()
                        .retain(|(_, name)| *name != "corepack")
                },
                |_, npm| {
                    npm.rows
                        .lock()
                        .unwrap()
                        .push((ArtifactKind::Package, "corepack"))
                },
            ),
        ];
        let committed: [(&str, Committed); 4] = [
            ("an npm tool updated", |snapshot| {
                let tool = snapshot
                    .artifacts
                    .iter_mut()
                    .find(|a| a.key.instance_id == NPM && a.key.name == "prettier");
                tool.unwrap().version = "3.6.2".to_string();
            }),
            ("Homebrew's catalogue may be behind", |snapshot| {
                let brew = snapshot.instances.iter_mut().find(|i| i.id == BREW);
                brew.unwrap().status.notes = vec![crate::model::InstanceNote::IndexMayBeStale];
            }),
            ("npm stopped answering, its rows carried", |snapshot| {
                let npm = snapshot.instances.iter_mut().find(|i| i.id == NPM);
                npm.unwrap().status.unavailable = Some(crate::model::Unavailable::NotResponding);
            }),
            ("only the answer times", |snapshot| {
                for instance in &mut snapshot.instances {
                    instance.answered_at = Some(42);
                }
            }),
        ];
        for (what, prepare, change) in listed {
            let root = Root::new("no-dependent-listed");
            root.link_into_bin("node/24.9.0");
            let (session, brew, npm, env) = session_with_fakes(&root).await;
            prepare(&brew, &npm);
            let before = session.refresh(&env, &CheckOptions::default()).await;
            let old = session
                .issue_listed_plan(&uninstall("node@22"))
                .await
                .unwrap();
            assert!(needed(&old.plan).is_empty(), "{what}");
            change(&brew, &npm);
            let after = session.refresh(&env, &CheckOptions::default()).await;
            assert!(after.generation > before.generation, "{what}");
            assert!(session.submit(old.id).is_ok(), "{what}");
        }
        for (what, change) in committed {
            let root = Root::new("no-dependent-committed");
            root.link_into_bin("node/24.9.0");
            let (session, _, _, _) = session_with_fakes(&root).await;
            let old = session
                .issue_listed_plan(&uninstall("node@22"))
                .await
                .unwrap();
            assert!(needed(&old.plan).is_empty(), "{what}");
            {
                let mut snapshot = session.snapshot.lock().unwrap();
                change(&mut snapshot);
                snapshot.generation += 1;
            }
            assert!(session.submit(old.id).is_ok(), "{what}");
        }
    }

    /// `subject`'s `Inputs` for an uninstall of the formula `python@3.13`
    /// of this Homebrew, over `instances` and `artifacts` as a snapshot
    /// lists them.
    fn inputs_of(
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
    ) -> Option<super::Inputs> {
        let request = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: BREW.to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "python@3.13".to_string(),
        };
        super::subject(instances, artifacts, &request).map(|subject| subject.inputs())
    }

    #[test]
    fn test_only_a_tool_the_preview_did_not_look_at_spends_it() {
        // A Homebrew Python, a pip that runs on whatever its `python3`
        // leads to, and a pipx whose tools each have an environment: the
        // rows as their inventories give them.
        let brew = ManagerInstance {
            prefix: PathBuf::from("/opt/homebrew"),
            ..test_support::make_instance("brew", BREW)
        };
        let pip = ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/python3"),
            ..test_support::make_instance("pip", "pip:/opt/homebrew/bin/python3")
        };
        let pipx = ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/pipx"),
            ..test_support::make_instance("pipx", "pipx:/Users/me/.local")
        };
        let row = |instance: &ManagerInstance, name: &str, reason, path: Option<&str>| {
            InstalledArtifact {
                key: ArtifactKey {
                    instance_id: instance.id.clone(),
                    kind: if instance.adapter_id == "brew" {
                        ArtifactKind::Formula
                    } else if instance.adapter_id == "pipx" {
                        ArtifactKind::Tool
                    } else {
                        ArtifactKind::Package
                    },
                    name: name.to_string(),
                },
                display_name: name.to_string(),
                version: "1.0.0".to_string(),
                reason,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path: path.map(PathBuf::from),
                auto_updates: false,
                uninstall_blocked: None,
                facts: Default::default(),
            }
        };
        let instances = vec![brew.clone(), pip.clone(), pipx.clone()];
        let artifacts = vec![
            row(&brew, "python@3.13", InstallReason::Requested, None),
            row(&pip, "pip", InstallReason::Requested, None),
            row(&pip, "requests", InstallReason::Requested, None),
            row(&pip, "urllib3", InstallReason::Dependency, None),
            row(
                &pipx,
                "ruff",
                InstallReason::Requested,
                Some("/Users/me/.local/pipx/venvs/ruff"),
            ),
        ];
        let before = inputs_of(&instances, &artifacts);
        assert!(before.is_some());
        let with = |change: &dyn Fn(&mut Vec<ManagerInstance>, &mut Vec<InstalledArtifact>)| {
            let (mut instances, mut artifacts) = (instances.clone(), artifacts.clone());
            change(&mut instances, &mut artifacts);
            super::adds_no_dependent(before.as_ref(), inputs_of(&instances, &artifacts).as_ref())
        };
        // Nothing `needed_by` reads, or fewer tools: the preview stands.
        assert!(with(&|_, _| {}));
        assert!(with(&|instances, artifacts| {
            for instance in instances.iter_mut() {
                instance.answered_at = Some(42);
                instance.version = Some("9.9.9".to_string());
                instance.status.notes = vec![crate::model::InstanceNote::IndexMayBeStale];
            }
            for artifact in artifacts.iter_mut() {
                artifact.version = "2.0.0".to_string();
                artifact.description = Some("changed".to_string());
            }
        }));
        assert!(with(
            &|_, artifacts| artifacts.retain(|a| a.key.name != "requests")
        ));
        assert!(with(
            &|_, artifacts| artifacts.retain(|a| a.key.name != "ruff")
        ));
        assert!(with(&|instances, artifacts| {
            instances.retain(|i| i.adapter_id != "pipx");
            artifacts.retain(|a| a.key.instance_id != pipx.id);
        }));
        assert!(with(&|_, artifacts| {
            artifacts.push(row(&pip, "setuptools", InstallReason::Requested, None));
            artifacts.push(row(&pip, "idna", InstallReason::Dependency, None));
            artifacts.push(row(&brew, "jq", InstallReason::Requested, None));
        }));
        // A tool the preview never looked at, or looked at elsewhere: a
        // fresh preview has to.
        for (what, change) in [
            (
                "a pip package installed",
                &(|_: &mut Vec<ManagerInstance>, artifacts: &mut Vec<InstalledArtifact>| {
                    artifacts.push(row(&pip, "black", InstallReason::Requested, None))
                })
                    as &dyn Fn(&mut Vec<ManagerInstance>, &mut Vec<InstalledArtifact>),
            ),
            ("a dependency that came to count", &|_, artifacts| {
                artifacts
                    .iter_mut()
                    .find(|a| a.key.name == "urllib3")
                    .unwrap()
                    .reason = InstallReason::Requested
            }),
            ("a pipx tool in another environment", &|_, artifacts| {
                artifacts
                    .iter_mut()
                    .find(|a| a.key.name == "ruff")
                    .unwrap()
                    .path = Some(PathBuf::from("/Users/me/.local/share/pipx/venvs/ruff"))
            }),
            ("pip run by another python3", &|instances, _| {
                instances
                    .iter_mut()
                    .find(|i| i.adapter_id == "pip")
                    .unwrap()
                    .exe_path = PathBuf::from("/usr/local/bin/python3")
            }),
            ("a new source with a tool", &|instances, artifacts| {
                let uv = test_support::make_instance("uv", "uv:/Users/me/.local");
                artifacts.push(row(&uv, "httpie", InstallReason::Requested, None));
                instances.push(uv);
            }),
            ("the package's Homebrew elsewhere", &|instances, _| {
                instances[0].prefix = PathBuf::from("/usr/local")
            }),
            ("the package gone", &|_, artifacts| {
                artifacts.retain(|a| a.key.name != "python@3.13")
            }),
        ] {
            assert!(!with(change), "{what}");
        }
        // A preview that looked at nothing stays one only while there is
        // nothing to look at.
        assert!(super::adds_no_dependent(None, None));
        assert!(!super::adds_no_dependent(None, before.as_ref()));
    }

    #[tokio::test]
    async fn test_first_dependent_committed_after_runtime_preview_runs_no_removal() {
        let root = Root::new("first-dependent-after-preview");
        root.link_into_bin("node@22/22.23.3");
        let prefix = root.path("opt/homebrew");
        let runner = Arc::new(MockRunner::new());
        let brew_exe = prefix.join("bin/brew");
        let answer = CommandOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            stderr_cause: Default::default(),
            timed_out: false,
            cancelled: false,
        };
        runner.respond(
            vec![brew_exe.to_str().unwrap(), "uses", "--installed", "node@22"],
            answer.clone(),
        );
        runner.respond(
            vec![
                brew_exe.to_str().unwrap(),
                "uninstall",
                "--formula",
                "node@22",
            ],
            answer,
        );
        // Fixture inventory, real Homebrew planning/execution over MockRunner.
        // Every on-disk path is in this test's synthetic tree.
        let brew = Arc::new(Fake {
            meta: test_support::fake_adapter_meta("brew"),
            instance: ManagerInstance {
                exe_path: brew_exe,
                prefix: prefix.clone(),
                ..test_support::make_instance("brew", BREW)
            },
            rows: Mutex::new(vec![(ArtifactKind::Formula, "node@22")]),
            brew: Some(BrewAdapter::new(runner.clone())),
        });
        let npm = Arc::new(Fake {
            meta: test_support::fake_adapter_meta("npm"),
            instance: ManagerInstance {
                exe_path: prefix.join("bin/npm"),
                prefix: prefix.clone(),
                ..test_support::make_instance("npm", NPM)
            },
            rows: Mutex::new(vec![
                (ArtifactKind::Package, "npm"),
                (ArtifactKind::Package, "corepack"),
            ]),
            brew: None,
        });
        let session = Session::with_adapters_and_sizes(
            Arc::new(VecSink::new()),
            vec![brew, npm.clone()],
            None,
        );
        let env = HostEnv {
            path_dirs: vec![prefix.join("bin")],
            home: root.path("home"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let before = session.refresh(&env, &CheckOptions::default()).await;
        let old = session
            .issue_listed_plan(&uninstall("node@22"))
            .await
            .unwrap();
        assert!(old.plan.affected.is_empty());
        assert!(needed(&old.plan).is_empty());
        assert!(!old.plan.warnings.contains(&Warning::DependentsUnknown));
        assert!(
            matches!(&old.plan.action, crate::model::PlanAction::Command { args, .. } if args == &["uninstall", "--formula", "node@22"])
        );

        npm.rows
            .lock()
            .unwrap()
            .push((ArtifactKind::Package, "prettier"));
        let after = session.refresh(&env, &CheckOptions::default()).await;
        assert!(after.generation > before.generation);
        assert!(after
            .artifacts
            .iter()
            .any(|a| a.key.instance_id == NPM && a.key.name == "prettier"));
        let result = session.submit(old.id.clone());
        // Drain a mistakenly accepted operation too, so the mock command
        // count observes the regression rather than task scheduling.
        for _ in 0..100 {
            if !session.busy() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|argv| argv.get(1).is_some_and(|verb| verb == "uninstall"))
                .count(),
            0
        );
        assert_eq!(result, Err(SubmitError::Unknown));
        assert!(session.operations().is_empty());
        assert_eq!(session.submit(old.id), Err(SubmitError::Unknown));
        let fresh = session
            .issue_listed_plan(&uninstall("node@22"))
            .await
            .unwrap();
        assert_eq!(
            needed(&fresh.plan),
            vec![Warning::NeededBySource {
                instance_id: NPM.to_string(),
                program: true,
                tools: 1
            }]
        );
        assert_eq!(
            session.submit(fresh.id),
            Err(SubmitError::UninstallBlocked {
                reason: UninstallBlocked::NeededBySource
            })
        );
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

    #[tokio::test]
    async fn test_a_look_that_does_not_come_back_in_time_or_panics_is_not_waited_for() {
        // A step stuck in the kernel never returns to check the budget: the
        // preview stops waiting, and says the look did not finish.
        let done = crate::needed_by::NeededBy {
            warnings: Vec::new(),
            complete: true,
        };
        let quick = done.clone();
        assert_eq!(
            super::bounded(move || quick, Duration::from_secs(5)).await,
            Some(done.clone())
        );
        let started = std::time::Instant::now();
        let slow = done.clone();
        let stuck = super::bounded(
            move || {
                std::thread::sleep(Duration::from_millis(400));
                slow
            },
            Duration::from_millis(50),
        )
        .await;
        assert_eq!(stuck, None);
        assert!(
            started.elapsed() < Duration::from_millis(350),
            "it did not wait for the look"
        );
        let panicked = super::bounded(
            || -> crate::needed_by::NeededBy { panic!("a look that panics") },
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(panicked, None);
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
