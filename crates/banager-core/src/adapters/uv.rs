use crate::adapters::{
    ensure_instance_match, lookup_failure_reason, reconcile_from, run_plan, second_token,
    uncheckable_from_inventory, validate_package_name, Adapter, AdapterError, AdapterMeta,
    CheckOptions, CheckOutcome, LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactFacts, ArtifactKey, ArtifactKind, CancelPolicy, CommandInputs, InstallReason,
    InstalledArtifact, InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan,
    PlanAction, ProvidedCommand, Reconciled, ResourceLock, Scope, SearchHit, Unavailable,
    UninstallBlocked, UninstallScope, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use std::collections::HashMap;
use std::ffi::OsString;
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
/// per tool, followed by one `- binary (path)` line per executable the
/// tool installed. The header has everything `InstalledArtifact` needs;
/// the binary lines are the tool's commands (`CommandInputs.provided`),
/// each with the tool's environment as where its link must lead -- pipx
/// can have put a `ruff` at the same path. They are read here alone:
/// `tool_list_header_lines`, which `--outdated`'s parser shares, still
/// skips them, and this function reads a header the way it does.
pub(crate) fn parse_tool_list_show_paths(text: &str, instance_id: &str) -> Vec<InstalledArtifact> {
    // Each `- binary (path)` line, with the tool whose header it follows.
    // A line with no path (uv without `--show-paths`) names no file.
    let mut binaries: Vec<(&str, &str, &str)> = Vec::new();
    let mut tool: Option<&str> = None;
    for line in text.lines() {
        if let Some(binary) = line.strip_prefix("- ") {
            let entry = binary
                .split_once(" (")
                .and_then(|(command, path)| Some((command, path.strip_suffix(')')?)));
            if let (Some(tool), Some((command, path))) = (tool, entry) {
                binaries.push((tool, command, path));
            }
        } else if !line.trim().is_empty() {
            tool = line
                .split_once(' ')
                .filter(|(_, rest)| rest.starts_with('v'))
                .map(|(name, _)| name);
        }
    }
    // Each tool's binaries, in order, looked up by name: filtering the
    // whole list for every tool was quadratic, 56 s over 100,000 tools in
    // a debug build.
    let mut binaries_of: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for (tool, command, link) in binaries {
        binaries_of.entry(tool).or_default().push((command, link));
    }
    let artifacts = tool_list_header_lines(text)
        .filter_map(|(name, rest)| {
            let (version, path_part) = rest.split_once(" (")?;
            let path = path_part.trim_end_matches(')');
            let provided = binaries_of
                .get(name)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|(command, link)| ProvidedCommand {
                    name: command.to_string(),
                    path: PathBuf::from(link),
                    within: vec![PathBuf::from(path)],
                })
                .collect();
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
                uninstall_blocked: None,
                facts: ArtifactFacts {
                    command_inputs: CommandInputs {
                        provided,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            })
        })
        .collect();
    crate::adapters::sanity::artifacts(artifacts)
}

/// Parses `uv tool list --outdated`: `name vOLD [latest: NEW]` per outdated
/// tool, followed by its `- binary` lines (skipped). With nothing outdated
/// this command prints **nothing at all** — not a message, not a newline —
/// and with no tools installed at all it prints `No tools installed`; both
/// are treated as "no updates", never an error (this phase's documented
/// trap for uv).
pub(crate) fn parse_tool_list_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate> {
    let candidates = tool_list_header_lines(text)
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
                blocked: None,
            })
        })
        .collect();
    crate::adapters::sanity::candidates(candidates)
}

pub struct UvAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// How to read `UV_TOOL_DIR` from Banager's own environment, which
    /// every `uv` command inherits (a uv plan adds no variables): read at
    /// every inventory and every uninstall preview (`uninstall_blocked`).
    /// The same fn-pointer seam as `BrewAdapter::askpass_fn`; inside this
    /// crate's unit tests it reads as unset unless a test sets it
    /// (`with_tool_dir_fn`), so that no test answers differently on a Mac
    /// whose environment sets it.
    tool_dir_fn: fn() -> Option<OsString>,
}

