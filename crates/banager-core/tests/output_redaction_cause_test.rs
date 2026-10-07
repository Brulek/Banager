//! Re-check 2's N1: a proxy password that is part of one of sudo's words
//! (`pass`, `word`, `term`, `requ`, `ask`) is masked in sudo's own lines,
//! "a password is required" becoming "a ****word is required". Why the
//! operation failed is read off the lines as the tool wrote them, before
//! the mask (`CommandOutput::failure_cause`), and carried as
//! `Outcome::Failed`'s `cause`: the operation still ends as needing the
//! Mac's password (`ops::Ended`), the history keeps that cause, and the
//! window reads it from the outcome, never from the masked summary.
//!
//! A file of its own: `login_path::accept` sets state for the whole
//! process, and each file under tests/ runs in a process of its own.

use banager_core::adapters::run_plan;
use banager_core::events::{EventSink, OperationEvent, VecSink};
use banager_core::history::{
    failure_cause, record_for, Ended as HistoryEnded, FailureCause, HistoryResult, Started,
};
use banager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, OpKind, OpRequest, Outcome, Plan, PlanAction,
    ResourceLock,
};
use banager_core::ops::Ended;
use banager_core::runner::login_path::{self, LoginEnv};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Each proxy's password is a part of one of sudo's words.
const PROXIES: &[(&str, &str)] = &[
    ("https_proxy", "http://user:pass@127.0.0.1:8080"),
    ("http_proxy", "http://user:word@127.0.0.1:8080"),
    ("all_proxy", "socks5://user:term@127.0.0.1:7891"),
    ("HTTPS_PROXY", "http://user:requ@127.0.0.1:8080"),
    ("ALL_PROXY", "socks5://user:ask@127.0.0.1:7891"),
];

/// What sudo 1.9 prints when it cannot ask for the Mac's password, as
/// Homebrew passes it on: a cask's installer that runs `sudo`.
const SUDO_SAYS: &str = r#"
printf '==> Installing Cask example\n' >&2
printf 'sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper\n' >&2
printf 'sudo: a password is required\n' >&2
exit 1
"#;

fn accept_the_settings() {
    login_path::accept(&LoginEnv {
        path: "/usr/bin:/bin".to_string(),
        imported: PROXIES
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
    });
}

async fn run_sudo() -> (Outcome, Vec<String>) {
    accept_the_settings();
    let runner: Arc<dyn banager_core::runner::CommandRunner> =
        Arc::new(banager_core::runner::RealRunner::new());
    let plan = Plan {
        request: OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "brew".to_string(),
            artifact_kind: ArtifactKind::Cask,
            name: "example".to_string(),
        },
        action: PlanAction::Command {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_string(), SUDO_SAYS.to_string()],
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
        3,
        CancellationToken::new(),
    )
    .await
    .expect("the plan runs");
    let lines = sink
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            OperationEvent::Log { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    (outcome, lines)
}

#[tokio::test]
async fn test_a_password_inside_sudos_words_is_masked_and_the_cause_still_read() {
    let (outcome, lines) = run_sudo().await;
    let Outcome::Failed {
        exit_code,
        summary,
        cause,
    } = &outcome
    else {
        panic!("expected a failure, got {outcome:?}");
    };
    assert_eq!(*exit_code, Some(1));
    // The mask took sudo's words -- `pass` and `word` in "password",
    // `requ` in "required" -- so the summary no longer says it ...
    assert!(
        summary.contains("sudo: a ******** is ****ired"),
        "{summary}"
    );
    assert!(!summary.contains("password"), "{summary}");
    assert!(lines
        .iter()
        .any(|line| line == "sudo: a ******** is ****ired"));
    assert_eq!(failure_cause(summary), None, "{summary}");
    // ... and the cause, read before the mask, does.
    assert_eq!(*cause, Some(FailureCause::NeedsPassword));
    // The operation ends as needing the password: the notification and
    // the operation bar say so.
    assert_eq!(Ended::of(&outcome), Ended::NeedsPassword);
    // The history keeps the cause.
    let key = ArtifactKey {
        instance_id: "brew".to_string(),
        kind: ArtifactKind::Cask,
        name: "example".to_string(),
    };
    let record = record_for(
        &HistoryEnded {
            op_id: 3,
            key: &key,
            op_kind: OpKind::Upgrade,
            outcome: &outcome,
            started: true,
            before: Some("1.0"),
            after: Some("1.0"),
            already_updated: None,
            follow_up_warnings: Vec::new(),
        },
        &Started {
            display_name: "example".to_string(),
            adapter_id: "brew".to_string(),
            listed_version: Some("1.0".to_string()),
        },
        "run",
        1_790_000_000,
    )
    .expect("an update is kept");
    assert_eq!(
        record.result,
        HistoryResult::Failed {
            cause: Some(FailureCause::NeedsPassword),
            detail: None
        }
    );
    // On the wire, for the window: the cause, beside the masked summary.
    let json = serde_json::to_string(&outcome).unwrap();
    assert!(json.contains(r#""cause":"needsPassword""#), "{json}");
}

#[tokio::test]
async fn test_no_password_reaches_the_summary_or_the_log() {
    let (outcome, lines) = run_sudo().await;
    let Outcome::Failed { summary, .. } = &outcome else {
        panic!("expected a failure, got {outcome:?}");
    };
    // sudo's words are where these passwords are found: masked, all of
    // them, wherever they stand in a word.
    for said in lines.iter().chain(std::iter::once(summary)) {
        for password in ["pass", "word", "term", "requ", "ask"] {
            assert!(!said.contains(password), "{password} in {said}");
        }
    }
}

#[tokio::test]
async fn test_source_detection_keeps_only_real_runner_redacted_diagnostic_and_original_cause() {
    use banager_core::runner::{no_answer, CommandRunner, CommandSpec, OutputUse, RealRunner};
    accept_the_settings();
    // A synthetic command only prints fixture stderr; no package manager runs.
    let result = RealRunner::new()
        .run(
            CommandSpec {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), SUDO_SAYS.into()],
                env: vec![],
                cwd: None,
                timeout: std::time::Duration::from_secs(5),
                output_use: OutputUse::Parsed,
            },
            None,
            CancellationToken::new(),
        )
        .await;
    let why = no_answer::of(&result).unwrap();
    let diagnostic = why.diagnostic.as_deref().unwrap();
    assert!(diagnostic.contains("sudo: a ******** is ****ired"));
    assert_eq!(failure_cause(diagnostic), None);
    assert_eq!(why.cause, Some(FailureCause::NeedsPassword));
    assert!(!diagnostic.contains("password"));
    assert!(diagnostic.len() <= 4096);
}
