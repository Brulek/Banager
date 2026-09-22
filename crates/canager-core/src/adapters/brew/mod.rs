pub mod parse;

use crate::adapters::{
    ensure_instance_match, reconcile_from, validate_package_name, Adapter, AdapterError,
    AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, InstanceId, InstanceNote,
    InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock,
    Scope, SearchHit, Unavailable, Warning,
};
use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, OutputUse};
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
    /// Serialises `maybe_update` per instance: without this, two
    /// concurrent `check_updates` calls for the same instance could both
    /// observe "TTL expired" before either had recorded a fresh
    /// timestamp, and both run `brew update` concurrently — wasteful, and
    /// two `brew update` processes writing the same Homebrew cache
    /// directory at once is not something Homebrew is designed to
    /// tolerate. Keyed the same way as `last_update`.
    update_locks: Mutex<HashMap<InstanceId, Arc<tokio::sync::Mutex<()>>>>,
    update_ttl: Duration,
    /// How to read the *real* effective UID for the root-refusal check on
    /// every brew subprocess call (not just `detect`, which instead checks
    /// the caller-supplied `HostEnv::euid`). A plain fn pointer (rather than
    /// a boxed closure) keeps this injectable for tests without touching
    /// `BrewAdapter::new`'s public signature: production code always gets
    /// the default `|| unsafe { libc::geteuid() }`, and tests can swap in
    /// `|| 0` via the `#[cfg(test)]`-only `with_euid_fn`.
    euid_fn: fn() -> u32,
    /// How to read `SUDO_ASKPASS` for the cask install/upgrade
    /// passthrough. Same fn-pointer trick as `euid_fn`, and for the same
    /// reason: the value lives in the process environment, and a test that
    /// wants a known one used to `set_var` it. Rust runs tests in threads
    /// within one binary, so that was a write to process-global state
    /// racing every other test in the binary that reads it — including
    /// every other cask plan, which reads exactly this variable. Injecting
    /// the reader instead means no test has to touch the environment at
    /// all. Production still reads the real variable, and still reads it
    /// per plan rather than once at startup.
    askpass_fn: fn() -> Option<String>,
    /// How to ask whether one of `CANDIDATE_PATHS` is on this machine.
    /// Same fn-pointer seam as `euid_fn` and `askpass_fn`, and as
    /// `NpmAdapter::prefix_writable_fn`, for the same reason: `detect`'s
    /// whole job is probing the filesystem, so a test that cannot answer
    /// that probe can only assert whatever the machine running it happens
    /// to have. That made every `detect` test here a test of the author's
    /// Mac -- green on Apple Silicon with Homebrew in `/opt/homebrew`, red
    /// on an Intel Mac (`/usr/local`), red on a checkout with no Homebrew
    /// at all, and red again where both prefixes exist. Production always
    /// gets `|path| path.exists()`; tests hand in a layout.
    path_exists_fn: fn(&Path) -> bool,
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
            update_locks: Mutex::new(HashMap::new()),
            update_ttl: Duration::from_secs(6 * 3600),
            euid_fn: || unsafe { libc::geteuid() },
            askpass_fn: || std::env::var("SUDO_ASKPASS").ok(),
            path_exists_fn: |path| path.exists(),
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

    /// Test-only hook to pin what `SUDO_ASKPASS` reads as, so no test has
    /// to mutate the process environment other tests are reading from.
    #[cfg(test)]
    fn with_askpass_fn(mut self, askpass_fn: fn() -> Option<String>) -> BrewAdapter {
        self.askpass_fn = askpass_fn;
        self
    }

    /// Test-only hook to describe the Homebrew layout `detect` should see:
    /// which of `CANDIDATE_PATHS` exist. `pub(crate)` rather than private
    /// because `session::refresh`'s own tests build a real `BrewAdapter`
    /// (deliberately -- what they exercise is brew's own root policy) and
    /// so need to pin the layout too.
    #[cfg(test)]
    pub(crate) fn with_path_exists_fn(mut self, path_exists_fn: fn(&Path) -> bool) -> BrewAdapter {
        self.path_exists_fn = path_exists_fn;
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
            // `inventory`, `check_updates`, `plan`'s dependent scan and
            // `search` all hand this stdout to a parser, so it must
            // arrive whole. `brew update` is the one caller that reads
            // only the exit code, and refusing an implausible 64 MiB of
            // it costs that caller nothing.
            output_use: OutputUse::Parsed,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    fn update_lock_for(&self, inst_id: &InstanceId) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.update_locks.lock().unwrap();
        locks
            .entry(inst_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    async fn maybe_update(&self, inst: &ManagerInstance) -> Result<(), AdapterError> {
        let lock = self.update_lock_for(&inst.id);
        let _guard = lock.lock().await;
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

    /// True when `env`'s effective UID means every brew invocation this
    /// adapter would make will refuse to run.
    ///
    /// The rule lives in one named place, rather than as `env.euid == 0`
    /// inline, because two things in `detect` below turn on it: whether to
    /// run `brew --version` at all, and which `Unavailable` the detected
    /// instance carries. It is deliberately *not* public: it used to be,
    /// for a `Session::refresh` call site that no longer exists -- the root
    /// objection is Homebrew's alone and disabling the other six sources
    /// over it was the bug that call site caused.
    fn refuses_as_root(env: &HostEnv) -> bool {
        env.euid == 0
    }

    /// Every Homebrew on this Mac, each with the state it is in.
    ///
    /// Under root this still reports the installs it finds, marked
    /// `Unavailable::RefusesAsRoot`, and runs no brew process at all. An
    /// empty `Vec` would have been read as "Homebrew is not installed" --
    /// by `Session::refresh`'s `DetectOutcome` and, through it, by the
    /// user, who would be told to go and install the Homebrew they already
    /// have instead of to reopen Canager without `sudo`.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let as_root = Self::refuses_as_root(env);
        let mut found = Vec::new();
        for candidate in Self::CANDIDATE_PATHS {
            let path = PathBuf::from(candidate);
            if !(self.path_exists_fn)(&path) {
                continue;
            }
            let version = if as_root {
                None
            } else {
                let spec = CommandSpec {
                    program: path.clone(),
                    args: vec!["--version".to_string()],
                    env: self.env_vec(),
                    cwd: None,
                    timeout: Duration::from_secs(30),
                    output_use: OutputUse::Parsed,
                };
                let output = self.runner.run(spec, None, CancellationToken::new()).await;
                match output {
                    Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
                    _ => None,
                }
            };
            let unverified_version = self.meta.unverified_version(&version);
            let prefix = Self::prefix_for(&path);
            found.push(ManagerInstance {
                id: Self::instance_id_for(&prefix),
                adapter_id: self.meta.id.clone(),
                exe_path: path,
                prefix,
                scope: Scope::User,
                status: InstanceStatus {
                    // The state axis. Under root nothing was asked, so the
                    // reason is the root run itself -- not `NotResponding`,
                    // which would send the user off reinstalling a Homebrew
                    // that is working perfectly well. Otherwise `version`
                    // is `None` exactly when the CLI is on PATH but
                    // `--version` would not run or could not be parsed: the
                    // tool is there, it just did not answer.
                    unavailable: if as_root {
                        Some(Unavailable::RefusesAsRoot)
                    } else {
                        version.is_none().then_some(Unavailable::NotResponding)
                    },
                    notes: Vec::new(),
                },
                version,
                unverified_version,
                read_only_reason: None,
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
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        // A failed `brew update` is a fact about this Homebrew, not about
        // any package it lists: the catalogue everything below is compared
        // against may be behind, which makes "no updates" as suspect as
        // any version number here. So it rides back on the *source*, as a
        // note, and not on the candidates.
        //
        // It used to be a sentence pushed onto every candidate's
        // `warnings`. Two things were wrong with that. `UpdatesPage`
        // renders no warning text on a checkable row -- only a "1 warning"
        // badge -- so the sentence was unreadable; and a Homebrew with
        // nothing outdated produces no candidates at all, so in the one
        // case where the caveat decides whether the page is lying, there
        // was nothing to attach it to.
        //
        // The stderr detail that sentence carried is gone, deliberately:
        // `InstanceNote` is payload-free so the hand-written TypeScript
        // mirror keeps seeing a bare string on the wire (spec §2.3).
        let notes = match self.maybe_update(inst).await {
            Ok(()) => Vec::new(),
            Err(_) => vec![InstanceNote::IndexMayBeStale],
        };
        let mut args = vec!["outdated".to_string(), "--json=v2".to_string()];
        if opts.include_self_updating {
            args.push("--greedy".to_string());
        }
        let output = self.run_brew(inst, args, Duration::from_secs(120)).await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        let mut candidates = parse_outdated(&output.stdout, &inst.id)?;
        // brew's two readers spell the same package differently.
        // `parse_info_installed` keys by `full_name`/`full_token`
        // (`gautham-v/tap/claudebar`); `brew outdated --json=v2` never
        // emits `full_name`, so `parse_outdated` falls back to the bare
        // `claudebar`. Both are faithful readings of what brew printed --
        // which is why the resolution belongs here and not in either
        // parser -- but `ArtifactKey` is what everything downstream joins
        // on, and while the two disagree every tapped formula and cask
        // loses its "update available" badge on the Installed page, its
        // description on the Updates page, and its place in
        // `Session::refresh`'s check that a carried-forward candidate is
        // still installed.
        //
        // The cost is one extra `brew info --installed --json=v2` per
        // check, the same local command `inventory` runs; it buys keys
        // that mean one thing across this adapter. A failed inventory is
        // not a failed check: `qualified_key` returns the key it was
        // given when nothing matches, so the worst case is the short
        // spelling that shipped before.
        let installed = self.inventory(inst).await.unwrap_or_default();
        if !installed.is_empty() {
            for candidate in &mut candidates {
                candidate.key = qualified_key(&installed, &candidate.key);
            }
        }
        Ok(CheckOutcome { candidates, notes })
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

/// The name `inventory()` lists an artifact under, given whatever spelling
/// the caller had.
///
/// Homebrew is the one source where two names address the same artifact: a
/// tapped cask is installed as `gautham-v/tap/claudebar` and is just as
/// legitimately called `claudebar`. Returns the caller's own key unchanged
/// when nothing matches, so a genuinely absent artifact stays absent.
///
/// This is the part of brew's reconcile that is *not* shared, and it is
/// deliberately kept here rather than pushed into `reconcile_from` as an
/// option: the last-segment rule would be wrong for npm, whose scoped
/// package names contain `/` (`@types/node` must never be found by
/// `node`).
fn qualified_key(artifacts: &[InstalledArtifact], key: &ArtifactKey) -> ArtifactKey {
    artifacts
        .iter()
        .find(|a| {
            a.key.kind == key.kind
                && (a.key.name == key.name
                    || a.key.name.rsplit('/').next() == Some(key.name.as_str()))
        })
        .map(|a| a.key.clone())
        .unwrap_or_else(|| key.clone())
}

impl BrewAdapter {
    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
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
                if let Some(askpass) = (self.askpass_fn)() {
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
                    warnings.push(Warning::DependentsUnknown);
                    Vec::new()
                };
                if !affected.is_empty() {
                    warnings.push(Warning::WouldBreak {
                        names: affected.clone(),
                    });
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
                if let Some(askpass) = (self.askpass_fn)() {
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
            // Same as `adapters::run_plan`: a transcript for the log
            // drawer, never parsed.
            output_use: OutputUse::Transcript,
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
        // built before full names existed). Resolve the short spelling to
        // the name inventory actually uses first; the presence rule itself
        // is the shared one every adapter applies.
        let key = qualified_key(&artifacts, key);
        Ok(reconcile_from(artifacts, &key))
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
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        BrewAdapter::check_updates(self, inst, opts).await
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
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
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

    /// The four Homebrew layouts a contributor's Mac can be in, written as
    /// answers to the one filesystem question `detect` asks. Every detect
    /// test below picks one, so what it proves is the same everywhere the
    /// suite runs -- rather than whatever the machine running it happens
    /// to have installed.
    fn apple_silicon_layout(path: &Path) -> bool {
        path == Path::new("/opt/homebrew/bin/brew")
    }

    fn intel_layout(path: &Path) -> bool {
        path == Path::new("/usr/local/bin/brew")
    }

    fn both_prefixes_layout(path: &Path) -> bool {
        apple_silicon_layout(path) || intel_layout(path)
    }

    fn no_homebrew_layout(_path: &Path) -> bool {
        false
    }

    /// A `HostEnv` for the detect tests. Brew's `detect` reads only `euid`
    /// from it -- the candidate paths are absolute, not searched on `PATH`
    /// -- so the rest is filler.
    fn detect_env(euid: u32) -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: if euid == 0 {
                PathBuf::from("/var/root")
            } else {
                PathBuf::from("/tmp")
            },
            euid,
            cargo_home: None,
            ollama_host: None,
        }
    }

    /// A runner that answers `--version` for one candidate path.
    fn version_runner(brew_path: &str, version_line: &str) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![brew_path, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: version_line.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    #[tokio::test]
    async fn test_detect_under_root_reports_the_instance_as_refusing_rather_than_missing() {
        // Returning no instance at all here is indistinguishable from
        // "Homebrew is not installed", and the front end says exactly that:
        // `SnapshotStatus` falls through to "None of them are set up on
        // this Mac yet". Homebrew *is* set up; the one thing the user has
        // to be told -- quit and reopen without `sudo` -- is the one thing
        // an empty `Vec` cannot say. So the instance is detected and marked
        // unavailable with a reason, which `sourceNoticesFor` turns into
        // that sentence and `Session::issue_plan` turns into a refusal.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_path_exists_fn(apple_silicon_layout);
        let env = detect_env(0);
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].id, "brew:/opt/homebrew");
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::RefusesAsRoot),
            "a root run is its own reason, not the generic NotResponding"
        );
        assert!(
            instances[0].writable(),
            "nothing about root makes the prefix read-only; the state axis carries this"
        );
        assert_eq!(
            runner.calls().len(),
            0,
            "no brew process should ever be started as root, `--version` included"
        );
    }

    #[test]
    fn test_refuses_as_root_is_true_only_for_euid_zero() {
        let root = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
            cargo_home: None,
            ollama_host: None,
        };
        let user = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert!(BrewAdapter::refuses_as_root(&root));
        assert!(!BrewAdapter::refuses_as_root(&user));
    }

    #[tokio::test]
    async fn test_detect_finds_the_apple_silicon_prefix() {
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 7.0.3\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].id, "brew:/opt/homebrew");
        assert_eq!(
            instances[0].exe_path,
            PathBuf::from("/opt/homebrew/bin/brew")
        );
        assert_eq!(instances[0].prefix, PathBuf::from("/opt/homebrew"));
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert!(instances[0].available());
    }

    #[tokio::test]
    async fn test_detect_finds_the_intel_prefix() {
        // The same Homebrew, installed where an Intel Mac puts it. This
        // used to be untestable without owning an Intel Mac, which meant
        // the `/usr/local` half of `CANDIDATE_PATHS` -- and the
        // `prefix_for` derivation that turns the exe path back into a
        // prefix -- was never exercised at all.
        let runner = version_runner("/usr/local/bin/brew", "Homebrew 7.0.3\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(intel_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].id, "brew:/usr/local");
        assert_eq!(instances[0].exe_path, PathBuf::from("/usr/local/bin/brew"));
        assert_eq!(instances[0].prefix, PathBuf::from("/usr/local"));
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert!(instances[0].available());
    }

    #[tokio::test]
    async fn test_detect_finds_both_prefixes_as_separate_instances() {
        // A Mac that has been through a Rosetta phase carries both. They
        // are two Homebrews, not one: separate ids (which is what keys the
        // per-instance `brew update` throttle and the operation lock),
        // separate exe paths, and separate version answers.
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 7.0.3\n");
        runner.respond(
            vec!["/usr/local/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(both_prefixes_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 2, "got {instances:?}");
        let ids: Vec<&str> = instances.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["brew:/opt/homebrew", "brew:/usr/local"],
            "in CANDIDATE_PATHS order, so the Apple Silicon install is listed first"
        );
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert_eq!(instances[1].version, Some("99.9.9".to_string()));
        assert_eq!(
            instances[1].unverified_version,
            Some("99.9.9".to_string()),
            "each instance is judged against brew.toml on its own version"
        );
        assert!(instances[0].unverified_version.is_none());
    }

    #[tokio::test]
    async fn test_detect_finds_nothing_when_homebrew_is_not_installed() {
        // A clean Mac with no Homebrew: no instance, and -- the part worth
        // pinning -- no subprocess either. `detect` must not try to run a
        // `brew` that is not there.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_path_exists_fn(no_homebrew_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert!(instances.is_empty(), "got {instances:?}");
        assert!(
            runner.calls().is_empty(),
            "nothing on disk means nothing to ask for a version"
        );
    }

    #[tokio::test]
    async fn test_detect_marks_an_install_that_does_not_answer_as_not_responding() {
        // The binary is on disk but `--version` fails: the tool is there,
        // it just did not answer, which is `NotResponding` and not a
        // missing install. The MockRunner has no canned response for this
        // argv, which is exactly what that failure looks like here.
        let adapter =
            BrewAdapter::new(Arc::new(MockRunner::new())).with_path_exists_fn(intel_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].version, None);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[tokio::test]
    async fn test_detect_does_not_flag_a_version_listed_in_brew_toml() {
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 7.0.3\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1);
        assert!(
            instances[0].unverified_version.is_none(),
            "7.0.3 is listed in adapters/meta/brew.toml's verified_versions"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version_not_in_brew_toml() {
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 99.9.9\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].unverified_version,
            Some("99.9.9".to_string()),
            "a version not listed in adapters/meta/brew.toml's verified_versions must be flagged"
        );
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
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("first check_updates")
            .candidates;
        assert_eq!(first.len(), 1);
        let second = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("second check_updates")
            .candidates;
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
            exe_path: PathBuf::from("/usr/local/bin/brew"),
            prefix: PathBuf::from("/usr/local"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/usr/local")
        };

        adapter
            .check_updates(&inst_opt, &CheckOptions::default())
            .await
            .expect("check_updates opt/homebrew #1");
        adapter
            .check_updates(&inst_local, &CheckOptions::default())
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
            .check_updates(&inst_opt, &CheckOptions::default())
            .await
            .expect("check_updates opt/homebrew #2");
        adapter
            .check_updates(&inst_local, &CheckOptions::default())
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
    async fn test_check_updates_passes_greedy_flag_when_include_self_updating_is_true() {
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
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        runner.respond(
            vec![
                "/opt/homebrew/bin/brew",
                "outdated",
                "--json=v2",
                "--greedy",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: empty_outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let opts = CheckOptions {
            include_self_updating: true,
        };
        let result = adapter
            .check_updates(&test_instance(), &opts)
            .await
            .expect("check_updates with --greedy")
            .candidates;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_names_a_tapped_package_the_way_the_inventory_does() {
        // brew's two readers spell the same package differently: `brew
        // info --installed --json=v2` gives a cask's `full_token`
        // (`gautham-v/tap/claudebar`), while `brew outdated --json=v2`
        // never emits `full_name` at all and so falls back to the bare
        // `claudebar` -- both true of this repo's recorded fixtures.
        // `ArtifactKey` is what everything downstream joins on: the
        // Installed page's "update available" badge, the Updates page's
        // description lookup, and `Session::refresh`'s check that a
        // carried-forward candidate is still installed. All three miss
        // for every tapped formula and cask while the two spellings
        // disagree, so the candidate is renamed here, once, against the
        // inventory -- the same resolution `reconcile` already does
        // through `qualified_key`.
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
        let installed = r#"{"formulae":[],"casks":[{"token":"claudebar","full_token":"gautham-v/tap/claudebar","name":["ClaudeBar"],"installed":"0.1.1","desc":"Menu bar app"}]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: installed.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated = r#"{"formulae":[],"casks":[{"name":"claudebar","installed_versions":["0.1.1"],"current_version":"0.1.2"}]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].key.name, "gautham-v/tap/claudebar",
            "the candidate must carry the name the inventory lists, not the short one"
        );
        assert_eq!(candidates[0].key.kind, ArtifactKind::Cask);
        assert_eq!(candidates[0].target, "0.1.2");
    }

    #[tokio::test]
    async fn test_check_updates_keeps_the_short_name_when_the_inventory_cannot_be_read() {
        // Qualifying is a best effort over a second command, and that
        // command can fail. When it does, the candidates are still the
        // right candidates under brew's own short spelling -- which is
        // what shipped before -- so the check must not fail with it.
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
        // No response registered for `brew info --installed --json=v2`:
        // MockRunner answers `NoMock`, i.e. the inventory errors.
        let outdated = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1"}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a failed inventory must not fail the update check")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "jq");
    }

    #[tokio::test]
    async fn test_check_updates_reports_a_failed_brew_update_as_a_note_on_the_source() {
        // A failed `brew update` is a fact about Homebrew, not about jq:
        // the catalogue Canager compared against may be behind, so every
        // answer from this round -- including "nothing is outdated" -- may
        // be wrong. It used to ride along as a string on each candidate's
        // `warnings`, where `UpdatesPage` renders a "1 warning" badge on a
        // checkable row and never the warning's text: a badge whose
        // contents the user cannot read. Worse, a source with nothing
        // outdated has no candidates at all, so the one case where the
        // caveat matters most carried it nowhere.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "error: brew update failed: no such remote".to_string(),
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
        let adapter = BrewAdapter::new(runner);
        let outcome = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a failed `brew update` must not fail check_updates");
        assert_eq!(
            outcome.notes,
            vec![InstanceNote::IndexMayBeStale],
            "the source has to carry the caveat, because no package row can"
        );
        assert_eq!(outcome.candidates.len(), 1);
        assert!(
            outcome.candidates[0].warnings.is_empty(),
            "removed, not duplicated: leaving it on the row adds a badge \
             whose text this page never renders, got {:?}",
            outcome.candidates[0].warnings
        );
    }

    #[tokio::test]
    async fn test_check_updates_reports_no_note_when_brew_update_succeeded() {
        // The other half: a note that appears when nothing is wrong would
        // put a permanent "this may not be accurate" banner over a page
        // that is accurate, and the user would learn to ignore it.
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
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let outcome = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(outcome.notes.is_empty(), "got {:?}", outcome.notes);
        assert!(outcome.candidates.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_serialises_maybe_update_across_concurrent_callers() {
        // Before this fix, two concurrent `check_updates` calls for the same
        // instance could both observe the TTL expired and both run `brew
        // update` — this test makes the first `update` slow enough that a
        // second, concurrent call is guaranteed to reach its own TTL check
        // while the first is still in flight, and proves the fix serialises
        // them: only one `brew update` process ever runs.
        let runner = Arc::new(MockRunner::new());
        runner.delay(
            vec!["/opt/homebrew/bin/brew", "update"],
            Duration::from_millis(200),
        );
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
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: empty_outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter =
            Arc::new(BrewAdapter::new(runner.clone()).with_update_ttl(Duration::from_secs(3600)));
        let inst = test_instance();

        let task_a = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(
                async move { adapter.check_updates(&inst, &CheckOptions::default()).await },
            )
        };
        // Give task_a time to acquire the per-instance update lock and start
        // its (slow) `brew update` before task_b starts.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let task_b = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(
                async move { adapter.check_updates(&inst, &CheckOptions::default()).await },
            )
        };

        task_a
            .await
            .expect("task a panicked")
            .expect("check_updates a");
        task_b
            .await
            .expect("task b panicked")
            .expect("check_updates b");

        let update_calls = runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls, 1,
            "two concurrent check_updates calls for the same instance must run `brew update` at most once"
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
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
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
            exe_path: PathBuf::from("/usr/local/bin/brew"),
            prefix: PathBuf::from("/usr/local"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/usr/local")
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
            vec![Warning::WouldBreak {
                names: vec!["python@3.13".to_string()]
            }]
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
            plan.warnings.contains(&Warning::DependentsUnknown),
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

    /// This used to `set_var("SUDO_ASKPASS", ...)` and then `remove_var`
    /// it. Cargo runs a binary's tests in threads by default, and the two
    /// other cask plans in this same binary read that variable while this
    /// one was writing it: a genuine data race on the process environment,
    /// the kind that fails once in fifty CI runs and costs someone an
    /// afternoon. The adapter now reads the variable through an injectable
    /// reader, so the test states the value it wants and leaves the
    /// environment alone.
    #[tokio::test]
    async fn test_plan_passes_through_sudo_askpass_for_cask_install() {
        let runner = Arc::new(MockRunner::new());
        let adapter =
            BrewAdapter::new(runner).with_askpass_fn(|| Some("/tmp/fake-askpass.sh".to_string()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(plan.env.contains(&(
            "SUDO_ASKPASS".to_string(),
            "/tmp/fake-askpass.sh".to_string()
        )));
    }

    /// The other half, which the old env-mutating test could not express
    /// without unsetting a variable the developer running the tests may
    /// legitimately have set: with no `SUDO_ASKPASS` in the environment,
    /// the plan must not invent one. The command preview the user reads
    /// before confirming shows this env, so a phantom entry there is a
    /// lie about what is going to run.
    #[tokio::test]
    async fn test_plan_adds_no_askpass_when_the_variable_is_not_set() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner).with_askpass_fn(|| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(
            !plan.env.iter().any(|(k, _)| k == "SUDO_ASKPASS"),
            "plan env must not carry an askpass that is not set: {:?}",
            plan.env
        );
    }
}
