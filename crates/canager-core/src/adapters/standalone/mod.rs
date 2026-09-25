//! Tools installed by their own installer rather than by a package
//! manager -- Claude Code's native install in this step; Antigravity CLI,
//! Grok Build and rustup in the steps after it (phase 4 spec, §一 D1-D7).
//!
//! One type, `StandaloneAdapter`, driven by one `&'static Recipe` per tool
//! and registered once per tool under the adapter id `standalone-<tool>`,
//! so everything keyed by adapter id today -- the front end's labels,
//! `verified_versions`, the fixture directory, the concurrent detect
//! fan-out -- works for each tool without a special case, and the logic
//! exists once. The instance *is* the native install: `exe_path` is the
//! launcher the installer wrote (`~/.local/bin/claude`, a symlink),
//! `prefix` the tool's own root, and the one artifact under it is the
//! tool itself (`ArtifactKind::Binary`).
//!
//! `recipe` holds the shape of a tool, `recipes` the tools, `route`
//! answers "is this launcher this route's, and which copy runs when its
//! name is typed", `latest` parses and compares versions.

pub mod latest;
pub mod recipe;
pub mod recipes;
pub mod route;

use self::recipe::{Latest, Recipe};
use self::route::Probe;
use crate::adapters::{
    ensure_instance_match, reconcile_from, run_plan, uncheckable_candidate, validate_package_name,
    Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceNote, InstanceStatus,
    ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit,
    Unavailable, UninstallBlocked, UpdateCandidate, UpdateChannel,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How long a version read may take before the runner stops it (the same
/// 30 s every adapter gives `--version`); `detect` then reports the
/// instance as not responding.
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

/// What `detect` learned that the `Adapter` methods without a `HostEnv`
/// need later -- the same seat `CargoAdapter.binstall` is (detect writes,
/// later calls read; `Session` always detects before it asks anything
/// else of an instance). In this step: `home`, which `check_updates` needs
/// to find `~/.claude/settings.json`. Step C adds `euid` (the removal's
/// ownership check), step E `cargo_home` (rustup's cargo lock).
pub struct Detected {
    pub home: PathBuf,
}

/// One tool installed by its own installer, as the `Adapter` contract
/// sees it. Built once per `Recipe` by `all()`; the instance it detects
/// *is* the native install (spec D2).
pub struct StandaloneAdapter {
    recipe: &'static Recipe,
    meta: AdapterMeta,
    runner: Arc<dyn CommandRunner>,
    /// The channel pointer request in `check_updates`.
    http: Arc<dyn HttpClient>,
    detected: Mutex<Option<Detected>>,
}

impl StandaloneAdapter {
    /// Panics on a meta file that does not parse or whose `id` is not
    /// `standalone-<recipe.id>`: both are compile-time data
    /// (`recipes::tests` holds every recipe to them), never a state of a
    /// user's Mac.
    pub fn new(
        recipe: &'static Recipe,
        runner: Arc<dyn CommandRunner>,
        http: Arc<dyn HttpClient>,
    ) -> StandaloneAdapter {
        let meta = AdapterMeta::from_toml(recipe.meta_toml).unwrap_or_else(|e| {
            panic!(
                "adapters/meta/standalone-{}.toml must parse: {e}",
                recipe.id
            )
        });
        assert_eq!(
            meta.id,
            format!("standalone-{}", recipe.id),
            "the meta file's id must be the recipe's adapter id"
        );
        StandaloneAdapter {
            recipe,
            meta,
            runner,
            http,
            detected: Mutex::new(None),
        }
    }

    /// Spec §3.3, steps 1-6: the launcher at the installer's fixed path
    /// (never `resolve_exe`, spec D3), the fingerprint, the version read,
    /// the PATH note. One instance or none; never two.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let launcher = route::expand(&env.home, self.recipe.route.launcher);
        let root = route::expand(&env.home, self.recipe.route.root);
        let (version, unavailable, notes) =
            match route::probe(self.recipe.route.kind, &launcher, &root) {
                Probe::Absent => return Vec::new(),
                // No program to ask: no version read; not unavailable,
                // because nothing about the source has stopped answering --
                // the state is on the row's note, and from step C the
                // uninstall that finishes it must be allowed on this
                // instance (spec Q17).
                Probe::LauncherOnly => (None, None, vec![InstanceNote::LauncherOnly]),
                Probe::Present { real } => {
                    let version = self.read_version(&launcher).await;
                    // The state axis, exactly as uv's rule: the launcher is
                    // there and is ours, it just did not answer.
                    let unavailable = version.is_none().then_some(Unavailable::NotResponding);
                    let notes = route::shadow_note(self.recipe.id, env, &real)
                        .into_iter()
                        .collect();
                    (version, unavailable, notes)
                }
            };
        *self.detected.lock().unwrap() = Some(Detected {
            home: env.home.clone(),
        });
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            // Bare adapter id: one native install per tool is the real
            // cardinality (spec §2.1), and this id is persisted in
            // `Settings.ignored_updates`.
            id: crate::model::instance_id(&self.meta.id, None),
            adapter_id: self.meta.id.clone(),
            // The launcher itself: the program every plan runs, the path
            // the notices take the command name from, the raw path the
            // Unknown page's rule 0 matches.
            exe_path: launcher,
            // The tool's own root: what the launcher must resolve into,
            // and an owned root for the Unknown page's rule 3
            // (`scan::owned_roots`).
            prefix: root,
            scope: Scope::User,
            status: InstanceStatus { unavailable, notes },
            version,
            unverified_version,
            read_only_reason: None,
        }]
    }

    /// `<launcher> --version` with the recipe's environment (the updater
    /// switched off, spec §3.4), parsed per the recipe; `None` when it did
    /// not exit 0, timed out, could not be spawned, or printed no version.
    async fn read_version(&self, launcher: &Path) -> Option<String> {
        let cmd = &self.recipe.version;
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: launcher.to_path_buf(),
                    args: cmd.args.iter().map(|a| a.to_string()).collect(),
                    env: cmd
                        .env
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    cwd: None,
                    timeout: VERSION_TIMEOUT,
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await;
        match output {
            Ok(o) if o.exit_code == Some(0) && !o.timed_out && !o.cancelled => {
                latest::parse_version(&o.stdout, cmd.parse)
            }
            _ => None,
        }
    }

    /// The one artifact's key: the tool id as the name (what `OpRequest`
    /// hands back and `reconcile_from` matches on), `Binary` as the kind
    /// (a standalone tool is a binary; `ArtifactKey` includes the instance
    /// id, so cargo's `Binary` artifacts never collide).
    fn artifact_key(&self, inst: &ManagerInstance) -> ArtifactKey {
        ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Binary,
            name: self.recipe.id.to_string(),
        }
    }

    /// The tool itself, read from the disk again -- not detect's answer
    /// cached: `refresh` calls this under the instance lock and
    /// `run_operation`'s reconcile after an operation must see what is
    /// there now (spec §3.6). The launcher and root are the instance's own
    /// `exe_path` and `prefix`, which detect expanded.
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let (version, path) =
            match route::probe(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
                Probe::Absent => return Ok(Vec::new()),
                Probe::LauncherOnly => (String::new(), None),
                Probe::Present { real } => (
                    self.read_version(&inst.exe_path).await.unwrap_or_default(),
                    Some(real),
                ),
            };
        Ok(vec![InstalledArtifact {
            key: self.artifact_key(inst),
            // From the meta TOML, not a second copy in the recipe.
            display_name: self.meta.name.clone(),
            version,
            // The user ran the installer themselves; `Dependency` would fold
            // the row behind "N components".
            reason: InstallReason::Requested,
            // A sentence has to be localised, and this field is a bare
            // string that does not know the UI language: the summary is
            // left to the front end, keyed by adapter id
            // (`STANDALONE_SUMMARY_KEYS`, which arrives with Task 10 of the
            // phase 4 step B plan).
            description: None,
            homepage: Some(self.meta.homepage.clone()),
            size_bytes: None,
            installed_at: None,
            // The real binary: the Unknown page's rule 2.
            path,
            // For the Updates page's `selfUpdatingHint` sentence, which
            // arrives with Task 10 of the phase 4 step B plan.
            auto_updates: self.recipe.self_updates,
            // No uninstall method in this step (spec §6.1 "Neither"): the
            // gate refuses, the page hides the button and says why. Step
            // C's path-list uninstall replaces this with `None`.
            uninstall_blocked: Some(UninstallBlocked::NoSafeMethod),
        }])
    }

    /// Discovery is phase 5; a standalone tool has nothing to search anyway.
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(format!(
            "{} is one tool with nothing to search; installing tools is phase 5",
            self.meta.name
        )))
    }

    /// Spec §4.1/§4.3/D4/D5: a fresh installed version reading against
    /// the published one; a candidate only when the
    /// published one is greater, comparing dotted integers -- Claude
    /// Code's `stable` pointer sits behind its `latest`, so "different"
    /// would be a downgrade badge. When `probe` no longer finds this
    /// route's program behind the launcher (`Absent`, or `LauncherOnly`:
    /// only the dangling link is left), there is no installed version to
    /// compare: no request and no row. Anything else that stops the
    /// comparison (the version read failing, no network, a non-200, a body
    /// that is not a version, an incomparable pair) is one "could not
    /// check" row, never an `Err`: a failed lookup is not knowing, and an
    /// `Err` would hold the whole source stale.
    ///
    /// `include_self_updating` is not read: that is Homebrew's `--greedy`
    /// for casks whose live version `brew outdated` cannot see. This badge
    /// compares the launcher's live version and is true whatever the
    /// switch says; that the tool usually updates itself is for the row to
    /// say (`selfUpdatingHint`, which arrives with Task 10 of the phase 4
    /// step B plan), not hidden behind a setting.
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        // Detect's version predates refresh's inventory; an auto-update
        // can happen between them. Re-probe and read now, with the same
        // version command environment as detect/inventory/reconcile.
        match route::probe(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
            Probe::Absent | Probe::LauncherOnly => return Ok(CheckOutcome::default()),
            Probe::Present { .. } => {}
        }
        let key = self.artifact_key(inst);
        let Some(current) = self.read_version(&inst.exe_path).await else {
            return Ok(vec![uncheckable_candidate(
                key,
                inst.version.clone().unwrap_or_default(),
                UpdateChannel::Registry,
                "cannot read the installed version now".to_string(),
            )]
            .into());
        };
        let remote = match self.latest_version().await {
            Ok(remote) => remote,
            Err(reason) => {
                return Ok(vec![uncheckable_candidate(
                    key,
                    current,
                    UpdateChannel::Registry,
                    reason,
                )]
                .into())
            }
        };
        Ok(match latest::compare_dotted(&current, &remote) {
            Some(Ordering::Less) => vec![UpdateCandidate {
                key,
                current,
                target: remote,
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }],
            Some(Ordering::Equal | Ordering::Greater) => Vec::new(),
            None => {
                let reason = format!(
                    "cannot compare the installed version {current:?} with the published {remote:?}"
                );
                vec![uncheckable_candidate(
                    key,
                    current,
                    UpdateChannel::Registry,
                    reason,
                )]
            }
        }
        .into())
    }

    /// The newest published version per the recipe's `Latest`, or the
    /// reason it could not be read (one line, for an uncheckable row).
    async fn latest_version(&self) -> Result<String, String> {
        match self.recipe.latest {
            Latest::ClaudeChannel { base } => {
                // `home` from detect's seat; before any detect (which
                // `Session` never does) the default channel is as good an
                // answer as any.
                let home = self
                    .detected
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|d| d.home.clone());
                let channel = match home {
                    Some(home) => latest::claude_channel(&home),
                    None => latest::CHANNEL_LATEST,
                };
                let url = format!("{base}/{channel}");
                let resp = self
                    .http
                    .send(HttpRequest {
                        method: "GET",
                        url,
                        headers: Vec::new(),
                        timeout: Duration::from_secs(30),
                    })
                    .await
                    .map_err(|e| format!("downloads.claude.ai request failed: {e}"))?;
                if resp.status != 200 {
                    return Err(format!(
                        "downloads.claude.ai returned status {}",
                        resp.status
                    ));
                }
                latest::parse_channel_body(&resp.body)
            }
        }
    }

    /// Spec §五: the tool's own documented update command, run against the
    /// launcher through `run_plan` unchanged. `Install` is `Unsupported`
    /// (the installer is Anthropic's and Canager never runs it; installing
    /// tools is phase 5). `Uninstall` is refused with the artifact's own
    /// reason -- the gate (`blocked_uninstall`) refuses it first; this is
    /// its late twin for a stale snapshot. The one artifact is
    /// `Binary`/`<recipe.id>`, so any other name or kind is a request this
    /// adapter cannot mean.
    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        if req.name != self.recipe.id || req.artifact_kind != ArtifactKind::Binary {
            return Err(AdapterError::InvalidName(req.name.clone()));
        }
        match req.kind {
            OpKind::Install => Err(AdapterError::Unsupported(format!(
                "{} is installed by its own installer, which Canager never runs",
                self.meta.name
            ))),
            OpKind::Uninstall => Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::NoSafeMethod,
            }),
            OpKind::Upgrade => {
                let upgrade = &self.recipe.upgrade;
                Ok(Plan {
                    request: req.clone(),
                    // The launcher, exactly as previewed: never a program
                    // the recipe could name (spec 附录 B).
                    program: inst.exe_path.clone(),
                    args: upgrade.args.iter().map(|a| a.to_string()).collect(),
                    // Not the version read's environment: `claude update`
                    // must not be told to stop updating (spec §3.4).
                    env: Vec::new(),
                    // Everything lives under $HOME (spec §五).
                    needs_password: false,
                    locks: vec![ResourceLock(inst.id.clone())],
                    cancel_policy: upgrade.cancel,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: upgrade.timeout_secs,
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
        run_plan(&self.runner, plan, sink, op_id, cancel).await
    }

    /// Step B only executes upgrades: an owned launcher without a readable
    /// version is not sufficient evidence that an upgrade succeeded, so
    /// after `claude update` exits 0 this `Err` makes `run_operation`
    /// report `Unconfirmed`. `run_operation` takes this reading before an
    /// upgrade too, and there the same `Err` only leaves nothing to
    /// compare: an update that exits 0 is then judged by the reading after
    /// alone, as for every adapter (`VersionChange::Unknown`), and is
    /// `Succeeded` when that reading has a version, even if `claude update`
    /// found nothing to install (both cases are in
    /// tests/ops_upgrade_version_test.rs). Inventory still preserves
    /// `LauncherOnly` presence for display and step C's removal. Step C
    /// must use that presence for uninstall verification while keeping this
    /// stricter upgrade check (phase 4 step B plan, deviation 15).
    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        let reconciled = reconcile_from(artifacts, key);
        if reconciled.present && reconciled.version.as_deref().is_none_or(str::is_empty) {
            return Err(AdapterError::Parse(
                "cannot verify the standalone launcher's installed version".to_string(),
            ));
        }
        Ok(reconciled)
    }
}

