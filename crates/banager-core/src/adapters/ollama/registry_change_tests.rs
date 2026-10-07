//! What counts as a registry change of a model, and what the update offers
//! (r40 R40-1, R40-4). Each test has its own temporary `~/.ollama`, a mocked
//! daemon and a mocked registry: nothing reads this Mac's models or asks a
//! network. The shapes are recorded ones: the qwen3.8 manifests and the
//! `/api/tags` row under `adapters/fixtures/ollama/0.34.1/`, and the cloud
//! model's manifest below.

use super::*;
use crate::http::mock::MockHttpClient;
use crate::http::HttpResponse;
use crate::runner::mock::MockRunner;
use crate::testing::TempTree;
use sha2::{Digest, Sha256};

const TAGS: &str = "http://127.0.0.1:11434/api/tags";

/// The recorded qwen3.8:27b-mlx pair, byte-identical (the up-to-date case).
const QWEN: &str = include_str!(
    "../../../../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json"
);
const QWEN_REGISTRY: &str = include_str!(
    "../../../../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json"
);
const QWEN_NAME: &str = "qwen3.8:27b-mlx";

/// `gpt-oss:120b-cloud`, one of Ollama's cloud models, as
/// `GET https://registry.ollama.ai/v2/library/gpt-oss/manifests/120b-cloud`
/// (with Banager's `Accept` header) answered it read-only on 2026-10-07 for
/// the r40 review, byte for byte: 264 bytes and no layers -- the whole model
/// is its 307-byte config (`remote_host`, `remote_model`, capabilities...).
/// `ollama pull` writes the manifest as the registry sent it, so this is
/// also the local file, and its SHA-256 the `/api/tags` digest.
const CLOUD: &str = r#"{"config":{"digest":"sha256:dad8bd034e571c7856a11969a6a9d59a0b8a10a13a39b9f5899b308165d6ddb7","mediaType":"application/vnd.docker.container.image.v1+json","size":307},"layers":[],"mediaType":"application/vnd.docker.distribution.manifest.v2+json","schemaVersion":2}"#;
const CLOUD_NAME: &str = "gpt-oss:120b-cloud";

fn sha256_hex(bytes: &str) -> String {
    format!("{:x}", Sha256::digest(bytes.as_bytes()))
}

/// `manifest` with its config's digest and size replaced: what the registry
/// serves after a config-only republish.
fn with_config(manifest: &str, digest: &str, size: u64) -> String {
    let mut json: serde_json::Value = serde_json::from_str(manifest).unwrap();
    json["config"]["digest"] = digest.into();
    json["config"]["size"] = size.into();
    serde_json::to_string(&json).unwrap()
}

/// `manifest` with layer `index`'s digest replaced: a weights-only
/// republish, the config left as it was.
fn with_layer(manifest: &str, index: usize, digest: &str) -> String {
    let mut json: serde_json::Value = serde_json::from_str(manifest).unwrap();
    json["layers"][index]["digest"] = digest.into();
    serde_json::to_string(&json).unwrap()
}

/// `check_updates` for one model, `name`, pulled from Ollama's library on
/// this Mac: its manifest `local` where Ollama 0.40 keeps it (a regular
/// file at its `manifests-v2` entry), the daemon's `/api/tags` row -- the
/// recorded one, renamed -- giving that file's SHA-256, and the registry
/// answering `registry`. Returns the rows and every URL asked.
async fn check(name: &str, local: &str, registry: &str) -> (Vec<UpdateCandidate>, Vec<String>) {
    let tree = TempTree::new("ollama-registry-change");
    let (namespace, model, tag) = parse::split_model_reference(name);
    let path = tree.at(&format!(
        "models/manifests-v2/ollama.com/{namespace}/{model}/{tag}"
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, local).unwrap();
    let mut tags: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../adapters/fixtures/ollama/0.34.1/api-tags.json"
    ))
    .unwrap();
    tags["models"][0]["name"] = name.into();
    tags["models"][0]["model"] = name.into();
    tags["models"][0]["digest"] = sha256_hex(local).into();
    let registry_url = format!("https://registry.ollama.ai/v2/{namespace}/{model}/manifests/{tag}");
    let http = Arc::new(MockHttpClient::new());
    http.respond(
        TAGS,
        HttpResponse {
            status: 200,
            body: tags.to_string(),
        },
    );
    http.respond(
        &registry_url,
        HttpResponse {
            status: 200,
            body: registry.to_string(),
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
    let calls = http.calls();
    assert_eq!(calls, vec![TAGS.to_string(), registry_url], "{name}");
    (rows, calls)
}

#[tokio::test]
async fn a_republish_that_changes_only_the_config_is_an_update() {
    // The recorded model's config (251 bytes) names its renderer, parser,
    // capabilities and the Ollama it requires; a republish moving it to a
    // new parser changes that blob and no layer. `ollama pull` fetches it
    // and writes the new manifest, so the model is not up to date.
    let new_config = format!("sha256:{}", "c".repeat(64));
    let registry = with_config(QWEN_REGISTRY, &new_config, 412);
    let (rows, _) = check(QWEN_NAME, QWEN, &registry).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].checkable, "an update Banager can offer: {rows:?}");
    assert_eq!(rows[0].channel, UpdateChannel::Digest);
    assert_eq!(rows[0].current, sha256_hex(QWEN));
    // Only the new config is new to this Mac.
    assert_eq!(rows[0].download_bytes, Some(412));

    // The same model, its manifest only spelled differently (its keys in
    // another order), is the same model: no update.
    let respelled =
        serde_json::to_string(&serde_json::from_str::<serde_json::Value>(QWEN_REGISTRY).unwrap())
            .unwrap();
    assert_ne!(respelled, QWEN_REGISTRY);
    let (rows, _) = check(QWEN_NAME, QWEN, &respelled).await;
    assert!(rows.is_empty(), "{rows:?}");
}

