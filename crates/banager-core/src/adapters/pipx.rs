use crate::adapters::{
    ensure_instance_match, get_ok, lookup_failure_reason, reconcile_from, run_plan,
    uncheckable_candidate, uncheckable_from_inventory, url_path_segment, validate_package_name,
    Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome, LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::http::HttpClient;
use crate::model::{
    ArtifactFacts, ArtifactKey, ArtifactKind, CancelPolicy, CommandInputs, InstallReason,
    InstalledArtifact, InstanceNote, InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome,
    Plan, PlanAction, ProvidedCommand, Reconciled, ResourceLock, Scope, SearchHit, Unavailable,
    UninstallBlocked, UninstallScope, UpdateBlocked, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Parses `pipx --version`'s output, which — unlike `brew --version`'s
/// "Homebrew 7.0.3" — is the bare version string with no label
/// (`adapters/fixtures/pipx/1.17.3/version.txt` is exactly `1.17.3\n`).
/// Only the first non-blank line is the version: a warning printed after
/// it is not part of it, and a version with a control character in it is
/// none (`sanity::version_token`).
pub(crate) fn parse_version(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    crate::adapters::sanity::version_token(Some(line.to_string()))
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
    #[serde(default)]
    pinned: bool,
    package: String,
    package_version: String,
    /// Every executable pipx exposed for this package, absolute. Empty for
    /// a venv with no app; `default` for a `pipx list --json` too old to
    /// write the key at all, which then simply gives rule 2 nothing.
    #[serde(default)]
    app_paths: Vec<PipxAppPath>,
    /// `pipx install --include-deps`: the executables pipx exposed for
    /// each dependency, by its name, in the same venv as the package's own
    /// (`{"notebook": [{"__Path__": "<venv>/bin/jupyter-notebook", ...}]}`).
    /// Read only for where the venv is, when the package exposes no app of
    /// its own (`venv_dir`); read loosely, as `suffix` is, so a value of
    /// another shape names no venv rather than failing the list.
    #[serde(default)]
    app_paths_of_dependencies: Option<serde_json::Value>,
    /// `pipx install --suffix`: what pipx adds to each app's name when it
    /// exposes it (`black@3.12` for the venv's `black`), so the name typed
    /// in Terminal. Empty without one; a value that is not a string is
    /// read as none rather than failing the list.
    #[serde(default)]
    suffix: Option<serde_json::Value>,
    /// What was handed to `pipx install`: a requirement (`black`,
    /// `black==24.1`) or a URL or path (`git+https://…`, `/Users/…/tool`).
    /// Read loosely, as `suffix` is (`from_index`).
    #[serde(default)]
    package_or_url: Option<serde_json::Value>,
    /// The `--pip-args` it was installed with; `--editable` among them for
    /// `pipx install --editable`. Read loosely (`from_index`).
    #[serde(default)]
    pip_args: Option<serde_json::Value>,
}

/// Whether pipx installed `package` from a package index by its name, so
/// that PyPI's latest release is what `pipx upgrade` would install: not
/// editable, and `package_or_url` a plain requirement -- a name, perhaps
/// with extras or a version, not a URL, a path or an archive. pipx 1.16+'s
/// own `list --outdated` skips the rest as "editable" and "non-index"
/// (`outdated.py:186-189`, `valid_pypi_name`); below 1.16, `pipx upgrade`
/// reinstalls from `package_or_url`, so a git or folder install offered
/// PyPI's release would be "updated" to the same version every time, and a
/// folder's name PyPI does not know was "could not check" for good. A
/// metadata without `package_or_url` (pipx before it wrote one) is read as
/// the name it was looked up by before.
fn from_index(package: &PipxMainPackage) -> bool {
    let editable = match &package.pip_args {
        Some(serde_json::Value::Array(args)) => args
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|arg| matches!(arg, "--editable" | "-e") || arg.starts_with("--editable=")),
        _ => false,
    };
    let spec = match &package.package_or_url {
        None | Some(serde_json::Value::Null) => return !editable,
        Some(serde_json::Value::String(spec)) => spec,
        Some(_) => return false,
    };
    // The requirement before any environment marker (`; python_version …`).
    let requirement = spec.split(';').next().unwrap_or("").trim();
    let name_end = requirement
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(requirement.len());
    let (name, rest) = requirement.split_at(name_end);
    const ARCHIVES: [&str; 7] = [
        ".whl", ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tar.xz",
    ];
    !editable
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.ends_with(|c: char| c.is_ascii_alphanumeric())
        && !ARCHIVES.iter().any(|archive| name.ends_with(archive))
        && rest.chars().all(|c| !matches!(c, '@' | '/' | '\\' | ':'))
        && rest
            .trim_start()
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '[' | '(' | '<' | '>' | '=' | '!' | '~'))
}

/// The commands pipx exposed for a package (`CommandInputs.provided`):
/// one per `app_paths` entry, named as exposed -- the app's own name and
/// the package's `suffix` -- with the app in the tool's environment as the
/// file the command is. Where pipx put the command itself (its bin folder,
/// `~/.local/bin` unless `PIPX_BIN_DIR` says otherwise) is not in this
/// answer; `commands::judge` looks for it.
fn pipx_commands(package: &PipxMainPackage) -> Vec<ProvidedCommand> {
    let suffix = match &package.suffix {
        Some(serde_json::Value::String(suffix)) => suffix.as_str(),
        _ => "",
    };
    package
        .app_paths
        .iter()
        .filter_map(|app| {
            let path = PathBuf::from(&app.path);
            let name = path.file_name()?.to_str()?;
            path.is_absolute().then(|| ProvidedCommand {
                name: format!("{name}{suffix}"),
                path: path.clone(),
                within: Vec::new(),
            })
        })
        .collect()
}

/// The venv directory, two levels above any exposed app
/// (`<venv>/bin/<app>`): what a `~/.local/bin` shim resolves under, so the
/// unknown-source scan's rule 2 (scan/mod.rs) can claim the shim the way
/// it claims a uv tool's -- uv.rs:65 fills the same thing from `uv tool
/// list --show-paths` -- and whose `bin/python` the uninstall preview asks
/// about (`needed_by`). The package's own first app's; for a package that
/// exposes none of its own but its dependencies' (`--include-deps`), the
/// first of those, by dependency name, that is absolute. No app, no path.
fn venv_dir(package: &PipxMainPackage) -> Option<PathBuf> {
    let above = |app: &str| Some(Path::new(app).parent()?.parent()?.to_path_buf());
    if let Some(app) = package.app_paths.first() {
        return above(&app.path);
    }
    let Some(serde_json::Value::Object(dependencies)) = &package.app_paths_of_dependencies else {
        return None;
    };
    let mut names: Vec<&String> = dependencies.keys().collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| dependencies[name].as_array())
        .flatten()
        .filter_map(|app| app.get("__Path__")?.as_str())
        .find(|app| Path::new(app).is_absolute())
        .and_then(above)
}

