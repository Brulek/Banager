use crate::adapters::AdapterError;
use crate::model::{
    ArtifactFacts, ArtifactKey, ArtifactKind, CommandInputs, HomebrewFacts, HomebrewLifecycle,
    InstallReason, InstalledArtifact, ProvidedCommand, SearchHit, UninstallBlocked, UpdateBlocked,
    UpdateCandidate, UpdateChannel,
};
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

// Both partitions are required, deliberately. `brew info --installed
// --json=v2` and `brew outdated --json=v2` always emit both keys, the
// empty one as `[]` (verified against `brew info --json=v2 jq`, which
// answers `"casks": []`). A reply that omits one — `{}`, an error object,
// the output of some wrapper that is not brew — is not brew reporting
// nothing; it is not the reply we asked for. With `#[serde(default)]`
// here it parsed as "you have nothing installed", which is silent, total
// and the single worst thing this app can tell someone. Refusing it
// instead routes the source into the "Banager couldn't check this" state
// the UI already knows how to explain.
#[derive(Debug, Deserialize)]
struct InfoInstalledRoot {
    formulae: Vec<FormulaInfo>,
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
    /// `keg_only` (`formula.rb`'s `"keg_only" => keg_only?`): Homebrew
    /// keeps the formula out of its `bin` on purpose, so Banager says
    /// nothing about its commands (`CommandInputs.keg_only`). Read as any
    /// JSON value, so a brew that one day writes something else costs this
    /// one judgement and not the whole inventory: anything but `true` is
    /// not keg-only.
    #[serde(default)]
    keg_only: Option<serde_json::Value>,
    #[serde(default)]
    installed: Vec<FormulaInstalledEntry>,
    /// `brew pin`. `brew info --json=v2` writes it for every formula
    /// (`"pinned" => pinned?`, `formula.rb:3140` in Homebrew 7.0.6), and
    /// casks have the same key (`CaskInfo.pinned`). Becomes
    /// `UninstallBlocked::Pinned` in `parse_info_installed`. Defaulted for
    /// the same reason as `OutdatedItem.pinned`: a missing key is a brew
    /// that did not say the package is pinned, and the worst it costs is
    /// Homebrew's own refusal.
    #[serde(default)]
    pinned: bool,
    #[serde(flatten)]
    status: StatusFields,
}

#[derive(Debug, Deserialize)]
struct FormulaInstalledEntry {
    version: String,
    // `Option`, not a defaulted `bool`: `false` is brew saying something
    // else pulled this in, and the field being absent is brew saying
    // nothing. Collapsing the second into the first filed unknown-origin
    // packages as dependencies, which hides them behind the Installed
    // page's collapsed "N components installed by other software" — the
    // one place a user will not look for something they did not install.
    #[serde(default)]
    installed_on_request: Option<bool>,
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
    /// `brew pin --cask` (`"pinned" => pinned?`, `cask/cask.rb:574`). See
    /// `FormulaInfo.pinned`.
    #[serde(default)]
    pinned: bool,
    /// When brew installed it, in unix seconds (`"installed_time" =>
    /// install_time&.to_i`, `cask/cask.rb:571` in Homebrew 7.0.7; all four
    /// casks of the recorded `7.0.3/info-installed.json` carry one). Read
    /// leniently (`StatusFields` says why): anything but a positive whole
    /// number is no date.
    #[serde(default)]
    installed_time: Option<Value>,
    #[serde(flatten)]
    status: StatusFields,
    /// The cask's stanzas as brew lists them. Defaulted: a brew that omits
    /// the key has said nothing about where the app went, and the cost is
    /// the one this field exists to remove -- the cask's command listed on
    /// the Unknown page.
    #[serde(default)]
    artifacts: Vec<CaskArtifact>,
}

/// The lifecycle marks and notes that formulae and casks share in `brew
/// info --json=v2` (`formula.rb:3137-3152`, `cask/cask.rb:579-596` in
/// Homebrew 7.0.7): `deprecated` / `disabled`, each with its date, reason
/// and suggested replacement, and `caveats`. They become
/// `ArtifactFacts.homebrew` (`homebrew_facts`).
///
/// Every one is read as a bare JSON value and kept only when it has the
/// expected shape. They only ever add a sentence to the details pane, so a
/// Homebrew that one day writes a reason as an object or a date as a
/// number must cost that sentence, not the whole Homebrew inventory, which
/// a strict `Option<String>` would fail with a parse error.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StatusFields {
    deprecated: Option<Value>,
    deprecation_date: Option<Value>,
    deprecation_reason: Option<Value>,
    deprecation_replacement_formula: Option<Value>,
    deprecation_replacement_cask: Option<Value>,
    disabled: Option<Value>,
    disable_date: Option<Value>,
    disable_reason: Option<Value>,
    disable_replacement_formula: Option<Value>,
    disable_replacement_cask: Option<Value>,
    caveats: Option<Value>,
}

