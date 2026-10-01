use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, InstalledArtifact, InstanceNote, ManagerInstance, OpRequest, Outcome, Plan,
    PlanAction, ReadOnlyReason, Reconciled, SearchHit, Unavailable, UninstallBlocked,
    UninstallUnsafeReason, UpdateBlocked, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, LineCallback, OutputUse, RunLine};
use async_trait::async_trait;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub mod brew;
pub mod cargo;
pub mod npm;
pub mod ollama;
pub mod pip;
pub mod pipx;
pub mod standalone;
pub mod uv;

pub(crate) mod sanity;


/// Options a caller passes down to `check_updates`. Adapters ignore fields
/// that do not apply to them; a new field must never change behaviour for an
/// adapter that does not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOptions {
    /// Homebrew only: include casks that update themselves (`brew outdated --greedy`).
    pub include_self_updating: bool,
    /// When the refresh round this check is part of began. Set by
    /// `Session`'s `refresh_round` over whatever the caller passed; `None`,
    /// a check made outside a refresh, counts as a round beginning now.
    /// Homebrew only: see `UpdateRecord::unreported_failure`. Never on the
    /// wire (`serde(skip)`): an `Instant` means nothing outside this
    /// process.
    #[serde(skip)]
    pub round_started: Option<std::time::Instant>,
}

/// Everything one `check_updates` call learned: the per-package
/// candidates, and anything it found out *about the source itself* while
/// looking.
///
/// A struct rather than a tuple because the second element needs a name
/// that says what it is, and because a third thing to report later must
/// not mean editing seven adapters' signatures again -- this is the
/// second time this phase has touched this one (`CheckOptions` was the
/// first).
///
/// Core-internal on purpose: `check_updates` has exactly one production
/// caller (`Session::refresh`) and never crosses IPC, so this type needs
/// **no TypeScript mirror**. That is what makes it cheaper than the
/// alternatives -- `src/lib/types.ts` is hand-written, and a mismatch
/// there is silent at compile time and wrong at runtime. What does reach
/// the front end is `InstanceNote`, which already had to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckOutcome {
    pub candidates: Vec<UpdateCandidate>,
    /// What the source said about itself. Empty for every adapter but
    /// brew, which reports `IndexMayBeStale` when `brew update` failed.
    /// A `brew update` still running is not a note here: brew then reads
    /// no candidates at all and returns `AdapterError::IndexUpdating`,
    /// which `Session::refresh` turns into `InstanceNote::IndexUpdating`.
    pub notes: Vec<InstanceNote>,
}

