//! A proxy's or mirror's login, read from the login shell and handed to
//! every command (U12), never reaches an operation's log, its failure
//! summary or a source's error, though tools print it back (F2 of the
//! decisions-round review; `runner::redact`).
//!
//! A file of its own: `login_path::accept` sets state for the whole
//! process, and each file under tests/ runs in a process of its own, so no
//! other test sees it. Every test here accepts the same settings.

use banager_core::adapters::run_plan;
use banager_core::events::{EventSink, OperationEvent, VecSink};
use banager_core::model::{
    ArtifactKind, CancelPolicy, OpKind, OpRequest, Outcome, Plan, PlanAction, ResourceLock,
};
use banager_core::runner::login_path::{self, LoginEnv};
use banager_core::runner::{
    CommandRunner, CommandSpec, LineCallback, OutputUse, RealRunner, RunLine,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// `https_proxy`: a proxy whose port curl and pip refuse, so both print it
/// back, login and all, before connecting anywhere.
const HTTPS_PROXY: &str = "http://review-user:review-secret@127.0.0.1:invalid";
/// `http_proxy`: a login written with no scheme, as curl also accepts --
/// and prints back as written, with no `scheme://` for a pattern to find --
/// with a `/` in its password, as written.
const HTTP_PROXY: &str = "review-user:rev/bare-secret@127.0.0.1:8080";
/// `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`: a `/`, `#` or `?` in the
/// password as written, not percent-encoded. curl 8.7.1 reads the address
/// as ending there, fails on the "port" and prints the setting back whole
/// (exit 5, on this Mac), the F2 review's reproduction.
const SLASH_PROXY: &str = "http://review-user:rev/secret@127.0.0.1:8080";
const HASH_PROXY: &str = "http://review-user:rev#secret@127.0.0.1:8080";
const QUERY_PROXY: &str = "http://review-user:rev?secret@127.0.0.1:8080";
/// A password that has to be percent-encoded in an address: `p@ss/word`.
const ALL_PROXY: &str = "socks5://someone:p%40ss%2Fword@127.0.0.1:7891";
/// A mirror whose login is an access token alone.
const MIRROR: &str = "https://ghp_mirrortoken42@mirror.example/homebrew-bottles";
/// A remote whose token stands where a user name would, GitHub's
/// documented `TOKEN:x-oauth-basic` form.
const REMOTE: &str = "https://ghp_reviewtoken42xoauth:x-oauth-basic@github.com/Homebrew/brew";

/// Every secret above, in each form a test below has a tool print: the
/// user names too, since a name can be a token (R1 of the decisions-round
/// re-check; `runner::redact`).
const SECRETS: &[&str] = &[
    "review-user",
    "someone",
    "review-secret",
    "rev/bare-secret",
    "rev/secret",
    "rev#secret",
    "rev?secret",
    "p%40ss%2Fword",
    "p@ss/word",
    "ghp_mirrortoken42",
    "ghp_reviewtoken42xoauth",
    // base64("review-user:review-secret"), as `curl -v` sends a proxy.
    "cmV2aWV3LXVzZXI6cmV2aWV3LXNlY3JldA==",
];

fn accept_settings_with_logins() {
    login_path::accept(&LoginEnv {
        path: "/usr/bin:/bin".to_string(),
        imported: vec![
            ("http_proxy".to_string(), HTTP_PROXY.to_string()),
            ("https_proxy".to_string(), HTTPS_PROXY.to_string()),
            ("all_proxy".to_string(), ALL_PROXY.to_string()),
            ("HTTP_PROXY".to_string(), SLASH_PROXY.to_string()),
            ("HTTPS_PROXY".to_string(), HASH_PROXY.to_string()),
            ("ALL_PROXY".to_string(), QUERY_PROXY.to_string()),
            ("HOMEBREW_BOTTLE_DOMAIN".to_string(), MIRROR.to_string()),
            ("HOMEBREW_BREW_GIT_REMOTE".to_string(), REMOTE.to_string()),
        ],
    });
}

fn assert_no_secret(what: &str, text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{what} holds {secret:?}:\n{text}");
    }
}

