use crate::adapters::{
    ensure_instance_match, reconcile_from, run_plan, second_token, uncheckable_candidate,
    url_path_segment, validate_package_name, Adapter, AdapterError, AdapterMeta, CheckOptions,
    CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, InstanceStatus,
    ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit,
    Unavailable, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// `.crates2.json`'s `installs` object carries the package name, version and
/// source **in the JSON key** — e.g. `"hexyl 0.17.0 (registry+https://
/// github.com/rust-lang/crates.io-index)"` — not in the value, which only
/// has `bins`/`features`/`profile`/`rustc`/`target`/`version_req` (this
/// phase's documented trap for cargo). Splits that key into
/// `(name, version, source_kind)`, where `source_kind` is the part before
/// the first `+` inside the parens (`"registry"`, `"git"` or `"path"`).
fn parse_install_key(key: &str) -> Option<(String, String, String)> {
    let mut parts = key.splitn(3, ' ');
    let name = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    let source = parts.next()?;
    let source = source.strip_prefix('(')?.strip_suffix(')')?;
    let kind = source.split('+').next().unwrap_or(source).to_string();
    Some((name, version, kind))
}

#[derive(Debug, Deserialize)]
struct Crates2Root {
    #[serde(default)]
    installs: HashMap<String, serde_json::Value>,
}

fn parse_crates2_entries(json: &str) -> Result<Vec<(String, String, String)>, AdapterError> {
    let root: Crates2Root =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut entries: Vec<(String, String, String)> = root
        .installs
        .keys()
        .filter_map(|k| parse_install_key(k))
        .collect();
    entries.sort();
    Ok(entries)
}

fn parse_crates2(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let entries = parse_crates2_entries(json)?;
    Ok(entries
        .into_iter()
        .map(|(name, version, _source_kind)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Binary,
                name: name.clone(),
            },
            display_name: name,
            version,
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect())
}

/// Real detection: resolves `cargo-binstall` through `HostEnv`'s hydrated
/// `PATH` — the same list `cargo` itself would search for a
/// `cargo-<subcommand>` plugin, and the same list every other adapter
/// resolves its executable from. Returns the resolved path, not a boolean,
/// so `plan` can preview exactly the program that will run; an earlier draft
/// scanned the *process* `PATH` for a yes/no answer and then previewed a
/// sibling-of-cargo path that the scan had never checked. A plain `fn`
/// pointer (not a closure) so tests can swap in a fixed answer — a test
/// cannot control whether the machine running it has cargo-binstall.
fn default_binstall_check(env: &HostEnv) -> Option<PathBuf> {
    resolve_exe("cargo-binstall", env)
}

pub struct CargoAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
    binstall_check: fn(&HostEnv) -> Option<PathBuf>,
    /// The path `detect` last resolved for cargo-binstall, or `None` when it
    /// is not installed. `plan` has no `HostEnv` of its own — the `Adapter`
    /// trait gives it only an instance — and `Session` always refreshes, and
    /// therefore detects, before it will issue a plan for an instance, so
    /// reading the cached answer here is what makes the previewed
    /// `Plan::program` the exact path that will run.
    binstall: Mutex<Option<PathBuf>>,
}