/// Parses `pipx list --json`. The venv name (the JSON object's key under
/// `venvs`) is the tool's `ArtifactKey.name`; the installed version lives at
/// `venvs.<name>.metadata.main_package.package_version` (this phase's other
/// documented trap for pipx). `venvs` is a `HashMap`, so entries are sorted
/// by name before returning to keep output deterministic.
#[cfg(test)]
pub(crate) fn parse_list(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: PipxListRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(artifacts_from_list(root, instance_id))
}

fn artifacts_from_list(root: PipxListRoot, instance_id: &str) -> Vec<InstalledArtifact> {
    let mut out: Vec<InstalledArtifact> = root
        .venvs
        .into_iter()
        .map(|(tool_name, venv)| {
            let path = venv_dir(&venv.metadata.main_package);
            let provided = pipx_commands(&venv.metadata.main_package);
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
                // 1.17.3's `commands/uninstall.py` never reads `pinned`. The
                // pipx Banager runs, installed with itself, is blocked by
                // `read_list` (`own_venv`).
                uninstall_blocked: None,
                facts: ArtifactFacts {
                    command_inputs: CommandInputs {
                        provided,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    crate::adapters::sanity::artifacts(out)
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
///
/// The name is printed as package and `--suffix` were typed (`black@3.12`,
/// `outdated.py:226/238`), but the venv, the tool's key in `list --json`,
/// is that name PEP 503-canonicalized (`black@3-12`, `venv.py:142-144`;
/// `list` refuses any other, `common.py:269`), so the row is keyed by it.
pub(crate) fn parse_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate> {
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
                name: super::pip::canonical_project(name),
            },
            current: old.to_string(),
            target: new.to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
            blocked,
            download_bytes: None,
        });
    }
    crate::adapters::sanity::candidates(out)
}

#[derive(Debug, Deserialize)]
struct PyPiResponse {
    info: PyPiInfo,
}

#[derive(Debug, Deserialize)]
struct PyPiInfo {
    version: String,
}

/// The latest version in PyPI's answer about one package (`GET
/// /pypi/<name>/json`), or why there is none. A version that is empty or
/// holds a control character (`sanity::is_name`) is no version: the
/// package's row says it could not be checked rather than offer an
/// update to it.
pub(crate) fn parse_pypi_body(body: &str) -> Result<String, String> {
    let parsed: PyPiResponse =
        serde_json::from_str(body).map_err(|e| format!("could not parse PyPI response: {e}"))?;
    let version = parsed.info.version;
    if crate::adapters::sanity::is_name(&version) {
        Ok(version)
    } else {
        Err("PyPI named no usable version".to_string())
    }
}

/// What one `pipx list --json` answered (`PipxAdapter::read_list`).
struct ListAnswer {
    artifacts: Vec<InstalledArtifact>,
    /// The environments whose `main_package.pinned` is true.
    pinned: HashSet<String>,
    /// The environments of tools not installed from a package index by
    /// name (`from_index`), which the PyPI fallback does not look up.
    not_from_index: HashSet<String>,
    /// pipx left some environment out of the list (exit 1): the source's
    /// `InstanceNote::SomeNotListed`.
    left_out: bool,
}

impl ListAnswer {
    fn notes(&self) -> Vec<InstanceNote> {
        if self.left_out {
            vec![InstanceNote::SomeNotListed]
        } else {
            Vec::new()
        }
    }
}

/// `pipx list --outdated`'s failures that name a listed tool: pipx prints
/// each environment it could not check on stderr as `{environment}:
/// {error}` (`outdated.py:36-43`), the error running on until the next
/// such line, and the healthy environments' lines on stdout, then exits 1.
/// Each listed tool named gets the words from its line to the next line
/// that names one or starts a left-out environment's failure
/// (`left_out_failure_head`); a left-out environment gets none.
fn failures_by_tool(stderr: &str, listed: &HashSet<&str>) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut current: Option<String> = None;
    for line in stderr.lines() {
        if let Some((name, _)) = line.split_once(": ") {
            if listed.contains(name) {
                current = Some(name.to_string());
            } else if left_out_failure_head(line) {
                current = None;
            }
        }
        if let Some(name) = &current {
            let words = out.entry(name.clone()).or_default();
            words.push_str(line);
            words.push('\n');
        }
    }
    out
}

/// Whether `line` starts the failure of an environment that is no listed
/// tool: `{environment}: {error}`, the name without capitals or spaces and
/// the error one pipx 1.17 words itself (`outdated.py:177,190`,
/// `backends/_base.py:128`, `venv_inspect.py:475`). Any other line that
/// names no listed tool, such as the backend's own `error: …`, goes on the
/// failure before it.
fn left_out_failure_head(line: &str) -> bool {
    let Some((name, error)) = line.split_once(": ") else {
        return false;
    };
    !name.is_empty()
        && !name.chars().any(|c| c.is_whitespace() || c.is_uppercase())
        && (error == "Missing internal pipx metadata."
            || error.starts_with("Package backend exited with code ")
            || (error.starts_with("Package ") && error.ends_with(" has corrupt pipx metadata."))
            || error.ends_with(" while pipx inspected its environment."))
}

/// Whether a `pipx list --outdated` that exited 1 finished and said which
/// environments it could not check, rather than dying before it printed
/// anything: pipx 1.17 doesn't catch the FileNotFoundError a pip-backend
/// environment whose Python is gone raises, so `cli()` ends in a traceback
/// with nothing on stdout (`main.py` `cli`).
fn named_its_failures(stderr: &str, failures: &HashMap<String, String>) -> bool {
    !stderr
        .lines()
        .any(|line| line.starts_with("Traceback (most recent call last):"))
        && (!failures.is_empty() || stderr.lines().any(left_out_failure_head))
}

/// The venv the pipx at `program` is in, when pipx installed itself
/// (`~/.local/bin/pipx` leading into `<PIPX_HOME>/venvs/pipx`, pipx 1.17's
/// `self_install.py`): that tool is `UninstallBlocked::SourceProgram`.
fn own_venv(program: &Path) -> Option<PathBuf> {
    super::own_tool_environment(program, "venvs")
}

