use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunLine, RunnerError};
use crate::events::Stream;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub struct MockRunner {
    responses: Mutex<HashMap<Vec<String>, CommandOutput>>,
    delays: Mutex<HashMap<Vec<String>, Duration>>,
    calls: Mutex<Vec<CommandSpec>>,
    /// What `command_setting` answers where the spec sets nothing itself
    /// (`set_setting`): the stand-in for the login shell's read.
    settings: Mutex<Vec<(String, String)>>,
}

impl MockRunner {
    pub fn new() -> MockRunner {
        MockRunner {
            responses: Mutex::new(HashMap::new()),
            delays: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            settings: Mutex::new(Vec::new()),
        }
    }

    /// Sets, or with `None` unsets, the value `name` has for every command
    /// this runner runs (`CommandRunner::command_setting`), as the login
    /// shell's read would.
    pub fn set_setting(&self, name: &str, value: Option<&str>) {
        let mut settings = self.settings.lock().unwrap();
        settings.retain(|(set, _)| set != name);
        if let Some(value) = value {
            settings.push((name.to_string(), value.to_string()));
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

    /// Makes the canned response for `argv` (registered via `respond`)
    /// available only after `delay` has elapsed, so a test can observe what
    /// happens *while* a call is still in flight — e.g. proving two
    /// concurrent callers serialise on a lock rather than both proceeding
    /// immediately.
    pub fn delay(&self, argv: Vec<&str>, delay: Duration) {
        let key: Vec<String> = argv.into_iter().map(|s| s.to_string()).collect();
        self.delays.lock().unwrap().insert(key, delay);
    }

    /// Full dispatched commands, including the environment the real runner receives.
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.calls.lock().unwrap().clone()
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().iter().map(Self::argv).collect()
    }
}

impl Default for MockRunner {
    fn default() -> Self {
        MockRunner::new()
    }
}

#[async_trait]
impl CommandRunner for MockRunner {
    fn command_setting(&self, name: &str, env: &[(String, String)]) -> Option<String> {
        let settings = self.settings.lock().unwrap().clone();
        super::login_path::command_setting(name, None, env, &|name| {
            settings
                .iter()
                .find(|(set, _)| set == name)
                .map(|(_, value)| value.clone())
        })
    }

    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        _cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let key = Self::argv(&spec);
        self.calls.lock().unwrap().push(spec);
        let delay = self.delays.lock().unwrap().get(&key).copied();
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
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
                    cb(RunLine::Output(Stream::Stdout, line.to_string()));
                }
            }
            for line in output.stderr.split('\n') {
                if !line.is_empty() {
                    cb(RunLine::Output(Stream::Stderr, line.to_string()));
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::OutputUse;

    #[tokio::test]
    async fn test_mock_runner_returns_canned_output_and_records_argv() {
        let runner = MockRunner::new();
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
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
            env: vec![("HOMEBREW_NO_AUTOREMOVE".into(), "1".into())],
            cwd: Some("/tmp/test-owned".into()),
            timeout: std::time::Duration::from_secs(5),
            output_use: OutputUse::Parsed,
        };
        let output = runner
            .run(spec.clone(), None, CancellationToken::new())
            .await
            .expect("mocked call");

        let recorded = runner.specs();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].program, spec.program);
        assert_eq!(recorded[0].args, spec.args);
        assert_eq!(recorded[0].env, spec.env);
        assert_eq!(recorded[0].cwd, spec.cwd);
        assert_eq!(recorded[0].timeout, spec.timeout);
        assert_eq!(recorded[0].output_use, spec.output_use);
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
            output_use: OutputUse::Parsed,
        };
        let result = runner.run(spec, None, CancellationToken::new()).await;
        assert!(matches!(result, Err(RunnerError::NoMock(_))));
    }
}