/// Prints, from the settings the command was handed, what curl and pip
/// print for them, the first line in two writes a moment apart (two
/// reads), and fails as curl does.
const TOOLS_SAY: &str = r#"
printf "curl: (5) Unsupported proxy syntax in '%s" "${https_proxy%%secret*}" >&2
sleep 0.2
printf "secret%s': Port number was not a decimal number between 0 and 65535\n" "${https_proxy#*secret}" >&2
printf "curl: (5) Unsupported proxy syntax in '%s': Port number was not a decimal number between 0 and 65535\n" "$http_proxy" >&2
printf "pip._vendor.urllib3.exceptions.LocationParseError: Failed to parse: %s\n" "$https_proxy" >&2
printf "fatal: unable to access '%s/': The requested URL returned error: 403\n" "$HOMEBREW_BREW_GIT_REMOTE" >&2
printf "==> Downloading %s/jq-1.8.1.bottle.tar.gz\n" "$HOMEBREW_BOTTLE_DOMAIN"
printf "proxy %s refused the login for p@ss/word\n" "$all_proxy"
printf "> Proxy-Authorization: Basic cmV2aWV3LXVzZXI6cmV2aWV3LXNlY3JldA==\n"
for proxy in "$HTTP_PROXY" "$HTTPS_PROXY" "$ALL_PROXY"; do
  printf "curl: (5) Unsupported proxy syntax in '%s': Port number was not a decimal number between 0 and 65535\n" "$proxy" >&2
done
printf "pip._vendor.requests.exceptions.InvalidURL: Failed to parse: %s\n" "$https_proxy" >&2
exit 5
"#;

#[tokio::test]
async fn test_a_failed_operation_logs_and_summarises_tool_output_with_the_logins_masked() {
    accept_settings_with_logins();
    let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::new());
    let plan = Plan {
        request: OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "brew".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        },
        action: PlanAction::Command {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_string(), TOOLS_SAY.to_string()],
            env: vec![],
        },
        needs_password: false,
        locks: vec![ResourceLock("brew".to_string())],
        cancel_policy: CancelPolicy::KillThenReconcile,
        warnings: vec![],
        affected: vec![],
        timeout_secs: 30,
    };
    let sink = Arc::new(VecSink::new());
    let outcome = run_plan(
        &runner,
        &plan,
        sink.clone() as Arc<dyn EventSink>,
        7,
        CancellationToken::new(),
    )
    .await
    .expect("the plan runs");

    let lines: Vec<String> = sink
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            OperationEvent::Log { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 11, "{lines:#?}");
    for line in &lines {
        assert_no_secret("a log line", line);
    }
    // The command was handed the settings as they are -- the login is
    // masked in what it printed, not taken out of what it was given.
    let expected = [
        "curl: (5) Unsupported proxy syntax in 'http://****:****@127.0.0.1:invalid': Port number was not a decimal number between 0 and 65535",
        "curl: (5) Unsupported proxy syntax in '****:****@127.0.0.1:8080': Port number was not a decimal number between 0 and 65535",
        "curl: (5) Unsupported proxy syntax in 'http://****:****@127.0.0.1:8080': Port number was not a decimal number between 0 and 65535",
        "pip._vendor.urllib3.exceptions.LocationParseError: Failed to parse: http://****:****@127.0.0.1:invalid",
        "pip._vendor.requests.exceptions.InvalidURL: Failed to parse: http://****:****@127.0.0.1:invalid",
        "==> Downloading https://****@mirror.example/homebrew-bottles/jq-1.8.1.bottle.tar.gz",
        "proxy socks5://****:****@127.0.0.1:7891 refused the login for ****",
        "> Proxy-Authorization: Basic ****",
        "fatal: unable to access 'https://****:****@github.com/Homebrew/brew/': The requested URL returned error: 403",
    ];
    for line in expected {
        assert!(
            lines.iter().any(|seen| seen == line),
            "missing {line:?} in {lines:#?}"
        );
    }

    let Outcome::Failed { exit_code, summary } = outcome else {
        panic!("expected a failure, got {outcome:?}");
    };
    assert_eq!(exit_code, Some(5));
    assert_no_secret("the failure summary", &summary);
    // The last five lines: git's, curl's for the three proxies with a
    // `/`, `#` or `?` in the password, and pip's.
    assert_eq!(summary.lines().count(), 5, "{summary}");
    assert!(
        summary.contains("'https://****:****@github.com/Homebrew/brew/'"),
        "{summary}"
    );
    assert!(
        summary.contains("//****:****@127.0.0.1:invalid"),
        "{summary}"
    );
    assert_eq!(
        summary.matches("'http://****:****@127.0.0.1:8080'").count(),
        3,
        "{summary}"
    );
}

