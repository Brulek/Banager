use crate::adapters::{
    run_plan, validate_package_name, Adapter, AdapterError, AdapterMeta, CheckOptions,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, ReadOnlyReason, Reconciled, ResourceLock, Scope, SearchHit,
    UpdateCandidate, UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// A search box takes free text, not a package name: `validate_package_name`
/// matches `^[A-Za-z0-9@._+/-]+$`, so it rejects any multi-word query
/// ("json parser") as an *invalid name*, which is both wrong and confusing.
/// That function exists to keep a path out of an argv; this one exists to
/// keep a query out of argv's flag namespace and to bound its length. npm
/// itself decides what matches.
fn validate_search_query(query: &str) -> Result<(), AdapterError> {
    let trimmed = query.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.len() > 200 {
        return Err(AdapterError::InvalidName(query.to_string()));
    }
    Ok(())
}

/// The real check for `NpmAdapter::new`'s `prefix_writable_fn` default:
/// whether the current user can write where npm actually places global
/// packages under `prefix` (the prefix *root* that `npm prefix -g` reports,
/// e.g. `/opt/homebrew` — **not** the `node_modules` directory itself, despite
/// what an earlier version of this comment claimed). npm writes into
/// `{prefix}/lib/node_modules`.
///
/// When that directory doesn't exist yet (e.g. the first global install on
/// this prefix), npm has to create it, which needs write permission on the
/// nearest *existing* ancestor, not on the missing leaf. So this walks
/// `{prefix}/lib/node_modules` → `{prefix}/lib` → `{prefix}` and tests
/// whichever of those exists first. A prefix owned by another user (e.g. a
/// system-wide npm) is read-only for this adapter — see the per-adapter
/// contract table's Notes column.
fn real_prefix_is_writable(prefix: &Path) -> bool {
    let node_modules = prefix.join("lib").join("node_modules");
    let lib_dir = prefix.join("lib");
    for candidate in [node_modules.as_path(), lib_dir.as_path(), prefix] {
        if candidate.exists() {
            return path_is_writable(candidate);
        }
    }
    // None of the three exist (npm prefix -g pointed at a prefix that isn't
    // on disk at all) — there is nothing nearer to test than the root itself,
    // and `access` on a missing path correctly reports "not writable".
    path_is_writable(prefix)
}

fn path_is_writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(c_path) => unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 },
        Err(_) => false,
    }
}

pub struct NpmAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// How to decide whether `inst.prefix` is writable by the current user
    /// (in practice: whether npm can write into `{prefix}/lib/node_modules`,
    /// creating it if needed — see `real_prefix_is_writable`), gating
    /// install/uninstall/upgrade. Production always gets
    /// `real_prefix_is_writable`; tests inject a fixed answer via the
    /// `#[cfg(test)]`-only `with_prefix_writable_fn`, mirroring
    /// `BrewAdapter::with_euid_fn`. `detect()` asks the same question to
    /// fill `read_only_reason`, which is why this takes `&Path` (the
    /// prefix) rather than a `&ManagerInstance` that does not exist yet.
    prefix_writable_fn: fn(&Path) -> bool,
}

impl NpmAdapter {
    pub const ENV: [(&'static str, &'static str); 3] = [
        ("NO_COLOR", "1"),
        ("npm_config_update_notifier", "false"),
        ("npm_config_fund", "false"),
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> NpmAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/npm.toml"))
            .expect("adapters/meta/npm.toml must parse");
        NpmAdapter {
            runner,
            meta,
            prefix_writable_fn: real_prefix_is_writable,
        }
    }

