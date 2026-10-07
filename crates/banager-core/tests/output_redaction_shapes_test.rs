//! The shapes of login the decisions-round re-check found leaking (R1, R2)
//! or masked where there was none (R3), end to end: settings from the
//! login shell, a command printing what git, curl, npm and pip printed for
//! them on this Mac, and what reaches the operation's log, its failure
//! summary and the command's output (`runner::redact`).
//!
//! A file of its own: `login_path::accept` sets state for the whole
//! process, and each file under tests/ runs in a process of its own, so no
//! other test sees these settings.

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

/// `http_proxy`, R2: written with no scheme, its password holding `://`.
/// curl 8.7.1 printed it back whole (exit 5, before connecting anywhere).
const R2_PROXY: &str = "review-user:rev://secret@127.0.0.1:invalid";
/// `https_proxy`: written with no scheme, a token for its user name.
/// npm 10.9.9 printed the name as the proxy's "protocol".
const TOKEN_PROXY: &str = "proxytokenabcdefghijklmn:x-oauth-basic@127.0.0.1:8080";
/// `all_proxy`: a password that has to be percent-encoded, `p@ss/word`;
/// pip 26.2.1 printed the setting as written.
const ENCODED_PROXY: &str = "http://someone:p%40ss%2Fword@127.0.0.1:invalid";
/// `HOMEBREW_BREW_GIT_REMOTE`, R1: a token of letters alone in the user
/// name slot, GitHub's `TOKEN:x-oauth-basic` form. git 2.54 asks again for
/// its password naming it before an `@`.
const R1_REMOTE: &str = "https://abcdefghijklmnopqrstuvwx:x-oauth-basic@github.com/Homebrew/brew";
/// `HOMEBREW_CORE_GIT_REMOTE`: a JWT-shaped token alone, dots and all.
const JWT_REMOTE: &str =
    "https://eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJyZXZpZXcifQ.c2lnbmF0dXJl@github.com/Homebrew/homebrew-core";
/// `PIP_INDEX_URL`, R3: no login, an address in its path after a port.
const R3_MIRROR: &str = "https://mirror.example:8443/x/user@example.com/simple";

/// Every secret above, and the halves of those a command prints in two
/// writes.
const SECRETS: &[&str] = &[
    "review-user",
    "secret",
    "proxytoken",
    "abcdefghijklmn",
    "someone",
    "p%40ss",
    "p@ss",
    "abcdefghijkl",
    "mnopqrstuvwx",
    "eyJhbGciOiJIUzI1NiJ9",
    "eyJzdWIiOiJyZXZpZXcifQ",
    "c2lnbmF0dXJl",
];

fn accept_the_settings() {
    login_path::accept(&LoginEnv {
        path: "/usr/bin:/bin".to_string(),
        imported: vec![
            ("http_proxy".to_string(), R2_PROXY.to_string()),
            ("https_proxy".to_string(), TOKEN_PROXY.to_string()),
            ("all_proxy".to_string(), ENCODED_PROXY.to_string()),
            (
                "HOMEBREW_BREW_GIT_REMOTE".to_string(),
                R1_REMOTE.to_string(),
            ),
            (
                "HOMEBREW_CORE_GIT_REMOTE".to_string(),
                JWT_REMOTE.to_string(),
            ),
            ("PIP_INDEX_URL".to_string(), R3_MIRROR.to_string()),
        ],
    });
}

fn assert_no_secret(what: &str, text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{what} holds {secret:?}:\n{text}");
    }
}

/// Prints, from the settings it was handed, what pip, npm, git and curl
/// printed for them, git's and curl's lines each in two writes a moment
/// apart (two reads) that cut the secret in half, and fails as curl does.
/// The last five lines on stderr are the failure's summary.
const TOOLS_SAY: &str = r#"
printf "Looking in indexes: %s\n" "$PIP_INDEX_URL"
printf "npm error Invalid protocol \`%s:\` connecting to proxy \`\`\n" "${https_proxy%%:*}" >&2
printf "pip._vendor.requests.exceptions.InvalidURL: Failed to parse: %s\n" "$all_proxy" >&2
printf "fatal: unable to access '%s/': The requested URL returned error: 403\n" "$HOMEBREW_CORE_GIT_REMOTE" >&2
token="${HOMEBREW_BREW_GIT_REMOTE#https://}"
token="${token%%:*}"
printf "fatal: could not read Password for 'https://%s" "${token%%mnop*}" >&2
sleep 0.2
printf "mnop%s@github.com': terminal prompts disabled\n" "${token#*mnop}" >&2
printf "curl: (5) Unsupported proxy syntax in '%s" "${http_proxy%%//secret*}" >&2
sleep 0.2
printf "//secret%s': Port number was not a decimal number between 0 and 65535\n" "${http_proxy#*//secret}" >&2
exit 5
"#;

