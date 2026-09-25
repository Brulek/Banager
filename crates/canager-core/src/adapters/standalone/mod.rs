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
pub mod removal;
pub mod route;

use self::recipe::{Latest, Recipe, Uninstall};
use self::route::Probe;
use crate::adapters::{
    ensure_instance_match, reconcile_from, run_plan, uncheckable_candidate, validate_package_name,
    Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, Fault, InstallReason, InstalledArtifact, InstanceNote,
    InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction, Reconciled,
    ResourceLock, Scope, SearchHit, Unavailable, UninstallBlocked, UpdateCandidate, UpdateChannel,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, OutputUse};
use crate::trash::Trasher;
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
/// else of an instance). `home`, which `check_updates` needs to find
/// `~/.claude/settings.json` and the removal needs to expand its paths;
/// `euid`, which the removal's check 3 compares each path's owner with
/// (`removal::plan_removal`). Step E adds `cargo_home` (rustup's cargo
/// lock). `Clone`, so `plan` and `execute` take a copy out of the mutex
/// before they await anything, and the removal owns one for the blocking
/// pool (`removal::Job`).
#[derive(Clone, Debug)]
pub struct Detected {
    pub home: PathBuf,
    pub euid: u32,
}

/// What `inventory` read at the launcher, kept for the `check_updates`
/// that follows it -- so the Installed page's row and the Updates page's
/// candidate come from one reading of the disk, and Claude Code updating
/// itself between the two cannot put one version on each page (B's Astra
/// finding B-2). `Session::refresh` calls `inventory` and then
/// `check_updates` for each instance, in that order, under the instance's
/// lock (`refresh_round` in session/refresh.rs), and nothing else calls
/// `check_updates`. `check_updates` takes the reading out, so one
/// inventory feeds one check, and a check with nothing to take is
/// refused. `reconcile` reads the disk too but not through `inventory`,
/// and leaves this alone.
#[derive(Clone, Debug)]
enum Reading {
    /// Not the install detect listed any more (`inventory` says which
    /// way): `inventory` refused with this reason, and `check_updates`
    /// refuses with the same, so `refresh` keeps the previous round's rows
    /// on both pages and marks them stale.
    Changed(String),
    /// The dangling launcher detect listed, still: no version to compare.
    LauncherOnly,
    /// This route's launcher, and what `--version` said (`None`: it did
    /// not answer).
    Present { version: Option<String> },
}

