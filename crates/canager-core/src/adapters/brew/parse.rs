use crate::adapters::AdapterError;
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct InfoInstalledRoot {
    #[serde(default)]
    formulae: Vec<FormulaInfo>,
    #[serde(default)]
    casks: Vec<CaskInfo>,
}

#[derive(Debug, Deserialize)]
struct FormulaInfo {
    name: String,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    linked_keg: Option<String>,
    #[serde(default)]
    installed: Vec<FormulaInstalledEntry>,
}

#[derive(Debug, Deserialize)]
struct FormulaInstalledEntry {
    version: String,
    #[serde(default)]
    installed_on_request: bool,
    #[serde(default)]
    time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CaskInfo {
    token: String,
    #[serde(default)]
    full_token: Option<String>,
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    installed: Option<String>,
    #[serde(default)]
    auto_updates: Option<bool>,
}

/// Parses `brew info --installed --json=v2`. For each formula, picks the
/// `installed` entry whose `version` matches `linked_keg` (falling back to
/// the last entry in the chronological array if no match, e.g. an unlinked
/// keg-only formula). Homebrew's JSON records only a single
/// `installed_on_request` boolean per installed entry (there is no separate
/// "installed as dependency" field): `true` -> `Requested`, `false` ->
/// `Dependency`; `Unknown` is reserved for the case where a formula has no
/// installed entry to pick from at all. Casks have no install-reason field
/// in brew's JSON, so they are always `Requested`.
///
/// `ArtifactKey.name` always uses the *fully qualified* name — a formula's
/// `full_name` (e.g. a core formula's own `name` if it has no tap prefix) or
/// a cask's `full_token` (e.g. `gautham-v/tap/claudebar`) when brew reports
/// one, falling back to the short `name`/`token` otherwise. This matters
/// because third-party taps only disambiguate by their full name, and using
/// the short name as the key would make `reconcile` (see `brew/mod.rs`)
/// unable to ever find a tapped artifact again. The short name is preserved
/// in `display_name` for formulae; casks already use their human-readable
/// `name[0]` for `display_name`, so no separate short-name field is needed
/// there.
pub fn parse_info_installed(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: InfoInstalledRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut out = Vec::new();

    for f in root.formulae {
        let picked = f
            .installed
            .iter()
            .find(|entry| Some(&entry.version) == f.linked_keg.as_ref())
            .or_else(|| f.installed.last());

        let (version, reason, installed_at) = match picked {
            Some(entry) => {
                let reason = if entry.installed_on_request {
                    InstallReason::Requested
                } else {
                    InstallReason::Dependency
                };
                (entry.version.clone(), reason, entry.time)
            }
            None => (String::new(), InstallReason::Unknown, None),
        };

        let key_name = f
            .full_name
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| f.name.clone());

        out.push(InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: key_name,
            },
            display_name: f.name,
            version,
            reason,
            description: f.desc,
            homepage: f.homepage,
            size_bytes: None,
            installed_at,
            path: None,
            auto_updates: false,
        });
    }

    for c in root.casks {
        let key_name = c
            .full_token
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| c.token.clone());
        let display_name = c.name.into_iter().next().unwrap_or_else(|| c.token.clone());
        out.push(InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Cask,
                name: key_name,
            },
            display_name,
            version: c.installed.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: c.desc,
            homepage: c.homepage,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: c.auto_updates.unwrap_or(false),
        });
    }

    Ok(out)
}

#[derive(Debug, Deserialize)]
struct OutdatedRoot {
    #[serde(default)]
    formulae: Vec<OutdatedItem>,
    #[serde(default)]
    casks: Vec<OutdatedItem>,
}

#[derive(Debug, Deserialize)]
struct OutdatedItem {
    name: String,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    installed_versions: Vec<String>,
    current_version: String,
    // `pinned` and `pinned_version` used to be listed here too, unread,
    // behind `#[allow(dead_code)]`. serde ignores keys a struct does not
    // mention, so declaring them bought nothing at all and the `allow` was
    // what kept the compiler from saying so. What pinned items actually
    // need is per-package actionability (`UpdateCandidate.actionable: bool`
    // plus a reason enum whose first variant is `Pinned`, spec §8), which
    // is backlogged; it will read them from the JSON then.
}

/// Parses `brew outdated --json=v2`.
pub fn parse_outdated(json: &str, instance_id: &str) -> Result<Vec<UpdateCandidate>, AdapterError> {
    let root: OutdatedRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut out = Vec::new();

    let items = root
        .formulae
        .into_iter()
        .map(|i| (i, ArtifactKind::Formula))
        .chain(root.casks.into_iter().map(|i| (i, ArtifactKind::Cask)));

    for (item, kind) in items {
        let current = item.installed_versions.last().cloned().unwrap_or_default();
        let key_name = item
            .full_name
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| item.name.clone());
        out.push(UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind,
                name: key_name,
            },
            current,
            target: item.current_version,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        });
    }

    Ok(out)
}

