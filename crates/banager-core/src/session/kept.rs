//! What an uninstall leaves behind, added to its preview by `issue_plan`
//! (`kept_data`). Its own file, like `sizes.rs`, so `plans.rs` only calls
//! it.

use super::Session;
use crate::adapters::AdapterError;
use crate::kept_data;
use crate::model::{InstalledArtifact, ManagerInstance, OpKind, OpRequest, Plan};
use std::path::Path;

/// The selected installed artifact, including its family and removal path; `None` for any other operation, and for an artifact
/// with none or not listed.
pub(super) fn subject_of_uninstall(
    artifacts: &[InstalledArtifact],
    req: &OpRequest,
) -> Option<InstalledArtifact> {
    if req.kind != OpKind::Uninstall {
        return None;
    }
    artifacts
        .iter()
        .find(|a| {
            a.key.instance_id == req.instance_id
                && a.key.kind == req.artifact_kind
                && a.key.name == req.name
        })
        .filter(|a| a.facts.family.is_some())
        .cloned()
}

impl Session {
    /// Remembers whose home folder the data an uninstall leaves behind is
    /// looked for in: the one the last refresh read. Only for a session
    /// that looks at the disk (`Session::new`, `with_adapters_and_sizes`),
    /// as with sizes, so a test refreshing a fake source never has a real
    /// home folder walked.
    pub(super) fn note_kept_data_home(&self, home: &Path) {
        if self.sizes.is_some() {
            *self.kept_data_home.lock().unwrap() = Some(home.to_path_buf());
        }
    }