/// A non-empty string, trimmed at the end only (a caveat's indented lines
/// keep their indent); anything else -- `null`, `""`, a number -- is
/// nothing.
fn text(value: &Option<Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim_end().to_string()),
        _ => None,
    }
}

fn flag(value: &Option<Value>) -> bool {
    matches!(value, Some(Value::Bool(true)))
}

/// One lifecycle mark, present only when its flag is `true`: Homebrew
/// fills `deprecation_date` for a `deprecate!` dated in the future too,
/// while the package is not deprecated yet (`formula.rb:5213-5225`, 7.0.7), so the
/// date alone says nothing. The replacement is the formula's name, else
/// the cask's: Homebrew 7 accepts only one of the two
/// (`formula.rb:5197-5199`), and an old `replacement:` fills both with the
/// same name.
fn lifecycle(
    on: &Option<Value>,
    date: &Option<Value>,
    reason: &Option<Value>,
    formula: &Option<Value>,
    cask: &Option<Value>,
) -> Option<HomebrewLifecycle> {
    flag(on).then(|| HomebrewLifecycle {
        date: text(date),
        reason: text(reason),
        replacement: text(formula).or_else(|| text(cask)),
    })
}

/// `ArtifactFacts.homebrew` for one package: `None` when Homebrew has
/// nothing of the kind to say, so the wire stays as small as before for
/// the many that have none.
///
/// Known limitation: the inventory (`brew info --installed`) is read before
/// a check runs `brew update`, so `deprecated` / `disabled` are as of the
/// previous update -- one refresh old when Homebrew has just marked a
/// package, or just lifted a mark.
fn homebrew_facts(status: &StatusFields, other_versions: Vec<String>) -> Option<HomebrewFacts> {
    let facts = HomebrewFacts {
        deprecated: lifecycle(
            &status.deprecated,
            &status.deprecation_date,
            &status.deprecation_reason,
            &status.deprecation_replacement_formula,
            &status.deprecation_replacement_cask,
        ),
        disabled: lifecycle(
            &status.disabled,
            &status.disable_date,
            &status.disable_reason,
            &status.disable_replacement_formula,
            &status.disable_replacement_cask,
        ),
        caveats: text(&status.caveats),
        other_versions,
    };
    (facts != HomebrewFacts::default()).then_some(facts)
}

/// One entry of a cask's `artifacts` in `brew info --json=v2`: one stanza,
/// keyed by the stanza's name, plus the absolute `target` brew adds for a
/// stanza that moves or links something (`artifacts_list`,
/// `cask/cask.rb:709-732` in Homebrew 7.0.6). An `app` stanza's `target`
/// is the cask's `path`. A `binary` stanza's `target` is the link in
/// `<prefix>/bin` itself -- the command -- which the unknown-source scan
/// finds by reading that directory and `cask_commands` names for
/// `commands::judge`; no other stanza's entry is read.
#[derive(Debug, Deserialize)]
struct CaskArtifact {
    /// Present exactly when this entry is an `app` stanza. What it holds
    /// (the bundle's name in the download, an optional rename) decides
    /// nothing here; the moved-to path is `target`.
    #[serde(default)]
    app: Option<IgnoredAny>,
    /// Present exactly when this entry is a `binary` stanza: its arguments,
    /// the file it links (`"grok"`, relative to the cask's folder in
    /// `Caskroom`, or an absolute path into the app) and, when the command
    /// is renamed, `{"target": "agent"}` -- `grok-build` links one file
    /// as both `grok` and `agent`, in two stanzas. Read as any JSON value
    /// (`cask_commands`), so an argument of an unexpected shape costs that
    /// one command, not the inventory.
    #[serde(default)]
    binary: Option<serde_json::Value>,
    #[serde(default)]
    target: Option<String>,
}