/// `PipxAdapter::own_venv_fn` as `PipxAdapter::new` sets it.
#[cfg(not(test))]
const DEFAULT_OWN_VENV_FN: fn(&Path) -> Option<PathBuf> = own_venv;
#[cfg(test)]
const DEFAULT_OWN_VENV_FN: fn(&Path) -> Option<PathBuf> = |_| None;

pub struct PipxAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
    /// Where the pipx at a path is a tool of its own (`own_venv`): looked
    /// at by every list and uninstall preview. Inside this crate's unit
    /// tests nothing is looked at unless a test sets it, and the
    /// integration tests set it to nothing (`with_own_venv_fn`), so that no
    /// test follows a link on the Mac running it.
    own_venv_fn: fn(&Path) -> Option<PathBuf>,
}

impl PipxAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> PipxAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/pipx.toml"))
            .expect("adapters/meta/pipx.toml must parse");
        PipxAdapter {
            runner,
            http,
            meta,
            own_venv_fn: DEFAULT_OWN_VENV_FN,
        }
    }

    /// Test-only hook to set where the pipx at a path is a tool of its own
    /// (see `own_venv_fn`). Public with the `test-support` feature, for the
    /// integration tests, which are built without `cfg(test)`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_own_venv_fn(mut self, own_venv_fn: fn(&Path) -> Option<PathBuf>) -> PipxAdapter {
        self.own_venv_fn = own_venv_fn;
        self
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
        let version = match &output {
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
                no_answer: crate::runner::no_answer::unless_answered(&version, &output),
            },
            version,
            answered_at: None,
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
        Ok(self.read_list(inst).await?.artifacts)
    }

    /// `pipx list --json`. Exit 1 with a list on stdout is an answer: pipx
    /// 1.17 prints every environment it could read and leaves out, naming
    /// it on stderr, one whose Python is gone or whose pipx data is
    /// missing, then exits 1 (`list_packages.py:69-90, 151-175`,
    /// `EXIT_CODE_LIST_PROBLEM`). One such environment used to fail the
    /// whole source, and every pipx Update's pin read with it. Any other
    /// exit, or a stdout that is not pipx's list, is still a failure.
    async fn read_list(&self, inst: &ManagerInstance) -> Result<ListAnswer, AdapterError> {
        let output = self
            .run_pipx(
                inst,
                vec!["list".to_string(), "--json".to_string()],
                Duration::from_secs(60),
            )
            .await?;
        let failed = || AdapterError::CommandFailed {
            code: output.exit_code,
            stderr: output.stderr.clone(),
        };
        let root: PipxListRoot = match output.exit_code {
            Some(0) => serde_json::from_str(&output.stdout)
                .map_err(|e| AdapterError::Parse(e.to_string()))?,
            Some(1) => serde_json::from_str(&output.stdout).map_err(|_| failed())?,
            _ => return Err(failed()),
        };
        let named = |keep: fn(&PipxMainPackage) -> bool| {
            root.venvs
                .iter()
                .filter(|(_, venv)| keep(&venv.metadata.main_package))
                .map(|(name, _)| name.clone())
                .collect::<HashSet<String>>()
        };
        let pinned = named(|package| package.pinned);
        let not_from_index = named(|package| !from_index(package));
        Ok(ListAnswer {
            artifacts: super::block_own_tool(
                artifacts_from_list(root, &inst.id),
                (self.own_venv_fn)(&inst.exe_path).as_deref(),
            ),
            pinned,
            not_from_index,
            left_out: output.exit_code == Some(1),
        })
    }

    async fn latest_pypi_version(&self, name: &str) -> Result<String, LookupFailure> {
        // Percent-encoded: the package name comes out of `pipx list
        // --json`, and raw it could re-point the request at a different
        // path on PyPI.
        let url = format!("https://pypi.org/pypi/{}/json", url_path_segment(name)?);
        let resp = get_ok(
            self.http.as_ref(),
            url,
            Vec::new(),
            "PyPI request failed",
            "PyPI",
        )
        .await?;
        Ok(parse_pypi_body(&resp.body)?)
    }

    /// Below pipx 1.16 there is no `pipx list --outdated`, so each installed
    /// tool is looked up individually on PyPI. A per-package failure (network
    /// down, package removed from PyPI) becomes a `checkable: false`
    /// candidate for just that tool, never a hard error for the whole check.
    /// Handed only the tools installed from an index by name
    /// (`from_index`): the rest get no row, as pipx 1.16+ gives them none.
    async fn check_outdated_via_pypi(
        &self,
        installed: &[InstalledArtifact],
        pinned: &HashSet<String>,
    ) -> Vec<UpdateCandidate> {
        let mut out = Vec::new();
        let answers = super::registry_checks(
            installed
                .iter()
                .map(|artifact| self.latest_pypi_version(&artifact.display_name))
                .collect(),
        )
        .await;
        for (artifact, answer) in installed.iter().zip(answers) {
            match answer.and_then(|latest| {
                super::python_version::strictly_newer(&latest, &artifact.version)
                    .map(|newer| (latest, newer))
                    .ok_or_else(|| {
                        LookupFailure::from("could not compare Python package versions".to_string())
                    })
            }) {
                Ok((latest, true)) => out.push(UpdateCandidate {
                    key: artifact.key.clone(),
                    current: artifact.version.clone(),
                    target: latest,
                    channel: UpdateChannel::Registry,
                    checkable: true,
                    warnings: Vec::new(),
                    blocked: pinned
                        .contains(&artifact.key.name)
                        .then_some(UpdateBlocked::Pinned),
                    download_bytes: None,
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
                let list = self.read_list(inst).await?;
                let listed: HashSet<&str> =
                    list.artifacts.iter().map(|a| a.key.name.as_str()).collect();
                let failures = failures_by_tool(&output.stderr, &listed);
                // pipx checked each environment on its own and said which
                // it could not: those listed tools are "could not check",
                // in pipx's words for each, and the lines it printed for
                // the rest stand. Without such an answer -- another exit,
                // a crash, or nothing that names an environment's failure
                // -- every tool is, with pipx's first words.
                if output.exit_code == Some(1) && named_its_failures(&output.stderr, &failures) {
                    let mut candidates: Vec<UpdateCandidate> =
                        parse_outdated(&output.stdout, &inst.id)
                            .into_iter()
                            .filter(|c| !failures.contains_key(&c.key.name))
                            .collect();
                    for artifact in &list.artifacts {
                        if let Some(words) = failures.get(&artifact.key.name) {
                            candidates.push(uncheckable_candidate(
                                artifact.key.clone(),
                                artifact.version.clone(),
                                UpdateChannel::Native,
                                LookupFailure::words(
                                    lookup_failure_reason(
                                        "pipx list --outdated",
                                        output.exit_code,
                                        words,
                                    ),
                                    words,
                                ),
                            ));
                        }
                    }
                    return Ok(CheckOutcome {
                        candidates,
                        notes: list.notes(),
                    });
                }
                let failure = LookupFailure::words(
                    lookup_failure_reason("pipx list --outdated", output.exit_code, &output.stderr),
                    &output.stderr,
                );
                return Ok(CheckOutcome {
                    candidates: uncheckable_from_inventory(
                        &list.artifacts,
                        UpdateChannel::Native,
                        &failure,
                    ),
                    notes: list.notes(),
                });
            }
            Ok(parse_outdated(&output.stdout, &inst.id).into())
        } else {
            let list = self.read_list(inst).await?;
            let from_index: Vec<InstalledArtifact> = list
                .artifacts
                .iter()
                .filter(|a| !list.not_from_index.contains(&a.key.name))
                .cloned()
                .collect();
            Ok(CheckOutcome {
                candidates: self
                    .check_outdated_via_pypi(&from_index, &list.pinned)
                    .await,
                notes: list.notes(),
            })
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
        // The pipx this pipx is (`own_venv`): the gate's late twin
        // (`blocked_uninstall` in session/plans.rs refuses the same row from
        // the snapshot), so no preview of its uninstall is ever built. Its
        // update is planned as any tool's.
        if req.kind == OpKind::Uninstall
            && (self.own_venv_fn)(&inst.exe_path)
                .is_some_and(|venv| venv.file_name() == Some(req.name.as_ref()))
        {
            return Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::SourceProgram,
            });
        }
        if req.kind == OpKind::Upgrade {
            let pinned = self.read_list(inst).await?.pinned;
            if pinned.contains(&req.name) {
                return Err(AdapterError::UpdateBlocked {
                    reason: UpdateBlocked::Pinned,
                });
            }
        }
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["install".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["uninstall".to_string(), req.name.clone()],
            OpKind::Upgrade => vec!["upgrade".to_string(), req.name.clone()],
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
        };
        // What `pipx uninstall` removes and leaves (pipx 1.17.3
        // `commands/uninstall.py:67-125`: the tool's venv and the links
        // into it, nothing else of the tool's), said under the tool.
        let warnings = match req.kind {
            OpKind::Uninstall => vec![Warning::UninstallScope {
                what: UninstallScope::Pipx,
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
            basis: None,
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

    #[tokio::test]
    async fn test_pypi_lookups_overlap_four_at_a_time_and_share_the_host_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct DelayedRegistry {
            active: AtomicUsize,
            peak: AtomicUsize,
        }
        #[async_trait]
        impl crate::http::HttpClient for DelayedRegistry {
            async fn send(
                &self,
                req: crate::http::HttpRequest,
            ) -> Result<HttpResponse, crate::http::HttpError> {
                assert_eq!(req.timeout, Duration::from_secs(30));
                let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                self.peak.fetch_max(active, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                self.active.fetch_sub(1, Ordering::SeqCst);
                Ok(HttpResponse {
                    status: 200,
                    body: r#"{"info":{"version":"99.0"},"releases":{}}"#.into(),
                })
            }
        }
        let root: PipxListRoot =
            serde_json::from_str(&recorded_list_with_suffixed_copy(false)).unwrap();
        let template = artifacts_from_list(root, &test_instance().id).remove(0);
        let installed: Vec<_> = (0..200)
            .map(|i| {
                let mut artifact = template.clone();
                artifact.key.name = format!("cowsay_{i}");
                artifact
            })
            .collect();
        let http = Arc::new(DelayedRegistry {
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        });
        let adapter = PipxAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let start = std::time::Instant::now();
        let rows = adapter
            .check_outdated_via_pypi(&installed, &HashSet::new())
            .await;
        eprintln!(
            "pypi lookups: packages=200 latency=5ms elapsed={:?} peak={}",
            start.elapsed(),
            http.peak.load(Ordering::SeqCst)
        );
        assert_eq!(rows.len(), 200);
        assert_eq!(
            rows.iter().map(|r| &r.key).collect::<Vec<_>>(),
            installed.iter().map(|r| &r.key).collect::<Vec<_>>()
        );
        assert_eq!(http.peak.load(Ordering::SeqCst), 4);
        http.peak.store(0, Ordering::SeqCst);
        let pins = HashSet::new();
        let (first, second) = tokio::join!(
            adapter.check_outdated_via_pypi(&installed[..8], &pins),
            adapter.check_outdated_via_pypi(&installed[8..16], &pins),
        );
        assert_eq!((first.len(), second.len()), (8, 8));
        assert_eq!(
            http.peak.load(Ordering::SeqCst),
            4,
            "concurrent instances share the host limit"
        );
    }

    /// The recorded `pipx list --json` (`list.json`) with its one tool,
    /// cowsay, installed a second time as `cowsay-alt` (`pipx install
    /// cowsay --suffix=_alt`: the venv is named with the suffix,
    /// PEP 503-canonicalized as pipx's `venv.py:142-144` names it, the
    /// package keeps its own name), that copy's pin set to `pinned`. The
    /// field is the one pipx 1.6+ writes (`main_package.pinned`).
    fn recorded_list_with_suffixed_copy(pinned: bool) -> String {
        let json = std::fs::read_to_string("../../adapters/fixtures/pipx/1.17.3/list.json")
            .expect("read pipx list.json fixture");
        let mut root: serde_json::Value = serde_json::from_str(&json).expect("fixture parses");
        let mut copy = root["venvs"]["cowsay"].clone();
        copy["metadata"]["main_package"]["suffix"] = "_alt".into();
        copy["metadata"]["main_package"]["pinned"] = pinned.into();
        root["venvs"]["cowsay-alt"] = copy;
        root.to_string()
    }

    #[tokio::test]
    async fn regression_f01_legacy_pipx_pins_block_candidates_and_late_plans() {
        let list = |stdout: String| CommandOutput {
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
            stderr_cause: Default::default(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            list(recorded_list_with_suffixed_copy(true)),
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://pypi.org/pypi/cowsay/json",
            HttpResponse {
                status: 200,
                body: r#"{"info":{"name":"cowsay","version":"6.1"},"releases":{}}"#.into(),
            },
        );
        let adapter = PipxAdapter::new(runner.clone(), http);
        let mut inst = test_instance();
        // pipx 1.7.1 has pins but no `list --outdated`: the PyPI fallback.
        inst.version = Some("1.7.1".into());
        let rows = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap()
            .candidates;
        let blocked: Vec<_> = rows
            .iter()
            .map(|row| (row.key.name.as_str(), row.blocked))
            .collect();
        assert_eq!(
            blocked,
            [
                ("cowsay", None),
                ("cowsay-alt", Some(UpdateBlocked::Pinned))
            ],
            "only the pinned copy is held back"
        );
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay-alt".into(),
        };
        assert!(matches!(
            adapter.plan(&inst, &req).await,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::Pinned
            })
        ));
        // A later unpin is observed by planning; a pin introduced after
        // that plan is observed by the next plan, on native pipx too.
        inst.version = Some("1.17.3".into());
        for pinned in [false, true] {
            runner.respond(
                vec!["/opt/homebrew/bin/pipx", "list", "--json"],
                list(recorded_list_with_suffixed_copy(pinned)),
            );
            assert_eq!(adapter.plan(&inst, &req).await.is_ok(), !pinned);
        }
        let uninstall = OpRequest {
            kind: OpKind::Uninstall,
            ..req
        };
        assert!(adapter.plan(&inst, &uninstall).await.is_ok());
    }

    #[tokio::test]
    async fn regression_legacy_pipx_queries_project_but_keeps_alias_for_operations() {
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://pypi.org/pypi/ruff/json",
            HttpResponse {
                status: 200,
                body: r#"{"info":{"version":"1.9"}}"#.into(),
            },
        );
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec!["/opt/homebrew/bin/pipx", "list", "--json"], CommandOutput {
            exit_code: Some(0), stdout: r#"{"venvs":{"ruff-alt":{"metadata":{"main_package":{"package":"ruff","package_version":"1.0"}}}}}"#.into(),
            stderr: String::new(), stderr_cause: Default::default(), timed_out: false, cancelled: false,
        });
        let adapter = PipxAdapter::new(runner, http.clone());
        let inst = test_instance();
        for (current, actionable) in [
            ("1.0", true),
            ("2.0rc1", false),
            ("1.9.0", false),
            ("1.9+local", false),
            ("1!1.0", false),
        ] {
            let json = format!(
                r#"{{"venvs":{{"ruff-alt":{{"metadata":{{"main_package":{{"package":"ruff","package_version":"{current}"}}}}}}}}}}"#
            );
            let installed = parse_list(&json, &inst.id).unwrap();
            let rows = adapter
                .check_outdated_via_pypi(&installed, &HashSet::new())
                .await;
            assert_eq!(!rows.is_empty(), actionable, "{current}");
            if actionable {
                assert_eq!(rows[0].key.name, "ruff-alt");
            }
        }
        assert!(http
            .calls()
            .iter()
            .all(|url| url == "https://pypi.org/pypi/ruff/json"));
        for kind in [OpKind::Upgrade, OpKind::Uninstall] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Tool,
                name: "ruff-alt".into(),
            };
            let plan = adapter.plan(&inst, &req).await.unwrap();
            assert_eq!(command_args(&plan).last().unwrap(), "ruff-alt");
        }
    }

    #[tokio::test]
    async fn regression_r43_4_legacy_pipx_asks_pypi_only_about_tools_installed_from_it() {
        // pipx 1.7.1 `pipx upgrade` reinstalls from `package_or_url`, so a
        // git, folder or editable install is never PyPI's latest release:
        // offered it, the update "succeeded" at the same version every
        // time, and a folder's name PyPI did not know was "could not
        // check" for good. pipx 1.16+ skips them itself.
        let venv = |package: &str, spec: &str, pip_args: &str| {
            format!(
                r#""{package}":{{"metadata":{{"main_package":{{"package":"{package}","package_version":"1.0"{spec}{pip_args}}}}}}}"#
            )
        };
        let list = format!(
            r#"{{"venvs":{{{},{},{},{},{},{}}}}}"#,
            venv(
                "llm",
                r#","package_or_url":"git+https://github.com/simonw/llm""#,
                ""
            ),
            venv(
                "mytool",
                r#","package_or_url":"/Users/someone/src/mytool""#,
                ""
            ),
            venv(
                "devtool",
                r#","package_or_url":"devtool""#,
                r#","pip_args":["--editable"]"#
            ),
            venv(
                "wheel-tool",
                r#","package_or_url":"wheel_tool-1.0-py3-none-any.whl""#,
                ""
            ),
            venv(
                "black",
                r#","package_or_url":"black[d] >=24""#,
                r#","pip_args":[]"#
            ),
            venv("cowsay", "", ""),
        );
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: list,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        for name in ["black", "cowsay"] {
            http.respond(
                &format!("https://pypi.org/pypi/{name}/json"),
                HttpResponse {
                    status: 200,
                    body: r#"{"info":{"version":"2.0"}}"#.into(),
                },
            );
        }
        let adapter = PipxAdapter::new(runner, http.clone());
        let mut inst = test_instance();
        inst.version = Some("1.7.1".into());
        let rows: Vec<_> = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap()
            .candidates
            .into_iter()
            .map(|c| (c.key.name, c.checkable))
            .collect();
        assert_eq!(
            rows,
            [("black".to_string(), true), ("cowsay".to_string(), true)]
        );
        let mut asked = http.calls();
        asked.sort();
        assert_eq!(
            asked,
            [
                "https://pypi.org/pypi/black/json",
                "https://pypi.org/pypi/cowsay/json"
            ]
        );
    }

    #[tokio::test]
    async fn regression_r43_9_the_pipx_that_pipx_installed_cannot_be_uninstalled_here() {
        // pipx 1.17's `self_install.py`: `~/.local/bin/pipx` leads into the
        // `pipx` venv, which `pipx list --json` lists like any tool. Its
        // uninstall would remove the pipx every other pipx tool is updated
        // and uninstalled with -- as npm's own npm would.
        let root = crate::testing::unique_temp_path("r43-9-pipx");
        let venvs = root.join("pipx/venvs");
        let mut venv_json = Vec::new();
        for name in ["pipx", "cowsay"] {
            let app = venvs.join(name).join("bin").join(name);
            std::fs::create_dir_all(app.parent().unwrap()).unwrap();
            std::fs::write(&app, b"#!/bin/sh\n").unwrap();
            venv_json.push(format!(
                r#""{name}":{{"metadata":{{"main_package":{{"package":"{name}","package_version":"1.0","app_paths":[{{"__Path__":"{}","__type__":"Path"}}]}}}}}}"#,
                app.display()
            ));
        }
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(venvs.join("pipx/bin/pipx"), bin.join("pipx")).unwrap();
        let runner = Arc::new(MockRunner::new());
        let exe = bin.join("pipx");
        runner.respond(
            vec![exe.to_str().unwrap(), "list", "--json"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: format!(r#"{{"venvs":{{{}}}}}"#, venv_json.join(",")),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter =
            PipxAdapter::new(runner, Arc::new(MockHttpClient::new())).with_own_venv_fn(own_venv);
        let inst = ManagerInstance {
            exe_path: exe.clone(),
            prefix: bin.clone(),
            ..test_instance()
        };
        let blocked: Vec<_> = adapter
            .inventory(&inst)
            .await
            .unwrap()
            .into_iter()
            .map(|a| (a.key.name, a.uninstall_blocked))
            .collect();
        let request = |kind, name: &str| OpRequest {
            kind,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: name.to_string(),
        };
        let uninstall_pipx = adapter
            .plan(&inst, &request(OpKind::Uninstall, "pipx"))
            .await;
        let upgrade_pipx = adapter.plan(&inst, &request(OpKind::Upgrade, "pipx")).await;
        let uninstall_cowsay = adapter
            .plan(&inst, &request(OpKind::Uninstall, "cowsay"))
            .await;
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            blocked,
            [
                ("cowsay".to_string(), None),
                ("pipx".to_string(), Some(UninstallBlocked::SourceProgram))
            ]
        );
        assert!(matches!(
            uninstall_pipx,
            Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::SourceProgram
            })
        ));
        assert!(upgrade_pipx.is_ok(), "its update is offered as any tool's");
        assert!(uninstall_cowsay.is_ok());
    }

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_parse_version_reads_the_first_line_alone() {
        assert_eq!(
            parse_version("1.17.3\n1.17.3\n"),
            Some("1.17.3".to_string())
        );
        assert_eq!(
            parse_version("\n  1.17.3  \nwarning\n"),
            Some("1.17.3".to_string())
        );
        assert_eq!(parse_version("1.17.3\u{1b}\n"), None);
        assert_eq!(parse_version("1.17.3\n"), Some("1.17.3".to_string()));
    }

    #[test]
    fn regression_parse_list_falls_back_to_the_venv_name_for_a_blank_package() {
        let json = r#"{"venvs":{"black":{"metadata":{"main_package":
            {"package":"","package_version":"25.1\n0"}}}}}"#;
        let artifacts = parse_list(json, "pipx:/x").unwrap();
        assert_eq!(artifacts[0].display_name, "black");
        assert_eq!(artifacts[0].version, "");
    }

    #[test]
    fn regression_parse_outdated_drops_a_line_with_a_control_character() {
        let text = "r\ruff: 0.1 -> 0.2\nblack: 1\u{e} -> 2\ncowsay: 5.0 -> 6.1\n";
        let names: Vec<String> = parse_outdated(text, "pipx:/x")
            .into_iter()
            .map(|c| c.key.name)
            .collect();
        assert_eq!(names, vec!["cowsay".to_string()]);
    }
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
        // Its one app, as the command `cowsay`: the program in the venv
        // (where pipx exposed it is not in this answer).
        let provided = &artifacts[0].facts.command_inputs.provided;
        assert_eq!(provided.len(), 1);
        assert_eq!(provided[0].name, "cowsay");
        assert!(
            provided[0].path.ends_with("pipx/venvs/cowsay/bin/cowsay"),
            "{provided:?}"
        );
        assert!(provided[0].within.is_empty());
    }

    #[test]
    fn test_parse_list_names_every_app_as_pipx_exposed_it() {
        // All of `app_paths`, not just the first, each with the package's
        // `suffix`; a suffix of another shape is read as none.
        let json = r#"{
            "venvs": {
                "black": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [
                                { "__Path__": "/Users/someone/.local/pipx/venvs/black/bin/black", "__type__": "Path" },
                                { "__Path__": "/Users/someone/.local/pipx/venvs/black/bin/blackd", "__type__": "Path" }
                            ],
                            "package": "black",
                            "package_version": "25.9.0",
                            "suffix": "@3.12"
                        }
                    }
                },
                "httpie": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [
                                { "__Path__": "/Users/someone/.local/pipx/venvs/httpie/bin/http", "__type__": "Path" }
                            ],
                            "package": "httpie",
                            "package_version": "3.2.4",
                            "suffix": 7
                        }
                    }
                }
            }
        }"#;
        let artifacts = parse_list(json, "pipx").expect("parse inline pipx list");
        let names: Vec<Vec<&str>> = artifacts
            .iter()
            .map(|a| {
                a.facts
                    .command_inputs
                    .provided
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect()
            })
            .collect();
        assert_eq!(names, vec![vec!["black@3.12", "blackd@3.12"], vec!["http"]]);
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
    fn test_parse_list_finds_the_venv_of_a_package_whose_apps_are_its_dependencies() {
        // `pipx install --include-deps` of a package with no console script
        // of its own: `app_paths` is empty, and the apps pipx exposed are
        // its dependencies', in the same venv (`<venv>/bin/<app>`). The
        // venv is still there to name -- for the uninstall preview to ask
        // what its Python is, and for rule 2 to claim the shims -- though
        // no command is the package's own. Inline, as above.
        let json = r#"{
            "pipx_spec_version": "0.1",
            "venvs": {
                "jupyter": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [],
                            "app_paths_of_dependencies": {
                                "notebook": [
                                    {"__Path__": "/Users/someone/.local/pipx/venvs/jupyter/bin/jupyter-notebook", "__type__": "Path"}
                                ],
                                "jupyter-core": [
                                    {"__Path__": "/Users/someone/.local/pipx/venvs/jupyter/bin/jupyter", "__type__": "Path"}
                                ]
                            },
                            "package": "jupyter",
                            "package_version": "1.1.1"
                        }
                    }
                },
                "odd": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [],
                            "app_paths_of_dependencies": {"x": "not a list", "y": [{"__Path__": 7}], "z": [{"__Path__": "relative/bin/z"}]},
                            "package": "odd",
                            "package_version": "0.1"
                        }
                    }
                },
                "null-deps": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [],
                            "app_paths_of_dependencies": null,
                            "package": "null-deps",
                            "package_version": "0.1"
                        }
                    }
                }
            }
        }"#;
        let artifacts = parse_list(json, "pipx").expect("parse inline pipx list");
        let path_of = |name: &str| {
            artifacts
                .iter()
                .find(|artifact| artifact.key.name == name)
                .unwrap()
                .path
                .clone()
        };
        assert_eq!(
            path_of("jupyter"),
            Some(PathBuf::from("/Users/someone/.local/pipx/venvs/jupyter"))
        );
        // No command is the package's own: those are its dependencies'.
        let jupyter = artifacts.iter().find(|a| a.key.name == "jupyter").unwrap();
        assert!(jupyter.facts.command_inputs.provided.is_empty());
        // What cannot be read as a path, or is not absolute, names none,
        // and fails nothing.
        assert_eq!(path_of("odd"), None);
        assert_eq!(path_of("null-deps"), None);
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
    // Made by editing the recording above, so not among the recordings: see
    // adapters/fixtures-derived/pipx/1.17.3/README.md.
    const PINNED_OUTDATED: &str =
        "../../adapters/fixtures-derived/pipx/1.17.3/list-outdated-pinned.txt";

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
    fn regression_r43_1_parse_outdated_names_a_suffixed_tool_as_its_venv() {
        // `pipx install --suffix=@3.12 black` names the venv
        // `canonicalize_name("black@3.12")`, `black@3-12` (pipx's
        // `venv.py:142-144`), while `list --outdated` prints the package
        // and suffix as typed (`outdated.py:226/238`). The row's key has to
        // be the venv's, or Update finds no installed tool and never runs.
        let text = "black@3.12: 24.1.0 -> 25.1.0\ncowsay_alt [pinned]: 5.0 -> 6.1\n";
        let rows: Vec<_> = parse_outdated(text, "pipx")
            .into_iter()
            .map(|c| (c.key.name, c.blocked))
            .collect();
        assert_eq!(
            rows,
            [
                ("black@3-12".to_string(), None),
                ("cowsay-alt".to_string(), Some(UpdateBlocked::Pinned)),
            ]
        );
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
        let tmp_dir = crate::testing::unique_temp_path("pipx-detect");
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let exe_path = tmp_dir.join("pipx");
        std::fs::write(&exe_path, b"#!/bin/sh\n").expect("write fake pipx executable");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
            CommandOutput { stderr_cause: Default::default(),
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
    async fn regression_check_updates_offers_no_update_to_an_empty_or_control_character_pypi_version(
    ) {
        for body in [
            r#"{"info":{"version":""}}"#,
            r#"{"info":{"version":"6.1\n"}}"#,
            r#"{"info":{"version":"\u001b[31m6.1"}}"#,
        ] {
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec!["/opt/homebrew/bin/pipx", "list", "--json"],
                CommandOutput { stderr_cause: Default::default(),
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
                    body: body.to_string(),
                },
            );
            let adapter = PipxAdapter::new(runner, http);
            let mut inst = test_instance();
            inst.version = Some("1.10.0".to_string());
            let candidates = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates")
                .candidates;
            assert_eq!(candidates.len(), 1, "{body}");
            assert!(!candidates[0].checkable, "{body}");
            assert_eq!(candidates[0].target, "5.0", "{body}");
        }
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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

    /// `pipx list --json` as pipx 1.17 answers it with one environment,
    /// `black`, whose Python is gone: every other environment on stdout,
    /// the broken one left out and named on stderr, exit 1
    /// (`list_packages.py:69-90`, `EXIT_CODE_LIST_PROBLEM`).
    fn list_without_a_broken_venv() -> CommandOutput {
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(1),
            stdout: r#"{"pipx_spec_version":"0.1","venvs":{
                "cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}},
                "ruff":{"metadata":{"main_package":{"package":"ruff","package_version":"0.5.0"}}}
            }}"#
            .to_string(),
            stderr: "   package black has invalid interpreter /opt/homebrew/opt/python@3.12/bin/python3.12\n\nOne or more packages have a missing python interpreter.\n    To fix, execute: pipx reinstall-all\n\n".to_string(),
            timed_out: false,
            cancelled: false,
        }
    }

    #[tokio::test]
    async fn regression_r43_2_one_broken_venv_leaves_the_others_listed_checked_and_planned() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            list_without_a_broken_venv(),
        );
        // pipx 1.17's `list --outdated`: the healthy environments' lines on
        // stdout, the broken one's failure on stderr as
        // `{environment}: {error}`, exit 1 (`outdated.py:25-66`).
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--outdated"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: "cowsay: 5.0 -> 6.1\n".to_string(),
                stderr: "black: Package backend exited with code 1.\nstderr: No such file\n"
                    .to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let names: Vec<String> = adapter
            .inventory(&inst)
            .await
            .expect("a list that left one environment out still lists the rest")
            .into_iter()
            .map(|a| a.key.name)
            .collect();
        assert_eq!(names, ["cowsay", "ruff"]);
        let outcome = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("one broken environment is not a failed source");
        let rows: Vec<_> = outcome
            .candidates
            .iter()
            .map(|c| (c.key.name.as_str(), c.checkable, c.target.as_str()))
            .collect();
        assert_eq!(rows, [("cowsay", true, "6.1")], "ruff is up to date");
        assert_eq!(outcome.notes, [InstanceNote::SomeNotListed]);
        let upgrade = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay".to_string(),
        };
        let plan = adapter
            .plan(&inst, &upgrade)
            .await
            .expect("the pin read answered");
        assert_eq!(command_args(&plan), ["upgrade", "cowsay"]);

        // A listed environment pipx could not check is that tool's own
        // "could not check", with pipx's words; the others stand.
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--outdated"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: "cowsay: 5.0 -> 6.1\n".to_string(),
                stderr:
                    "black: Package backend exited with code 1.\nruff: Could not reach pypi.org\n"
                        .to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outcome = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        let rows: Vec<_> = outcome
            .candidates
            .iter()
            .map(|c| (c.key.name.as_str(), c.checkable))
            .collect();
        assert_eq!(rows, [("cowsay", true), ("ruff", false)]);
        assert!(outcome.candidates[1]
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Message(m) if m.contains("Could not reach pypi.org") && !m.contains("black"))));
    }

    #[tokio::test]
    async fn regression_r43_2_a_crashed_outdated_check_is_no_answer() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            list_without_a_broken_venv(),
        );
        // pipx 1.17's `list --outdated` when the broken environment uses the
        // pip backend: running its missing Python raises FileNotFoundError,
        // which `_collect_outdated` doesn't catch, so pipx dies before
        // printing the healthy environments' lines (`main.py` `cli`).
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--outdated"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "Traceback (most recent call last):\n  File \"/opt/homebrew/bin/pipx\", line 8, in <module>\n    sys.exit(cli())\nFileNotFoundError: [Errno 2] No such file or directory: '/Users/me/.local/pipx/venvs/black/bin/python'\n"
                    .to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let outcome = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a crashed check is each tool's \"could not check\"");
        let rows: Vec<_> = outcome
            .candidates
            .iter()
            .map(|c| (c.key.name.as_str(), c.checkable))
            .collect();
        assert_eq!(rows, [("cowsay", false), ("ruff", false)]);
        assert_eq!(outcome.notes, [InstanceNote::SomeNotListed]);
    }

    #[test]
    fn regression_r43_2_a_left_out_environments_failure_is_no_listed_tools() {
        let listed: HashSet<&str> = ["aaa", "ruff"].into_iter().collect();
        let failures = failures_by_tool(
            "aaa: Could not reach pypi.org\nblack: Package backend exited with code 1.\nstderr: error: No interpreter found\nruff: /Users/me/.local/pipx/venvs/ruff/bin/python exited with code 1 while pipx inspected its environment.\n",
            &listed,
        );
        assert_eq!(failures["aaa"], "aaa: Could not reach pypi.org\n");
        assert!(failures["ruff"].starts_with("ruff: "));
        assert_eq!(failures.len(), 2);
        // A listed tool's own backend error keeps all its lines.
        let failures = failures_by_tool(
            "aaa: Package backend exited with code 1.\nstderr: error: No interpreter found\n",
            &listed,
        );
        assert_eq!(
            failures["aaa"],
            "aaa: Package backend exited with code 1.\nstderr: error: No interpreter found\n"
        );
    }

    #[tokio::test]
    async fn regression_r43_2_a_list_that_did_not_parse_still_fails() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                stdout: String::new(),
                ..list_without_a_broken_venv()
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        assert!(matches!(
            adapter.inventory(&test_instance()).await,
            Err(AdapterError::CommandFailed { code: Some(1), .. })
        ));
    }

    #[tokio::test]
    async fn test_check_updates_via_pypi_marks_a_failed_lookup_as_uncheckable() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput { stderr_cause: Default::default(),
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
    async fn test_check_updates_via_pypi_marks_only_a_network_failure_as_one_to_check_again() {
        // Round-5 review finding 6: a certificate rustls would not accept
        // and a redirect the client will not follow used to be the network
        // too, and so "check again" on every check. Each kind, per tool.
        use crate::http::HttpError;
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput { stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: r#"{"venvs":{
                    "cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}},
                    "black":{"metadata":{"main_package":{"package":"black","package_version":"24.1.0"}}},
                    "ruff":{"metadata":{"main_package":{"package":"ruff","package_version":"0.5.0"}}}
                }}"#
                .to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        http.fail("https://pypi.org/pypi/cowsay/json", "connection refused");
        http.fail_with(
            "https://pypi.org/pypi/black/json",
            HttpError::Tls {
                host: "pypi.org".to_string(),
                detail: "invalid peer certificate: UnknownIssuer".to_string(),
            },
        );
        http.fail_with(
            "https://pypi.org/pypi/ruff/json",
            HttpError::Refused(
                "refusing to follow a redirect: https://pypi.org/pypi/ruff/json answered 301 Moved Permanently pointing at /pypi/ruff/json/".to_string(),
            ),
        );
        let adapter = PipxAdapter::new(runner, http);
        let mut inst = test_instance();
        inst.version = Some("1.10.0".to_string());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("failed lookups are rows, not a failed source")
            .candidates;
        let row = |name: &str| {
            candidates
                .iter()
                .find(|c| c.key.name == name)
                .unwrap_or_else(|| panic!("no row for {name}"))
        };
        for name in ["cowsay", "black", "ruff"] {
            assert!(!row(name).checkable, "{name}");
        }
        assert!(row("cowsay")
            .warnings
            .contains(&Warning::TransientLookupFailure));
        assert_eq!(
            row("black").warnings[1..],
            [Warning::SecureConnectionFailed {
                host: "pypi.org".to_string()
            }]
        );
        assert!(matches!(
            &row("black").warnings[0],
            Warning::Message(m) if m.contains("secure connection to pypi.org failed")
        ));
        assert_eq!(row("ruff").warnings.len(), 1, "{:?}", row("ruff").warnings);
        assert!(matches!(
            &row("ruff").warnings[0],
            Warning::Message(m) if m.contains("refusing to follow a redirect")
        ));
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
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec!["/opt/homebrew/bin/pipx", "list", "--json"], CommandOutput {
            exit_code: Some(0), stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0","pinned":false}}}}}"#.into(),
            stderr: String::new(), stderr_cause: Default::default(), timed_out: false, cancelled: false,
        });
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
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
                OpKind::Link => unreachable!("only the three kinds are planned here"),
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
                stderr_cause: Default::default(),
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
            CommandOutput { stderr_cause: Default::default(),
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
        let tmp_dir = crate::testing::unique_temp_path("pipx-session");
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let exe_path = tmp_dir.join("pipx");
        std::fs::write(&exe_path, b"#!/bin/sh\n").expect("write fake pipx executable");
        let exe = exe_path.to_str().expect("utf8 temp path");

        let ok = |stdout: String| CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![exe, "--version"], ok("1.17.3\n".to_string()));
        // The recording's paths are under the author's home folder, which a
        // refresh looks at (the tools' commands): moved under the test's
        // own home, where nothing is.
        let home = tmp_dir.join("home");
        runner.respond(
            vec![exe, "list", "--json"],
            ok(
                read_fixture("../../adapters/fixtures/pipx/1.17.3/list.json")
                    .replace("/Users/brulek/", &format!("{}/", home.display())),
            ),
        );
        runner.respond(vec![exe, "list", "--outdated"], ok(outdated));
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home,
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

    #[tokio::test]
    async fn test_latest_pypi_version_holds_to_the_shared_lookup_failure_table() {
        crate::adapters::lookup_cases::hold_to_the_table(
            "https://pypi.org/pypi/black/json",
            "PyPI request failed",
            "PyPI",
            |http| async move {
                PipxAdapter::new(Arc::new(MockRunner::new()), http)
                    .latest_pypi_version("black")
                    .await
            },
        )
        .await;
    }
}
