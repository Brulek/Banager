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
use std::path::{Path, PathBuf};
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
                download_bytes: None,
            })
        })
        .collect();
    crate::adapters::sanity::candidates(candidates)
}

/// A successful command with an unknown format is not an empty inventory.
fn require_parsed_tools(text: &str, count: usize) -> Result<(), AdapterError> {
    if count == 0 && !matches!(text.trim(), "" | "No tools installed") {
        return Err(AdapterError::Parse(
            "unrecognized uv tool list output".into(),
        ));
    }
    Ok(())
}

/// Until compatible-target resolution is available, a saved constraint
/// must never be advertised as an unconstrained upgrade. Unknown receipts
/// are equally unable to prove that `uv tool upgrade` can install latest.
fn upgrade_requirement(artifact: &InstalledArtifact) -> Result<(), String> {
    upgrade_basis(artifact).map(|_| ())
}

fn upgrade_basis(artifact: &InstalledArtifact) -> Result<String, String> {
    let path = artifact
        .path
        .as_ref()
        .filter(|p| p.is_absolute())
        .ok_or_else(|| "uv tool environment path is unknown".to_string())?;
    let text = crate::adapters::read_file::read_text(
        &path.join("uv-receipt.toml"),
        &crate::protected::Protected::of_this_process(),
    )
    .map_err(|_| "could not read uv tool requirements".to_string())?;
    unconstrained_requirement(&text, &artifact.key.name)?;
    let mut receipt: toml::Value =
        toml::from_str(&text).map_err(|_| "could not parse uv tool requirements".to_string())?;
    // Not the installed version: a tool updated another way before its
    // turn (`uv tool upgrade` in Terminal) was installed the same way, and
    // the readings around the command judge its version -- at the
    // confirmed target it is already updated (`AlreadyUpdated::
    // BeforeItsTurn`), not changed since shown (r20 R20-1).
    let commands_from = commands_from(&mut receipt, &artifact.key.name);
    Ok(super::plan_basis(serde_json::json!([
        artifact.key.name,
        path,
        receipt,
        commands_from
    ])))
}

/// Takes the commands (`entrypoints`) out of a parsed receipt, and gives
/// which packages' commands `uv tool upgrade` installs again in their
/// place: the `from` of each, and the tool's own, PEP 503 names.
///
/// The commands themselves follow the installed version, as the version
/// does: an upgrade that changes the tool removes them and writes the
/// ones the new version provides, from the packages the receipt's `from`
/// names and the tool itself (uv 0.12.17
/// `crates/uv/src/commands/tool/upgrade.rs:597-623`, `finalize_tool_install`
/// in `common.rs:733-799`), so a version that brought a command -- as
/// huggingface_hub 0.34 brought `hf` -- was installed the same way (r20
/// R20-1, skeptic finding 2). An older uv wrote no `from`; the upgrade
/// skips such an entry and installs the tool's own all the same. Which
/// packages is still the way it was installed: one added with
/// `--with-executables-from` would have its commands installed again by
/// the upgrade, which the preview did not say. `entrypoints` that is not
/// a list of tables stays in the receipt, compared as it is.
fn commands_from(receipt: &mut toml::Value, name: &str) -> std::collections::BTreeSet<String> {
    let mut packages = std::collections::BTreeSet::from([normalized(name)]);
    let Some(tool) = receipt.get_mut("tool").and_then(toml::Value::as_table_mut) else {
        return packages;
    };
    let Some(toml::Value::Array(entries)) = tool.get("entrypoints") else {
        return packages;
    };
    if !entries.iter().all(toml::Value::is_table) {
        return packages;
    }
    packages.extend(
        entries
            .iter()
            .filter_map(|entry| entry.get("from").and_then(toml::Value::as_str))
            .map(normalized),
    );
    tool.remove("entrypoints");
    packages
}

/// The words a tool's row says when something saved in its receipt can
/// hold the main package below the latest release.
const SAVED_VERSION_REQUIREMENT: &str =
    "saved uv version requirements need a compatible-target check";

/// A package name as PEP 503 compares it: any case, and every run of `-`,
/// `_` and `.` one `-`.
fn normalized(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut separator = false;
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            separator = true;
            continue;
        }
        if separator {
            out.push('-');
            separator = false;
        }
        out.push(c.to_ascii_lowercase());
    }
    if separator {
        out.push('-');
    }
    out
}

