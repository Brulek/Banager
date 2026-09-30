use crate::adapters::AdapterError;
use crate::model::{ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact};
use serde::Deserialize;
use std::collections::HashSet;

/// Parses `ollama --version`'s "ollama version is X.Y.Z" output.
pub fn parse_version(text: &str) -> Option<String> {
    let v = text.split_whitespace().last()?;
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct TagsRoot {
    #[serde(default)]
    models: Vec<TagsModel>,
}

#[derive(Debug, Deserialize)]
struct TagsModel {
    name: String,
    digest: String,
    #[serde(default)]
    size: u64,
}

/// Parses `GET {host}/api/tags`'s body into the models Ollama currently has
/// pulled. `TagsModel::name` is the full `name:tag` form (e.g.
/// `qwen3.8:27b-mlx`) exactly as Ollama reports it, which is what
/// `split_model_reference` expects. There is no per-model install
/// timestamp in this response, so `installed_at` is always `None`.
pub fn parse_tags(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: TagsRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(root
        .models
        .into_iter()
        .map(|m| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Model,
                name: m.name.clone(),
            },
            display_name: m.name,
            version: m.digest,
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: Some(m.size),
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
        })
        .collect())
}

/// Splits an Ollama model reference (`name:tag`, e.g. `qwen3.8:27b-mlx`, or
/// `namespace/name:tag`) into `(namespace, name, tag)`. A bare name with no
/// `/` uses Ollama's default namespace, `library`; a reference with no
/// `:tag` uses Ollama's default tag, `latest`.
pub fn split_model_reference(reference: &str) -> (String, String, String) {
    let (name_part, tag) = match reference.split_once(':') {
        Some((n, t)) => (n, t.to_string()),
        None => (reference, "latest".to_string()),
    };
    match name_part.split_once('/') {
        Some((ns, n)) => (ns.to_string(), n.to_string(), tag),
        None => ("library".to_string(), name_part.to_string(), tag),
    }
}

#[derive(Debug, Deserialize)]
struct ManifestLayer {
    digest: String,
}

#[derive(Debug, Deserialize)]
struct ManifestConfig {
    digest: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    layers: Vec<ManifestLayer>,
    #[serde(default)]
    config: Option<ManifestConfig>,
}

/// Parses a v2 Docker-distribution manifest (the shape both the local
/// `~/.ollama/models/manifests/...` file and the registry's `GET
/// /v2/{ns}/{name}/manifests/{tag}` response use) into the set of its
/// layer digests. Comparing this set — not the serialized manifest bytes —
/// is what makes the up-to-date check correct: Ollama rewrites the local
/// manifest file on disk, so a byte-for-byte comparison would report a
/// false "outdated" for a model that has not actually changed.
pub fn layer_digests(json: &str) -> Result<HashSet<String>, AdapterError> {
    let manifest: Manifest =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(manifest.layers.into_iter().map(|l| l.digest).collect())
}

/// The manifest's `config.digest`, the one short, stable string that names
/// *this* build of the model. `UpdateCandidate`'s contract is that `current`
/// and `target` differ, and a model's tag (`27b-mlx`) does not change when
/// the model behind it is republished — so the tag cannot be the target.
/// `None` when the manifest carries no config section.
pub fn config_digest(json: &str) -> Result<Option<String>, AdapterError> {
    let manifest: Manifest =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(manifest.config.map(|c| c.digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_reads_the_last_token() {
        // adapters/fixtures/ollama/0.34.1/version.txt: "ollama version is 0.34.1"
        assert_eq!(
            parse_version("ollama version is 0.34.1\n"),
            Some("0.34.1".to_string())
        );
    }

    #[test]
    fn test_parse_tags_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture");
        let artifacts =
            parse_tags(&json, "ollama:http://127.0.0.1:11434").expect("parse api-tags.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Model);
        assert_eq!(artifacts[0].key.name, "qwen3.8:27b-mlx");
        assert_eq!(
            artifacts[0].version,
            "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e"
        );
        assert_eq!(artifacts[0].size_bytes, Some(18174721847));
    }

    #[test]
    fn test_split_model_reference_handles_namespace_and_tag_defaults() {
        assert_eq!(
            split_model_reference("qwen3.8:27b-mlx"),
            (
                "library".to_string(),
                "qwen3.8".to_string(),
                "27b-mlx".to_string()
            )
        );
        assert_eq!(
            split_model_reference("someuser/somemodel:sometag"),
            (
                "someuser".to_string(),
                "somemodel".to_string(),
                "sometag".to_string()
            )
        );
        assert_eq!(
            split_model_reference("llama3"),
            (
                "library".to_string(),
                "llama3".to_string(),
                "latest".to_string()
            )
        );
    }

    #[test]
    fn test_config_digest_of_the_recorded_local_and_registry_manifests_is_the_same_string() {
        // The config digest is what check_updates reports as an available
        // update's `target`; the recorded pair is the already-up-to-date
        // case, so the two must agree here too.
        let local = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");
        let local_config = config_digest(&local).expect("parse local manifest");
        assert!(
            local_config.is_some(),
            "the recorded manifest has a config section"
        );
        assert_eq!(
            local_config,
            config_digest(&registry).expect("parse registry manifest")
        );
    }

    #[test]
    fn test_layer_digests_of_the_recorded_local_and_registry_manifests_are_equal() {
        let local = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");
        let local_digests = layer_digests(&local).expect("parse local manifest");
        let registry_digests = layer_digests(&registry).expect("parse registry manifest");
        assert_eq!(local_digests.len(), 1209);
        assert_eq!(registry_digests.len(), 1209);
        assert_eq!(
            local_digests, registry_digests,
            "the recorded fixture pair is the already-up-to-date case"
        );
    }
}