/// `UvAdapter::tool_dir_fn` as `UvAdapter::new` sets it: Banager's real
/// environment in every build but this crate's unit tests.
#[cfg(not(test))]
const DEFAULT_TOOL_DIR_FN: fn() -> Option<OsString> = || std::env::var_os("UV_TOOL_DIR");
#[cfg(test)]
const DEFAULT_TOOL_DIR_FN: fn() -> Option<OsString> = || None;

impl UvAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>) -> UvAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/uv.toml"))
            .expect("adapters/meta/uv.toml must parse");
        UvAdapter {
            runner,
            meta,
            tool_dir_fn: DEFAULT_TOOL_DIR_FN,
        }
    }

    /// Test-only hook to set what `UV_TOOL_DIR` reads as (see
    /// `tool_dir_fn`).
    #[cfg(test)]
    fn with_tool_dir_fn(mut self, tool_dir_fn: fn() -> Option<OsString>) -> UvAdapter {
        self.tool_dir_fn = tool_dir_fn;
        self
    }

    /// Why Banager uninstalls no uv tool here, or `None`: `UV_TOOL_DIR` set
    /// and not empty, which is how uv reads it (`InstalledTools::from_settings`
    /// filters an empty one out, uv 0.12.17 `crates/uv-tool/src/lib.rs:133`).
    /// uv then keeps its tools in that folder, and removing the last one
    /// deletes the folder above it too, with every file in it, when that
    /// holds no other folder (`UninstallBlocked::UvToolDirSet`).
    fn uninstall_blocked(&self) -> Option<UninstallBlocked> {
        (self.tool_dir_fn)()
            .filter(|dir| !dir.is_empty())
            .map(|_| UninstallBlocked::UvToolDirSet)
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
                    output_use: OutputUse::Parsed,
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
            id: crate::model::instance_id(&self.meta.id, None),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            status: InstanceStatus {
                // The state axis. `version` is `None` exactly when the
                // CLI is on PATH but `--version` would not run or could
                // not be parsed: the tool is there, it just did not
                // answer.
                unavailable: version.is_none().then_some(Unavailable::NotResponding),
                notes: Vec::new(),
            },
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
            // Every caller of this helper hands the result to a parser.
            output_use: OutputUse::Parsed,
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
        let uninstall_blocked = self.uninstall_blocked();
        Ok(parse_tool_list_show_paths(&output.stdout, &inst.id)
            .into_iter()
            .map(|artifact| InstalledArtifact {
                uninstall_blocked,
                ..artifact
            })
            .collect())
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
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
        // An index that did not answer is not a failed source: every tool
        // gets a `checkable: false` row carrying the reason, the same
        // answer cargo, pip, pipx and npm give. See
        // `uncheckable_from_inventory`.
        if output.exit_code != Some(0) {
            let failure = LookupFailure::words(
                lookup_failure_reason("uv tool list --outdated", output.exit_code, &output.stderr),
                &output.stderr,
            );
            let installed = self.inventory(inst).await?;
            return Ok(
                uncheckable_from_inventory(&installed, UpdateChannel::Native, &failure).into(),
            );
        }
        Ok(parse_tool_list_outdated(&output.stdout, &inst.id).into())
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
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        // The gate's late twin (`blocked_uninstall` in session/plans.rs
        // refuses the same row from the snapshot): read again here, so no
        // preview of `uv tool uninstall` is ever built while uv would take
        // the folder above its tools folder with it.
        if req.kind == OpKind::Uninstall {
            if let Some(reason) = self.uninstall_blocked() {
                return Err(AdapterError::UninstallBlocked { reason });
            }
        }
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
        // What `uv tool uninstall` removes and leaves (uv 0.12.17
        // `crates/uv/src/commands/tool/uninstall.rs:187-226`: the tool's
        // environment and the executables its receipt records), said under
        // the tool -- only here, past the refusal above: with `UV_TOOL_DIR`
        // set there is no plan to say it of.
        let warnings = match req.kind {
            OpKind::Uninstall => vec![Warning::UninstallScope {
                what: UninstallScope::Uv,
            }],
            OpKind::Install | OpKind::Upgrade => Vec::new(),
        };
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args,
                env: Vec::new(),
            },
            needs_password: false,
            locks: vec![lock],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings,
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
        Ok(reconcile_from(artifacts, key))
    }
}