impl CargoAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> CargoAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/cargo.toml"))
            .expect("adapters/meta/cargo.toml must parse");
        CargoAdapter {
            runner,
            http,
            meta,
            binstall_check: default_binstall_check,
            binstall: Mutex::new(None),
        }
    }

    /// Test seam: pin what `plan` will believe about cargo-binstall without
    /// running `detect` (and so without depending on the host machine).
    #[cfg(test)]
    fn with_binstall(self, path: Option<PathBuf>) -> CargoAdapter {
        *self.binstall.lock().unwrap() = path;
        self
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("cargo", env) else {
            return Vec::new();
        };
        let cargo_home = env
            .cargo_home
            .clone()
            .unwrap_or_else(|| env.home.join(".cargo"));
        *self.binstall.lock().unwrap() = (self.binstall_check)(env);
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
            // "cargo 1.98.1 (hash date)" — the shared second-token rule
            // (crate::adapters::second_token, Task 5).
            Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
            _ => None,
        };
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: format!("cargo:{}", cargo_home.display()),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix: cargo_home,
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

    /// A Rust toolchain that has never run `cargo install` has no
    /// `.crates2.json` at all, which is "nothing installed", not a failure —
    /// treating it as one would make every refresh on such a machine report
    /// a per-instance error and hold the whole snapshot permanently stale.
    /// Any other IO error (an unreadable or truncated file) is still an
    /// error.
    fn read_crates2(&self, inst: &ManagerInstance) -> Result<String, AdapterError> {
        let path = inst.prefix.join(".crates2.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => Ok(json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok("{\"installs\":{}}".to_string())
            }
            Err(e) => Err(AdapterError::Parse(format!(
                "reading {}: {e}",
                path.display()
            ))),
        }
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let json = self.read_crates2(inst)?;
        parse_crates2(&json, &inst.id)
    }

    async fn latest_stable_version(&self, name: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct CratesIoResponse {
            #[serde(rename = "crate")]
            krate: CrateInfo,
        }
        #[derive(Deserialize)]
        struct CrateInfo {
            max_stable_version: String,
        }
        let resp = self
            .http
            .send(HttpRequest {
                method: "GET",
                // Percent-encoded: the crate name is a `.crates2.json` key,
                // i.e. off disk, and raw it could add path segments or a
                // query string to crates.io's API url.
                url: format!(
                    "https://crates.io/api/v1/crates/{}",
                    url_path_segment(name)?
                ),
                headers: Vec::new(),
                timeout: Duration::from_secs(30),
            })
            .await
            .map_err(|e| format!("crates.io request failed: {e}"))?;
        if resp.status != 200 {
            return Err(format!("crates.io returned status {}", resp.status));
        }
        let parsed: CratesIoResponse = serde_json::from_str(&resp.body)
            .map_err(|e| format!("could not parse crates.io response: {e}"))?;
        Ok(parsed.krate.max_stable_version)
    }

    /// Registry-sourced crates are checked one at a time against crates.io.
    /// Git and path sources are `checkable: false` with a reason
    /// unconditionally — Canager has no way to check those for updates at
    /// all, so every such crate always gets a row explaining why, not just
    /// the ones that happen to be outdated (contract: "git and path sources
    /// are checkable: false with a reason").
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let json = self.read_crates2(inst)?;
        let entries = parse_crates2_entries(&json)?;
        let mut out = Vec::new();
        for (name, version, source_kind) in entries {
            let key = ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: name.clone(),
            };
            if source_kind != "registry" {
                out.push(UpdateCandidate {
                    key,
                    current: version.clone(),
                    target: version,
                    channel: UpdateChannel::Registry,
                    checkable: false,
                    warnings: vec![Warning::NonRegistrySource],
                });
                continue;
            }
            match self.latest_stable_version(&name).await {
                Ok(latest) if latest != version => out.push(UpdateCandidate {
                    key,
                    current: version,
                    target: latest,
                    channel: UpdateChannel::Registry,
                    checkable: true,
                    warnings: Vec::new(),
                }),
                Ok(_) => {}
                Err(reason) => out.push(uncheckable_candidate(
                    key,
                    version,
                    UpdateChannel::Registry,
                    reason,
                )),
            }
        }
        Ok(out.into())
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "cargo has no search command Canager uses; browse crates.io directly".to_string(),
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
        match req.kind {
            OpKind::Install | OpKind::Upgrade => {
                // The path `detect` resolved through HostEnv, not a fresh
                // guess: whatever is previewed here is exactly what runs.
                let binstall = self.binstall.lock().unwrap().clone();
                let mut warnings = Vec::new();
                let (program, mut args) = match binstall {
                    Some(path) => (path, vec!["-y".to_string()]),
                    None => {
                        warnings.push(Warning::CompilesLocally);
                        (inst.exe_path.clone(), vec!["install".to_string()])
                    }
                };
                if matches!(req.kind, OpKind::Upgrade) {
                    args.push("--force".to_string());
                }
                args.push(req.name.clone());
                Ok(Plan {
                    request: req.clone(),
                    program,
                    args,
                    env: Vec::new(),
                    needs_password: false,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => Ok(Plan {
                request: req.clone(),
                program: inst.exe_path.clone(),
                args: vec!["uninstall".to_string(), req.name.clone()],
                env: Vec::new(),
                needs_password: false,
                locks: vec![lock],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: Vec::new(),
                affected: Vec::new(),
                timeout_secs: 300,
            }),
        }
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
impl Adapter for CargoAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        CargoAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        CargoAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        CargoAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        CargoAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        CargoAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        CargoAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        CargoAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_token_reads_cargos_recorded_version_line() {
        // adapters/fixtures/cargo/1.98.1/version.txt:
        // "cargo 1.98.1 (797e8a9bc 2026-08-05)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against cargo's real recorded output.
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
    }

    #[test]
    fn test_parse_install_key_splits_name_version_and_source_kind() {
        // ".crates2.json keeps the package name, version and source in the
        // JSON key" (this phase's documented trap for cargo).
        let key = "hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)";
        assert_eq!(
            parse_install_key(key),
            Some((
                "hexyl".to_string(),
                "0.17.0".to_string(),
                "registry".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_install_key_recognizes_git_and_path_sources() {
        assert_eq!(
            parse_install_key("my-fork 0.1.0 (git+https://github.com/example/my-fork#abc123)"),
            Some((
                "my-fork".to_string(),
                "0.1.0".to_string(),
                "git".to_string()
            ))
        );
        assert_eq!(
            parse_install_key("local-tool 0.1.0 (path+file:///Users/brulek/dev/local-tool)"),
            Some((
                "local-tool".to_string(),
                "0.1.0".to_string(),
                "path".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_crates2_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let artifacts =
            parse_crates2(&json, "cargo:/Users/brulek/.cargo").expect("parse crates2.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Binary);
        assert_eq!(artifacts[0].key.name, "hexyl");
        assert_eq!(artifacts[0].version, "0.17.0");
    }

    #[test]
    fn test_default_binstall_check_resolves_cargo_binstall_through_host_env() {
        // The real resolver must read HostEnv's hydrated PATH, not the
        // process PATH: a Finder-launched app's process PATH is minimal, and
        // an answer taken from it could name a path `plan` then previews but
        // never runs. A dedicated temp directory stands in for a PATH entry,
        // so this cannot depend on whether the machine running it actually
        // has cargo-binstall installed.
        let dir = std::env::temp_dir().join(format!(
            "canager-binstall-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert_eq!(default_binstall_check(&env), None);

        let exe = dir.join("cargo-binstall");
        std::fs::write(&exe, b"#!/bin/sh\n").expect("write fake cargo-binstall");
        assert_eq!(default_binstall_check(&env), Some(exe));

        let _ = std::fs::remove_dir_all(&dir);
    }

    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::runner::{CommandOutput, MockRunner};

    fn temp_cargo_home(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-cargo-home-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn test_instance(prefix: PathBuf) -> ManagerInstance {
        let id = format!("cargo:{}", prefix.display());
        ManagerInstance {
            exe_path: PathBuf::from("/Users/brulek/.cargo/bin/cargo"),
            prefix,
            version: Some("1.98.1".to_string()),
            ..crate::testing::manager_instance("cargo", &id)
        }
    }

    #[tokio::test]
    async fn test_check_updates_flags_the_fixture_crate_as_outdated() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("outdated");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/hexyl",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"0.18.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "hexyl");
        assert!(candidates[0].checkable);
        assert_eq!(candidates[0].target, "0.18.0");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_reports_nothing_when_the_fixture_crate_is_current() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("current");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/hexyl",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"0.17.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_git_sourced_crate_as_uncheckable() {
        // Edge case the recorded fixture (a single registry-sourced crate)
        // cannot show: a crate installed from a git repository.
        let json = r#"{"installs":{"my-fork 0.1.0 (git+https://github.com/example/my-fork#abc123)":{"version_req":null,"bins":["my-fork"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"}}}"#;
        let home = temp_cargo_home("git-source");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert_eq!(candidates[0].warnings, vec![Warning::NonRegistrySource]);
        assert!(
            http.calls().is_empty(),
            "a git-sourced crate must never reach crates.io"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_inventory_of_a_cargo_home_with_no_crates2_json_is_empty_not_an_error() {
        // A Rust toolchain that has never run `cargo install` has no
        // .crates2.json. That is "nothing installed", not a failed refresh:
        // an error here would push a SourceError and hold the whole snapshot
        // stale on every refresh, forever, on an entirely healthy machine.
        let home = temp_cargo_home("empty");
        std::fs::create_dir_all(&home).expect("create cargo home");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert!(artifacts.is_empty());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-cargo".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let result = CargoAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_without_binstall_compiles_and_warns() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        assert_eq!(
            plan.program,
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(plan.args, vec!["install", "hexyl"]);
        assert_eq!(plan.warnings, vec![Warning::CompilesLocally]);
    }

    #[tokio::test]
    async fn test_plan_upgrade_with_binstall_skips_the_compile_warning() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(Some(PathBuf::from(
                    "/Users/brulek/.cargo/bin/cargo-binstall",
                )));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        assert_eq!(
            plan.program,
            PathBuf::from("/Users/brulek/.cargo/bin/cargo-binstall")
        );
        assert_eq!(plan.args, vec!["-y", "--force", "hexyl"]);
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_plan_uninstall_never_uses_binstall() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(Some(PathBuf::from(
                    "/Users/brulek/.cargo/bin/cargo-binstall",
                )));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        assert_eq!(
            plan.program,
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(plan.args, vec!["uninstall", "hexyl"]);
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/Users/brulek/.cargo/bin/cargo", "install", "hexyl"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installing hexyl\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter =
            CargoAdapter::new(runner, Arc::new(MockHttpClient::new())).with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome =
            CargoAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("reconcile");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let present = CargoAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: "hexyl".to_string(),
            },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        let absent = CargoAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: "missing".to_string(),
            },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
        let _ = std::fs::remove_dir_all(&home);
    }

    fn fake_exe(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").expect("write fake executable");
        path
    }

    fn version_runner(cargo_path: &str, stdout: &str) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![cargo_path, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: stdout.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    #[tokio::test]
    async fn test_detect_keys_the_instance_on_cargo_home_and_caches_the_binstall_path() {
        // `cargo_home` is the whole reason HostEnv grew a field in this task:
        // a machine with CARGO_HOME set keeps its crates somewhere other than
        // ~/.cargo, and `inventory` reads `.crates2.json` out of the instance
        // prefix. This also pins the hand-off detect -> plan: the program the
        // preview names is the path detect resolved, not a fresh guess.
        let dir = std::env::temp_dir().join(format!(
            "canager-cargo-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let cargo_path = fake_exe(&dir, "cargo");
        let binstall_path = fake_exe(&dir, "cargo-binstall");
        let runner = version_runner(
            cargo_path.to_str().expect("utf8 path"),
            "cargo 1.98.1 (797e8a9bc 2026-08-05)\n",
        );
        let adapter = CargoAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: Some(PathBuf::from("/opt/cargo")),
            ollama_host: None,
        };
        let instances = CargoAdapter::detect(&adapter, &env).await;

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "cargo:/opt/cargo");
        assert_eq!(instances[0].prefix, PathBuf::from("/opt/cargo"));
        assert_eq!(instances[0].exe_path, cargo_path);
        assert_eq!(instances[0].version, Some("1.98.1".to_string()));
        assert!(instances[0].available());
        assert!(
            instances[0].unverified_version.is_none(),
            "1.98.1 is verified in adapters/meta/cargo.toml"
        );

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: instances[0].id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &instances[0], &req)
            .await
            .expect("plan");
        assert_eq!(plan.program, binstall_path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_detect_falls_back_to_home_dot_cargo_and_flags_an_unverified_version() {
        let dir = std::env::temp_dir().join(format!(
            "canager-cargo-detect-default-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let cargo_path = fake_exe(&dir, "cargo");
        let runner = version_runner(
            cargo_path.to_str().expect("utf8 path"),
            "cargo 99.9.9 (deadbeef 2099-01-01)\n",
        );
        let adapter = CargoAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/Users/brulek"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        let instances = CargoAdapter::detect(&adapter, &env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "cargo:/Users/brulek/.cargo");
        assert_eq!(
            instances[0].unverified_version,
            Some("99.9.9".to_string()),
            "99.9.9 is not in adapters/meta/cargo.toml's verified_versions"
        );
    }

    #[tokio::test]
    async fn test_detect_returns_no_instance_when_cargo_is_not_on_path() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/definitely/not/a/real/path")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert!(CargoAdapter::detect(&adapter, &env).await.is_empty());
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let result = <CargoAdapter as Adapter>::search(&adapter, &inst, "hexyl").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_latest_stable_version_percent_encodes_the_crate_name_into_the_url() {
        // The crate name comes off disk, out of `.crates2.json`'s keys —
        // a file Canager does not write. Interpolated raw, a `/` in it adds
        // path segments to crates.io's API and a `?` starts a query string.
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/evil%2F..%2Fsummary%3Fx=1",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"1.0.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());

        let latest = adapter.latest_stable_version("evil/../summary?x=1").await;

        assert_eq!(latest.as_deref(), Ok("1.0.0"));
        assert_eq!(
            http.calls(),
            vec!["https://crates.io/api/v1/crates/evil%2F..%2Fsummary%3Fx=1".to_string()]
        );
    }
}