/// Lets the seven adapters with nothing to report about themselves write
/// `Ok(out.into())` instead of naming a struct they never fill.
///
/// This conversion is the reason `CheckOutcome` is not a third block of
/// verbatim seven-adapter duplication: what repeats is one call, not a
/// literal with a `notes: Vec::new()` in it.
impl From<Vec<UpdateCandidate>> for CheckOutcome {
    fn from(candidates: Vec<UpdateCandidate>) -> CheckOutcome {
        CheckOutcome {
            candidates,
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AdapterMeta {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub platforms: Vec<String>,
    pub homepage: String,
    pub schema_version: u32,
    pub verified_versions: Vec<String>,
}

impl AdapterMeta {
    pub fn from_toml(s: &str) -> Result<AdapterMeta, toml::de::Error> {
        toml::from_str(s)
    }

    /// `None` when this adapter's metadata lists no verified versions, or
    /// when `detected` is among them (or absent). `Some(detected)` when it
    /// is not, so the UI can mark the source as running an unverified
    /// version (spec §4.1).
    ///
    /// Defined once, here, rather than per adapter: every `detect()` in the
    /// workspace calls this, so the rule can only ever mean one thing. An
    /// earlier draft of this phase had seven copies of it.
    pub fn unverified_version(&self, detected: &Option<String>) -> Option<String> {
        detected
            .as_ref()
            .filter(|v| !self.verified_versions.is_empty() && !self.verified_versions.contains(v))
            .cloned()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("runner: {0}")]
    Runner(#[from] crate::runner::RunnerError),
    #[error("parse: {0}")]
    Parse(String),
    #[error("command failed (exit {code:?}): {stderr}")]
    CommandFailed { code: Option<i32>, stderr: String },
    #[error("refused: {0}")]
    Refused(String),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// `Session::issue_plan`'s actionability gate (spec §2.5) refused an
    /// operation the front end should never have offered: both pages hide
    /// every control for an instance that fails `read_only_reason.is_none()
    /// && status.unavailable.is_none()`. A user sees this only through a
    /// stale snapshot or a genuine TOCTOU (detect and the click can be
    /// seconds to hours apart).
    ///
    /// A dedicated variant rather than a `Refused(String)` built from
    /// `format!("{:?}", ...)` because this one *can* reach a real person,
    /// and a `{:?}` of two Rust enums is the worst possible thing to show
    /// someone who does not read Rust -- `ipc.rs` maps these two fields to
    /// the same localised copy the source's own notice already uses,
    /// instead of this variant's own `Display` (below), which stays plain
    /// English for logs and test failure output.
    ///
    /// Lives on `AdapterError` rather than a new `SessionError` because
    /// `issue_plan` constructs it directly, before ever calling
    /// `adapter.plan()` -- no `Adapter` trait method's signature changes,
    /// so no adapter *needs* to know this variant exists. npm's `plan()`
    /// returns it anyway, for a prefix that stopped being writable after
    /// `detect` looked: that is the gate's own read-only reason, found late.
    #[error("not actionable (read-only: {read_only:?}, unavailable: {unavailable:?})")]
    NotActionable {
        read_only: Option<ReadOnlyReason>,
        unavailable: Option<Unavailable>,
    },
    /// `Session::issue_plan` was asked to plan an `Upgrade` of a package
    /// whose own update candidate says the tool will refuse it
    /// (`UpdateCandidate.blocked`): the per-package half of the same gate
    /// `NotActionable` is the per-source half of. The Updates page hides
    /// the button for such a row, so like `NotActionable` this reaches a
    /// person only through a stale page. `plan_operation_error` in
    /// src-tauri/src/ipc.rs sends it as `{"kind": "update_blocked"}` with
    /// the reason, never as this `Display`.
    #[error("the tool will refuse to update this package ({reason:?})")]
    UpdateBlocked { reason: UpdateBlocked },
    /// `Session::issue_plan` was asked to plan an `Uninstall` of a package
    /// whose own inventory entry says the tool will refuse it
    /// (`InstalledArtifact.uninstall_blocked`). The uninstall twin of
    /// `UpdateBlocked`: the Installed page hides the button for such a
    /// row, so this reaches a person only through a stale page, and
    /// `plan_operation_error` in src-tauri/src/ipc.rs sends it as
    /// `{"kind": "uninstall_blocked"}` with the reason.
    #[error("the tool will refuse to uninstall this package ({reason:?})")]
    UninstallBlocked { reason: UninstallBlocked },
    /// A path-list uninstall's `plan()` (`StandaloneAdapter`, through
    /// `removal::plan_removal`) refused one of the paths the recipe names:
    /// `reason` is which check failed, `path` the path with the home folder
    /// abbreviated to `~` (for `OverlapsKept`, the kept path it concerns).
    /// Nothing was moved. Sent to the front end by
    /// `plan_operation_error` (src-tauri/src/ipc.rs) as
    /// `{"kind":"uninstall_unsafe","path":…,"reason":<snake_case>}`, which
    /// `parseUninstallUnsafe` in src/lib/sources.ts words as one of six
    /// sentences -- never as this `Display`, which is for logs. No
    /// `execute` returns it: the same checks failing at run time are
    /// `Fault::PathChanged` (`removal::execute_removal`).
    #[error("unsafe to remove {path}: {reason:?}")]
    UninstallUnsafe {
        path: String,
        reason: UninstallUnsafeReason,
    },
    /// `Session::issue_plan` was asked to plan against an instance that is
    /// not in the snapshot at all -- the source was removed between the
    /// refresh that drew the row and the click. The `issue_plan` twin of
    /// `SubmitError::SourceGone`, and sent to the front end the same way
    /// (`{"kind": "source_gone"}`), so both read as the same localised
    /// sentence.
    ///
    /// It used to be a `Refused(format!("unknown instance ..."))`, which
    /// `ipc.rs` could only pass on as this project's English: a free-form
    /// string gives the front end nothing to translate. The `Display`
    /// keeps that wording for logs and test failure output.
    #[error("unknown instance {instance_id}")]
    SourceGone { instance_id: String },
    /// The source's package catalogue is being rewritten right now, so the
    /// adapter did not read it. Only brew returns it: its `inventory`
    /// while a `brew update` is running for that instance, its
    /// `check_updates` when the `brew update` it started outlasts the
    /// refresh's patience, and its uninstall `plan` when an update is
    /// running before or begins during its `brew uses`. That update
    /// git-merges Homebrew's own Ruby code and `curl`s the package list
    /// over the file `brew info`, `brew outdated` and `brew uses` read, so
    /// an answer read alongside it could be an error or, worse, a parse of
    /// a half-written file that succeeds.
    ///
    /// Not a failure. `Session::refresh` turns it into
    /// `InstanceNote::IndexUpdating` rather than a `SourceError`, and
    /// carries that instance's previous rows forward through the same
    /// branches a failed read uses. From `plan` it reaches the uninstall
    /// dialog as the `index_updating` kind (`plan_operation_error` in
    /// src-tauri/src/ipc.rs), which asks the user to try again shortly.
    #[error("the package index is being updated")]
    IndexUpdating,
}

/// Percent-encodes one untrusted value for interpolation into a single URL
/// **path segment**.
///
/// Every registry lookup in this crate builds its URL by interpolating a
/// name Banager did not choose: Ollama's model references arrive in the body
/// of `GET {host}/api/tags`, cargo's crate names come out of
/// `.crates2.json`'s keys, pipx's package names out of `pipx list --json`.
/// Raw, such a name can change *which resource the URL addresses* rather
/// than merely name it: `/` adds path segments, `?` starts a query string,
/// `#` truncates the path at a fragment, and `%` lets the value smuggle in
/// its own encoding. The host and scheme are fixed before any interpolation
/// point in all three URLs, so this is not header or origin injection — the
/// request still goes to the right server, at the wrong path.
///
/// The set mirrors the WHATWG URL standard's path percent-encode set plus
/// `/` and `%` — the same set the `url` crate applies in
/// `PathSegmentsMut::push`. Ordinary names (`hexyl`, `qwen3.8:27b-mlx`'s
/// parts) pass through byte-for-byte, so no existing URL changes shape.
///
/// An empty value is an error rather than an empty encoding, because it
/// collapses the path: `https://crates.io/api/v1/crates/` is the crate
/// index, not a crate. The error is a human-readable reason, since all three
/// callers turn a failure into one `checkable: false` row.
pub(crate) fn url_path_segment(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("refusing to build a registry url from an empty name".to_string());
    }
    Ok(utf8_percent_encode(value, URL_PATH_SEGMENT).to_string())
}

/// The WHATWG URL path percent-encode set, plus `/` and `%`. Spelled out
/// rather than imported so the exact bytes are reviewable here.
const URL_PATH_SEGMENT: &AsciiSet = &CONTROLS
    // fragment percent-encode set
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    // path percent-encode set
    .add(b'#')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    // and what keeps the value inside its own segment
    .add(b'/')
    .add(b'%');

/// One "Banager could not find out" row: the item is listed at the version
/// it is installed at, with `checkable: false` and the reason attached.
///
/// Every adapter that reaches a registry answers a failed lookup this way,
/// and this function is why they cannot drift apart again. The alternative
/// three answers all shipped at once: cargo built rows like these, pip, uv
/// and pipx's native path returned `Err` (failing the whole source), and
/// npm returned an empty list -- which on the Updates page is
/// indistinguishable from "everything is up to date". A failed lookup is
/// none of those things: it is not knowing.
///
/// `target` is the installed version, not a guess. `UpdateCandidate` has no
/// "unknown" target, and any other value would be a version Banager is
/// claiming exists.
pub(crate) fn uncheckable_candidate(
    key: ArtifactKey,
    current: String,
    channel: UpdateChannel,
    reason: String,
) -> UpdateCandidate {
    UpdateCandidate {
        key,
        target: current.clone(),
        current,
        channel,
        checkable: false,
        warnings: vec![Warning::Message(reason)],
        blocked: None,
    }
}

/// One `uncheckable_candidate` per installed item, for the adapters whose
/// lookup is a single command covering everything at once (`pip list
/// --outdated`, `uv tool list --outdated`, `pipx list --outdated`, `npm
/// outdated -g`). When that one command cannot reach the index, nothing is
/// known about *any* of them, so every installed item gets a row rather
/// than the source reporting an error and the page showing nothing.
pub(crate) fn uncheckable_from_inventory(
    installed: &[InstalledArtifact],
    channel: UpdateChannel,
    reason: &str,
) -> Vec<UpdateCandidate> {
    installed
        .iter()
        .map(|a| {
            uncheckable_candidate(
                a.key.clone(),
                a.version.clone(),
                channel,
                reason.to_string(),
            )
        })
        .collect()
}

/// Why the lookup could not be made, as one line to hang on those rows.
///
/// The tool's own first line of stderr, because that is the only thing that
/// distinguishes "your network is down" from "this index is refusing you"
/// -- and because `Warning::Message` is shown verbatim (spec §6 backlogs
/// localising it), a whole multi-line stderr dump would become the row's
/// description. `what` names the command for the case where the tool exits
/// non-zero and says nothing at all, which would otherwise leave the row
/// with an empty reason.
pub(crate) fn lookup_failure_reason(what: &str, code: Option<i32>, stderr: &str) -> String {
    const MAX: usize = 200;
    let line = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return match code {
            Some(code) => format!("`{what}` exited with code {code}"),
            None => format!("`{what}` did not finish"),
        };
    }
    match line.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{what}: {}...", &line[..cut]),
        None => format!("{what}: {line}"),
    }
}

/// Matches `^[A-Za-z0-9@._+/-]+$`, rejects names starting with `-`, `/` or
/// `.`, rejects a `..` path segment anywhere, and rejects a trailing `.rb`
/// (implemented by hand instead of pulling in the `regex` crate, since this
/// is the only place in the crate that needs pattern matching). The `/`,
/// leading-`.`, `..`-segment and `.rb`-suffix rules exist specifically so
/// `brew install --formula {name}` can never be handed a path: without them
/// `validate_package_name("/tmp/evil.rb")` — or a tap-relative
/// `"../../tmp/evil.rb"` — would pass, and Homebrew treats a `.rb`-suffixed
/// argument as a local formula file to load and run, not a formula name to
/// look up.
pub fn validate_package_name(name: &str) -> Result<(), AdapterError> {
    if name.is_empty()
        || name.starts_with('-')
        || name.starts_with('/')
        || name.starts_with('.')
        || name.ends_with(".rb")
        || name.split('/').any(|segment| segment == "..")
    {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '/' | '-'));
    if !valid {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Whether `key` is installed, and at what version, given one `inventory()`
/// answer.
///
/// The rule is `kind` **and** `name`, both exactly: a source can hold two
/// artifacts that share a name and differ in kind (Homebrew's `python`
/// formula and its `python` cask), and answering "present, at 3.14.2" for
/// the wrong one would make Banager report an uninstall as having failed,
/// or an install as having already happened.
///
/// `ArtifactKey::instance_id` is deliberately *not* compared. An adapter
/// reconciles against the inventory of the instance it was handed, and the
/// caller is the one that pairs the two; `ensure_instance_match` is where
/// that pairing is checked, on the write path where it matters.
///
/// Extracted because all seven adapters had this body copied out by hand
/// and npm's copy had already lost the `kind` half of the rule -- harmless
/// only because every npm artifact happens to be a `Package`, and nothing
/// kept the seventh copy in step with the other six.
pub fn reconcile_from(artifacts: Vec<InstalledArtifact>, key: &ArtifactKey) -> Reconciled {
    match artifacts
        .into_iter()
        .find(|a| a.key.kind == key.kind && a.key.name == key.name)
    {
        Some(a) => Reconciled {
            present: true,
            version: Some(a.version),
        },
        None => Reconciled {
            present: false,
            version: None,
        },
    }
}

/// Refuses a plan whose `OpRequest` names a different instance than the one
/// the adapter was handed.
///
/// Every writable adapter's `plan()` opens with this, before it validates a
/// name or builds a single argument: the request carries the instance the
/// user acted on, `inst` is the instance the caller resolved, and if they
/// disagree then the command about to be built would run against the wrong
/// prefix -- a `brew uninstall` in `/usr/local` for a row the user clicked
/// in `/opt/homebrew`. Refusing costs nothing and is not recoverable
/// further down.
///
/// `pip` is the one adapter that does not call this: its `plan()` refuses
/// every operation outright, so there is no wrong prefix to protect.
pub fn ensure_instance_match(req: &OpRequest, inst: &ManagerInstance) -> Result<(), AdapterError> {
    if req.instance_id != inst.id {
        return Err(AdapterError::Refused(format!(
            "plan requested for instance {} but given instance {}",
            req.instance_id, inst.id
        )));
    }
    Ok(())
}

#[async_trait]
pub trait Adapter: Send + Sync {
    fn meta(&self) -> &AdapterMeta;
    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance>;
    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError>;
    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError>;
    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError>;
    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError>;
    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError>;
    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError>;
    /// The reading `run_operation` (ops/mod.rs) takes after an uninstall,
    /// which asks one question only: is the artifact still there
    /// (`Reconciled::present`)? Every other operation is verified with
    /// `reconcile`. Defaults to `reconcile`, which is what every source's
    /// uninstall was verified with before this existed.
    ///
    /// `StandaloneAdapter` (adapters/standalone/mod.rs) is what this is
    /// for, and its path-list uninstall (phase 4 step C) brings the one
    /// production override: its `reconcile` refuses a launcher it cannot
    /// read a version from -- after an upgrade that exits 0, such a
    /// launcher is no evidence of success -- while after an uninstall that
    /// launcher, the dangling link such an uninstall leaves when it stops
    /// partway, is exactly the evidence that the tool is still there.
    ///
    /// An adapter that cannot tell -- a permission error where the
    /// launcher should be, say -- answers `Err`, never `present: false`:
    /// `run_operation` turns an `Err` into `Unconfirmed`, so "could not
    /// tell" is never reported as a finished uninstall.
    ///
    /// `plan` is the plan the uninstall carried out -- the one `execute`
    /// was handed -- for an adapter whose answer depends on what it did:
    /// the standalone path-list uninstall reads which paths it moved, and
    /// counts a listed path that is there afterwards as the tool still
    /// there (`removal::left_behind`). The default ignores it.
    async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
        _plan: &Plan,
    ) -> Result<Reconciled, AdapterError> {
        self.reconcile(inst, key).await
    }
}

/// Runs a plan through the runner, streaming each line to the sink, and maps
/// the result the way every adapter must: a clean exit is `Succeeded`, a
/// run that did not reach its exit is `Unconfirmed` (the operation may or
/// may not have taken effect — only `reconcile` can say), and a non-zero
/// exit is `Failed` carrying the last five stderr lines.
///
/// Three things end a run short of its exit, and all three are
/// `Unconfirmed`: the runner stopped it at the user's Cancel (`cancelled`)
/// or at the plan's deadline (`timed_out`), or a signal the run did not
/// send ended it — Activity Monitor, `kill`, a crash — which `RealRunner`
/// reports as no exit code with neither flag set (runner/real.rs, the
/// `child_code` arm of `run`). A command ended that way reported no
/// failure: whatever it wrote to stderr before it died is not a verdict,
/// and `run_operation` (ops/mod.rs) shows a `Failed` as the tool's own
/// verdict and reconciles it with nothing, so mapping it there — as this
/// did until B's Astra finding B-3 — reported an uninstall killed after
/// it had removed the tool as a failure. The `Ok(Outcome::Unconfirmed)`
/// arm judges by the reading after instead, and calls the stop
/// `Cancelled` only when the user's own token fired.
///
/// Every adapter's `execute()` calls this function to turn a finished run
/// into an `Outcome`; brew is the only one that does anything else first
/// (`refuse_if_root`, since Homebrew itself refuses to run as root). It
/// lives here so the did-not-finish rule and the five-line summary can
/// only ever mean one thing; an earlier draft of this phase had six
/// byte-identical copies of it.
pub async fn run_plan(
    runner: &Arc<dyn CommandRunner>,
    plan: &Plan,
    sink: Arc<dyn EventSink>,
    op_id: OpId,
    cancel: CancellationToken,
) -> Result<Outcome, AdapterError> {
    let sink_for_line = sink.clone();
    let on_line: LineCallback = Arc::new(move |run_line| {
        sink_for_line.emit(match run_line {
            RunLine::Output(stream, line) => crate::events::OperationEvent::Log {
                op_id,
                stream,
                line,
            },
            RunLine::Note(note) => crate::events::OperationEvent::Note { op_id, note },
        });
    });
    // The one thing this function spawns is a `Command`. A `TrashPaths`
    // plan is carried out by `StandaloneAdapter::execute` itself, item by
    // item (adapters/standalone/removal.rs); reaching here with one is a
    // bug in an adapter, refused before anything is started.
    let PlanAction::Command { program, args, env } = &plan.action else {
        return Err(AdapterError::Refused(
            "run_plan was handed a plan that runs no command (TrashPaths)".to_string(),
        ));
    };
    let spec = CommandSpec {
        program: program.clone(),
        args: args.clone(),
        env: env.clone(),
        cwd: None,
        timeout: Duration::from_secs(plan.timeout_secs),
        // A build log on its way to the log drawer. Nothing reads this
        // command's stdout as data -- the outcome comes from the exit
        // code and the last five stderr lines -- so a runaway build is
        // better shortened than turned into a failed operation.
        output_use: OutputUse::Transcript,
    };
    let output = runner.run(spec, Some(on_line), cancel).await?;
    if output.cancelled || output.timed_out {
        return Ok(Outcome::Unconfirmed);
    }
    match output.exit_code {
        Some(0) => Ok(Outcome::Succeeded),
        Some(code) => {
            let stderr_lines: Vec<&str> = output.stderr.lines().collect();
            let start = stderr_lines.len().saturating_sub(5);
            Ok(Outcome::Failed {
                exit_code: Some(code),
                summary: stderr_lines[start..].join("\n"),
            })
        }
        // Neither flag is set, so the runner stopped nothing: a signal the
        // run did not send ended the command before it could exit (the doc
        // comment above). No exit code, no verdict.
        None => Ok(Outcome::Unconfirmed),
    }
}

/// `"cargo 1.98.1 (…)"` -> `Some("1.98.1")`. Several tools (cargo, uv, pip)
/// print their version as the second whitespace-separated token of the first
/// line; this is that rule, once. Tools that print it differently — pipx's
/// bare `1.17.3`, Ollama's `ollama version is 0.34.1` — keep their own
/// parser. A token with a control character in it is no version
/// (`sanity::version_token`).
pub fn second_token(text: &str) -> Option<String> {
    let mut parts = text.lines().next()?.split_whitespace();
    let _label = parts.next()?;
    sanity::version_token(parts.next().map(|token| token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_second_token_refuses_a_version_with_a_control_character() {
        // Found by `adapters/robustness.rs` (a flipped byte in cargo's line).
        assert_eq!(second_token("cargo 1.9\u{0}.1 (797e8a9bc)\n"), None);
        assert_eq!(second_token("uv 0.12.1\u{17} (Homebrew)\n"), None);
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
    }

    #[test]
    fn test_from_toml_parses_the_committed_brew_meta_file() {
        // cargo runs tests with cwd = the package manifest directory
        // (crates/banager-core), so this reaches the repo-root file.
        let s = std::fs::read_to_string("../../adapters/meta/brew.toml")
            .expect("read adapters/meta/brew.toml");
        let meta = AdapterMeta::from_toml(&s).expect("parse brew.toml");
        assert_eq!(meta.id, "brew");
        assert_eq!(meta.name, "Homebrew");
        assert_eq!(meta.platforms, vec!["macos".to_string()]);
    }

    #[test]
    fn test_unverified_version_flags_only_a_version_outside_a_non_empty_verified_list() {
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec!["1.0".to_string()],
        };
        assert_eq!(meta.unverified_version(&Some("1.0".to_string())), None);
        assert_eq!(
            meta.unverified_version(&Some("9.9".to_string())),
            Some("9.9".to_string())
        );
        assert_eq!(meta.unverified_version(&None), None);

        // An adapter whose meta file lists no verified versions has nothing
        // to compare against, so it never flags anything.
        let unpinned = AdapterMeta {
            verified_versions: vec![],
            ..meta
        };
        assert_eq!(unpinned.unverified_version(&Some("9.9".to_string())), None);
    }

    #[test]
    fn test_url_path_segment_encodes_everything_that_could_leave_the_segment() {
        // Ordinary names must survive untouched, or every registry lookup
        // in the app changes shape.
        assert_eq!(url_path_segment("hexyl").expect("plain name"), "hexyl");
        assert_eq!(
            url_path_segment("qwen3.8_v2-beta+1~x").expect("punctuated name"),
            "qwen3.8_v2-beta+1~x"
        );

        // The characters that would otherwise change which resource the URL
        // addresses rather than merely naming it.
        assert_eq!(url_path_segment("a/b").expect("slash"), "a%2Fb");
        assert_eq!(
            url_path_segment("latest?x=1").expect("query"),
            "latest%3Fx=1"
        );
        assert_eq!(
            url_path_segment("tag#frag").expect("fragment"),
            "tag%23frag"
        );
        assert_eq!(url_path_segment("a%2Fb").expect("percent"), "a%252Fb");
        assert_eq!(url_path_segment("a b").expect("space"), "a%20b");
        assert_eq!(url_path_segment("a\r\nb").expect("crlf"), "a%0D%0Ab");
        assert_eq!(url_path_segment("..").expect("dotdot"), "..");
    }

    #[test]
    fn test_url_path_segment_refuses_an_empty_value() {
        // An empty segment collapses the path and addresses a *different*
        // endpoint (`https://crates.io/api/v1/crates/` is the crate index,
        // not one crate), so it is refused rather than encoded.
        assert!(url_path_segment("").is_err());
    }

    #[test]
    fn test_validate_package_name_accepts_normal_names() {
        assert!(validate_package_name("jq").is_ok());
        assert!(validate_package_name("node@20").is_ok());
        assert!(validate_package_name("some.tool_v2+beta").is_ok());
    }

    #[test]
    fn test_validate_package_name_rejects_shell_metacharacters() {
        assert!(validate_package_name("-rf").is_err());
        assert!(validate_package_name("a;b").is_err());
        assert!(validate_package_name("").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_an_absolute_path() {
        assert!(validate_package_name("/tmp/evil.rb").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_a_leading_dot() {
        assert!(validate_package_name(".hidden").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_a_dotdot_segment() {
        assert!(validate_package_name("foo/../evil").is_err());
        assert!(validate_package_name("../evil").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_an_rb_suffix() {
        assert!(validate_package_name("evil.rb").is_err());
        assert!(validate_package_name("some/tap/evil.rb").is_err());
    }

    #[test]
    fn test_validate_package_name_still_accepts_a_tap_qualified_cask_name() {
        assert!(validate_package_name("gautham-v/tap/claudebar").is_ok());
    }

    #[test]
    fn test_second_token_reads_the_version_out_of_a_labelled_version_line() {
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
        assert_eq!(
            second_token("uv 0.12.17 (Homebrew)"),
            Some("0.12.17".to_string())
        );
        assert_eq!(second_token(""), None);
        assert_eq!(second_token("onlyoneword\n"), None);
    }

    /// An install plan that runs `/bin/fake` with `args`, for the
    /// `run_plan` tests: what `run_plan` returns depends only on the
    /// `CommandOutput` the runner answers with.
    fn plan_for(args: Vec<&str>) -> Plan {
        use crate::model::{CancelPolicy, OpKind, OpRequest, ResourceLock};
        use std::path::PathBuf;

        Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "fake:1".to_string(),
                artifact_kind: ArtifactKind::Package,
                name: "jq".to_string(),
            },
            action: PlanAction::Command {
                program: PathBuf::from("/bin/fake"),
                args: args.into_iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("fake:1".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 60,
        }
    }

    #[tokio::test]
    async fn test_run_plan_maps_a_cancelled_run_to_unconfirmed_and_a_failure_to_the_last_stderr_lines(
    ) {
        use crate::events::VecSink;
        use crate::runner::{CommandOutput, MockRunner};
        use tokio_util::sync::CancellationToken;

        let runner_raw = MockRunner::new();
        runner_raw.respond(
            vec!["/bin/fake", "cancelled"],
            CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: true,
            },
        );
        runner_raw.respond(
            vec!["/bin/fake", "failed"],
            CommandOutput {
                exit_code: Some(2),
                stdout: String::new(),
                stderr: "l1\nl2\nl3\nl4\nl5\nl6\nl7".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let runner: Arc<dyn CommandRunner> = Arc::new(runner_raw);

        let sink = Arc::new(VecSink::new());
        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["cancelled"]),
                sink.clone(),
                1,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Unconfirmed
        );
        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["failed"]),
                sink,
                2,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Failed {
                exit_code: Some(2),
                summary: "l3\nl4\nl5\nl6\nl7".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_run_plan_maps_a_run_ended_by_a_signal_it_did_not_send_to_unconfirmed() {
        // No exit code and neither flag set is how `RealRunner` reports a
        // child that a signal the run did not send ended -- Activity
        // Monitor, `kill`, a crash (runner/real.rs, the `child_code` arm of
        // `run`). Such a command did not run to its end and reported no
        // failure: whatever it wrote to stderr before it died is not a
        // verdict, so it is not `Failed`, which `run_operation` shows as
        // the tool's own and never reconciles. `Unconfirmed` is reconciled
        // like a Cancel or a timeout.
        use crate::events::VecSink;
        use crate::runner::{CommandOutput, MockRunner};
        use tokio_util::sync::CancellationToken;

        let runner_raw = MockRunner::new();
        runner_raw.respond(
            vec!["/bin/fake", "killed"],
            CommandOutput {
                exit_code: None,
                stdout: "==> Pouring jq".to_string(),
                stderr: "Warning: partial\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let runner: Arc<dyn CommandRunner> = Arc::new(runner_raw);

        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["killed"]),
                Arc::new(VecSink::new()),
                3,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Unconfirmed
        );
    }

    #[tokio::test]
    async fn test_run_plan_sends_a_runner_note_to_the_log_as_a_note_not_as_text() {
        // The one door between a runner's callback and the log drawer. A
        // note that came out the other side as `Log { line }` would need
        // English text to carry it, which is the thing `LogNote` exists to
        // keep out of the log.
        use crate::events::{LogNote, OperationEvent, Stream, VecSink};
        use crate::model::{CancelPolicy, OpKind, OpRequest, ResourceLock};
        use crate::runner::{CommandOutput, RunnerError};
        use tokio_util::sync::CancellationToken;

        struct NotingRunner;
        #[async_trait::async_trait]
        impl CommandRunner for NotingRunner {
            async fn run(
                &self,
                _spec: CommandSpec,
                on_line: Option<LineCallback>,
                _cancel: CancellationToken,
            ) -> Result<CommandOutput, RunnerError> {
                let cb = on_line.expect("run_plan always streams");
                cb(RunLine::Output(
                    Stream::Stdout,
                    "==> Pouring jq".to_string(),
                ));
                cb(RunLine::Note(LogNote::ReadFailed {
                    stream: Stream::Stdout,
                    error: "Input/output error (os error 5)".to_string(),
                }));
                Ok(CommandOutput {
                    exit_code: Some(0),
                    stdout: "==> Pouring jq".to_string(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                })
            }
        }

        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "fake:1".to_string(),
                artifact_kind: ArtifactKind::Package,
                name: "jq".to_string(),
            },
            action: PlanAction::Command {
                program: std::path::PathBuf::from("/bin/fake"),
                args: Vec::new(),
                env: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("fake:1".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 60,
        };
        let runner: Arc<dyn CommandRunner> = Arc::new(NotingRunner);
        let sink = Arc::new(VecSink::new());
        run_plan(&runner, &plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("run_plan");

        assert_eq!(
            sink.snapshot(),
            vec![
                OperationEvent::Log {
                    op_id: 9,
                    stream: Stream::Stdout,
                    line: "==> Pouring jq".to_string(),
                },
                OperationEvent::Note {
                    op_id: 9,
                    note: LogNote::ReadFailed {
                        stream: Stream::Stdout,
                        error: "Input/output error (os error 5)".to_string(),
                    },
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_run_plan_refuses_a_plan_that_runs_no_command() {
        // A `TrashPaths` plan is carried out by `StandaloneAdapter::execute`
        // itself (adapters/standalone/removal.rs), never by a runner. Handing
        // one to `run_plan` is a bug in an adapter, refused before anything
        // could be spawned.
        use crate::events::VecSink;
        use crate::model::{CancelPolicy, OpKind, OpRequest, PlanAction, ResourceLock};
        use crate::runner::MockRunner;
        use std::path::PathBuf;
        use tokio_util::sync::CancellationToken;

        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: "standalone-claude".to_string(),
                artifact_kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            },
            action: PlanAction::TrashPaths {
                paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
                previewed: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("standalone-claude".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 120,
        };
        let runner_raw = Arc::new(MockRunner::new());
        let runner: Arc<dyn CommandRunner> = runner_raw.clone();
        let result = run_plan(
            &runner,
            &plan,
            Arc::new(VecSink::new()),
            3,
            CancellationToken::new(),
        )
        .await;
        assert!(
            matches!(result, Err(AdapterError::Refused(_))),
            "{result:?}"
        );
        assert!(runner_raw.calls().is_empty(), "nothing was spawned");
    }

    use crate::model::ArtifactKind;

    fn installed(kind: ArtifactKind, name: &str, version: &str) -> InstalledArtifact {
        InstalledArtifact {
            key: ArtifactKey {
                instance_id: "inst".to_string(),
                kind,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: version.to_string(),
            reason: crate::model::InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
            facts: Default::default(),
        }
    }

    fn key(kind: ArtifactKind, name: &str) -> ArtifactKey {
        ArtifactKey {
            instance_id: "inst".to_string(),
            kind,
            name: name.to_string(),
        }
    }

    #[test]
    fn test_reconcile_from_reports_the_installed_version_of_a_matching_key() {
        let artifacts = vec![
            installed(ArtifactKind::Package, "lodash", "4.17.21"),
            installed(ArtifactKind::Package, "typescript", "5.9.2"),
        ];
        assert_eq!(
            reconcile_from(artifacts, &key(ArtifactKind::Package, "typescript")),
            Reconciled {
                present: true,
                version: Some("5.9.2".to_string()),
            }
        );
    }

    #[test]
    fn test_reconcile_from_reports_absent_for_a_name_that_is_not_installed() {
        let artifacts = vec![installed(ArtifactKind::Package, "lodash", "4.17.21")];
        assert_eq!(
            reconcile_from(artifacts, &key(ArtifactKind::Package, "does-not-exist")),
            Reconciled {
                present: false,
                version: None,
            }
        );
    }

    #[test]
    fn test_reconcile_from_does_not_match_the_same_name_of_another_kind() {
        // The rule npm's hand-written copy had already lost.
        let artifacts = vec![installed(ArtifactKind::Formula, "python", "3.14.2")];
        assert_eq!(
            reconcile_from(artifacts, &key(ArtifactKind::Cask, "python")),
            Reconciled {
                present: false,
                version: None,
            }
        );
    }

    #[test]
    fn test_ensure_instance_match_accepts_a_request_for_the_instance_it_was_given() {
        use crate::model::{OpKind, OpRequest};
        let inst = crate::testing::manager_instance("npm", "npm:/opt/homebrew");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        assert!(ensure_instance_match(&req, &inst).is_ok());
    }

    #[test]
    fn test_ensure_instance_match_refuses_a_request_meant_for_another_instance() {
        use crate::model::{OpKind, OpRequest};
        let inst = crate::testing::manager_instance("npm", "npm:/opt/homebrew");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "npm:/usr/local".to_string(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        match ensure_instance_match(&req, &inst) {
            Err(AdapterError::Refused(message)) => {
                assert!(
                    message.contains("npm:/usr/local") && message.contains("npm:/opt/homebrew"),
                    "the refusal must name both instances, got {message}"
                );
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }
}
