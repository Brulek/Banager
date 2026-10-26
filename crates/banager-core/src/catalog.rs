//! The Discover page's catalog (phase 5 step A,
//! `docs/superpowers/plans/2026-10-26-phase-5-step-a-discover.md`): the
//! tools Banager suggests, what installs each, how each is recognised as
//! already on the Mac, and -- from a snapshot -- which state each is in.
//!
//! The catalog is data compiled into the app (Q5 of the phase 5
//! proposal): nothing is downloaded for it and nothing is run to list it.
//! It names no command (spec §6): what installs an entry is a source and a
//! package name, which the source's adapter turns into its own command
//! (`install_request`, step B). The words each entry is shown with --
//! what it is, who needs it, how it is used -- are the window's, in its
//! three locale files, keyed by `CatalogItem::id`.

use crate::model::{ArtifactKind, InstalledArtifact, ManagerInstance, OpKind, OpRequest};
use crate::session::Snapshot;
use serde::Serialize;

/// Where the Discover page groups an entry, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CatalogCategory {
    /// Homebrew: what every other entry is installed and updated with.
    Basics,
    /// AI coding assistants used in Terminal.
    AiAssistants,
    /// Running models on the Mac.
    LocalModels,
    /// Everyday command-line tools.
    CommonTools,
}

/// What installs an entry: never a command (spec §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CatalogInstall {
    /// Homebrew itself, which Banager does not install: the user runs its
    /// official command in Terminal (Q1 (a), step C).
    HomebrewItself,
    /// A Homebrew formula or cask, by name.
    Homebrew {
        kind: ArtifactKind,
        name: &'static str,
    },
}

/// One way of already having an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recognizer {
    /// Any copy of an AI tool family, by `ArtifactFacts::family`
    /// (`data/ai-tools.json`): Claude Code's npm package, its two casks and
    /// its own installer alike.
    Family(&'static str),
    /// A source of this adapter is on the Mac, answering or not: npm's runs
    /// on a Node, a uv source is uv, an Ollama source is Ollama.
    Source(&'static str),
    /// A Homebrew formula by name, or a versioned one of it (`node@22`).
    Formula(&'static str),
}

/// One entry of the catalog.
#[derive(Clone, Copy, Debug)]
pub struct CatalogItem {
    /// The entry's id: what the window keys its words and its logo by,
    /// and what installing will name it by (`plan_install`, step B).
    pub id: &'static str,
    pub category: CatalogCategory,
    pub install: CatalogInstall,
    /// Every way of already having it the catalog knows of.
    recognized_by: &'static [Recognizer],
    /// The oldest macOS major version it installs on, where Homebrew's
    /// record says (`depends_on macos:`).
    pub needs_macos: Option<u32>,
    /// Whether Discover installs it from here (`install_request`, step B):
    /// every Homebrew formula and cask but Ollama's, which step D installs
    /// with a model to start with. Homebrew itself is installed in
    /// Terminal (step C).
    pub installs_here: bool,
}

/// The catalog, in the order the Discover page lists it.
pub const CATALOG: [CatalogItem; 8] = [
    CatalogItem {
        id: "homebrew",
        category: CatalogCategory::Basics,
        install: CatalogInstall::HomebrewItself,
        recognized_by: &[Recognizer::Source("brew")],
        needs_macos: None,
        installs_here: false,
    },
    CatalogItem {
        id: "claude-code",
        category: CatalogCategory::AiAssistants,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Cask,
            name: "claude-code",
        },
        recognized_by: &[Recognizer::Family("claude-code")],
        needs_macos: None,
        installs_here: true,
    },
    CatalogItem {
        id: "codex",
        category: CatalogCategory::AiAssistants,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Cask,
            name: "codex",
        },
        recognized_by: &[Recognizer::Family("codex")],
        needs_macos: None,
        installs_here: true,
    },
    CatalogItem {
        id: "gemini-cli",
        category: CatalogCategory::AiAssistants,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Formula,
            name: "gemini-cli",
        },
        recognized_by: &[Recognizer::Family("gemini-cli")],
        needs_macos: None,
        installs_here: true,
    },
    CatalogItem {
        id: "ollama",
        category: CatalogCategory::LocalModels,
        // The app, which runs the models' server itself; it needs macOS 14
        // (the cask's `depends_on macos: ">= :sonoma"`).
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Cask,
            name: "ollama-app",
        },
        recognized_by: &[Recognizer::Family("ollama"), Recognizer::Source("ollama")],
        needs_macos: Some(14),
        installs_here: false,
    },
    CatalogItem {
        id: "node",
        category: CatalogCategory::CommonTools,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Formula,
            name: "node",
        },
        recognized_by: &[Recognizer::Formula("node"), Recognizer::Source("npm")],
        needs_macos: None,
        installs_here: true,
    },
    CatalogItem {
        id: "uv",
        category: CatalogCategory::CommonTools,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Formula,
            name: "uv",
        },
        recognized_by: &[Recognizer::Formula("uv"), Recognizer::Source("uv")],
        needs_macos: None,
        installs_here: true,
    },
    CatalogItem {
        id: "ffmpeg",
        category: CatalogCategory::CommonTools,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Formula,
            name: "ffmpeg",
        },
        recognized_by: &[Recognizer::Formula("ffmpeg")],
        needs_macos: None,
        installs_here: true,
    },
];

