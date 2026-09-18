pub mod parse;

use crate::adapters::{validate_package_name, Adapter, AdapterError, AdapterMeta, Capabilities};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, InstanceId, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback};
use async_trait::async_trait;
use parse::{parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub struct BrewAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// Per-instance `brew update` TTL bookkeeping. Keyed by `ManagerInstance
    /// id` (e.g. `brew:/opt/homebrew` vs `brew:/usr/local`) so that two
    /// installs of Homebrew each get their own throttle instead of sharing
    /// a single adapter-wide timer.
    last_update: Mutex<HashMap<InstanceId, Instant>>,
    update_ttl: Duration,
    /// How to read the *real* effective UID for the root-refusal check on
    /// every brew subprocess call (not just `detect`, which instead checks
    /// the caller-supplied `HostEnv::euid`). A plain fn pointer (rather than
    /// a boxed closure) keeps this injectable for tests without touching
    /// `BrewAdapter::new`'s public signature: production code always gets
    /// the default `|| unsafe { libc::geteuid() }`, and tests can swap in
    /// `|| 0` via the `#[cfg(test)]`-only `with_euid_fn`.
    euid_fn: fn() -> u32,
}

impl BrewAdapter {
    pub const ENV: [(&'static str, &'static str); 4] = [
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
        ("HOMEBREW_NO_ENV_HINTS", "1"),
        ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
        ("NO_COLOR", "1"),
    ];

    pub const CANDIDATE_PATHS: [&'static str; 3] = [
        "/opt/homebrew/bin/brew",
        "/usr/local/bin/brew",
        "/home/linuxbrew/.linuxbrew/bin/brew",
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> BrewAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../../adapters/meta/brew.toml"))
            .expect("adapters/meta/brew.toml must parse");
        BrewAdapter {
            runner,
            meta,
            last_update: Mutex::new(HashMap::new()),
            update_ttl: Duration::from_secs(6 * 3600),
            euid_fn: || unsafe { libc::geteuid() },
        }
    }

    pub fn with_update_ttl(mut self, ttl: Duration) -> BrewAdapter {
        self.update_ttl = ttl;
        self
    }

    /// Test-only hook to inject a fake euid, since a test cannot change its
    /// own real effective UID. Deliberately not part of the public API
    /// surface (`BrewAdapter::new`'s signature is unchanged).
    #[cfg(test)]
    fn with_euid_fn(mut self, euid_fn: fn() -> u32) -> BrewAdapter {
        self.euid_fn = euid_fn;
        self
    }

