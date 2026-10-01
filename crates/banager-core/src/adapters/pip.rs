use crate::adapters::{
    lookup_failure_reason, reconcile_from, second_token, uncheckable_candidate, Adapter,
    AdapterError, AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceStatus, ManagerInstance,
    OpRequest, Outcome, Plan, ReadOnlyReason, Reconciled, Scope, SearchHit, Unavailable,
    UpdateCandidate, UpdateChannel,
};
use crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct PipPackage {
    name: String,
    version: String,
}

fn parse_pip_list(json: &str) -> Result<Vec<PipPackage>, AdapterError> {
    serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))
}

#[derive(Debug, Deserialize)]
struct PipOutdatedPackage {
    name: String,
    version: String,
    latest_version: String,
}

fn parse_pip_outdated(json: &str, instance_id: &str) -> Result<Vec<UpdateCandidate>, AdapterError> {
    let items: Vec<PipOutdatedPackage> =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(items
        .into_iter()
        .map(|p| UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: p.name,
            },
            current: p.version,
            target: p.latest_version,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
            blocked: None,
        })
        .collect())
}

pub struct PipAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// Where the developer-tool shims are: `SHIM_DIR` (`/usr/bin`), and a
    /// folder of the test's own in the tests below, which cannot put a
    /// file in `/usr/bin`.
    shim_dir: PathBuf,
}

/// The folder macOS keeps its developer-tool shims in: since macOS 10.9,
/// programs in `/usr/bin` that run the tool of their name inside Xcode or
/// the Command Line Tools (Apple's TN2339), `/usr/bin/python3` and
/// `/usr/bin/pip3` among them (`man xcode-select`, FILES). With neither
/// installed, a shim opens the system's dialog offering to install the
/// Command Line Tools instead of running anything -- the reason Homebrew's
/// own git shim (`Library/Homebrew/shims/shared/git`) never runs
/// `/usr/bin/git` then -- which `detect`, running `/usr/bin/python3 -m pip
/// --version` at every refresh on a Mac whose `PATH` has no other
/// `python3`, would have done at every refresh.
const SHIM_DIR: &str = "/usr/bin";

impl PipAdapter {
    /// Interpreter names to probe on `PATH`, most-specific first, so a
    /// `python3` symlink and its versioned target (e.g. `python3.14`)
    /// resolving to the same real file are still only counted once (see
    /// `detect`'s canonicalization-based dedup).
    pub const CANDIDATE_INTERPRETERS: [&'static str; 7] = [
        "python3.14",
        "python3.13",
        "python3.12",
        "python3.11",
        "python3.10",
        "python3",
        "python",
    ];

    /// What `detect` asks before it runs an interpreter in `SHIM_DIR`:
    /// which developer directory the shims run their tools from. `-p`
    /// (`--print-path`) only prints it, for inspection; of xcode-select's
    /// options only `--install` opens a dialog (`man xcode-select`), and
    /// TN2339 gives `--print-path` as the way to see which Xcode the tools
    /// use.
    pub const XCODE_SELECT_ARGV: [&'static str; 2] = ["/usr/bin/xcode-select", "-p"];

    pub fn new(runner: Arc<dyn CommandRunner>) -> PipAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/pip.toml"))
            .expect("adapters/meta/pip.toml must parse");
        PipAdapter {
            runner,
            meta,
            shim_dir: PathBuf::from(SHIM_DIR),
        }
    }

