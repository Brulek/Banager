use crate::adapters::{
    lookup_failure_reason, reconcile_from, second_token, uncheckable_candidate, Adapter,
    AdapterError, AdapterMeta, CheckOptions, CheckOutcome, LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceStatus, ManagerInstance,
    OpRequest, Outcome, Plan, ReadOnlyReason, Reconciled, Scope, SearchHit, Unavailable,
    UpdateCandidate, UpdateChannel,
};
use crate::protected::{self, look, Protected};
use crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
pub(crate) struct PipPackage {
    pub(crate) name: String,
    pub(crate) version: String,
}

/// Parses `pip list --format=json`. A package with no usable name is
/// left out and a version that is not text is unknown
/// (`adapters::sanity`).
pub(crate) fn parse_pip_list(json: &str) -> Result<Vec<PipPackage>, AdapterError> {
    let mut packages: Vec<PipPackage> =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    packages.retain(|p| crate::adapters::sanity::is_name(&p.name));
    for p in &mut packages {
        if !crate::adapters::sanity::is_version(&p.version) {
            p.version.clear();
        }
    }
    Ok(packages)
}

#[derive(Debug, Deserialize)]
struct PipOutdatedPackage {
    name: String,
    version: String,
    latest_version: String,
}

pub(crate) fn parse_pip_outdated(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, AdapterError> {
    let items: Vec<PipOutdatedPackage> =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(crate::adapters::sanity::candidates(
        items
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
                download_bytes: None,
            })
            .collect(),
    ))
}

/// A project's name as pip asks its index for it (PEP 503, pip's
/// `canonicalize_name`): lower case, each run of `-`, `_` and `.` one `-`.
/// `pip list` prints a package as it was published (`PyYAML`,
/// `typing_extensions`, `zope.interface`), the index's address has it as
/// `pyyaml`, `typing-extensions`, `zope-interface`.
fn canonical_project(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_run = false;
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !in_run {
                out.push('-');
            }
            in_run = true;
        } else {
            out.extend(c.to_lowercase());
            in_run = false;
        }
    }
    out
}

/// `text` without the `<... object at 0x...>` Python puts in an error's
/// repr for the connection it failed on, and the `: ` or `, ` after it:
/// `NameResolutionError("<pip._vendor.urllib3.connection.HTTPSConnection
/// object at 0x1048a5e50>: Failed to resolve 'pypi.org' (...)")` becomes
/// `NameResolutionError("Failed to resolve 'pypi.org' (...)")`, so the
/// words that say what failed fit in a row's reason
/// (`lookup_failure_reason` keeps 200 characters).
fn without_object_reprs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let end = start + len + 1;
        if rest[start..end].contains(" object at 0x") {
            out.push_str(&rest[..start]);
            let after = &rest[end..];
            rest = after
                .strip_prefix(": ")
                .or_else(|| after.strip_prefix(", "))
                .unwrap_or(after);
        } else {
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out
}

/// One lookup pip all but gave up on, read off the warning urllib3 prints
/// before its final try:
///
/// ```text
/// WARNING: Retrying (Retry(total=0, connect=None, read=None, redirect=None, status=None)) after connection broken by '<error>': /simple/<project>/
/// ```
///
/// urllib3 (pip 26.2.1 vendors 2.7.0) warns
/// `"Retrying (%r) after connection broken by '%r': %s"` with what is
/// left of the `Retry` each time a request fails to connect or breaks off
/// (`connectionpool.py`, `urlopen`), and pip shows it on stderr as
/// `WARNING: ` (`utils/logging.py`; urllib3's logger stays at `WARNING`,
/// one line, `soft_wrap`). `total=0` comes after the fifth failure in a
/// row -- pip retries 5 times unless told otherwise (`--retries`,
/// `network/session.py`) -- and before one final try. If that try fails
/// too, urllib3 gives up with no further warning (`Retry.increment`
/// raises once `total` goes below 0), pip's collector drops the page with
/// a debug message only ("Could not fetch URL ... - skipping",
/// `index/collector.py`), finds no candidates, and `pip list --outdated`
/// leaves the project out and exits 0 (`commands/list.py`): stdout alone
/// reads as "up to date". If that final try answers, nothing on stderr
/// says so, and an up-to-date project is still taken as not checked --
/// the cautious side, after five failures in a row: its row is transient
/// (`TransientLookupFailure`, where the words name the network) and the
/// next check that reaches the index clears it. `project` is the last
/// segment of the address it was fetching -- the project, for an index's
/// `/simple/<project>/` -- and `words` the error, without the connection's
/// `<... object at 0x...>`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GaveUp {
    project: String,
    words: String,
}

