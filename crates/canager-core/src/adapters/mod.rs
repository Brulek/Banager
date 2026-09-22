use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, InstalledArtifact, InstanceNote, ManagerInstance, OpRequest, Outcome, Plan,
    Reconciled, SearchHit, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, LineCallback};
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
pub mod uv;

/// Options a caller passes down to `check_updates`. Adapters ignore fields
/// that do not apply to them; a new field must never change behaviour for an
/// adapter that does not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOptions {
    /// Homebrew only: include casks that update themselves (`brew outdated --greedy`).
    pub include_self_updating: bool,
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
    pub notes: Vec<InstanceNote>,
}

/// Lets the six adapters with nothing to report about themselves write
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
}

/// Percent-encodes one untrusted value for interpolation into a single URL
/// **path segment**.
///
/// Every registry lookup in this crate builds its URL by interpolating a
/// name Canager did not choose: Ollama's model references arrive in the body
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

/// One "Canager could not find out" row: the item is listed at the version
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
/// "unknown" target, and any other value would be a version Canager is
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
}

/// Runs a plan through the runner, streaming each line to the sink, and maps
/// the result the way every adapter must: a clean exit is `Succeeded`, a
/// cancelled or timed-out run is `Unconfirmed` (the operation may or may not
/// have taken effect — only `reconcile` can say), and a non-zero exit is
/// `Failed` carrying the last five stderr lines.
///
/// Every adapter's `execute()` is this function and nothing else. It lives
/// here so the cancelled/timed-out rule and the five-line summary can only
/// ever mean one thing; an earlier draft of this phase had six byte-identical
/// copies of it.
pub async fn run_plan(
    runner: &Arc<dyn CommandRunner>,
    plan: &Plan,
    sink: Arc<dyn EventSink>,
    op_id: OpId,
    cancel: CancellationToken,
) -> Result<Outcome, AdapterError> {
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
    let output = runner.run(spec, Some(on_line), cancel).await?;
    if output.cancelled || output.timed_out {
        return Ok(Outcome::Unconfirmed);
    }
    match output.exit_code {
        Some(0) => Ok(Outcome::Succeeded),
        code => {
            let stderr_lines: Vec<&str> = output.stderr.lines().collect();
            let start = stderr_lines.len().saturating_sub(5);
            Ok(Outcome::Failed {
                exit_code: code,
                summary: stderr_lines[start..].join("\n"),
            })
        }
    }
}

/// `"cargo 1.98.1 (…)"` -> `Some("1.98.1")`. Several tools (cargo, uv, pip)
/// print their version as the second whitespace-separated token of the first
/// line; this is that rule, once. Tools that print it differently — pipx's
/// bare `1.17.3`, Ollama's `ollama version is 0.34.1` — keep their own
/// parser.
pub fn second_token(text: &str) -> Option<String> {
    let mut parts = text.lines().next()?.split_whitespace();
    let _label = parts.next()?;
    parts.next().map(|token| token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_toml_parses_the_committed_brew_meta_file() {
        // cargo runs tests with cwd = the package manifest directory
        // (crates/canager-core), so this reaches the repo-root file.
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

    #[tokio::test]
    async fn test_run_plan_maps_a_cancelled_run_to_unconfirmed_and_a_failure_to_the_last_stderr_lines(
    ) {
        use crate::events::VecSink;
        use crate::model::{ArtifactKind, CancelPolicy, OpKind, OpRequest, ResourceLock};
        use crate::runner::{CommandOutput, MockRunner};
        use std::path::PathBuf;
        use tokio_util::sync::CancellationToken;

        fn plan_for(args: Vec<&str>) -> Plan {
            Plan {
                request: OpRequest {
                    kind: OpKind::Install,
                    instance_id: "fake:1".to_string(),
                    artifact_kind: ArtifactKind::Package,
                    name: "jq".to_string(),
                },
                program: PathBuf::from("/bin/fake"),
                args: args.into_iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
                needs_password: false,
                locks: vec![ResourceLock("fake:1".to_string())],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: Vec::new(),
                affected: Vec::new(),
                timeout_secs: 60,
            }
        }

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
}
