use crate::adapters::{
    ensure_instance_match, lookup_failure_reason, reconcile_from, run_plan,
    uncheckable_from_inventory, validate_package_name, Adapter, AdapterError, AdapterMeta,
    CheckOptions, CheckOutcome, LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, InstanceStatus,
    ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction, ReadOnlyReason, Reconciled,
    ResourceLock, Scope, SearchHit, Unavailable, UninstallBlocked, UninstallScope, UpdateBlocked,
    UpdateCandidate, UpdateChannel, Warning,
};

/// npm's own package, as `npm ls -g` lists it beside the user's: the npm
/// every other package is updated and uninstalled with, and the program
/// `npm uninstall -g npm` would remove (`UninstallBlocked::SourceProgram`).
const OWN_PACKAGE: &str = "npm";
use crate::protected::{look, Protected};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
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

/// The real check for `NpmAdapter::new`'s `prefix_read_only_fn` default:
/// whether the current user can write where npm actually places global
/// packages under `prefix` (the prefix *root* that `npm prefix -g` reports,
/// e.g. `/opt/homebrew` — **not** the `node_modules` directory itself, despite
/// what an earlier version of this comment claimed). npm writes into
/// `{prefix}/lib/node_modules`. `None` where it can, and otherwise why not.
///
/// When that directory doesn't exist yet (e.g. the first global install on
/// this prefix), npm has to create it, which needs write permission on the
/// nearest *existing* ancestor, not on the missing leaf. So this walks
/// `{prefix}/lib/node_modules` → `{prefix}/lib` → `{prefix}` and tests
/// whichever of those exists first. A prefix owned by another user (e.g. a
/// system-wide npm) is `PrefixNotWritable` for this adapter — see the
/// per-adapter contract table's Notes column.
///
/// Each is looked up one step at a time and never into or through a
/// protected place (`protected::look`): one that is, or leads into, one --
/// a prefix kept in `~/Documents` -- is not looked at, and the prefix is
/// read-only here as `PrefixProtected`: whether it could be written is not
/// known, which is not the same as the account not being able to.
fn real_prefix_read_only(prefix: &Path) -> Option<ReadOnlyReason> {
    let protected = Protected::of_this_process();
    let node_modules = prefix.join("lib").join("node_modules");
    let lib_dir = prefix.join("lib");
    let not_writable = |candidate: &Path| {
        (!look::writable_folder(candidate, &protected)).then_some(ReadOnlyReason::PrefixNotWritable)
    };
    for candidate in [node_modules.as_path(), lib_dir.as_path(), prefix] {
        match look::target(candidate, &protected) {
            Ok(_) => return not_writable(candidate),
            Err(error) if look::is_protected(&error) => {
                return Some(ReadOnlyReason::PrefixProtected)
            }
            // Not there, or not to be looked up: the next one out.
            Err(_) => continue,
        }
    }
    // None of the three exist (npm prefix -g pointed at a prefix that isn't
    // on disk at all) — there is nothing nearer to test than the root itself,
    // and a missing path correctly reports "not writable".
    not_writable(prefix)
}

/// Whether the `npm` in `prefix`'s `bin` is a Homebrew formula's: a link
/// that leads, every link followed, into `<prefix>/Cellar/` -- what
/// `brew link --force node@22` puts there (`UpdateBlocked::UpdatesWithFormula`).
/// The unversioned `node` formula's npm is a copy in
/// `<prefix>/lib/node_modules/npm`, outside the Cellar, and is not; nor is
/// npm's own, after `npm install -g npm`. Read-only: where two links lead,
/// one step at a time and never into or through a protected place
/// (`protected::look`); `false` for anything it cannot tell.
fn real_npm_comes_with_formula(prefix: &Path) -> bool {
    let protected = Protected::of_this_process();
    let Ok(cellar) = look::real_path(&prefix.join("Cellar"), &protected) else {
        return false;
    };
    match look::target(&prefix.join("bin").join(OWN_PACKAGE), &protected) {
        Ok((real, _)) => real.starts_with(&cellar),
        Err(_) => false,
    }
}

/// `NpmAdapter::npm_comes_with_formula_fn` as `NpmAdapter::new` sets it: the
/// real read in every build but this crate's unit tests, where nothing is
/// read unless a test installs a reader.
#[cfg(not(test))]
const DEFAULT_NPM_COMES_WITH_FORMULA_FN: fn(&Path) -> bool = real_npm_comes_with_formula;
#[cfg(test)]
const DEFAULT_NPM_COMES_WITH_FORMULA_FN: fn(&Path) -> bool = |_| false;

/// The sentence an uninstall says under the tool (`UninstallScope::Npm`),
/// for the npm `version` Banager detected (`npm --version`), only when that
/// is 7 or later. npm 7 and later run no script of the package's on
/// `uninstall -g` (npm 10.9.9: arborist's `reify.js:1308-1341` runs scripts
/// only for added and changed packages, and for `-g` it loads only the
/// named one, `reify.js:372-395`; npm 12.0.2 the same), so its settings and
/// data outside its folder stay; npm 6 ran the package's `preuninstall`,
/// `uninstall` and `postuninstall` scripts (npm 10.9.9's
/// `docs/content/using-npm/scripts.md:216-228`), which could do anything.
/// A version Banager could not read, or whose major number does not parse,
/// gets no sentence.
fn uninstall_scope(version: Option<&str>) -> Option<Warning> {
    let major: u64 = version?.trim().split('.').next()?.parse().ok()?;
    (major >= 7).then_some(Warning::UninstallScope {
        what: UninstallScope::Npm,
    })
}

/// The id `NpmAdapter::detect` gives an npm at `exe_path` whose `npm
/// prefix -g` did not answer: built from the executable, since the global
/// prefix every other npm id is built from is what failed. Read by
/// `resume_unanswered_npm` (session/refresh.rs) to recognise that
/// stand-in.
pub(crate) fn unanswered_instance_id(exe_path: &std::path::Path) -> String {
    crate::model::instance_id("npm", Some(&exe_path.display().to_string()))
}

