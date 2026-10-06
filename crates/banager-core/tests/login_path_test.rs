//! The login shell's `PATH`, once read, reaches what Banager runs and
//! looks for without the process environment being changed
//! (`runner::login_path::accept`); so do the proxy and mirror settings
//! read with it (U12), and none of their values reaches the diagnostics.
//! A file of its own: `accept` sets state for the whole process, and each
//! file under tests/ runs in a process of its own, so no other test sees
//! it.

use banager_core::runner::login_path;
use banager_core::runner::{CommandRunner, CommandSpec, HostEnv, OutputUse, RealRunner};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_an_accepted_login_path_reaches_commands_and_discovery_but_not_the_environment() {
    let before = std::env::var_os("PATH");
    let proxy_before = std::env::var_os("https_proxy");
    let mirror_before = std::env::var_os("HOMEBREW_BOTTLE_DOMAIN");
    // Folders under no home, so the diagnostics show them as they are;
    // `/usr/bin` and `/bin` so that `env` itself still runs.
    let login = "/opt/test-login/bin:/usr/bin:/bin";
    assert_eq!(login_path::accepted(), None);
    // Before any read worked, a round goes along the process's own PATH
    // and knows it is not the login shell's -- one value, one look.
    let (env, known) = login_path::round_env();
    assert!(!known);
    assert_eq!(env, HostEnv::discover_along(before.clone()));
    let proxy = "http://someone:u12-secret@127.0.0.1:7890";
    let mirror = "https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles";
    login_path::accept(&login_path::LoginEnv {
        path: login.to_string(),
        imported: vec![
            ("https_proxy".to_string(), proxy.to_string()),
            ("HOMEBREW_BOTTLE_DOMAIN".to_string(), mirror.to_string()),
            (
                "PIP_INDEX_URL".to_string(),
                "https://pypi.example/simple".to_string(),
            ),
        ],
    });
    let (env, known) = login_path::round_env();
    assert!(known);
    assert_eq!(
        env.path_dirs,
        ["/opt/test-login/bin", "/usr/bin", "/bin"]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    );

    // Every command gets it as its `PATH`, and the settings read with it
    // as they were -- but a variable the command's own spec sets wins.
    let output = RealRunner::new()
        .run(
            CommandSpec {
                program: PathBuf::from("/usr/bin/env"),
                args: Vec::new(),
                env: vec![(
                    "PIP_INDEX_URL".to_string(),
                    "https://the-spec-wins.example/simple".to_string(),
                )],
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
    for expected in [
        format!("https_proxy={proxy}"),
        format!("HOMEBREW_BOTTLE_DOMAIN={mirror}"),
        "PIP_INDEX_URL=https://the-spec-wins.example/simple".to_string(),
    ] {
        assert!(
            output.stdout.lines().any(|line| line == expected),
            "{expected} is not in {}",
            output.stdout
        );
    }
    assert!(!output.stdout.contains("pypi.example"), "{}", output.stdout);
    // Banager's own requests take their proxy from the same place.
    assert_eq!(
        login_path::command_var("https_proxy").as_deref(),
        Some(proxy)
    );

    // Sources are looked for along it, and the diagnostics say it.
    let expected: Vec<PathBuf> = ["/opt/test-login/bin", "/usr/bin", "/bin"]
        .iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(HostEnv::discover().path_dirs, expected);
    let facts = banager_core::diagnostics::current(true, &[]);
    assert_eq!(facts.path_dirs, ["/opt/test-login/bin", "/usr/bin", "/bin"]);
    // ... and never a setting's value: a proxy's can hold a password.
    let json = serde_json::to_string(&facts).unwrap();
    for value in ["u12-secret", "7890", "tuna", "pypi.example"] {
        assert!(!json.contains(value), "{value} is in {json}");
    }

    // The process environment is as it was: nothing set it while other
    // threads may read it.
    assert_eq!(std::env::var_os("PATH"), before);
    assert_eq!(std::env::var_os("https_proxy"), proxy_before);
    assert_eq!(std::env::var_os("HOMEBREW_BOTTLE_DOMAIN"), mirror_before);
}
