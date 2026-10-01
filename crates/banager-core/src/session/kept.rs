//! What an uninstall leaves behind, added to its preview by `issue_plan`
//! (`kept_data`). Its own file, like `sizes.rs`, so `plans.rs` only calls
//! it.

use super::Session;
use crate::kept_data;
use crate::model::{InstalledArtifact, OpKind, OpRequest, Plan};
use std::path::Path;

/// The family of the artifact `req` uninstalls, as the snapshot tags it
/// (`facts.family`); `None` for any other operation, and for an artifact
/// with none or not listed.
pub(super) fn family_of_uninstall(
    artifacts: &[InstalledArtifact],
    req: &OpRequest,
) -> Option<String> {
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
        .and_then(|a| a.facts.family.clone())
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
    pub(super) async fn with_kept_data(&self, mut plan: Plan, family: Option<String>) -> Plan {
        let Some(family) = family else {
            return plan;
        };
        let Some(home) = self.kept_data_home.lock().unwrap().clone() else {
            return plan;
        };
        let named = kept_data::named_paths(&plan.warnings);
        let found = tokio::task::spawn_blocking(move || {
            kept_data::kept_data(&home, &family, &named, kept_data::BUDGET)
        })
        .await;
        if let Ok(kept) = found {
            plan.warnings.extend(kept);
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
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, KeptData, KeptWhat,
        ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, SearchHit, Warning,
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
                path: None,
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
            plan.warnings = self.warnings.clone();
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

    async fn session_over(adapter: Arc<Fake>, home: &Path, sizes: bool) -> Arc<Session> {
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