/// The commands a cask's `binary` stanzas put in `<prefix>/bin`, for
/// `CommandInputs.provided`: each named after the absolute `target` brew
/// adds beside the stanza (the link: `/opt/homebrew/bin/agent`), with the
/// file the stanza links as where that link must lead when it is an
/// absolute path (`within`; `commands::judge` adds the cask's own folder in
/// `Caskroom` and its app). A stanza without an absolute `target` names no
/// command: there is no saying where its link is.
fn cask_commands(artifacts: &[CaskArtifact]) -> Vec<ProvidedCommand> {
    artifacts
        .iter()
        .filter_map(|artifact| {
            let args = artifact.binary.as_ref()?;
            let link = PathBuf::from(artifact.target.as_deref()?);
            if !link.is_absolute() {
                return None;
            }
            let name = link.file_name()?.to_str()?.to_string();
            let source = args
                .as_array()
                .and_then(|args| args.first())
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
                .filter(|source| source.is_absolute());
            Some(ProvidedCommand {
                name,
                path: link,
                within: source.into_iter().collect(),
            })
        })
        .collect()
}

/// Parses `brew info --installed --json=v2`. For each formula, picks the
/// `installed` entry whose `version` matches `linked_keg` (falling back to
/// the last entry in the chronological array if no match, e.g. an unlinked
/// keg-only formula). Homebrew's JSON records only a single
/// `installed_on_request` boolean per installed entry (there is no separate
/// "installed as dependency" field): `true` -> `Requested`, `false` ->
/// `Dependency`, and the field being *absent* -> `Unknown`, same as a
/// formula with no installed entry to pick from at all. Casks have no
/// install-reason field in brew's JSON, so they are always `Requested`.
///
/// Both top-level partitions must be present; see `InfoInstalledRoot`.
///
/// A pinned formula or cask gets `uninstall_blocked: Some(Pinned)`:
/// `brew uninstall` without `--force` refuses it (`UninstallBlocked`).
///
/// A cask's `path` is the `.app` its `app` stanza was moved to: the
/// absolute `target` brew writes beside that stanza's entry in
/// `artifacts` (`cask/cask.rb:724-728` in Homebrew 7.0.6, for every
/// stanza with a source and a target location; the recorded
/// `7.0.3/info-installed.json` has one on all four casks), resolved
/// against the configured `appdir`, so `--appdir` is honoured
/// (`cask/artifact/relocated.rb:33-42,69-71`). Its reader is the
/// unknown-source scan's rule 2 (`scan/mod.rs`, `Known`): a cask's
/// `binary` link in `<prefix>/bin` -- `code`, `docker` -- resolves into
/// that bundle, under none of the roots the scan gives Homebrew, and with
/// `path: None` it was listed as a program no source installed while the
/// cask sat under Homebrew on the Installed page. One `path` per
/// artifact, so it is the first `app` stanza's; a cask with none (a
/// `pkg`, a font), or whose entry carries no absolute `target`, keeps
/// `None`, and a command such a cask puts outside `Caskroom` stays on the
/// Unknown page. A formula's `path` stays `None`: its keg is under
/// `Cellar`, which the scan gives Homebrew outright.
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

        // Every other installed keg, in brew's order: an older version
        // Homebrew's cleanup has not removed yet (or a newer one, unlinked).
        let other_versions: Vec<String> = match picked {
            Some(entry) => f
                .installed
                .iter()
                .filter(|other| !std::ptr::eq(*other, entry))
                .map(|other| other.version.clone())
                .collect(),
            None => Vec::new(),
        };

        let (version, reason, installed_at) = match picked {
            Some(entry) => {
                let reason = match entry.installed_on_request {
                    Some(true) => InstallReason::Requested,
                    Some(false) => InstallReason::Dependency,
                    None => InstallReason::Unknown,
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
            uninstall_blocked: f.pinned.then_some(UninstallBlocked::Pinned),
            facts: ArtifactFacts {
                homebrew: homebrew_facts(&f.status, other_versions),
                command_inputs: CommandInputs {
                    keg_only: matches!(f.keg_only, Some(serde_json::Value::Bool(true))),
                    ..Default::default()
                },
                ..Default::default()
            },
        });
    }

    for c in root.casks {
        let key_name = c
            .full_token
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| c.token.clone());
        let display_name = c.name.into_iter().next().unwrap_or_else(|| c.token.clone());
        let path = c
            .artifacts
            .iter()
            .find(|artifact| artifact.app.is_some())
            .and_then(|artifact| artifact.target.as_deref())
            .map(PathBuf::from)
            .filter(|target| target.is_absolute());
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
            // 0 or less is no date, not 1970.
            installed_at: c
                .installed_time
                .as_ref()
                .and_then(Value::as_i64)
                .filter(|t| *t > 0),
            path,
            auto_updates: c.auto_updates.unwrap_or(false),
            uninstall_blocked: c.pinned.then_some(UninstallBlocked::Pinned),
            facts: ArtifactFacts {
                homebrew: homebrew_facts(&c.status, Vec::new()),
                command_inputs: CommandInputs {
                    provided: cask_commands(&c.artifacts),
                    ..Default::default()
                },
                ..Default::default()
            },
        });
    }

    Ok(out)
}