    /// `plan`, with a `Warning::KeepsData` after its own warnings for each
    /// path a tool of `family` keeps its data in that is there and that
    /// the plan does not already name (`kept_data::kept_data`), measured on
    /// a blocking thread within `kept_data::BUDGET`. `plan` as it is with
    /// no family, before any refresh, or when that thread fails.
    pub(super) async fn with_kept_data(
        &self,
        mut plan: Plan,
        subject: Option<InstalledArtifact>,
        instance: &ManagerInstance,
    ) -> Result<Plan, AdapterError> {
        let Some(subject) = subject else {
            return Ok(plan);
        };
        let Some(home) = self.kept_data_home.lock().unwrap().clone() else {
            return Ok(plan);
        };
        let family = subject.facts.family.clone().expect("family of uninstall");
        let Some(roots) = kept_data::removal_roots(instance, &subject, &plan) else {
            return Ok(plan);
        };
        let named = kept_data::named_paths(&plan.warnings);
        let found = tokio::task::spawn_blocking(move || {
            kept_data::check_kept_paths(&home, &family, &named, &roots)?;
            Ok::<_, AdapterError>(kept_data::kept_data(
                &home,
                &family,
                &named,
                kept_data::BUDGET,
            ))
        })
        .await;
        if let Ok(kept) = found {
            plan.warnings.extend(kept?);
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, CaskStep, InstallReason, InstalledArtifact, KeptData, KeptWhat,
        ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, SearchHit, UninstallScope,
        Warning,
    };
    use crate::runner::HostEnv;
    use crate::session::Session;
    use async_trait::async_trait;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    /// A source of a test's own, under a real adapter id so the family
    /// table tags its package, whose plans carry `warnings`.
    struct Fake {
        meta: AdapterMeta,
        instance: ManagerInstance,
        kind: ArtifactKind,
        name: String,
        warnings: Vec<Warning>,
        path: Option<PathBuf>,
        recorded: Option<InstalledArtifact>,
        /// What `uv tool list --show-paths` printed for `recorded`, which
        /// the real uv plan reads again (its commands, `taken_command`).
        listed: String,
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
            if let Some(artifact) = &self.recorded {
                return Ok(vec![artifact.clone()]);
            }
            Ok(vec![InstalledArtifact {
                key: ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: self.kind,
                    name: self.name.clone(),
                },
                display_name: self.name.clone(),
                version: "1.0.0".to_string(),
                reason: InstallReason::Requested,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path: self.path.clone(),
                auto_updates: false,
                uninstall_blocked: None,
                facts: Default::default(),
            }])
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
            let mut plan = test_support::fake_plan(inst, req);
            if self.recorded.is_some() && self.meta.id == "uv" {
                // Real uv uninstall planning, with a runner that can never
                // spawn a package manager on the host.
                let runner = Arc::new(crate::runner::MockRunner::new());
                runner.respond(
                    vec![
                        inst.exe_path.to_str().unwrap(),
                        "tool",
                        "list",
                        "--show-paths",
                    ],
                    crate::runner::CommandOutput {
                        stderr_cause: Default::default(),
                        exit_code: Some(0),
                        stdout: self.listed.clone(),
                        stderr: String::new(),
                        timed_out: false,
                        cancelled: false,
                    },
                );
                let adapter = crate::adapters::uv::UvAdapter::new(runner);
                plan = adapter.plan(inst, req).await?;
            }
            plan.warnings.extend(self.warnings.clone());
            Ok(plan)
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

    fn fake(adapter_id: &str, kind: ArtifactKind, name: &str, warnings: Vec<Warning>) -> Arc<Fake> {
        Arc::new(Fake {
            meta: test_support::fake_adapter_meta(adapter_id),
            instance: test_support::make_instance(adapter_id, &format!("{adapter_id}:1")),
            kind,
            name: name.to_string(),
            warnings,
            path: None,
            recorded: None,
            listed: String::new(),
        })
    }

    /// A home folder of a test's own, removed when dropped.
    struct Home(PathBuf);

    impl Home {
        fn new(tag: &str) -> Home {
            let dir = std::env::temp_dir().join(format!(
                "banager-kept-session-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Home(std::fs::canonicalize(&dir).unwrap())
        }

        fn file(&self, relative: &str, len: usize) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, vec![1u8; len]).unwrap();
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn request(adapter_id: &str, op: OpKind, kind: ArtifactKind, name: &str) -> OpRequest {
        OpRequest {
            kind: op,
            instance_id: format!("{adapter_id}:1"),
            artifact_kind: kind,
            name: name.to_string(),
        }
    }

    fn kept(plan: &Plan) -> Vec<(String, KeptData, bool)> {
        plan.warnings
            .iter()
            .filter_map(|w| match w {
                Warning::KeepsData {
                    path, what, size, ..
                } => Some((path.clone(), *what, size.is_some())),
                _ => None,
            })
            .collect()
    }

    async fn session_over(mut adapter: Arc<Fake>, home: &Path, sizes: bool) -> Arc<Session> {
        // A source left at the fixture's prefix, `/`, is given one under
        // the test's own home: a refresh reads the bin folders of every
        // Homebrew and npm prefix (`commands::bin_folders`) and the uninstall
        // preview looks under the prefix, and `/bin` and `/sbin` are the
        // Mac's own.
        match Arc::get_mut(&mut adapter) {
            Some(inner) if inner.instance.prefix == Path::new("/") => {
                inner.instance.prefix = home.join("prefix");
            }
            Some(_) => {}
            None => assert_ne!(
                adapter.instance.prefix,
                Path::new("/"),
                "a fake the test still holds gives itself a prefix"
            ),
        }
        let sink = Arc::new(VecSink::new());
        let adapters: Vec<Arc<dyn Adapter>> = vec![adapter];
        let session = if sizes {
            Session::with_adapters_and_sizes(sink, adapters, None)
        } else {
            Session::with_adapters(sink, adapters, None)
        };
        let env = HostEnv {
            path_dirs: Vec::new(),
            home: home.to_path_buf(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        session.refresh(&env, &CheckOptions::default()).await;
        session
    }

    #[tokio::test]
    async fn test_data_link_into_removed_environment_is_not_promised_kept() {
        use std::os::unix::fs::symlink;
        for adapter_id in ["uv", "pipx", "npm", "brew"] {
            let home = Home::new(adapter_id);
            let (kind, name, root, data) = match adapter_id {
                "uv" => (
                    ArtifactKind::Tool,
                    "mistral-vibe",
                    ".local/share/uv/tools/mistral-vibe",
                    ".vibe",
                ),
                "pipx" => (
                    ArtifactKind::Tool,
                    "mistral-vibe",
                    ".local/share/pipx/venvs/mistral-vibe",
                    ".vibe",
                ),
                "npm" => (
                    ArtifactKind::Package,
                    "@openai/codex",
                    "prefix/lib/node_modules/@openai/codex",
                    ".codex",
                ),
                _ => (
                    ArtifactKind::Formula,
                    "ollama",
                    "prefix/Cellar/ollama/1.0",
                    ".ollama/models",
                ),
            };
            home.file(&format!("{root}/user-data/config"), 32);
            let data_path = home.0.join(data);
            std::fs::create_dir_all(data_path.parent().unwrap()).unwrap();
            symlink(home.0.join(root).join("user-data"), &data_path).unwrap();
            let mut adapter = fake(adapter_id, kind, name, vec![]);
            let inner = Arc::get_mut(&mut adapter).unwrap();
            inner.path = (adapter_id != "npm").then(|| home.0.join(root));
            inner.instance.prefix = home.0.join("prefix");
            if adapter_id == "uv" {
                inner.listed = format!(
                    "mistral-vibe v1.0.0 ({})\n- vibe ({})\n",
                    home.0.join(root).display(),
                    home.0.join(".local/bin/vibe").display()
                );
                inner.recorded = crate::adapters::uv::parse_tool_list_show_paths(
                    &inner.listed,
                    &inner.instance.id,
                )
                .pop();
            } else if adapter_id == "pipx" {
                let json = serde_json::json!({"venvs": {"mistral-vibe": {"metadata": {"main_package": {
                    "package": "mistral-vibe", "package_version": "1.0.0",
                    "app_paths": [{"__Path__": home.0.join(root).join("bin/vibe"), "__type__": "Path"}]
                }}}}});
                inner.recorded =
                    crate::adapters::pipx::parse_list(&json.to_string(), &inner.instance.id)
                        .unwrap()
                        .pop();
            } else if adapter_id == "npm" {
                let json =
                    serde_json::json!({"dependencies": {"@openai/codex": {"version": "1.0.0"}}});
                inner.recorded =
                    crate::adapters::npm::parse_ls_global(&json.to_string(), &inner.instance.id)
                        .unwrap()
                        .pop();
            } else if adapter_id == "brew" {
                let json = serde_json::json!({"formulae": [{"name": "ollama", "installed": [{"version": "1.0"}]}], "casks": []});
                inner.recorded = crate::adapters::brew::parse::parse_info_installed(
                    &json.to_string(),
                    &inner.instance.id,
                )
                .unwrap()
                .pop();
            }
            let session = session_over(adapter, &home.0, true).await;
            let result = session
                .issue_plan(&request(adapter_id, OpKind::Uninstall, kind, name))
                .await;
            assert!(
                matches!(result, Err(AdapterError::UninstallUnsafe { .. })),
                "{adapter_id}: {result:?}"
            );
            assert!(data_path.join("config").is_file());
        }
    }

    /// A data folder kept in `~/Documents` (or iCloud Drive, Dropbox) and
    /// linked from its usual place is nowhere an uninstall deletes: it is
    /// still said to stay, unmeasured, and never refuses the uninstall --
    /// while one whose way there runs through what goes is refused.
    #[tokio::test]
    async fn test_a_data_folder_kept_in_a_protected_place_still_uninstalls() {
        use std::os::unix::fs::symlink;
        let home = Home::new("documents");
        home.file("Documents/claude-data/settings.json", 4_000);
        symlink(home.0.join("Documents/claude-data"), home.0.join(".claude")).unwrap();
        let name = "@anthropic-ai/claude-code";
        let mut adapter = fake("npm", ArtifactKind::Package, name, vec![]);
        Arc::get_mut(&mut adapter).unwrap().instance.prefix = home.0.join("prefix");
        let session = session_over(adapter.clone(), &home.0, true).await;
        let issued = session
            .issue_plan(&request(
                "npm",
                OpKind::Uninstall,
                ArtifactKind::Package,
                name,
            ))
            .await
            .unwrap();
        assert_eq!(
            kept(&issued.plan),
            vec![("~/.claude".to_string(), KeptData::ToolData, false)]
        );

        // The same folder reached through a link inside npm's package.
        let package = home
            .0
            .join("prefix/lib/node_modules/@anthropic-ai/claude-code");
        std::fs::create_dir_all(&package).unwrap();
        symlink(home.0.join("Documents/claude-data"), package.join("data")).unwrap();
        std::fs::remove_file(home.0.join(".claude")).unwrap();
        symlink(package.join("data"), home.0.join(".claude")).unwrap();
        let refused = session
            .issue_plan(&request(
                "npm",
                OpKind::Uninstall,
                ArtifactKind::Package,
                name,
            ))
            .await;
        assert!(
            matches!(refused, Err(AdapterError::UninstallUnsafe { .. })),
            "{refused:?}"
        );
    }

    /// Ollama's app as Homebrew records it (`ollama-app`: `uninstall
    /// launchctl: "com.ollama.ollama", quit: "com.electron.ollama"`, then
    /// `app` and `binary`): a cask with steps that delete no files of the
    /// user's still says the models stay. One whose steps run a program or
    /// delete paths says nothing of what stays, and is not refused.
    #[tokio::test]
    async fn test_a_cask_with_steps_that_delete_no_files_still_names_the_models() {
        let home = Home::new("ollama-app");
        home.file(".ollama/models/blobs/sha256-a", 9_000);
        let steps = |scope, step, items: &[&str]| {
            vec![
                Warning::UninstallScope { what: scope },
                Warning::CaskUninstallStep {
                    step,
                    items: items.iter().map(|item| item.to_string()).collect(),
                    only_if: None,
                },
                Warning::CaskUninstallStep {
                    step: CaskStep::QuitsApps,
                    items: vec!["com.electron.ollama".to_string()],
                    only_if: None,
                },
            ]
        };
        for (warnings, named) in [
            (
                steps(
                    UninstallScope::HomebrewCaskStepsAutoremoves,
                    CaskStep::RemovesServices,
                    &["com.ollama.ollama"],
                ),
                true,
            ),
            (
                steps(
                    UninstallScope::HomebrewCaskStepsUnseen,
                    CaskStep::RunsScript,
                    &["/Applications/Ollama.app/uninstall.sh"],
                ),
                false,
            ),
            (
                steps(
                    UninstallScope::HomebrewCaskSteps,
                    CaskStep::Deletes,
                    &["~/.ollama"],
                ),
                false,
            ),
        ] {
            let session = session_over(
                fake("brew", ArtifactKind::Cask, "ollama-app", warnings.clone()),
                &home.0,
                true,
            )
            .await;
            let issued = session
                .issue_plan(&request(
                    "brew",
                    OpKind::Uninstall,
                    ArtifactKind::Cask,
                    "ollama-app",
                ))
                .await
                .unwrap();
            let expected = if named {
                vec![("~/.ollama/models".to_string(), KeptData::Models, true)]
            } else {
                Vec::new()
            };
            assert_eq!(kept(&issued.plan), expected, "{warnings:?}");
        }
    }

    #[tokio::test]
    async fn test_uninstalling_npm_claude_code_names_its_folders_that_are_there() {
        let home = Home::new("npm");
        home.file(".claude/settings.json", 4_000);
        home.file(".claude.json", 100);
        let name = "@anthropic-ai/claude-code";
        let session = session_over(
            fake("npm", ArtifactKind::Package, name, vec![]),
            &home.0,
            true,
        )
        .await;

        let issued = session
            .issue_plan(&request(
                "npm",
                OpKind::Uninstall,
                ArtifactKind::Package,
                name,
            ))
            .await
            .unwrap();
        assert_eq!(
            kept(&issued.plan),
            vec![
                ("~/.claude".to_string(), KeptData::ToolData, true),
                ("~/.claude.json".to_string(), KeptData::ToolData, true),
            ]
        );

        // An upgrade keeps everything as a matter of course: nothing said.
        let upgrade = session
            .issue_plan(&request(
                "npm",
                OpKind::Upgrade,
                ArtifactKind::Package,
                name,
            ))
            .await
            .unwrap();
        assert!(kept(&upgrade.plan).is_empty());
    }

    #[tokio::test]
    async fn test_uninstalling_homebrews_ollama_names_the_models_folder() {
        let home = Home::new("ollama");
        home.file(".ollama/models/blobs/sha256-a", 9_000);
        for (kind, name) in [
            (ArtifactKind::Formula, "ollama"),
            (ArtifactKind::Cask, "ollama-app"),
        ] {
            let session = session_over(fake("brew", kind, name, vec![]), &home.0, true).await;
            let issued = session
                .issue_plan(&request("brew", OpKind::Uninstall, kind, name))
                .await
                .unwrap();
            assert_eq!(
                kept(&issued.plan),
                vec![("~/.ollama/models".to_string(), KeptData::Models, true)],
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn test_a_tool_with_no_family_or_no_folder_gets_no_line() {
        let home = Home::new("none");
        home.file(".claude/settings.json", 10);
        // jq is no AI tool.
        let session = session_over(
            fake("brew", ArtifactKind::Formula, "jq", vec![]),
            &home.0,
            true,
        )
        .await;
        let issued = session
            .issue_plan(&request(
                "brew",
                OpKind::Uninstall,
                ArtifactKind::Formula,
                "jq",
            ))
            .await
            .unwrap();
        assert!(kept(&issued.plan).is_empty());
        // Codex, with no `~/.codex` in this home.
        let session = session_over(
            fake("npm", ArtifactKind::Package, "@openai/codex", vec![]),
            &home.0,
            true,
        )
        .await;
        let issued = session
            .issue_plan(&request(
                "npm",
                OpKind::Uninstall,
                ArtifactKind::Package,
                "@openai/codex",
            ))
            .await
            .unwrap();
        assert!(kept(&issued.plan).is_empty());
    }

    #[tokio::test]
    async fn test_the_standalone_list_already_naming_a_folder_is_not_repeated() {
        let home = Home::new("standalone");
        home.file(".claude/settings.json", 10);
        home.file(".claude.json", 10);
        let own = vec![
            Warning::WillKeep {
                path: "~/.claude".to_string(),
                what: KeptWhat::SettingsAndHistory,
            },
            Warning::WillKeep {
                path: "~/.claude.json".to_string(),
                what: KeptWhat::Settings,
            },
        ];
        let session = session_over(
            fake(
                "standalone-claude",
                ArtifactKind::Binary,
                "claude",
                own.clone(),
            ),
            &home.0,
            true,
        )
        .await;
        let issued = session
            .issue_plan(&request(
                "standalone-claude",
                OpKind::Uninstall,
                ArtifactKind::Binary,
                "claude",
            ))
            .await
            .unwrap();
        assert_eq!(issued.plan.warnings, own);
    }

    #[tokio::test]
    async fn test_a_session_that_does_not_look_at_the_disk_adds_nothing() {
        let home = Home::new("off");
        home.file(".codex/config.toml", 10);
        let session = session_over(
            fake("npm", ArtifactKind::Package, "@openai/codex", vec![]),
            &home.0,
            false,
        )
        .await;
        let issued = session
            .issue_plan(&request(
                "npm",
                OpKind::Uninstall,
                ArtifactKind::Package,
                "@openai/codex",
            ))
            .await
            .unwrap();
        assert!(kept(&issued.plan).is_empty());
    }
}