/// One look at the launcher, from the disk now -- never detect's answer
/// cached: `refresh` reads under the instance lock and `run_operation`'s
/// reconcile after an operation must see what is there now (spec §3.6).
/// The probe, and, for this route's launcher, its answer to `--version`.
/// Read by `inventory` and `reconcile`, through `look`.
struct Look {
    probe: Probe,
    /// `None` when the launcher is not this route's (there is no program
    /// to ask) or did not answer.
    version: Option<String>,
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
    /// The system's "move to Trash", for a path-list uninstall
    /// (`removal::execute_removal`, from `execute`): `RealTrasher` in
    /// production (`Session::new`), `MockTrasher` in tests -- injected
    /// like the runner and the client.
    trasher: Arc<dyn Trasher>,
    /// The pause after each item of a path-list uninstall
    /// (`removal::PUT_BACK_SETTLE`); zero in tests (`with_trash_gap`).
    /// Read by `execute`.
    trash_gap: Duration,
    detected: Mutex<Option<Detected>>,
    /// Written by `inventory`, taken by `check_updates` (`Reading`).
    inventoried: Mutex<Option<Reading>>,
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
        trasher: Arc<dyn Trasher>,
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
            trasher,
            trash_gap: removal::PUT_BACK_SETTLE,
            detected: Mutex::new(None),
            inventoried: Mutex::new(None),
        }
    }

    /// Test seam, like `BrewAdapter::with_background_change`: the pause
    /// after each item of a path-list uninstall -- zero in tests, so no
    /// test waits seconds per item. Public so `tests/` can use it too.
    pub fn with_trash_gap(mut self, gap: Duration) -> StandaloneAdapter {
        self.trash_gap = gap;
        self
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
            euid: env.euid,
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

    /// `Look`: the probe at the instance's own `exe_path` and `prefix`,
    /// which detect expanded, then `--version` when the launcher is this
    /// route's.
    async fn look(&self, inst: &ManagerInstance) -> Look {
        let probe = route::probe(self.recipe.route.kind, &inst.exe_path, &inst.prefix);
        let version = match probe {
            Probe::Present { .. } => self.read_version(&inst.exe_path).await,
            Probe::Absent | Probe::LauncherOnly => None,
        };
        Look { probe, version }
    }

    /// The tool itself, read from the disk again (`look`), not detect's
    /// answer cached. What is there has to be the install detect listed:
    /// the launcher whole, or the launcher alone (the instance then carries
    /// `InstanceNote::LauncherOnly`, which only `detect` writes). Anything
    /// else -- the launcher gone; the program files gone behind a launcher
    /// detect saw whole; the program files back behind one detect saw
    /// dangling -- is refused, because no artifact list could say it: the
    /// instance row is detect's, and would stay healthy beside no row, or
    /// beside an empty-version row, with the Updates page saying
    /// everything is up to date (B's Astra finding B-2). `refresh` turns
    /// the refusal into "this refresh did not finish for this source": the
    /// previous round's rows are kept and marked stale, and the next
    /// refresh detects what is there now. Whatever the answer, the reading
    /// is left for `check_updates` (`Reading`).
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let look = self.look(inst).await;
        let listed_launcher_only = inst.status.notes.contains(&InstanceNote::LauncherOnly);
        let reading = match (&look.probe, listed_launcher_only) {
            (Probe::Absent, _) => Reading::Changed(format!(
                "{}'s launcher {} is gone since it was detected",
                self.meta.name,
                inst.exe_path.display()
            )),
            (Probe::LauncherOnly, false) => Reading::Changed(format!(
                "{}'s program files are gone since it was detected; only the launcher {} is left",
                self.meta.name,
                inst.exe_path.display()
            )),
            (Probe::Present { .. }, true) => Reading::Changed(format!(
                "{}'s program files are back since it was detected with the launcher {} alone",
                self.meta.name,
                inst.exe_path.display()
            )),
            (Probe::LauncherOnly, true) => Reading::LauncherOnly,
            (Probe::Present { .. }, false) => Reading::Present {
                version: look.version.clone(),
            },
        };
        *self.inventoried.lock().unwrap() = Some(reading.clone());
        match reading {
            Reading::Changed(reason) => Err(AdapterError::Refused(reason)),
            Reading::LauncherOnly | Reading::Present { .. } => Ok(self.rows(inst, look)),
        }
    }

    /// The artifact list for what `look` found: nothing for `Absent`; for
    /// the launcher alone, the row with no version and no path that an
    /// Uninstall finishes (spec §3.6); for this route's launcher, the tool
    /// with the version it answered (empty when it did not) and the real
    /// binary. Read by `inventory` and `reconcile`.
    fn rows(&self, inst: &ManagerInstance, look: Look) -> Vec<InstalledArtifact> {
        let (version, path) = match look.probe {
            Probe::Absent => return Vec::new(),
            Probe::LauncherOnly => (String::new(), None),
            Probe::Present { real } => (look.version.unwrap_or_default(), Some(real)),
        };
        vec![InstalledArtifact {
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
            // A recipe with no uninstall method (spec §6.1 "Neither"; none
            // in the first batch, the second batch's Ollama.app): the gate
            // refuses, the page hides the button and says why. With a path
            // list, nothing blocks it.
            uninstall_blocked: self
                .recipe
                .uninstall
                .is_none()
                .then_some(UninstallBlocked::NoSafeMethod),
        }]
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

    /// Spec §4.1/§4.3/D4/D5: the installed version `inventory` read just
    /// before this (`Reading`) against the published one; a candidate only
    /// when the published one is greater, comparing dotted integers --
    /// Claude Code's `stable` pointer sits behind its `latest`, so
    /// "different" would be a downgrade badge. Never detect's
    /// `inst.version`, which predates the inventory, and never a read of
    /// its own, which could postdate it: Claude Code updating itself
    /// between the two would then put one version on the Installed page
    /// and another on the Updates page (B's Astra finding B-2); a
    /// self-update that lands between the inventory and this is listed by
    /// the next refresh, on both pages. A launcher-only row has no
    /// installed version to compare: no request and no row. An inventory
    /// that refused because the install is not what detect listed is
    /// refused here with the same reason, so `refresh` keeps the previous
    /// candidates beside the previous rows. Anything else that stops the
    /// comparison (the version read having failed, no network, a non-200, a
    /// body that is not a version, an incomparable pair) is one "could not
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
        let reading = self.inventoried.lock().unwrap().take();
        let version = match reading {
            // Unreachable through `Session`, which inventories before it
            // checks (`refresh_round`), so no sentence of its own.
            None => {
                return Err(AdapterError::Refused(format!(
                    "{} has not been inventoried since its last update check",
                    self.meta.name
                )))
            }
            Some(Reading::Changed(reason)) => return Err(AdapterError::Refused(reason)),
            Some(Reading::LauncherOnly) => return Ok(CheckOutcome::default()),
            Some(Reading::Present { version }) => version,
        };
        let key = self.artifact_key(inst);
        let Some(current) = version else {
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
    /// tools is phase 5). `Uninstall` is the recipe's path list as a
    /// `TrashPaths` plan under the removal's checks (spec §6.2-§6.3), with
    /// what the preview saw at each path riding along on this side only
    /// (`previewed`, skipped by serde; Ruling 10), or `NoSafeMethod` for a
    /// recipe without one -- the gate (`blocked_uninstall`) refuses that
    /// first; this is its late twin for a stale snapshot. The one artifact
    /// is `Binary`/`<recipe.id>`, so any other name or kind is a request
    /// this adapter cannot mean.
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
            OpKind::Uninstall => {
                let Some(uninstall) = &self.recipe.uninstall else {
                    return Err(AdapterError::UninstallBlocked {
                        reason: UninstallBlocked::NoSafeMethod,
                    });
                };
                match *uninstall {
                    Uninstall::Paths { remove, keep } => {
                        let removal = removal::plan_removal(&removal::Job {
                            recipe: self.recipe,
                            detected: self.detected_or_refuse()?,
                            remove,
                            keep,
                        })?;
                        Ok(Plan {
                            request: req.clone(),
                            // No command: `execute` moves these itself. What
                            // the preview saw at each stays with the plan on
                            // this side (`previewed`, skipped on the wire).
                            action: PlanAction::TrashPaths {
                                paths: removal.paths,
                                previewed: removal.identities,
                            },
                            // Everything lives under $HOME (spec §6.2).
                            needs_password: false,
                            locks: vec![ResourceLock(inst.id.clone())],
                            // Between items the token is watched; there is
                            // no process to stop (spec §6.2).
                            cancel_policy: CancelPolicy::KillThenReconcile,
                            warnings: removal.warnings,
                            // "Would break": nothing depends on a tool's
                            // own files this way, and a non-empty list
                            // disables the confirm button.
                            affected: Vec::new(),
                            timeout_secs: removal::TIMEOUT_SECS,
                        })
                    }
                }
            }
            OpKind::Upgrade => {
                let upgrade = &self.recipe.upgrade;
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        // The launcher, exactly as previewed: never a
                        // program the recipe could name (spec 附录 B).
                        program: inst.exe_path.clone(),
                        args: upgrade.args.iter().map(|a| a.to_string()).collect(),
                        // Not the version read's environment: `claude
                        // update` must not be told to stop updating (spec
                        // §3.4).
                        env: Vec::new(),
                    },
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

    /// What `detect` wrote, or a plain `Refused` when nothing has been
    /// detected -- unreachable through `Session`, which detects before it
    /// plans (spec §3.2), so no sentence of its own.
    fn detected_or_refuse(&self) -> Result<Detected, AdapterError> {
        self.detected.lock().unwrap().clone().ok_or_else(|| {
            AdapterError::Refused(format!(
                "{} has not been detected in this session",
                self.meta.name
            ))
        })
    }

    /// A `Command` plan -- the upgrade, `<launcher> update` -- runs through
    /// `run_plan` like every source's, after one more look at the launcher
    /// immediately before the spawn (below). A `TrashPaths` plan is carried
    /// out here, item by item (`removal::execute_removal`), against the
    /// list re-read from the recipe and the disk and compared with what the
    /// preview saw (`previewed`) -- the plan's paths are what the user
    /// confirmed, not the source of truth.
    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        match &plan.action {
            // The launcher is a link that the user, another installer or
            // the tool's own updater can re-point between the preview and
            // the click: at Homebrew's or npm's copy, at some other file,
            // or at nothing (B's Astra finding B-1). The plan names that
            // path, so spawning it would run whatever is behind it now --
            // another copy's updater. So the launcher is looked at again
            // here, right before the spawn, with the same `probe` detect
            // used: it must still be this route's (`Present`: one link
            // straight into the root, resolving there, not a package
            // manager's). Anything else -- gone, dangling, a plain file,
            // resolving outside the root, or a launcher `probe` cannot look
            // at -- is `Fault::PathChanged` naming the launcher, nothing
            // started: what a path-list uninstall reports for a path that
            // changed. A link re-pointed at a newer version *inside* the
            // root (the updater's own work) is still this route's, and
            // runs. What remains is the instant between this look and the
            // spawn, the same edge the removal documents (`take_turn`).
            PlanAction::Command { program, .. } => {
                let detected = self.detected_or_refuse()?;
                let launcher = route::expand(&detected.home, self.recipe.route.launcher);
                // `plan` names the launcher and nothing else (spec 附录 B):
                // a plan naming any other program was not built by it, and
                // is a bug's, refused before anything is looked at or run.
                if *program != launcher {
                    return Err(AdapterError::Refused(format!(
                        "{}: the plan runs {} rather than the launcher {}",
                        self.meta.name,
                        program.display(),
                        launcher.display()
                    )));
                }
                let root = route::expand(&detected.home, self.recipe.route.root);
                if !matches!(
                    route::probe(self.recipe.route.kind, &launcher, &root),
                    Probe::Present { .. }
                ) {
                    return Ok(Outcome::CanagerFailed(Fault::PathChanged {
                        path: crate::scan::display_path(&launcher, &detected.home)
                            .display()
                            .to_string(),
                    }));
                }
                run_plan(&self.runner, plan, sink, op_id, cancel).await
            }
            PlanAction::TrashPaths { paths, previewed } => {
                let Some(Uninstall::Paths { remove, keep }) = self.recipe.uninstall else {
                    return Err(AdapterError::Refused(format!(
                        "{} has no path list to carry out",
                        self.meta.name
                    )));
                };
                removal::execute_removal(
                    &removal::Job {
                        recipe: self.recipe,
                        detected: self.detected_or_refuse()?,
                        remove,
                        keep,
                    },
                    removal::Confirmed { paths, previewed },
                    &self.trasher,
                    removal::Pacing {
                        settle: self.trash_gap,
                        budget: Duration::from_secs(plan.timeout_secs),
                    },
                    sink,
                    op_id,
                    cancel,
                )
                .await
            }
        }
    }

    /// The reading before and after an upgrade (and after an install,
    /// which this adapter never plans), from the disk now (`look`, then
    /// `rows`) -- not through `inventory`, which refuses an install that
    /// is no longer what detect listed: a launcher gone after an upgrade
    /// must read as absent here (`GoneAfterUpgrade`), not as a refusal
    /// (`Unconfirmed`). An owned launcher without a readable version --
    /// one that did not answer, or the dangling link alone -- is no
    /// evidence that an upgrade succeeded, so it is refused (`Parse`) and
    /// `run_operation` reports `Unconfirmed` (B's Astra finding 1). After
    /// an uninstall `run_operation` reads `reconcile_after_uninstall`
    /// instead, which counts that launcher as still there.
    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let look = self.look(inst).await;
        let reconciled = reconcile_from(self.rows(inst, look), key);
        if reconciled.present && reconciled.version.as_deref().is_none_or(str::is_empty) {
            return Err(AdapterError::Parse(
                "cannot verify the standalone launcher's installed version".to_string(),
            ));
        }
        Ok(reconciled)
    }

    /// The reading after an uninstall (`Adapter::reconcile_after_uninstall`):
    /// presence alone, from the disk now, through `route::probe_strict`. A
    /// launcher-only launcher -- the dangling link a stopped path-list
    /// uninstall leaves -- is present, so `run_operation` reports the stop
    /// truthfully (`Cancelled` after the user's Cancel,
    /// `StillInstalledAfterUninstall` after a run that claimed success) and
    /// the next refresh shows the row a second Uninstall finishes; a
    /// launcher that is gone is absent (spec §3.6); and a launcher Canager
    /// cannot look at -- a permission error, a loop -- is neither: an
    /// error, which `run_operation` reports as `Unconfirmed`, never as a
    /// finished uninstall (Ruling 27). No version is read: presence is the
    /// whole question, and a stopped uninstall's launcher has none.
    pub async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let present =
            match route::probe_strict(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
                Ok(Probe::Absent) => false,
                Ok(Probe::Present { .. } | Probe::LauncherOnly) => true,
                Err(error) => {
                    return Err(AdapterError::Parse(format!(
                        "cannot tell whether {} is still there: {error}",
                        inst.exe_path.display()
                    )))
                }
            };
        // The one artifact, matched as `reconcile_from` matches -- kind and
        // name, never the instance id (adapters/mod.rs says why).
        let this_tool = key.kind == ArtifactKind::Binary && key.name == self.recipe.id;
        Ok(Reconciled {
            present: present && this_tool,
            version: None,
        })
    }
}