fn unconstrained_requirement(text: &str, name: &str) -> Result<(), String> {
    let receipt: toml::Value =
        toml::from_str(text).map_err(|_| "could not parse uv tool requirements".to_string())?;
    let name = normalized(name);
    let tool = receipt.get("tool");
    let requirements = tool
        .and_then(|t| t.get("requirements"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "uv tool requirements are unknown".to_string())?;
    let main: Vec<_> = requirements
        .iter()
        .filter(|r| {
            r.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|n| normalized(n) == name)
        })
        .collect();
    if main.len() != 1 {
        return Err("uv main-package requirement is unknown".into());
    }
    let table = main[0]
        .as_table()
        .ok_or_else(|| "uv requirement is unknown".to_string())?;
    // URL, git, path and future source forms are not ordinary index upgrades.
    if table
        .keys()
        .any(|key| !matches!(key.as_str(), "name" | "extras" | "specifier"))
    {
        return Err("uv requirement uses an unsupported source or marker".into());
    }
    match table.get("specifier") {
        None => {}
        Some(toml::Value::String(specifier)) if specifier.trim().is_empty() => {}
        _ => return Err(SAVED_VERSION_REQUIREMENT.into()),
    }
    // `uv tool upgrade` restores the constraints and overrides saved at
    // install (`--constraint`, `--override`) while `uv tool list
    // --outdated` looks for the latest release without them. Any naming
    // the main package can pin it, bound it or give it another source;
    // one Banager cannot read may be one of those.
    for saved in ["constraints", "overrides"] {
        let Some(entries) = tool.and_then(|t| t.get(saved)) else {
            continue;
        };
        let entries = entries
            .as_array()
            .ok_or_else(|| format!("uv tool {saved} are unknown"))?;
        for entry in entries {
            let entry_name = entry
                .as_table()
                .and_then(|t| t.get("name"))
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("uv tool {saved} are unknown"))?;
            if normalized(entry_name) == name {
                return Err(SAVED_VERSION_REQUIREMENT.into());
            }
        }
    }
    Ok(())
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
    /// `tool_dir_fn`). Public with the `test-support` feature, for the
    /// integration tests, which are built without `cfg(test)` and so would
    /// otherwise read the variable from the environment of the Mac running
    /// them.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_tool_dir_fn(mut self, tool_dir_fn: fn() -> Option<OsString>) -> UvAdapter {
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
        let version = match &output {
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
                no_answer: crate::runner::no_answer::unless_answered(&version, &output),
            },
            version,
            answered_at: None,
            unverified_version,
            read_only_reason: None,
        }]
    }

    /// `program` with `args`, stopped at `timeout` or once `cancel` is
    /// cancelled, whichever comes first.
    async fn run_uv(
        &self,
        program: &Path,
        args: Vec<String>,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, AdapterError> {
        let spec = CommandSpec {
            program: program.to_owned(),
            args,
            env: vec![("NO_COLOR".into(), "1".into())],
            cwd: None,
            timeout,
            // Every caller of this helper hands the result to a parser.
            output_use: OutputUse::Parsed,
        };
        Ok(self.runner.run(spec, None, cancel).await?)
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        self.inventory_at(&inst.exe_path, &inst.id).await
    }

    async fn inventory_at(
        &self,
        program: &Path,
        instance_id: &str,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .list_tools(program, Self::LIST_TIMEOUT, CancellationToken::new())
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        self.tools_in(&output.stdout, instance_id)
    }

    /// How long an inventory's `uv tool list --show-paths` and a check's
    /// `uv tool list --outdated` are given.
    const LIST_TIMEOUT: Duration = Duration::from_secs(60);

    /// `uv tool list --show-paths`, run by `program`, stopped at `timeout`
    /// or once `cancel` is cancelled.
    async fn list_tools(
        &self,
        program: &Path,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, AdapterError> {
        self.run_uv(
            program,
            vec![
                "tool".to_string(),
                "list".to_string(),
                "--show-paths".to_string(),
            ],
            timeout,
            cancel,
        )
        .await
    }

    /// The tools a `uv tool list --show-paths` that exited 0 printed.
    fn tools_in(
        &self,
        stdout: &str,
        instance_id: &str,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let uninstall_blocked = self.uninstall_blocked();
        let artifacts = parse_tool_list_show_paths(stdout, instance_id);
        require_parsed_tools(stdout, artifacts.len())?;
        Ok(artifacts
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
                &inst.exe_path,
                vec![
                    "tool".to_string(),
                    "list".to_string(),
                    "--outdated".to_string(),
                ],
                Self::LIST_TIMEOUT,
                CancellationToken::new(),
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
        let mut candidates = parse_tool_list_outdated(&output.stdout, &inst.id);
        require_parsed_tools(&output.stdout, candidates.len())?;
        if !candidates.is_empty() {
            let installed = self.inventory(inst).await?;
            for candidate in &mut candidates {
                let requirement = installed
                    .iter()
                    .find(|a| a.key == candidate.key)
                    .ok_or_else(|| "uv tool is absent from inventory".to_string())
                    .and_then(upgrade_requirement);
                if let Err(reason) = requirement {
                    *candidate = crate::adapters::uncheckable_candidate(
                        candidate.key.clone(),
                        candidate.current.clone(),
                        UpdateChannel::Native,
                        reason,
                    );
                }
            }
        }
        Ok(candidates.into())
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
        if req.kind == OpKind::Link {
            return Err(super::links_nothing(&self.meta.id));
        }
        // The gate's late twin (`blocked_uninstall` in session/plans.rs
        // refuses the same row from the snapshot): read again here, so no
        // preview of `uv tool uninstall` is ever built while uv would take
        // the folder above its tools folder with it.
        if req.kind == OpKind::Uninstall {
            if let Some(reason) = self.uninstall_blocked() {
                return Err(AdapterError::UninstallBlocked { reason });
            }
        }
        let mut basis = None;
        if req.kind == OpKind::Upgrade {
            let installed = self.inventory(inst).await?;
            let artifact = installed
                .iter()
                .find(|a| a.key.name == req.name)
                .ok_or_else(|| AdapterError::Refused("uv tool is absent from inventory".into()))?;
            basis = Some(upgrade_basis(artifact).map_err(AdapterError::Refused)?);
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
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
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
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
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
            basis,
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
        if plan.request.kind == OpKind::Upgrade {
            let current = match &plan.action {
                PlanAction::Command { program, .. } if plan.basis.is_some() => {
                    // `uv tool list` takes the same exclusive lock of the
                    // tools folder that `uv tool upgrade` holds from its
                    // start (uv 0.12.17 `commands/tool/list.rs:45`,
                    // `commands/tool/upgrade.rs:63`, `InstalledTools::lock`),
                    // and uv waits up to 5 minutes for it, its default
                    // (`uv-fs/src/locked_file.rs:17-19`). So while another
                    // uv holds it -- a `uv tool upgrade --all` in Terminal --
                    // this read is given the upgrade's own deadline, to wait
                    // for uv as the upgrade itself would, rather than the
                    // inventory's 60 s; and it is handed the operation's own
                    // token, as npm's prefix read is, so a Cancel while it
                    // waits stops it, and the update ends cancelled at once
                    // with nothing run: `run_operation` takes no reading
                    // after a `Cancelled`, which would wait for the same
                    // lock (r28 R28-1 and its skeptic).
                    let read = self
                        .list_tools(
                            program,
                            Duration::from_secs(plan.timeout_secs),
                            cancel.clone(),
                        )
                        .await;
                    if cancel.is_cancelled() {
                        return Ok(Outcome::Cancelled);
                    }
                    // A list that did not answer -- uv gone (as between
                    // `brew upgrade uv`'s unlink and link), failing, or not
                    // in time -- ends as uv's own failure would, its stderr
                    // in the log, with nothing written; only an answer
                    // that no longer reads as the preview did is a change
                    // (r20 R20-2).
                    let output = match super::read_before_run(read, sink.as_ref(), op_id)? {
                        super::ReadBeforeRun::Answered(output) => output,
                        super::ReadBeforeRun::Ends(outcome) => return Ok(outcome),
                    };
                    self.tools_in(&output.stdout, &plan.request.instance_id)
                        .ok()
                        .and_then(|installed| {
                            let mut matching = installed
                                .iter()
                                .filter(|artifact| artifact.key.name == plan.request.name);
                            let artifact = matching.next()?;
                            if matching.next().is_some() {
                                return None;
                            }
                            upgrade_basis(artifact).ok()
                        })
                }
                _ => None,
            };
            if cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            if current.is_none() || current != plan.basis {
                return Ok(Outcome::BanagerFailed(
                    crate::model::Fault::ChangedSinceShown,
                ));
            }
        }
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
    #[test]
    fn test_saved_uv_constraints_and_unknown_receipts_never_promise_latest() {
        for specifier in ["==0.5.0", ">=0.5,<0.6", "~=0.5.0", "!=0.6"] {
            let receipt =
                format!("[tool]\nrequirements = [{{ name = 'ruff', specifier = '{specifier}' }}]");
            assert!(super::unconstrained_requirement(&receipt, "ruff").is_err());
        }
        assert!(super::unconstrained_requirement(
            "[tool]\nrequirements = [{ name = 'ruff' }]",
            "ruff"
        )
        .is_ok());
        assert!(super::unconstrained_requirement(
            "[tool]\nrequirements = [{ name = 'ruff', git = 'remote' }]",
            "ruff"
        )
        .is_err());
        assert!(super::unconstrained_requirement(
            "[tool]\nrequirements = [{ name = 'other' }]",
            "ruff"
        )
        .is_err());
        assert!(super::unconstrained_requirement("bad receipt", "ruff").is_err());
    }

    /// `uv tool install ruff --constraint c.txt` (or `--override o.txt`)
    /// saves those beside `requirements` in the receipt's `[tool]` table
    /// (uv-tool's `Tool::to_toml`), and `uv tool upgrade` restores both
    /// while `uv tool list --outdated` looks for the latest release without
    /// them: one naming the main package can hold it below that target.
    #[test]
    fn regression_saved_constraints_and_overrides_on_the_main_package_never_promise_latest() {
        let pinned =
            Err("saved uv version requirements need a compatible-target check".to_string());
        for (table, entry) in [
            ("constraints", "{ name = 'ruff', specifier = '==0.15.0' }"),
            ("constraints", "{ name = 'ruff', specifier = '<0.16' }"),
            ("constraints", "{ name = 'ruff' }"),
            ("overrides", "{ name = 'ruff', specifier = '==0.15.0' }"),
            (
                "overrides",
                "{ name = 'ruff', git = 'https://github.com/astral-sh/ruff' }",
            ),
            (
                "overrides",
                "{ name = 'ruff', url = 'https://example.invalid/ruff.whl' }",
            ),
            // PEP 503: any case, and a run of `-`, `_` and `.` is one `-`.
            ("constraints", "{ name = 'RUFF', specifier = '==0.15.0' }"),
        ] {
            let receipt =
                format!("[tool]\nrequirements = [{{ name = 'ruff' }}]\n{table} = [{entry}]\n");
            assert_eq!(
                super::unconstrained_requirement(&receipt, "ruff"),
                pinned,
                "{receipt}"
            );
        }
        assert_eq!(
            super::unconstrained_requirement(
                "[tool]\nrequirements = [{ name = 'my-tool' }]\n\
                 overrides = [{ name = 'My__Tool', specifier = '<2' }]\n",
                "my-tool"
            ),
            pinned
        );
        // As uv writes it: one entry per line once there are two, the
        // entrypoints, and the options table after.
        let written = "[tool]\n\
            requirements = [{ name = \"ruff\" }]\n\
            constraints = [\n    { name = \"pydantic\", specifier = \"<3\" },\n    \
                { name = \"ruff\", specifier = \">=0.15,<0.16\" },\n]\n\
            entrypoints = [\n    { name = \"ruff\", install-path = \"/Users/u/.local/bin/ruff\", from = \"ruff\" },\n]\n\
            \n[tool.options]\nexclude-newer = \"2026-01-01T00:00:00Z\"\n";
        assert_eq!(super::unconstrained_requirement(written, "ruff"), pinned);
        // Saved inputs Banager cannot read safely say nothing about the
        // main package, so they cannot let latest through either.
        for unknown in [
            "constraints = 'ruff==0.15.0'",
            "constraints = ['ruff==0.15.0']",
            "constraints = [{ specifier = '==0.15.0' }]",
            "overrides = [{ name = 7 }]",
            "overrides = [['ruff']]",
        ] {
            let receipt = format!("[tool]\nrequirements = [{{ name = 'ruff' }}]\n{unknown}\n");
            assert!(
                super::unconstrained_requirement(&receipt, "ruff").is_err(),
                "{receipt}"
            );
        }
        // Ones naming another package only leave the main one as it was.
        for other in [
            "constraints = [{ name = 'pydantic', specifier = '<3' }]",
            "overrides = [{ name = 'ruff-lsp', specifier = '==0.0.1' }]",
            "constraints = []",
        ] {
            let receipt = format!("[tool]\nrequirements = [{{ name = 'ruff' }}]\n{other}\n");
            assert_eq!(
                super::unconstrained_requirement(&receipt, "ruff"),
                Ok(()),
                "{receipt}"
            );
        }
    }

    fn receipt_runner() -> (tempfile::TempDir, Arc<MockRunner>) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("uv-receipt.toml"),
            "[tool]\nrequirements = [{name = 'ruff'}]\n",
        )
        .unwrap();
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--show-paths"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: format!("ruff v0.15.0 ({})\n", dir.path().display()),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        (dir, runner)
    }

    #[tokio::test]
    async fn test_upgrade_plan_rechecks_saved_constraints() {
        let (dir, runner) = receipt_runner();
        let adapter = UvAdapter::new(runner);
        assert!(adapter
            .plan(&test_instance(), &request(OpKind::Upgrade))
            .await
            .is_ok());
        std::fs::write(
            dir.path().join("uv-receipt.toml"),
            "[tool]\nrequirements = [{name = 'ruff', specifier = '==0.15.0'}]\n",
        )
        .unwrap();
        assert!(matches!(
            adapter
                .plan(&test_instance(), &request(OpKind::Upgrade))
                .await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_update_check_never_offers_a_target_outside_saved_requirements() {
        let (dir, runner) = receipt_runner();
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--outdated"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "ruff v0.15.0 [latest: 0.16.0]\n".into(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        for requirement in ["", "==0.15.0", ">=0.15,<0.16"] {
            std::fs::write(
                dir.path().join("uv-receipt.toml"),
                format!(
                    "[tool]\nrequirements = [{{ name = 'ruff', specifier = '{requirement}' }}]\n"
                ),
            )
            .unwrap();
            let result = adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .unwrap();
            assert_eq!(result.candidates.len(), 1);
            assert_eq!(result.candidates[0].checkable, requirement.is_empty());
        }
        std::fs::remove_file(dir.path().join("uv-receipt.toml")).unwrap();
        assert!(
            !adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .unwrap()
                .candidates[0]
                .checkable
        );
    }

    #[tokio::test]
    async fn regression_a_saved_constraint_or_override_keeps_latest_unoffered_and_unplanned() {
        let (dir, runner) = receipt_runner();
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--outdated"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "ruff v0.15.0 [latest: 0.16.0]\n".into(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        for saved in [
            "constraints = [{ name = 'ruff', specifier = '==0.15.0' }]",
            "overrides = [{ name = 'ruff', specifier = '<0.16' }]",
        ] {
            std::fs::write(
                dir.path().join("uv-receipt.toml"),
                format!("[tool]\nrequirements = [{{ name = 'ruff' }}]\n{saved}\n"),
            )
            .unwrap();
            let result = adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .unwrap();
            assert_eq!(result.candidates.len(), 1);
            let row = &result.candidates[0];
            assert!(!row.checkable, "{saved}");
            assert_eq!(row.target, "0.15.0", "never the unreachable 0.16.0");
            assert_eq!(
                row.warnings,
                [Warning::Message(
                    "saved uv version requirements need a compatible-target check".into()
                )],
                "the same row a pinned requirement gets"
            );
            assert!(matches!(
                adapter
                    .plan(&test_instance(), &request(OpKind::Upgrade))
                    .await,
                Err(AdapterError::Refused(_))
            ));
        }
    }

    #[tokio::test]
    async fn test_unknown_nonempty_output_is_not_a_successful_empty_list() {
        for text in [
            "\x1b[1mruff v1.0 (path)\x1b[0m",
            "new output format",
            "- orphan",
            "",
            "No tools installed",
        ] {
            let runner = Arc::new(MockRunner::new());
            for option in ["--show-paths", "--outdated"] {
                runner.respond(
                    vec!["/opt/homebrew/bin/uv", "tool", "list", option],
                    CommandOutput {
                        stderr_cause: Default::default(),
                        exit_code: Some(0),
                        stdout: text.into(),
                        stderr: String::new(),
                        timed_out: false,
                        cancelled: false,
                    },
                );
            }
            let adapter = UvAdapter::new(runner.clone());
            let inventory = adapter.inventory(&test_instance()).await;
            let updates = adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await;
            if matches!(text, "" | "No tools installed") {
                assert!(inventory.unwrap().is_empty());
                assert!(updates.unwrap().candidates.is_empty());
            } else {
                assert!(matches!(inventory, Err(AdapterError::Parse(_))), "{text:?}");
                assert!(matches!(updates, Err(AdapterError::Parse(_))), "{text:?}");
            }
            assert_eq!(runner.calls().len(), 2);
        }
    }

    #[tokio::test]
    async fn test_inventory_and_update_commands_disable_forced_color() {
        struct ColorRunner;
        #[async_trait]
        impl CommandRunner for ColorRunner {
            async fn run(
                &self,
                spec: CommandSpec,
                _on_line: Option<crate::runner::LineCallback>,
                _cancel: CancellationToken,
            ) -> Result<CommandOutput, crate::runner::RunnerError> {
                assert!(
                    spec.env.contains(&("NO_COLOR".into(), "1".into())),
                    "parsed output must stay plain even with inherited FORCE_COLOR"
                );
                Ok(CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: "No tools installed".into(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                })
            }
        }
        let adapter = UvAdapter::new(Arc::new(ColorRunner));
        assert!(adapter
            .inventory(&test_instance())
            .await
            .unwrap()
            .is_empty());
        assert!(adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .unwrap()
            .candidates
            .is_empty());
    }

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
                stderr_cause: Default::default(),
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
        let (_dir, runner) = receipt_runner();
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--outdated"],
            CommandOutput {
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
        let (_dir, runner) = receipt_runner();
        let adapter = UvAdapter::new(runner);
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
                OpKind::Link => unreachable!("only the three kinds are planned here"),
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
                stderr_cause: Default::default(),
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
        let (_dir, runner) = receipt_runner();
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
        assert_eq!(
            runner.calls().len(),
            1,
            "upgrade rereads inventory requirements"
        );
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "install", "ruff"],
            CommandOutput {
                stderr_cause: Default::default(),
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
            CommandOutput { stderr_cause: Default::default(),
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
    async fn f08_execute_saved_receipt(changed: Option<&str>) {
        let (dir, runner) = receipt_runner();
        let adapter = UvAdapter::new(runner.clone());
        let plan = adapter
            .plan(&test_instance(), &request(OpKind::Upgrade))
            .await
            .unwrap();
        if let Some(receipt) = changed {
            let path = dir.path().join("uv-receipt.toml");
            match receipt {
                "remove receipt" => std::fs::remove_file(path).unwrap(),
                "receipt is a directory" => {
                    std::fs::remove_file(&path).unwrap();
                    std::fs::create_dir(path).unwrap();
                }
                _ => {
                    std::fs::write(path, receipt).unwrap();
                    // Even an accepted new preview with identical argv
                    // cannot authorize the saved plan's changed basis.
                    if let Ok(again) = adapter
                        .plan(&test_instance(), &request(OpKind::Upgrade))
                        .await
                    {
                        assert_eq!(plan.action, again.action);
                        assert_ne!(plan.basis, again.basis);
                    }
                }
            }
        }
        let crate::model::PlanAction::Command { program, args, .. } = &plan.action else {
            panic!("command");
        };
        let mut argv = vec![program.to_str().unwrap()];
        argv.extend(args.iter().map(String::as_str));
        runner.respond(
            argv,
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let before = runner.calls().len();
        let result = adapter
            .execute(
                &plan,
                Arc::new(crate::events::VecSink::new()),
                1,
                CancellationToken::new(),
            )
            .await;
        let writes: Vec<_> = runner.calls()[before..]
            .iter()
            .filter(|call| call.iter().any(|arg| arg == "upgrade"))
            .cloned()
            .collect();
        if changed.is_some() {
            assert!(
                writes.is_empty(),
                "saved preview must be refused after receipt changes: {writes:?}; {result:?}"
            );
            assert!(
                matches!(
                    result,
                    Ok(Outcome::BanagerFailed(
                        crate::model::Fault::ChangedSinceShown
                    ))
                ),
                "explicit stale-preview refusal: {result:?}"
            );
        } else {
            assert_eq!(result.unwrap(), Outcome::Succeeded);
            let expected: Vec<_> = std::iter::once(program.to_string_lossy().into_owned())
                .chain(args.clone())
                .collect();
            assert_eq!(writes, [expected]);
        }
    }

    #[tokio::test]
    async fn f08_g09_execute_original_plan_after_constraint_change() {
        f08_execute_saved_receipt(Some(
            "[tool]\nrequirements = [{name = 'ruff', specifier = '==0.15.0'}]\n",
        ))
        .await;
    }

    #[tokio::test]
    async fn f08_g09_execute_original_plan_after_unreadable_receipt() {
        f08_execute_saved_receipt(Some("not valid [toml")).await;
    }

    #[tokio::test]
    async fn f08_g09_unchanged_receipt_executes_saved_plan() {
        f08_execute_saved_receipt(None).await;
    }
    #[tokio::test]
    async fn f30b_uv_changed_dependency_constraint_refuses_saved_plan() {
        f08_execute_saved_receipt(Some(
            "[tool]\nrequirements = [{name = 'ruff'}]\nconstraints = [{name = 'dependency', specifier = '<2'}]\n",
        )).await;
    }

    #[tokio::test]
    async fn f30b_uv_missing_or_non_file_receipt_refuses_saved_plan() {
        for changed in ["remove receipt", "receipt is a directory"] {
            f08_execute_saved_receipt(Some(changed)).await;
        }
    }

    /// A receipt in the shape uv writes one (requirements, entrypoints and
    /// `[tool.options]`), read again by a refresh between the preview and
    /// the confirmation: reading the same receipt again changes nothing,
    /// so the saved upgrade runs, exactly as previewed.
    #[tokio::test]
    async fn f30b_uv_refresh_rereading_the_same_receipt_keeps_the_saved_plan() {
        let (dir, runner) = receipt_runner();
        std::fs::write(
            dir.path().join("uv-receipt.toml"),
            format!(
                "[tool]\nrequirements = [{{ name = \"ruff\" }}]\nentrypoints = [\n    {{ name = \"ruff\", install-path = \"{home}/.local/bin/ruff\", from = \"ruff\" }},\n]\n\n[tool.options]\nexclude-newer-package = {{}}\n",
                home = dir.path().display()
            ),
        )
        .unwrap();
        let adapter = UvAdapter::new(runner.clone());
        let plan = adapter
            .plan(&test_instance(), &request(OpKind::Upgrade))
            .await
            .unwrap();
        let basis = plan
            .basis
            .clone()
            .expect("an upgrade preview carries its basis");
        assert_eq!(basis.len(), 64);
        assert!(basis.bytes().all(|b| b.is_ascii_hexdigit()));
        // A refresh: the list and every receipt are read again.
        for _ in 0..2 {
            let rows = adapter.inventory(&test_instance()).await.unwrap();
            assert_eq!(rows.len(), 1);
        }
        let crate::model::PlanAction::Command { program, args, .. } = &plan.action else {
            panic!("command");
        };
        let argv: Vec<String> = std::iter::once(program.to_string_lossy().into_owned())
            .chain(args.iter().cloned())
            .collect();
        runner.respond(
            argv.iter().map(String::as_str).collect(),
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let before = runner.calls().len();
        let result = adapter
            .execute(
                &plan,
                Arc::new(crate::events::VecSink::new()),
                1,
                CancellationToken::new(),
            )
            .await;
        assert_eq!(result.unwrap(), Outcome::Succeeded);
        let list = vec![
            program.to_string_lossy().into_owned(),
            "tool".into(),
            "list".into(),
            "--show-paths".into(),
        ];
        assert_eq!(
            runner.calls()[before..],
            [list, argv],
            "one documented read, then exactly the previewed upgrade"
        );
    }

    /// A uv at `dir/bin/uv` -- the test's own, never this Mac's -- whose
    /// `tool list --show-paths` lists ruff at `version` in `dir`, ruff's
    /// receipt `receipt`: the instance, and the runner answering for it.
    fn own_uv(
        dir: &std::path::Path,
        version: &str,
        receipt: &str,
    ) -> (ManagerInstance, Arc<MockRunner>) {
        std::fs::write(dir.join("uv-receipt.toml"), receipt).unwrap();
        let inst = ManagerInstance {
            exe_path: dir.join("bin/uv"),
            prefix: dir.join("bin"),
            ..test_instance()
        };
        let runner = Arc::new(MockRunner::new());
        list_ruff(&runner, &inst, dir, version);
        (inst, runner)
    }

    /// `inst`'s uv lists ruff at `version`, its environment `dir`.
    fn list_ruff(
        runner: &MockRunner,
        inst: &ManagerInstance,
        dir: &std::path::Path,
        version: &str,
    ) {
        runner.respond(
            vec![
                inst.exe_path.to_str().unwrap(),
                "tool",
                "list",
                "--show-paths",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: format!("ruff v{version} ({})\n", dir.display()),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
    }

    /// ruff updated another way before its turn (`uv tool upgrade` in
    /// Terminal): the list's version moved, the receipt did not. The saved
    /// upgrade runs as previewed, and the readings around it say it was
    /// already updated (r20 R20-1).
    #[tokio::test]
    async fn r20_uv_version_moved_keeps_the_saved_plan() {
        let dir = tempfile::tempdir().unwrap();
        let (inst, runner) = own_uv(
            dir.path(),
            "0.15.0",
            "[tool]\nrequirements = [{name = 'ruff'}]\n",
        );
        let uv = inst.exe_path.to_str().unwrap().to_string();
        let adapter = UvAdapter::new(runner.clone());
        let plan = adapter
            .plan(&inst, &request(OpKind::Upgrade))
            .await
            .unwrap();
        list_ruff(&runner, &inst, dir.path(), "0.16.8");
        let again = adapter
            .plan(&inst, &request(OpKind::Upgrade))
            .await
            .unwrap();
        assert_eq!((&again.action, &again.basis), (&plan.action, &plan.basis));
        runner.respond(
            vec![&uv, "tool", "upgrade", "ruff"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: "Nothing to upgrade\n".into(),
                timed_out: false,
                cancelled: false,
            },
        );
        let result = adapter
            .execute(
                &plan,
                Arc::new(crate::events::VecSink::new()),
                1,
                CancellationToken::new(),
            )
            .await;
        assert_eq!(result.unwrap(), Outcome::Succeeded);
        assert_eq!(
            runner.calls().last().unwrap(),
            &[uv.as_str(), "tool", "upgrade", "ruff"]
        );
    }

    /// What the fingerprint keeps of a receipt's `entrypoints`: which
    /// packages' commands the upgrade installs again, not the commands.
    /// uv writes the commands the installed version provides each time it
    /// upgrades a tool (`finalize_tool_install`, uv 0.12.17), so a new
    /// version with another command is the same plan; an older uv wrote
    /// them with no `from`, and `uv tool upgrade` always installs the
    /// tool's own (r20 R20-1, skeptic finding 2).
    #[tokio::test]
    async fn r20_uv_commands_a_version_brings_are_not_a_change_but_their_packages_are() {
        let entry = |name: &str, from: Option<&str>| {
            let from = from
                .map(|from| format!(", from = '{from}'"))
                .unwrap_or_default();
            format!("{{name = '{name}', install-path = '/Users/x/.local/bin/{name}'{from}}}")
        };
        let receipt = |entries: &[String]| {
            format!(
                "[tool]\nrequirements = [{{name = 'ruff'}}]\nentrypoints = [{}]\n\n[tool.options]\nexclude-newer-package = {{}}\n",
                entries.join(", ")
            )
        };
        let dir = tempfile::tempdir().unwrap();
        let shown = receipt(&[entry("ruff", Some("ruff"))]);
        let (inst, runner) = own_uv(dir.path(), "0.15.0", &shown);
        let adapter = UvAdapter::new(runner);
        /// The fingerprint of ruff's upgrade planned with `text` as its
        /// receipt.
        async fn basis_with(
            adapter: &UvAdapter,
            inst: &ManagerInstance,
            env: &std::path::Path,
            text: &str,
        ) -> Option<String> {
            std::fs::write(env.join("uv-receipt.toml"), text).unwrap();
            adapter
                .plan(inst, &request(OpKind::Upgrade))
                .await
                .unwrap()
                .basis
        }
        let basis = |text: String| {
            let (adapter, inst, env) = (&adapter, &inst, dir.path());
            async move { basis_with(adapter, inst, env, &text).await }
        };
        let shown = basis(shown).await;
        assert!(shown.is_some());
        for same in [
            // The new version brought a command.
            receipt(&[entry("ruff", Some("ruff")), entry("ruff-lsp", Some("ruff"))]),
            // Or took its only one away for another name.
            receipt(&[entry("ruff-cli", Some("ruff"))]),
            // An older uv's receipt, with no `from`.
            receipt(&[entry("ruff", None)]),
            // None listed: the tool's own are still the ones installed.
            receipt(&[]),
        ] {
            assert_eq!(basis(same.clone()).await, shown, "{same}");
        }
        // Another package's commands installed with ruff's
        // (`--with-executables-from black`): the upgrade would install
        // them again, which the preview did not say.
        let black = receipt(&[entry("black", Some("black")), entry("ruff", Some("ruff"))]);
        assert_ne!(basis(black).await, shown);
    }

    #[tokio::test]
    async fn f30b_uv_saved_overrides_and_extras_changes_refuse_saved_plan() {
        for receipt in [
            "[tool]\nrequirements = [{name = 'ruff'}]\noverrides = [{name = 'ruff', specifier = '==0.15.0'}]\n",
            "[tool]\nrequirements = [{name = 'ruff', extras = ['custom']}]\n",
        ] {
            f08_execute_saved_receipt(Some(receipt)).await;
        }
    }
}