/// What each line of `TOOLS_SAY` reads masked, in order.
const MASKED: &[&str] = &[
    "Looking in indexes: https://mirror.example:8443/x/user@example.com/simple",
    "npm error Invalid protocol `****:` connecting to proxy ``",
    "pip._vendor.requests.exceptions.InvalidURL: Failed to parse: http://****:****@127.0.0.1:invalid",
    "fatal: unable to access 'https://****@github.com/Homebrew/homebrew-core/': The requested URL returned error: 403",
    "fatal: could not read Password for 'https://****@github.com': terminal prompts disabled",
    "curl: (5) Unsupported proxy syntax in '****:****@127.0.0.1:invalid': Port number was not a decimal number between 0 and 65535",
];

fn tools_say() -> CommandSpec {
    CommandSpec {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".to_string(), TOOLS_SAY.to_string()],
        env: vec![],
        cwd: None,
        timeout: Duration::from_secs(30),
        output_use: OutputUse::Transcript,
    }
}

#[tokio::test]
async fn test_each_shape_is_masked_in_the_log_and_the_failure_summary() {
    accept_the_settings();
    let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::without_this_macs_settings());
    let spec = tools_say();
    let plan = Plan {
        request: OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "brew".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        },
        action: PlanAction::Command {
            program: spec.program,
            args: spec.args,
            env: vec![],
        },
        needs_password: false,
        locks: vec![ResourceLock("brew".to_string())],
        cancel_policy: CancelPolicy::KillThenReconcile,
        warnings: vec![],
        affected: vec![],
        basis: None,
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

    let mut lines: Vec<String> = sink
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            OperationEvent::Log { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    for line in &lines {
        assert_no_secret("a log line", line);
    }
    // stdout and stderr are read side by side: compare as sets.
    lines.sort();
    let mut expected: Vec<String> = MASKED.iter().map(|line| line.to_string()).collect();
    expected.sort();
    assert_eq!(lines, expected);

    let Outcome::Failed {
        exit_code, summary, ..
    } = outcome
    else {
        panic!("expected a failure, got {outcome:?}");
    };
    assert_eq!(exit_code, Some(5));
    assert_no_secret("the failure summary", &summary);
    // The summary is stderr's last five lines: the same secrets as the
    // log's, masked the same way.
    assert_eq!(summary.lines().collect::<Vec<_>>(), MASKED[1..].to_vec());
}

#[tokio::test]
async fn test_the_lines_and_the_output_of_a_command_are_masked_alike() {
    accept_the_settings();
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = seen.clone();
    let on_line: LineCallback = Arc::new(move |run_line| {
        if let RunLine::Output(_, line) = run_line {
            seen_cb.lock().unwrap().push(line);
        }
    });
    let output = RealRunner::without_this_macs_settings()
        .run(tools_say(), Some(on_line), CancellationToken::new())
        .await
        .expect("the command runs");
    assert_eq!(output.exit_code, Some(5));
    assert_no_secret("stdout", &output.stdout);
    assert_no_secret("stderr", &output.stderr);
    assert_eq!(output.stdout, format!("{}\n", MASKED[0]));
    assert_eq!(output.stderr, format!("{}\n", MASKED[1..].join("\n")));
    let seen = seen.lock().unwrap();
    for line in seen.iter() {
        assert_no_secret("a line", line);
    }
    // Each secret that is in stderr is in a line handed on as it went, and
    // masked the same way in both.
    for line in output.stderr.lines() {
        assert!(seen.iter().any(|said| said == line), "{line}");
    }
}

#[tokio::test]
async fn test_curls_own_error_for_a_proxy_with_a_scheme_separator_in_its_password_is_masked() {
    // R2's own reproduction, through the runner: curl, handed `http_proxy`
    // for an http address, refuses its port before connecting anywhere and
    // prints the setting back as written (curl 8.7.1, this Mac's
    // `/usr/bin/curl`: exit 5, "Unsupported proxy syntax in '…'").
    accept_the_settings();
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
    let output = RealRunner::without_this_macs_settings()
        .run(
            CommandSpec {
                program: curl,
                // `-q` first: no `.curlrc` of this Mac's home is read.
                args: ["-q", "--head", "--max-time", "1", "http://127.0.0.1:9"]
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
            output.stderr.contains("'****:****@127.0.0.1:invalid'"),
            "{}",
            output.stderr
        );
    }
}