pub struct NpmAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// How to decide whether `inst.prefix` is writable by the current user
    /// (in practice: whether npm can write into `{prefix}/lib/node_modules`,
    /// creating it if needed — see `real_prefix_read_only`), and if not,
    /// why: `None` for writable, gating install/uninstall/upgrade
    /// otherwise. Production always gets `real_prefix_read_only`; tests
    /// inject a fixed answer via the `#[cfg(test)]`-only
    /// `with_prefix_read_only_fn`, mirroring `BrewAdapter::with_euid_fn`.
    /// `detect()` asks the same question to fill `read_only_reason`, which
    /// is why this takes `&Path` (the prefix) rather than a
    /// `&ManagerInstance` that does not exist yet.
    prefix_read_only_fn: fn(&Path) -> Option<ReadOnlyReason>,
    /// How to tell whether the `npm` in a prefix's `bin` is a Homebrew
    /// formula's (`real_npm_comes_with_formula`), whose own update is then
    /// not offered (`UpdateBlocked::UpdatesWithFormula`).
    npm_comes_with_formula_fn: fn(&Path) -> bool,
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
            prefix_read_only_fn: real_prefix_read_only,
            npm_comes_with_formula_fn: DEFAULT_NPM_COMES_WITH_FORMULA_FN,
        }
    }

    #[cfg(test)]
    fn with_prefix_read_only_fn(mut self, f: fn(&Path) -> Option<ReadOnlyReason>) -> NpmAdapter {
        self.prefix_read_only_fn = f;
        self
    }

    /// Test-only hook: whether the prefix's `npm` is a Homebrew formula's
    /// (see `npm_comes_with_formula_fn`).
    #[cfg(test)]
    fn with_npm_comes_with_formula_fn(mut self, f: fn(&Path) -> bool) -> NpmAdapter {
        self.npm_comes_with_formula_fn = f;
        self
    }

    /// `candidates`, npm's own marked `UpdatesWithFormula` where the npm in
    /// `inst`'s prefix is a Homebrew formula's: its update would take that
    /// formula's link away (`UpdateBlocked::UpdatesWithFormula`). The link
    /// is read only when npm's own is among them.
    fn with_formulas_npm_marked(
        &self,
        inst: &ManagerInstance,
        mut candidates: Vec<UpdateCandidate>,
    ) -> Vec<UpdateCandidate> {
        let lists_own = candidates
            .iter()
            .any(|candidate| candidate.key.name == OWN_PACKAGE && candidate.blocked.is_none());
        if lists_own && (self.npm_comes_with_formula_fn)(&inst.prefix) {
            for candidate in candidates
                .iter_mut()
                .filter(|candidate| candidate.key.name == OWN_PACKAGE)
            {
                candidate.blocked = Some(UpdateBlocked::UpdatesWithFormula);
            }
        }
        candidates
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn instance_id_for(&self, prefix: &Path) -> String {
        crate::model::instance_id(&self.meta.id, Some(&prefix.display().to_string()))
    }

    async fn run_npm(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        let mut args = args;
        if args.iter().any(|arg| arg == "-g") {
            args.extend([
                "--prefix".into(),
                inst.prefix.to_string_lossy().into_owned(),
            ]);
        }
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
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
            output_use: OutputUse::Parsed,
        };
        let prefix_output = self
            .runner
            .run(prefix_spec, None, CancellationToken::new())
            .await;
        let prefix = match &prefix_output {
            Ok(o) if o.exit_code == Some(0) => PathBuf::from(o.stdout.trim()),
            // npm is on `PATH` but would not even answer `npm prefix -g`
            // -- a malformed `~/.npmrc` makes every npm command exit
            // non-zero, for instance. Every other adapter that finds its
            // executable but cannot talk to it (brew/cargo/pipx/uv/ollama,
            // all on a failed `--version`) still reports the instance,
            // marked unavailable, so the user is told; returning
            // `Vec::new()` here instead made npm vanish with no notice and
            // no `SourceError` -- the exact vanishing act `refresh()`'s
            // carry-forward exists to prevent, one level below where it
            // looks for it.
            //
            // Every other adapter's id survives this because it never
            // depended on the command that just failed (brew's three
            // candidate paths, cargo's `CARGO_HOME`-or-`~/.cargo`) --  npm
            // has no such fallback; `npm prefix -g`'s answer *is* the only
            // source of its id, and there is no env-level default global
            // prefix to substitute. So this id is built from the resolved
            // executable path instead: stable across rounds of this same
            // failure (`resolve_exe` answers the same way each time), and
            // never collides with a real `npm:{prefix}` id, since a real
            // global prefix never ends in `/bin/npm`. It will simply not
            // match the id a later, successful round produces -- carrying
            // this round's non-existent inventory forward to that one
            // would be wrong access anyway; see `refresh.rs`'s per-instance
            // carry-forward, which is keyed on exactly this id being
            // stable while it recurs and does not promise continuity
            // across a source going from broken to working.
            //
            // Continuity across a source going from working to broken is
            // `refresh_round`'s: it gives this instance back the id the
            // same executable had last round (`resume_unanswered_npm` in
            // session/refresh.rs, which knows this id by
            // `unanswered_instance_id`), so its rows and the updates the
            // user hid stay with it.
            _ => {
                return vec![ManagerInstance {
                    id: unanswered_instance_id(&exe_path),
                    adapter_id: self.meta.id.clone(),
                    exe_path: exe_path.clone(),
                    prefix: exe_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("/")),
                    scope: Scope::User,
                    status: InstanceStatus {
                        unavailable: Some(Unavailable::NotResponding),
                        notes: Vec::new(),
                        // What `npm prefix -g` did: npm's launcher with no
                        // `node` on PATH never starts (finding (1) of the
                        // 2026-10-07 run).
                        no_answer: crate::runner::no_answer::of(&prefix_output),
                    },
                    version: None,
                    answered_at: None,
                    unverified_version: None,
                    read_only_reason: None,
                }];
            }
        };
        let version_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["--version".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
            output_use: OutputUse::Parsed,
        };
        let version_output = self
            .runner
            .run(version_spec, None, CancellationToken::new())
            .await;
        let version = match &version_output {
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
        let read_only_reason = (self.prefix_read_only_fn)(&prefix);
        vec![ManagerInstance {
            id: self.instance_id_for(&prefix),
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
                no_answer: version
                    .is_none()
                    .then(|| crate::runner::no_answer::of(&version_output))
                    .flatten(),
            },
            version,
            answered_at: None,
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
    ) -> Result<CheckOutcome, AdapterError> {
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
        // Exit 0 is the plain answer: whatever is on stdout is the result,
        // and stdout that will not parse is a real parse error.
        if output.exit_code == Some(0) {
            let found = parse_outdated_global(&output.stdout, &inst.id)?;
            return Ok(self.with_formulas_npm_marked(inst, found).into());
        }
        // npm exits 1 whenever it *finds* anything outdated — a result, not
        // a failure (per-adapter contract table). So a non-zero exit that
        // came with findings is that result...
        if let Ok(found) = parse_outdated_global(&output.stdout, &inst.id) {
            if !found.is_empty() {
                return Ok(self.with_formulas_npm_marked(inst, found).into());
            }
        }
        // ...and a non-zero exit with nothing to show for it is a lookup
        // that did not happen: npm exits 1 *because* it found something, so
        // "exit 1 and found nothing" is a contradiction, not good news.
        // Returning the empty list here is what used to tell a user whose
        // registry was unreachable that everything was up to date.
        let failure = LookupFailure::words(
            lookup_failure_reason("npm outdated -g", output.exit_code, &output.stderr),
            &output.stderr,
        );
        let installed = self.inventory(inst).await?;
        Ok(uncheckable_from_inventory(&installed, UpdateChannel::Native, &failure).into())
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
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        // The same refusal the actionability gate in `Session::issue_plan`
        // gives a prefix `detect` already found read-only, because it is the
        // same fact found later: permissions can change between detect and
        // the click. Sent as `NotActionable` rather than a `Refused` string
        // so the front end words it with the very sentence the source's own
        // rows use (`READ_ONLY_DETAIL_KEYS` in src/lib/sources.ts), in the
        // user's language, instead of this adapter's English.
        if let Some(reason) = (self.prefix_read_only_fn)(&inst.prefix) {
            return Err(AdapterError::NotActionable {
                read_only: Some(reason),
                unavailable: None,
            });
        }
        // npm itself (`UninstallBlocked::SourceProgram`): the gate's late
        // twin (`blocked_uninstall` in session/plans.rs refuses the same
        // row from the snapshot), so no preview of `npm uninstall -g npm` is
        // ever built. Its update is planned as any package's.
        if req.kind == OpKind::Uninstall && req.name == OWN_PACKAGE {
            return Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::SourceProgram,
            });
        }
        // npm that a Homebrew formula linked into the prefix updates with
        // it (`UpdateBlocked::UpdatesWithFormula`): the gate's late twin,
        // for a page from before the formula was linked.
        if req.kind == OpKind::Upgrade
            && req.name == OWN_PACKAGE
            && (self.npm_comes_with_formula_fn)(&inst.prefix)
        {
            return Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::UpdatesWithFormula,
            });
        }
        // npm's own lock, and that of a Homebrew at npm's global prefix
        // (y1-keg review): npm that came with a Node from Homebrew writes
        // into Homebrew's prefix -- `npm install -g npm@latest` puts its own
        // `bin/npm` there -- and a `brew upgrade` of that Node unlinks the
        // places it linked and links them again, stopping at any file in
        // the way (`Keg::ConflictError`). With both locks no npm operation
        // runs while a brew one on the same prefix does, so none can land
        // between that unlink and that link. Where no Homebrew lives at the
        // prefix, no other plan takes the second lock.
        let locks = vec![
            ResourceLock(inst.id.clone()),
            ResourceLock(crate::model::instance_id(
                "brew",
                Some(&inst.prefix.display().to_string()),
            )),
        ];
        let warnings = match req.kind {
            OpKind::Uninstall => uninstall_scope(inst.version.as_deref())
                .into_iter()
                .collect(),
            OpKind::Install | OpKind::Upgrade => Vec::new(),
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
        };
        let mut args = match req.kind {
            OpKind::Install => vec!["install".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["uninstall".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Upgrade => vec![
                "install".to_string(),
                "-g".to_string(),
                format!("{}@latest", req.name),
            ],
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
        };
        args.extend([
            "--prefix".into(),
            inst.prefix.to_string_lossy().into_owned(),
        ]);
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args,
                env: self.env_vec(),
            },
            needs_password: false,
            locks,
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
    ) -> Result<CheckOutcome, AdapterError> {
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
/// order is not). npm's own `npm` is listed like any package, and is the
/// one Banager does not offer to uninstall (`UninstallBlocked::
/// SourceProgram`).
pub(crate) fn parse_ls_global(
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
            uninstall_blocked: (name == OWN_PACKAGE).then_some(UninstallBlocked::SourceProgram),
            display_name: name,
            version: dep.version.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            facts: Default::default(),
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(crate::adapters::sanity::artifacts(out))
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
pub(crate) fn parse_outdated_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, crate::adapters::AdapterError> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: HashMap<String, OutdatedEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out = Vec::new();
    for (name, entry) in root {
        if !crate::adapters::sanity::is_name(&entry.latest) {
            continue;
        }
        let key = ArtifactKey {
            instance_id: instance_id.to_string(),
            kind: ArtifactKind::Package,
            name,
        };
        match (
            semver::Version::parse(&entry.latest),
            semver::Version::parse(&entry.current),
        ) {
            (Ok(latest), Ok(current)) if latest.cmp_precedence(&current).is_gt() => {
                out.push(UpdateCandidate {
                    key,
                    current: entry.current,
                    target: entry.latest,
                    channel: UpdateChannel::Native,
                    checkable: true,
                    warnings: Vec::new(),
                    blocked: None,
                    download_bytes: None,
                });
            }
            (Ok(_), Ok(_)) => {}
            _ => out.push(crate::adapters::uncheckable_candidate(
                key,
                entry.current,
                UpdateChannel::Native,
                "could not compare npm versions".to_string(),
            )),
        }
    }
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(crate::adapters::sanity::candidates(out))
}

#[derive(Debug, Deserialize)]
struct SearchEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// Parses `npm search --json --searchlimit 20 {query}`.
pub(crate) fn parse_search(
    json: &str,
    adapter_id: &str,
) -> Result<Vec<SearchHit>, crate::adapters::AdapterError> {
    let entries: Vec<SearchEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    Ok(crate::adapters::sanity::hits(
        entries
            .into_iter()
            .map(|e| SearchHit {
                adapter_id: adapter_id.to_string(),
                kind: ArtifactKind::Package,
                name: e.name,
                description: e.description,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_outdated_requires_strict_semver_increase() {
        for (current, latest, count, checkable) in [
            ("2.0.0-beta.1", "1.9.0", 0, false),
            ("2.0.0", "1.9.0", 0, false),
            ("1.0.0+local", "1.0.0+registry", 0, false),
            ("2.0.0-beta.1", "2.0.0", 1, true),
            ("unknown", "2.0.0", 1, false),
        ] {
            let json = serde_json::json!({"tool": {"current": current, "latest": latest}});
            let rows = super::parse_outdated_global(&json.to_string(), "npm").unwrap();
            assert_eq!(rows.len(), count, "{current} -> {latest}");
            if let Some(row) = rows.first() {
                assert_eq!(row.checkable, checkable);
            }
        }
    }

    use super::*;

    #[tokio::test]
    async fn regression_f01_npm_global_commands_pin_the_confirmed_prefix() {
        let runner = Arc::new(MockRunner::new());
        let adapter = NpmAdapter::new(runner.clone()).with_prefix_read_only_fn(|_| None);
        let mut inst = test_instance();
        inst.prefix = PathBuf::from("/tmp/confirmed prefix");
        let _ = adapter.inventory(&inst).await;
        let _ = adapter.check_updates(&inst, &CheckOptions::default()).await;
        for call in runner.calls() {
            assert!(
                call.windows(2)
                    .any(|w| w == ["--prefix", "/tmp/confirmed prefix"]),
                "{call:?}"
            );
        }
        for kind in [OpKind::Install, OpKind::Upgrade, OpKind::Uninstall] {
            let plan = adapter
                .plan(
                    &inst,
                    &OpRequest {
                        kind,
                        instance_id: inst.id.clone(),
                        artifact_kind: ArtifactKind::Package,
                        name: "demo".into(),
                    },
                )
                .await
                .unwrap();
            assert!(command_args(&plan)
                .windows(2)
                .any(|w| w == ["--prefix", "/tmp/confirmed prefix"]));
            let mut argv = vec![inst.exe_path.to_str().unwrap()];
            let args = command_args(&plan);
            argv.extend(args.iter().map(String::as_str));
            runner.respond(
                argv,
                CommandOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    stderr_cause: Default::default(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            assert_eq!(
                adapter
                    .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                    .await
                    .unwrap(),
                Outcome::Succeeded
            );
            assert!(runner
                .calls()
                .last()
                .unwrap()
                .windows(2)
                .any(|w| w == ["--prefix", "/tmp/confirmed prefix"]));
        }
    }

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_parse_ls_global_reads_a_version_with_a_control_character_as_unknown() {
        let json = r#"{"dependencies":{"a":{"version":"1.0\n"},"":{"version":"1"}}}"#;
        let artifacts = parse_ls_global(json, "npm:/opt/homebrew/bin/npm").unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "a");
        assert_eq!(artifacts[0].version, "");
    }

    #[test]
    fn regression_parse_outdated_global_drops_an_update_to_nothing() {
        let json = r#"{"a":{"current":"1","latest":""},"b":{"current":"1\u0000","latest":"2"},
            "c":{"current":"1","latest":"2"}}"#;
        let names: Vec<String> = parse_outdated_global(json, "npm:/opt/homebrew/bin/npm")
            .unwrap()
            .into_iter()
            .map(|c| c.key.name)
            .collect();
        assert_eq!(names, vec!["c".to_string()]);
    }

    #[test]
    fn regression_parse_search_drops_a_hit_with_no_name() {
        let json = r#"[{"name":""},{"name":"jq\u001b"},{"name":"node-jq"}]"#;
        let hits = parse_search(json, "npm").unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["node-jq"]);
    }
    use crate::testing::command_args;

    #[test]
    fn parse_ls_global_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        let artifacts = parse_ls_global(&json, "npm:/opt/homebrew").expect("parse");
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
        // npm itself is not offered for uninstalling; every other package,
        // `corepack` among them, is.
        assert_eq!(
            npm_self.uninstall_blocked,
            Some(UninstallBlocked::SourceProgram)
        );
        let blocked: Vec<&str> = artifacts
            .iter()
            .filter(|a| a.uninstall_blocked.is_some())
            .map(|a| a.key.name.as_str())
            .collect();
        assert_eq!(blocked, vec!["npm"]);
    }

    #[test]
    fn parse_outdated_global_matches_the_recorded_fixture() {
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read adapters/fixtures/npm/12.0.2/outdated-global.json");
        let candidates = parse_outdated_global(&json, "npm:/opt/homebrew").expect("parse");
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
        let candidates = parse_outdated_global("", "npm:/opt/homebrew").expect("parse");
        assert!(candidates.is_empty());
        assert!(parse_outdated_global("{}", "npm:/opt/homebrew")
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
        ArtifactKey, CancelPolicy, OpKind, OpRequest, Outcome, ResourceLock, Warning,
    };
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/npm"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("12.0.2".to_string()),
            ..crate::testing::manager_instance("npm", "npm:/opt/homebrew")
        }
    }

    fn fake_exe(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").expect("write fake npm executable");
        path
    }

    #[tokio::test]
    async fn test_detect_finds_npm_on_path_and_resolves_its_global_prefix() {
        let dir = crate::testing::unique_temp_path("npm-detect");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "12.0.2\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        // The writability answer is pinned like every other detect test
        // below: `/opt/homebrew` is what this mock `npm prefix -g` reports,
        // not a directory the machine running this is expected to have, and
        // asking the real filesystem about it would make the answer depend
        // on which Mac this is.
        let adapter = NpmAdapter::new(runner).with_prefix_read_only_fn(|_| None);
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
        assert_eq!(instances[0].id, "npm:/opt/homebrew");
        assert_eq!(instances[0].version, Some("12.0.2".to_string()));
        assert!(instances[0].available());
        assert!(
            instances[0].unverified_version.is_none(),
            "12.0.2 is verified in adapters/meta/npm.toml"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version() {
        let dir = crate::testing::unique_temp_path("npm-detect-unverified");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner).with_prefix_read_only_fn(|_| None);
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
        assert_eq!(instances[0].unverified_version, Some("99.9.9".to_string()));
    }

    /// A temp directory holding a fake `npm` executable, plus a `HostEnv`
    /// whose `PATH` is exactly that directory. Returned rather than
    /// inlined because every detect test needs the same three lines and
    /// the unique-name dance around them.
    fn detect_fixture(tag: &str) -> (PathBuf, PathBuf, HostEnv) {
        let dir = std::env::temp_dir().join(format!(
            "banager-npm-detect-{}-{}-{}",
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
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        (dir, npm_path, env)
    }

    /// `/opt/homebrew` here is what this *mock* `npm prefix -g` answers --
    /// a realistic prefix to read, not a claim about the machine running
    /// the test. Nothing in these detect tests touches a real one: the exe
    /// lives in a temp directory and the writability probe is injected.
    fn detect_runner(npm_path: &std::path::Path) -> Arc<MockRunner> {
        let npm_path_str = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
        // The instance must still be detected -- Banager lists what is
        // there -- and must carry the reason it cannot be changed, so the
        // Updates page can say "install Node with Homebrew instead"
        // rather than pip's "use pipx or uv".
        let (dir, npm_path, env) = detect_fixture("readonly");
        let adapter = NpmAdapter::new(detect_runner(&npm_path))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixNotWritable));
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
    async fn test_detect_reports_a_prefix_in_a_protected_place_as_not_looked_into() {
        // `npm config set prefix ~/Documents/npm-global`: the folder may
        // well be the user's to write, but Banager never looks into a
        // protected place, so it cannot say. The reason says that, not
        // that the account cannot change it (decision I23).
        let (dir, npm_path, env) = detect_fixture("protected");
        let adapter = NpmAdapter::new(detect_runner(&npm_path))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixProtected));
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::PrefixProtected)
        );
        assert!(!instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_leaves_the_instance_writable_when_the_prefix_is_writable() {
        let (dir, npm_path, env) = detect_fixture("writable");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_read_only_fn(|_| None);
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
        fn record(path: &Path) -> Option<ReadOnlyReason> {
            ASKED.lock().unwrap().push(path.to_path_buf());
            None
        }
        let (dir, npm_path, env) = detect_fixture("prefix-arg");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_read_only_fn(record);
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
            rustup_home: None,
            zdotdir: None,
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
    async fn test_detect_says_why_npm_did_not_answer() {
        // Finding (1) of the 2026-10-07 run: `brew upgrade node@22` left no
        // `node` on PATH, npm's launcher (`#!/usr/bin/env node`) could not
        // start, and the window said only 「npm没有响应」. What the command
        // did is the reason: it never ran, for want of `node`. And when the
        // prefix answers but `--version` does not, that command's reason.
        use crate::model::{NoAnswer, NoAnswerKind};
        let env_said = |exit_code| CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(exit_code),
            stdout: String::new(),
            stderr: "env: node: No such file or directory\n".to_string(),
            timed_out: false,
            cancelled: false,
        };
        let (dir, npm_path, env) = detect_fixture("no-node");
        let npm = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![npm, "prefix", "-g"], env_said(127));
        let instances = NpmAdapter::new(runner).detect(&env).await;
        assert_eq!(
            instances[0].status.no_answer,
            Some(NoAnswer {
                kind: NoAnswerKind::CouldNotStart,
                missing_program: Some("node".to_string()),
                link_fixes: Vec::new(),
            })
        );

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm, "prefix", "-g"],
            CommandOutput {
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                ..env_said(0)
            },
        );
        runner.respond(
            vec![npm, "--version"],
            CommandOutput {
                timed_out: true,
                exit_code: None,
                ..env_said(0)
            },
        );
        let instances = NpmAdapter::new(runner)
            .with_prefix_read_only_fn(|_| None)
            .detect(&env)
            .await;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert_eq!(
            instances[0].status.no_answer.as_ref().map(|why| why.kind),
            Some(NoAnswerKind::TimedOut)
        );
    }

    #[tokio::test]
    async fn test_detect_reports_unavailable_instead_of_vanishing_when_npm_prefix_fails() {
        // A malformed ~/.npmrc makes every npm command exit non-zero: npm
        // itself is on PATH, but `npm prefix -g` -- the very command this
        // adapter's id comes from -- cannot answer. Returning `Vec::new()`
        // here used to make npm disappear with no unavailable notice and
        // no SourceError, indistinguishable from npm not being installed
        // at all. brew/cargo/pipx/uv/ollama all report an instance marked
        // unavailable on an equivalent failure (their own `--version`);
        // npm must too.
        let (dir, npm_path, env) = detect_fixture("prefix-fails");
        let npm_path_str = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "npm error config Invalid npmrc".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner.clone());
        let instances = adapter.detect(&env).await;
        // Stable across repeated failures of this same shape: calling
        // detect again while npm is still broken must not fabricate a new
        // id each time, or refresh()'s carry-forward (keyed on the
        // instance id recurring) could never accumulate anything for it.
        let instances_again = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert!(!instances[0].available());
        assert_eq!(instances[0].version, None);
        assert_eq!(instances_again.len(), 1);
        assert_eq!(instances[0].id, instances_again[0].id);
        // And it must not collide with a real `npm:{prefix}` id: no real
        // global prefix ends in `/bin/npm`.
        assert_eq!(instances[0].id, format!("npm:{}", npm_path.display()));
        assert_eq!(
            runner.calls().len(),
            2,
            "must not run --version after prefix already failed to answer, across both detect() calls"
        );
    }

    #[tokio::test]
    async fn test_inventory_accepts_exit_code_1() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
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
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
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
            .expect("exit 1 means updates were found, not a failure")
            .candidates;
        assert_eq!(candidates.len(), 1);
    }

    /// A prefix as the author's was before 2026-10-07: `node@22` linked by
    /// hand, so `<prefix>/bin/npm` is Homebrew's link into the keg's own
    /// npm. `own` false: npm's own copy, as after `npm install -g npm`.
    fn prefix_with_npm(tag: &str, own: bool) -> PathBuf {
        use std::os::unix::fs::symlink;
        let root = crate::testing::unique_temp_path(&format!("npm-formula-{tag}"));
        let keg = root.join("Cellar/node@22/22.23.3_1");
        std::fs::create_dir_all(keg.join("lib/node_modules/npm/bin")).unwrap();
        std::fs::write(keg.join("lib/node_modules/npm/bin/npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(keg.join("bin")).unwrap();
        symlink(
            "../lib/node_modules/npm/bin/npm-cli.js",
            keg.join("bin/npm"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("lib/node_modules/npm/bin")).unwrap();
        std::fs::write(root.join("lib/node_modules/npm/bin/npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let target = if own {
            "../lib/node_modules/npm/bin/npm-cli.js"
        } else {
            "../Cellar/node@22/22.23.3_1/bin/npm"
        };
        symlink(target, root.join("bin/npm")).unwrap();
        root
    }

    #[test]
    fn test_an_npm_a_homebrew_formula_linked_is_that_formulas() {
        // Finding (2) of the y2-npmwhy review: `npm install -g npm@latest`
        // in a prefix whose `bin/npm` is Homebrew's link into `node@22`'s
        // keg replaces that link, and the next `brew upgrade node@22`
        // cannot link the new version over it.
        let formulas = prefix_with_npm("formulas", false);
        let own = prefix_with_npm("own", true);
        let elsewhere = crate::testing::unique_temp_path("npm-formula-none");
        let found = (
            real_npm_comes_with_formula(&formulas),
            real_npm_comes_with_formula(&own),
            real_npm_comes_with_formula(&elsewhere),
        );
        let _ = std::fs::remove_dir_all(&formulas);
        let _ = std::fs::remove_dir_all(&own);
        assert_eq!(found, (true, false, false));
    }

    #[tokio::test]
    async fn test_npm_that_comes_with_a_homebrew_formula_is_not_offered_its_own_update() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{
            "npm": {"current": "10.9.9", "wanted": "10.9.9", "latest": "12.2.0", "dependent": "global", "location": "/opt/homebrew/lib/node_modules/npm"},
            "prettier": {"current": "3.8.1", "wanted": "3.8.2", "latest": "3.8.2", "dependent": "global", "location": "/opt/homebrew/lib/node_modules/prettier"}
        }"#;
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        async fn blocked(adapter: &NpmAdapter) -> Vec<(String, Option<UpdateBlocked>)> {
            let mut found: Vec<(String, Option<UpdateBlocked>)> = adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .expect("checked")
                .candidates
                .into_iter()
                .map(|candidate| (candidate.key.name, candidate.blocked))
                .collect();
            found.sort_by(|a, b| a.0.cmp(&b.0));
            found
        }
        let formulas = NpmAdapter::new(runner.clone()).with_npm_comes_with_formula_fn(|_| true);
        assert_eq!(
            blocked(&formulas).await,
            vec![
                ("npm".to_string(), Some(UpdateBlocked::UpdatesWithFormula)),
                ("prettier".to_string(), None),
            ]
        );
        let own = NpmAdapter::new(runner).with_npm_comes_with_formula_fn(|_| false);
        assert_eq!(
            blocked(&own).await,
            vec![("npm".to_string(), None), ("prettier".to_string(), None)]
        );

        // The gate's late twin: planned from a page that is out of date.
        let inst = test_instance();
        let upgrade = |name: &str| OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: name.to_string(),
        };
        let formulas = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| None)
            .with_npm_comes_with_formula_fn(|_| true);
        match formulas.plan(&inst, &upgrade("npm")).await {
            Err(AdapterError::UpdateBlocked { reason }) => {
                assert_eq!(reason, UpdateBlocked::UpdatesWithFormula)
            }
            other => panic!("expected UpdateBlocked, got {other:?}"),
        }
        formulas
            .plan(&inst, &upgrade("prettier"))
            .await
            .expect("every other package is updated as before");
    }

    #[tokio::test]
    async fn test_check_updates_does_not_call_a_failed_registry_lookup_up_to_date() {
        // `npm outdated -g --json` exits 1 *when it finds updates*, which is
        // why exit 1 is accepted at all. A registry that could not be
        // reached also exits non-zero -- with nothing on stdout and the
        // reason on stderr -- and reading that as an empty result told the
        // user "Everything is up to date" about packages Banager had not
        // managed to ask about. It is the one adapter whose failure mode was
        // indistinguishable from good news.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "outdated", "-g", "--json", "--prefix", "/opt/homebrew"],
            CommandOutput { stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "npm error code ENOTFOUND\nnpm error network request to https://registry.npmjs.org failed".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let ls = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: ls,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a registry that did not answer is not a source failure")
            .candidates;
        assert_eq!(
            candidates.len(),
            6,
            "one row per installed package, so the page cannot claim to know"
        );
        assert!(candidates.iter().all(|c| !c.checkable));
        let reason = candidates[0]
            .warnings
            .iter()
            .find_map(|w| match w {
                Warning::Message(m) => Some(m.clone()),
                _ => None,
            })
            .expect("the row carries why");
        assert!(
            reason.contains("ENOTFOUND"),
            "the reason the user reads has to be npm's own: {reason}"
        );
        // npm's words name the network: every row says a later check can
        // get past it, so the lists count them as not checked this time.
        assert!(candidates
            .iter()
            .all(|c| c.warnings.contains(&Warning::TransientLookupFailure)));
    }

    #[tokio::test]
    async fn test_check_updates_still_errors_when_npm_itself_could_not_be_run() {
        // The other half of the same rule: `Err` is reserved for "the tool
        // could not be run at all". Nothing is mocked, so the runner reports
        // no such command -- that must not turn into a page full of
        // "can't check" rows built from an inventory nobody could read.
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()));
        let result = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await;
        assert!(result.is_err());
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
                stderr_cause: Default::default(),
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
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            command_args(&plan),
            vec!["install", "-g", "jq", "--prefix", "/opt/homebrew"]
        );
        assert!(!plan.needs_password);
        assert_eq!(plan.locks[0], ResourceLock(inst.id.clone()));
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    /// y1-keg review: npm's global prefix is Homebrew's own when npm came
    /// with a Node from Homebrew, and `npm install -g npm@latest` there
    /// rewrites `bin/npm` -- the very place `brew upgrade node@22` unlinks
    /// and links again. Each npm operation takes the lock of a Homebrew at
    /// that prefix too, so the two never run at the same time.
    #[tokio::test]
    async fn test_every_plan_takes_the_lock_of_a_homebrew_at_its_prefix() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = ManagerInstance {
            prefix: PathBuf::from("/opt/homebrew"),
            ..test_instance()
        };
        for kind in [OpKind::Install, OpKind::Upgrade, OpKind::Uninstall] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "typescript".to_string(),
            };
            let plan = adapter.plan(&inst, &req).await.expect("plan");
            assert_eq!(
                plan.locks,
                vec![
                    ResourceLock(inst.id.clone()),
                    ResourceLock("brew:/opt/homebrew".to_string()),
                ],
                "{kind:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_plan_uninstall() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            command_args(&plan),
            vec!["uninstall", "-g", "jq", "--prefix", "/opt/homebrew"]
        );
    }

    #[tokio::test]
    async fn test_npm_itself_is_never_planned_for_uninstalling_but_is_updated_as_any_package() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let request = |kind: OpKind, name: &str| OpRequest {
            kind,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: name.to_string(),
        };
        match adapter
            .plan(&inst, &request(OpKind::Uninstall, "npm"))
            .await
        {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::SourceProgram)
            }
            other => panic!("expected UninstallBlocked(SourceProgram), got {other:?}"),
        }
        let upgrade = adapter
            .plan(&inst, &request(OpKind::Upgrade, "npm"))
            .await
            .expect("an update of npm is planned");
        assert_eq!(
            command_args(&upgrade),
            vec!["install", "-g", "npm@latest", "--prefix", "/opt/homebrew"]
        );
        // Nor is a package merely named like it, or npm's other bundled one.
        for name in ["npm-check-updates", "@scope/npm", "corepack"] {
            let plan = adapter
                .plan(&inst, &request(OpKind::Uninstall, name))
                .await
                .expect("planned");
            assert_eq!(
                command_args(&plan),
                vec!["uninstall", "-g", name, "--prefix", "/opt/homebrew"]
            );
        }
        // A prefix that cannot be written is the bigger news.
        let read_only = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixNotWritable));
        assert!(matches!(
            read_only
                .plan(&inst, &request(OpKind::Uninstall, "npm"))
                .await,
            Err(AdapterError::NotActionable { .. })
        ));
    }

    fn uninstall_jq(inst: &ManagerInstance) -> OpRequest {
        OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        }
    }

    #[tokio::test]
    async fn test_an_uninstall_by_npm_7_or_later_says_what_goes_and_what_stays() {
        // npm 7 and later run none of a package's scripts on `uninstall -g`,
        // so its settings and data outside its folder stay. The version is
        // the one `detect` read from `npm --version`.
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        for version in ["7.0.0", "10.9.9", "12.0.2", " 12.0.2\n"] {
            let inst = ManagerInstance {
                version: Some(version.to_string()),
                ..test_instance()
            };
            let plan = adapter
                .plan(&inst, &uninstall_jq(&inst))
                .await
                .expect("plan");
            assert_eq!(
                plan.warnings,
                vec![Warning::UninstallScope {
                    what: UninstallScope::Npm
                }],
                "{version:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_an_uninstall_by_npm_6_or_of_an_unread_version_says_nothing_of_what_stays() {
        // npm 6 ran the package's `preuninstall`, `uninstall` and
        // `postuninstall` scripts, which could do anything; a version
        // Banager could not read might be that.
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        for version in [
            Some("6.14.18"),
            Some("5.6.0"),
            Some(""),
            Some("v-next"),
            None,
        ] {
            let inst = ManagerInstance {
                version: version.map(str::to_string),
                ..test_instance()
            };
            let plan = adapter
                .plan(&inst, &uninstall_jq(&inst))
                .await
                .expect("plan");
            assert!(plan.warnings.is_empty(), "{version:?}: {:?}", plan.warnings);
        }
    }

    #[tokio::test]
    async fn test_no_install_or_upgrade_plan_says_what_an_uninstall_would() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        for kind in [OpKind::Install, OpKind::Upgrade] {
            let req = OpRequest {
                kind,
                ..uninstall_jq(&inst)
            };
            let plan = adapter.plan(&inst, &req).await.expect("plan");
            assert!(plan.warnings.is_empty(), "{kind:?}: {:?}", plan.warnings);
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_targets_latest() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            command_args(&plan),
            vec!["install", "-g", "jq@latest", "--prefix", "/opt/homebrew"]
        );
    }

    #[tokio::test]
    async fn test_plan_is_refused_when_the_prefix_is_not_writable() {
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixNotWritable));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let result = adapter.plan(&inst, &req).await;
        match result {
            Err(AdapterError::NotActionable {
                read_only: Some(ReadOnlyReason::PrefixNotWritable),
                unavailable: None,
            }) => {}
            other => panic!("expected NotActionable(PrefixNotWritable), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_is_refused_with_the_protected_place_as_its_reason() {
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixProtected));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        match adapter.plan(&inst, &req).await {
            Err(AdapterError::NotActionable {
                read_only: Some(ReadOnlyReason::PrefixProtected),
                unavailable: None,
            }) => {}
            other => panic!("expected NotActionable(PrefixProtected), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "install",
                "-g",
                "jq",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "added 1 package\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner).with_prefix_read_only_fn(|_| None);
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
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
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

    #[tokio::test]
    async fn test_reconcile_ignores_an_artifact_of_a_different_kind() {
        // Every npm artifact is a `Package` today, so this can only fail
        // if npm's match rule stops looking at `kind` at all -- which is
        // exactly the drift `reconcile_from` exists to prevent.
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let inst = test_instance();
        let wrong_kind = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Tool,
                    name: "npm".to_string(),
                },
            )
            .await
            .expect("reconcile wrong kind");
        assert!(
            !wrong_kind.present,
            "a key of another kind must not match an installed package by name alone"
        );
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
            "banager-npm-prefix-writable-{}-{}-{}",
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
    fn real_prefix_read_only_tests_node_modules_itself_not_just_the_prefix_root() {
        // `{prefix}/lib/node_modules` exists but is not writable, even though
        // the prefix root (which we just created and own) is. npm writes
        // into node_modules directly here, so this must report false — a
        // regression to "test prefix root only" would wrongly report true.
        let prefix = scratch_dir("existing-node-modules-read-only");
        let node_modules = prefix.join("lib").join("node_modules");
        std::fs::create_dir_all(&node_modules).expect("create node_modules");
        make_read_only(&node_modules);

        assert_eq!(
            real_prefix_read_only(&prefix),
            Some(ReadOnlyReason::PrefixNotWritable),
            "node_modules itself is read-only, so npm cannot write packages into it"
        );

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_read_only_walks_up_to_lib_when_node_modules_does_not_exist_yet() {
        // First global install on this prefix: node_modules hasn't been
        // created yet, but its parent (`lib`) exists and is writable, so npm
        // can create node_modules when it needs to.
        let prefix = scratch_dir("missing-node-modules-writable-lib");
        let lib_dir = prefix.join("lib");
        std::fs::create_dir_all(&lib_dir).expect("create lib");

        assert_eq!(real_prefix_read_only(&prefix), None);

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_read_only_walks_up_to_lib_and_finds_it_unwritable() {
        // Same as above, but `lib` itself cannot be written to, so npm could
        // not create node_modules inside it even though the prefix root can
        // be written to.
        let prefix = scratch_dir("missing-node-modules-read-only-lib");
        let lib_dir = prefix.join("lib");
        std::fs::create_dir_all(&lib_dir).expect("create lib");
        make_read_only(&lib_dir);

        assert_eq!(
            real_prefix_read_only(&prefix),
            Some(ReadOnlyReason::PrefixNotWritable)
        );

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_read_only_never_looks_into_a_protected_place_and_says_so() {
        // A prefix kept in `~/Documents`, by its own path or through a
        // link (`npm config set prefix ~/.npm-global`, that folder synced
        // into Documents): never looked at, so read-only here, though it
        // could be written -- and the reason is that it was not looked
        // into, not that the account cannot change it (decision I23).
        let home = std::fs::canonicalize(scratch_dir("prefix-in-documents")).unwrap();
        let kept = home.join("Documents/npm-global");
        std::fs::create_dir_all(kept.join("lib/node_modules")).expect("create prefix");
        let linked = home.join(".npm-global");
        std::os::unix::fs::symlink(&kept, &linked).expect("link");
        assert_eq!(
            real_prefix_read_only(&linked),
            None,
            "writable where nothing is protected"
        );
        let as_if = crate::protected::as_if_home(&home);
        assert_eq!(
            real_prefix_read_only(&kept),
            Some(ReadOnlyReason::PrefixProtected)
        );
        assert_eq!(
            real_prefix_read_only(&linked),
            Some(ReadOnlyReason::PrefixProtected)
        );
        drop(as_if);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn real_prefix_read_only_falls_back_to_the_prefix_root_when_lib_is_also_missing() {
        // Neither `lib` nor `lib/node_modules` exist yet; the nearest
        // existing ancestor is the prefix root itself.
        let prefix = scratch_dir("missing-lib-entirely");

        assert_eq!(real_prefix_read_only(&prefix), None);

        let _ = std::fs::remove_dir_all(&prefix);
    }
}
