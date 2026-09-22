use crate::adapters::{
    run_plan, second_token, validate_package_name, Adapter, AdapterError, AdapterMeta,
    Capabilities, CheckOptions,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Shared preamble for both of `uv tool list`'s text formats (`--show-paths`
/// and `--outdated`): a completely empty body or the literal `No tools
/// installed` yields no lines at all, `- binary (path)` lines and blank
/// lines are skipped, and each remaining header line is split into its
/// `name` and the `v`-prefixed remainder with the `v` stripped. A header
/// line that doesn't fit that shape (no space, or no `v` prefix) is skipped
/// here too; each caller applies its own further parsing to `rest` and
/// skips on its own mismatches.
fn tool_list_header_lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    let trimmed = text.trim();
    let body = if trimmed.is_empty() || trimmed == "No tools installed" {
        ""
    } else {
        text
    };
    body.lines().filter_map(|line| {
        if line.starts_with("- ") || line.trim().is_empty() {
            return None;
        }
        let (name, rest) = line.split_once(' ')?;
        let rest = rest.strip_prefix('v')?;
        Some((name, rest))
    })
}

/// Parses `uv tool list --show-paths`: one `name vX.Y.Z (path)` header line
/// per tool, followed by `- binary (path)` lines that this function skips
/// (the header alone has everything `InstalledArtifact` needs).
fn parse_tool_list_show_paths(text: &str, instance_id: &str) -> Vec<InstalledArtifact> {
    tool_list_header_lines(text)
        .filter_map(|(name, rest)| {
            let (version, path_part) = rest.split_once(" (")?;
            let path = path_part.trim_end_matches(')');
            Some(InstalledArtifact {
                key: ArtifactKey {
                    instance_id: instance_id.to_string(),
                    kind: ArtifactKind::Tool,
                    name: name.to_string(),
                },
                display_name: name.to_string(),
                version: version.to_string(),
                reason: InstallReason::Requested,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path: Some(PathBuf::from(path)),
                auto_updates: false,
            })
        })
        .collect()
}

/// Parses `uv tool list --outdated`: `name vOLD [latest: NEW]` per outdated
/// tool, followed by its `- binary` lines (skipped). With nothing outdated
/// this command prints **nothing at all** — not a message, not a newline —
/// and with no tools installed at all it prints `No tools installed`; both
/// are treated as "no updates", never an error (this phase's documented
/// trap for uv).
fn parse_tool_list_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate> {
    tool_list_header_lines(text)
        .filter_map(|(name, rest)| {
            let (old, bracket) = rest.split_once(" [latest: ")?;
            let new = bracket.strip_suffix(']')?;
            Some(UpdateCandidate {
                key: ArtifactKey {
                    instance_id: instance_id.to_string(),
                    kind: ArtifactKind::Tool,
                    name: name.to_string(),
                },
                current: old.to_string(),
                target: new.to_string(),
                channel: UpdateChannel::Native,
                checkable: true,
                warnings: Vec::new(),
            })
        })
        .collect()
}

pub struct UvAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
}

impl UvAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>) -> UvAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/uv.toml"))
            .expect("adapters/meta/uv.toml must parse");
        UvAdapter { runner, meta }
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("uv", env) else {
            return Vec::new();
        };
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: exe_path.clone(),
                    args: vec!["--version".to_string()],
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(30),
                },
                None,
                CancellationToken::new(),
            )
            .await;
        let version = match output {
            // `uv --version` prints "uv X.Y.Z (...)" — the shared
            // second-token rule (crate::adapters::second_token, Task 5).
            Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
            _ => None,
        };
        let prefix = exe_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: "uv".to_string(),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            healthy: version.is_some(),
            version,
            unverified_version,
            read_only_reason: None,
        }]
    }

    async fn run_uv(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: Vec::new(),
            cwd: None,
            timeout,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_uv(
                inst,
                vec![
                    "tool".to_string(),
                    "list".to_string(),
                    "--show-paths".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        Ok(parse_tool_list_show_paths(&output.stdout, &inst.id))
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let output = self
            .run_uv(
                inst,
                vec![
                    "tool".to_string(),
                    "list".to_string(),
                    "--outdated".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        Ok(parse_tool_list_outdated(&output.stdout, &inst.id))
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "uv has no tool-search command; browse PyPI directly".to_string(),
        ))
    }

    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        if req.instance_id != inst.id {
            return Err(AdapterError::Refused(format!(
                "plan requested for instance {} but given instance {}",
                req.instance_id, inst.id
            )));
        }
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["tool".to_string(), "install".to_string(), req.name.clone()],
            OpKind::Uninstall => vec![
                "tool".to_string(),
                "uninstall".to_string(),
                req.name.clone(),
            ],
            OpKind::Upgrade => vec!["tool".to_string(), "upgrade".to_string(), req.name.clone()],
        };
        Ok(Plan {
            request: req.clone(),
            program: inst.exe_path.clone(),
            args,
            env: Vec::new(),
            needs_password: false,
            locks: vec![lock],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 600,
        })
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        run_plan(&self.runner, plan, sink, op_id, cancel).await
    }

    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        match artifacts
            .into_iter()
            .find(|a| a.key.kind == key.kind && a.key.name == key.name)
        {
            Some(a) => Ok(Reconciled {
                present: true,
                version: Some(a.version),
            }),
            None => Ok(Reconciled {
                present: false,
                version: None,
            }),
        }
    }
}

