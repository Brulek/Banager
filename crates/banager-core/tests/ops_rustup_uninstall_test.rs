//! `rustup self uninstall -y`'s outcome, end to end through
//! `OperationManager` with the real `StandaloneAdapter` over the `RUSTUP`
//! recipe: its own `plan`, its own `execute` (`run_plan`), and step C's
//! `reconcile_after_uninstall` reading presence of the launcher. The
//! command is scripted, and the runner removes files as the command
//! "returns", standing in for what rustup's own uninstall leaves on the
//! disk in each case (phase 4 step E, plan ruling 6's outcome list):
//!
//! - exit 0, the launcher gone, other `bin/` files left: `Succeeded` --
//!   presence of the launcher is what is read;
//! - exit 0, the launcher still there: `NeedsAttention(StillInstalledAfterUninstall)`;
//! - stopped by the timeout, the launcher still there: `Unconfirmed`;
//! - stopped by the timeout, the launcher gone: `Succeeded` (the
//!   `Ok(Outcome::Unconfirmed)` arm's `Uninstall` branch, ops/mod.rs:
//!   presence decides, and a NoCancel op cannot have been cancelled).
//!
//! And the lock cases: rustup's plan names the cargo instance's lock
//! whether or not a cargo instance is registered (the name is what the
//! engine compares), the operation holds both names while the command
//! runs and releases both when it ends, and a plan for an instance the
//! adapter's seat no longer describes is refused before anything is
//! submitted.
//!
//! No recorded fixture: nothing here runs a command but the scripted
//! `--version` and the scripted uninstall, and every layout is built by
//! the test in a throwaway home (spec §9.3). Nothing touches the real
//! `~/.cargo` or `~/.rustup`.

use async_trait::async_trait;
use banager_core::adapters::standalone::recipes::RUSTUP;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::AdapterError;
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    ArtifactKind, Attention, ManagerInstance, OpKind, OpRequest, Outcome, ResourceLock,
};
use banager_core::ops::OperationManager;
use banager_core::runner::{
    CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner, RunnerError,
};
use banager_core::trash::MockTrasher;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tokio_util::sync::CancellationToken;

/// The line `rustup --version` printed on the author's Mac when the phase
/// 4 step E plan was written (its rulings quote it); the recipe's parser
/// takes the second token. An inline string, not a fixture.
const VERSION_LINE: &str = "rustup 1.29.1 (d95a37b6a 2026-08-13)\n";

fn exited_0(stdout: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    }
}

fn timed_out() -> CommandOutput {
    CommandOutput {
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        timed_out: true,
        cancelled: false,
    }
}

/// What the scripted uninstall leaves on the disk as it returns.
#[derive(Clone, Copy, Debug)]
enum Leaves {
    /// The launcher gone, `bin/hexyl` and the `cargo` proxy still there: a
    /// removal of the Cargo home that got as far as the launcher and no
    /// further. Synthetic, to prove the reading after looks at the
    /// launcher alone and not at the folder.
    LauncherGoneOthersLeft,
    /// Everything: the whole Cargo home and the rustup home are gone,
    /// which is what rustup 1.29.1's `uninstall()` leaves when it runs to
    /// the end (plan ruling 1).
    CargoHomeGone,
    /// Nothing changed.
    Everything,
}

/// A rustup layout in a temp home: the launcher (an executable regular
/// file), its `cargo` proxy link, a `cargo install`ed `hexyl`, and one
/// toolchain directory under `~/.rustup`. Canonical, so the paths the
/// adapter expands compare equal to what it resolves (macOS's
/// `/var/folders` is `/private/var/…`); removed on drop.
struct RustupHome {
    home: PathBuf,
    env: HostEnv,
}