#[tokio::test]
async fn test_curls_own_error_for_a_proxy_with_a_login_is_masked() {
    // The finding's own reproduction, through the runner: curl, handed the
    // proxy setting, refuses its port before connecting anywhere and
    // prints the setting back, login and all (curl 8.7.1, this Mac's
    // `/usr/bin/curl`: exit 5, "Unsupported proxy syntax in '…'").
    accept_settings_with_logins();
    let curl = PathBuf::from("/usr/bin/curl");
    if !curl.exists() {
        eprintln!("no /usr/bin/curl here; nothing to run");
        return;
    }
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = seen.clone();
    let on_line: LineCallback = Arc::new(move |run_line| {
        if let RunLine::Output(_, line) = run_line {
            seen_cb.lock().unwrap().push(line);
        }
    });
    let output = RealRunner::new()
        .run(
            CommandSpec {
                program: curl,
                args: ["--head", "--max-time", "1", "https://127.0.0.1:9"]
                    .map(String::from)
                    .to_vec(),
                env: vec![],
                cwd: None,
                timeout: Duration::from_secs(10),
                output_use: OutputUse::Transcript,
            },
            Some(on_line),
            CancellationToken::new(),
        )
        .await
        .expect("curl runs");
    assert_ne!(output.exit_code, Some(0), "{output:?}");
    assert!(!output.stderr.trim().is_empty(), "curl said nothing");
    assert_no_secret("curl's stderr", &output.stderr);
    for line in seen.lock().unwrap().iter() {
        assert_no_secret("a line of curl's", line);
    }
    if output.stderr.contains("Unsupported proxy syntax") {
        assert!(
            output
                .stderr
                .contains("'http://****:****@127.0.0.1:invalid'"),
            "{}",
            output.stderr
        );
    }
}

#[tokio::test]
async fn test_the_login_shell_read_still_gets_the_settings_as_written() {
    // The read that imports the settings is a parser's stdout, never
    // masked: a mask there would hand every command `****` as its proxy's
    // password. Its "shell" here prints the environment it was handed,
    // framed as `login_path::read` expects, the settings included.
    accept_settings_with_logins();
    let dir = std::env::temp_dir().join(format!("banager-redact-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let shell = dir.join("shell");
    std::fs::write(
        &shell,
        "#!/bin/sh\nprintf '_SHELL_ENV_DELIMITER_'; /usr/bin/env; printf '_SHELL_ENV_DELIMITER_'\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();

    let found = login_path::read(
        &RealRunner::new(),
        shell,
        dir.clone(),
        Duration::from_secs(10),
    )
    .await
    .expect("the read works");
    let value = |name: &str| {
        found
            .imported
            .iter()
            .find(|(imported, _)| imported == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(value("https_proxy"), Some(HTTPS_PROXY));
    assert_eq!(value("http_proxy"), Some(HTTP_PROXY));
    assert_eq!(value("all_proxy"), Some(ALL_PROXY));
    assert_eq!(value("HTTP_PROXY"), Some(SLASH_PROXY));
    assert_eq!(value("HTTPS_PROXY"), Some(HASH_PROXY));
    assert_eq!(value("ALL_PROXY"), Some(QUERY_PROXY));
    assert_eq!(value("HOMEBREW_BOTTLE_DOMAIN"), Some(MIRROR));
    assert_eq!(value("HOMEBREW_BREW_GIT_REMOTE"), Some(REMOTE));
    let _ = std::fs::remove_dir_all(&dir);
}