#[async_trait]
impl Adapter for UvAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // No upgrade-all: OpKind has no variant for it (see this task's
            // Interfaces block).
            search: false,
            per_item_upgrade: true,
            upgrade_all: false,
            uninstall: true,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        UvAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        UvAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        UvAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        UvAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        UvAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        UvAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        UvAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_token_reads_uvs_recorded_version_line() {
        // adapters/fixtures/uv/0.12.17/version.txt:
        // "uv 0.12.17 (Homebrew 2026-09-18 aarch64-apple-darwin)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against uv's real recorded output.
        assert_eq!(
            second_token("uv 0.12.17 (Homebrew 2026-09-18 aarch64-apple-darwin)\n"),
            Some("0.12.17".to_string())
        );
    }

    #[test]
    fn test_parse_tool_list_show_paths_from_the_recorded_fixture() {
        let text =
            std::fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-show-paths.txt")
                .expect("read uv tool-list-show-paths.txt fixture");
        let artifacts = parse_tool_list_show_paths(&text, "uv");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Tool);
        assert_eq!(artifacts[0].key.name, "ruff");
        assert_eq!(artifacts[0].version, "0.15.0");
        assert_eq!(
            artifacts[0].path,
            Some(PathBuf::from("/Users/brulek/.local/share/uv/tools/ruff"))
        );
    }

    #[test]
    fn test_parse_tool_list_show_paths_of_no_tools_installed_is_empty() {
        // Not in the recorded fixture (that machine has ruff installed) but
        // documented in adapters/fixtures/uv/0.12.17/README.md.
        assert!(parse_tool_list_show_paths("No tools installed\n", "uv").is_empty());
    }

    #[test]
    fn test_parse_tool_list_outdated_from_the_recorded_fixture() {
        let text =
            std::fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-outdated.txt")
                .expect("read uv tool-list-outdated.txt fixture");
        let candidates = parse_tool_list_outdated(&text, "uv");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "ruff");
        assert_eq!(candidates[0].current, "0.15.0");
        assert_eq!(candidates[0].target, "0.16.8");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[test]
    fn test_parse_tool_list_outdated_of_empty_output_is_empty() {
        // "uv tool list --outdated prints nothing at all" when nothing is
        // outdated — no message, not even a newline (this phase's
        // documented trap for uv).
        assert!(parse_tool_list_outdated("", "uv").is_empty());
    }

    use crate::events::VecSink;
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "uv".to_string(),
            adapter_id: "uv".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/uv"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            scope: Scope::User,
            version: Some("0.12.17".to_string()),
            healthy: true,
            unverified_version: None,
            read_only_reason: None,
        }
    }

    #[tokio::test]
    async fn test_check_updates_calls_tool_list_outdated_and_parses_the_fixture_output() {
        let text =
            std::fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-outdated.txt")
                .expect("read uv tool-list-outdated.txt fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--outdated"],
            CommandOutput {
                exit_code: Some(0),
                stdout: text,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "ruff");
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter = UvAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-uv".to_string(),
            artifact_kind: ArtifactKind::Tool,
            name: "ruff".to_string(),
        };
        let result = UvAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_uninstall_upgrade_build_the_expected_argv() {
        let adapter = UvAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        for (kind, expected) in [
            (OpKind::Install, vec!["tool", "install", "ruff"]),
            (OpKind::Uninstall, vec!["tool", "uninstall", "ruff"]),
            (OpKind::Upgrade, vec!["tool", "upgrade", "ruff"]),
        ] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Tool,
                name: "ruff".to_string(),
            };
            let plan = UvAdapter::plan(&adapter, &inst, &req).await.expect("plan");
            assert_eq!(plan.args, expected);
            assert!(!plan.needs_password);
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "install", "ruff"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installed ruff\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: "ruff".to_string(),
        };
        let plan = UvAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome =
            UvAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--show-paths"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "ruff v0.15.0 (/Users/brulek/.local/share/uv/tools/ruff)\n- ruff (/Users/brulek/.local/bin/ruff)\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let inst = test_instance();
        let present = UvAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Tool,
                name: "ruff".to_string(),
            },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("0.15.0".to_string()));
        let absent = UvAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Tool,
                name: "missing".to_string(),
            },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter = UvAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        let result = <UvAdapter as Adapter>::search(&adapter, &inst, "ruff").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }
}
