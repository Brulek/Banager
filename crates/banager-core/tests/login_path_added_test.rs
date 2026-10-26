//! A folder put at the end of the `PATH` after the login shell was read
//! (`runner::login_path::add_folder`, the Homebrew hand-off's
//! `/opt/homebrew/bin`) reaches what Banager runs and looks along, before
//! any read of the login shell has worked and after, once. A file of its
//! own: `add_folder` and `accept` set state for the whole process, and each
//! file under tests/ runs in a process of its own.

use banager_core::runner::login_path;
use banager_core::runner::{CommandRunner, CommandSpec, OutputUse, RealRunner};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The `PATH` `/usr/bin/env` is handed by the runner.
async fn command_path() -> String {
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
    output
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("PATH=").map(str::to_string))
        .unwrap_or_default()
}

#[tokio::test]
async fn test_an_added_folder_ends_the_path_of_every_command_and_round_once() {
    let own = std::env::var("PATH").expect("the test runs with a PATH");
    let added = "/opt/test-homebrew/bin";
    // Neither a relative folder nor one with a `:` in its name: either
    // would be other folders, or a folder relative to where a command runs.
    login_path::add_folder(Path::new("test-homebrew/bin"));
    login_path::add_folder(Path::new("/opt/a:/opt/b"));
    assert_eq!(login_path::path().as_deref(), Some(own.as_ref()));

    // No read of the login shell has worked: the process's own, then it.
    login_path::add_folder(Path::new(added));
    login_path::add_folder(Path::new(added));
    let expected = format!("{own}:{added}");
    assert_eq!(login_path::path().as_deref(), Some(expected.as_ref()));
    let (env, known) = login_path::round_env();
    assert!(!known);
    assert_eq!(env.path_dirs.last(), Some(&PathBuf::from(added)));
    assert_eq!(command_path().await, expected);
    // The process's own environment is left as it was.
    assert_eq!(std::env::var("PATH").unwrap(), own);

    // Once a read works: the login shell's, then it -- and not twice where
    // the login shell already lists it.
    let login = "/opt/test-login/bin:/usr/bin:/bin";
    login_path::accept(&login_path::LoginEnv {
        path: login.to_string(),
        imported: Vec::new(),
        tilde_is_home: false,
    });
    let expected = format!("{login}:{added}");
    assert_eq!(login_path::accepted().as_deref(), Some(expected.as_ref()));
    let (env, known) = login_path::round_env();
    assert!(known);
    assert_eq!(
        env.path_dirs,
        ["/opt/test-login/bin", "/usr/bin", "/bin", added]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    );
    assert_eq!(command_path().await, expected);
    let listed = format!("{added}:/usr/bin:/bin");
    login_path::accept(&login_path::LoginEnv {
        path: listed.clone(),
        imported: Vec::new(),
        tilde_is_home: false,
    });
    assert_eq!(login_path::accepted().as_deref(), Some(listed.as_ref()));
}
