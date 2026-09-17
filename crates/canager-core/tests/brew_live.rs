//! Live Homebrew smoke test: installs, inventories and removes the tiny GNU
//! `hello` formula through the real adapter. Runs only when
//! `CANAGER_LIVE=1` is set AND the test is invoked with `--ignored`, so a
//! plain `cargo test` never touches the machine.

use canager_core::adapters::brew::BrewAdapter;
use canager_core::adapters::Adapter;
use canager_core::events::VecSink;
use canager_core::model::{ArtifactKey, ArtifactKind, OpKind, OpRequest, Outcome};
use canager_core::runner::{HostEnv, RealRunner};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

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
        install_plan.args,
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
        uninstall_plan.args,
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