#[tokio::test]
async fn a_cloud_model_has_an_update_when_its_config_changes_and_none_otherwise() {
    // No layers on either side: the two empty sets are always equal, so
    // only the config can say the model changed.
    let (rows, _) = check(CLOUD_NAME, CLOUD, CLOUD).await;
    assert!(
        rows.is_empty(),
        "the recorded manifest is up to date: {rows:?}"
    );

    let new_config = format!("sha256:{}", "d".repeat(64));
    let registry = with_config(CLOUD, &new_config, 307);
    let (rows, _) = check(CLOUD_NAME, CLOUD, &registry).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].checkable, "{rows:?}");
    assert_eq!(rows[0].key.name, CLOUD_NAME);
    assert_eq!(rows[0].current, sha256_hex(CLOUD));
    assert_eq!(rows[0].download_bytes, Some(307));
}

#[tokio::test]
async fn a_republish_that_changes_only_a_layer_is_still_an_update() {
    // The other half of the rule, as before: the config the same, one
    // layer (lm_head.weight, 715161924 bytes) new.
    let registry = with_layer(QWEN_REGISTRY, 0, &format!("sha256:{}", "a".repeat(64)));
    let (rows, _) = check(QWEN_NAME, QWEN, &registry).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].checkable);
    assert_eq!(rows[0].download_bytes, Some(715_161_924));
}

#[tokio::test]
async fn each_republish_is_offered_as_a_build_of_its_own() {
    // Two weights-only republishes of the recorded model -- one with a new
    // lm_head.weight, one with a new embed_tokens.weight -- keep its config
    // (`sha256:25a98d24...`, which names no layer). Offered by that config
    // digest, both were one "version", and Skip This Version on the first
    // hid the second for good. Each is now named by its own manifest.
    let installed_config = parse::config_digest(QWEN).unwrap().unwrap();
    let first = with_layer(QWEN_REGISTRY, 0, &format!("sha256:{}", "a".repeat(64)));
    let second = with_layer(QWEN_REGISTRY, 1, &format!("sha256:{}", "b".repeat(64)));
    let config_only = with_config(QWEN_REGISTRY, &format!("sha256:{}", "c".repeat(64)), 412);
    let mut targets = Vec::new();
    for (registry, download) in [
        (&first, 715_161_924),
        (&second, 2_542_796_928),
        (&config_only, 412),
    ] {
        let (rows, _) = check(QWEN_NAME, QWEN, registry).await;
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(rows[0].checkable);
        assert_eq!(rows[0].download_bytes, Some(download));
        // `sha256:` and the SHA-256 of the manifest as the registry sent
        // it: what its Docker-Content-Digest would say.
        assert_eq!(rows[0].target, format!("sha256:{}", sha256_hex(registry)));
        assert_ne!(rows[0].target, installed_config);
        targets.push(rows[0].target.clone());
    }
    targets.sort();
    targets.dedup();
    assert_eq!(targets.len(), 3, "one target per build: {targets:?}");
}

#[tokio::test]
async fn once_pulled_the_offered_build_is_up_to_date() {
    // `ollama pull` writes the manifest as the registry sent it (0.40
    // `WriteManifestData(n, manifestData)`), and `/api/tags` names it by
    // its SHA-256: the hex of the `target` that was offered. The next
    // check finds that model up to date.
    let republished = with_layer(QWEN_REGISTRY, 0, &format!("sha256:{}", "a".repeat(64)));
    let (rows, _) = check(QWEN_NAME, QWEN, &republished).await;
    let target = rows[0].target.clone();
    let (rows, _) = check(QWEN_NAME, &republished, &republished).await;
    assert!(rows.is_empty(), "{rows:?}");
    assert_eq!(target, format!("sha256:{}", sha256_hex(&republished)));

    // The cloud model likewise.
    let new_config = with_config(CLOUD, &format!("sha256:{}", "d".repeat(64)), 307);
    let (rows, _) = check(CLOUD_NAME, CLOUD, &new_config).await;
    assert_eq!(
        rows[0].target,
        format!("sha256:{}", sha256_hex(&new_config))
    );
    let (rows, _) = check(CLOUD_NAME, &new_config, &new_config).await;
    assert!(rows.is_empty(), "{rows:?}");
}

#[tokio::test]
async fn a_registry_answer_with_no_config_is_no_update_to_offer() {
    // A 200 whose body parses but is no model -- here `{}`, read as a
    // manifest with no layers and no config -- differs from every model,
    // and must not be offered as one with nothing to download.
    let (rows, _) = check(QWEN_NAME, QWEN, "{}").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(!rows[0].checkable, "{rows:?}");
    assert_eq!(rows[0].target, rows[0].current);
    assert_eq!(rows[0].download_bytes, None);
}