impl RustupHome {
    fn new(tag: &str) -> RustupHome {
        use std::os::unix::fs::PermissionsExt;
        let raw = std::env::temp_dir().join(format!(
            "banager-ops-rustup-uninstall-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("temp home");
        let home = std::fs::canonicalize(&raw).expect("canonical temp home");
        let bin = home.join(".cargo/bin");
        std::fs::create_dir_all(&bin).expect("cargo bin");
        let launcher = bin.join("rustup");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("rustup");
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("executable rustup");
        std::os::unix::fs::symlink("rustup", bin.join("cargo")).expect("cargo proxy");
        std::fs::write(bin.join("hexyl"), b"#!/bin/sh\n").expect("hexyl");
        std::fs::create_dir_all(home.join(".rustup/toolchains/stable-aarch64-apple-darwin"))
            .expect("toolchain");
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.clone(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        RustupHome { home, env }
    }

    fn launcher(&self) -> PathBuf {
        self.home.join(".cargo/bin/rustup")
    }

    /// The cargo instance's id for this home's `~/.cargo`, spelled as
    /// `cargo::instance_id_for` spells it (`model::instance_id`).
    fn cargo_id(&self) -> String {
        format!("cargo:{}", self.home.join(".cargo").display())
    }
}

impl Drop for RustupHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// Answers from the inner mock, then changes the disk the way the
/// scripted uninstall would have as it returns. While the uninstall
/// command "runs" it also reads which locks the manager holds, so a test
/// can assert on what was held during the run and not only on what is
/// left afterwards.
struct UninstallingRunner {
    inner: Arc<MockRunner>,
    home: PathBuf,
    leaves: Leaves,
    /// The manager the uninstall is submitted through, set by `uninstall`
    /// once it has built one; `run` reads its held locks through it.
    manager: OnceLock<Weak<OperationManager>>,
    /// What `OperationManager::locks_held` answered while the uninstall
    /// command ran; `None` until it has run.
    held_during_uninstall: Mutex<Option<HashSet<ResourceLock>>>,
}

impl UninstallingRunner {
    fn held_during_uninstall(&self) -> Option<HashSet<ResourceLock>> {
        self.held_during_uninstall.lock().unwrap().clone()
    }
}

#[async_trait]
impl CommandRunner for UninstallingRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let is_uninstall = spec.args == ["self", "uninstall", "-y"];
        if is_uninstall {
            if let Some(manager) = self.manager.get().and_then(Weak::upgrade) {
                *self.held_during_uninstall.lock().unwrap() = Some(manager.locks_held());
            }
        }
        let output = self.inner.run(spec, on_line, cancel).await?;
        if is_uninstall {
            match self.leaves {
                Leaves::LauncherGoneOthersLeft => {
                    std::fs::remove_file(self.home.join(".cargo/bin/rustup"))
                        .expect("unlink rustup");
                }
                Leaves::CargoHomeGone => {
                    std::fs::remove_dir_all(self.home.join(".cargo")).expect("remove cargo home");
                    std::fs::remove_dir_all(self.home.join(".rustup")).expect("remove rustup home");
                }
                Leaves::Everything => {}
            }
        }
        Ok(output)
    }
}

/// Detects rustup in `home` over a mock that answers `--version` and the
/// uninstall with `uninstall_output`, leaving `leaves` behind.
async fn rustup_adapter(
    home: &RustupHome,
    uninstall_output: CommandOutput,
    leaves: Leaves,
) -> (
    Arc<StandaloneAdapter>,
    ManagerInstance,
    Arc<UninstallingRunner>,
) {
    let mock = Arc::new(MockRunner::new());
    let launcher = home.launcher().to_string_lossy().to_string();
    mock.respond(vec![launcher.as_str(), "--version"], exited_0(VERSION_LINE));
    mock.respond(
        vec![launcher.as_str(), "self", "uninstall", "-y"],
        uninstall_output,
    );
    let runner = Arc::new(UninstallingRunner {
        inner: mock,
        home: home.home.clone(),
        leaves,
        manager: OnceLock::new(),
        held_during_uninstall: Mutex::new(None),
    });
    let adapter = Arc::new(StandaloneAdapter::new(
        &RUSTUP,
        runner.clone(),
        Arc::new(MockHttpClient::new()),
        // The recipe's uninstall is a command: nothing here goes to the
        // Trash.
        Arc::new(MockTrasher::new()),
    ));
    let inst = adapter.detect(&home.env).await.remove(0);
    (adapter, inst, runner)
}

fn uninstall_request() -> OpRequest {
    OpRequest {
        kind: OpKind::Uninstall,
        instance_id: "standalone-rustup".to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: "rustup".to_string(),
    }
}

/// Plans and submits the uninstall through a fresh `OperationManager`
/// holding only the rustup adapter and instance -- no cargo instance --
/// and returns the outcome and the manager (for its held locks). The
/// manager is handed to `runner` first, so it can read the locks held
/// while the command runs.
async fn uninstall(
    adapter: Arc<StandaloneAdapter>,
    inst: ManagerInstance,
    runner: &UninstallingRunner,
) -> (Outcome, Arc<OperationManager>) {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    runner
        .manager
        .set(Arc::downgrade(&manager))
        .expect("one manager per runner");
    let plan = adapter
        .plan(&inst, &uninstall_request())
        .await
        .expect("plan");
    let op_id = manager.submit(plan);
    let outcome = manager.wait(op_id).await.expect("an outcome");
    (outcome, manager)
}

#[tokio::test]
async fn test_an_uninstall_exiting_zero_with_the_launcher_gone_and_other_bin_files_left_succeeded()
{
    let home = RustupHome::new("launcher-gone");
    let launcher = home.launcher().to_string_lossy().to_string();
    let (adapter, inst, runner) = rustup_adapter(
        &home,
        exited_0("info: rustup is uninstalled\n"),
        Leaves::LauncherGoneOthersLeft,
    )
    .await;
    let (outcome, manager) = uninstall(adapter, inst, &runner).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert!(
        home.home.join(".cargo/bin/hexyl").exists(),
        "presence is the launcher's"
    );
    // Detect's `--version`, then the uninstall itself, and nothing else:
    // the look before the spawn, the gate asked again there, and the
    // reading after all read the disk and run no command
    // (`reconcile_after_uninstall`: presence is the whole question).
    assert_eq!(
        runner.inner.calls(),
        vec![
            vec![launcher.clone(), "--version".to_string()],
            vec![
                launcher,
                "self".to_string(),
                "uninstall".to_string(),
                "-y".to_string()
            ],
        ]
    );
    assert!(manager.locks_held().is_empty());
}

#[tokio::test]
async fn test_an_uninstall_exiting_zero_with_the_launcher_still_there_needs_attention() {
    let home = RustupHome::new("launcher-left");
    let (adapter, inst, runner) = rustup_adapter(
        &home,
        exited_0("info: rustup is uninstalled\n"),
        Leaves::Everything,
    )
    .await;
    let (outcome, _manager) = uninstall(adapter, inst, &runner).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::StillInstalledAfterUninstall)
    );
}