/// Each lookup pip gave up on, or came to its final try of, in `stderr`'s
/// order (`GaveUp`): the lines that are urllib3's retry warning with
/// `total=0`. A retry warning with any other total is followed by another
/// warning if the next try fails too, so without one of `total=0` after it
/// the lookup was answered, and it is not one.
fn lookups_given_up(stderr: &str) -> Vec<GaveUp> {
    const RETRY: &str = "Retrying (Retry(total=";
    const BROKEN: &str = "after connection broken by '";
    stderr
        .lines()
        .filter_map(|line| {
            let total = &line[line.find(RETRY)? + RETRY.len()..];
            if total.split(',').next()?.trim() != "0" {
                return None;
            }
            let error_start = line.find(BROKEN)? + BROKEN.len();
            let error_end = line.rfind("': ")?;
            if error_end < error_start {
                return None;
            }
            let address = line[error_end + 3..].trim();
            let project = address
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("");
            Some(GaveUp {
                project: canonical_project(project),
                words: without_object_reprs(&line[error_start..error_end]),
            })
        })
        .collect()
}

/// The lookups pip says it could not fetch, from its `Could not fetch URL
/// <address>: <reason> - skipping` lines (pip 26.2.1
/// `index/collector.py`, `_handle_get_simple_fail`). At pip's normal
/// verbosity -- the one `check_updates` pins with `OUTDATED_ENV` -- only
/// a certificate failure is said this way: pip logs it at INFO, so it is
/// printed on stdout ahead of the JSON. Every other failure is logged at
/// DEBUG and printed only at `-vv`, which Banager does not pass: its
/// output grows by a line per file pip skips, thousands for one project
/// (opus-int finding 4).
///
/// An HTTP answer that says the index does not have the project is no
/// failed lookup (`index_lacks_project`), whatever the verbosity: with an
/// extra index, pip asks every index about every project, and the one
/// that does not host it answers 404 -- or 403, as PyTorch's
/// `download.pytorch.org` does -- while another index answers with the
/// project (opus-int finding 3). Keep only the project and a fixed
/// reason, never an index URL's credentials.
fn final_fetch_failures(text: &str) -> Vec<GaveUp> {
    text.lines()
        .filter_map(|line| {
            let (_, tail) = line.split_once("Could not fetch URL ")?;
            let (address, reason) = tail.split_once(": ")?;
            if index_lacks_project(reason) {
                return None;
            }
            let project = url::Url::parse(address)
                .ok()
                .and_then(|u| {
                    u.path()
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .map(canonical_project)
                })
                .unwrap_or_default();
            Some(GaveUp {
                project,
                words: if super::says_network_failed(reason) {
                    "network error: pip could not fetch the package index"
                } else {
                    "pip could not fetch the package index"
                }
                .into(),
            })
        })
        .collect()
}

/// Whether the reason of a `Could not fetch URL` line is an index's answer
/// that it has no such project: pip's own wording of a 403, 404 or 410
/// status (`network/utils.py`, `raise_for_status`: `"<status> Client
/// Error: <reason> for url: <url>"`). A 401 (credentials needed), a 407
/// (proxy credentials) and a 5xx stay failures.
fn index_lacks_project(reason: &str) -> bool {
    ["403 Client Error", "404 Client Error", "410 Client Error"]
        .iter()
        .any(|answer| reason.trim_start().starts_with(answer))
}

