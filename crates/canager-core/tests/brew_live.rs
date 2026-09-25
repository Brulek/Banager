//! Live Homebrew tests against the real machine. Both are `#[ignore]`d, so
//! a plain `cargo test` never runs them and a clean checkout on a Mac with
//! no Homebrew is still green.
//!
//! - `live_detect_finds_the_homebrew_installed_on_this_machine` only reads:
//!   it probes the real filesystem and runs `brew --version`. This is the
//!   real-machine half of detect's coverage, the half the unit tests in
//!   `adapters/brew/mod.rs` gave up when their filesystem probe became
//!   injectable -- those now prove the *logic* against a pinned layout, and
//!   this proves the logic still matches a real Homebrew install.
//! - `live_install_inventory_uninstall_hello` changes the machine, so it
//!   additionally requires `CANAGER_LIVE=1` and skips loudly without it.

use canager_core::adapters::brew::BrewAdapter;
use canager_core::adapters::Adapter;
use canager_core::events::VecSink;
use canager_core::model::{ArtifactKey, ArtifactKind, OpKind, OpRequest, Outcome};
use canager_core::runner::{HostEnv, RealRunner};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
#[ignore = "reads the real filesystem and runs `brew --version`; run with cargo test -p canager-core --test brew_live -- --ignored"]
async fn live_detect_finds_the_homebrew_installed_on_this_machine() {
    let adapter = BrewAdapter::new(Arc::new(RealRunner));
    let env = HostEnv::discover();
    let instances = Adapter::detect(&adapter, &env).await;

    let on_disk: Vec<&str> = BrewAdapter::CANDIDATE_PATHS
        .iter()
        .copied()
        .filter(|candidate| Path::new(candidate).exists())
        .collect();
    if on_disk.is_empty() {
        eprintln!(
            "no `brew` at any of {:?}; skipping -- this test asserts about a real \
             Homebrew and there is none here",
            BrewAdapter::CANDIDATE_PATHS
        );
        assert!(
            instances.is_empty(),
            "nothing on disk, so detect must report nothing: {instances:?}"
        );
        return;
    }

    assert_eq!(
        instances.len(),
        on_disk.len(),
        "one instance per `brew` on disk ({on_disk:?}), got {instances:?}"
    );
    for (inst, candidate) in instances.iter().zip(&on_disk) {
        assert_eq!(inst.exe_path, Path::new(candidate));
        assert_eq!(inst.id, format!("brew:{}", inst.prefix.display()));
        assert!(
            inst.exe_path.starts_with(&inst.prefix),
            "the prefix must be the install root the exe sits under, got {inst:?}"
        );
        if env.euid == 0 {
            assert!(
                inst.version.is_none(),
                "as root no brew process is run at all, so there is no version"
            );
        } else {
            assert!(
                inst.version.is_some(),
                "a real Homebrew answers `--version`, got {inst:?}"
            );
            assert!(inst.available(), "got {inst:?}");
        }
    }
}

#[tokio::test]
#[ignore = "installs and removes the `hello` formula; run with CANAGER_LIVE=1 cargo test -p canager-core --test brew_live -- --ignored"]
async fn live_install_inventory_uninstall_hello() {
    if std::env::var("CANAGER_LIVE").as_deref() != Ok("1") {
        eprintln!("CANAGER_LIVE is not 1; skipping live smoke test");
        return;
    }

    let runner = Arc::new(RealRunner);
    // A huge TTL means `brew update` is not run here; CI runners already ship
    // a fresh Homebrew and the install path does not need the newest index.
    let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(60 * 60 * 24 * 365));
    let env = HostEnv::discover();
    let instances = Adapter::detect(&adapter, &env).await;
    let inst = instances
        .first()
        .cloned()
        .expect("a Homebrew instance must be detected on the CI runner");
    let sink = Arc::new(VecSink::new());
    let key = ArtifactKey {
        instance_id: inst.id.clone(),
        kind: ArtifactKind::Formula,
        name: "hello".to_string(),
    };

    // Install.
    let install_req = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "hello".to_string(),
    };
    let install_plan = Adapter::plan(&adapter, &inst, &install_req)
        .await
        .expect("install plan");
    assert_eq!(
        canager_core::testing::command_args(&install_plan),
        vec![
            "install".to_string(),
            "--formula".to_string(),
            "hello".to_string()
        ],
        "install argv preview must be exactly `brew install --formula hello`"
    );
    let outcome = Adapter::execute(
        &adapter,
        &install_plan,
        sink.clone(),
        1,
        CancellationToken::new(),
    )
    .await
    .expect("install execute");
    assert_eq!(
        outcome,
        Outcome::Succeeded,
        "install must succeed; log: {:?}",
        sink.snapshot()
    );

    // Inventory + reconcile see it.
    let reconciled = Adapter::reconcile(&adapter, &inst, &key)
        .await
        .expect("reconcile after install");
    assert!(reconciled.present, "hello must be present after install");
    assert!(reconciled.version.is_some());
    let inventory = Adapter::inventory(&adapter, &inst)
        .await
        .expect("inventory");
    assert!(
        inventory.iter().any(|a| a.key == key),
        "inventory must list hello"
    );

    // Uninstall.
    let uninstall_req = OpRequest {
        kind: OpKind::Uninstall,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "hello".to_string(),
    };
    let uninstall_plan = Adapter::plan(&adapter, &inst, &uninstall_req)
        .await
        .expect("uninstall plan");
    assert!(
        uninstall_plan.affected.is_empty(),
        "nothing installed depends on hello"
    );
    assert_eq!(
        canager_core::testing::command_args(&uninstall_plan),
        vec![
            "uninstall".to_string(),
            "--formula".to_string(),
            "hello".to_string()
        ],
        "uninstall argv preview must be exactly `brew uninstall --formula hello`"
    );
    let outcome = Adapter::execute(
        &adapter,
        &uninstall_plan,
        sink.clone(),
        2,
        CancellationToken::new(),
    )
    .await
    .expect("uninstall execute");
    assert_eq!(
        outcome,
        Outcome::Succeeded,
        "uninstall must succeed; log: {:?}",
        sink.snapshot()
    );

    let reconciled = Adapter::reconcile(&adapter, &inst, &key)
        .await
        .expect("reconcile after uninstall");
    assert!(!reconciled.present, "hello must be gone after uninstall");
}
