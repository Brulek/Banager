use crate::adapters::{
    ensure_instance_match, lookup_failure_reason, reconcile_from, run_plan, uncheckable_candidate,
    uncheckable_from_inventory, url_path_segment, validate_package_name, Adapter, AdapterError,
    AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, InstanceStatus,
    ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction, Reconciled, ResourceLock, Scope,
    SearchHit, Unavailable, UninstallScope, UpdateBlocked, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Parses `pipx --version`'s output, which — unlike `brew --version`'s
/// "Homebrew 7.0.3" — is the bare version string with no label
/// (`adapters/fixtures/pipx/1.17.3/version.txt` is exactly `1.17.3\n`).
fn parse_version(text: &str) -> Option<String> {
    let v = text.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// True when `version` is pipx >= 1.16, the floor `pipx list --outdated`
/// needs to exist at all (per this phase's ruling — see
/// `adapters/fixtures/pipx/1.17.3/README.md`). Only major/minor are
/// compared; pipx has never shipped a patch-level `--outdated` gate.
fn supports_native_outdated(version: &str) -> bool {
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor) >= (1, 16)
}

#[derive(Debug, Deserialize)]
struct PipxListRoot {
    venvs: HashMap<String, PipxVenv>,
}

#[derive(Debug, Deserialize)]
struct PipxVenv {
    metadata: PipxMetadata,
}

#[derive(Debug, Deserialize)]
struct PipxMetadata {
    main_package: PipxMainPackage,
}

/// One entry of `main_package.app_paths`: pipx serialises a `Path` as
/// `{"__Path__": "…", "__type__": "Path"}`, and an exposed app lives at
/// `<venv>/bin/<app>`.
#[derive(Debug, Deserialize)]
struct PipxAppPath {
    #[serde(rename = "__Path__")]
    path: String,
}

#[derive(Debug, Deserialize)]
struct PipxMainPackage {
    package: String,
    package_version: String,
    /// Every executable pipx exposed for this package, absolute. Empty for
    /// a venv with no app; `default` for a `pipx list --json` too old to
    /// write the key at all, which then simply gives rule 2 nothing.
    #[serde(default)]
    app_paths: Vec<PipxAppPath>,
}

/// Parses `pipx list --json`. The venv name (the JSON object's key under
/// `venvs`) is the tool's `ArtifactKey.name`; the installed version lives at
/// `venvs.<name>.metadata.main_package.package_version` (this phase's other
/// documented trap for pipx). `venvs` is a `HashMap`, so entries are sorted
/// by name before returning to keep output deterministic.
fn parse_list(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: PipxListRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<InstalledArtifact> = root
        .venvs
        .into_iter()
        .map(|(tool_name, venv)| {
            // The venv directory, two levels above any exposed app
            // (`<venv>/bin/<app>`): what a `~/.local/bin` shim resolves
            // under, so the unknown-source scan's rule 2 (scan/mod.rs)
            // can claim the shim the way it claims a uv tool's -- uv.rs:65
            // fills the same thing from `uv tool list --show-paths`. No
            // app, no path.
            let path = venv
                .metadata
                .main_package
                .app_paths
                .first()
                .and_then(|app| Path::new(&app.path).parent()?.parent())
                .map(Path::to_path_buf);
            InstalledArtifact {
                key: ArtifactKey {
                    instance_id: instance_id.to_string(),
                    kind: ArtifactKind::Tool,
                    name: tool_name,
                },
                display_name: venv.metadata.main_package.package,
                version: venv.metadata.main_package.package_version,
                reason: InstallReason::Requested,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path,
                auto_updates: false,
                // pipx pins, but `pipx uninstall` removes a pinned tool: pipx
                // 1.17.3's `commands/uninstall.py` never reads `pinned`.
                uninstall_blocked: None,
                facts: Default::default(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

/// What `pipx list --outdated` puts between a pinned tool's name and the
/// colon: `f"{subject}{' [pinned]' if package.pinned else ''}: ..."` in
/// `_package_message` (pipx's `commands/outdated.py:238-244`, the same at
/// every tag from 1.16.0, where `--outdated` first exists, to 1.17.6).
const PINNED_MARKER: &str = " [pinned]";

/// Parses `pipx list --outdated`'s prose output: one `name: old -> new`
/// line per outdated tool, and the literal sentence `pipx found no
/// available upgrades.` when there are none. An unmatched line is skipped,
/// never treated as an error (this phase's documented trap for pipx).
///
/// A tool someone ran `pipx pin` on is still listed, as
/// `name [pinned]: old -> new`: `list` never sets the `upgradable_only`
/// that alone drops it (pipx's `commands/outdated.py:191-192`). The marker
/// is cut off the name and becomes `UpdateBlocked::Pinned`, because
/// `pipx upgrade` of that tool changes nothing (`_upgrade_package` returns
/// `UpgradeStatus.PINNED` at `commands/upgrade.py:408-409`) and still
/// exits 0 (`upgrade()` builds its `OperationResult` without an
/// `exit_code`, `commands/upgrade.py:74-81`, whose default is
/// `EXIT_CODE_OK`, `result.py:65`), so running it would be a false
/// "Succeeded". Left on the name, the marker made the row's key
/// `cowsay [pinned]`, which matches no installed tool and which
/// `validate_package_name` refuses.
fn parse_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "pipx found no available upgrades." {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "pipx found no available upgrades." {
            continue;
        }
        let Some((name, versions)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let (name, blocked) = match name.strip_suffix(PINNED_MARKER) {
            Some(bare) => (bare.trim_end(), Some(UpdateBlocked::Pinned)),
            None => (name, None),
        };
        let Some((old, new)) = versions.trim().split_once("->") else {
            continue;
        };
        let (old, new) = (old.trim(), new.trim());
        if name.is_empty() || old.is_empty() || new.is_empty() {
            continue;
        }
        out.push(UpdateCandidate {
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
            blocked,
        });
    }
    out
}

#[derive(Debug, Deserialize)]
struct PyPiResponse {
    info: PyPiInfo,
}

#[derive(Debug, Deserialize)]
struct PyPiInfo {
    version: String,
}

pub struct PipxAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
}

impl PipxAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> PipxAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/pipx.toml"))
            .expect("adapters/meta/pipx.toml must parse");
        PipxAdapter { runner, http, meta }
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("pipx", env) else {
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
            Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
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

    async fn run_pipx(
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
            .run_pipx(
                inst,
                vec!["list".to_string(), "--json".to_string()],
                Duration::from_secs(60),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_list(&output.stdout, &inst.id)
    }

    async fn latest_pypi_version(&self, name: &str) -> Result<String, String> {
        let resp = self
            .http
            .send(HttpRequest {
                method: "GET",
                // Percent-encoded: the package name comes out of `pipx
                // list --json`, and raw it could re-point the request at a
                // different path on PyPI.
                url: format!("https://pypi.org/pypi/{}/json", url_path_segment(name)?),
                headers: Vec::new(),
                timeout: Duration::from_secs(30),
            })
            .await
            .map_err(|e| format!("PyPI request failed: {e}"))?;
        if resp.status != 200 {
            return Err(format!("PyPI returned status {}", resp.status));
        }
        let parsed: PyPiResponse = serde_json::from_str(&resp.body)
            .map_err(|e| format!("could not parse PyPI response: {e}"))?;
        Ok(parsed.info.version)
    }

    /// Below pipx 1.16 there is no `pipx list --outdated`, so each installed
    /// tool is looked up individually on PyPI. A per-package failure (network
    /// down, package removed from PyPI) becomes a `checkable: false`
    /// candidate for just that tool, never a hard error for the whole check.
    async fn check_outdated_via_pypi(
        &self,
        installed: &[InstalledArtifact],
    ) -> Vec<UpdateCandidate> {
        let mut out = Vec::new();
        for artifact in installed {
            match self.latest_pypi_version(&artifact.key.name).await {
                Ok(latest) if latest != artifact.version => out.push(UpdateCandidate {
                    key: artifact.key.clone(),
                    current: artifact.version.clone(),
                    target: latest,
                    channel: UpdateChannel::Registry,
                    checkable: true,
                    warnings: Vec::new(),
                    blocked: None,
                }),
                Ok(_) => {}
                Err(reason) => out.push(uncheckable_candidate(
                    artifact.key.clone(),
                    artifact.version.clone(),
                    UpdateChannel::Registry,
                    reason,
                )),
            }
        }
        out
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let native = inst
            .version
            .as_deref()
            .map(supports_native_outdated)
            .unwrap_or(false);
        if native {
            let output = self
                .run_pipx(
                    inst,
                    vec!["list".to_string(), "--outdated".to_string()],
                    Duration::from_secs(60),
                )
                .await?;
            // The PyPI fallback below already answers a failed lookup with
            // `checkable: false` rows. This path used to answer the same
            // question with `Err`, so which of the two a user got depended
            // only on which pipx they happened to have installed.
            if output.exit_code != Some(0) {
                let reason =
                    lookup_failure_reason("pipx list --outdated", output.exit_code, &output.stderr);
                let installed = self.inventory(inst).await?;
                return Ok(
                    uncheckable_from_inventory(&installed, UpdateChannel::Native, &reason).into(),
                );
            }
            Ok(parse_outdated(&output.stdout, &inst.id).into())
        } else {
            let installed = self.inventory(inst).await?;
            Ok(self.check_outdated_via_pypi(&installed).await.into())
        }
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "pipx has no search command; browse PyPI directly".to_string(),
        ))
    }

    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["install".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["uninstall".to_string(), req.name.clone()],
            OpKind::Upgrade => vec!["upgrade".to_string(), req.name.clone()],
        };
        // What `pipx uninstall` removes and leaves (pipx 1.17.3
        // `commands/uninstall.py:67-125`: the tool's venv and the links
        // into it, nothing else of the tool's), said under the tool.
        let warnings = match req.kind {
            OpKind::Uninstall => vec![Warning::UninstallScope {
                what: UninstallScope::Pipx,
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
impl Adapter for PipxAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        PipxAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        PipxAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        PipxAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        PipxAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        PipxAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        PipxAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        PipxAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Warning;
    use crate::testing::command_args;

    #[test]
    fn test_parse_version_reads_the_bare_version_string() {
        // pipx's own `pipx --version` fixture is just the version number,
        // no "pipx" label in front (unlike `brew --version`'s "Homebrew
        // 7.0.3") — see adapters/fixtures/pipx/1.17.3/version.txt.
        assert_eq!(parse_version("1.17.3\n"), Some("1.17.3".to_string()));
    }

    #[test]
    fn test_parse_version_of_empty_output_is_none() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("\n"), None);
    }

    #[test]
    fn test_supports_native_outdated_thresholds_at_1_16() {
        assert!(supports_native_outdated("1.17.3"));
        assert!(supports_native_outdated("1.16.0"));
        assert!(!supports_native_outdated("1.15.9"));
        assert!(!supports_native_outdated("0.9.0"));
    }

    #[test]
    fn test_parse_list_from_the_recorded_fixture() {
        // cargo runs tests with cwd = crates/banager-core (see
        // adapters/mod.rs's own `test_from_toml_parses_the_committed_brew_meta_file`).
        let json = std::fs::read_to_string("../../adapters/fixtures/pipx/1.17.3/list.json")
            .expect("read pipx list.json fixture");
        let artifacts = parse_list(&json, "pipx").expect("parse pipx list.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Tool);
        assert_eq!(artifacts[0].key.name, "cowsay");
        assert_eq!(artifacts[0].version, "5.0");
        assert_eq!(artifacts[0].reason, InstallReason::Requested);
        // The tool's venv directory, two levels above its exposed app:
        // the path the unknown-source scan's rule 2 (scan/mod.rs) compares
        // a `~/.local/bin` shim's target against, the way it does uv's.
        // `ends_with` rather than the fixture's absolute path, so the test
        // does not repeat the recording machine's home directory.
        let venv = artifacts[0]
            .path
            .as_deref()
            .expect("pipx fills path from app_paths");
        assert!(venv.ends_with("pipx/venvs/cowsay"), "{venv:?}");
    }

    #[test]
    fn test_parse_list_leaves_path_none_for_a_venv_that_exposes_no_app() {
        // A venv pipx could expose nothing from (`include_apps` false, or
        // a package with no console script): `app_paths` is empty and
        // there is no directory to hand rule 2. Inline: the recorded
        // fixture has no such venv, and this is a literal, not a fixture.
        let json = r#"{
            "pipx_spec_version": "0.1",
            "venvs": {
                "lib-only": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [],
                            "package": "lib-only",
                            "package_version": "0.1"
                        }
                    }
                }
            }
        }"#;
        let artifacts = parse_list(json, "pipx").expect("parse inline pipx list");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, None);
    }

    #[test]
    fn test_parse_outdated_from_the_recorded_fixture() {
        let text = std::fs::read_to_string("../../adapters/fixtures/pipx/1.17.3/list-outdated.txt")
            .expect("read pipx list-outdated.txt fixture");
        let candidates = parse_outdated(&text, "pipx");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].current, "5.0");
        assert_eq!(candidates[0].target, "6.1");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
        assert!(candidates[0].checkable);
        assert_eq!(candidates[0].blocked, None);
    }

    const RECORDED_OUTDATED: &str = "../../adapters/fixtures/pipx/1.17.3/list-outdated.txt";
    const PINNED_OUTDATED: &str = "../../adapters/fixtures/pipx/1.17.3/list-outdated-pinned.txt";

    fn read_fixture(path: &str) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    /// The fixture README's claim, checked: take the one ` [pinned]` back
    /// out of the edited file and it is the recording, byte for byte. If
    /// someone re-records one file and not the other, or edits anything
    /// besides the marker, this fails instead of the fixture quietly
    /// becoming hand-written.
    #[test]
    fn test_pinned_fixture_differs_from_the_recording_only_by_the_marker() {
        let recorded = read_fixture(RECORDED_OUTDATED);
        let edited = read_fixture(PINNED_OUTDATED);
        assert_eq!(
            edited.matches(PINNED_MARKER).count(),
            1,
            "exactly one marker was inserted"
        );
        assert_eq!(
            edited.replacen("cowsay [pinned]:", "cowsay:", 1),
            recorded,
            "the marker, after `cowsay`, is the only difference"
        );
    }

    #[test]
    fn test_parse_outdated_names_a_pinned_tool_without_its_marker_and_blocks_it() {
        // `pipx list --outdated` prints a pinned tool as
        // `cowsay [pinned]: 5.0 -> 6.1` (pipx's `commands/outdated.py:243`).
        // The name is what every operation is handed and what the row's key
        // must match in the inventory, so the marker cannot stay on it; it
        // becomes the same signal a pinned Homebrew formula carries.
        let candidates = parse_outdated(&read_fixture(PINNED_OUTDATED), "pipx");
        assert_eq!(candidates.len(), 1, "a pinned tool is still listed");
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].key.kind, ArtifactKind::Tool);
        assert_eq!(candidates[0].current, "5.0");
        assert_eq!(candidates[0].target, "6.1");
        assert!(candidates[0].checkable);
        assert_eq!(candidates[0].blocked, Some(UpdateBlocked::Pinned));
    }

    #[test]
    fn test_parse_outdated_recognizes_the_no_upgrades_sentence() {
        // "An unmatched line means no updates, never an error" — the literal
        // sentence pipx prints when nothing is outdated (README trap #1).
        let candidates = parse_outdated("pipx found no available upgrades.\n", "pipx");
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_parse_outdated_skips_unmatched_lines_instead_of_erroring() {
        // Edge case the fixture cannot show: pipx sometimes intersperses a
        // warning line above/below the real ones. An unmatched line must be
        // skipped, not treated as an error or a malformed candidate.
        let text = "WARNING: some pipx warning\ncowsay: 5.0 -> 6.1\n";
        let candidates = parse_outdated(text, "pipx");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
    }

    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/pipx"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            version: Some("1.17.3".to_string()),
            ..crate::testing::manager_instance("pipx", "pipx")
        }
    }

    #[tokio::test]
    async fn test_detect_finds_pipx_via_an_isolated_path_dir_and_marks_an_unverified_version() {
        // A dedicated temp directory used only as a fake PATH entry — never
        // a real system path — so this test cannot collide with, depend on,
        // or modify anything actually installed on the machine running it.
        let tmp_dir = std::env::temp_dir().join(format!(
            "banager-pipx-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let exe_path = tmp_dir.join("pipx");
        std::fs::write(&exe_path, b"#!/bin/sh\n").expect("write fake pipx executable");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "9.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, Some("9.9.9".to_string()));
        assert_eq!(instances[0].unverified_version, Some("9.9.9".to_string()));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_unverified_version_is_none_for_a_verified_version() {
        // The rule itself lives on AdapterMeta (Task 4) and is tested there;
        // this asserts pipx's own meta file pins the version the fixtures
        // were recorded against, so a future meta edit cannot silently start
        // flagging a healthy install.
        let adapter =
            PipxAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert_eq!(
            adapter.meta.unverified_version(&Some("1.17.3".to_string())),
            None
        );
    }

    #[tokio::test]
    async fn test_check_updates_uses_native_list_outdated_when_pipx_is_recent_enough() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--outdated"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "cowsay: 5.0 -> 6.1\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance(); // version 1.17.3, >= the 1.16 floor
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[tokio::test]
    async fn test_check_updates_falls_back_to_pypi_below_pipx_1_16() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}}}}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://pypi.org/pypi/cowsay/json",
            HttpResponse {
                status: 200,
                body: r#"{"info":{"version":"6.1"}}"#.to_string(),
            },
        );
        let adapter = PipxAdapter::new(runner, http.clone());
        let mut inst = test_instance();
        inst.version = Some("1.10.0".to_string()); // below the 1.16 floor
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].target, "6.1");
        assert_eq!(candidates[0].channel, UpdateChannel::Registry);
        assert_eq!(
            http.calls(),
            vec!["https://pypi.org/pypi/cowsay/json".to_string()]
        );
    }

    #[tokio::test]
    async fn test_native_check_updates_marks_every_tool_uncheckable_when_the_lookup_fails() {
        // pipx >= 1.16 has its own `list --outdated`, which reaches PyPI.
        // Below 1.16 the same failure already produced `checkable: false`
        // rows (the test below); the native path used to fail the whole
        // source instead, so one adapter answered the same question two
        // different ways depending on which pipx the user happened to have.
        let list = std::fs::read_to_string("../../adapters/fixtures/pipx/1.17.3/list.json")
            .expect("read pipx list.json fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--outdated"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "Error: Could not reach pypi.org".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: list,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("an index that did not answer is not a source failure")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert!(!candidates[0].checkable);
        assert!(candidates[0]
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Message(m) if m.contains("Could not reach pypi.org"))));
    }

    #[tokio::test]
    async fn test_check_updates_via_pypi_marks_a_failed_lookup_as_uncheckable() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}}}}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        http.fail("https://pypi.org/pypi/cowsay/json", "connection refused");
        let adapter = PipxAdapter::new(runner, http);
        let mut inst = test_instance();
        inst.version = Some("1.10.0".to_string());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates should not fail outright on one bad lookup")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert_eq!(candidates[0].current, "5.0");
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let runner = Arc::new(MockRunner::new());
        let adapter = PipxAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-pipx".to_string(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay".to_string(),
        };
        let result = PipxAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_uninstall_upgrade_build_the_expected_argv() {
        let adapter =
            PipxAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        for (kind, expected) in [
            (OpKind::Install, vec!["install", "cowsay"]),
            (OpKind::Uninstall, vec!["uninstall", "cowsay"]),
            (OpKind::Upgrade, vec!["upgrade", "cowsay"]),
        ] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Tool,
                name: "cowsay".to_string(),
            };
            let plan = PipxAdapter::plan(&adapter, &inst, &req)
                .await
                .expect("plan");
            assert_eq!(command_args(&plan), expected);
            assert!(!plan.needs_password);
            // Only an uninstall says, under the tool, what goes and what
            // stays: pipx deletes the tool's own venv and the links into it.
            let scope = match kind {
                OpKind::Uninstall => vec![Warning::UninstallScope {
                    what: UninstallScope::Pipx,
                }],
                OpKind::Install | OpKind::Upgrade => vec![],
            };
            assert_eq!(plan.warnings, scope, "{kind:?}");
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "install", "cowsay"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "installed cowsay\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay".to_string(),
        };
        let plan = PipxAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome =
            PipxAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}}}}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let present = PipxAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Tool,
                name: "cowsay".to_string(),
            },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("5.0".to_string()));
        let absent = PipxAdapter::reconcile(
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
        let adapter =
            PipxAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let result = <PipxAdapter as Adapter>::search(&adapter, &inst, "cowsay").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_latest_pypi_version_percent_encodes_the_package_name_into_the_url() {
        // The package name comes out of `pipx list --json`, i.e. off disk
        // and out of a subprocess Banager does not control. Interpolated
        // raw, a `/` in it re-points the request at a different PyPI path.
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://pypi.org/pypi/evil%2F..%2Fsimple%3Fx=1/json",
            HttpResponse {
                status: 200,
                body: r#"{"info":{"version":"9.9.9"}}"#.to_string(),
            },
        );
        let adapter = PipxAdapter::new(Arc::new(MockRunner::new()), http.clone());

        let latest = adapter.latest_pypi_version("evil/../simple?x=1").await;

        assert_eq!(latest.as_deref(), Ok("9.9.9"));
        assert_eq!(
            http.calls(),
            vec!["https://pypi.org/pypi/evil%2F..%2Fsimple%3Fx=1/json".to_string()]
        );
    }

    /// A `Session` whose only source is a real `PipxAdapter`, refreshed
    /// once: `pipx --version` answers 1.17.3 (at the 1.16 floor for
    /// `list --outdated`), `list --json` is the recorded fixture and
    /// `list --outdated` answers `outdated`. The pipx executable is a file
    /// in a fresh temp directory used only as a fake PATH entry, as in the
    /// detect test above; nothing installed on the machine is touched.
    async fn refreshed_pipx_session(outdated: String) -> Arc<crate::session::Session> {
        let tmp_dir = std::env::temp_dir().join(format!(
            "banager-pipx-session-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let exe_path = tmp_dir.join("pipx");
        std::fs::write(&exe_path, b"#!/bin/sh\n").expect("write fake pipx executable");
        let exe = exe_path.to_str().expect("utf8 temp path");

        let ok = |stdout: String| CommandOutput {
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![exe, "--version"], ok("1.17.3\n".to_string()));
        runner.respond(
            vec![exe, "list", "--json"],
            ok(read_fixture(
                "../../adapters/fixtures/pipx/1.17.3/list.json",
            )),
        );
        runner.respond(vec![exe, "list", "--outdated"], ok(outdated));
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let adapter: Arc<dyn Adapter> =
            Arc::new(PipxAdapter::new(runner, Arc::new(MockHttpClient::new())));
        let session =
            crate::session::Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None);
        session.refresh(&env, &CheckOptions::default()).await;
        let _ = std::fs::remove_dir_all(&tmp_dir);
        session
    }

    fn upgrade_cowsay() -> OpRequest {
        OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "pipx".to_string(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay".to_string(),
        }
    }

    #[tokio::test]
    async fn test_the_session_refuses_to_upgrade_a_pinned_pipx_tool_like_a_pinned_formula() {
        // `pipx upgrade cowsay` on a pinned cowsay exits 0 having changed
        // nothing (see `parse_outdated`), so the refusal has to come from
        // Banager: the same `blocked_upgrade` gate in `issue_plan` that
        // refuses a pinned Homebrew formula (session/plans.rs).
        let session = refreshed_pipx_session(read_fixture(PINNED_OUTDATED)).await;
        match session.issue_plan(&upgrade_cowsay()).await {
            Err(AdapterError::UpdateBlocked { reason }) => {
                assert_eq!(reason, UpdateBlocked::Pinned);
            }
            other => panic!("expected UpdateBlocked(Pinned) for cowsay, got {other:?}"),
        }
        assert!(
            session.operations().is_empty(),
            "a refused plan must never reach the OperationManager"
        );
    }

    #[tokio::test]
    async fn test_the_session_still_plans_the_upgrade_of_an_unpinned_pipx_tool() {
        // The recording itself: the same tool, not pinned, still plans, so
        // the refusal above is the pin's doing and not something else about
        // this setup.
        let session = refreshed_pipx_session(read_fixture(RECORDED_OUTDATED)).await;
        let issued = session
            .issue_plan(&upgrade_cowsay())
            .await
            .expect("an unpinned tool plans");
        assert_eq!(command_args(&issued.plan), vec!["upgrade", "cowsay"]);
    }
}