// Required for the same reason as `InfoInstalledRoot`'s, and the lie this
// one told was "everything is up to date".
#[derive(Debug, Deserialize)]
struct OutdatedRoot {
    formulae: Vec<OutdatedItem>,
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
    /// `brew pin`: Homebrew will refuse a named `brew upgrade` of this
    /// package (exit 1, "Not upgrading 1 pinned package"), yet still lists
    /// it here. Becomes `UpdateBlocked::Pinned` below. Defaulted rather
    /// than required: a missing key is a brew that did not say the package
    /// is pinned, not a malformed reply worth failing the whole source
    /// over, and the worst it costs is Homebrew's own honest refusal.
    ///
    /// `pinned_version` is deliberately not read. Homebrew sets it exactly
    /// when `pinned` is true (`formula_pin.rb:50-52`, `cask/cask.rb:350-352`
    /// in 7.0.6), so it carries no second state, and nothing on screen
    /// needs the number.
    #[serde(default)]
    pinned: bool,
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
            blocked: item.pinned.then_some(UpdateBlocked::Pinned),
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

    // Homebrew's lifecycle marks, inline because the recording has none.
    // The shapes are `brew info --json=v2`'s: a symbol reason is written as
    // its name, a date as `YYYY-MM-DD` (`formula.rb:3142-3152`,
    // `cask/cask.rb:586-596` in Homebrew 7.0.7).

