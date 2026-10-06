use crate::events::{LogNote, Stream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub mod login_path;
pub mod mock;
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
