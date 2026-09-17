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

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
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
}

#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<CommandOutput, RunnerError>;
}
