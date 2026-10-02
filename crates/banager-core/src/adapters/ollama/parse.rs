use crate::adapters::AdapterError;
use crate::model::{ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Parses `ollama --version`'s "ollama version is X.Y.Z" output. A last
/// token with a control character in it is no version
/// (`sanity::version_token`).
pub fn parse_version(text: &str) -> Option<String> {
    let v = text.split_whitespace().last()?;
    crate::adapters::sanity::version_token(Some(v.to_string()))
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
    Ok(crate::adapters::sanity::artifacts(
        root.models
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
                facts: Default::default(),
            })
            .collect(),
    ))
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

/// One blob a manifest names -- a layer, or its config -- with the `size`
/// the manifest says it has. Its own shape, not `ManifestLayer`'s: a size
/// that is missing, `null`, negative, fractional or a string has to make
/// the download's size unknown (`changed_blob_bytes`), never make the
/// up-to-date check (`layer_digests`) fail on a manifest it reads today.
#[derive(Debug, Deserialize)]
struct SizedBlob {
    digest: String,
    #[serde(default)]
    size: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SizedManifest {
    #[serde(default)]
    layers: Vec<SizedBlob>,
    #[serde(default)]
    config: Option<SizedBlob>,
}

/// The most `ollama pull` can download to bring the local model to the
/// registry's manifest: the sum of the `size`s the registry manifest gives
/// its blobs -- layers and config -- whose digests the local manifest does
/// not name. Worked out from the two manifests the up-to-date check has
/// already read and fetched (`OllamaAdapter::compare_digests`): no other
/// request, file or command. An upper bound, not the download: a blob
/// another local model shares is already in `~/.ollama/models/blobs`, and
/// `ollama pull` skips it, but this number does not look in that folder.
///
/// Every entry counts, a digest listed twice -- or a config digest that is
/// also a layer's -- twice: Ollama's pull for a model with tensor layers
/// (`server/images.go` `pullWithTransfer`, `transfer/download.go`) starts
/// one download per entry left, with no de-duplication, so counting each
/// once could say less than it fetches. Its classic path de-duplicates;
/// there the number only says more. The one thing assumed: that the files
/// the local manifest names are on this Mac with their sizes -- the
/// transfer path fetches again one that is missing or a different size,
/// and only reading `blobs` could tell.
///
/// `None` whenever the number could be wrong: either manifest does not
/// parse, a blob to download has no size or one that is not a whole
/// number of bytes, one digest is given two different sizes, or the sum
/// does not fit in a `u64`. `Some(0)` when every blob is already named
/// locally.
pub fn changed_blob_bytes(local_json: &str, registry_json: &str) -> Option<u64> {
    let local: SizedManifest = serde_json::from_str(local_json).ok()?;
    let registry: SizedManifest = serde_json::from_str(registry_json).ok()?;
    let have: HashSet<&str> = local
        .layers
        .iter()
        .chain(local.config.iter())
        .map(|blob| blob.digest.as_str())
        .collect();
    // Each digest's size as first given, to catch one given two.
    let mut sizes: HashMap<&str, u64> = HashMap::new();
    let mut total: u64 = 0;
    for blob in registry.layers.iter().chain(registry.config.iter()) {
        let digest = blob.digest.as_str();
        if have.contains(digest) {
            continue;
        }
        let size = blob.size.as_ref()?.as_u64()?;
        if *sizes.entry(digest).or_insert(size) != size {
            return None;
        }
        total = total.checked_add(size)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_parse_version_and_parse_tags_refuse_control_characters() {
        assert_eq!(parse_version("ollama version is 0.34\u{1}1\n"), None);
        let json = r#"{"models":[{"name":"","digest":"a"},{"name":"qwen\u001b:1b","digest":"b"},
            {"name":"llama:1b","digest":"c\nd"}]}"#;
        let artifacts = parse_tags(json, "ollama:http://127.0.0.1:11434").unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "llama:1b");
        assert_eq!(artifacts[0].version, "");
    }

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

    /// A v2 manifest with `config` and `layers`, each `(digest, size)`; a
    /// `size` is spliced in as written, so a test can give one that is not
    /// a number, or leave it out (`None`).
    fn manifest(config: (&str, Option<&str>), layers: &[(&str, Option<&str>)]) -> String {
        let blob = |(digest, size): (&str, Option<&str>)| match size {
            Some(size) => format!(r#"{{"digest":"{digest}","size":{size}}}"#),
            None => format!(r#"{{"digest":"{digest}"}}"#),
        };
        format!(
            r#"{{"schemaVersion":2,"config":{},"layers":[{}]}}"#,
            blob(config),
            layers
                .iter()
                .map(|&l| blob(l))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    #[test]
    fn test_changed_blob_bytes_sums_the_registry_blobs_the_local_manifest_does_not_name() {
        let local = manifest(
            ("sha256:c1", Some("251")),
            &[("sha256:a", Some("100")), ("sha256:b", Some("200"))],
        );
        // `a` stays; `b` is replaced by `d`, and `e` and a new config come in.
        let registry = manifest(
            ("sha256:c2", Some("300")),
            &[
                ("sha256:a", Some("100")),
                ("sha256:d", Some("4683087000")),
                ("sha256:e", Some("20")),
            ],
        );
        assert_eq!(
            changed_blob_bytes(&local, &registry),
            Some(4_683_087_000 + 20 + 300)
        );
    }

    #[test]
    fn test_changed_blob_bytes_counts_neither_a_shared_layer_nor_an_unchanged_config() {
        // The config's blob is content-addressed like any layer: the same
        // digest is the same file, already pulled.
        let local = manifest(("sha256:c1", Some("251")), &[("sha256:a", Some("100"))]);
        let registry = manifest(
            ("sha256:c1", Some("251")),
            &[("sha256:a", Some("100")), ("sha256:n", Some("7"))],
        );
        assert_eq!(changed_blob_bytes(&local, &registry), Some(7));
        assert_eq!(changed_blob_bytes(&local, &local), Some(0));
    }

    #[test]
    fn test_changed_blob_bytes_counts_a_blob_listed_twice_twice() {
        // Ollama's transfer path downloads each entry it has left, with no
        // de-duplication: counted once, the number could say less.
        let local = manifest(("sha256:c1", Some("251")), &[]);
        let registry = manifest(
            ("sha256:c1", Some("251")),
            &[("sha256:n", Some("7")), ("sha256:n", Some("7"))],
        );
        assert_eq!(changed_blob_bytes(&local, &registry), Some(14));
    }

    #[test]
    fn test_changed_blob_bytes_counts_a_config_that_is_also_a_layer_twice() {
        // The case Ollama's own pull comments on (`server/images.go`): the
        // config's digest is one of the layers'. Both entries count when
        // new; neither when the local manifest names the digest, as a
        // layer or as its config.
        let registry = manifest(
            ("sha256:x", Some("9")),
            &[("sha256:x", Some("9")), ("sha256:n", Some("7"))],
        );
        let none_local = manifest(("sha256:c1", Some("251")), &[]);
        assert_eq!(changed_blob_bytes(&none_local, &registry), Some(9 + 9 + 7));
        let as_layer = manifest(("sha256:c1", Some("251")), &[("sha256:x", Some("9"))]);
        assert_eq!(changed_blob_bytes(&as_layer, &registry), Some(7));
        let as_config = manifest(("sha256:x", Some("9")), &[]);
        assert_eq!(changed_blob_bytes(&as_config, &registry), Some(7));
        // The two entries disagreeing on its size: unknown.
        let disagreeing = manifest(
            ("sha256:x", Some("10")),
            &[("sha256:x", Some("9")), ("sha256:n", Some("7"))],
        );
        assert_eq!(changed_blob_bytes(&none_local, &disagreeing), None);
    }

    #[test]
    fn test_changed_blob_bytes_is_unknown_when_any_number_could_be_wrong() {
        let local = manifest(("sha256:c1", Some("251")), &[("sha256:a", Some("100"))]);
        let with = |size: Option<&str>| {
            manifest(
                ("sha256:c1", Some("251")),
                &[("sha256:a", Some("100")), ("sha256:n", size)],
            )
        };
        // A changed layer with no size, or one that is not a whole number
        // of bytes.
        for size in [None, Some("null"), Some("-1"), Some("1.5"), Some(r#""7""#)] {
            assert_eq!(
                changed_blob_bytes(&local, &with(size)),
                None,
                "size {size:?}"
            );
        }
        // A changed config with no size.
        let registry = manifest(("sha256:c2", None), &[("sha256:a", Some("100"))]);
        assert_eq!(changed_blob_bytes(&local, &registry), None);
        // One digest, two sizes.
        let registry = manifest(
            ("sha256:c1", Some("251")),
            &[("sha256:n", Some("7")), ("sha256:n", Some("8"))],
        );
        assert_eq!(changed_blob_bytes(&local, &registry), None);
        // More than a u64 holds.
        let registry = manifest(
            ("sha256:c1", Some("251")),
            &[
                ("sha256:n", Some(&u64::MAX.to_string())),
                ("sha256:m", Some("1")),
            ],
        );
        assert_eq!(changed_blob_bytes(&local, &registry), None);
        // A manifest that is not one.
        assert_eq!(changed_blob_bytes(&local, "not json"), None);
        assert_eq!(changed_blob_bytes("{", &local), None);
    }

    #[test]
    fn test_changed_blob_bytes_ignores_a_bad_size_on_a_blob_already_named_locally() {
        // Nothing to download from `a`, so what its size says does not
        // matter -- and it never stops the up-to-date check reading the
        // manifest (`layer_digests` has a shape of its own).
        let local = manifest(("sha256:c1", Some("251")), &[("sha256:a", Some("100"))]);
        let registry = manifest(
            ("sha256:c1", Some("251")),
            &[("sha256:a", Some(r#""big""#)), ("sha256:n", Some("7"))],
        );
        assert_eq!(changed_blob_bytes(&local, &registry), Some(7));
        assert_eq!(
            layer_digests(&registry).expect("a string size still parses"),
            HashSet::from(["sha256:a".to_string(), "sha256:n".to_string()])
        );
    }

    #[test]
    fn test_changed_blob_bytes_of_the_recorded_up_to_date_pair_is_zero() {
        let local = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");
        assert_eq!(changed_blob_bytes(&local, &registry), Some(0));
        // Against a local manifest naming none of its blobs, the whole
        // recorded model: its 1209 layers' sizes and its config's 251.
        let empty = manifest(("sha256:none", Some("0")), &[]);
        assert_eq!(
            changed_blob_bytes(&empty, &registry),
            Some(18_174_721_596 + 251)
        );
    }
}
