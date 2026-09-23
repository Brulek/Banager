use crate::events::Stream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub mod mock;
pub mod path_env;
pub mod real;

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
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
}

pub type LineCallback = Arc<dyn Fn(Stream, String) + Send + Sync>;

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
    #[error("the command wrote more than {limit} bytes to stdout, which is more than Canager will read from output it has to parse")]
    OutputTooLarge { limit: usize },
}

#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    /// Runs `spec` to completion, or until `cancel` fires or `spec.timeout`
    /// passes.
    ///
    /// **Dropping the returned future kills the command.** If the future is
    /// dropped before it resolves -- by a `select!` or `timeout` around it,
    /// by aborting the task awaiting it, or by dropping any future that
    /// contains it, such as a `Session::refresh` -- `RealRunner` SIGKILLs
    /// the command's whole process group at whatever point it has reached.
    /// It does not wait for the group to die, since `Drop` cannot await, so
    /// "killed" here is not yet "dead". A cancel and a timeout end in the
    /// same SIGKILL; dropping only makes it arrive with no warning to the
    /// caller.
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