/// One adapter per recipe in `recipes::RECIPES`, over the shared runner
/// and http client, for `Session::new`'s registration list.
pub fn all(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> Vec<Arc<dyn Adapter>> {
    recipes::RECIPES
        .iter()
        .map(|&recipe| {
            Arc::new(StandaloneAdapter::new(recipe, runner.clone(), http.clone()))
                as Arc<dyn Adapter>
        })
        .collect()
}

#[async_trait]
impl Adapter for StandaloneAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        StandaloneAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        StandaloneAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        StandaloneAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        StandaloneAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        StandaloneAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        StandaloneAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        StandaloneAdapter::reconcile(self, inst, key).await
    }
}

/// Synthetic installs in a throwaway home, for this module's tests: real
/// links and real files on a real file system, since `route::probe`
/// answers from `lstat`, `readlink` and `realpath` and nothing else. Never
/// a recorded fixture (spec §9.3: fingerprint tests build their own
/// layouts and must not write under `adapters/fixtures/`).
#[cfg(test)]
pub(super) mod testing {
    use crate::runner::HostEnv;
    use std::path::{Path, PathBuf};

    /// A fresh, canonical directory under the system temp dir, removed on
    /// drop. Canonical, so paths built from it compare equal to what
    /// `canonicalize` answers (macOS's `/var/folders` is `/private/var/…`).
    pub struct TempHome(PathBuf);

