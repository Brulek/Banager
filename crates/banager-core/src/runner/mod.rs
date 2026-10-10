use crate::events::{LogNote, Stream};
use crate::history::{operation_failure_cause, FailureCause};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub mod login_path;
pub mod mock;
pub mod no_answer;
pub mod path_env;
pub mod real;
pub mod redact;

pub use mock::MockRunner;
pub use path_env::{resolve_exe, HostEnv};
pub use real::RealRunner;

/// What the caller is going to do with the bytes this command writes to
/// stdout, which decides what a runner may do when there are too many of
/// them to hold.
///
/// The two answers need opposite handling and there is no safe default,
/// so this is a field every `CommandSpec` must fill in: adding it made
/// the compiler walk every call site in the workspace and ask, which is
/// the whole point of it being a field rather than something inferred.
///
/// It is deliberately *not* derived from whether a `LineCallback` was
/// passed. A callback says where the lines are *sent*; it says nothing
/// about what the caller does with `CommandOutput.stdout` afterwards, and
/// a future adapter that streams progress to the log drawer *and* parses
/// the final stdout (an `ollama pull` shape) would silently get the
/// eliding buffer. The pairing happens to hold today, which is exactly
/// what makes relying on it easy to break without noticing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputUse {
    /// A log a person reads. Keeping the middle of a runaway build log is
    /// worth less than keeping the app alive, so a runner may drop it and
    /// say in the text that it did.
    Transcript,
    /// Bytes handed to a parser (`serde_json`, a version string). Eliding
    /// the middle of these does not shorten the data, it corrupts it: a
    /// spliced JSON document fails to parse and the source vanishes from
    /// the app. The only honest answers are all of the bytes, or
    /// [`RunnerError::OutputTooLarge`].
    Parsed,
}

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    /// See [`OutputUse`]. Decides whether a stdout too large to hold is
    /// shortened or refused.
    pub output_use: OutputUse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    /// The command's exit code; `None` if it was ended by a signal or
    /// stopped by the runner.
    pub exit_code: Option<i32>,
    /// As written for an [`OutputUse::Parsed`] command; for an
    /// [`OutputUse::Transcript`] one, with a proxy's or mirror's login
    /// masked, as in `stderr` (`redact`).
    pub stdout: String,
    /// With a proxy's or mirror's login masked (`redact`): it is a
    /// message for a person on every path.
    pub stderr: String,
    /// The runner stopped the command because `spec.timeout` passed.
    ///
    /// Both this and `cancelled` mean the command did not finish. A command
    /// that exits 0 during the stop's grace period (after SIGTERM, before
    /// any SIGKILL) finished its work, and is reported as it would have been
    /// without the stop: both flags false and `exit_code: Some(0)`.
    pub timed_out: bool,
    /// The runner stopped the command because the cancellation token fired.
    /// See `timed_out` for a command that finishes while being stopped.
    pub cancelled: bool,
    /// Why the command failed, by the last lines it wrote to stderr as it
    /// wrote them (`CommandOutput::failure_cause`). `RealRunner` reads it
    /// before it masks a login (`StderrCause::Read`), since the mask can
    /// take the words that say it: a proxy password `pass` turns sudo's
    /// "a password is required" into "a ****word is required" (re-check
    /// 2's N1). A runner that masks nothing leaves it to be read off
    /// `stderr` (`StderrCause::InStderr`, the default).
    pub stderr_cause: StderrCause,
}

/// How many of the last lines a failed command wrote to stderr are its
/// failure's summary (`Outcome::Failed`, `run_plan`), and are read for why
/// it failed (`history::operation_failure_cause`).
pub const SUMMARY_LINES: usize = 5;

/// A failed command's summary: the last [`SUMMARY_LINES`] lines of its
/// stderr.
pub fn failure_summary(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    let start = lines.len().saturating_sub(SUMMARY_LINES);
    lines[start..].join("\n")
}

/// Where a command's failure cause is: see [`CommandOutput::stderr_cause`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum StderrCause {
    /// `stderr` is as the command wrote it, so the cause is read off its
    /// summary (`failure_summary`). A runner that masks nothing says this.
    #[default]
    InStderr,
    /// Read by the runner off the summary's lines as the command wrote
    /// them, before any login was masked out of `stderr`: why it failed
    /// (`history::operation_failure_cause`), and the program `env` said it could not
    /// find (`no_answer::missing_program`) -- a proxy user name `node`
    /// would mask the very word. `RealRunner` always says this.
    Read {
        cause: Option<FailureCause>,
        missing_program: Option<String>,
    },
}