#[async_trait]
impl Adapter for UvAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
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
    ) -> Result<CheckOutcome, AdapterError> {
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

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_parse_tool_list_show_paths_reads_a_hundred_thousand_tools_in_linear_time() {
        let text: String = (0..100_000)
            .map(|i| format!("t{i} v1.{i} (/u/t{i})\n- b{i} (/u/bin/b{i})\n"))
            .collect();
        let started = std::time::Instant::now();
        let artifacts = parse_tool_list_show_paths(&text, "uv:/x");
        assert_eq!(artifacts.len(), 100_000);
        assert_eq!(
            artifacts[99_999].facts.command_inputs.provided[0].name,
            "b99999"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn regression_uv_parsers_drop_a_control_character() {
        let shown = parse_tool_list_show_paths("ruff v0.15\u{e}0 (/u/ruff)\n", "uv:/x");
        assert_eq!(shown[0].version, "");
        let outdated = parse_tool_list_outdated(
            "r\rff v0.15.0 [latest: 0.16.8]\nruff v0.15.0 [latest: 0.16\u{e}8]\nty v0.1 [latest: 0.2]\n",
            "uv:/x",
        );
        let names: Vec<String> = outdated.into_iter().map(|c| c.key.name).collect();
        assert_eq!(names, vec!["ty".to_string()]);
    }
    use crate::model::Warning;
    use crate::testing::command_args;

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
        // Its `- ruff (…)` line: the command, its link, and the tool's
        // environment as where that link must lead.
        assert_eq!(
            artifacts[0].facts.command_inputs.provided,
            vec![ProvidedCommand {
                name: "ruff".to_string(),
                path: PathBuf::from("/Users/brulek/.local/bin/ruff"),
                within: vec![PathBuf::from("/Users/brulek/.local/share/uv/tools/ruff")],
            }]
        );
    }

    #[test]
    fn test_parse_tool_list_show_paths_gives_each_tool_its_own_binary_lines() {
        // Two tools, one with two executables; a binary line with no path
        // (uv without `--show-paths`) names no file.
        let text = "\
black v25.9.0 (/Users/someone/.local/share/uv/tools/black)
- black (/Users/someone/.local/bin/black)
- blackd (/Users/someone/.local/bin/blackd)
ruff v0.15.0 (/Users/someone/.local/share/uv/tools/ruff)
- ruff
";
        let artifacts = parse_tool_list_show_paths(text, "uv");
        let commands: Vec<(&str, Vec<(&str, &std::path::Path)>)> = artifacts
            .iter()
            .map(|a| {
                (
                    a.key.name.as_str(),
                    a.facts
                        .command_inputs
                        .provided
                        .iter()
                        .map(|p| (p.name.as_str(), p.path.as_path()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            commands,
            vec![
                (
                    "black",
                    vec![
                        (
                            "black",
                            std::path::Path::new("/Users/someone/.local/bin/black")
                        ),
                        (
                            "blackd",
                            std::path::Path::new("/Users/someone/.local/bin/blackd")
                        ),
                    ]
                ),
                ("ruff", Vec::new()),
            ]
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
            exe_path: PathBuf::from("/opt/homebrew/bin/uv"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            version: Some("0.12.17".to_string()),
            ..crate::testing::manager_instance("uv", "uv")
        }
    }

    #[tokio::test]
    async fn test_inventory_keeps_the_commands_its_parse_read() {
        // `inventory` puts this round's `uninstall_blocked` on each row;
        // the commands the parse read have to come through it.
        let fixture =
            std::fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-show-paths.txt")
                .expect("read uv tool-list-show-paths.txt fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--show-paths"],
            CommandOutput {
                exit_code: Some(0),
                stdout: fixture,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 1);
        let names: Vec<&str> = artifacts[0]
            .facts
            .command_inputs
            .provided
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["ruff"]);
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
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "ruff");
    }

    #[tokio::test]
    async fn test_check_updates_marks_every_tool_uncheckable_when_the_lookup_fails() {
        // Same rule as cargo, pip, pipx and npm: `uv tool list --outdated`
        // asks PyPI, and an index that did not answer means "Banager does
        // not know about these tools", not "this source failed".
        let list =
            std::fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-show-paths.txt")
                .expect("read uv tool-list-show-paths.txt fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--outdated"],
            CommandOutput {
                exit_code: Some(2),
                stdout: String::new(),
                stderr: "error: Request failed after 3 retries".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--show-paths"],
            CommandOutput {
                exit_code: Some(0),
                stdout: list,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("an index that did not answer is not a source failure")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "ruff");
        assert!(!candidates[0].checkable);
        assert!(candidates[0]
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Message(m) if m.contains("Request failed"))));
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
            assert_eq!(command_args(&plan), expected);
            assert!(!plan.needs_password);
            // With `UV_TOOL_DIR` unset (these tests' default), an uninstall
            // says under the tool what goes and what stays: the tool's
            // environment and the executables its receipt records.
            let scope = match kind {
                OpKind::Uninstall => vec![Warning::UninstallScope {
                    what: UninstallScope::Uv,
                }],
                OpKind::Install | OpKind::Upgrade => vec![],
            };
            assert_eq!(plan.warnings, scope, "{kind:?}");
        }
    }

    /// A runner that answers `uv tool list --show-paths` with the recorded
    /// fixture (one tool, ruff).
    fn show_paths_runner() -> Arc<MockRunner> {
        let text =
            std::fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-show-paths.txt")
                .expect("read uv tool-list-show-paths.txt fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--show-paths"],
            CommandOutput {
                exit_code: Some(0),
                stdout: text,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    fn request(kind: OpKind) -> OpRequest {
        OpRequest {
            kind,
            instance_id: test_instance().id,
            artifact_kind: ArtifactKind::Tool,
            name: "ruff".to_string(),
        }
    }

    #[tokio::test]
    async fn test_with_uv_tool_dir_set_no_tool_offers_an_uninstall() {
        // `uv tool uninstall` of the last tool deletes the tools folder and
        // then its parent with every file in it, when the parent holds no
        // folder but `.tmp*` ones (uv 0.12.17
        // `crates/uv/src/commands/tool/uninstall.rs:40-52`); under
        // `UV_TOOL_DIR` that parent is one of the user's own folders. Every
        // row says why it has no Uninstall button, from the inventory.
        let adapter = UvAdapter::new(show_paths_runner())
            .with_tool_dir_fn(|| Some(OsString::from("/Users/someone/work/uv-tools")));
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].uninstall_blocked,
            Some(UninstallBlocked::UvToolDirSet)
        );

        // Unset -- or set to nothing, which uv reads as unset
        // (`InstalledTools::from_settings`) -- every row keeps its button.
        for tool_dir_fn in [(|| None) as fn() -> Option<OsString>, || {
            Some(OsString::new())
        }] {
            let adapter = UvAdapter::new(show_paths_runner()).with_tool_dir_fn(tool_dir_fn);
            let artifacts = adapter
                .inventory(&test_instance())
                .await
                .expect("inventory");
            assert_eq!(artifacts[0].uninstall_blocked, None);
        }
    }

    #[tokio::test]
    async fn test_with_uv_tool_dir_set_plan_refuses_the_uninstall_and_nothing_else() {
        // The gate refuses the row from the snapshot; the plan refuses it
        // too, for a snapshot older than the environment it reads. Install
        // and upgrade delete no folder, so they still plan.
        let runner = Arc::new(MockRunner::new());
        let adapter = UvAdapter::new(runner.clone())
            .with_tool_dir_fn(|| Some(OsString::from("/Users/someone/work/uv-tools")));
        let inst = test_instance();
        // Refused, so no preview says what goes and what stays: the sentence
        // would not hold under `UV_TOOL_DIR`.
        match UvAdapter::plan(&adapter, &inst, &request(OpKind::Uninstall)).await {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::UvToolDirSet)
            }
            other => panic!("expected UninstallBlocked(UvToolDirSet), got {other:?}"),
        }
        for kind in [OpKind::Install, OpKind::Upgrade] {
            let plan = UvAdapter::plan(&adapter, &inst, &request(kind))
                .await
                .unwrap_or_else(|e| panic!("{kind:?} still plans: {e}"));
            assert!(plan.warnings.is_empty(), "{kind:?}: {:?}", plan.warnings);
        }
        assert!(runner.calls().is_empty(), "planning runs no uv command");
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