/// The JSON list `pip list --outdated --format=json` prints on stdout,
/// also when INFO lines precede it there: a certificate failure's `Could
/// not fetch URL` line comes first (`final_fetch_failures`), and a pip run
/// with more verbosity prints its diagnostics before the one JSON line.
/// A plain or pretty-printed result alone is read as a whole.
fn parse_outdated_stdout(
    stdout: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, AdapterError> {
    if let Ok(rows) = parse_pip_outdated(stdout, instance_id) {
        return Ok(rows);
    }
    for line in stdout.lines().rev() {
        if line.trim_start().starts_with('[') {
            if let Ok(rows) = parse_pip_outdated(line, instance_id) {
                return Ok(rows);
            }
        }
    }
    Err(AdapterError::Parse(
        "pip did not return its outdated JSON list".into(),
    ))
}

/// Whether `stderr` is Python's own answer to `-m pip` when it has no
/// module named `pip`: `<python>: No module named pip`, alone on its line.
/// Not `No module named pip.__main__; 'pip' is a package and cannot be
/// directly executed`, which is a pip that is there but broken, nor any
/// other module's name: those stay "did not answer".
fn says_no_pip_module(stderr: &str) -> bool {
    stderr.lines().any(|line| {
        let line = line.trim_end();
        line == "No module named pip" || line.ends_with(": No module named pip")
    })
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

    /// The environment of `check_updates`' `pip list --outdated`: pip's
    /// normal verbosity, whatever the user's `pip.conf` or environment
    /// set. Not quieter: at `quiet = 2` pip hides the urllib3 warnings
    /// `lookups_given_up` reads, and the lookups it gave up on would read
    /// as up to date. Not louder: `verbose = 2` prints a line per file
    /// pip skips, thousands for one project, toward the runner's 64 MiB
    /// and 60-second limits (opus-int finding 4) -- and its index
    /// answers, a 404 from an extra index among them (finding 3). Both
    /// are counts in pip, and `0` is a count it accepts from the
    /// environment (`cli/parser.py`, `_update_defaults`).
    /// `tests/what_we_run_test.rs` checks that pip's section of
    /// `docs/what-we-run.md` shows each entry.
    pub const OUTDATED_ENV: [(&'static str, &'static str); 2] =
        [("PIP_QUIET", "0"), ("PIP_VERBOSE", "0")];

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
        // Every look here one step at a time, never into or through a
        // protected place (`protected::look`), for this home folder and
        // the account's.
        let protected = Protected::new(&env.home);
        for name in Self::CANDIDATE_INTERPRETERS {
            let Some(python_path) = resolve_exe(name, env) else {
                continue;
            };
            let canonical =
                look::real_path(&python_path, &protected).unwrap_or_else(|_| python_path.clone());
            // As the disk compares names: each is spelled as `PATH` and
            // its links spell it (`protected::look`).
            if !seen.insert(protected::folded(&canonical)) {
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
                if !self.shim_has_tool(shim, dir, &protected) {
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
            let (version, no_pip) = match output {
                // "pip 26.2.1 from … (python 3.14)" — the shared
                // second-token rule (crate::adapters::second_token, Task 5)
                // yields pip's own version, which is what
                // `ManagerInstance::version` means here, not the
                // interpreter's Python version.
                Ok(o) if o.exit_code == Some(0) => (second_token(&o.stdout), false),
                Ok(o) => (None, says_no_pip_module(&o.stderr)),
                Err(_) => (None, false),
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
                // through it. With no pip at all -- Python said so
                // itself (`says_no_pip_module`) -- that is `NoPip`, which
                // checking again does not change; anything else is the
                // same "found the executable, it didn't answer" state
                // brew/cargo/pipx/uv/ollama report as `NotResponding` on
                // their own failed `--version`. This
                // instance's id needs no command to exist -- it is built
                // from `python_path`, which `resolve_exe` already
                // resolved -- so there is no reason to drop the row
                // instead of reporting it: doing that used to make this
                // interpreter's pip disappear with no notice and no
                // `SourceError`, indistinguishable from "there never was a
                // pip here to ask about".
                status: InstanceStatus {
                    unavailable: match (&version, no_pip) {
                        (Some(_), _) => None,
                        (None, true) => Some(Unavailable::NoPip),
                        (None, false) => Some(Unavailable::NotResponding),
                    },
                    notes: Vec::new(),
                },
                version,
                answered_at: None,
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
    /// `usr/bin` there, run only when it is an executable file). Looked up
    /// one step at a time and never into or through a protected place
    /// (`protected::look`): a developer folder in `~/Downloads` -- an
    /// Xcode beta never moved to Applications, chosen with `xcode-select
    /// -s` -- is one Banager cannot look at, so the shim has no tool it
    /// knows of, and is skipped.
    fn shim_has_tool(
        &self,
        shim: &Path,
        developer_dir: Option<&Path>,
        protected: &Protected,
    ) -> bool {
        let (Some(dir), Some(name)) = (developer_dir, shim.file_name()) else {
            return false;
        };
        let Ok((tool, meta)) = look::target(&dir.join("usr/bin").join(name), protected) else {
            return false;
        };
        tool.parent() != Some(self.shim_dir.as_path()) && meta.is_file() && meta.mode() & 0o111 != 0
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
                    env: Self::OUTDATED_ENV
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
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
            let failure = LookupFailure::words(
                lookup_failure_reason("pip list --outdated", output.exit_code, &output.stderr),
                &output.stderr,
            );
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
                        failure.clone(),
                    )
                })
                .collect::<Vec<_>>()
                .into());
        }
        let checked = parse_outdated_stdout(&output.stdout, &inst.id)?;
        // Exit 0 is not "every package was looked up": a lookup pip gave
        // up on is left out of stdout as if it were up to date (round-5
        // review finding 7). Its retry warnings on stderr say which, and
        // a certificate failure says so on stdout. A lookup that failed
        // with retries turned off, or that an index answered with an
        // HTTP error, is printed only at `-vv` (`final_fetch_failures`)
        // and still reads as up to date.
        let mut gave_up = lookups_given_up(&output.stderr);
        gave_up.extend(final_fetch_failures(&output.stdout));
        gave_up.extend(final_fetch_failures(&output.stderr));
        if gave_up.is_empty() {
            return Ok(checked.into());
        }
        Ok(self
            .with_lookups_given_up(inst, checked, &gave_up)
            .await
            .into())
    }

    /// `checked` -- what `pip list --outdated` listed, each a real answer
    /// -- and, after it, a "could not check" row (`uncheckable_candidate`)
    /// for each installed package it did not list whose lookup pip gave
    /// up on (`lookups_given_up`), with that lookup's words: transient
    /// where they say the network failed, as when `pip list --outdated`
    /// exits non-zero (`LookupFailure::words`). A lookup whose address
    /// names no installed package -- a `--find-links` page, say -- left
    /// every package's answer short, so every package not listed gets the
    /// row. Installed packages come from the plain list, as on that path.
    ///
    /// If that list cannot be had, `checked` as it is: the answers pip did
    /// give are real, and a package Banager cannot name gets no row -- it
    /// reads as up to date, as it did before any of this was read -- where
    /// failing the source would throw those answers away too (a2 review 4).
    async fn with_lookups_given_up(
        &self,
        inst: &ManagerInstance,
        mut checked: Vec<UpdateCandidate>,
        gave_up: &[GaveUp],
    ) -> Vec<UpdateCandidate> {
        let Ok(installed) = self.run_pip_list(inst, &[]).await else {
            return checked;
        };
        let projects: HashSet<String> = installed
            .iter()
            .map(|p| canonical_project(&p.name))
            .collect();
        let everyone = gave_up.iter().rfind(|g| !projects.contains(&g.project));
        let listed: HashSet<String> = checked
            .iter()
            .map(|c| canonical_project(&c.key.name))
            .collect();
        for package in installed {
            let project = canonical_project(&package.name);
            if listed.contains(&project) {
                continue;
            }
            let Some(lookup) = gave_up.iter().rfind(|g| g.project == project).or(everyone) else {
                continue;
            };
            checked.push(uncheckable_candidate(
                ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: package.name,
                },
                package.version,
                UpdateChannel::Native,
                LookupFailure::words(
                    lookup_failure_reason("pip list --outdated", Some(0), &lookup.words),
                    &lookup.words,
                ),
            ));
        }
        checked
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
    use std::os::unix::fs::PermissionsExt;

    // Regressions found by `adapters/robustness.rs`.

    /// pip 26.2.1's words for an index certificate it could not verify,
    /// as `ssl` gives them on macOS (`SSLVerificationError` around
    /// urllib3's `SSLError`).
    const BAD_CERTIFICATE: &str = "[SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed: unable to get local issuer certificate (_ssl.c:1032)";

    /// The INFO line pip 26.2.1 prints on stdout, at its normal
    /// verbosity, for an index page it gave up on over its certificate
    /// (`index/collector.py`: `_handle_get_simple_fail(..., meth=
    /// logger.info)`). The address is the link as pip prints it, its
    /// password already `****` (`Link.__str__`, `redacted_url`).
    fn certificate_failure(address: &str) -> String {
        format!(
            "Could not fetch URL {address}: There was a problem confirming the ssl certificate: {BAD_CERTIFICATE} - skipping"
        )
    }

    /// The DEBUG line pip prints, only at `-vv`, for an index that
    /// answered a project's page with an HTTP error status: the reason is
    /// pip's `raise_for_status` wording (`network/utils.py`).
    fn http_status_failure(address: &str, status: &str) -> String {
        format!(
            "Could not fetch URL {address}: {status} Client Error: {} for url: {address} - skipping",
            match status {
                "401" => "Unauthorized",
                "403" => "Forbidden",
                "404" => "Not Found",
                "407" => "Proxy Authentication Required",
                "410" => "Gone",
                _ => "Error",
            }
        )
    }

    #[tokio::test]
    async fn regression_a_certificate_failure_is_not_a_successful_empty_check() {
        // fixrev's F8 at pip's normal verbosity: the index's certificate
        // cannot be verified, every try fails, and pip exits 0 with `[]`.
        // urllib3 warned on stderr before each retry, and pip says it
        // skipped the page on stdout, ahead of the JSON.
        let address = "https://user:****@index.example/simple/cowsay/";
        let ssl = format!("SSLError(SSLCertVerificationError(1, '{BAD_CERTIFICATE}'))");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            OUTDATED_ARGV.to_vec(),
            exited_with(
                0,
                &format!("{}\n[]\n", certificate_failure(address)),
                &gave_up_on(&ssl, "/simple/cowsay/"),
            ),
        );
        runner.respond(
            LIST_ARGV.to_vec(),
            exited_with(0, r#"[{"name":"cowsay","version":"5.0"}]"#, ""),
        );
        let adapter = PipAdapter::new(runner);
        let rows = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .unwrap()
            .candidates;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key.name, "cowsay");
        assert!(!rows[0].checkable);
        // A certificate is not the network: Check Again does not fix it.
        assert!(!rows[0].warnings.contains(&Warning::TransientLookupFailure));
        let row = format!("{:?}", rows[0]);
        assert!(!row.contains("user") && !row.contains("****"), "{row}");
    }

    #[tokio::test]
    async fn regression_an_extra_index_without_the_project_is_not_a_failed_lookup() {
        // opus-int finding 3. With `extra-index-url` (PyTorch's, a
        // company's), pip asks every index for every project; the one that
        // does not host it answers 404 -- or 403, as PyTorch's does -- and PyPI
        // answers with the project. At pip's normal verbosity none of that
        // is printed: stdout is the JSON alone, stderr is empty.
        // cowsay is out of date on PyPI; six is up to date there, and so
        // not in the JSON at all.
        let outdated = r#"[{"name": "cowsay", "version": "5.0", "latest_version": "6.1", "latest_filetype": "wheel"}]"#;
        let extra = |project: &str| format!("https://download.pytorch.org/whl/cpu/{project}/");
        for stdout in [
            format!("{outdated}\n"),
            // A pip that prints the index's answers anyway, as at `-vv`.
            format!(
                "{}\n{}\n{outdated}\n",
                http_status_failure(&extra("cowsay"), "404"),
                http_status_failure(&extra("six"), "404")
            ),
            format!(
                "{}\n{}\n{outdated}\n",
                http_status_failure(&extra("cowsay"), "403"),
                http_status_failure(&extra("six"), "403")
            ),
        ] {
            let runner = Arc::new(MockRunner::new());
            runner.respond(OUTDATED_ARGV.to_vec(), exited_with(0, &stdout, ""));
            runner.respond(
                LIST_ARGV.to_vec(),
                exited_with(
                    0,
                    r#"[{"name":"cowsay","version":"5.0"},{"name":"six","version":"1.17.0"}]"#,
                    "",
                ),
            );
            let adapter = PipAdapter::new(runner.clone());
            let rows = adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .unwrap()
                .candidates;
            // six reads as up to date, not as "could not check".
            assert_eq!(rows.len(), 1, "{stdout}");
            assert_eq!(rows[0].key.name, "cowsay", "{stdout}");
            assert!(rows[0].checkable, "{stdout}");
            assert_eq!(rows[0].target, "6.1");
            // No lookup was given up on, so no second list was needed.
            assert_eq!(runner.calls(), vec![OUTDATED_ARGV.to_vec()], "{stdout}");
        }
    }

    #[test]
    fn test_final_fetch_failures_count_failures_but_not_an_index_without_the_project() {
        let address = "https://index.example/simple/cowsay/";
        for status in ["403", "404", "410"] {
            assert_eq!(
                final_fetch_failures(&http_status_failure(address, status)),
                vec![],
                "{status}"
            );
        }
        let failed = |line: String| {
            let found = final_fetch_failures(&line);
            assert_eq!(found.len(), 1, "{line}");
            assert_eq!(found[0].project, "cowsay", "{line}");
            found[0].words.clone()
        };
        for status in ["401", "407"] {
            assert_eq!(
                failed(http_status_failure(address, status)),
                "pip could not fetch the package index"
            );
        }
        assert_eq!(
            failed(format!(
                "Could not fetch URL {address}: 503 Server Error: Service Unavailable for url: {address} - skipping"
            )),
            "pip could not fetch the package index"
        );
        assert_eq!(
            failed(certificate_failure(address)),
            "pip could not fetch the package index"
        );
        assert_eq!(
            failed(format!(
                "Could not fetch URL {address}: connection error: HTTPSConnectionPool(host='index.example', port=443): Max retries exceeded with url: /simple/cowsay/ (Caused by NewConnectionError('Failed to establish a new connection: [Errno 61] Connection refused')) - skipping"
            )),
            "network error: pip could not fetch the package index"
        );
    }

    #[test]
    fn test_parse_outdated_stdout_reads_the_json_after_pips_info_lines() {
        let address = "https://pypi.org/simple/cowsay/";
        let rows =
            parse_outdated_stdout(&format!("{}\n[]\n", certificate_failure(address)), "pip:/x")
                .unwrap();
        assert!(rows.is_empty());
        let fixture =
            std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-outdated.json")
                .expect("read pip list-outdated.json fixture");
        assert_eq!(parse_outdated_stdout(&fixture, "pip:/x").unwrap().len(), 3);
        assert!(parse_outdated_stdout("fetch failed but no JSON", "pip:/x").is_err());
    }

    /// Records each command it is given and answers the outdated check
    /// with an empty list.
    struct SpecRecordingRunner {
        specs: std::sync::Mutex<Vec<CommandSpec>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SpecRecordingRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, crate::runner::RunnerError> {
            self.specs.lock().unwrap().push(spec);
            Ok(exited_with(0, "[]\n", ""))
        }
    }

    #[tokio::test]
    async fn regression_the_outdated_check_runs_at_pips_normal_verbosity() {
        // opus-int finding 4: `-vv` printed a line per file pip skipped,
        // thousands for numpy alone. The check passes no `-v`, and pins
        // pip's verbosity against a `pip.conf` that sets it either way.
        let runner = Arc::new(SpecRecordingRunner {
            specs: std::sync::Mutex::new(Vec::new()),
        });
        let adapter = PipAdapter::new(runner.clone());
        adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .unwrap();
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].args,
            ["-m", "pip", "list", "--outdated", "--format=json"]
        );
        assert!(!specs[0].args.iter().any(|a| a.starts_with("-v")));
        assert_eq!(
            specs[0].env,
            [
                ("PIP_QUIET".to_string(), "0".to_string()),
                ("PIP_VERBOSE".to_string(), "0".to_string())
            ]
        );
    }

    #[test]
    fn regression_parse_pip_list_drops_a_nameless_package_and_reads_a_broken_version_as_unknown() {
        let json = r#"[{"name":"","version":"1"},{"name":"six","version":"1.17\n0"}]"#;
        let packages = parse_pip_list(json).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "six");
        assert_eq!(packages[0].version, "");
    }

    #[test]
    fn regression_parse_pip_outdated_drops_an_update_to_nothing() {
        let json = r#"[{"name":"a","version":"1","latest_version":""},
            {"name":"","version":"1","latest_version":"2"},
            {"name":"six","version":"1.16.0","latest_version":"1.17.0"}]"#;
        let names: Vec<String> = parse_pip_outdated(json, "pip:/x")
            .unwrap()
            .into_iter()
            .map(|c| c.key.name)
            .collect();
        assert_eq!(names, vec!["six".to_string()]);
    }

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
        let dir = crate::testing::unique_temp_path("pip-detect");
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
        let dir = crate::testing::unique_temp_path("pip-detect-unreachable");
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
        // Python said there is no pip: not "did not answer", which
        // checking again would fix, but `NoPip`, which it will not.
        assert_eq!(instances[0].status.unavailable, Some(Unavailable::NoPip));
        assert!(!instances[0].available());
        // Read-only by design is a property of pip itself, independent of
        // whether this round could reach it.
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::ByDesign)
        );
    }

    #[tokio::test]
    async fn test_detect_keeps_not_responding_for_a_pip_that_is_there_but_broken() {
        // Only Python's own "No module named pip" is `NoPip`. A pip whose
        // package is there but will not run says something else, and
        // checking again after fixing it can help: still `NotResponding`.
        let dir = temp_folder("detect-broken-pip");
        let python_path = dir.join("python3.13");
        std::fs::write(&python_path, b"#!/bin/sh\n").expect("write fake python");
        let python_path_str = python_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![python_path_str, "-m", "pip", "--version"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: format!(
                    "{python_path_str}: No module named pip.__main__; 'pip' is a package and cannot be directly executed\n"
                ),
                timed_out: false,
                cancelled: false,
            },
        );
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = PipAdapter::new(runner).detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[test]
    fn test_says_no_pip_module_reads_pythons_own_line_and_nothing_else() {
        // As `python3 -m pip` prints it, with the interpreter's path first.
        assert!(says_no_pip_module(
            "/opt/local/bin/python3.13: No module named pip\n"
        ));
        assert!(says_no_pip_module("No module named pip"));
        // A pip that is there but broken, another module, a crash.
        assert!(!says_no_pip_module(
            "/opt/local/bin/python3.13: No module named pip.__main__; 'pip' is a package and cannot be directly executed\n"
        ));
        assert!(!says_no_pip_module(
            "/usr/bin/python3: No module named pipx\n"
        ));
        assert!(!says_no_pip_module(
            "Traceback (most recent call last):\n  ModuleNotFoundError: No module named 'pip._internal'\n"
        ));
        assert!(!says_no_pip_module(""));
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
    async fn test_detect_never_looks_for_the_shims_tool_in_a_protected_place() {
        // A developer folder in `~/Downloads` -- an Xcode beta never moved
        // to Applications, chosen with `xcode-select -s` -- or reached
        // through a link into one: never looked into, so the shim is
        // skipped as when the folder has no tool, and never run.
        let home = temp_folder("home-xcode-in-downloads");
        let shims = temp_folder("shims-protected");
        let shim = shims.join("python3");
        file_at(&shim, 0o755);
        let developer = home.join("Downloads/Xcode-beta.app/Contents/Developer");
        file_at(&developer.join("usr/bin/python3"), 0o755);
        let linked = home.join("xcode-developer");
        std::os::unix::fs::symlink(&developer, &linked).expect("link");
        for answer in [&developer, &linked] {
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                PipAdapter::XCODE_SELECT_ARGV.to_vec(),
                exited(0, &format!("{}\n", answer.display())),
            );
            runner.respond(
                vec![text(&shim), "-m", "pip", "--version"],
                exited(0, PIP_VERSION),
            );
            let adapter = adapter_with_shims_in(runner.clone(), &shims);
            let mut env = path_of(&[&shims]);
            env.home = home.clone();
            let instances = adapter.detect(&env).await;
            assert!(instances.is_empty(), "{answer:?}: {instances:?}");
            assert_eq!(
                runner.calls(),
                vec![argv(&PipAdapter::XCODE_SELECT_ARGV)],
                "{answer:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&shims);
        let _ = std::fs::remove_dir_all(&home);
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

    /// One line of urllib3's retry warning as pip prints it on stderr
    /// (`connectionpool.py`: `"Retrying (%r) after connection broken by
    /// '%r': %s"`, pip's `WARNING: `), `total` tries left.
    fn retry_warning(total: u8, error: &str, address: &str) -> String {
        format!(
            "WARNING: Retrying (Retry(total={total}, connect=None, read=None, redirect=None, status=None)) after connection broken by '{error}': {address}"
        )
    }

    /// urllib3's `NameResolutionError` repr, as macOS's resolver words a
    /// name that would not resolve.
    const NO_DNS: &str = "NameResolutionError(\"<pip._vendor.urllib3.connection.HTTPSConnection object at 0x1048a5e50>: Failed to resolve 'pypi.org' ([Errno 8] nodename nor servname provided, or not known)\")";
    /// urllib3's `ReadTimeoutError` repr at pip's 15-second timeout.
    const READ_TIMEOUT: &str = "ReadTimeoutError(\"HTTPSConnectionPool(host='pypi.org', port=443): Read timed out. (read timeout=15)\")";
    /// A server that hung up: urllib3's `ProtocolError` around
    /// http.client's `RemoteDisconnected`.
    const HUNG_UP: &str = "ProtocolError('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))";

    /// Every warning pip prints for one lookup that never connects: one
    /// per retry, `total=4` down to `total=0`, then nothing.
    fn gave_up_on(error: &str, address: &str) -> String {
        (0..=4)
            .rev()
            .map(|total| retry_warning(total, error, address))
            .collect::<Vec<_>>()
            .join("\n")
    }

    const OUTDATED_ARGV: [&str; 6] = [
        "/opt/homebrew/bin/python3.14",
        "-m",
        "pip",
        "list",
        "--outdated",
        "--format=json",
    ];
    const LIST_ARGV: [&str; 5] = [
        "/opt/homebrew/bin/python3.14",
        "-m",
        "pip",
        "list",
        "--format=json",
    ];

    fn exited_with(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            timed_out: false,
            cancelled: false,
        }
    }

    #[test]
    fn test_canonical_project_is_the_name_the_index_is_asked_for() {
        for (name, project) in [
            ("PyYAML", "pyyaml"),
            ("typing_extensions", "typing-extensions"),
            ("zope.interface", "zope-interface"),
            ("Foo__Bar-.baz", "foo-bar-baz"),
            ("cowsay", "cowsay"),
        ] {
            assert_eq!(canonical_project(name), project, "{name}");
        }
    }

    #[test]
    fn test_without_object_reprs_keeps_the_words_and_drops_the_connections_address() {
        assert_eq!(
            without_object_reprs(NO_DNS),
            "NameResolutionError(\"Failed to resolve 'pypi.org' ([Errno 8] nodename nor servname provided, or not known)\")"
        );
        assert_eq!(
            without_object_reprs("ConnectTimeoutError(<pip._vendor.urllib3.connection.HTTPSConnection object at 0x10>, 'Connection to pypi.org timed out. (connect timeout=15)')"),
            "ConnectTimeoutError('Connection to pypi.org timed out. (connect timeout=15)')"
        );
        // A `<` that is no object's repr stays.
        assert_eq!(without_object_reprs("a <b> c"), "a <b> c");
        assert_eq!(without_object_reprs("unclosed <x"), "unclosed <x");
    }

    #[test]
    fn test_lookups_given_up_reads_only_the_warning_before_the_last_try() {
        let stderr = [
            retry_warning(4, READ_TIMEOUT, "/simple/requests/"),
            gave_up_on(NO_DNS, "/simple/cowsay/"),
            "WARNING: There was an error checking the latest version of pip.".to_string(),
            // Through a proxy urllib3 names the whole address.
            retry_warning(0, HUNG_UP, "https://pypi.org/simple/pyyaml/"),
        ]
        .join("\n");
        assert_eq!(
            lookups_given_up(&stderr),
            vec![
                GaveUp {
                    project: "cowsay".to_string(),
                    words: without_object_reprs(NO_DNS),
                },
                GaveUp {
                    project: "pyyaml".to_string(),
                    words: HUNG_UP.to_string(),
                },
            ]
        );
        assert_eq!(lookups_given_up(""), vec![]);
        assert_eq!(
            lookups_given_up(&retry_warning(1, NO_DNS, "/simple/cowsay/")),
            vec![]
        );
    }

    #[tokio::test]
    async fn test_check_updates_lists_every_lookup_pip_gave_up_on_though_it_exited_0() {
        // Round-5 review finding 7. A small environment, no network: pip
        // retries each lookup five times, gives each up, finds nothing, and
        // exits 0 with `[]` -- which read as "every package is up to date".
        let stderr = [
            gave_up_on(NO_DNS, "/simple/cowsay/"),
            gave_up_on(NO_DNS, "/simple/pip/"),
            gave_up_on(NO_DNS, "/simple/requests/"),
            "WARNING: There was an error checking the latest version of pip.".to_string(),
        ]
        .join("\n");
        let runner = Arc::new(MockRunner::new());
        runner.respond(OUTDATED_ARGV.to_vec(), exited_with(0, "[]\n", &stderr));
        runner.respond(
            LIST_ARGV.to_vec(),
            exited_with(
                0,
                r#"[{"name": "cowsay", "version": "6.1"}, {"name": "pip", "version": "26.2.1"}, {"name": "requests", "version": "2.32.3"}]"#,
                "",
            ),
        );
        let adapter = PipAdapter::new(runner.clone());
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("lookups pip gave up on are rows, not a failed source")
            .candidates;
        let names: Vec<&str> = candidates.iter().map(|c| c.key.name.as_str()).collect();
        assert_eq!(names, ["cowsay", "pip", "requests"]);
        for candidate in &candidates {
            assert!(!candidate.checkable, "{}", candidate.key.name);
            assert_eq!(candidate.target, candidate.current);
            assert_eq!(
                candidate.warnings,
                vec![
                    Warning::Message(
                        "pip list --outdated: NameResolutionError(\"Failed to resolve 'pypi.org' ([Errno 8] nodename nor servname provided, or not known)\")".to_string()
                    ),
                    Warning::TransientLookupFailure,
                ],
                "{}",
                candidate.key.name
            );
        }
        assert_eq!(runner.calls(), vec![argv(&OUTDATED_ARGV), argv(&LIST_ARGV)]);
    }

    #[tokio::test]
    async fn test_check_updates_keeps_what_pip_listed_and_marks_only_the_lookups_it_gave_up_on() {
        // black was looked up and is listed; requests needed one retry and
        // then answered; cowsay answered first time; PyYAML's lookup timed
        // out five times over. Only PyYAML was not checked.
        let stderr = [
            retry_warning(4, READ_TIMEOUT, "/simple/requests/"),
            gave_up_on(READ_TIMEOUT, "/simple/pyyaml/"),
        ]
        .join("\n");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            OUTDATED_ARGV.to_vec(),
            exited_with(
                0,
                r#"[{"name": "black", "version": "24.1.0", "latest_version": "24.10.0", "latest_filetype": "wheel"}]"#,
                &stderr,
            ),
        );
        runner.respond(
            LIST_ARGV.to_vec(),
            exited_with(
                0,
                r#"[{"name": "black", "version": "24.1.0"}, {"name": "cowsay", "version": "6.1"}, {"name": "PyYAML", "version": "6.0.1"}, {"name": "requests", "version": "2.32.3"}]"#,
                "",
            ),
        );
        let adapter = PipAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 2, "{candidates:?}");
        let black = &candidates[0];
        assert_eq!(black.key.name, "black");
        assert!(black.checkable);
        assert_eq!(black.target, "24.10.0");
        assert!(black.warnings.is_empty());
        let yaml = &candidates[1];
        assert_eq!(yaml.key.name, "PyYAML");
        assert_eq!(yaml.current, "6.0.1");
        assert!(!yaml.checkable);
        assert!(yaml.warnings.contains(&Warning::TransientLookupFailure));
        assert!(matches!(
            &yaml.warnings[0],
            Warning::Message(m) if m.contains("Read timed out")
        ));
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_connection_aborted_after_retries_as_transient() {
        // A server that hung up five times left no answer, even when pip
        // exits successfully with []. The window must offer Check Again.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            OUTDATED_ARGV.to_vec(),
            exited_with(0, "[]", &gave_up_on(HUNG_UP, "/simple/cowsay/")),
        );
        runner.respond(
            LIST_ARGV.to_vec(),
            exited_with(0, r#"[{"name": "cowsay", "version": "6.1"}]"#, ""),
        );
        let adapter = PipAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert_eq!(
            candidates[0].warnings,
            vec![
                Warning::Message(format!("pip list --outdated: {HUNG_UP}")),
                Warning::TransientLookupFailure,
            ]
        );
    }

    #[tokio::test]
    async fn test_check_updates_marks_every_unlisted_package_when_a_lookup_names_none_of_them() {
        // A `--find-links` page from pip.conf that could not be reached:
        // every package's answer is short of what it holds.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            OUTDATED_ARGV.to_vec(),
            exited_with(
                0,
                r#"[{"name": "black", "version": "24.1.0", "latest_version": "24.10.0", "latest_filetype": "wheel"}]"#,
                &gave_up_on(NO_DNS, "https://wheels.example.org/simple-links/index.html"),
            ),
        );
        runner.respond(
            LIST_ARGV.to_vec(),
            exited_with(
                0,
                r#"[{"name": "black", "version": "24.1.0"}, {"name": "cowsay", "version": "6.1"}]"#,
                "",
            ),
        );
        let adapter = PipAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        let names: Vec<(&str, bool)> = candidates
            .iter()
            .map(|c| (c.key.name.as_str(), c.checkable))
            .collect();
        assert_eq!(names, [("black", true), ("cowsay", false)]);
        assert!(candidates[1]
            .warnings
            .contains(&Warning::TransientLookupFailure));
    }

    #[tokio::test]
    async fn test_check_updates_keeps_what_pip_listed_when_the_plain_list_then_fails() {
        // a2 review 4: the extra `pip list` failing used to fail the
        // whole source, throwing away the answers pip did give.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            OUTDATED_ARGV.to_vec(),
            exited_with(
                0,
                r#"[{"name": "black", "version": "24.1.0", "latest_version": "24.10.0", "latest_filetype": "wheel"}]"#,
                &gave_up_on(READ_TIMEOUT, "/simple/pyyaml/"),
            ),
        );
        runner.respond(
            LIST_ARGV.to_vec(),
            exited_with(
                1,
                "",
                "ERROR: Exception:\nTraceback (most recent call last):",
            ),
        );
        let adapter = PipAdapter::new(runner.clone());
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("the answers pip gave are kept")
            .candidates;
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].key.name, "black");
        assert!(candidates[0].checkable);
        assert_eq!(candidates[0].target, "24.10.0");
        assert_eq!(runner.calls(), vec![argv(&OUTDATED_ARGV), argv(&LIST_ARGV)]);
    }

    #[tokio::test]
    async fn test_check_updates_runs_nothing_more_when_every_retry_was_answered() {
        // One retry, then an answer: every lookup was made, so stdout is
        // the whole answer and the plain list is not run.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            OUTDATED_ARGV.to_vec(),
            exited_with(0, "[]", &retry_warning(4, READ_TIMEOUT, "/simple/cowsay/")),
        );
        let adapter = PipAdapter::new(runner.clone());
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert!(candidates.is_empty());
        assert_eq!(runner.calls(), vec![argv(&OUTDATED_ARGV)]);
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