    impl TempHome {
        pub fn new(tag: &str) -> TempHome {
            let raw = std::env::temp_dir().join(format!(
                "canager-standalone-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&raw).expect("create temp home");
            TempHome(std::fs::canonicalize(&raw).expect("canonical temp home"))
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        /// Creates `rel` (and its parents) under the home.
        pub fn dir(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(&path).expect("create dir");
            path
        }

        /// Writes a small regular file at `rel` (parents created).
        pub fn file(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::fs::write(&path, b"#!/bin/sh\n").expect("write file");
            path
        }

        /// An executable synthetic target; it is never actually spawned.
        pub fn executable(&self, rel: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.file(rel);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("executable target");
            path
        }

        /// A symbolic link at `rel` whose text is `target` exactly --
        /// absolute or relative, existing or not (parents created).
        pub fn link(&self, rel: &str, target: &Path) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::os::unix::fs::symlink(target, &path).expect("symlink");
            path
        }

        /// A `HostEnv` whose home is this directory and whose `PATH` is
        /// `path_dirs`.
        pub fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
            HostEnv {
                path_dirs,
                home: self.0.clone(),
                euid: 501,
                cargo_home: None,
                ollama_host: None,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The native Claude Code layout, as `ls -la` shows it on a Mac with
    /// the installer's defaults: `~/.local/bin/claude` linking (absolute
    /// text) to `~/.local/share/claude/versions/<version>`.
    pub struct ClaudeLayout {
        pub launcher: PathBuf,
        pub root: PathBuf,
        pub real: PathBuf,
    }

    pub fn claude_layout(home: &TempHome, version: &str) -> ClaudeLayout {
        let real = home.executable(&format!(".local/share/claude/versions/{version}"));
        let launcher = home.link(".local/bin/claude", &real);
        ClaudeLayout {
            launcher,
            root: home.path().join(".local/share/claude"),
            real,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recipes::CLAUDE;
    use super::testing::{claude_layout, TempHome};
    use super::*;
    use crate::adapters::{Adapter, CheckOptions};
    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::model::{
        ArtifactKind, CancelPolicy, InstallReason, InstanceNote, OpKind, OpRequest, Outcome,
        ResourceLock, Unavailable, UninstallBlocked, UpdateChannel, Warning,
    };
    use crate::runner::{CommandOutput, MockRunner, RunnerError};
    // For `RecordingRunner` below. Named here as well as through
    // `super::*` (the parent imports it for `impl Adapter`), so this impl
    // does not lean on the imports above; an explicit import beside a glob
    // is not a warning.
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    fn exited_0(stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    fn adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new()))
    }

    /// `MockRunner` keys and records argv only. This one records every
    /// `CommandSpec` it is handed, so a test can prove what environment,
    /// timeout and output use the version read carried -- the point of
    /// spec §3.4 is one environment variable.
    struct RecordingRunner {
        specs: StdMutex<Vec<CommandSpec>>,
        output: CommandOutput,
    }

    #[async_trait]
    impl CommandRunner for RecordingRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, RunnerError> {
            self.specs.lock().unwrap().push(spec);
            Ok(self.output.clone())
        }
    }

    #[test]
    fn test_new_takes_its_meta_from_the_recipe_and_names_the_standalone_id() {
        let adapter = adapter(Arc::new(MockRunner::new()));
        assert_eq!(adapter.meta.id, "standalone-claude");
        assert_eq!(adapter.meta.name, "Claude Code");
        assert!(
            adapter.detected.lock().unwrap().is_none(),
            "nothing detected yet"
        );
    }

    #[tokio::test]
    async fn test_detect_lists_the_native_install_as_one_instance() {
        let home = TempHome::new("detect-present");
        let meta = AdapterMeta::from_toml(CLAUDE.meta_toml).expect("meta");
        let version = meta.verified_versions.first().expect("a verified version");
        let layout = claude_layout(&home, version);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version} (Claude Code)\n")),
        );
        let adapter = adapter(runner);

        let instances = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await;

        assert_eq!(instances.len(), 1);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-claude");
        assert_eq!(inst.adapter_id, "standalone-claude");
        // The launcher itself, not the binary it resolves to: the program
        // every plan runs and the path `sourceNoticesFor` takes the
        // command name from (spec §2.2).
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.scope, Scope::User);
        assert_eq!(inst.version, Some(version.clone()));
        assert_eq!(
            inst.unverified_version, None,
            "the metadata version is verified"
        );
        assert_eq!(inst.read_only_reason, None);
        assert_eq!(inst.status.unavailable, None);
        assert!(
            inst.status.notes.is_empty(),
            "PATH finds this very copy: no note"
        );
        assert_eq!(
            adapter
                .detected
                .lock()
                .unwrap()
                .as_ref()
                .map(|d| d.home.clone()),
            Some(home.path().to_path_buf()),
            "detect seats home for check_updates"
        );
    }

