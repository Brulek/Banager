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
    // Nothing of the Mac running the test is read: not its Cellar, pins
    // or `brew.env` files.
    let adapter: Arc<dyn Adapter> =
        Arc::new(BrewAdapter::new(runner.clone()).reading_nothing_of_this_mac());
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

#[tokio::test]
async fn test_a_formula_whose_link_step_failed_with_nothing_to_compare_says_it_is_not_linked() {
    // Review of r6 y3-batch, finding 2: node@22's own upgrade on
    // 2026-10-07. Homebrew writes only its `ofail` line to stderr; "Could
    // not symlink bin/npm / Target /opt/homebrew/bin/npm already exists"
    // goes to stdout with `puts` (`FormulaInstaller#link`,
    // formula_installer.rb in Homebrew 7.0.8), where no cause is read.
    //
    // The link fails only after the new keg is poured, so where both
    // readings of the version could be taken it reads as moved, and the
    // update as installed with that step failed, still not linked
    // (`test_a_formula_whose_link_step_met_a_file_in_the_way_says_it_is_not_linked`
    // in ops_upgrade_version_test.rs; skeptic of r35 U2, 1). This is the
    // update with nothing to compare -- no reading of what is installed
    // before it or after it (here `brew info` refuses both times; on a
    // real Mac, a `brew update` still running refuses the one before) --
    // which stays a failure, with that cause.
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![BREW, "upgrade", "--formula", "jq"],
        output(
            1,
            "==> Pouring jq--1.8.3.arm64_tahoe.bottle.tar.gz\n\
             The formula built, but is not symlinked into /opt/homebrew\n\
             Could not symlink bin/jq\n\
             Target /opt/homebrew/bin/jq\n\
             already exists. You may want to remove it:\n  rm '/opt/homebrew/bin/jq'\n",
            "Error: The `brew link` step did not complete successfully\n",
        ),
    );
    runner.respond(
        vec![BREW, "info", "--installed", "--json=v2"],
        output(1, "", "Error: Failed to load the installed formulae\n"),
    );
    // Nothing of the Mac running the test is read: not its Cellar, pins
    // or `brew.env` files.
    let adapter: Arc<dyn Adapter> =
        Arc::new(BrewAdapter::new(runner.clone()).reading_nothing_of_this_mac());
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
        artifact_kind: ArtifactKind::Formula,
        name: "jq".to_string(),
    };
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    let outcome = manager.wait(op_id).await.expect("an outcome");
    assert_eq!(cause(&outcome), Some(FailureCause::NotLinked));
}
