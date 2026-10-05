//! The login shell's `PATH`, once read, reaches what Banager runs and
//! looks for without the process environment being changed
//! (`runner::login_path::accept`). A file of its own: `accept` sets state
//! for the whole process, and each file under tests/ runs in a process of
//! its own, so no other test sees it.

use banager_core::runner::login_path;
use banager_core::runner::{CommandRunner, CommandSpec, HostEnv, OutputUse, RealRunner};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_an_accepted_login_path_reaches_commands_and_discovery_but_not_the_environment() {
    let before = std::env::var_os("PATH");
    // Folders under no home, so the diagnostics show them as they are;
    // `/usr/bin` and `/bin` so that `env` itself still runs.
    let login = "/opt/test-login/bin:/usr/bin:/bin";
    assert_eq!(login_path::accepted(), None);
    // Before any read worked, a round goes along the process's own PATH
    // and knows it is not the login shell's -- one value, one look.
    let (env, known) = login_path::round_env();
    assert!(!known);
    assert_eq!(env, HostEnv::discover_along(before.clone()));
    login_path::accept(login);
    let (env, known) = login_path::round_env();
    assert!(known);
    assert_eq!(
        env.path_dirs,
        ["/opt/test-login/bin", "/usr/bin", "/bin"]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    );

    // Every command gets it as its `PATH`.
    let output = RealRunner::new()
        .run(
            CommandSpec {
                program: PathBuf::from("/usr/bin/env"),
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
                timeout: Duration::from_secs(10),
                output_use: OutputUse::Parsed,
            },
            None,
            CancellationToken::new(),
        )
        .await
        .expect("env runs");
    assert_eq!(output.exit_code, Some(0));
    assert!(
        output
            .stdout
            .lines()
            .any(|line| line == format!("PATH={login}")),
        "{}",
        output.stdout
    );

    // Sources are looked for along it, and the diagnostics say it.
    let expected: Vec<PathBuf> = ["/opt/test-login/bin", "/usr/bin", "/bin"]
        .iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(HostEnv::discover().path_dirs, expected);
    assert_eq!(
        banager_core::diagnostics::current(true, &[]).path_dirs,
        ["/opt/test-login/bin", "/usr/bin", "/bin"]
    );

    // The process environment is as it was: nothing set it while other
    // threads may read it.
    assert_eq!(std::env::var_os("PATH"), before);
}
