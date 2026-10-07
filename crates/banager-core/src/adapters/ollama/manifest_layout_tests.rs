//! Ollama 0.40 layout regression tests. No test reads the real home or runs Ollama.
//! Source: ollama/ollama v0.40.0 manifest/{paths,manifest}.go and server/images.go.
//! The Mac had only a legacy manifest on 2026-10-07. REAL_MANIFEST is the recorded
//! fixture, byte-identical to it (251725 bytes, 1209 layers); the 0.40 relative
//! symlink and downgrade anchor below are reconstructed from upstream's
//! writers, not claimed as a recording of a pull on this Mac.

use super::*;
use crate::http::mock::MockHttpClient;
use crate::http::HttpResponse;
use crate::runner::mock::MockRunner;
use crate::testing::TempTree;
use sha2::{Digest, Sha256};
use std::os::unix::fs::symlink;

const LIVE_DIGEST: &str = "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e";
const V2: &str = "models/manifests-v2/ollama.com/library/qwen3.8/27b-mlx";
const LEGACY: &str = "models/manifests/registry.ollama.ai/library/qwen3.8/27b-mlx";
const REGISTRY: &str = "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx";
const TAGS: &str = "http://127.0.0.1:11434/api/tags";

fn write(tree: &TempTree, path: &str, bytes: &str) -> PathBuf {
    let path = tree.at(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn anchor() -> String {
    // WriteLegacyAnchor clones the runnable manifest, then appends the manifest
    // blob itself as a layer so an older daemon's GC retains that blob.
    let mut json: serde_json::Value = serde_json::from_str(REAL_MANIFEST).unwrap();
    json["layers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
            "digest": format!("sha256:{LIVE_DIGEST}"),
            "size": REAL_MANIFEST.len(),
        }));
    serde_json::to_string(&json).unwrap()
}

fn v2_link(tree: &TempTree) -> PathBuf {
    let blob = write(
        tree,
        &format!("models/blobs/sha256-{LIVE_DIGEST}"),
        REAL_MANIFEST,
    );
    let path = tree.at(V2);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    symlink(format!("../../../../blobs/sha256-{LIVE_DIGEST}"), path).unwrap();
    blob
}

async fn check(
    tree: &TempTree,
    digest: &str,
    registry: &str,
) -> (Vec<UpdateCandidate>, Vec<String>) {
    let http = Arc::new(MockHttpClient::new());
    // Recorded api/tags row, with only the manifest digest replaced for negative cases.
    let mut tags: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../adapters/fixtures/ollama/0.34.1/api-tags.json"
    ))
    .unwrap();
    tags["models"][0]["digest"] = digest.into();
    http.respond(
        TAGS,
        HttpResponse {
            status: 200,
            body: tags.to_string(),
        },
    );
    http.respond(
        REGISTRY,
        HttpResponse {
            status: 200,
            body: registry.into(),
        },
    );
    let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
    let inst = ManagerInstance {
        prefix: tree.root.clone(),
        version: Some("0.40.0".into()),
        ..crate::testing::manager_instance("ollama", "ollama:http://127.0.0.1:11434")
    };
    let rows = adapter
        .check_updates(&inst, &CheckOptions::default())
        .await
        .unwrap()
        .candidates;
    (rows, http.calls())
}

fn assert_uncheckable(rows: &[UpdateCandidate], calls: &[String]) {
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].checkable, "{:?}", rows[0]);
    assert_eq!(rows[0].current, rows[0].target);
    assert_eq!(rows[0].download_bytes, None);
    assert_eq!(calls, &[TAGS]);
}

#[tokio::test]
async fn freshly_pulled_v2_symlink_is_current_despite_legacy_anchor() {
    let tree = TempTree::new("ollama-v2-current");
    assert_eq!(format!("{:x}", Sha256::digest(REAL_MANIFEST)), LIVE_DIGEST);
    assert_eq!(REAL_MANIFEST.len(), 251725);
    v2_link(&tree);
    write(&tree, LEGACY, &anchor());
    // Compare against the real recorded registry response.
    let registry = include_str!(
        "../../../../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json"
    );
    let (rows, calls) = check(&tree, LIVE_DIGEST, registry).await;
    assert!(rows.is_empty(), "just pulled is up to date: {rows:?}");
    assert_eq!(calls, vec![TAGS, REGISTRY]);
}