    #[cfg(test)]
    fn with_prefix_writable_fn(mut self, f: fn(&Path) -> bool) -> NpmAdapter {
        self.prefix_writable_fn = f;
        self
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn instance_id_for(prefix: &Path) -> String {
        format!("npm:{}", prefix.display())
    }

    async fn run_npm(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
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

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("npm", env) else {
            return Vec::new();
        };
        let prefix_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["prefix".to_string(), "-g".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
        };
        let prefix_output = self
            .runner
            .run(prefix_spec, None, CancellationToken::new())
            .await;
        let prefix = match &prefix_output {
            Ok(o) if o.exit_code == Some(0) => PathBuf::from(o.stdout.trim()),
            _ => return Vec::new(),
        };
        let version_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["--version".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
        };
        let version_output = self
            .runner
            .run(version_spec, None, CancellationToken::new())
            .await;
        let version = match version_output {
            Ok(o) if o.exit_code == Some(0) => Some(o.stdout.trim().to_string()),
            _ => None,
        };
        let unverified_version = self.meta.unverified_version(&version);
        // Asked once, here, about the prefix npm itself reported -- not
        // about wherever the `npm` binary happens to live. A Node
        // installed from nodejs.org's package leaves a root-owned prefix
        // this user cannot write, and the two pages need to say so
        // *before* the user clicks anything. `plan()` asks again at click
        // time: permissions can change in between, so that gate stays.
        let read_only_reason = if (self.prefix_writable_fn)(&prefix) {
            None
        } else {
            Some(ReadOnlyReason::PrefixNotWritable)
        };
        vec![ManagerInstance {
            id: Self::instance_id_for(&prefix),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            healthy: version.is_some(),
            version,
            unverified_version,
            read_only_reason,
        }]
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_npm(
                inst,
                vec![
                    "ls".to_string(),
                    "-g".to_string(),
                    "--depth=0".to_string(),
                    "--json".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        // `npm ls -g --depth=0 --json` exits 1 for various non-fatal
        // reasons (e.g. peer dependency mismatches); accept 0 or 1 and
        // always parse stdout — see the per-adapter contract table.
        if output.exit_code != Some(0) && output.exit_code != Some(1) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_ls_global(&output.stdout, &inst.id)
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let output = self
            .run_npm(
                inst,
                vec![
                    "outdated".to_string(),
                    "-g".to_string(),
                    "--json".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        // npm exits 1 whenever it finds anything outdated — a result, not a
        // failure. See the per-adapter contract table.
        if output.exit_code != Some(0) && output.exit_code != Some(1) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_outdated_global(&output.stdout, &inst.id)
    }

    pub async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        validate_search_query(query)?;
        let output = self
            .run_npm(
                inst,
                vec![
                    "search".to_string(),
                    "--json".to_string(),
                    "--searchlimit".to_string(),
                    "20".to_string(),
                    query.to_string(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_search(&output.stdout, &self.meta.id)
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
        if !(self.prefix_writable_fn)(&inst.prefix) {
            return Err(AdapterError::Refused(format!(
                "{} is not writable; this npm install is read-only for the current user",
                inst.prefix.display()
            )));
        }
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["install".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["uninstall".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Upgrade => vec![
                "install".to_string(),
                "-g".to_string(),
                format!("{}@latest", req.name),
            ],
        };
        Ok(Plan {
            request: req.clone(),
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
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
        match artifacts.into_iter().find(|a| a.key.name == key.name) {
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
impl Adapter for NpmAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        NpmAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        NpmAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        NpmAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        NpmAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        NpmAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        NpmAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        NpmAdapter::reconcile(self, inst, key).await
    }
}

#[derive(Debug, Deserialize)]
struct LsGlobalRoot {
    #[serde(default)]
    dependencies: HashMap<String, LsGlobalDependency>,
}

#[derive(Debug, Deserialize)]
struct LsGlobalDependency {
    #[serde(default)]
    version: Option<String>,
}

/// Parses `npm ls -g --depth=0 --json`. The real, committed fixture
/// (`adapters/fixtures/npm/12.0.2/ls-global.json`) shows the top level is a
/// `dependencies` **object** keyed by package name, not an array — a parser
/// expecting an array silently sees zero packages instead of erroring.
/// Sorted by name for deterministic output (a `HashMap`'s own iteration
/// order is not).
fn parse_ls_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, crate::adapters::AdapterError> {
    let root: LsGlobalRoot = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<InstalledArtifact> = root
        .dependencies
        .into_iter()
        .map(|(name, dep)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            display_name: name,
            version: dep.version.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct OutdatedEntry {
    current: String,
    latest: String,
}

/// Parses `npm outdated -g --json`. npm exits 1 whenever it finds anything
/// outdated — the caller must still treat that stdout as the real result,
/// not an error (see the per-adapter contract table). Empty stdout (no
/// output at all, not even `{}`) means nothing is outdated.
fn parse_outdated_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, crate::adapters::AdapterError> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: HashMap<String, OutdatedEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<UpdateCandidate> = root
        .into_iter()
        .map(|(name, entry)| UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            current: entry.current,
            target: entry.latest,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct SearchEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// Parses `npm search --json --searchlimit 20 {query}`.
fn parse_search(
    json: &str,
    adapter_id: &str,
) -> Result<Vec<SearchHit>, crate::adapters::AdapterError> {
    let entries: Vec<SearchEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|e| SearchHit {
            adapter_id: adapter_id.to_string(),
            kind: ArtifactKind::Package,
            name: e.name,
            description: e.description,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ls_global_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        let artifacts = parse_ls_global(&json, "npm:/opt/homebrew/lib").expect("parse");
        assert_eq!(artifacts.len(), 6);
        let names: Vec<&str> = artifacts.iter().map(|a| a.key.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "@alisaitteke/photoshop-mcp",
                "@openai/codex",
                "corepack",
                "get-shit-done-cc",
                "npm",
                "zsxq-cli",
            ]
        );
        let npm_self = artifacts
            .iter()
            .find(|a| a.key.name == "npm")
            .expect("npm entry");
        assert_eq!(npm_self.version, "12.0.2");
        assert_eq!(npm_self.key.kind, ArtifactKind::Package);
    }

    #[test]
    fn parse_outdated_global_matches_the_recorded_fixture() {
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read adapters/fixtures/npm/12.0.2/outdated-global.json");
        let candidates = parse_outdated_global(&json, "npm:/opt/homebrew/lib").expect("parse");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "@alisaitteke/photoshop-mcp");
        assert_eq!(candidates[0].current, "1.7.15");
        assert_eq!(candidates[0].target, "1.7.17");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[test]
    fn parse_outdated_global_of_empty_stdout_is_no_updates() {
        // With nothing outdated npm prints either nothing at all or `{}`,
        // depending on version; both mean "no updates". Neither is committed
        // as a fixture, since there is nothing to record — but the parser
        // must not choke on either.
        let candidates = parse_outdated_global("", "npm:/opt/homebrew/lib").expect("parse");
        assert!(candidates.is_empty());
        assert!(parse_outdated_global("{}", "npm:/opt/homebrew/lib")
            .expect("parse")
            .is_empty());
    }

    #[test]
    fn parse_search_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read adapters/fixtures/npm/12.0.2/search-jq.json");
        let hits = parse_search(&json, "npm").expect("parse");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(
            hits[0].description.as_deref(),
            Some("Server-side jQuery wrapper for node.")
        );
        assert!(hits.iter().all(|h| h.adapter_id == "npm"));
    }

    use crate::adapters::{AdapterError, CheckOptions};
    use crate::events::VecSink;
    use crate::model::{
        ArtifactKey, CancelPolicy, OpKind, OpRequest, Outcome, ResourceLock, Scope,
    };
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "npm:/opt/homebrew/lib".to_string(),
            adapter_id: "npm".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/npm"),
            prefix: PathBuf::from("/opt/homebrew/lib"),
            scope: Scope::User,
            version: Some("12.0.2".to_string()),
            healthy: true,
            unverified_version: None,
            read_only_reason: None,
        }
    }

    fn fake_exe(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").expect("write fake npm executable");
        path
    }

    #[tokio::test]
    async fn test_detect_finds_npm_on_path_and_resolves_its_global_prefix() {
        let dir = std::env::temp_dir().join(format!(
            "canager-npm-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "12.0.2\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
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
        assert_eq!(instances[0].id, "npm:/opt/homebrew");
        assert_eq!(instances[0].version, Some("12.0.2".to_string()));
        assert!(instances[0].healthy);
        assert!(
            instances[0].unverified_version.is_none(),
            "12.0.2 is verified in adapters/meta/npm.toml"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version() {
        let dir = std::env::temp_dir().join(format!(
            "canager-npm-detect-unverified-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
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
        assert_eq!(instances[0].unverified_version, Some("99.9.9".to_string()));
    }

    /// A temp directory holding a fake `npm` executable, plus a `HostEnv`
    /// whose `PATH` is exactly that directory. Returned rather than
    /// inlined because every detect test needs the same three lines and
    /// the unique-name dance around them.
    fn detect_fixture(tag: &str) -> (PathBuf, PathBuf, HostEnv) {
        let dir = std::env::temp_dir().join(format!(
            "canager-npm-detect-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        (dir, npm_path, env)
    }

    fn detect_runner(npm_path: &std::path::Path) -> Arc<MockRunner> {
        let npm_path_str = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "12.0.2\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    #[tokio::test]
    async fn test_detect_reports_prefix_not_writable_when_npm_cannot_write_where_it_installs() {
        // The nodejs.org installer's npm: the CLI works, `npm prefix -g`
        // answers, but the directory it would write into belongs to root.
        // The instance must still be detected -- Canager lists what is
        // there -- and must carry the reason it cannot be changed, so the
        // Updates page can say "install Node with Homebrew instead"
        // rather than pip's "use pipx or uv".
        let (dir, npm_path, env) = detect_fixture("readonly");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_writable_fn(|_| false);
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::PrefixNotWritable)
        );
        assert!(!instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_leaves_the_instance_writable_when_the_prefix_is_writable() {
        let (dir, npm_path, env) = detect_fixture("writable");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_writable_fn(|_| true);
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].read_only_reason, None);
        assert!(instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_asks_about_the_prefix_npm_reported_not_the_exe_directory() {
        // The writability question is about `npm prefix -g`'s answer
        // (`/opt/homebrew`), not about wherever the `npm` binary happens to
        // live -- a Homebrew npm's exe sits in `{prefix}/bin`, but a
        // volta/nvm shim's does not.
        use std::sync::Mutex;
        static ASKED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        ASKED.lock().unwrap().clear();
        fn record(path: &Path) -> bool {
            ASKED.lock().unwrap().push(path.to_path_buf());
            true
        }
        let (dir, npm_path, env) = detect_fixture("prefix-arg");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_writable_fn(record);
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            *ASKED.lock().unwrap(),
            vec![PathBuf::from("/opt/homebrew")],
            "detect must ask about the reported prefix exactly once"
        );
    }

    #[tokio::test]
    async fn test_detect_returns_empty_when_npm_is_not_on_path() {
        let runner = Arc::new(MockRunner::new());
        let adapter = NpmAdapter::new(runner.clone());
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/definitely/not/a/real/path")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        assert!(instances.is_empty());
        assert!(
            runner.calls().is_empty(),
            "no subprocess should run when npm isn't found"
        );
    }

    #[tokio::test]
    async fn test_inventory_accepts_exit_code_1() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "ls", "-g", "--depth=0", "--json"],
            CommandOutput {
                exit_code: Some(1),
                stdout: json,
                stderr: "npm warn config global".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("exit 1 must still be parsed");
        assert_eq!(artifacts.len(), 6);
    }

    #[tokio::test]
    async fn test_check_updates_accepts_exit_code_1() {
        let runner = Arc::new(MockRunner::new());
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read fixture");
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "outdated", "-g", "--json"],
            CommandOutput {
                exit_code: Some(1),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("exit 1 means updates were found, not a failure");
        assert_eq!(candidates.len(), 1);
    }

    #[tokio::test]
    async fn test_search_matches_the_recorded_fixture_end_to_end() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "search",
                "--json",
                "--searchlimit",
                "20",
                "jq",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let hits = adapter
            .search(&test_instance(), "jq")
            .await
            .expect("search");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
    }

    #[tokio::test]
    async fn test_plan_install() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "-g", "jq"]);
        assert!(!plan.needs_password);
        assert_eq!(plan.locks, vec![ResourceLock(inst.id.clone())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    #[tokio::test]
    async fn test_plan_uninstall() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["uninstall", "-g", "jq"]);
    }

    #[tokio::test]
    async fn test_plan_upgrade_targets_latest() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "-g", "jq@latest"]);
    }

    #[tokio::test]
    async fn test_plan_is_refused_when_the_prefix_is_not_writable() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| false);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let result = adapter.plan(&inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "install", "-g", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "added 1 package\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "ls", "-g", "--depth=0", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let inst = test_instance();
        let present = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "npm".to_string(),
                },
            )
            .await
            .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("12.0.2".to_string()));

        let absent = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "does-not-exist".to_string(),
                },
            )
            .await
            .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[test]
    fn test_validate_search_query_rejects_a_flag_and_accepts_free_text() {
        // A search box takes free text, so a multi-word query must pass
        // where `validate_package_name` would reject it as an invalid name.
        assert!(validate_search_query("json parser").is_ok());
        assert!(validate_search_query("jq").is_ok());
        // Anything that would land in argv's flag namespace, or that is not
        // a query at all, is refused before it reaches npm.
        assert!(validate_search_query("--registry=http://evil.invalid").is_err());
        assert!(validate_search_query("   ").is_err());
        assert!(validate_search_query(&"x".repeat(201)).is_err());
    }

    fn scratch_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "canager-npm-prefix-writable-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// `set_mode` only strips permission bits; it never has to widen them
    /// back for cleanup here because an empty, no-write directory can still
    /// be unlinked by its (writable) parent.
    fn make_read_only(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dir).expect("stat").permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(dir, perms).expect("chmod read-only");
    }

    #[test]
    fn real_prefix_is_writable_tests_node_modules_itself_not_just_the_prefix_root() {
        // `{prefix}/lib/node_modules` exists but is not writable, even though
        // the prefix root (which we just created and own) is. npm writes
        // into node_modules directly here, so this must report false — a
        // regression to "test prefix root only" would wrongly report true.
        let prefix = scratch_dir("existing-node-modules-read-only");
        let node_modules = prefix.join("lib").join("node_modules");
        std::fs::create_dir_all(&node_modules).expect("create node_modules");
        make_read_only(&node_modules);

        assert!(
            !real_prefix_is_writable(&prefix),
            "node_modules itself is read-only, so npm cannot write packages into it"
        );

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_is_writable_walks_up_to_lib_when_node_modules_does_not_exist_yet() {
        // First global install on this prefix: node_modules hasn't been
        // created yet, but its parent (`lib`) exists and is writable, so npm
        // can create node_modules when it needs to.
        let prefix = scratch_dir("missing-node-modules-writable-lib");
        let lib_dir = prefix.join("lib");
        std::fs::create_dir_all(&lib_dir).expect("create lib");

        assert!(real_prefix_is_writable(&prefix));

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_is_writable_walks_up_to_lib_and_finds_it_unwritable() {
        // Same as above, but `lib` itself cannot be written to, so npm could
        // not create node_modules inside it even though the prefix root can
        // be written to.
        let prefix = scratch_dir("missing-node-modules-read-only-lib");
        let lib_dir = prefix.join("lib");
        std::fs::create_dir_all(&lib_dir).expect("create lib");
        make_read_only(&lib_dir);

        assert!(!real_prefix_is_writable(&prefix));

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_is_writable_falls_back_to_the_prefix_root_when_lib_is_also_missing() {
        // Neither `lib` nor `lib/node_modules` exist yet; the nearest
        // existing ancestor is the prefix root itself.
        let prefix = scratch_dir("missing-lib-entirely");

        assert!(real_prefix_is_writable(&prefix));

        let _ = std::fs::remove_dir_all(&prefix);
    }
}
