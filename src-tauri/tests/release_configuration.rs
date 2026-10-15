//! What a tagged release builds, read from the files that decide it:
//! `tauri.conf.json` and `.github/workflows/release.yml`. The updater's
//! public key is still the placeholder `REPLACE_WITH_UPDATER_PUBKEY`, and
//! the Tauri CLI decodes that key whenever it signs updater artifacts, so a
//! release that makes them fails before anything is uploaded. Until the
//! author's matching public key is in place, neither updater artifacts nor
//! `latest.json` are made; everything else the release does stays.
//! No process, no network: both files are compiled in.

const CONFIG: &str = include_str!("../tauri.conf.json");
const WORKFLOW: &str = include_str!("../../.github/workflows/release.yml");

fn config() -> serde_json::Value {
    serde_json::from_str(CONFIG).expect("tauri.conf.json is JSON")
}

/// The `with:` settings of the `tauri-apps/tauri-action` step, one
/// trimmed line each.
fn tauri_action_lines() -> Vec<&'static str> {
    let step = WORKFLOW
        .split_once("uses: tauri-apps/tauri-action@")
        .expect("release.yml runs tauri-apps/tauri-action")
        .1;
    let step = step.split("\n      - ").next().unwrap();
    step.lines().map(str::trim).collect()
}

#[test]
fn release_packaging_does_not_require_unconfigured_updater_signing() {
    let config = config();
    let pubkey = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .expect("plugins.updater.pubkey is a string");
    if pubkey == "REPLACE_WITH_UPDATER_PUBKEY" {
        assert_eq!(
            config["bundle"]["createUpdaterArtifacts"],
            serde_json::Value::Bool(false),
            "updater artifacts are signed with the configured public key, which is still the placeholder"
        );
        let lines = tauri_action_lines();
        assert!(
            lines.contains(&"includeUpdaterJson: false"),
            "the release publishes latest.json while the updater public key is still the placeholder"
        );
        assert!(
            !WORKFLOW.contains("secrets.TAURI_SIGNING_PRIVATE_KEY"),
            "the updater signing key is handed to a release that makes no updater artifacts"
        );
    }
}

#[test]
fn release_keeps_its_targets_apple_signing_draft_and_intel_check() {
    let config = config();
    assert_eq!(
        config["bundle"]["targets"],
        serde_json::json!(["app", "dmg"])
    );
    assert_eq!(config["bundle"]["macOS"]["minimumSystemVersion"], "13.3");
    let lines = tauri_action_lines();
    for wanted in [
        "args: --target universal-apple-darwin",
        "releaseDraft: true",
        "APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}",
        "APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}",
        "APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}",
        "APPLE_ID: ${{ secrets.APPLE_ID }}",
        "APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}",
        "APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}",
    ] {
        assert!(lines.contains(&wanted), "the release step lost `{wanted}`");
    }
    assert!(WORKFLOW.contains("\n  intel-launch:\n    needs: release\n"));
    assert!(WORKFLOW.contains("runs-on: macos-15-intel"));
}