/// What the Discover page says of an entry on this Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CatalogState {
    /// One of the ways of having it is in the snapshot.
    Installed,
    /// Not installed, and a Homebrew that can install it is there.
    NotInstalled,
    /// Not installed, and no Homebrew that can install it is there: none,
    /// or none that answered and can be written to.
    NeedsHomebrew,
    /// Not installed, and this Mac's macOS is older than it needs.
    NeedsMacOS { major: u32 },
    /// Homebrew itself, on a Mac that is not Apple silicon and has none:
    /// Homebrew's official installer no longer installs on Intel Macs
    /// (phase 5 proposal §1.1).
    HomebrewUnsupported,
}

/// One entry as the window lists it (`get_catalog` in
/// src-tauri/src/ipc.rs; `CatalogEntry` in src/lib/types.ts).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub category: CatalogCategory,
    pub install: CatalogInstall,
    pub state: CatalogState,
    /// `CatalogItem::installs_here`: whether its row offers Install.
    pub installs_here: bool,
}

/// What this Mac is, for the states that depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host {
    /// macOS's major version, `None` where it could not be read.
    pub macos_major: Option<u32>,
    /// Whether the Mac is Apple silicon, whatever this process runs as.
    pub apple_silicon: bool,
}

impl Host {
    /// This Mac, from the kernel: `kern.osproductversion` and
    /// `hw.optional.arm64`, which is 1 on Apple silicon under Rosetta too
    /// (`sysctlbyname`). Runs nothing and opens nothing.
    pub fn of_this_mac() -> Host {
        let version = crate::diagnostics::read_os().macos_version;
        Host {
            macos_major: version.as_deref().and_then(major_of),
            apple_silicon: apple_silicon(),
        }
    }
}

/// The major version of a macOS version string: 14 of `14.5`.
fn major_of(version: &str) -> Option<u32> {
    version.split('.').next()?.trim().parse().ok()
}

