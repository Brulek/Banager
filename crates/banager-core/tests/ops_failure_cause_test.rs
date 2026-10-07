//! A failed operation keeps why it failed (r6 y3-batch, finding 3): on
//! 2026-10-07 Claudebar's and OnyX's updates ended `Failed` with no cause,
//! and once the window closed nothing said why. Both casks' apps had been
//! moved out of /Applications before the update; Homebrew backs the old
//! app up before it upgrades and refuses with "It seems the App source
//! '/Applications/…' is not there" (`move_back`, cask/artifact/moved.rb in
//! Homebrew 7.0.8). End to end through the Homebrew adapter, with only the
//! commands' output scripted.

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::Adapter;
use banager_core::events::VecSink;
use banager_core::history::FailureCause;
use banager_core::model::{ArtifactKind, ManagerInstance, OpKind, OpRequest, Outcome};
use banager_core::ops::OperationManager;
use banager_core::runner::{CommandOutput, MockRunner};
use std::path::PathBuf;
use std::sync::Arc;

const BREW: &str = "/opt/homebrew/bin/brew";

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        stderr_cause: Default::default(),
        exit_code: Some(code),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        timed_out: false,
        cancelled: false,
    }
}

async fn upgrade_cask_failing_with(token: &str, stderr: &str) -> Outcome {
    let info = std::fs::read_to_string("../../adapters/fixtures/brew/7.0.3/info-installed.json")
        .expect("fixture");
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![BREW, "upgrade", "--cask", token],
        output(1, "==> Upgrading onyx\n", stderr),
    );
    runner.respond(
        vec![BREW, "info", "--installed", "--json=v2"],
        output(0, &info, ""),
    );
    let adapter: Arc<dyn Adapter> = Arc::new(BrewAdapter::new(runner.clone()));
    let inst = ManagerInstance {
        exe_path: PathBuf::from(BREW),
        prefix: PathBuf::from("/opt/homebrew"),
        version: Some("7.0.3".to_string()),
        ..banager_core::testing::manager_instance("brew", "brew:/opt/homebrew")
    };
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    let req = OpRequest {
        kind: OpKind::Upgrade,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Cask,
        name: token.to_string(),
    };
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    manager.wait(op_id).await.expect("an outcome")
}

fn cause(outcome: &Outcome) -> Option<FailureCause> {
    match outcome {
        Outcome::Failed { cause, .. } => *cause,
        other => panic!("not a failure: {other:?}"),
    }
}

#[tokio::test]
async fn test_a_cask_whose_app_was_moved_away_fails_saying_so() {
    let outcome = upgrade_cask_failing_with(
        "onyx",
        "Error: It seems the App source '/Applications/OnyX.app' is not there.\n",
    )
    .await;
    assert_eq!(cause(&outcome), Some(FailureCause::AppMissing));
}

#[tokio::test]
async fn test_a_cask_for_another_macos_fails_saying_so() {
    let outcome = upgrade_cask_failing_with(
        "onyx",
        "Error: onyx: This cask does not run on macOS versions older than Tahoe.\n",
    )
    .await;
    assert_eq!(cause(&outcome), Some(FailureCause::Unsupported));
}

#[tokio::test]
async fn test_a_cask_that_needs_the_password_still_says_that_first() {
    let outcome = upgrade_cask_failing_with(
        "onyx",
        "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper\n\
         sudo: a password is required\n\
         Error: It seems the App source '/Applications/OnyX.app' is not there.\n",
    )
    .await;
    assert_eq!(cause(&outcome), Some(FailureCause::NeedsPassword));
}