    #[tokio::test]
    async fn test_detect_reads_the_version_with_the_autoupdater_off_and_a_thirty_second_timeout() {
        // Spec §3.4: Claude Code checks for updates on startup (doc text)
        // and a refresh is read-only, so the documented switch for that
        // background check goes on this read (and inventory's) whether or
        // not a bare `--version` would reach the updater -- never on the
        // upgrade plan.
        let home = TempHome::new("detect-env");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0("2.1.281 (Claude Code)\n"),
        });
        let adapter = adapter(runner.clone());

        adapter.detect(&home.env(vec![])).await;

        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.program, layout.launcher);
        assert_eq!(spec.args, vec!["--version".to_string()]);
        assert_eq!(
            spec.env,
            vec![("DISABLE_AUTOUPDATER".to_string(), "1".to_string())]
        );
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.output_use, OutputUse::Parsed);
        assert_eq!(spec.cwd, None);
    }

    #[tokio::test]
    async fn test_detect_flags_a_version_outside_the_verified_list() {
        let meta = AdapterMeta::from_toml(CLAUDE.meta_toml).expect("meta");
        let version = (0_u64..)
            .map(|major| format!("{major}.0.0"))
            .find(|version| !meta.verified_versions.contains(version))
            .expect("a version outside the finite recorded list");
        let home = TempHome::new("detect-unverified");
        let layout = claude_layout(&home, &version);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version} (Claude Code)\n")),
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances[0].version, Some(version.clone()));
        assert_eq!(instances[0].unverified_version, Some(version));
    }

    #[tokio::test]
    async fn test_detect_marks_a_failed_version_read_as_not_responding() {
        // The launcher is there and is this route's, but `--version` did
        // not answer: the state axis (`NotResponding`), like uv's rule.
        let home = TempHome::new("detect-not-responding");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "dyld: Library not loaded\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, None);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert_eq!(instances[0].exe_path, layout.launcher);
    }

    #[tokio::test]
    async fn test_detect_marks_a_timed_out_version_read_as_not_responding() {
        // A `claude --version` that never returns is stopped by the runner
        // at 30 s and reported as not answering; nothing hangs and nothing
        // crashes. A guard, not a known bug: the one hang claude.md §6
        // records is `claude update`'s (before 2.1.214, on a directory
        // named like a shell rc file), and none is recorded for `--version`.
        let home = TempHome::new("detect-timed-out");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: true,
                cancelled: false,
            },
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[tokio::test]
    async fn test_detect_marks_a_runner_error_as_not_responding() {
        // No canned answer is the mock's spawn failure; a real one is the
        // same shape (`RunnerError::Spawn`).
        let home = TempHome::new("detect-spawn-failed");
        claude_layout(&home, "2.1.281");
        let instances = adapter(Arc::new(MockRunner::new()))
            .detect(&home.env(vec![]))
            .await;
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[tokio::test]
    async fn test_detect_carries_the_path_note_when_another_copy_wins() {
        let home = TempHome::new("detect-shadowed");
        let layout = claude_layout(&home, "2.1.281");
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        let brew_bin = home.dir("opt/homebrew/bin");
        home.link("opt/homebrew/bin/claude", &cask);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let env = home.env(vec![brew_bin, home.path().join(".local/bin")]);
        let instances = adapter(runner).detect(&env).await;
        assert_eq!(
            instances[0].status.notes,
            vec![InstanceNote::ShadowedByHomebrew]
        );
        // And with nothing on PATH at all: NotOnPath.
        let instances = adapter(Arc::new(MockRunner::new()))
            .detect(&home.env(vec![]))
            .await;
        assert_eq!(instances[0].status.notes, vec![InstanceNote::NotOnPath]);
        // And with Homebrew's directory on PATH but not the launcher's:
        // NotOnPath too. Homebrew's copy runs when the name is typed, but
        // this copy is not behind it on PATH; it is not on PATH at all.
        let instances = adapter(Arc::new(MockRunner::new()))
            .detect(&home.env(vec![home.path().join("opt/homebrew/bin")]))
            .await;
        assert_eq!(instances[0].status.notes, vec![InstanceNote::NotOnPath]);
    }

    #[tokio::test]
    async fn test_detect_keeps_a_dangling_launcher_as_a_launcher_only_row() {
        // The half-uninstalled state: no version read at all (there is no
        // program to run); not unavailable (the source has not stopped
        // answering, and step C's uninstall must be allowed on this
        // instance); the note says what is left.
        let home = TempHome::new("detect-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let runner = Arc::new(MockRunner::new());
        let adapter = adapter(runner.clone());

        let instances = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await;

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].exe_path, launcher);
        assert_eq!(instances[0].version, None);
        assert_eq!(instances[0].status.unavailable, None);
        assert_eq!(instances[0].status.notes, vec![InstanceNote::LauncherOnly]);
        assert!(runner.calls().is_empty(), "nothing to run");
    }

    #[tokio::test]
    async fn test_detect_finds_nothing_without_a_launcher_or_with_a_package_managers_copy() {
        let home = TempHome::new("detect-absent");
        let runner = Arc::new(MockRunner::new());
        assert!(adapter(runner.clone())
            .detect(&home.env(vec![]))
            .await
            .is_empty());
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        home.link(".local/bin/claude", &cask);
        assert!(adapter(runner.clone())
            .detect(&home.env(vec![]))
            .await
            .is_empty());
        assert!(
            runner.calls().is_empty(),
            "no `--version` for a row this adapter does not own"
        );
    }

    fn instance_for(
        layout: &super::testing::ClaudeLayout,
        version: Option<&str>,
    ) -> ManagerInstance {
        ManagerInstance {
            exe_path: layout.launcher.clone(),
            prefix: layout.root.clone(),
            version: version.map(str::to_string),
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        }
    }

    #[tokio::test]
    async fn test_inventory_is_the_tool_itself_with_no_safe_uninstall_method() {
        let home = TempHome::new("inventory-present");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));

        let artifacts = adapter.inventory(&inst).await.expect("inventory");

        assert_eq!(artifacts.len(), 1);
        let a = &artifacts[0];
        assert_eq!(
            a.key,
            ArtifactKey {
                instance_id: "standalone-claude".to_string(),
                kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            }
        );
        assert_eq!(a.display_name, "Claude Code");
        assert_eq!(a.version, "2.1.281");
        assert_eq!(a.reason, InstallReason::Requested);
        // Left to the front end to localise (`STANDALONE_SUMMARY_KEYS`,
        // which arrives with Task 10 of the phase 4 step B plan), not a
        // bare English string here.
        assert_eq!(a.description, None);
        assert_eq!(
            a.homepage.as_deref(),
            Some("https://code.claude.com/docs/en/setup")
        );
        // The real binary, for the Unknown page's rule 2.
        assert_eq!(a.path, Some(layout.real.clone()));
        assert!(a.auto_updates, "claude updates itself in the background");
        assert_eq!(a.uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));
        assert_eq!(a.size_bytes, None);
        assert_eq!(a.installed_at, None);
    }

    #[tokio::test]
    async fn test_inventory_reads_the_disk_again_rather_than_detects_answer() {
        // `refresh` calls inventory under the instance lock and
        // `run_operation`'s reconcile must see the disk as it is now (spec
        // §3.6): a launcher removed since detect means an empty inventory.
        let home = TempHome::new("inventory-fresh");
        let layout = claude_layout(&home, "2.1.281");
        let inst = instance_for(&layout, Some("2.1.281"));
        std::fs::remove_file(&layout.launcher).expect("remove launcher");
        let artifacts = adapter(Arc::new(MockRunner::new()))
            .inventory(&inst)
            .await
            .expect("inventory");
        assert!(artifacts.is_empty());
    }

    #[tokio::test]
    async fn test_inventory_of_a_launcher_only_install_has_no_version_and_no_path() {
        let home = TempHome::new("inventory-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let inst = ManagerInstance {
            exe_path: launcher,
            prefix: root,
            version: None,
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        };
        let runner = Arc::new(MockRunner::new());
        let artifacts = adapter(runner.clone())
            .inventory(&inst)
            .await
            .expect("inventory");
        assert_eq!(
            artifacts.len(),
            1,
            "still a row: the link is still there, and the state must be visible"
        );
        assert_eq!(artifacts[0].version, "");
        assert_eq!(artifacts[0].path, None);
        assert_eq!(
            artifacts[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_with_the_live_version_and_absent_when_gone() {
        let home = TempHome::new("reconcile");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));
        let key = ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Binary,
            name: "claude".to_string(),
        };

        let present = adapter.reconcile(&inst, &key).await.expect("reconcile");
        assert!(present.present);
        assert_eq!(present.version, Some("2.1.281".to_string()));

        // Another kind or name of the same instance is not this artifact.
        let other = ArtifactKey {
            kind: ArtifactKind::Tool,
            ..key.clone()
        };
        assert!(
            !adapter
                .reconcile(&inst, &other)
                .await
                .expect("reconcile")
                .present
        );

        std::fs::remove_file(&layout.launcher).expect("remove launcher");
        let absent = adapter.reconcile(&inst, &key).await.expect("reconcile");
        assert!(!absent.present);
        assert_eq!(absent.version, None);
    }

    #[tokio::test]
    async fn test_reconcile_rejects_an_unreadable_version_and_a_dangling_launcher() {
        let home = TempHome::new("reconcile-broken");
        let layout = claude_layout(&home, "2.1.281");
        let inst = instance_for(&layout, Some("2.1.281"));
        let adapter = adapter(Arc::new(MockRunner::new()));
        let key = adapter.artifact_key(&inst);
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));
        std::fs::remove_file(&layout.real).unwrap();
        let artifacts = adapter.inventory(&inst).await.unwrap();
        assert_eq!(artifacts.len(), 1, "presence survives for step C");
        assert_eq!(artifacts[0].path, None);
        assert_eq!(artifacts[0].version, "");
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let home = TempHome::new("search");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let result = adapter
            .search(&instance_for(&layout, Some("2.1.281")), "claude")
            .await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    const LATEST_URL: &str = "https://downloads.claude.ai/claude-code-releases/latest";
    const STABLE_URL: &str = "https://downloads.claude.ai/claude-code-releases/stable";

    fn answer(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: body.to_string(),
        }
    }

    /// An adapter that has detected the layout in `home` (so `Detected`
    /// holds that home), over `http`.
    async fn detected_adapter(
        home: &TempHome,
        layout: &super::testing::ClaudeLayout,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance) {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);
        let inst = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await
            .remove(0);
        (adapter, inst)
    }

    #[tokio::test]
    async fn test_check_updates_uses_the_version_after_inventory_not_detects_version() {
        let home = TempHome::new("check-fresh");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter = StandaloneAdapter::new(&CLAUDE, runner.clone(), http);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        runner.respond(argv, exited_0("2.1.290 (Claude Code)\n"));
        assert_eq!(
            adapter.inventory(&inst).await.unwrap()[0].version,
            "2.1.290"
        );
        assert_eq!(inst.version.as_deref(), Some("2.1.281"));
        assert!(adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap()
            .candidates
            .is_empty());
        assert_eq!(
            runner.calls().len(),
            3,
            "detect, inventory, then a fresh check read"
        );
    }

    #[tokio::test]
    async fn test_prereleases_stay_available_and_incomparable_pairs_are_uncheckable() {
        for (local, remote) in [
            ("2.1.281-beta", "2.1.290"),
            ("2.1.281", "2.1.290-beta"),
            ("2.1.281+build.7", "2.1.290+build.8"),
        ] {
            let home = TempHome::new("check-prerelease");
            let layout = claude_layout(&home, local);
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0(&format!("{local} (Claude Code)\n")),
            );
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(remote));
            let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            assert_eq!(inst.status.unavailable, None);
            assert_eq!(inst.version.as_deref(), Some(local));
            assert_eq!(adapter.inventory(&inst).await.unwrap()[0].version, local);
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .unwrap();
            assert_eq!(out.candidates.len(), 1);
            let candidate = &out.candidates[0];
            assert!(!candidate.checkable);
            assert_eq!(candidate.current, local);
            assert_eq!(
                candidate.target, local,
                "existing uncheckable candidate convention"
            );
            assert!(
                matches!(&candidate.warnings[..], [Warning::Message(message)]
                if message.contains("cannot compare") && message.contains(local) && message.contains(remote))
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_does_not_use_a_stale_version_after_a_failed_read() {
        let home = TempHome::new("check-failed-live-read");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter = StandaloneAdapter::new(&CLAUDE, runner.clone(), http.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        runner.respond(argv, exited_0(""));
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        assert_eq!(out.candidates.len(), 1);
        assert!(!out.candidates[0].checkable);
        assert_eq!(out.candidates[0].current, "2.1.281");
        assert!(http.calls().is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_lists_a_newer_published_version_as_one_candidate() {
        let home = TempHome::new("check-newer");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290\n"));
        let (adapter, inst) = detected_adapter(&home, &layout, http.clone()).await;

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert!(out.notes.is_empty());
        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-claude".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "claude".to_string(),
                },
                current: "2.1.281".to_string(),
                target: "2.1.290".to_string(),
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }]
        );
        assert_eq!(http.calls(), vec![LATEST_URL.to_string()]);
        let request = &http.requests()[0];
        assert_eq!(request.method, "GET");
        assert!(
            request.headers.is_empty(),
            "nothing of Canager's own but the client's UA"
        );
        assert_eq!(request.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_check_updates_lists_nothing_when_the_pointer_is_equal_or_behind() {
        // The `stable` pointer was 2.1.273 while 2.1.281 was installed
        // (claude.md §4, 2026-09-24): "different" would badge a downgrade.
        // Only remote > local is an update (spec §4.3, D4).
        for body in ["2.1.281", "2.1.273\n", "2.0.999"] {
            let home = TempHome::new("check-not-newer");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(body));
            let (adapter, inst) = detected_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert!(out.candidates.is_empty(), "{body:?}");
        }
    }

    #[tokio::test]
    async fn test_check_updates_asks_the_stable_pointer_when_settings_say_so() {
        // Q7: `~/.claude/settings.json` `autoUpdatesChannel: "stable"`
        // picks the other pointer; missing or malformed is `latest`.
        let home = TempHome::new("check-stable");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude");
        std::fs::write(
            home.path().join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .expect("write settings");
        let http = Arc::new(MockHttpClient::new());
        http.respond(STABLE_URL, answer("2.1.273"));
        let (adapter, inst) = detected_adapter(&home, &layout, http.clone()).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert_eq!(http.calls(), vec![STABLE_URL.to_string()]);

        std::fs::write(home.path().join(".claude/settings.json"), "{ not json").expect("write");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.281"));
        let (adapter, inst) = detected_adapter(&home, &layout, http.clone()).await;
        adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(http.calls(), vec![LATEST_URL.to_string()]);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_failed_request_uncheckable_never_an_error() {
        // A network failure is "Canager could not find out", not a failed
        // source (which would hold the snapshot stale): one row at the
        // installed version, `checkable: false`, with the reason.
        let home = TempHome::new("check-network");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.fail(LATEST_URL, "connection refused");
        let (adapter, inst) = detected_adapter(&home, &layout, http).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("a failed lookup is not a source failure");
        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert!(!c.checkable);
        assert_eq!(c.current, "2.1.281");
        assert_eq!(c.target, "2.1.281");
        assert_eq!(c.channel, UpdateChannel::Registry);
        assert!(
            matches!(&c.warnings[..], [Warning::Message(m)] if m.contains("connection refused"))
        );
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_non_200_uncheckable() {
        let home = TempHome::new("check-status");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            LATEST_URL,
            HttpResponse {
                status: 503,
                body: "<html>busy</html>".to_string(),
            },
        );
        let (adapter, inst) = detected_adapter(&home, &layout, http).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(out.candidates.len(), 1);
        assert!(!out.candidates[0].checkable);
        assert!(
            matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("503"))
        );
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_non_version_answer_uncheckable() {
        // An HTML page with status 200 (a captive portal), multiple tokens:
        // no candidate is built from it, and the reason quotes only a
        // little of the body.
        for body in [
            "<html><body>Sign in to the network</body></html>",
            "2.1.290 extra",
        ] {
            let home = TempHome::new("check-garbage");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(body));
            let (adapter, inst) = detected_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert_eq!(out.candidates.len(), 1, "{body:?}");
            assert!(!out.candidates[0].checkable);
            assert!(
                matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("did not answer with a version") && m.len() < 120),
                "{:?}",
                out.candidates[0].warnings
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_ignores_the_include_self_updating_switch() {
        // D5: the switch is Homebrew's `--greedy`, for casks whose live
        // version `brew outdated` cannot see. This badge is read from the
        // launcher's live version and is true whatever the switch says.
        for include_self_updating in [false, true] {
            let home = TempHome::new("check-greedy");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer("2.1.290"));
            let (adapter, inst) = detected_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(
                    &inst,
                    &CheckOptions {
                        include_self_updating,
                        ..CheckOptions::default()
                    },
                )
                .await
                .expect("check_updates");
            assert_eq!(
                out.candidates.len(),
                1,
                "include_self_updating={include_self_updating}"
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_asks_nothing_for_a_launcher_only_row() {
        // No installed version to compare, so no request and no row: the
        // notice already says what is left.
        let home = TempHome::new("check-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let http = Arc::new(MockHttpClient::new());
        let adapter = StandaloneAdapter::new(&CLAUDE, Arc::new(MockRunner::new()), http.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.exe_path, launcher);
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert!(http.calls().is_empty());
    }

    fn request(kind: OpKind, artifact_kind: ArtifactKind, name: &str) -> OpRequest {
        OpRequest {
            kind,
            instance_id: "standalone-claude".to_string(),
            artifact_kind,
            name: name.to_string(),
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_is_the_tools_own_update_command_without_the_version_env() {
        // Spec §五: `<launcher> update`, 1800 s, KillThenReconcile, the
        // instance's own lock, no password. Version reads add
        // `DISABLE_AUTOUPDATER=1`; upgrade adds no environment override.
        // RealRunner inherits ambient variables, including this one.
        let home = TempHome::new("plan-upgrade");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));

        let plan = adapter
            .plan(
                &inst,
                &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"),
            )
            .await
            .expect("plan");

        assert_eq!(plan.program, layout.launcher);
        assert_eq!(plan.args, vec!["update".to_string()]);
        assert!(plan.env.is_empty(), "upgrade adds no environment override");
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("standalone-claude".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(plan.warnings.is_empty());
        assert!(plan.affected.is_empty());
        assert_eq!(plan.timeout_secs, 1800);
        assert_eq!(plan.request.name, "claude");
    }

    #[tokio::test]
    async fn test_plan_refuses_a_request_for_another_instance() {
        let home = TempHome::new("plan-other-instance");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let req = OpRequest {
            instance_id: "standalone-grok".to_string(),
            ..request(OpKind::Upgrade, ArtifactKind::Binary, "claude")
        };
        assert!(matches!(
            adapter
                .plan(&instance_for(&layout, Some("2.1.281")), &req)
                .await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_a_name_or_kind_that_is_not_this_tool() {
        // The one artifact is `Binary`/`claude`; anything else is a request
        // this adapter cannot mean, refused before an argv exists.
        let home = TempHome::new("plan-wrong-name");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        for (kind, name) in [
            (ArtifactKind::Binary, "grok"),
            (ArtifactKind::Binary, "claude-code"),
            (ArtifactKind::Tool, "claude"),
        ] {
            assert!(
                matches!(
                    adapter
                        .plan(&inst, &request(OpKind::Upgrade, kind, name))
                        .await,
                    Err(AdapterError::InvalidName(_))
                ),
                "{kind:?} {name}"
            );
        }
        // A name that fails `validate_package_name` is refused as such
        // before the tool-name comparison.
        assert!(matches!(
            adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Binary, "-rf")
                )
                .await,
            Err(AdapterError::InvalidName(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_install_as_unsupported_and_uninstall_as_no_safe_method() {
        // An uninstall is refused by the gate first (`blocked_uninstall`
        // reads the artifact's `NoSafeMethod`), so this refusal is its late
        // twin for a stale snapshot. The gate has no install-specific rule:
        // an install against an installed, answering tool reaches this
        // `plan`, and `Unsupported` is the answer it gets.
        let home = TempHome::new("plan-refusals");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter
                .plan(
                    &inst,
                    &request(OpKind::Install, ArtifactKind::Binary, "claude")
                )
                .await,
            Err(AdapterError::Unsupported(_))
        ));
        match adapter
            .plan(
                &inst,
                &request(OpKind::Uninstall, ArtifactKind::Binary, "claude"),
            )
            .await
        {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::NoSafeMethod)
            }
            other => panic!("expected UninstallBlocked(NoSafeMethod), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_runs_the_plan_and_streams_its_output() {
        let home = TempHome::new("execute");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));
        let plan = adapter
            .plan(
                &inst,
                &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"),
            )
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1, "one log line, streamed");
    }

    #[tokio::test]
    async fn test_all_builds_one_adapter_per_recipe_under_its_standalone_id() {
        let adapters = all(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let ids: Vec<String> = adapters.iter().map(|a| a.meta().id.clone()).collect();
        assert_eq!(ids, vec!["standalone-claude".to_string()]);
        assert_eq!(adapters.len(), super::recipes::RECIPES.len());
    }

    #[tokio::test]
    async fn test_the_adapter_trait_delegates_to_the_inherent_methods() {
        // Through `dyn Adapter`, as Session sees it.
        let home = TempHome::new("trait");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter: Arc<dyn Adapter> = Arc::new(StandaloneAdapter::new(
            &CLAUDE,
            runner,
            Arc::new(MockHttpClient::new()),
        ));
        let instances = adapter.detect(&home.env(vec![])).await;
        assert_eq!(instances.len(), 1);
        let artifacts = adapter.inventory(&instances[0]).await.expect("inventory");
        assert_eq!(artifacts[0].display_name, "Claude Code");
        assert!(matches!(
            adapter.search(&instances[0], "x").await,
            Err(AdapterError::Unsupported(_))
        ));
    }

    /// The recorded fixture directory for the version the meta file
    /// verifies: `adapters/fixtures/standalone-claude/<verified>/`.
    fn fixture(name: &str) -> String {
        let adapter = adapter(Arc::new(MockRunner::new()));
        let version = adapter
            .meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version")
            .clone();
        let path = format!("../../adapters/fixtures/standalone-claude/{version}/{name}");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_the_recorded_version_line_parses_to_the_verified_version() {
        let verified = adapter(Arc::new(MockRunner::new())).meta.verified_versions[0].clone();
        assert_eq!(
            latest::parse_version(&fixture("version.txt"), CLAUDE.version.parse),
            Some(verified)
        );
    }

    #[test]
    fn test_the_recorded_channel_pointers_are_bare_versions() {
        let latest_pointer = latest::parse_channel_body(&fixture("latest.txt")).expect("latest");
        let stable_pointer = latest::parse_channel_body(&fixture("stable.txt")).expect("stable");
        // The recording's reason for existing: the stable pointer is not
        // ahead of the latest one.
        assert_ne!(
            latest::compare_dotted(&stable_pointer, &latest_pointer),
            Some(Ordering::Greater),
            "stable {stable_pointer} is not ahead of latest {latest_pointer}"
        );
    }

    #[tokio::test]
    async fn test_check_updates_over_the_recorded_pointers_lists_only_a_real_update() {
        // Fed each recorded pointer as the endpoint's body: the stable
        // pointer, behind or equal to the installed version, yields no
        // candidate; the latest pointer yields one exactly when it is
        // greater than the installed version -- both derived from the
        // recording, so a re-recording on a later day stays honest.
        let installed = adapter(Arc::new(MockRunner::new())).meta.verified_versions[0].clone();
        for (name, url) in [("stable.txt", STABLE_URL), ("latest.txt", LATEST_URL)] {
            let body = fixture(name);
            let pointer = latest::parse_channel_body(&body).expect(name);
            let home = TempHome::new("check-recorded");
            let layout = claude_layout(&home, &installed);
            if name == "stable.txt" {
                home.dir(".claude");
                std::fs::write(
                    home.path().join(".claude/settings.json"),
                    r#"{"autoUpdatesChannel":"stable"}"#,
                )
                .expect("write settings");
            }
            let http = Arc::new(MockHttpClient::new());
            http.respond(url, answer(&body));
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0(&format!("{installed} (Claude Code)\n")),
            );
            let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            match latest::compare_dotted(&installed, &pointer) {
                Some(Ordering::Less) => {
                    assert_eq!(out.candidates.len(), 1, "{name}");
                    assert_eq!(out.candidates[0].target, pointer);
                    assert!(out.candidates[0].checkable);
                }
                Some(Ordering::Equal | Ordering::Greater) => assert!(out.candidates.is_empty()),
                None => {
                    assert_eq!(out.candidates.len(), 1, "{name}");
                    assert!(!out.candidates[0].checkable);
                    assert_eq!(out.candidates[0].current, installed);
                    assert_eq!(out.candidates[0].target, installed);
                }
            }
        }
    }
}