    /// One `ManagerInstance` per distinct Python interpreter on `PATH` that
    /// has a working `pip` module (contract: "one per interpreter, invoked
    /// as `{python} -m pip`"). Interpreters are deduplicated by their
    /// canonicalized path so `python3` and `python3.14` naming the same
    /// binary do not produce two instances.
    ///
    /// An interpreter that is one of the developer-tool shims (`SHIM_DIR`)
    /// is run only when the developer directory has its tool
    /// (`shim_has_tool`); otherwise it is skipped as if it were not on
    /// `PATH` -- no instance, no error, no note -- because running it would
    /// open the system's dialog offering to install the Command Line Tools.
    /// `xcode-select -p` is asked at most once a call, and only when such
    /// an interpreter turns up; nothing keeps its answer past the call, so
    /// the refresh after the tools are installed finds them.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        // `Some(answer)` once xcode-select has been asked in this call.
        let mut developer_dir: Option<Option<PathBuf>> = None;
        for name in Self::CANDIDATE_INTERPRETERS {
            let Some(python_path) = resolve_exe(name, env) else {
                continue;
            };
            let canonical =
                std::fs::canonicalize(&python_path).unwrap_or_else(|_| python_path.clone());
            if !seen.insert(canonical.clone()) {
                continue;
            }
            // By where it leads, so a link elsewhere on `PATH` to the shim
            // is caught too; by where it was found as well, for a path
            // that could not be resolved.
            if let Some(shim) = [&canonical, &python_path]
                .into_iter()
                .find(|path| path.parent() == Some(self.shim_dir.as_path()))
            {
                if developer_dir.is_none() {
                    developer_dir = Some(self.active_developer_dir().await);
                }
                let dir = developer_dir.as_ref().and_then(|dir| dir.as_deref());
                if !self.shim_has_tool(shim, dir) {
                    continue;
                }
            }
            let output = self
                .runner
                .run(
                    CommandSpec {
                        program: python_path.clone(),
                        args: vec!["-m".to_string(), "pip".to_string(), "--version".to_string()],
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
                // "pip 26.2.1 from … (python 3.14)" — the shared
                // second-token rule (crate::adapters::second_token, Task 5)
                // yields pip's own version, which is what
                // `ManagerInstance::version` means here, not the
                // interpreter's Python version.
                Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
                _ => None,
            };
            let prefix = python_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("/"));
            let unverified_version = self.meta.unverified_version(&version);
            found.push(ManagerInstance {
                id: crate::model::instance_id(
                    &self.meta.id,
                    Some(&python_path.display().to_string()),
                ),
                adapter_id: self.meta.id.clone(),
                exe_path: python_path,
                prefix,
                scope: Scope::User,
                // The state axis. `version` is `None` exactly when this
                // interpreter would not run `-m pip --version` -- no pip
                // module for it, or pip crashed, or it timed out; either
                // way this Python was found but pip could not be reached
                // through it, the same "found the executable, it didn't
                // answer" state brew/cargo/pipx/uv/ollama report as
                // `NotResponding` on their own failed `--version`. This
                // instance's id needs no command to exist -- it is built
                // from `python_path`, which `resolve_exe` already
                // resolved -- so there is no reason to drop the row
                // instead of reporting it: doing that used to make this
                // interpreter's pip disappear with no notice and no
                // `SourceError`, indistinguishable from "there never was a
                // pip here to ask about".
                status: InstanceStatus {
                    unavailable: version.is_none().then_some(Unavailable::NotResponding),
                    notes: Vec::new(),
                },
                version,
                unverified_version,
                // Not a property of this machine: pip offers no
                // install/uninstall path Banager can safely drive, so
                // every pip instance anywhere is read-only by design --
                // whether or not this round could reach it.
                read_only_reason: Some(ReadOnlyReason::ByDesign),
            });
        }
        found
    }

    /// `xcode-select -p`'s answer: the developer directory the shims run
    /// their tools from, or `None` when it names none -- it could not be
    /// run, did not exit 0, or printed no absolute path.
    async fn active_developer_dir(&self) -> Option<PathBuf> {
        let [program, arg] = Self::XCODE_SELECT_ARGV;
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: PathBuf::from(program),
                    args: vec![arg.to_string()],
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(10),
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await
            .ok()?;
        if output.exit_code != Some(0) {
            return None;
        }
        let path = PathBuf::from(output.stdout.strip_suffix('\n').unwrap_or(&output.stdout));
        path.is_absolute().then_some(path)
    }

    /// Whether the shim `shim` has a tool to run: `developer_dir` names a
    /// folder whose `usr/bin` holds an executable file of the shim's name
    /// that is not itself in `shim_dir`. xcode-select prints the folder
    /// `DEVELOPER_DIR` names without checking that it is there
    /// (`DEVELOPER_DIR=/nonexistent/dir xcode-select -p` prints
    /// `/nonexistent/dir` and exits 0 on macOS 27), so its answer alone
    /// does not say the tool is there; and a developer directory of `/`
    /// would lead back to the shim. Homebrew's git shim goes by the same
    /// two things (`Library/Homebrew/shims/shared/git`: the folder
    /// `xcode-select -print-path` names, then the tool of its name under
    /// `usr/bin` there, run only when it is an executable file).
    fn shim_has_tool(&self, shim: &Path, developer_dir: Option<&Path>) -> bool {
        let (Some(dir), Some(name)) = (developer_dir, shim.file_name()) else {
            return false;
        };
        let Ok(tool) = std::fs::canonicalize(dir.join("usr/bin").join(name)) else {
            return false;
        };
        tool.parent() != Some(self.shim_dir.as_path())
            && std::fs::metadata(&tool)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }

    async fn run_pip_list(
        &self,
        inst: &ManagerInstance,
        extra_args: &[&str],
    ) -> Result<Vec<PipPackage>, AdapterError> {
        let mut args = vec![
            "-m".to_string(),
            "pip".to_string(),
            "list".to_string(),
            "--format=json".to_string(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: inst.exe_path.clone(),
                    args,
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(60),
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_pip_list(&output.stdout)
    }

    /// `--not-required` means "nothing else installed depends on this" —
    /// that is not the same as "the user asked for this", so packages in
    /// that set map to `InstallReason::Unknown`, and everything else (something
    /// depends on it) maps to `InstallReason::Dependency`. pip never tells
    /// Banager what the user explicitly typed `pip install` for, so
    /// `InstallReason::Requested` is never used here (this phase's documented
    /// trap for pip).
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let all = self.run_pip_list(inst, &[]).await?;
        let not_required = self.run_pip_list(inst, &["--not-required"]).await?;
        let leaf_names: HashSet<String> = not_required.into_iter().map(|p| p.name).collect();
        Ok(all
            .into_iter()
            .map(|p| {
                let reason = if leaf_names.contains(&p.name) {
                    InstallReason::Unknown
                } else {
                    InstallReason::Dependency
                };
                InstalledArtifact {
                    key: ArtifactKey {
                        instance_id: inst.id.clone(),
                        kind: ArtifactKind::Package,
                        name: p.name.clone(),
                    },
                    display_name: p.name,
                    version: p.version,
                    reason,
                    description: None,
                    homepage: None,
                    size_bytes: None,
                    installed_at: None,
                    path: None,
                    auto_updates: false,
                    uninstall_blocked: None,
                    facts: Default::default(),
                }
            })
            .collect())
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let args = vec![
            "-m".to_string(),
            "pip".to_string(),
            "list".to_string(),
            "--outdated".to_string(),
            "--format=json".to_string(),
        ];
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: inst.exe_path.clone(),
                    args,
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(60),
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await?;
        // `pip list --outdated` reaches PyPI. When it cannot, pip itself
        // answered fine -- the index did not -- so this is "Banager does
        // not know about these packages", not "this source failed". Failing
        // the source made every refresh on such a machine report an error
        // and hold the whole snapshot stale; cargo already answers this
        // question the way it is answered here.
        if output.exit_code != Some(0) {
            let reason =
                lookup_failure_reason("pip list --outdated", output.exit_code, &output.stderr);
            // The plain list, not `inventory()`: all this needs is what is
            // installed and at what version, and `inventory()` would run a
            // second `--not-required` pass to work out install reasons
            // nothing here reads.
            let installed = self.run_pip_list(inst, &[]).await?;
            return Ok(installed
                .into_iter()
                .map(|p| {
                    uncheckable_candidate(
                        ArtifactKey {
                            instance_id: inst.id.clone(),
                            kind: ArtifactKind::Package,
                            name: p.name,
                        },
                        p.version,
                        UpdateChannel::Native,
                        reason.clone(),
                    )
                })
                .collect::<Vec<_>>()
                .into());
        }
        Ok(parse_pip_outdated(&output.stdout, &inst.id)?.into())
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "pip has no search command; browse PyPI directly".to_string(),
        ))
    }

    /// pip is read-only in Banager (contract: `detect()` reports
    /// `read_only_reason: Some(ReadOnlyReason::ByDesign)`, and `plan()` must
    /// refuse every kind with a clear `AdapterError::Unsupported` rather than
    /// building an argv nobody should run). Every `OpKind` refuses here,
    /// before any argv is built, so the UI's install/uninstall/upgrade
    /// affordances for a pip-backed artifact never reach a working `Plan`.
    pub async fn plan(
        &self,
        _inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        Err(AdapterError::Unsupported(format!(
            "pip is read-only in Banager; use pipx or uv to manage {}",
            req.name
        )))
    }

    /// Unreachable by construction: `plan()` above refuses every `OpKind`,
    /// so no `Plan` for a pip instance can exist and nothing can ever reach
    /// this. The `Adapter` trait requires the method, so it states that
    /// rather than carrying thirty lines of runner plumbing nothing can
    /// call. (If this ever fires, `plan()` has gained a success path and
    /// this needs a real body.)
    pub async fn execute(
        &self,
        _plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        Err(AdapterError::Unsupported(
            "pip is read-only in Banager".to_string(),
        ))
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
impl Adapter for PipAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        PipAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        PipAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        PipAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        PipAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        PipAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        PipAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        PipAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_token_reads_pips_recorded_version_line() {
        // adapters/fixtures/pip/26.2.1/version.txt:
        // "pip 26.2.1 from /opt/homebrew/lib/python3.14/site-packages/pip (python 3.14)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against pip's real recorded output, and documents that the version
        // wanted here is pip's own, not the interpreter's Python version.
        assert_eq!(
            second_token(
                "pip 26.2.1 from /opt/homebrew/lib/python3.14/site-packages/pip (python 3.14)\n"
            ),
            Some("26.2.1".to_string())
        );
    }

    #[test]
    fn test_parse_pip_list_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list.json")
            .expect("read pip list.json fixture");
        let packages = parse_pip_list(&json).expect("parse pip list.json");
        assert_eq!(packages.len(), 7);
        assert!(packages
            .iter()
            .any(|p| p.name == "PyYAML" && p.version == "6.0.3"));
    }

    #[test]
    fn test_parse_pip_outdated_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-outdated.json")
            .expect("read pip list-outdated.json fixture");
        let candidates = parse_pip_outdated(&json, "pip:/opt/homebrew/bin/python3.14")
            .expect("parse pip list-outdated.json");
        assert_eq!(candidates.len(), 3);
        let wheel = candidates
            .iter()
            .find(|c| c.key.name == "wheel")
            .expect("wheel candidate");
        assert_eq!(wheel.current, "0.47.0");
        assert_eq!(wheel.target, "0.48.0");
        assert_eq!(wheel.channel, UpdateChannel::Native);
        assert!(wheel.checkable);
    }

    use crate::model::{OpKind, PlanAction, Warning};
    use crate::runner::{CommandOutput, MockRunner};

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/python3.14"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            version: Some("26.2.1".to_string()),
            read_only_reason: Some(ReadOnlyReason::ByDesign),
            ..crate::testing::manager_instance("pip", "pip:/opt/homebrew/bin/python3.14")
        }
    }

    #[tokio::test]
    async fn test_detect_marks_every_interpreter_read_only_by_design() {
        // pip is the one source with no install/uninstall path Banager can
        // safely drive, and that is a property of the tool, not of this
        // machine's permissions -- so every pip instance, on every Mac,
        // carries `ByDesign`. This replaces the front end's hardcoded
        // "pip is the read-only adapter" list: the wire now says so.
        let dir = std::env::temp_dir().join(format!(
            "banager-pip-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let python_path = dir.join("python3.14");
        std::fs::write(&python_path, b"#!/bin/sh\n").expect("write fake python");
        let python_path_str = python_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![python_path_str, "-m", "pip", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "pip 26.2.1 from /opt/lib/pip (python 3.14)\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, Some("26.2.1".to_string()));
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::ByDesign)
        );
        assert!(!instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_reports_unavailable_instead_of_vanishing_when_pip_module_cannot_be_reached(
    ) {
        // The interpreter itself was found on PATH, but `-m pip --version`
        // did not answer -- no pip module installed for it, or pip
        // crashed, or it timed out. Every other adapter that finds its
        // executable but cannot talk to it (brew/cargo/pipx/uv/ollama, all
        // on a failed `--version`) still reports the instance, marked
        // unavailable; pip used to `continue` past this interpreter
        // instead, silently dropping it -- indistinguishable from there
        // never having been a Python here at all. Unlike npm's id, this
        // one never depended on the failing command (`python_path` was
        // already resolved by `resolve_exe`), so there is no reason not to
        // report it.
        let dir = std::env::temp_dir().join(format!(
            "banager-pip-detect-unreachable-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let python_path = dir.join("python3.14");
        std::fs::write(&python_path, b"#!/bin/sh\n").expect("write fake python");
        let python_path_str = python_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![python_path_str, "-m", "pip", "--version"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "No module named pip".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, format!("pip:{}", python_path.display()));
        assert_eq!(instances[0].version, None);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert!(!instances[0].available());
        // Read-only by design is a property of pip itself, independent of
        // whether this round could reach it.
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::ByDesign)
        );
    }

    /// A fresh folder of the test's own, by its canonical path: `detect`
    /// compares `shim_dir` with where an interpreter leads, and the temp
    /// folder on a Mac is reached through a link (`/var` is `/private/var`).
    fn temp_folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "banager-pip-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp folder");
        std::fs::canonicalize(&dir).expect("canonicalize temp folder")
    }

    /// A file at `path` with the given mode, its folder created. Never run.
    fn file_at(path: &Path, mode: u32) {
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("create folder");
        std::fs::write(path, b"#!/bin/sh\nexit 1\n").expect("write file");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    }

    fn path_of(dirs: &[&Path]) -> HostEnv {
        HostEnv {
            path_dirs: dirs.iter().map(|dir| dir.to_path_buf()).collect(),
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }

    fn exited(code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(code),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    /// `path`, as `MockRunner` keys and records it.
    fn text(path: &Path) -> &str {
        path.to_str().expect("utf8 path")
    }

    /// A pip adapter whose developer-tool shims are in `shim_dir` -- a
    /// folder of the test's own, standing in for `/usr/bin`.
    fn adapter_with_shims_in(runner: Arc<MockRunner>, shim_dir: &Path) -> PipAdapter {
        PipAdapter {
            shim_dir: shim_dir.to_path_buf(),
            ..PipAdapter::new(runner)
        }
    }

    const PIP_VERSION: &str =
        "pip 21.2.4 from /Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/lib/python3.9/site-packages/pip (python 3.9)\n";

    #[tokio::test]
    async fn test_detect_never_runs_the_python3_shim_when_no_developer_tools_are_installed() {
        // A Mac without the Command Line Tools or Xcode: `/usr/bin/python3`
        // is the only `python3` on `PATH`, and running it opens the
        // system's dialog offering to install the tools. xcode-select names
        // no developer directory (here, it exits non-zero), so the shim is
        // never run and pip is simply not there -- no instance, no error.
        let shims = temp_folder("shims-no-tools");
        let shim = shims.join("python3");
        file_at(&shim, 0o755);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            PipAdapter::XCODE_SELECT_ARGV.to_vec(),
            CommandOutput {
                stderr: "xcode-select: error: unable to get active developer directory\n"
                    .to_string(),
                ..exited(2, "")
            },
        );
        let adapter = adapter_with_shims_in(runner.clone(), &shims);
        let instances = adapter.detect(&path_of(&[&shims])).await;
        let _ = std::fs::remove_dir_all(&shims);

        assert!(instances.is_empty(), "{instances:?}");
        assert_eq!(
            runner.calls(),
            vec![argv(&PipAdapter::XCODE_SELECT_ARGV)],
            "only the read-only question, never the shim"
        );
    }

    #[tokio::test]
    async fn test_detect_skips_the_shim_when_the_developer_directory_has_no_tool_of_its_name() {
        // xcode-select prints a directory it was told of without checking
        // it is there (`DEVELOPER_DIR=/nonexistent/dir xcode-select -p`
        // prints `/nonexistent/dir` and exits 0), so its answer counts only
        // when the tool is really under `usr/bin` there, executable, and
        // not the shim again; and an answer that is no absolute path
        // counts for nothing.
        let shims = temp_folder("shims-no-tool");
        let shim = shims.join("python3");
        file_at(&shim, 0o755);
        let gone = temp_folder("developer-gone").join("Developer");
        let not_executable = temp_folder("developer-not-executable");
        file_at(&not_executable.join("usr/bin/python3"), 0o644);
        let loops_back = temp_folder("developer-loops-back");
        std::fs::create_dir_all(loops_back.join("usr/bin")).expect("create usr/bin");
        std::os::unix::fs::symlink(&shim, loops_back.join("usr/bin/python3")).expect("link");
        for answer in [
            format!("{}\n", gone.display()),
            format!("{}\n", not_executable.display()),
            format!("{}\n", loops_back.display()),
            "\n".to_string(),
            "Developer\n".to_string(),
        ] {
            let runner = Arc::new(MockRunner::new());
            runner.respond(PipAdapter::XCODE_SELECT_ARGV.to_vec(), exited(0, &answer));
            let adapter = adapter_with_shims_in(runner.clone(), &shims);
            let instances = adapter.detect(&path_of(&[&shims])).await;
            assert!(instances.is_empty(), "{answer:?}: {instances:?}");
            assert_eq!(
                runner.calls(),
                vec![argv(&PipAdapter::XCODE_SELECT_ARGV)],
                "{answer:?}"
            );
        }
        for dir in [&shims, &gone, &not_executable, &loops_back] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[tokio::test]
    async fn test_detect_runs_the_python3_shim_as_before_when_the_developer_tools_are_installed() {
        let shims = temp_folder("shims-tools");
        let shim = shims.join("python3");
        file_at(&shim, 0o755);
        let developer = temp_folder("developer");
        file_at(&developer.join("usr/bin/python3"), 0o755);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            PipAdapter::XCODE_SELECT_ARGV.to_vec(),
            exited(0, &format!("{}\n", developer.display())),
        );
        runner.respond(
            vec![text(&shim), "-m", "pip", "--version"],
            exited(0, PIP_VERSION),
        );
        let adapter = adapter_with_shims_in(runner.clone(), &shims);
        let instances = adapter.detect(&path_of(&[&shims])).await;
        let _ = std::fs::remove_dir_all(&shims);
        let _ = std::fs::remove_dir_all(&developer);

        assert_eq!(instances.len(), 1, "{instances:?}");
        assert_eq!(instances[0].id, format!("pip:{}", shim.display()));
        assert_eq!(instances[0].exe_path, shim);
        assert_eq!(instances[0].version, Some("21.2.4".to_string()));
        assert_eq!(instances[0].status.unavailable, None);
        assert_eq!(
            runner.calls(),
            vec![
                argv(&PipAdapter::XCODE_SELECT_ARGV),
                argv(&[text(&shim), "-m", "pip", "--version"]),
            ]
        );
    }

    #[tokio::test]
    async fn test_detect_asks_xcode_select_nothing_for_a_python_outside_the_shim_folder() {
        // The real shim folder, `/usr/bin`, and a Python elsewhere on
        // `PATH`: run as it always was, and xcode-select is never asked.
        let dir = temp_folder("python-elsewhere");
        let python = dir.join("python3.14");
        file_at(&python, 0o755);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![text(&python), "-m", "pip", "--version"],
            exited(0, "pip 26.2.1 from /opt/lib/pip (python 3.14)\n"),
        );
        let adapter = PipAdapter::new(runner.clone());
        assert_eq!(adapter.shim_dir, PathBuf::from("/usr/bin"));
        let instances = adapter.detect(&path_of(&[&dir])).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1, "{instances:?}");
        assert_eq!(instances[0].version, Some("26.2.1".to_string()));
        assert_eq!(
            runner.calls(),
            vec![argv(&[text(&python), "-m", "pip", "--version"])]
        );
    }

    #[tokio::test]
    async fn test_detect_asks_xcode_select_once_a_refresh_and_again_at_the_next() {
        // Two shims on `PATH` (distinct files, so both are probed): one
        // question for both in a refresh. Its answer is not kept past the
        // refresh, so once the tools are installed the next refresh asks
        // again, finds them, and lists both.
        let shims = temp_folder("shims-twice");
        let python3 = shims.join("python3");
        let python = shims.join("python");
        file_at(&python3, 0o755);
        file_at(&python, 0o755);
        let runner = Arc::new(MockRunner::new());
        runner.respond(PipAdapter::XCODE_SELECT_ARGV.to_vec(), exited(2, ""));
        let adapter = adapter_with_shims_in(runner.clone(), &shims);
        let env = path_of(&[&shims]);

        let before = adapter.detect(&env).await;
        assert!(before.is_empty(), "{before:?}");
        assert_eq!(runner.calls(), vec![argv(&PipAdapter::XCODE_SELECT_ARGV)]);

        // The user installs the Command Line Tools.
        let developer = temp_folder("developer-installed");
        file_at(&developer.join("usr/bin/python3"), 0o755);
        file_at(&developer.join("usr/bin/python"), 0o755);
        runner.respond(
            PipAdapter::XCODE_SELECT_ARGV.to_vec(),
            exited(0, &format!("{}\n", developer.display())),
        );
        for shim in [&python3, &python] {
            runner.respond(
                vec![text(shim), "-m", "pip", "--version"],
                exited(0, PIP_VERSION),
            );
        }
        let after = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&shims);
        let _ = std::fs::remove_dir_all(&developer);

        assert_eq!(
            after.iter().map(|i| i.exe_path.clone()).collect::<Vec<_>>(),
            vec![python3.clone(), python.clone()]
        );
        assert_eq!(
            runner.calls(),
            vec![
                argv(&PipAdapter::XCODE_SELECT_ARGV),
                argv(&PipAdapter::XCODE_SELECT_ARGV),
                argv(&[text(&python3), "-m", "pip", "--version"]),
                argv(&[text(&python), "-m", "pip", "--version"]),
            ]
        );
    }

    #[tokio::test]
    async fn test_inventory_maps_the_recorded_fixture_pair_to_unknown_reason() {
        // adapters/fixtures/pip/26.2.1/list.json and list-not-required.json
        // are byte-identical on the recorded machine — every installed
        // package there happens to be a leaf nothing depends on, so every
        // one must map to Unknown, never Requested or Dependency.
        let runner = Arc::new(MockRunner::new());
        let list_json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list.json")
            .expect("read pip list.json fixture");
        let not_required_json =
            std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-not-required.json")
                .expect("read pip list-not-required.json fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list_json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
                "--not-required",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: not_required_json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 7);
        assert!(artifacts.iter().all(|a| a.reason == InstallReason::Unknown));
    }

    #[test]
    fn test_a_package_required_by_another_maps_to_dependency_reason() {
        // Edge case the recorded fixture pair cannot show (there all
        // packages are leaves): a package present in the full list but
        // absent from --not-required has something depending on it.
        let all = parse_pip_list(
            r#"[{"name":"six","version":"1.16.0"},{"name":"leaf","version":"1.0.0"}]"#,
        )
        .expect("parse full list");
        let not_required = parse_pip_list(r#"[{"name":"leaf","version":"1.0.0"}]"#)
            .expect("parse not-required list");
        let leaf_names: HashSet<String> = not_required.into_iter().map(|p| p.name).collect();
        let reasons: Vec<InstallReason> = all
            .into_iter()
            .map(|p| {
                if leaf_names.contains(&p.name) {
                    InstallReason::Unknown
                } else {
                    InstallReason::Dependency
                }
            })
            .collect();
        assert_eq!(
            reasons,
            vec![InstallReason::Dependency, InstallReason::Unknown]
        );
    }

    #[tokio::test]
    async fn test_check_updates_calls_list_outdated_and_parses_the_fixture_output() {
        let json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-outdated.json")
            .expect("read pip list-outdated.json fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--outdated",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 3);
    }

    #[tokio::test]
    async fn test_check_updates_marks_every_package_uncheckable_when_the_lookup_fails() {
        // `pip list --outdated` reaches out to PyPI. When it cannot, that is
        // not "pip is broken" -- pip answered, the index did not. Failing the
        // whole source made every refresh on such a machine report an error
        // and hold the entire snapshot stale, which is exactly what cargo's
        // own comment says must not happen.
        let list = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list.json")
            .expect("read pip list.json fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--outdated",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "WARNING: Retrying ... Read timed out.\nERROR: Could not fetch URL https://pypi.org/simple/".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("an index that did not answer is not a source failure")
            .candidates;
        assert_eq!(candidates.len(), 7);
        assert!(candidates.iter().all(|c| !c.checkable));
        assert!(candidates.iter().all(|c| c
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Message(m) if m.contains("Read timed out")))));
    }

    #[tokio::test]
    async fn test_plan_refuses_every_op_kind() {
        let adapter = PipAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        for kind in [OpKind::Install, OpKind::Uninstall, OpKind::Upgrade] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "wheel".to_string(),
            };
            let result = PipAdapter::plan(&adapter, &inst, &req).await;
            match result {
                Err(AdapterError::Unsupported(_)) => {}
                other => panic!("expected Unsupported for {kind:?}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn test_execute_refuses_because_no_pip_plan_can_exist() {
        // Guards the claim in execute()'s doc comment. If plan() ever grows
        // a success path, this test is what says "execute now needs a real
        // body" instead of silently running nothing.
        let adapter = PipAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "wheel".to_string(),
            },
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args: vec!["-m".to_string(), "pip".to_string()],
                env: Vec::new(),
            },
            needs_password: false,
            locks: Vec::new(),
            cancel_policy: crate::model::CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 60,
        };
        let result = PipAdapter::execute(
            &adapter,
            &plan,
            Arc::new(crate::events::VecSink::new()),
            1,
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let list_json = r#"[{"name":"wheel","version":"0.47.0"}]"#.to_string();
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list_json.clone(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
                "--not-required",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list_json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let inst = test_instance();
        let present = PipAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Package,
                name: "wheel".to_string(),
            },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        let absent = PipAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Package,
                name: "missing".to_string(),
            },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
    }
}