    /// The common root-refusal gate for every brew subprocess invocation
    /// except `detect` (which checks the caller-supplied `HostEnv::euid`
    /// instead, by design — see its own doc comment). Called from
    /// `run_brew` (covers inventory/check_updates/search/plan's `uses`
    /// lookup) and from `execute` (which talks to the runner directly and
    /// so does not go through `run_brew`).
    fn refuse_if_root(&self) -> Result<(), AdapterError> {
        if (self.euid_fn)() == 0 {
            return Err(AdapterError::Refused(
                "refusing to run Homebrew as root".to_string(),
            ));
        }
        Ok(())
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn prefix_for(exe_path: &Path) -> PathBuf {
        exe_path
            .parent()
            .and_then(|bin| bin.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn instance_id_for(prefix: &Path) -> String {
        format!("brew:{}", prefix.display())
    }

    async fn run_brew(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        self.refuse_if_root()?;
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
            cwd: None,
            timeout,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    async fn maybe_update(&self, inst: &ManagerInstance) -> Result<(), AdapterError> {
        let needs_update = {
            let last = self.last_update.lock().unwrap();
            match last.get(&inst.id) {
                Some(t) => t.elapsed() >= self.update_ttl,
                None => true,
            }
        };
        if !needs_update {
            return Ok(());
        }
        let output = self
            .run_brew(inst, vec!["update".to_string()], Duration::from_secs(120))
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        self.last_update
            .lock()
            .unwrap()
            .insert(inst.id.clone(), Instant::now());
        Ok(())
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        if env.euid == 0 {
            return Vec::new();
        }
        let mut found = Vec::new();
        for candidate in Self::CANDIDATE_PATHS {
            let path = PathBuf::from(candidate);
            if !path.exists() {
                continue;
            }
            let spec = CommandSpec {
                program: path.clone(),
                args: vec!["--version".to_string()],
                env: self.env_vec(),
                cwd: None,
                timeout: Duration::from_secs(30),
            };
            let output = self.runner.run(spec, None, CancellationToken::new()).await;
            let version = match output {
                Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
                _ => None,
            };
            let prefix = Self::prefix_for(&path);
            found.push(ManagerInstance {
                id: Self::instance_id_for(&prefix),
                adapter_id: self.meta.id.clone(),
                exe_path: path,
                prefix,
                scope: Scope::User,
                healthy: version.is_some(),
                version,
            });
        }
        found
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_brew(
                inst,
                vec![
                    "info".to_string(),
                    "--installed".to_string(),
                    "--json=v2".to_string(),
                ],
                Duration::from_secs(120),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_info_installed(&output.stdout, &inst.id)
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        self.maybe_update(inst).await?;
        let output = self
            .run_brew(
                inst,
                vec!["outdated".to_string(), "--json=v2".to_string()],
                Duration::from_secs(120),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_outdated(&output.stdout, &inst.id)
    }

    pub async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        validate_package_name(query)?;
        let names_output = self
            .run_brew(
                inst,
                vec!["search".to_string(), query.to_string()],
                Duration::from_secs(30),
            )
            .await?;
        if names_output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: names_output.exit_code,
                stderr: names_output.stderr,
            });
        }
        let desc_output = self
            .run_brew(
                inst,
                vec![
                    "search".to_string(),
                    "--desc".to_string(),
                    query.to_string(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        if desc_output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: desc_output.exit_code,
                stderr: desc_output.stderr,
            });
        }
        let mut hits = parse_search(&desc_output.stdout, &self.meta.id);
        if hits.is_empty() {
            hits = parse_search(&names_output.stdout, &self.meta.id);
        }
        Ok(hits)
    }
}

impl BrewAdapter {
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
        match req.kind {
            OpKind::Install => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                let needs_password = matches!(req.artifact_kind, ArtifactKind::Cask);
                let mut env = self.env_vec();
                if let Ok(askpass) = std::env::var("SUDO_ASKPASS") {
                    env.push(("SUDO_ASKPASS".to_string(), askpass));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["install".to_string(), flag.to_string(), req.name.clone()],
                    env,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                let uses_output = self
                    .run_brew(
                        inst,
                        vec![
                            "uses".to_string(),
                            "--installed".to_string(),
                            req.name.clone(),
                        ],
                        Duration::from_secs(120),
                    )
                    .await?;
                let mut warnings = Vec::new();
                let affected = if uses_output.exit_code == Some(0) {
                    parse_uses(&uses_output.stdout)
                } else {
                    // The check itself failed or timed out — this is *not*
                    // the same thing as "confirmed no dependents", and must
                    // not be presented as if it were.
                    warnings.push(format!(
                        "could not determine what depends on {}; uninstalling may break other packages",
                        req.name
                    ));
                    Vec::new()
                };
                if !affected.is_empty() {
                    warnings.push(format!(
                        "Removing {} will break: {}",
                        req.name,
                        affected.join(", ")
                    ));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["uninstall".to_string(), flag.to_string(), req.name.clone()],
                    env: self.env_vec(),
                    needs_password: matches!(req.artifact_kind, ArtifactKind::Cask),
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected,
                    timeout_secs: 1800,
                })
            }
            OpKind::Upgrade => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                let needs_password = matches!(req.artifact_kind, ArtifactKind::Cask);
                let mut env = self.env_vec();
                if let Ok(askpass) = std::env::var("SUDO_ASKPASS") {
                    env.push(("SUDO_ASKPASS".to_string(), askpass));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["upgrade".to_string(), flag.to_string(), req.name.clone()],
                    env,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
        }
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        self.refuse_if_root()?;
        let sink_for_line = sink.clone();
        let on_line: LineCallback = Arc::new(move |stream, line| {
            sink_for_line.emit(crate::events::OperationEvent::Log {
                op_id,
                stream,
                line,
            });
        });
        let spec = CommandSpec {
            program: plan.program.clone(),
            args: plan.args.clone(),
            env: plan.env.clone(),
            cwd: None,
            timeout: Duration::from_secs(plan.timeout_secs),
        };
        let output = self.runner.run(spec, Some(on_line), cancel).await?;
        if output.cancelled || output.timed_out {
            return Ok(Outcome::Unconfirmed);
        }
        match output.exit_code {
            Some(0) => Ok(Outcome::Succeeded),
            code => {
                let stderr_lines: Vec<&str> = output.stderr.lines().collect();
                let start = stderr_lines.len().saturating_sub(5);
                let summary = stderr_lines[start..].join("\n");
                Ok(Outcome::Failed {
                    exit_code: code,
                    summary,
                })
            }
        }
    }

    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        // Inventory keys are always the fully-qualified name (see
        // `parse::parse_info_installed`), but a caller may have started the
        // operation from a short name (e.g. the user typed `claudebar`
        // rather than `gautham-v/tap/claudebar`, or an older `OpRequest` was
        // built before full names existed). Match either the exact full
        // name or its last `/`-separated segment so both spellings find the
        // same artifact.
        match artifacts.into_iter().find(|a| {
            a.key.kind == key.kind
                && (a.key.name == key.name
                    || a.key.name.rsplit('/').next() == Some(key.name.as_str()))
        }) {
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

// Adapter is implemented by forwarding to the inherent methods above via
// fully-qualified `BrewAdapter::method(self, ...)` calls. This is
// unambiguous even though the method names match: `BrewAdapter::detect`
// can only resolve to the inherent impl (a trait method would be spelled
// `<BrewAdapter as Adapter>::detect`), so there is no risk of the trait
// method accidentally calling itself.
#[async_trait]
impl Adapter for BrewAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            search: true,
            per_item_upgrade: true,
            upgrade_all: false,
            uninstall: true,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        BrewAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        BrewAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        BrewAdapter::check_updates(self, inst).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        BrewAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        BrewAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        BrewAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        BrewAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;
    use crate::runner::MockRunner;
    use std::sync::Arc;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
        }
    }

