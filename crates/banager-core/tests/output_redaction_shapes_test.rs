//! The shapes of login the decisions-round re-check found leaking (R1, R2)
//! or masked where there was none (R3), end to end: settings from the
//! login shell, a command printing what git, curl, npm and pip printed for
//! them on this Mac, and what reaches the operation's log, its failure
//! summary and the command's output (`runner::redact`); and (r25's M1) a
//! login no setting holds, from a tool's own settings file, its password
//! with a raw `/`, `?` or `#`, as Node and pip print it back.
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

/// r25's M1: a login in a tool's own settings file, which no imported
/// setting holds -- `https-proxy=http://alice:Ab3/xY+9@proxy.corp:8080` in
/// `~/.npmrc`; in `pip.conf`, an `index-url` with the same password and a
/// `proxy` with a `?` in its password -- so only the generic rule can find
/// it. Node 22.23.3 warns about the npm address once in every npm 10.9.9
/// command (its config validation hands it to `url.parse`, which reads
/// `alice:Ab3` as a host and a port that is no number); npm then reads the
/// proxy's host as `alice`. pip 26.2.1 prints the index as is in `Looking
/// in indexes:` (its own `redact_auth_from_url` ends the netloc at the
/// `/` and finds no `@` in it; with an `@` in the password it masks only up
/// to that `@`), and the proxy in its last two lines. The last five
/// stderr lines are the failure's summary.
const CONFIG_FILES_SAY: &str = r##"
printf '%s\n' 'Looking in indexes: https://pypi.org/simple, https://alice:Ab3/xY+9@pypi.corp/simple'
printf '%s\n' 'Looking in indexes: https://alice:****@ss/w0rd@pypi.corp/simple'
printf '%s\n' 'Looking in indexes: https://mirror.example:8443/x/user@example.com/simple'
printf '%s\n' '(node:4242) [DEP0170] DeprecationWarning: The URL http://alice:Ab3/xY+9@proxy.corp:8080 is invalid. Future versions of Node.js will throw an error.' >&2
printf '%s\n' '(Use `node --trace-deprecation ...` to show where the warning was created)' >&2
printf '%s\n' 'npm error network request to https://registry.npmjs.org/@types%2fnode failed, reason: getaddrinfo ENOTFOUND alice' >&2
printf '%s\n' 'pip._vendor.urllib3.exceptions.LocationParseError: Failed to parse: http://ci-bot:pa?ss42@proxy.corp:3128' >&2
printf '%s\n' 'pip._vendor.requests.exceptions.InvalidURL: Failed to parse: http://ci-bot:pa?ss42@proxy.corp:3128' >&2
exit 1
"##;

/// What each line of `CONFIG_FILES_SAY` reads masked, in order: stdout's
/// three, then stderr's five. The public index's `@` stays. `alice`, the
/// user name npm took for the proxy's host, stays as npm printed it: a
/// bare word from a file Banager does not read, which no rule can tell
/// from a host's name (docs/what-we-run.md: bare tokens from such files
/// are not discovered).
const CONFIG_FILES_MASKED: &[&str] = &[
    "Looking in indexes: https://pypi.org/simple, https://****:****@pypi.corp/simple",
    "Looking in indexes: https://****:****@pypi.corp/simple",
    "Looking in indexes: https://mirror.example:8443/x/user@example.com/simple",
    "(node:4242) [DEP0170] DeprecationWarning: The URL http://****:****@proxy.corp:8080 is invalid. Future versions of Node.js will throw an error.",
    "(Use `node --trace-deprecation ...` to show where the warning was created)",
    "npm error network request to https://registry.npmjs.org/@types%2fnode failed, reason: getaddrinfo ENOTFOUND alice",
    "pip._vendor.urllib3.exceptions.LocationParseError: Failed to parse: http://****:****@proxy.corp:3128",
    "pip._vendor.requests.exceptions.InvalidURL: Failed to parse: http://****:****@proxy.corp:3128",
];

/// The passwords above, and their parts.
const CONFIG_FILE_SECRETS: &[&str] = &["Ab3", "xY+9", "ss/w0rd", "w0rd", "ci-bot", "pa?ss42"];

fn assert_no_config_file_secret(what: &str, text: &str) {
    for secret in CONFIG_FILE_SECRETS {
        assert!(!text.contains(secret), "{what} holds {secret:?}:\n{text}");
    }
}

fn config_files_say(output_use: OutputUse) -> CommandSpec {
    CommandSpec {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".to_string(), CONFIG_FILES_SAY.to_string()],
        env: vec![],
        cwd: None,
        timeout: Duration::from_secs(30),
        output_use,
    }
}

#[tokio::test]
async fn test_a_login_from_a_tools_own_settings_file_is_masked_in_the_log_and_the_summary() {
    // The operation's log (Copy Log) and its failure summary (Copy Error
    // Details), through `run_plan` as every npm and pip operation runs.
    accept_the_settings();
    let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::without_this_macs_settings());
    let spec = config_files_say(OutputUse::Transcript);
    let plan = Plan {
        request: OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "npm".to_string(),
            artifact_kind: ArtifactKind::Package,
            name: "typescript".to_string(),
        },
        action: PlanAction::Command {
            program: spec.program,
            args: spec.args,
            env: vec![],
        },
        needs_password: false,
        locks: vec![ResourceLock("npm".to_string())],
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
        assert_no_config_file_secret("a log line", line);
    }
    // stdout and stderr are read side by side: compare as sets.
    lines.sort();
    let mut expected: Vec<String> = CONFIG_FILES_MASKED
        .iter()
        .map(|line| line.to_string())
        .collect();
    expected.sort();
    assert_eq!(lines, expected);
    let Outcome::Failed {
        exit_code, summary, ..
    } = outcome
    else {
        panic!("expected a failure, got {outcome:?}");
    };
    assert_eq!(exit_code, Some(1));
    assert_no_config_file_secret("the failure summary", &summary);
    assert_eq!(
        summary.lines().collect::<Vec<_>>(),
        CONFIG_FILES_MASKED[3..].to_vec()
    );
}

#[tokio::test]
async fn test_a_login_from_a_tools_own_settings_file_is_masked_in_what_a_read_and_a_source_keep() {
    // What a command's caller reads off it: a check's reason (npm's
    // `lookup_failure_reason` takes stderr's first line), the read before
    // a confirmed npm or uv run (g1's `read_before_run` logs its stderr),
    // and a source that did not answer (f13b's diagnostic, shown by Show
    // Error Details and copied by Copy Error Details and Copy Diagnostic
    // Info).
    accept_the_settings();
    let output = RealRunner::without_this_macs_settings()
        .run(
            config_files_say(OutputUse::Transcript),
            None,
            CancellationToken::new(),
        )
        .await
        .expect("the command runs");
    assert_eq!(output.exit_code, Some(1));
    assert_eq!(
        output.stdout,
        format!("{}\n", CONFIG_FILES_MASKED[..3].join("\n"))
    );
    assert_eq!(
        output.stderr,
        format!("{}\n", CONFIG_FILES_MASKED[3..].join("\n"))
    );

    let result = RealRunner::without_this_macs_settings()
        .run(
            config_files_say(OutputUse::Parsed),
            None,
            CancellationToken::new(),
        )
        .await;
    let why = banager_core::runner::no_answer::of(&result).expect("a source that did not answer");
    let diagnostic = why.diagnostic.expect("its diagnostic");
    assert_no_config_file_secret("the source's diagnostic", &diagnostic);
    assert_eq!(diagnostic, CONFIG_FILES_MASKED[3..].join("\n"));
}