/// Parses `brew search` / `brew search --desc` output.
///
/// `brew search --desc` prints explicit `==> Formulae` / `==> Casks` section
/// headers, so lines are attributed to whichever section header preceded
/// them. Plain `brew search` (no `--desc`), however, prints **no** section
/// headers at all when the output is not a TTY — real fixture output looks
/// like a flat list of names with a single blank line separating the
/// formula names from the cask names (confirmed against `brew search jq`
/// on a real Mac; see `adapters/fixtures/brew/7.0.3/search-jq.txt`). To
/// handle both shapes, a text with no headers anywhere treats the first
/// blank-line-delimited group of names as formulae and every subsequent
/// group as casks; a text with headers uses them as usual. Blank lines,
/// `If you meant` and `Error:` lines are ignored either way.
pub fn parse_search(text: &str, adapter_id: &str) -> Vec<SearchHit> {
    let has_headers = text
        .lines()
        .any(|l| matches!(l.trim(), "==> Formulae" | "==> Casks"));

    let mut hits = Vec::new();
    let mut current_kind: Option<ArtifactKind> = None;
    let mut block_has_content = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with("If you meant") || line.starts_with("Error:") {
            continue;
        }
        if line == "==> Formulae" {
            current_kind = Some(ArtifactKind::Formula);
            continue;
        }
        if line == "==> Casks" {
            current_kind = Some(ArtifactKind::Cask);
            continue;
        }
        if line.is_empty() {
            if !has_headers && block_has_content {
                // Headerless format: the blank line marks the end of the
                // formula group and the start of the cask group.
                current_kind = Some(ArtifactKind::Cask);
            }
            block_has_content = false;
            continue;
        }
        if !has_headers && current_kind.is_none() {
            // Headerless format: the first group of names is formulae.
            current_kind = Some(ArtifactKind::Formula);
        }
        block_has_content = true;
        let Some(kind) = current_kind else {
            continue;
        };
        let (name, description) = match line.split_once(':') {
            Some((n, d)) => (n.trim().to_string(), Some(d.trim().to_string())),
            None => (line.to_string(), None),
        };
        hits.push(SearchHit {
            adapter_id: adapter_id.to_string(),
            kind,
            name,
            description,
        });
    }

    hits
}

/// Parses `brew uses --installed {name}`: one whitespace-separated formula
/// name per token (brew prints one per line, but splitting on all whitespace
/// is robust to either layout).
pub fn parse_uses(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_string()).collect()
}

/// Parses `brew --version`'s first line, e.g. "Homebrew 7.0.3", returning
/// just the version.
pub fn parse_version(text: &str) -> Option<String> {
    let first_line = text.lines().next()?;
    let mut parts = first_line.split_whitespace();
    let _label = parts.next()?; // "Homebrew"
    let version = parts.next()?;
    Some(version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real `brew outdated --json=v2` fixtures never carry `full_name`
    // (confirmed against `adapters/fixtures/brew/*/outdated.json`), so the
    // fallback in `parse_outdated` — using `full_name` when present,
    // `name` otherwise — is untested by the fixture-driven snapshot tests.
    // These two inline-JSON cases cover both branches directly: a tapped
    // formula (where `full_name` disambiguates from a same-named formula
    // in another tap) and a plain formula with no `full_name` at all.

    #[test]
    fn parse_outdated_uses_full_name_when_present() {
        let json = r#"{
            "formulae": [
                {
                    "name": "jq",
                    "full_name": "myorg/tap/jq",
                    "installed_versions": ["1.6"],
                    "current_version": "1.7",
                    "pinned": false,
                    "pinned_version": null
                }
            ],
            "casks": []
        }"#;

        let result = parse_outdated(json, "brew:/opt/homebrew").expect("parse");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key.name, "myorg/tap/jq");
    }

    #[test]
    fn parse_outdated_falls_back_to_name_when_full_name_absent() {
        let json = r#"{
            "formulae": [
                {
                    "name": "jq",
                    "installed_versions": ["1.6"],
                    "current_version": "1.7",
                    "pinned": false,
                    "pinned_version": null
                }
            ],
            "casks": []
        }"#;

        let result = parse_outdated(json, "brew:/opt/homebrew").expect("parse");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key.name, "jq");
    }
}