#[tokio::test]
async fn test_an_uninstall_stopped_by_the_timeout_before_the_launcher_went_is_unconfirmed() {
    // NoCancel: the timeout is the only stop. The launcher is still
    // there, nobody cancelled, so nothing can be said.
    let home = RustupHome::new("timeout-before");
    let (adapter, inst, runner) = rustup_adapter(&home, timed_out(), Leaves::Everything).await;
    let (outcome, _manager) = uninstall(adapter, inst, &runner).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_an_uninstall_stopped_by_the_timeout_after_the_launcher_went_succeeded() {
    // The reading after tells the artifact's current state: gone is
    // gone, whatever stopped the command.
    let home = RustupHome::new("timeout-after");
    let (adapter, inst, runner) = rustup_adapter(&home, timed_out(), Leaves::CargoHomeGone).await;
    let (outcome, _manager) = uninstall(adapter, inst, &runner).await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_an_uninstall_with_no_cargo_instance_registered_still_holds_the_cargo_lock_by_name() {
    // Spec §2.4: the second lock is a name, `cargo:<cargo_home>`, and the
    // engine compares names; a cargo instance need not exist for the op
    // to take it and release it.
    let home = RustupHome::new("cargo-absent");
    let (adapter, inst, runner) = rustup_adapter(
        &home,
        exited_0("info: rustup is uninstalled\n"),
        Leaves::CargoHomeGone,
    )
    .await;
    let plan = adapter
        .plan(&inst, &uninstall_request())
        .await
        .expect("plan");
    let both = vec![
        ResourceLock("standalone-rustup".to_string()),
        ResourceLock(home.cargo_id()),
    ];
    assert_eq!(plan.locks, both);
    let (outcome, manager) = uninstall(adapter, inst, &runner).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.held_during_uninstall(),
        Some(both.into_iter().collect()),
        "both names held while the command ran"
    );
    assert!(manager.locks_held().is_empty(), "both names released");
}

#[tokio::test]
async fn test_a_plan_for_an_instance_from_another_home_is_refused_and_a_redetect_restores_it() {
    // Plan ruling 9: the adapter's seat is one slot. Detect A, detect B,
    // plan for A: refused, so nothing with B's cargo lock and B's
    // warnings ever reaches the manager for A's launcher. Detect A again,
    // and the plan goes through with A's lock.
    let home_a = RustupHome::new("seat-a");
    let home_b = RustupHome::new("seat-b");
    let mock = Arc::new(MockRunner::new());
    for home in [&home_a, &home_b] {
        mock.respond(
            vec![home.launcher().to_str().unwrap(), "--version"],
            exited_0(VERSION_LINE),
        );
    }
    let adapter = StandaloneAdapter::new(
        &RUSTUP,
        mock.clone(),
        Arc::new(MockHttpClient::new()),
        Arc::new(MockTrasher::new()),
    );
    let inst_a = adapter.detect(&home_a.env).await.remove(0);
    let _inst_b = adapter.detect(&home_b.env).await.remove(0);
    assert!(matches!(
        adapter.plan(&inst_a, &uninstall_request()).await,
        Err(AdapterError::Refused(_))
    ));
    adapter.detect(&home_a.env).await;
    let plan = adapter
        .plan(&inst_a, &uninstall_request())
        .await
        .expect("plan for A again");
    assert_eq!(plan.locks[1], ResourceLock(home_a.cargo_id()));
}