    /// (F5 / M7) Every brew subprocess entry point besides `detect` must
    /// also refuse to run as root — `detect` checks the *passed-in*
    /// `HostEnv::euid`, but everything else (inventory, check_updates,
    /// search, plan, execute) previously took a ready-made `ManagerInstance`
    /// and ran brew unconditionally regardless of the *real* euid. `euid_fn`
    /// is injected here (via the `#[cfg(test)]`-only `with_euid_fn`
    /// constructor) since a real test cannot change its own euid.
    #[tokio::test]
    async fn test_inventory_refuses_as_root() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_euid_fn(|| 0);
        let result = adapter.inventory(&test_instance()).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_eq!(
            runner.calls().len(),
            0,
            "no brew process should ever be started as root"
        );
    }

    #[tokio::test]
    async fn test_detect_refuses_root() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
        };
        let instances = adapter.detect(&env).await;
        assert!(instances.is_empty());
    }

    #[tokio::test]
    async fn test_detect_finds_opt_homebrew_on_this_apple_silicon_mac() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 7.0.3\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let instances = adapter.detect(&env).await;
        // Assumes Homebrew is installed at /opt/homebrew, true for Canager's
        // target (Apple Silicon Macs, per the design spec) and for CI's
        // macos-latest runners. /usr/local/bin/brew and the Linux path do
        // not exist on this machine, so exactly one instance is found.
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "brew:/opt/homebrew");
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert!(instances[0].healthy);
    }

    #[tokio::test]
    async fn test_inventory_parses_formula_and_cask() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{
            "formulae": [
                {"name":"jq","desc":"JSON processor","homepage":"https://jqlang.org","linked_keg":"1.7.1","installed":[{"version":"1.7.1","installed_on_request":true,"installed_as_dependency":false,"time":1700000000}]}
            ],
            "casks": [
                {"token":"claudebar","name":["ClaudeBar"],"desc":"Menu bar app","homepage":"https://example.invalid","installed":"1.0.0","auto_updates":false}
            ]
        }"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Formula);
        assert_eq!(artifacts[1].key.kind, ArtifactKind::Cask);
    }

    #[tokio::test]
    async fn test_check_updates_respects_ttl() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated_json = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1","pinned":false,"pinned_version":null}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated_json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(3600));
        let inst = test_instance();

        let first = adapter
            .check_updates(&inst)
            .await
            .expect("first check_updates");
        assert_eq!(first.len(), 1);
        let second = adapter
            .check_updates(&inst)
            .await
            .expect("second check_updates");
        assert_eq!(second.len(), 1);

        let calls = mock_ref.calls();
        let update_calls = calls
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        let outdated_calls = calls
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("outdated"))
            .count();
        assert_eq!(
            update_calls, 1,
            "brew update should run once within the TTL window"
        );
        assert_eq!(outdated_calls, 2);
    }

    /// (F7 / M6) `last_update` used to be a single `Option<Instant>` shared
    /// by the whole adapter, so checking `/opt/homebrew` first and then
    /// `/usr/local` would make the second instance skip its *own* `brew
    /// update` even though it has never run one. Each `ManagerInstance` must
    /// get its own TTL bookkeeping.
    #[tokio::test]
    async fn test_check_updates_ttl_is_tracked_per_instance() {
        let runner = Arc::new(MockRunner::new());
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        for brew in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
            runner.respond(
                vec![brew, "update"],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            runner.respond(
                vec![brew, "outdated", "--json=v2"],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: empty_outdated.to_string(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(3600));

        let inst_opt = test_instance();
        let inst_local = ManagerInstance {
            id: "brew:/usr/local".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/usr/local/bin/brew"),
            prefix: PathBuf::from("/usr/local"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
        };

        adapter
            .check_updates(&inst_opt)
            .await
            .expect("check_updates opt/homebrew #1");
        adapter
            .check_updates(&inst_local)
            .await
            .expect("check_updates usr/local #1");

        let update_calls_after_first_round = mock_ref
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls_after_first_round, 2,
            "each instance must run its own `brew update` once, not share one TTL"
        );

        adapter
            .check_updates(&inst_opt)
            .await
            .expect("check_updates opt/homebrew #2");
        adapter
            .check_updates(&inst_local)
            .await
            .expect("check_updates usr/local #2");

        let update_calls_after_second_round = mock_ref
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls_after_second_round, 2,
            "within the TTL window, neither instance should run `brew update` again"
        );
    }

    #[tokio::test]
    async fn test_search_calls_both_search_variants() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "search", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "==> Formulae\njq\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "search", "--desc", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "==> Formulae\njq: Command-line JSON processor\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner);
        let hits = adapter
            .search(&test_instance(), "jq")
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(
            hits[0].description.as_deref(),
            Some("Command-line JSON processor")
        );
        assert_eq!(
            mock_ref.calls(),
            vec![
                vec![
                    "/opt/homebrew/bin/brew".to_string(),
                    "search".to_string(),
                    "jq".to_string()
                ],
                vec![
                    "/opt/homebrew/bin/brew".to_string(),
                    "search".to_string(),
                    "--desc".to_string(),
                    "jq".to_string()
                ],
            ]
        );
    }
}