#[tokio::test]
async fn v2_regular_copy_is_current_without_a_legacy_file() {
    let tree = TempTree::new("ollama-v2-copy");
    write(&tree, V2, REAL_MANIFEST);
    let (rows, calls) = check(&tree, LIVE_DIGEST, REAL_MANIFEST).await;
    assert!(rows.is_empty(), "copy fallback in linkManifest: {rows:?}");
    assert_eq!(calls, vec![TAGS, REGISTRY]);
}

#[tokio::test]
async fn legacy_fallback_is_per_model_and_accepts_a_dangling_v2_link() {
    for dangling in [false, true] {
        let tree = TempTree::new("ollama-v2-legacy");
        tree.dir("models/manifests-v2/ollama.com/library/qwen3.8");
        if dangling {
            symlink(
                format!("../../../../blobs/sha256-{LIVE_DIGEST}"),
                tree.at(V2),
            )
            .unwrap();
        }
        write(&tree, LEGACY, REAL_MANIFEST);
        let (rows, calls) = check(&tree, LIVE_DIGEST, REAL_MANIFEST).await;
        assert!(
            rows.is_empty(),
            "legacy model with v2 tree present: {rows:?}"
        );
        assert_eq!(calls, vec![TAGS, REGISTRY]);
    }
}

#[tokio::test]
async fn legacy_anchor_is_never_compared_even_if_a_downgraded_daemon_reports_it() {
    for digest_matches_anchor in [false, true] {
        let tree = TempTree::new("ollama-v2-anchor-only");
        let anchor = anchor();
        write(&tree, LEGACY, &anchor);
        let digest = if digest_matches_anchor {
            format!("{:x}", Sha256::digest(&anchor))
        } else {
            LIVE_DIGEST.into()
        };
        let (rows, calls) = check(&tree, &digest, REAL_MANIFEST).await;
        assert_uncheckable(&rows, &calls);
    }
}

#[tokio::test]
async fn invalid_v2_never_falls_back_to_an_otherwise_matching_legacy_manifest() {
    for bad in [
        "malformed",
        "oversized",
        "directory",
        "fifo",
        "digest-mismatch",
    ] {
        let tree = TempTree::new("ollama-v2-invalid");
        write(&tree, LEGACY, REAL_MANIFEST);
        let path = tree.at(V2);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        match bad {
            "malformed" => {
                write(&tree, V2, "not json");
            }
            "oversized" => {
                let file = std::fs::File::create(path).unwrap();
                file.set_len(crate::adapters::read_file::LIMIT + 1).unwrap();
            }
            "directory" => {
                std::fs::create_dir(path).unwrap();
            }
            "fifo" => crate::adapters::read_file::tests::make_fifo(&path),
            _ => {
                write(&tree, V2, &format!("{REAL_MANIFEST}\n"));
            }
        }
        let (rows, calls) = check(&tree, LIVE_DIGEST, REAL_MANIFEST).await;
        assert_uncheckable(&rows, &calls);
    }
}

#[tokio::test]
async fn protected_v2_blob_does_not_fall_back_or_contact_registry() {
    let tree = TempTree::new("ollama-v2-protected");
    write(&tree, LEGACY, REAL_MANIFEST);
    let blob = write(&tree, "Documents/manifest", REAL_MANIFEST);
    std::fs::create_dir_all(tree.at(V2).parent().unwrap()).unwrap();
    symlink(blob, tree.at(V2)).unwrap();
    let _protected = crate::protected::as_if_home(&tree.root);
    let (rows, calls) = check(&tree, LIVE_DIGEST, REAL_MANIFEST).await;
    assert_uncheckable(&rows, &calls);
    assert!(rows[0].warnings.contains(&Warning::NotLookedUpHere));
}

#[tokio::test]
async fn v2_still_reports_real_registry_changes_and_download_bytes() {
    let tree = TempTree::new("ollama-v2-changed");
    v2_link(&tree);
    write(&tree, LEGACY, &anchor());
    let mut registry: serde_json::Value = serde_json::from_str(REAL_MANIFEST).unwrap();
    registry["layers"][0]["digest"] = format!("sha256:{}", "a".repeat(64)).into();
    let (rows, calls) = check(&tree, LIVE_DIGEST, &registry.to_string()).await;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].checkable);
    assert_eq!(rows[0].download_bytes, Some(715_161_924));
    assert_eq!(calls, vec![TAGS, REGISTRY]);
}

// The recorded manifest of the model pulled on this Mac (byte-identical to
// ~/.ollama/models/manifests/registry.ollama.ai/library/qwen3.8/27b-mlx on
// 2026-10-07: 251725 bytes, 1209 layers, SHA-256 = LIVE_DIGEST).
const REAL_MANIFEST: &str = include_str!(
    "../../../../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json"
);
