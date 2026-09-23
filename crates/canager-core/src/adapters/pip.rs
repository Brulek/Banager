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
use std::path::PathBuf;
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
}

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

    pub fn new(runner: Arc<dyn CommandRunner>) -> PipAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/pip.toml"))
            .expect("adapters/meta/pip.toml must parse");
        PipAdapter { runner, meta }
    }

    /// One `ManagerInstance` per distinct Python interpreter on `PATH` that
    /// has a working `pip` module (contract: "one per interpreter, invoked
    /// as `{python} -m pip`"). Interpreters are deduplicated by their
    /// canonicalized path so `python3` and `python3.14` naming the same
    /// binary do not produce two instances.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for name in Self::CANDIDATE_INTERPRETERS {
            let Some(python_path) = resolve_exe(name, env) else {
                continue;
            };
            let canonical =
                std::fs::canonicalize(&python_path).unwrap_or_else(|_| python_path.clone());
            if !seen.insert(canonical) {
                continue;
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
                // install/uninstall path Canager can safely drive, so
                // every pip instance anywhere is read-only by design --
                // whether or not this round could reach it.
                read_only_reason: Some(ReadOnlyReason::ByDesign),
            });
        }
        found
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
    /// Canager what the user explicitly typed `pip install` for, so
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
        // answered fine -- the index did not -- so this is "Canager does
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

    /// pip is read-only in Canager (contract: `detect()` reports
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
            "pip is read-only in Canager; use pipx or uv to manage {}",
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
            "pip is read-only in Canager".to_string(),
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

    use crate::model::{OpKind, Warning};
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
        // pip is the one source with no install/uninstall path Canager can
        // safely drive, and that is a property of the tool, not of this
        // machine's permissions -- so every pip instance, on every Mac,
        // carries `ByDesign`. This replaces the front end's hardcoded
        // "pip is the read-only adapter" list: the wire now says so.
        let dir = std::env::temp_dir().join(format!(
            "canager-pip-detect-{}-{}",
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
            "canager-pip-detect-unreachable-{}-{}",
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
            program: inst.exe_path.clone(),
            args: vec!["-m".to_string(), "pip".to_string()],
            env: Vec::new(),
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