    #[test]
    fn parse_info_installed_reads_a_disabled_cask() {
        let json = r#"{
            "formulae": [],
            "casks": [
                {
                    "token": "oldapp",
                    "name": ["Old App"],
                    "installed": "1.2.0",
                    "installed_time": 1786405354,
                    "deprecated": false,
                    "deprecation_date": null,
                    "deprecation_reason": null,
                    "deprecation_replacement_formula": null,
                    "deprecation_replacement_cask": null,
                    "disabled": true,
                    "disable_date": "2026-09-01",
                    "disable_reason": "fails_gatekeeper_check",
                    "disable_replacement_formula": null,
                    "disable_replacement_cask": null,
                    "caveats": null
                }
            ]
        }"#;
        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");
        assert_eq!(result[0].installed_at, Some(1786405354));
        assert_eq!(
            result[0].facts.homebrew,
            Some(HomebrewFacts {
                deprecated: None,
                disabled: Some(HomebrewLifecycle {
                    date: Some("2026-09-01".to_string()),
                    reason: Some("fails_gatekeeper_check".to_string()),
                    replacement: None,
                }),
                caveats: None,
                other_versions: Vec::new(),
            })
        );
    }

    #[test]
    fn parse_info_installed_reads_a_deprecated_formula_with_its_replacement_and_old_keg() {
        let json = r#"{
            "formulae": [
                {
                    "name": "oldtool",
                    "linked_keg": "2.0",
                    "installed": [
                        {"version": "1.9", "installed_on_request": true},
                        {"version": "2.0", "installed_on_request": true}
                    ],
                    "deprecated": true,
                    "deprecation_date": "2026-06-15",
                    "deprecation_reason": "the package is not compatible with Homebrew's installation parameters",
                    "deprecation_replacement_formula": "newtool",
                    "deprecation_replacement_cask": null,
                    "disabled": false,
                    "disable_date": null,
                    "disable_reason": null,
                    "caveats": "Run oldtool --init once.\n"
                }
            ],
            "casks": []
        }"#;
        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");
        assert_eq!(result[0].version, "2.0");
        assert_eq!(
            result[0].facts.homebrew,
            Some(HomebrewFacts {
                deprecated: Some(HomebrewLifecycle {
                    date: Some("2026-06-15".to_string()),
                    // A sentence, not a symbol: kept word for word.
                    reason: Some(
                        "the package is not compatible with Homebrew's installation parameters"
                            .to_string()
                    ),
                    replacement: Some("newtool".to_string()),
                }),
                disabled: None,
                caveats: Some("Run oldtool --init once.".to_string()),
                other_versions: vec!["1.9".to_string()],
            })
        );
    }

    #[test]
    fn parse_info_installed_says_nothing_of_a_date_without_its_flag_or_of_odd_shapes() {
        // `deprecate!` dated in the future fills the date while
        // `deprecated` is still false; a reason that is not a string, a
        // blank caveat and a non-numeric or non-positive install time are
        // nothing -- and none of them fails the inventory.
        let json = r#"{
            "formulae": [
                {
                    "name": "later",
                    "installed": [{"version": "1.0", "installed_on_request": true}],
                    "deprecated": false,
                    "deprecation_date": "2099-01-01",
                    "deprecation_reason": "unmaintained",
                    "disabled": "yes",
                    "caveats": "   \n"
                }
            ],
            "casks": [
                {
                    "token": "odd",
                    "installed": "1.0",
                    "installed_time": "yesterday",
                    "deprecated": true,
                    "deprecation_reason": {"symbol": "unmaintained"},
                    "deprecation_replacement_formula": "",
                    "deprecation_replacement_cask": "newer"
                },
                {"token": "epoch", "installed": "1.0", "installed_time": 0},
                {"token": "before", "installed": "1.0", "installed_time": -5}
            ]
        }"#;
        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");
        assert_eq!(result[0].facts.homebrew, None);
        assert_eq!(result[1].installed_at, None);
        // An install time of 0 or less is no date, not 1970.
        assert_eq!(result[2].installed_at, None);
        assert_eq!(result[3].installed_at, None);
        assert_eq!(
            result[1].facts.homebrew,
            Some(HomebrewFacts {
                deprecated: Some(HomebrewLifecycle {
                    date: None,
                    reason: None,
                    replacement: Some("newer".to_string()),
                }),
                ..Default::default()
            })
        );
    }

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

    // `pinned` is the one per-package refusal `brew outdated --json=v2`
    // reports, on formula and cask entries alike (Homebrew 7.0.6:
    // `cmd/outdated.rb:196-200`, `cask/cask.rb:472-478`). The recorded
    // fixture for it is `adapters/fixtures/brew/7.0.6/outdated-pinned.json`
    // (edited, see its README); these cover the branches directly.

    #[test]
    fn parse_outdated_marks_a_pinned_formula_and_a_pinned_cask_as_blocked() {
        let json = r#"{
            "formulae": [
                {
                    "name": "glib",
                    "installed_versions": ["2.88.3"],
                    "current_version": "2.90.0",
                    "pinned": true,
                    "pinned_version": "2.88.3"
                },
                {
                    "name": "cairo",
                    "installed_versions": ["1.18.4"],
                    "current_version": "1.18.6",
                    "pinned": false,
                    "pinned_version": null
                }
            ],
            "casks": [
                {
                    "name": "onyx",
                    "installed_versions": ["5.0.2"],
                    "current_version": "5.1.0",
                    "pinned": true,
                    "pinned_version": "5.0.2"
                }
            ]
        }"#;

        let result = parse_outdated(json, "brew:/opt/homebrew").expect("parse");

        let blocked: Vec<_> = result
            .iter()
            .map(|c| (c.key.name.as_str(), c.key.kind, c.blocked))
            .collect();
        assert_eq!(
            blocked,
            vec![
                ("glib", ArtifactKind::Formula, Some(UpdateBlocked::Pinned)),
                ("cairo", ArtifactKind::Formula, None),
                ("onyx", ArtifactKind::Cask, Some(UpdateBlocked::Pinned)),
            ]
        );
        // Pinned is not "could not check": Homebrew knows the newer
        // version exactly, and the row should still say what it is.
        assert!(result.iter().all(|c| c.checkable));
        assert_eq!(result[0].target, "2.90.0");
    }

    #[test]
    fn parse_outdated_treats_an_entry_without_pinned_as_not_pinned() {
        // Homebrew 7 always writes the key, but a missing one is not a
        // malformed reply worth failing the whole source over: it is a
        // brew that did not say the package is pinned.
        let json = r#"{
            "formulae": [
                {
                    "name": "jq",
                    "installed_versions": ["1.6"],
                    "current_version": "1.7"
                }
            ],
            "casks": []
        }"#;

        let result = parse_outdated(json, "brew:/opt/homebrew").expect("parse");

        assert_eq!(result[0].blocked, None);
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
    // `brew info --installed --json=v2` and `brew outdated --json=v2` always
    // emit *both* partitions, one of them empty when there is nothing in it
    // (verified against `brew info --json=v2 jq`, which answers
    // `"casks": []`). A reply missing one is therefore not a brew with
    // nothing to report — it is not the reply we asked for. Defaulting the
    // partition to an empty vec turned that into the two worst sentences
    // this app can say: "nothing installed" and "everything is up to date".
    // Failing the read instead puts the source into its own "Banager
    // couldn't check this" state, which the UI already explains.

    #[test]
    fn parse_info_installed_refuses_a_reply_with_no_partitions() {
        assert!(parse_info_installed("{}", "brew:/opt/homebrew").is_err());
    }

    #[test]
    fn parse_info_installed_refuses_a_reply_missing_a_partition() {
        let json = r#"{"formulae": []}"#;
        assert!(parse_info_installed(json, "brew:/opt/homebrew").is_err());
    }

    #[test]
    fn parse_info_installed_accepts_both_partitions_empty() {
        // The genuine "you have installed nothing" answer still reads as one.
        let json = r#"{"formulae": [], "casks": []}"#;
        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_outdated_refuses_a_reply_with_no_partitions() {
        assert!(parse_outdated("{}", "brew:/opt/homebrew").is_err());
    }

    #[test]
    fn parse_outdated_refuses_a_reply_missing_a_partition() {
        let json = r#"{"casks": []}"#;
        assert!(parse_outdated(json, "brew:/opt/homebrew").is_err());
    }

    #[test]
    fn parse_outdated_accepts_both_partitions_empty() {
        let json = r#"{"formulae": [], "casks": []}"#;
        let result = parse_outdated(json, "brew:/opt/homebrew").expect("parse");
        assert!(result.is_empty());
    }

    // `brew info --installed --json=v2` writes `pinned` for every formula
    // and every cask (Homebrew 7.0.6: `formula.rb:3140`, `cask/cask.rb:574`;
    // the recorded `7.0.3/info-installed.json` has it, `false`, on all 93
    // entries). A pinned package is one `brew uninstall` refuses without
    // `--force` (`uninstall.rb:48-49`, `cask/uninstall.rb:40-44`), and the
    // inventory is the only read that covers pinned packages that are up
    // to date.

    #[test]
    fn parse_info_installed_marks_a_pinned_formula_and_a_pinned_cask_as_uninstall_blocked() {
        let json = r#"{
            "formulae": [
                {
                    "name": "glib",
                    "linked_keg": "2.88.3",
                    "installed": [{ "version": "2.88.3", "installed_on_request": true }],
                    "pinned": true,
                    "outdated": false
                },
                {
                    "name": "cairo",
                    "linked_keg": "1.18.4",
                    "installed": [{ "version": "1.18.4", "installed_on_request": true }],
                    "pinned": false,
                    "outdated": false
                }
            ],
            "casks": [
                {
                    "token": "onyx",
                    "full_token": "onyx",
                    "name": ["OnyX"],
                    "installed": "5.0.2",
                    "pinned": true,
                    "pinned_version": "5.0.2",
                    "outdated": false
                }
            ]
        }"#;

        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");

        let blocked: Vec<_> = result
            .iter()
            .map(|a| (a.key.name.as_str(), a.key.kind, a.uninstall_blocked))
            .collect();
        assert_eq!(
            blocked,
            vec![
                (
                    "glib",
                    ArtifactKind::Formula,
                    Some(UninstallBlocked::Pinned)
                ),
                ("cairo", ArtifactKind::Formula, None),
                ("onyx", ArtifactKind::Cask, Some(UninstallBlocked::Pinned)),
            ]
        );
    }

    #[test]
    fn parse_info_installed_treats_an_entry_without_pinned_as_not_pinned() {
        let json = r#"{
            "formulae": [
                {
                    "name": "jq",
                    "linked_keg": "1.8.2",
                    "installed": [{ "version": "1.8.2", "installed_on_request": true }]
                }
            ],
            "casks": [
                { "token": "onyx", "installed": "5.0.2" }
            ]
        }"#;

        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");

        assert!(result.iter().all(|a| a.uninstall_blocked.is_none()));
    }

    #[test]
    fn a_formula_whose_entry_omits_installed_on_request_is_unknown_not_a_dependency() {
        // `installed_on_request: false` is brew saying "something else pulled
        // this in"; the field being absent is brew saying nothing at all.
        // Reading the second as the first hid the package behind the
        // Installed page's collapsed "N components installed by other
        // software", which is where a user goes looking for things they did
        // not install.
        let json = r#"{
            "formulae": [
                {
                    "name": "jq",
                    "linked_keg": "1.8.2",
                    "installed": [{ "version": "1.8.2" }]
                }
            ],
            "casks": []
        }"#;

        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].reason, InstallReason::Unknown);
    }

    #[test]
    fn installed_on_request_still_separates_requested_from_dependency() {
        let json = r#"{
            "formulae": [
                {
                    "name": "jq",
                    "linked_keg": "1.8.2",
                    "installed": [{ "version": "1.8.2", "installed_on_request": true }]
                },
                {
                    "name": "oniguruma",
                    "linked_keg": "6.9.10",
                    "installed": [{ "version": "6.9.10", "installed_on_request": false }]
                }
            ],
            "casks": []
        }"#;

        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");

        assert_eq!(result[0].reason, InstallReason::Requested);
        assert_eq!(result[1].reason, InstallReason::Dependency);
    }

    // `brew info --installed --json=v2` writes a cask's stanzas under
    // `artifacts`, one object per stanza, and beside an `app` stanza the
    // absolute path the bundle was moved to (`target`; the recorded
    // `7.0.3/info-installed.json` has one on all four casks). That path is
    // the cask's `InstalledArtifact.path`. Its reader is the unknown-source
    // scan's rule 2 (`scan/mod.rs`, `Known`): a cask's `binary` link in
    // `<prefix>/bin` resolves into the app, under none of the roots the
    // scan gives Homebrew, and with `path: None` it was listed as a
    // program no source installed.

    #[test]
    fn parse_info_installed_gives_a_cask_the_app_its_app_stanza_was_moved_to() {
        let json = r#"{
            "formulae": [],
            "casks": [
                {
                    "token": "visual-studio-code",
                    "full_token": "visual-studio-code",
                    "name": ["Microsoft Visual Studio Code", "VS Code"],
                    "installed": "1.104.0",
                    "artifacts": [
                        { "uninstall": [{ "quit": "com.microsoft.VSCode" }] },
                        { "app": ["Visual Studio Code.app"], "target": "/Applications/Visual Studio Code.app" },
                        {
                            "binary": ["/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code", { "target": "code" }],
                            "target": "/usr/local/bin/code"
                        },
                        { "zap": [{ "trash": ["~/.vscode"] }] }
                    ]
                }
            ]
        }"#;

        let result = parse_info_installed(json, "brew:/usr/local").expect("parse");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key.kind, ArtifactKind::Cask);
        assert_eq!(
            result[0].path,
            Some(PathBuf::from("/Applications/Visual Studio Code.app"))
        );
    }

    #[test]
    fn parse_info_installed_leaves_a_casks_path_none_without_an_absolute_app_target() {
        // A `pkg` cask installs wherever its package says and has no `app`
        // stanza. The other three are not shapes the recorded 7.0.3 output
        // has; they must not become a relative path for the scan to
        // `canonicalize` against the working directory.
        let json = r#"{
            "formulae": [],
            "casks": [
                {
                    "token": "some-installer",
                    "installed": "2.0",
                    "artifacts": [
                        { "pkg": ["SomeInstaller.pkg"] },
                        { "uninstall": [{ "pkgutil": "com.example.some-installer" }] }
                    ]
                },
                {
                    "token": "no-target",
                    "installed": "1.0",
                    "artifacts": [{ "app": ["NoTarget.app"] }]
                },
                {
                    "token": "relative-target",
                    "installed": "1.0",
                    "artifacts": [{ "app": ["Relative.app"], "target": "Relative.app" }]
                },
                { "token": "no-artifacts", "installed": "1.0" }
            ]
        }"#;

        let result = parse_info_installed(json, "brew:/usr/local").expect("parse");

        assert_eq!(result.len(), 4);
        assert!(result.iter().all(|a| a.path.is_none()), "{result:?}");
    }

    #[test]
    fn parse_info_installed_names_both_commands_grok_build_links_from_one_file() {
        // The `grok-build` cask's shape (formulae.brew.sh's cask API, and
        // the synthesis' S §3c): two `binary` stanzas for one file, the
        // second renamed. Each gets the absolute link brew writes beside
        // it; the file is relative to the cask's folder, so it says
        // nothing about where the link must lead (`commands::judge` adds
        // that folder). A stanza with no absolute `target` names nothing;
        // one whose arguments are not the expected shape is still named by
        // its link, and fails nothing else.
        let json = r#"{
            "formulae": [],
            "casks": [
                {
                    "token": "grok-build",
                    "full_token": "grok-build",
                    "name": ["Grok Build"],
                    "installed": "1.0.46",
                    "artifacts": [
                        { "binary": ["grok"], "target": "/opt/homebrew/bin/grok" },
                        { "binary": ["grok", { "target": "agent" }], "target": "/opt/homebrew/bin/agent" },
                        { "binary": ["grok-helper"] },
                        { "binary": ["relative"], "target": "bin/relative" },
                        { "binary": { "unexpected": true }, "target": "/opt/homebrew/bin/odd" }
                    ]
                }
            ]
        }"#;

        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");

        assert_eq!(result.len(), 1);
        let provided = &result[0].facts.command_inputs.provided;
        assert_eq!(
            provided,
            &vec![
                ProvidedCommand {
                    name: "grok".to_string(),
                    path: PathBuf::from("/opt/homebrew/bin/grok"),
                    within: Vec::new(),
                },
                ProvidedCommand {
                    name: "agent".to_string(),
                    path: PathBuf::from("/opt/homebrew/bin/agent"),
                    within: Vec::new(),
                },
                ProvidedCommand {
                    name: "odd".to_string(),
                    path: PathBuf::from("/opt/homebrew/bin/odd"),
                    within: Vec::new(),
                },
            ]
        );
        // What the window gets is the verdicts, which nothing has made yet.
        assert!(result[0].facts.commands.is_empty());
    }

    #[test]
    fn parse_info_installed_keeps_an_absolute_binary_source_as_where_its_link_must_lead() {
        // The recorded 7.0.3 `codexbar` cask: its command links into the
        // app, by absolute path.
        let json =
            std::fs::read_to_string("../../adapters/fixtures/brew/7.0.3/info-installed.json")
                .expect("read the recorded brew info");
        let result = parse_info_installed(&json, "brew:/opt/homebrew").expect("parse");
        let codexbar = result
            .iter()
            .find(|a| a.key.kind == ArtifactKind::Cask && a.key.name == "codexbar")
            .expect("codexbar is in the recording");
        assert_eq!(
            codexbar.facts.command_inputs.provided,
            vec![ProvidedCommand {
                name: "codexbar".to_string(),
                path: PathBuf::from("/opt/homebrew/bin/codexbar"),
                within: vec![PathBuf::from(
                    "/Applications/CodexBar.app/Contents/Helpers/CodexBarCLI"
                )],
            }]
        );
        // The other three casks of the recording link no command.
        let others: Vec<_> = result
            .iter()
            .filter(|a| a.key.kind == ArtifactKind::Cask && a.key.name != "codexbar")
            .collect();
        assert_eq!(others.len(), 3);
        assert!(others
            .iter()
            .all(|a| a.facts.command_inputs.provided.is_empty()));
        // A formula's commands are found from its links, not from here.
        assert!(result
            .iter()
            .filter(|a| a.key.kind == ArtifactKind::Formula)
            .all(|a| a.facts.command_inputs.provided.is_empty()));
    }

    #[test]
    fn parse_info_installed_marks_the_keg_only_formulae() {
        // The recording has four: `node@22`, linked by hand, among them --
        // keg-only is what Homebrew says of the formula, whatever was
        // linked since.
        let json =
            std::fs::read_to_string("../../adapters/fixtures/brew/7.0.3/info-installed.json")
                .expect("read the recorded brew info");
        let result = parse_info_installed(&json, "brew:/opt/homebrew").expect("parse");
        let keg_only: Vec<&str> = result
            .iter()
            .filter(|a| a.facts.command_inputs.keg_only)
            .map(|a| a.key.name.as_str())
            .collect();
        assert_eq!(keg_only, vec!["icu4c@78", "node@22", "readline", "sqlite"]);
        // Anything but `true` is not keg-only, and never fails the reply.
        let json = r#"{
            "formulae": [
                { "name": "a", "keg_only": "yes", "installed": [] },
                { "name": "b", "keg_only": null, "installed": [] },
                { "name": "c", "installed": [] },
                { "name": "d", "keg_only": true, "installed": [] }
            ],
            "casks": []
        }"#;
        let result = parse_info_installed(json, "brew:/opt/homebrew").expect("parse");
        let flags: Vec<bool> = result
            .iter()
            .map(|a| a.facts.command_inputs.keg_only)
            .collect();
        assert_eq!(flags, vec![false, false, false, true]);
    }
}