#[cfg(test)]
mod plan_execute_tests {
    use super::*;
    use crate::events::VecSink;
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
        }
    }

    /// (F4 / M2) `plan` must refuse when the request's `instance_id` does not
    /// match the instance it was actually given — otherwise a caller could
    /// end up with a plan that runs against instance A but reconciles
    /// against instance B's request, silently operating on the wrong
    /// Homebrew installation.
    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone());
        let inst_a = test_instance();
        let inst_b = ManagerInstance {
            id: "brew:/usr/local".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/usr/local/bin/brew"),
            prefix: PathBuf::from("/usr/local"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
        };
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst_b.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };

        let result = adapter.plan(&inst_a, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
        assert!(
            runner.calls().is_empty(),
            "a mismatched instance must be rejected before any brew command runs"
        );
    }

    #[tokio::test]
    async fn test_plan_install_formula() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "--formula", "jq"]);
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("brew:/opt/homebrew".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    #[tokio::test]
    async fn test_plan_install_cask_needs_password() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "--cask", "claudebar"]);
        assert!(plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_uninstall_with_dependents_warns() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "python@3.13\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["uninstall", "--formula", "jq"]);
        assert_eq!(plan.affected, vec!["python@3.13".to_string()]);
        assert_eq!(
            plan.warnings,
            vec!["Removing jq will break: python@3.13".to_string()]
        );
    }

    /// (F6 / M5) When `brew uses --installed {name}` itself fails (non-zero
    /// exit — the same shape a timeout produces, since `CommandOutput` for a
    /// timed-out run also carries `exit_code: None`), the dependents list
    /// must not be silently treated as "confirmed empty": the plan needs a
    /// distinct warning saying the check itself could not be completed, so
    /// the user is not told "nothing depends on this" when the truth is
    /// "we don't know".
    #[tokio::test]
    async fn test_plan_uninstall_warns_when_uses_check_fails() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "error: some transient brew failure".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(plan.affected.is_empty());
        assert!(
            plan.warnings.contains(
                &"could not determine what depends on jq; uninstalling may break other packages"
                    .to_string()
            ),
            "expected a could-not-determine warning, got {:?}",
            plan.warnings
        );
    }

    #[tokio::test]
    async fn test_plan_uninstall_without_dependents_has_no_warning() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["uninstall", "--formula", "jq"]);
        assert!(plan.affected.is_empty());
        assert!(plan.warnings.is_empty());
    }

    /// (F10 / M8) Uninstalling a cask can require a password (e.g. a
    /// `pkgutil`/kernel-extension/launch-daemon removal invoking `sudo`),
    /// unlike a formula uninstall which never does. Before this fix every
    /// uninstall plan hardcoded `needs_password: false` regardless of
    /// artifact kind.
    #[tokio::test]
    async fn test_plan_uninstall_needs_password_matches_artifact_kind() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "docker"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let cask_plan = adapter
            .plan(
                &inst,
                &OpRequest {
                    kind: OpKind::Uninstall,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Cask,
                    name: "docker".to_string(),
                },
            )
            .await
            .expect("cask uninstall plan");
        assert!(
            cask_plan.needs_password,
            "cask uninstalls may need sudo (e.g. pkgutil/kext removal)"
        );

        let formula_plan = adapter
            .plan(
                &inst,
                &OpRequest {
                    kind: OpKind::Uninstall,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                },
            )
            .await
            .expect("formula uninstall plan");
        assert!(!formula_plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_uninstall_cask_passes_cask_flag() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "docker"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "docker".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        // `docker` exists as both a formula and a cask; without `--cask`
        // here `brew uninstall docker` would act on the wrong one.
        assert_eq!(plan.args, vec!["uninstall", "--cask", "docker"]);
    }

    #[tokio::test]
    async fn test_plan_upgrade_formula_passes_formula_flag() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["upgrade", "--formula", "jq"]);
        assert!(!plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_upgrade_cask_passes_cask_flag() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "docker".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        // Same ambiguous-name hazard as uninstall: `docker` is both a
        // formula and a cask.
        assert_eq!(plan.args, vec!["upgrade", "--cask", "docker"]);
        assert!(plan.needs_password);
    }

    /// (F5 / M7) `execute` builds its `CommandSpec` and calls the runner
    /// directly rather than going through `run_brew`, so it needs its own
    /// euid gate rather than inheriting one that only lives in `run_brew`.
    #[tokio::test]
    async fn test_execute_refuses_as_root() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner.clone()).with_euid_fn(|| 0);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        // `plan` itself does not run brew, so it is unaffected by the euid
        // gate; build the plan with a non-root adapter and only inject root
        // for the `execute` call under test.
        let plan_adapter = BrewAdapter::new(Arc::new(MockRunner::new()));
        let plan = plan_adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let result = adapter
            .execute(&plan, sink, 1, CancellationToken::new())
            .await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_eq!(
            runner.calls().len(),
            0,
            "no brew process should ever be started as root"
        );
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installing jq\nDone\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 2);
    }

    #[tokio::test]
    async fn test_execute_failure_summary_is_last_five_stderr_lines() {
        let runner = Arc::new(MockRunner::new());
        let stderr = (1..=8)
            .map(|n| format!("line{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr,
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink, 1, CancellationToken::new())
            .await
            .expect("execute");
        match outcome {
            Outcome::Failed { exit_code, summary } => {
                assert_eq!(exit_code, Some(1));
                assert_eq!(summary, "line4\nline5\nline6\nline7\nline8");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{"formulae":[{"name":"jq","desc":null,"homepage":null,"linked_keg":"1.7.1","installed":[{"version":"1.7.1","installed_on_request":true,"installed_as_dependency":false,"time":null}]}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let present = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                },
            )
            .await
            .expect("reconcile present");
        assert_eq!(
            present,
            Reconciled {
                present: true,
                version: Some("1.7.1".to_string())
            }
        );

        let absent = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Formula,
                    name: "missing".to_string(),
                },
            )
            .await
            .expect("reconcile absent");
        assert_eq!(
            absent,
            Reconciled {
                present: false,
                version: None
            }
        );
    }

    /// (F3) A cask installed from a third-party tap — the real, committed
    /// fixture `adapters/fixtures/brew/7.0.3/info-installed.json` has exactly
    /// this shape for `claudebar` (`"token": "claudebar", "full_token":
    /// "gautham-v/tap/claudebar"`). Inlining the relevant fields here as a
    /// literal is the "inline JSON string unit test" exception the fix brief
    /// calls out — it does not require hand-writing a *fixture file*.
    fn tap_cask_json() -> &'static str {
        r#"{
            "formulae": [],
            "casks": [
                {
                    "token": "claudebar",
                    "full_token": "gautham-v/tap/claudebar",
                    "name": ["Claudebar"],
                    "desc": "Claude usage limits in the menu bar",
                    "homepage": "https://github.com/gautham-v/claudebar",
                    "installed": "0.1.1",
                    "auto_updates": null
                }
            ]
        }"#
    }

    #[tokio::test]
    async fn test_inventory_uses_full_token_as_key_name_for_tapped_cask() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: tap_cask_json().to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "gautham-v/tap/claudebar");
        assert_eq!(artifacts[0].display_name, "Claudebar");
    }

    #[tokio::test]
    async fn test_reconcile_matches_tapped_cask_by_full_name_and_short_name() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: tap_cask_json().to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let by_full_name = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Cask,
                    name: "gautham-v/tap/claudebar".to_string(),
                },
            )
            .await
            .expect("reconcile by full name");
        assert!(
            by_full_name.present,
            "must match on the full tap-qualified name"
        );

        let by_short_name = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Cask,
                    name: "claudebar".to_string(),
                },
            )
            .await
            .expect("reconcile by short name");
        assert!(
            by_short_name.present,
            "must also match when the request used only the short token"
        );

        let missing = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Cask,
                    name: "does-not-exist".to_string(),
                },
            )
            .await
            .expect("reconcile of a name that does not exist");
        assert!(!missing.present);
    }

    #[tokio::test]
    async fn test_plan_passes_through_sudo_askpass_for_cask_install() {
        std::env::set_var("SUDO_ASKPASS", "/tmp/fake-askpass.sh");
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        std::env::remove_var("SUDO_ASKPASS");
        assert!(plan.env.contains(&(
            "SUDO_ASKPASS".to_string(),
            "/tmp/fake-askpass.sh".to_string()
        )));
    }
}
