use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::CheckOptions;
use banager_core::runner::{HostEnv, RealRunner};
use std::sync::Arc;
use std::time::Duration;

/// Smoke check: detects a real Homebrew install, runs `brew update`
/// unconditionally (via `with_update_ttl(Duration::from_secs(0))`), then
/// lists installed and outdated counts. It runs no install, uninstall or
/// upgrade of its own; `brew update` itself can install, move or uninstall
/// Homebrew packages Homebrew has moved or renamed (docs/what-we-run.md,
/// Homebrew).
#[tokio::main]
async fn main() {
    let env = HostEnv::discover();
    let runner: Arc<dyn banager_core::runner::CommandRunner> = Arc::new(RealRunner::new());
    let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(0));

    let instances = adapter.detect(&env).await;
    if instances.is_empty() {
        println!("No Homebrew instance detected on this machine.");
        return;
    }

    for inst in &instances {
        println!(
            "Found instance: {} (brew {})",
            inst.id,
            inst.version.as_deref().unwrap_or("unknown")
        );

        let artifacts = adapter.inventory(inst).await.expect("inventory failed");
        println!("  {} installed artifacts", artifacts.len());

        let outdated = adapter
            .check_updates(inst, &CheckOptions::default())
            .await
            .expect("check_updates failed")
            .candidates;
        println!("  {} outdated artifacts", outdated.len());
        for candidate in outdated.iter().take(10) {
            println!(
                "    {} {} -> {}",
                candidate.key.name, candidate.current, candidate.target
            );
        }
    }
}