#[cfg(target_os = "macos")]
fn apple_silicon() -> bool {
    let mut value: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    // SAFETY: `value` is a c_int and `len` says so; the kernel writes at
    // most `len` bytes into it. The name is a NUL-terminated literal.
    let read = unsafe {
        libc::sysctlbyname(
            c"hw.optional.arm64".as_ptr(),
            (&mut value as *mut libc::c_int).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    read == 0 && value == 1
}

#[cfg(not(target_os = "macos"))]
fn apple_silicon() -> bool {
    false
}

/// Every entry of the catalog, in order, with its state on the Mac
/// `snapshot` and `host` describe.
pub fn entries(snapshot: &Snapshot, host: &Host) -> Vec<CatalogEntry> {
    let can_install = snapshot.instances.iter().any(homebrew_can_install);
    CATALOG
        .iter()
        .map(|item| CatalogEntry {
            id: item.id,
            category: item.category,
            install: item.install,
            state: state_of(item, snapshot, host, can_install),
            installs_here: item.installs_here,
        })
        .collect()
}

/// Why `install_request` names nothing to install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallRefusal {
    /// No entry has this id, or Discover does not install it from here
    /// (`CatalogItem::installs_here`): no row offers its Install.
    NotOffered,
    /// The entry is not `NotInstalled` on this Mac now: installed, or one
    /// this Mac cannot install, as its row says.
    State(CatalogState),
}

/// The install of the catalog entry `id` on the Mac `snapshot` and `host`
/// describe, for `Session::issue_plan` (`plan_install` in
/// src-tauri/src/ipc.rs): the entry's own Homebrew formula or cask, under
/// the first Homebrew of the snapshot that answered and can be written to
/// (`homebrew_can_install`), in the order detection found them
/// (`/opt/homebrew` before `/usr/local`). The window names an entry, never
/// a package (phase 5 proposal §3.4), so what is installed is always one of
/// the catalog's names. Only an entry offered from here and "Not
/// installed" now; anything else is refused before any adapter is asked.
pub fn install_request(
    snapshot: &Snapshot,
    host: &Host,
    id: &str,
) -> Result<OpRequest, InstallRefusal> {
    let item = CATALOG
        .iter()
        .find(|item| item.id == id && item.installs_here)
        .ok_or(InstallRefusal::NotOffered)?;
    let CatalogInstall::Homebrew { kind, name } = item.install else {
        return Err(InstallRefusal::NotOffered);
    };
    let brew = snapshot.instances.iter().find(|i| homebrew_can_install(i));
    let state = state_of(item, snapshot, host, brew.is_some());
    match (state, brew) {
        (CatalogState::NotInstalled, Some(brew)) => Ok(OpRequest {
            kind: OpKind::Install,
            instance_id: brew.id.clone(),
            artifact_kind: kind,
            name: name.to_string(),
        }),
        (state, _) => Err(InstallRefusal::State(state)),
    }
}

fn state_of(
    item: &CatalogItem,
    snapshot: &Snapshot,
    host: &Host,
    can_install: bool,
) -> CatalogState {
    let installed = item
        .recognized_by
        .iter()
        .any(|way| has(way, &snapshot.instances, &snapshot.artifacts));
    if installed {
        return CatalogState::Installed;
    }
    match item.install {
        CatalogInstall::HomebrewItself if host.apple_silicon => CatalogState::NotInstalled,
        CatalogInstall::HomebrewItself => CatalogState::HomebrewUnsupported,
        CatalogInstall::Homebrew { .. } => match (item.needs_macos, host.macos_major) {
            (Some(major), Some(mac)) if mac < major => CatalogState::NeedsMacOS { major },
            _ if can_install => CatalogState::NotInstalled,
            _ => CatalogState::NeedsHomebrew,
        },
    }
}

/// Whether `instance` is a Homebrew that can install: it answered, and is
/// not view only (a prefix this user cannot write, Homebrew run as root).
fn homebrew_can_install(instance: &ManagerInstance) -> bool {
    instance.adapter_id == "brew"
        && instance.status.unavailable.is_none()
        && instance.read_only_reason.is_none()
}

fn has(way: &Recognizer, instances: &[ManagerInstance], artifacts: &[InstalledArtifact]) -> bool {
    match *way {
        Recognizer::Family(family) => artifacts
            .iter()
            .any(|a| a.facts.family.as_deref() == Some(family)),
        Recognizer::Source(adapter) => instances.iter().any(|i| i.adapter_id == adapter),
        Recognizer::Formula(name) => artifacts.iter().any(|a| {
            a.key.kind == ArtifactKind::Formula
                && a.key.instance_id.starts_with("brew:")
                && (a.key.name == name
                    || a.key.name.strip_prefix(name).is_some_and(|rest| {
                        rest.strip_prefix('@').is_some_and(|v| {
                            !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')
                        })
                    }))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtifactFacts, ArtifactKey, InstallReason, InstanceStatus, Unavailable};
    use crate::session::DetectOutcome;

    fn instance(adapter: &str, id: &str) -> ManagerInstance {
        crate::testing::manager_instance(adapter, id)
    }

    fn artifact(
        instance_id: &str,
        kind: ArtifactKind,
        name: &str,
        family: Option<&str>,
    ) -> InstalledArtifact {
        InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: "1.0".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
            facts: ArtifactFacts {
                family: family.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn snapshot(instances: Vec<ManagerInstance>, artifacts: Vec<InstalledArtifact>) -> Snapshot {
        Snapshot {
            detect: DetectOutcome::Found,
            instances,
            artifacts,
            ..Snapshot::empty()
        }
    }

    const SILICON_15: Host = Host {
        macos_major: Some(15),
        apple_silicon: true,
    };

    fn states(snapshot: &Snapshot, host: &Host) -> Vec<(&'static str, CatalogState)> {
        entries(snapshot, host)
            .into_iter()
            .map(|e| (e.id, e.state))
            .collect()
    }

    #[test]
    fn test_the_catalog_names_each_entry_once_and_only_names_the_sources_can_take() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|item| item.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for item in CATALOG {
            if let CatalogInstall::Homebrew { kind, name } = item.install {
                assert!(
                    matches!(kind, ArtifactKind::Formula | ArtifactKind::Cask),
                    "{}",
                    item.id
                );
                crate::adapters::validate_package_name(name).expect(item.id);
            }
            assert!(!item.recognized_by.is_empty(), "{}", item.id);
        }
        // Every family named is one `data/ai-tools.json` has, and every
        // Homebrew name installed for one is one of that family's.
        let families = crate::families::families();
        for item in CATALOG {
            for way in item.recognized_by {
                if let Recognizer::Family(id) = way {
                    let family = families
                        .iter()
                        .find(|f| f.id == *id)
                        .unwrap_or_else(|| panic!("no family {id}"));
                    if let CatalogInstall::Homebrew { name, .. } = item.install {
                        assert!(
                            family.members.iter().any(|m| m.name == name),
                            "{}: {name} is not a member of {id}",
                            item.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_an_empty_mac_needs_homebrew_for_everything_but_homebrew() {
        let empty = snapshot(Vec::new(), Vec::new());
        assert_eq!(
            states(&empty, &SILICON_15),
            [
                ("homebrew", CatalogState::NotInstalled),
                ("claude-code", CatalogState::NeedsHomebrew),
                ("codex", CatalogState::NeedsHomebrew),
                ("gemini-cli", CatalogState::NeedsHomebrew),
                ("ollama", CatalogState::NeedsHomebrew),
                ("node", CatalogState::NeedsHomebrew),
                ("uv", CatalogState::NeedsHomebrew),
                ("ffmpeg", CatalogState::NeedsHomebrew),
            ]
        );
        // On an Intel Mac Homebrew's installer refuses; on macOS 13 Ollama
        // needs 14 whatever else.
        let intel_13 = Host {
            macos_major: Some(13),
            apple_silicon: false,
        };
        let on_intel = states(&empty, &intel_13);
        assert_eq!(on_intel[0], ("homebrew", CatalogState::HomebrewUnsupported));
        assert_eq!(
            on_intel[4],
            ("ollama", CatalogState::NeedsMacOS { major: 14 })
        );
    }

    #[test]
    fn test_each_known_way_of_having_an_entry_counts_as_installed() {
        let brew = instance("brew", "brew:/opt/homebrew");
        let b = brew.id.clone();
        let mac = snapshot(
            vec![
                brew,
                instance("npm", "npm:/usr/local"),
                instance("uv", "uv"),
            ],
            vec![
                // Claude Code from npm, Codex by its own installer: their
                // family says what they are.
                artifact(
                    "npm:/usr/local",
                    ArtifactKind::Package,
                    "@anthropic-ai/claude-code",
                    Some("claude-code"),
                ),
                artifact(
                    "standalone-codex",
                    ArtifactKind::Tool,
                    "codex",
                    Some("codex"),
                ),
                artifact(&b, ArtifactKind::Formula, "ffmpeg@7", None),
            ],
        );
        assert_eq!(
            states(&mac, &SILICON_15),
            [
                ("homebrew", CatalogState::Installed),
                ("claude-code", CatalogState::Installed),
                ("codex", CatalogState::Installed),
                ("gemini-cli", CatalogState::NotInstalled),
                ("ollama", CatalogState::NotInstalled),
                // npm runs on a Node; a uv source is uv.
                ("node", CatalogState::Installed),
                ("uv", CatalogState::Installed),
                ("ffmpeg", CatalogState::Installed),
            ]
        );
        // An Ollama source alone -- the app from ollama.com -- is Ollama.
        let ollama = snapshot(
            vec![instance("ollama", "ollama:http://127.0.0.1:11434")],
            Vec::new(),
        );
        assert_eq!(
            states(&ollama, &SILICON_15)[4],
            ("ollama", CatalogState::Installed)
        );
    }

    #[test]
    fn test_a_versioned_formula_counts_but_a_formula_with_a_longer_name_does_not() {
        let b = "brew:/opt/homebrew";
        for (name, installed) in [
            ("node", true),
            ("node@22", true),
            ("node@20.1", true),
            ("node@", false),
            ("nodenv", false),
            ("node-build", false),
        ] {
            let mac = snapshot(
                vec![instance("brew", b)],
                vec![artifact(b, ArtifactKind::Formula, name, None)],
            );
            let node = states(&mac, &SILICON_15)[5];
            assert_eq!(node.1 == CatalogState::Installed, installed, "{name}");
        }
        // Not a formula of Homebrew's: a cask named so, or another source's.
        let mac = snapshot(
            vec![
                instance("brew", b),
                instance("cargo", "cargo:/Users/you/.cargo"),
            ],
            vec![
                artifact(b, ArtifactKind::Cask, "ffmpeg", None),
                artifact("cargo:/Users/you/.cargo", ArtifactKind::Binary, "uv", None),
            ],
        );
        let states = states(&mac, &SILICON_15);
        assert_eq!(states[6], ("uv", CatalogState::NotInstalled));
        assert_eq!(states[7], ("ffmpeg", CatalogState::NotInstalled));
    }

    #[test]
    fn test_only_a_homebrew_that_answered_and_can_be_written_to_can_install() {
        let answering = instance("brew", "brew:/opt/homebrew");
        let stopped = ManagerInstance {
            status: InstanceStatus {
                unavailable: Some(Unavailable::NotResponding),
                notes: Vec::new(),
                no_answer: None,
            },
            ..instance("brew", "brew:/usr/local")
        };
        let view_only = ManagerInstance {
            read_only_reason: Some(crate::model::ReadOnlyReason::PrefixNotWritable),
            ..instance("brew", "brew:/opt/homebrew")
        };
        for (brews, state) in [
            (vec![answering.clone()], CatalogState::NotInstalled),
            (vec![stopped.clone()], CatalogState::NeedsHomebrew),
            (vec![view_only], CatalogState::NeedsHomebrew),
            (vec![stopped, answering], CatalogState::NotInstalled),
        ] {
            let mac = snapshot(brews, Vec::new());
            let states = states(&mac, &SILICON_15);
            // Homebrew is there either way.
            assert_eq!(states[0], ("homebrew", CatalogState::Installed));
            assert_eq!(states[3], ("gemini-cli", state));
        }
    }

    #[test]
    fn test_reads_the_major_version() {
        assert_eq!(major_of("14.5"), Some(14));
        assert_eq!(major_of("27.0"), Some(27));
        assert_eq!(major_of("15"), Some(15));
        assert_eq!(major_of(""), None);
        assert_eq!(major_of("x.1"), None);
    }

    #[test]
    fn test_the_window_reads_each_field_by_these_names() {
        let empty = snapshot(Vec::new(), Vec::new());
        let json = serde_json::to_value(entries(&empty, &SILICON_15)).unwrap();
        assert_eq!(
            json[0],
            serde_json::json!({
                "id": "homebrew",
                "category": "Basics",
                "install": "HomebrewItself",
                "state": "NotInstalled",
                "installs_here": false
            })
        );
        assert_eq!(
            json[4],
            serde_json::json!({
                "id": "ollama",
                "category": "LocalModels",
                "install": {"Homebrew": {"kind": "Cask", "name": "ollama-app"}},
                "state": "NeedsHomebrew",
                "installs_here": false
            })
        );
        let old = Host {
            macos_major: Some(13),
            apple_silicon: true,
        };
        assert_eq!(
            serde_json::to_value(entries(&empty, &old)).unwrap()[4]["state"],
            serde_json::json!({"NeedsMacOS": {"major": 14}})
        );
        assert_eq!(
            serde_json::to_value(entries(&empty, &old)).unwrap()[3]["installs_here"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn test_six_entries_install_from_here_each_a_homebrew_formula_or_cask() {
        let here: Vec<&str> = CATALOG
            .iter()
            .filter(|item| item.installs_here)
            .map(|item| item.id)
            .collect();
        assert_eq!(
            here,
            ["claude-code", "codex", "gemini-cli", "node", "uv", "ffmpeg"]
        );
        for item in CATALOG.iter().filter(|item| item.installs_here) {
            assert!(
                matches!(item.install, CatalogInstall::Homebrew { .. }),
                "{}",
                item.id
            );
        }
    }

    #[test]
    fn test_the_install_of_an_entry_is_its_own_name_under_the_first_homebrew_that_can_install() {
        let stopped = ManagerInstance {
            status: InstanceStatus {
                unavailable: Some(Unavailable::NotResponding),
                notes: Vec::new(),
                no_answer: None,
            },
            ..instance("brew", "brew:/opt/homebrew")
        };
        let mac = snapshot(
            vec![
                stopped,
                instance("brew", "brew:/usr/local"),
                instance("brew", "brew:/Users/you/homebrew"),
            ],
            Vec::new(),
        );
        assert_eq!(
            install_request(&mac, &SILICON_15, "gemini-cli"),
            Ok(OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/usr/local".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "gemini-cli".to_string(),
            })
        );
        assert_eq!(
            install_request(&mac, &SILICON_15, "claude-code"),
            Ok(OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/usr/local".to_string(),
                artifact_kind: ArtifactKind::Cask,
                name: "claude-code".to_string(),
            })
        );
    }

    #[test]
    fn test_only_an_entry_offered_here_and_not_installed_is_installed() {
        let b = "brew:/opt/homebrew";
        let mac = snapshot(
            vec![instance("brew", b)],
            vec![artifact(b, ArtifactKind::Formula, "uv", None)],
        );
        assert_eq!(
            install_request(&mac, &SILICON_15, "uv"),
            Err(InstallRefusal::State(CatalogState::Installed))
        );
        // Not offered from here: Homebrew, Ollama, and what is no entry --
        // a package name included.
        for id in ["homebrew", "ollama", "jq", "", "ollama-app", "Codex"] {
            assert_eq!(
                install_request(&mac, &SILICON_15, id),
                Err(InstallRefusal::NotOffered),
                "{id}"
            );
        }
        // No Homebrew that can install.
        let empty = snapshot(Vec::new(), Vec::new());
        assert_eq!(
            install_request(&empty, &SILICON_15, "node"),
            Err(InstallRefusal::State(CatalogState::NeedsHomebrew))
        );
        let view_only = ManagerInstance {
            read_only_reason: Some(crate::model::ReadOnlyReason::PrefixNotWritable),
            ..instance("brew", b)
        };
        assert_eq!(
            install_request(&snapshot(vec![view_only], Vec::new()), &SILICON_15, "node"),
            Err(InstallRefusal::State(CatalogState::NeedsHomebrew))
        );
    }
}