/// One adapter per recipe in `recipes::RECIPES`, over the shared runner,
/// http client and trasher, for `Session::new`'s registration list.
pub fn all(
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    trasher: Arc<dyn Trasher>,
) -> Vec<Arc<dyn Adapter>> {
    recipes::RECIPES
        .iter()
        .map(|&recipe| {
            Arc::new(StandaloneAdapter::new(
                recipe,
                runner.clone(),
                http.clone(),
                trasher.clone(),
            )) as Arc<dyn Adapter>
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

    async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        StandaloneAdapter::reconcile_after_uninstall(self, inst, key).await
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
                rustup_home: None,
                zdotdir: None,
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

    /// Takes every permission off the folder `path` until dropped, so an
    /// `lstat` of anything inside it fails with a permission error -- the
    /// "could not tell" a probe must never read as "gone" -- and gives
    /// `0o755` back on drop, so `TempHome` can remove the tree. `None` when
    /// the tests run as root, whom permissions do not stop: the caller
    /// then skips its check and says so.
    pub struct Unreadable(PathBuf);

    impl Unreadable {
        pub fn new(path: &Path) -> Option<Unreadable> {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if std::fs::metadata(path).expect("the folder exists").uid() == 0 {
                eprintln!("running as root: permissions stop nothing, check skipped");
                return None;
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000))
                .expect("take the folder's permissions away");
            Some(Unreadable(path.to_path_buf()))
        }
    }

    impl Drop for Unreadable {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recipe::{Route, RouteKind, UpgradeCmd, VersionCmd, VersionParse};
    use super::recipes::CLAUDE;
    use super::testing::{claude_layout, TempHome};
    use super::*;
    use crate::adapters::{Adapter, CheckOptions};
    use crate::events::{LogNote, OperationEvent, VecSink};
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::model::{
        ArtifactKind, CancelPolicy, Fault, InstallReason, InstanceNote, ItemKind, KeptWhat, OpKind,
        OpRequest, Outcome, PlanAction, RemovedWhat, ResourceLock, Unavailable, UninstallBlocked,
        UninstallUnsafeReason, UpdateChannel, Warning,
    };
    use crate::runner::{CommandOutput, MockRunner, RunnerError};
    use crate::testing::{command_args, command_env, command_program};
    use crate::trash::MockTrasher;
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
        adapter_with(runner, Arc::new(MockTrasher::new()))
    }

    /// An adapter over a trasher the test holds on to (to read its calls
    /// and its bin), with no pause after each item (ruling 12).
    fn adapter_with(
        runner: Arc<dyn CommandRunner>,
        trasher: Arc<MockTrasher>,
    ) -> StandaloneAdapter {
        StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new()), trasher)
            .with_trash_gap(Duration::ZERO)
    }

    /// `TempHome::env` with the euid of the user who made the temp home
    /// (the files in it are theirs), for a `detect` whose `Detected.euid`
    /// the removal's check 3 will compare against. (`TempHome::env` says
    /// 501; nothing before this step read it.)
    fn env_as_owner(home: &TempHome) -> HostEnv {
        use std::os::unix::fs::MetadataExt;
        HostEnv {
            euid: std::fs::metadata(home.path()).expect("home metadata").uid(),
            ..home.env(vec![])
        }
    }

    /// The claude layout plus the cache and settings the dialog lists,
    /// detected (so `Detected` is filled) with `--version` answered.
    async fn full_install(
        tag: &str,
        trasher: Arc<MockTrasher>,
    ) -> (
        TempHome,
        super::testing::ClaudeLayout,
        StandaloneAdapter,
        ManagerInstance,
    ) {
        let home = TempHome::new(tag);
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude/projects/p/session.jsonl");
        home.file(".claude.json");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter_with(runner, trasher);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        (home, layout, adapter, inst)
    }

    fn uninstall() -> OpRequest {
        request(OpKind::Uninstall, ArtifactKind::Binary, "claude")
    }

    fn notes_of(sink: &VecSink) -> Vec<LogNote> {
        sink.snapshot()
            .into_iter()
            .filter_map(|event| match event {
                OperationEvent::Note { note, .. } => Some(note),
                _ => None,
            })
            .collect()
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
    async fn test_inventory_is_the_tool_itself_and_offers_the_path_list_uninstall() {
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
        // The recipe has a path list, so nothing blocks the uninstall: the
        // gate lets it through to `plan`, and the page shows the button.
        assert_eq!(a.uninstall_blocked, None);
        assert_eq!(a.size_bytes, None);
        assert_eq!(a.installed_at, None);
    }

    #[tokio::test]
    async fn test_inventory_refuses_an_install_that_is_not_what_detect_listed() {
        // `refresh` calls inventory under the instance lock, after detect,
        // and reads the disk as it is now, never detect's answer (spec
        // §3.6). What it finds has to fit the instance detect listed, or
        // the snapshot would say two things at once -- a healthy row with
        // no artifact, or with an empty-version one, beside "Everything is
        // up to date" (B's Astra finding B-2). So a launcher that is gone,
        // program files gone behind a launcher detect saw whole, or
        // program files back behind a launcher detect saw dangling are each
        // a refusal, which `refresh` turns into "this refresh did not
        // finish for this source" with the previous rows kept; the next
        // refresh lists what is there.
        let home = TempHome::new("inventory-changed");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let whole = instance_for(&layout, Some("2.1.281"));
        let dangling = ManagerInstance {
            status: InstanceStatus {
                unavailable: None,
                notes: vec![InstanceNote::LauncherOnly],
            },
            ..instance_for(&layout, None)
        };

        // The program files are back behind a launcher detect saw dangling.
        let Err(AdapterError::Refused(reason)) = adapter.inventory(&dangling).await else {
            panic!("program files back behind a launcher detect saw dangling is a refusal");
        };
        assert!(reason.contains("program files"), "{reason}");

        // The program files are gone behind a launcher detect saw whole.
        std::fs::remove_dir_all(layout.root.join("versions")).expect("remove the program files");
        let Err(AdapterError::Refused(reason)) = adapter.inventory(&whole).await else {
            panic!("program files gone behind a launcher detect saw whole is a refusal");
        };
        assert!(reason.contains("program files"), "{reason}");

        // The launcher is gone, whatever detect listed.
        std::fs::remove_file(&layout.launcher).expect("remove the launcher");
        for inst in [&whole, &dangling] {
            let Err(AdapterError::Refused(reason)) = adapter.inventory(inst).await else {
                panic!("a launcher that is gone is a refusal");
            };
            assert!(reason.contains("gone"), "{reason}");
        }
    }

    #[tokio::test]
    async fn test_inventory_of_a_launcher_only_install_has_no_version_and_no_path() {
        // The row detect listed as the launcher alone (its
        // `InstanceNote::LauncherOnly` is what tells inventory so).
        let home = TempHome::new("inventory-launcher-only");
        let root = home.path().join(".local/share/claude");
        home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let runner = Arc::new(MockRunner::new());
        let adapter = adapter(runner.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.status.notes, vec![InstanceNote::LauncherOnly]);
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(
            artifacts.len(),
            1,
            "still a row: the link is still there, and the state must be visible"
        );
        assert_eq!(artifacts[0].version, "");
        assert_eq!(artifacts[0].path, None);
        // The uninstall that finishes this state is allowed on this row
        // (spec Q17).
        assert_eq!(artifacts[0].uninstall_blocked, None);
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

        // Gone is absent here -- `GoneAfterUpgrade` -- where `inventory`
        // would refuse (`test_inventory_refuses_an_install_that_is_not_what_detect_listed`).
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
        // The dangling link alone is present with nothing to verify: the
        // same refusal, read from the disk and not through `inventory`
        // (which refuses this instance outright, detected whole).
        std::fs::remove_file(&layout.real).unwrap();
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
    /// holds that home) and read its inventory (so `Reading` holds the
    /// launcher's version), over `http`: what `refresh` has done before it
    /// checks for updates.
    async fn refreshed_adapter(
        home: &TempHome,
        layout: &super::testing::ClaudeLayout,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance) {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(&CLAUDE, runner, http, Arc::new(MockTrasher::new()));
        let inst = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await
            .remove(0);
        adapter.inventory(&inst).await.expect("inventory");
        (adapter, inst)
    }

    #[tokio::test]
    async fn test_check_updates_compares_the_version_inventory_read_not_detects_nor_a_read_of_its_own(
    ) {
        // Detect read 2.1.281, the inventory 2.1.290, and by the time of
        // the check the launcher answers 2.1.299 (Claude Code updated
        // itself again): the candidate is built from the inventory's
        // 2.1.290, so the Updates page names the version the Installed
        // page's row shows, and the check runs nothing.
        let home = TempHome::new("check-inventoried");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.295"));
        let adapter =
            StandaloneAdapter::new(&CLAUDE, runner.clone(), http, Arc::new(MockTrasher::new()));
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.version.as_deref(), Some("2.1.281"));
        runner.respond(argv.clone(), exited_0("2.1.290 (Claude Code)\n"));
        assert_eq!(
            adapter.inventory(&inst).await.unwrap()[0].version,
            "2.1.290"
        );
        runner.respond(argv, exited_0("2.1.299 (Claude Code)\n"));
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        assert_eq!(out.candidates.len(), 1);
        assert_eq!(out.candidates[0].current, "2.1.290");
        assert_eq!(out.candidates[0].target, "2.1.295");
        assert_eq!(
            runner.calls().len(),
            2,
            "detect and inventory; the check reads nothing"
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
            let adapter =
                StandaloneAdapter::new(&CLAUDE, runner, http, Arc::new(MockTrasher::new()));
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
        let adapter = StandaloneAdapter::new(
            &CLAUDE,
            runner.clone(),
            http.clone(),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        runner.respond(argv, exited_0(""));
        // The inventory's read did not answer: its row has no version, and
        // the check, comparing that same reading, cannot check -- detect's
        // 2.1.281 is only what its row displays.
        assert_eq!(adapter.inventory(&inst).await.unwrap()[0].version, "");
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
        let (adapter, inst) = refreshed_adapter(&home, &layout, http.clone()).await;

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
            let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
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
        let (adapter, inst) = refreshed_adapter(&home, &layout, http.clone()).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert_eq!(http.calls(), vec![STABLE_URL.to_string()]);

        std::fs::write(home.path().join(".claude/settings.json"), "{ not json").expect("write");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.281"));
        let (adapter, inst) = refreshed_adapter(&home, &layout, http.clone()).await;
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
        let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
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
        let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
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
            let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
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
            let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
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
        let adapter = StandaloneAdapter::new(
            &CLAUDE,
            Arc::new(MockRunner::new()),
            http.clone(),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.exe_path, launcher);
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert!(http.calls().is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_refuses_as_inventory_did_and_needs_an_inventory_first() {
        // The reading `inventory` leaves for `check_updates` (`Reading`):
        // a check with no inventory before it has nothing to compare and
        // is refused (unreachable through `Session`); after an inventory
        // that refused because the launcher is gone, the check refuses
        // with the same reason and asks the endpoint nothing, so `refresh`
        // keeps the previous candidates beside the previous rows; and one
        // inventory feeds one check.
        let home = TempHome::new("check-changed");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter =
            StandaloneAdapter::new(&CLAUDE, runner, http.clone(), Arc::new(MockTrasher::new()));
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let Err(AdapterError::Refused(reason)) =
            adapter.check_updates(&inst, &CheckOptions::default()).await
        else {
            panic!("a check with no inventory before it is refused");
        };
        assert!(reason.contains("inventoried"), "{reason}");

        std::fs::remove_file(&layout.launcher).expect("remove the launcher");
        let Err(AdapterError::Refused(refused)) = adapter.inventory(&inst).await else {
            panic!("a launcher that is gone is a refusal");
        };
        let Err(AdapterError::Refused(reason)) =
            adapter.check_updates(&inst, &CheckOptions::default()).await
        else {
            panic!("the check refuses as the inventory did");
        };
        assert_eq!(reason, refused);
        assert!(http.calls().is_empty());
        // That reading has been taken: another check needs another inventory.
        assert!(matches!(
            adapter.check_updates(&inst, &CheckOptions::default()).await,
            Err(AdapterError::Refused(_))
        ));
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

        assert_eq!(command_program(&plan), layout.launcher);
        assert_eq!(command_args(&plan), vec!["update".to_string()]);
        assert!(
            command_env(&plan).is_empty(),
            "upgrade adds no environment override"
        );
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

    /// Claude Code's recipe with no uninstall method: what the second
    /// batch's Ollama.app will be (spec §6.1 "Neither"). Every other field
    /// is `CLAUDE`'s.
    static NO_UNINSTALL: Recipe = Recipe {
        id: "claude",
        meta_toml: include_str!("../../../../../adapters/meta/standalone-claude.toml"),
        route: Route {
            kind: RouteKind::SymlinkIntoRoot,
            launcher: "~/.local/bin/claude",
            root: "~/.local/share/claude",
        },
        version: VersionCmd {
            args: &["--version"],
            env: &[("DISABLE_AUTOUPDATER", "1")],
            parse: VersionParse::FirstToken,
        },
        latest: Latest::ClaudeChannel {
            base: "https://downloads.claude.ai/claude-code-releases",
        },
        self_updates: true,
        upgrade: UpgradeCmd {
            args: &["update"],
            timeout_secs: 1800,
            cancel: CancelPolicy::KillThenReconcile,
        },
        uninstall: None,
    };

    #[tokio::test]
    async fn test_plan_refuses_install_as_unsupported() {
        // The gate has no install-specific rule: an install against an
        // installed, answering tool reaches this `plan`, and `Unsupported`
        // is the answer it gets (B's corrected comment, kept).
        let home = TempHome::new("plan-install");
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
    }

    #[tokio::test]
    async fn test_a_recipe_without_an_uninstall_method_says_so_and_refuses_to_plan_one() {
        // Spec §6.1 "Neither": the artifact carries `NoSafeMethod` (the
        // gate and the page read it), and `plan(Uninstall)` refuses with
        // the same reason for a stale snapshot. The one production path of
        // that variant after this step.
        let home = TempHome::new("plan-no-uninstall");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(
            &NO_UNINSTALL,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(
            artifacts[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
        match adapter.plan(&inst, &uninstall()).await {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::NoSafeMethod)
            }
            other => panic!("expected UninstallBlocked(NoSafeMethod), got {other:?}"),
        }
    }

    /// The claude layout, detected -- so `Detected` holds the home
    /// `execute` finds the launcher under when it looks at it again before
    /// an upgrade -- over a runner the test keeps (to read its calls) that
    /// answers `--version` and `update` at the launcher.
    struct UpgradeSetup {
        home: TempHome,
        layout: super::testing::ClaudeLayout,
        runner: Arc<MockRunner>,
        adapter: StandaloneAdapter,
        inst: ManagerInstance,
    }

    async fn detected_for_upgrade(tag: &str) -> UpgradeSetup {
        let home = TempHome::new(tag);
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let adapter = adapter(runner.clone());
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        UpgradeSetup {
            home,
            layout,
            runner,
            adapter,
            inst,
        }
    }

    async fn upgrade_plan(adapter: &StandaloneAdapter, inst: &ManagerInstance) -> Plan {
        adapter
            .plan(
                inst,
                &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"),
            )
            .await
            .expect("plan")
    }

    /// The `update` calls the runner saw: what `execute` spawned, apart
    /// from detect's version read.
    fn update_calls(runner: &MockRunner, layout: &super::testing::ClaudeLayout) -> usize {
        let update = vec![
            layout.launcher.to_str().unwrap().to_string(),
            "update".to_string(),
        ];
        runner
            .calls()
            .iter()
            .filter(|call| **call == update)
            .count()
    }

    #[tokio::test]
    async fn test_execute_runs_the_plan_and_streams_its_output() {
        // `_home` is bound, not left to `..`: the temp home lives as long as
        // it does, and the launcher `execute` looks at again is in it.
        let UpgradeSetup {
            home: _home,
            layout,
            runner,
            adapter,
            inst,
        } = detected_for_upgrade("execute").await;
        let plan = upgrade_plan(&adapter, &inst).await;
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(update_calls(&runner, &layout), 1);
        assert_eq!(sink.snapshot().len(), 1, "one log line, streamed");
    }

    #[tokio::test]
    async fn test_execute_refuses_an_upgrade_when_the_launcher_is_no_longer_the_native_installs() {
        // B's Astra finding B-1: between the preview and the click,
        // `~/.local/bin/claude` can be re-pointed at Homebrew's or npm's
        // copy (the user reinstalling another way), at some other file,
        // replaced by a plain file, left dangling, or removed. The plan
        // names that path, and spawning it would run whatever is behind it
        // now -- another copy's updater. `execute` looks at the launcher
        // again immediately before spawning, as detect does: not this
        // route's, and nothing is started; the fault names the launcher,
        // as a path-list uninstall's names a path that changed.
        /// What happens to the launcher after the preview, in one case.
        type Retarget = fn(&TempHome, &super::testing::ClaudeLayout);
        let cases: [(&str, Retarget); 6] = [
            ("re-pointed at Homebrew's copy", |home, layout| {
                let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
                std::fs::remove_file(&layout.launcher).unwrap();
                std::os::unix::fs::symlink(cask, &layout.launcher).unwrap();
            }),
            ("re-pointed at npm's copy", |home, layout| {
                let cli = home
                    .executable("opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js");
                std::fs::remove_file(&layout.launcher).unwrap();
                std::os::unix::fs::symlink(cli, &layout.launcher).unwrap();
            }),
            ("re-pointed outside the root", |home, layout| {
                let elsewhere = home.executable("elsewhere/claude");
                std::fs::remove_file(&layout.launcher).unwrap();
                std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();
            }),
            ("replaced by a plain file", |home, layout| {
                std::fs::remove_file(&layout.launcher).unwrap();
                home.executable(".local/bin/claude");
            }),
            ("dangling: the program files gone", |_home, layout| {
                std::fs::remove_dir_all(&layout.root).unwrap();
            }),
            ("removed", |_home, layout| {
                std::fs::remove_file(&layout.launcher).unwrap();
            }),
        ];
        for (case, retarget) in cases {
            let UpgradeSetup {
                home,
                layout,
                runner,
                adapter,
                inst,
            } = detected_for_upgrade("execute-upgrade-retargeted").await;
            let plan = upgrade_plan(&adapter, &inst).await;
            retarget(&home, &layout);
            let sink = Arc::new(VecSink::new());

            let outcome = adapter
                .execute(&plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");

            assert_eq!(
                outcome,
                Outcome::CanagerFailed(Fault::PathChanged {
                    path: "~/.local/bin/claude".to_string()
                }),
                "{case}"
            );
            assert_eq!(update_calls(&runner, &layout), 0, "{case}: nothing spawned");
            assert!(sink.snapshot().is_empty(), "{case}: no log line");
        }
    }

    #[tokio::test]
    async fn test_execute_runs_an_upgrade_after_the_launcher_moved_to_a_newer_version_inside_the_root(
    ) {
        // Claude Code updating itself between the preview and the click
        // re-points the launcher at a new file inside its own root. That is
        // still the native install, and `<launcher> update` is still the
        // command the user confirmed: it runs. (A path-list uninstall
        // refuses the same change, because there the link itself is what
        // moves: `test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview`.)
        let UpgradeSetup {
            home,
            layout,
            runner,
            adapter,
            inst,
        } = detected_for_upgrade("execute-upgrade-self-updated").await;
        let plan = upgrade_plan(&adapter, &inst).await;
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(update_calls(&runner, &layout), 1);
        assert_eq!(sink.snapshot().len(), 1, "one log line, streamed");
    }

    #[tokio::test]
    async fn test_execute_refuses_an_upgrade_plan_that_names_another_program_as_canagers_own_bug() {
        // `plan` names the launcher and nothing else (spec 附录 B). A plan
        // whose program is any other file was not built by `plan`, and so
        // is one with no detect before it (no home to find the launcher
        // under -- unreachable through `Session`, which detects first):
        // both are refused as Canager's own bug (`Refused`, which
        // `run_operation` reports as `Fault::Internal`), nothing spawned --
        // never run on the plan's word.
        let UpgradeSetup {
            home,
            layout,
            runner,
            adapter: detected,
            inst,
        } = detected_for_upgrade("execute-upgrade-other-program").await;
        let other = home.executable("elsewhere/claude");
        runner.respond(
            vec![other.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let plan = Plan {
            action: PlanAction::Command {
                program: other,
                args: vec!["update".to_string()],
                env: Vec::new(),
            },
            ..upgrade_plan(&detected, &inst).await
        };
        assert!(matches!(
            detected
                .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                .await,
            Err(AdapterError::Refused(_))
        ));
        assert_eq!(runner.calls().len(), 1, "only detect's version read");

        let fresh = Arc::new(MockRunner::new());
        let undetected = adapter(fresh.clone());
        let plan = upgrade_plan(&undetected, &instance_for(&layout, Some("2.1.281"))).await;
        assert!(matches!(
            undetected
                .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                .await,
            Err(AdapterError::Refused(_))
        ));
        assert!(fresh.calls().is_empty(), "nothing spawned");
    }

    #[tokio::test]
    async fn test_all_builds_one_adapter_per_recipe_under_its_standalone_id() {
        let adapters = all(
            Arc::new(MockRunner::new()),
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
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
            Arc::new(MockTrasher::new()),
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

    /// `layout.txt` is evidence, not parser input, and it is the one
    /// recorded file in its directory whose text was edited after
    /// recording: its README says the home directory's absolute path
    /// became `~` and the owner column's account name became `user`, so
    /// that the directory names no account and no machine. This checks
    /// that no absolute home directory and no `.local` host name survives
    /// in that file or in the README beside it, and that the edited
    /// launcher line still names the verified version.
    #[test]
    fn test_the_recorded_layout_and_its_readme_name_no_home_directory_or_host() {
        let verified = adapter(Arc::new(MockRunner::new())).meta.verified_versions[0].clone();
        let layout = fixture("layout.txt");
        let readme = fixture("README.md");
        for (name, text) in [("layout.txt", &layout), ("README.md", &readme)] {
            for prefix in ["/Users/", "/home/"] {
                assert!(
                    !text.contains(prefix),
                    "{name} names an absolute home directory under {prefix}"
                );
            }
            // `X.local` is a Mac's Bonjour host name when `X` ends in a
            // letter, digit or hyphen; `~/.local/...` is preceded by `/`
            // and is not one.
            let names_a_host = text.match_indices(".local").any(|(at, _)| {
                text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
            });
            assert!(!names_a_host, "{name} names a .local host");
        }
        assert!(
            layout.contains(&format!(
                "~/.local/bin/claude -> ~/.local/share/claude/versions/{verified}"
            )),
            "layout.txt's launcher line names the verified version under ~"
        );
        assert!(
            layout.contains("~/.local/share/claude/versions:"),
            "layout.txt's versions listing is headed by the directory under ~"
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
            let adapter =
                StandaloneAdapter::new(&CLAUDE, runner, http, Arc::new(MockTrasher::new()));
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            adapter.inventory(&inst).await.expect("inventory");
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

    #[tokio::test]
    async fn test_plan_uninstall_is_a_trash_paths_plan_carrying_the_dialogs_list() {
        // Spec §6.2, §6.6: no command; the paths in execution order, the
        // launcher last, with what the preview saw at each (Ruling 10) --
        // which never reaches the window: the action's JSON is the paths
        // alone; the warnings the dialog lists, in that order; the
        // instance's lock; no password; the removal's own time budget.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("plan-uninstall", trasher).await;

        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list, not a command: {:?}", plan.action);
        };
        let listed = vec![
            home.path().join(".local/share/claude"),
            home.path().join(".claude/downloads"),
            layout.launcher.clone(),
        ];
        assert_eq!(paths, &listed);
        assert_eq!(
            previewed.iter().map(|seen| seen.kind).collect::<Vec<_>>(),
            vec![ItemKind::Dir, ItemKind::Dir, ItemKind::Symlink]
        );
        assert_eq!(
            serde_json::to_value(&plan.action).unwrap(),
            serde_json::json!({ "TrashPaths": { "paths": listed } })
        );
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/share/claude".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.claude/downloads".to_string(),
                    what: RemovedWhat::Cache
                },
                Warning::WillTrash {
                    path: "~/.local/bin/claude".to_string(),
                    what: RemovedWhat::Launcher
                },
                Warning::WillKeep {
                    path: "~/.claude".to_string(),
                    what: KeptWhat::SettingsAndHistory
                },
                Warning::WillKeep {
                    path: "~/.claude.json".to_string(),
                    what: KeptWhat::Settings
                },
            ]
        );
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("standalone-claude".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(
            plan.affected.is_empty(),
            "a non-empty list would disable Uninstall"
        );
        assert_eq!(plan.timeout_secs, removal::TIMEOUT_SECS);
        assert_eq!(plan.request, uninstall());
    }

    #[tokio::test]
    async fn test_plan_uninstall_refuses_before_anything_was_detected() {
        // Unreachable through `Session` (it detects before it plans); the
        // adapter's own answer, a plain `Refused`, for a caller that skips
        // that (spec §3.2).
        let home = TempHome::new("plan-undetected");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter.plan(&inst, &uninstall()).await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_uninstall_refuses_a_path_that_fails_a_check_with_the_reason() {
        // One of the checks failing reaches the dialog as `UninstallUnsafe`
        // (Task 5's kind), with the path abbreviated.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("plan-unsafe", trasher).await;
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();

        match adapter.plan(&inst, &uninstall()).await {
            Err(AdapterError::UninstallUnsafe { path, reason }) => {
                assert_eq!(path, "~/.local/bin/claude");
                assert_eq!(reason, UninstallUnsafeReason::NotWhatInstructionsExpect);
            }
            other => panic!("expected UninstallUnsafe, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_uninstall_on_a_launcher_only_row_lists_the_program_dir_as_already_gone() {
        let home = TempHome::new("plan-launcher-only");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_dir_all(&layout.root).unwrap();
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        assert_eq!(inst.status.notes, vec![InstanceNote::LauncherOnly]);

        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list, not a command: {:?}", plan.action);
        };
        assert_eq!(paths, &vec![layout.launcher.clone()]);
        assert_eq!(previewed.len(), 1, "what the preview saw at the launcher");
        assert_eq!(
            plan.warnings,
            vec![
                Warning::AlreadyGone {
                    path: "~/.local/share/claude".to_string()
                },
                Warning::WillTrash {
                    path: "~/.local/bin/claude".to_string(),
                    what: RemovedWhat::Launcher
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_moves_every_listed_path_in_order_and_logs_each() {
        // Review Focus 1: two paths named `claude`, both moved, both in the
        // Trash.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) =
            full_install("execute-uninstall", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(trasher.calls(), *paths);
        assert!(trasher.bin().join("claude/versions/2.1.281").is_file());
        assert!(trasher.bin().join("downloads").is_dir());
        let link = trasher.bin().join("claude 2");
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.launcher).is_err());
        assert!(home.path().join(".claude.json").is_file(), "settings stay");
        assert!(home
            .path()
            .join(".claude/projects/p/session.jsonl")
            .is_file());
        assert_eq!(
            notes_of(&sink),
            vec![
                LogNote::MovedToTrash {
                    path: "~/.local/share/claude".to_string(),
                    trashed_to: trasher.bin().join("claude").display().to_string(),
                },
                LogNote::MovedToTrash {
                    path: "~/.claude/downloads".to_string(),
                    trashed_to: trasher.bin().join("downloads").display().to_string(),
                },
                LogNote::MovedToTrash {
                    path: "~/.local/bin/claude".to_string(),
                    trashed_to: link.display().to_string(),
                },
            ]
        );
        // Afterwards the tool is gone for `detect`; and the inventory of
        // the row that detect listed refuses, the launcher being gone since
        // then (the refresh after an operation detects again first).
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
        assert!(matches!(
            adapter.inventory(&inst).await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_refuses_when_a_path_changed_since_the_preview() {
        // Review Focus 2: the launcher re-pointed between the preview and
        // the click. Nothing moved; the fault names the path.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("execute-changed", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
        assert!(layout.root.join("versions/2.1.281").is_file());
    }

    #[tokio::test]
    async fn test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview() {
        // Ruling 10 through the adapter: what the preview saw rides in the
        // plan it issued (`PlanAction::TrashPaths.previewed`). Claude Code
        // updating itself between the preview and the click re-points the
        // launcher at a new version inside the root -- every check still
        // passes and the path is the same string -- but it is not the link
        // the user was shown: nothing moves, and the user previews again.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) =
            full_install("execute-self-updated", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
        assert!(layout.root.join("versions/2.1.281").is_file());
    }

    #[tokio::test]
    async fn test_execute_stops_at_a_refused_item_and_leaves_the_launcher() {
        // Review Focus 4, first half: macOS refused the cache directory.
        let trasher = Arc::new(MockTrasher::new());
        trasher.refuse_call(1, "Operation not permitted");
        let (_home, layout, adapter, inst) = full_install("execute-refused", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                summary: "Operation not permitted".to_string()
            }
        );
        assert_eq!(trasher.calls().len(), 2);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(matches!(
            notes_of(&sink)[..],
            [LogNote::MovedToTrash { .. }, LogNote::TrashFailed { .. }]
        ));
    }

    #[tokio::test]
    async fn test_execute_stops_between_items_when_cancelled() {
        // Review Focus 4, second half: Cancel after the first item.
        let token = CancellationToken::new();
        let trasher = Arc::new(MockTrasher::new());
        trasher.cancel_after_call(0, token.clone());
        let (_home, layout, adapter, inst) = full_install("execute-cancel", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, token)
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(trasher.calls().len(), 1);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn test_execute_runs_out_its_budget_between_items_not_a_process() {
        // `Plan.timeout_secs` is the removal's own clock: a spent budget
        // stops the run before the next item, as `Unconfirmed`, and the
        // log line that says so carries that same number of seconds.
        let trasher = Arc::new(MockTrasher::new());
        let (_home, _layout, adapter, inst) = full_install("execute-budget", trasher.clone()).await;
        let plan = Plan {
            timeout_secs: 0,
            ..adapter.plan(&inst, &uninstall()).await.expect("plan")
        };
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(trasher.calls().is_empty());
        assert_eq!(
            notes_of(&sink),
            vec![LogNote::OutOfTime {
                path: "~/.local/share/claude".to_string(),
                seconds: plan.timeout_secs,
            }]
        );
    }

    #[tokio::test]
    async fn test_reconcile_after_uninstall_tells_there_gone_and_cannot_tell_apart() {
        // B's deviation 15, the hand-off, and Ruling 27: after an uninstall
        // only presence is asked. The launcher-only state a stopped run
        // leaves is present -- so `run_operation` reports the stop
        // truthfully (`Cancelled` after a Cancel,
        // `StillInstalledAfterUninstall` after an exit that claimed
        // success) -- while `reconcile` keeps B's strict rule for upgrades;
        // a launcher that is gone is absent; and one Canager cannot look at
        // (its folder unreadable) is neither: an error, which
        // `run_operation` reports as `Unconfirmed`, never as a finished
        // uninstall.
        let trasher = Arc::new(MockTrasher::new());
        let (_home, layout, adapter, inst) = full_install("reconcile-after", trasher).await;
        let key = adapter.artifact_key(&inst);
        std::fs::remove_dir_all(&layout.root).unwrap();

        let still_there = adapter
            .reconcile_after_uninstall(&inst, &key)
            .await
            .expect("a reading");
        assert!(still_there.present);
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));

        let bin = layout.launcher.parent().unwrap().to_path_buf();
        if let Some(_locked) = super::testing::Unreadable::new(&bin) {
            assert!(matches!(
                adapter.reconcile_after_uninstall(&inst, &key).await,
                Err(AdapterError::Parse(_))
            ));
        }

        std::fs::remove_file(&layout.launcher).unwrap();
        let gone = adapter
            .reconcile_after_uninstall(&inst, &key)
            .await
            .expect("a reading");
        assert!(!gone.present);
    }

    #[tokio::test]
    async fn test_detect_lists_nothing_for_a_launcher_that_reaches_the_root_through_another_link() {
        // Ruling 27, at the adapter: B listed this layout (its `realpath`
        // lands in the root); step C does not, because an uninstall that
        // stopped after moving the root would leave the middle link
        // dangling and the launcher reading as not installed. No instance,
        // so no Uninstall to offer; the Unknown page lists the two links.
        let home = TempHome::new("detect-hop-outside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/bin/claude-current", &real);
        home.link(".local/bin/claude", &current);
        let adapter = adapter(Arc::new(MockRunner::new()));

        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }
}
