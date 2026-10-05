//! Bounded login-shell PATH discovery. The shell command is the existing
//! fix-path-env command; only PATH is accepted, and no process environment
//! is changed until the entire command has completed successfully.
use super::{CommandRunner, CommandSpec, OutputUse};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const DELIMITER: &str = "_SHELL_ENV_DELIMITER_";
const COMMAND: &str =
    "echo -n \"_SHELL_ENV_DELIMITER_\"; env; echo -n \"_SHELL_ENV_DELIMITER_\"; exit";

pub async fn read(
    runner: &dyn CommandRunner,
    shell: PathBuf,
    home: PathBuf,
    timeout: Duration,
) -> Option<String> {
    let output = runner
        .run(
            CommandSpec {
                program: shell,
                args: vec!["-ilc".into(), COMMAND.into()],
                env: vec![("DISABLE_AUTO_UPDATE".into(), "true".into())],
                cwd: Some(home),
                timeout,
                output_use: OutputUse::Parsed,
            },
            None,
            CancellationToken::new(),
        )
        .await
        .ok()?;
    if output.exit_code != Some(0) || output.timed_out || output.cancelled {
        return None;
    }
    let (_, rest) = output.stdout.split_once(DELIMITER)?;
    let (environment, _) = rest.split_once(DELIMITER)?;
    environment
        .lines()
        .find_map(|line| line.strip_prefix("PATH="))
        .filter(|path| !path.is_empty() && !path.chars().any(char::is_control))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{CommandOutput, MockRunner, RealRunner};

    #[tokio::test]
    async fn test_only_complete_successful_shell_output_supplies_a_path() {
        for (stdout, timed_out, expected) in [
            (
                "noise_SHELL_ENV_DELIMITER_PATH=/a:/b\nOTHER=x\n_SHELL_ENV_DELIMITER_tail",
                false,
                Some("/a:/b"),
            ),
            (
                "_SHELL_ENV_DELIMITER_PATH=/late\n_SHELL_ENV_DELIMITER_",
                true,
                None,
            ),
            ("PATH=/unframed", false, None),
            ("_SHELL_ENV_DELIMITER_PATH=/partial", false, None),
            (
                "_SHELL_ENV_DELIMITER_PATH=\n_SHELL_ENV_DELIMITER_",
                false,
                None,
            ),
        ] {
            let runner = MockRunner::new();
            runner.respond(
                vec!["/test-shell", "-ilc", COMMAND],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: stdout.into(),
                    stderr: String::new(),
                    timed_out,
                    cancelled: false,
                },
            );
            assert_eq!(
                read(
                    &runner,
                    "/test-shell".into(),
                    "/tmp".into(),
                    Duration::from_millis(20)
                )
                .await
                .as_deref(),
                expected
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_a_hung_shell_is_killed_and_returns_without_a_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("shell");
        // An isolated synthetic shell, never the user's startup files.
        std::fs::write(&shell, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            read(
                &RealRunner::new(),
                shell,
                dir.path().into(),
                Duration::from_millis(30),
            ),
        )
        .await
        .expect("bounded startup subprocess");
        assert_eq!(result, None);
    }
}