impl CommandOutput {
    /// Why the command failed, by the last lines it wrote to stderr
    /// (`history::operation_failure_cause` over `failure_summary`), as it
    /// wrote them. An operation's failure is read with it (`run_plan`), and
    /// so is the command a source did not answer at startup
    /// (`no_answer::of`, `NoAnswer::cause`); a lookup's words are read for
    /// the network alone (`adapters::says_network_failed`,
    /// `lookupFailureCause` in src/lib/failureCause.ts).
    pub fn failure_cause(&self) -> Option<FailureCause> {
        match &self.stderr_cause {
            StderrCause::InStderr => operation_failure_cause(&failure_summary(&self.stderr)),
            StderrCause::Read { cause, .. } => *cause,
        }
    }

    /// The program the command's launcher needed and `env` did not find on
    /// `PATH`, by the last lines it wrote to stderr
    /// (`no_answer::missing_program` over `failure_summary`), as it wrote
    /// them: `node`, for npm's `#!/usr/bin/env node` with no `node` there.
    pub fn missing_program(&self) -> Option<String> {
        match &self.stderr_cause {
            StderrCause::InStderr => no_answer::missing_program(&failure_summary(&self.stderr)),
            StderrCause::Read {
                missing_program, ..
            } => missing_program.clone(),
        }
    }
}

/// One thing a runner hands its [`LineCallback`] while a command runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunLine {
    /// A line the command itself wrote, as it wrote it but for a proxy's
    /// or mirror's login, masked as `****` (`redact`, F2 of the
    /// decisions-round review).
    Output(Stream, String),
    /// A remark of the runner's own about the run -- never the command's
    /// words, and never text: the front end localises it. See [`LogNote`].
    Note(LogNote),
}

pub type LineCallback = Arc<dyn Fn(RunLine) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("program not found: {0}")]
    NotFound(PathBuf),
    #[error("spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("no canned response for {0:?}")]
    NoMock(Vec<String>),
    /// An [`OutputUse::Parsed`] command wrote more to stdout than a runner
    /// is willing to hold. Loud on purpose: the alternative -- handing a
    /// parser a shortened copy of its input -- is the same failure
    /// wearing a success's clothes.
    #[error("the command wrote more than {limit} bytes to stdout, which is more than Banager will read from output it has to parse")]
    OutputTooLarge { limit: usize },
}

#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    /// Runs `spec` to completion, or until `cancel` fires or `spec.timeout`
    /// passes.
    ///
    /// A cancel or a timeout stops the command gracefully: `RealRunner`
    /// sends its whole process group SIGTERM, gives it a few seconds to
    /// clean up and exit, and SIGKILLs only what is still there after that.
    ///
    /// **Dropping the returned future kills the command outright.** If the
    /// future is dropped before it resolves -- by a `select!` or `timeout`
    /// around it, by aborting the task awaiting it, or by dropping any
    /// future that contains it, such as a `Session::refresh` -- `RealRunner`
    /// SIGKILLs the command's whole process group at whatever point it has
    /// reached, with no SIGTERM and no grace period: `Drop` cannot await,
    /// so it can neither wait one out nor wait for the group to die, and
    /// "killed" here is not yet "dead".
    ///
    /// That is the right thing for a command whose work is worthless if
    /// interrupted and harmless to interrupt -- a query, a download -- and
    /// the wrong thing for one that must not stop halfway. `brew update`
    /// rewrites a git checkout, and a kill mid-way can leave
    /// `.git/index.lock` behind and Homebrew refusing to update until the
    /// file is deleted by hand. A command like that must not be run in a
    /// future anything might drop: run it in a task of its own and wait on
    /// that task's `JoinHandle`, which detaches rather than aborts when
    /// dropped (see `BrewAdapter::maybe_update`).
    ///
    /// `MockRunner` does not model this: dropping its future just stops
    /// waiting, and nothing is killed.
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<CommandOutput, RunnerError>;
}
