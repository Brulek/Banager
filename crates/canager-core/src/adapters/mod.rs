use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, InstalledArtifact, ManagerInstance, OpRequest, Outcome, Plan, Reconciled,
    SearchHit, UpdateCandidate,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, LineCallback};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub mod brew;
pub mod npm;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub search: bool,
    pub per_item_upgrade: bool,
    pub upgrade_all: bool,
    pub uninstall: bool,
    pub background_check: bool,
    pub cancel_safe: bool,
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
    fn capabilities(&self) -> Capabilities;
    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance>;
    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError>;
    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError>;
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
