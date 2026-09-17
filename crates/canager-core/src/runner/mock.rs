use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use crate::events::Stream;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct MockRunner {
    responses: Mutex<HashMap<Vec<String>, CommandOutput>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl MockRunner {
    pub fn new() -> MockRunner {
        MockRunner {
            responses: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn argv(spec: &CommandSpec) -> Vec<String> {
        let mut v = vec![spec.program.to_string_lossy().to_string()];
        v.extend(spec.args.iter().cloned());
        v
    }

    /// `argv` includes the program path as its first element, exactly as the
    /// caller built the `CommandSpec` — lookup is by exact argv match.
    pub fn respond(&self, argv: Vec<&str>, output: CommandOutput) {
        let key: Vec<String> = argv.into_iter().map(|s| s.to_string()).collect();
        self.responses.lock().unwrap().insert(key, output);
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for MockRunner {
    fn default() -> Self {
        MockRunner::new()
    }
}

#[async_trait]
impl CommandRunner for MockRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        _cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let key = Self::argv(&spec);
        self.calls.lock().unwrap().push(key.clone());
        let output = self
            .responses
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| RunnerError::NoMock(key))?;
        if let Some(cb) = on_line {
            for line in output.stdout.split('\n') {
                if !line.is_empty() {
                    cb(Stream::Stdout, line.to_string());
                }
            }
            for line in output.stderr.split('\n') {
                if !line.is_empty() {
                    cb(Stream::Stderr, line.to_string());
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_runner_returns_canned_output_and_records_argv() {
        let runner = MockRunner::new();
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 7.0.3\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );

        let spec = CommandSpec {
            program: std::path::PathBuf::from("/opt/homebrew/bin/brew"),
            args: vec!["--version".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("mocked call");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "Homebrew 7.0.3\n");
        assert_eq!(
            runner.calls(),
            vec![vec![
                "/opt/homebrew/bin/brew".to_string(),
                "--version".to_string()
            ]]
        );
    }

    #[tokio::test]
    async fn test_mock_runner_errors_on_unconfigured_argv() {
        let runner = MockRunner::new();
        let spec = CommandSpec {
            program: std::path::PathBuf::from("/opt/homebrew/bin/brew"),
            args: vec!["update".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let result = runner.run(spec, None, CancellationToken::new()).await;
        assert!(matches!(result, Err(RunnerError::NoMock(_))));
    }
}
