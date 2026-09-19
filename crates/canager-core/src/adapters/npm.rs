use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct LsGlobalRoot {
    #[serde(default)]
    dependencies: HashMap<String, LsGlobalDependency>,
}

#[derive(Debug, Deserialize)]
struct LsGlobalDependency {
    #[serde(default)]
    version: Option<String>,
}

/// Parses `npm ls -g --depth=0 --json`. The real, committed fixture
/// (`adapters/fixtures/npm/12.0.2/ls-global.json`) shows the top level is a
/// `dependencies` **object** keyed by package name, not an array — a parser
/// expecting an array silently sees zero packages instead of erroring.
/// Sorted by name for deterministic output (a `HashMap`'s own iteration
/// order is not).
fn parse_ls_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, crate::adapters::AdapterError> {
    let root: LsGlobalRoot = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<InstalledArtifact> = root
        .dependencies
        .into_iter()
        .map(|(name, dep)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            display_name: name,
            version: dep.version.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct OutdatedEntry {
    current: String,
    latest: String,
}

/// Parses `npm outdated -g --json`. npm exits 1 whenever it finds anything
/// outdated — the caller must still treat that stdout as the real result,
/// not an error (see the per-adapter contract table). Empty stdout (no
/// output at all, not even `{}`) means nothing is outdated.
fn parse_outdated_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, crate::adapters::AdapterError> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: HashMap<String, OutdatedEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<UpdateCandidate> = root
        .into_iter()
        .map(|(name, entry)| UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            current: entry.current,
            target: entry.latest,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct SearchEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// Parses `npm search --json --searchlimit 20 {query}`.
fn parse_search(
    json: &str,
    adapter_id: &str,
) -> Result<Vec<SearchHit>, crate::adapters::AdapterError> {
    let entries: Vec<SearchEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|e| SearchHit {
            adapter_id: adapter_id.to_string(),
            kind: ArtifactKind::Package,
            name: e.name,
            description: e.description,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ls_global_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        let artifacts = parse_ls_global(&json, "npm:/opt/homebrew/lib").expect("parse");
        assert_eq!(artifacts.len(), 6);
        let names: Vec<&str> = artifacts.iter().map(|a| a.key.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "@alisaitteke/photoshop-mcp",
                "@openai/codex",
                "corepack",
                "get-shit-done-cc",
                "npm",
                "zsxq-cli",
            ]
        );
        let npm_self = artifacts
            .iter()
            .find(|a| a.key.name == "npm")
            .expect("npm entry");
        assert_eq!(npm_self.version, "12.0.2");
        assert_eq!(npm_self.key.kind, ArtifactKind::Package);
    }

    #[test]
    fn parse_outdated_global_matches_the_recorded_fixture() {
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read adapters/fixtures/npm/12.0.2/outdated-global.json");
        let candidates = parse_outdated_global(&json, "npm:/opt/homebrew/lib").expect("parse");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "@alisaitteke/photoshop-mcp");
        assert_eq!(candidates[0].current, "1.7.15");
        assert_eq!(candidates[0].target, "1.7.17");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[test]
    fn parse_outdated_global_of_empty_stdout_is_no_updates() {
        // With nothing outdated npm prints either nothing at all or `{}`,
        // depending on version; both mean "no updates". Neither is committed
        // as a fixture, since there is nothing to record — but the parser
        // must not choke on either.
        let candidates = parse_outdated_global("", "npm:/opt/homebrew/lib").expect("parse");
        assert!(candidates.is_empty());
        assert!(parse_outdated_global("{}", "npm:/opt/homebrew/lib")
            .expect("parse")
            .is_empty());
    }

    #[test]
    fn parse_search_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read adapters/fixtures/npm/12.0.2/search-jq.json");
        let hits = parse_search(&json, "npm").expect("parse");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(
            hits[0].description.as_deref(),
            Some("Server-side jQuery wrapper for node.")
        );
        assert!(hits.iter().all(|h| h.adapter_id == "npm"));
    }
}
