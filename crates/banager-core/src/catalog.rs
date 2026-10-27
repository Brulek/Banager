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

use crate::model::{
    ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
};
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
    /// A model Ollama pulls from its library, by name, and about how much
    /// that downloads (the sizes of its manifest's layers and config, read
    /// from registry.ollama.ai on 2026-10-27). For the starter model, the
    /// one this Mac's memory suits (`starter_model`).
    OllamaModel {
        name: &'static str,
        download_bytes: u64,
    },
}

/// The starter model by memory (phase 5 step D): Gemma 3, Google's open
/// model, the largest whose download is at most about two-fifths of the
/// least memory it is chosen for, so it runs beside everything else; the
/// 4b where the memory is not known. In this order, smallest first:
/// `(least memory, name, bytes)`.
const STARTER_MODELS: [(u64, &str, u64); 4] = [
    (0, "gemma3:1b", 806_067_407),
    (8 * GIB, "gemma3:4b", 3_350_272_403),
    (24 * GIB, "gemma3:12b", 8_164_039_732),
    (48 * GIB, "gemma3:27b", 17_413_409_652),
];

const GIB: u64 = 1 << 30;

/// The starter model for a Mac with `memory_bytes` of memory, as
/// `CatalogInstall::OllamaModel` (`STARTER_MODELS`).
pub fn starter_model(memory_bytes: Option<u64>) -> CatalogInstall {
    let memory = memory_bytes.unwrap_or(8 * GIB);
    let (_, name, download_bytes) = STARTER_MODELS
        .iter()
        .rev()
        .find(|(least, _, _)| memory >= *least)
        .copied()
        .unwrap_or(STARTER_MODELS[0]);
    CatalogInstall::OllamaModel {
        name,
        download_bytes,
    }
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
    /// A Homebrew formula by name, or a versioned one of it (`node@22`) that
    /// was not only brought in for another: a versioned formula is keg-only,
    /// and one installed as a dependency (code-server's node@22) is not
    /// linked, so its command is nowhere Terminal looks (from the
    /// independent review of the branch).
    Formula(&'static str),
    /// Any model this Mac's own Ollama lists: a Mac with one has a model to
    /// start with.
    AnyModel,
    /// Ollama.app in Applications (`Host::ollama_app`), its `ollama` command
    /// set up or not: dragged in from ollama.com and never opened, say.
    OllamaApp,
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
    /// Whether Discover installs it from here (`install_request`): every
    /// Homebrew formula and cask (step B; Ollama's with step D), and the
    /// starter model (step D). Homebrew itself is installed in Terminal
    /// (step C).
    pub installs_here: bool,
}

/// The catalog, in the order the Discover page lists it.
pub const CATALOG: [CatalogItem; 9] = [
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
        // Google's AI agent for Terminal, in place of Gemini CLI, whose
        // formula Homebrew deprecated on 2026-06-18 and disables on
        // 2026-12-18, naming this cask its replacement
        // (`deprecation_replacement_cask`; the author's decision G3).
        id: "antigravity-cli",
        category: CatalogCategory::AiAssistants,
        install: CatalogInstall::Homebrew {
            kind: ArtifactKind::Cask,
            name: "antigravity-cli",
        },
        recognized_by: &[Recognizer::Family("antigravity-cli")],
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
        recognized_by: &[
            Recognizer::Family("ollama"),
            Recognizer::Source("ollama"),
            Recognizer::OllamaApp,
        ],
        needs_macos: Some(14),
        installs_here: true,
    },
    CatalogItem {
        id: "model",
        category: CatalogCategory::LocalModels,
        // The 4b, for a Mac whose memory is not known: `entries` and
        // `install_request` put the one this Mac's memory suits.
        install: CatalogInstall::OllamaModel {
            name: "gemma3:4b",
            download_bytes: 3_350_272_403,
        },
        recognized_by: &[Recognizer::AnyModel],
        needs_macos: None,
        installs_here: true,
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
    /// Not installed, and there is no Homebrew.
    NeedsHomebrew,
    /// Not installed, and Homebrew is there but none can install now: none
    /// answered, or none can be written to. Its source's notice says why.
    HomebrewNotReady,
    /// Not installed, and this Mac's macOS is older than it needs.
    NeedsMacOS { major: u32 },
    /// Homebrew itself, on a Mac that is not Apple silicon and has none:
    /// Homebrew's official installer no longer installs on Intel Macs
    /// (phase 5 proposal §1.1).
    HomebrewUnsupported,
    /// The starter model, on a Mac with no Ollama source and no Ollama.app.
    NeedsOllama,
    /// The starter model, where Ollama is there but cannot pull now onto
    /// this Mac: not running, not answering, or view only -- the source's
    /// own notice says which -- or Ollama.app with no `ollama` command
    /// yet. Open Ollama is the window's where the app is not running.
    OllamaNotReady,
    /// The starter model, where every Ollama source is at an address
    /// `OLLAMA_HOST` sets that is not known to be this Mac's own
    /// (`host_is_this_mac`): not offered from here, and nothing in its
    /// notice says why, since it answers (from the review of abba6f7e).
    OllamaElsewhere,
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
    /// For the starter model, the room free on the home folder's volume
    /// (`Host::free_bytes`), for its sheet to say beside the download;
    /// `None` for every other entry, and where it could not be read.
    pub free_bytes: Option<u64>,
}

/// What this Mac is, for the states that depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host {
    /// macOS's major version, `None` where it could not be read.
    pub macos_major: Option<u32>,
    /// Whether the Mac is Apple silicon, whatever this process runs as.
    pub apple_silicon: bool,
    /// How much memory the Mac has, in bytes, for the starter model
    /// (`starter_model`); `None` where it could not be read.
    pub memory_bytes: Option<u64>,
    /// How much room the home folder's volume has free, in bytes, for the
    /// starter model's sheet (`CatalogEntry::free_bytes`); `None` where it
    /// could not be read.
    pub free_bytes: Option<u64>,
    /// Whether Ollama.app is in /Applications or ~/Applications
    /// (`ollama::ollama_app_path`), for `Recognizer::OllamaApp`.
    pub ollama_app: bool,
}

impl Host {
    /// This Mac, from the kernel: `kern.osproductversion`,
    /// `hw.optional.arm64`, which is 1 on Apple silicon under Rosetta too,
    /// and `hw.memsize` (`sysctlbyname`); and the free room of the home
    /// folder's volume (`statfs`); and whether Ollama.app is in
    /// Applications, looked up as Open Ollama looks it up. Runs nothing and
    /// opens nothing.
    pub fn of_this_mac() -> Host {
        let version = crate::diagnostics::read_macos_version();
        Host {
            macos_major: version.as_deref().and_then(major_of),
            apple_silicon: apple_silicon(),
            memory_bytes: memory_bytes(),
            free_bytes: std::env::var_os("HOME")
                .and_then(|home| free_bytes(std::path::Path::new(&home))),
            ollama_app: std::env::var_os("HOME").is_some_and(|home| {
                crate::adapters::ollama::ollama_app_path(std::path::Path::new(&home)).is_some()
            }),
        }
    }
}

impl Host {
    /// This Mac's macOS major version and chip only, as `of_this_mac`
    /// reads them; nothing of its memory, free room or Ollama.app: what a
    /// search's results need (`Session::search_homebrew`), and no more is
    /// looked at for one (from the review of the fix batch).
    pub(crate) fn chip_and_macos() -> Host {
        let version = crate::diagnostics::read_macos_version();
        Host {
            macos_major: version.as_deref().and_then(major_of),
            apple_silicon: apple_silicon(),
            memory_bytes: None,
            free_bytes: None,
            ollama_app: false,
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

#[cfg(target_os = "macos")]
fn memory_bytes() -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: `value` is a u64 and `len` says so; the kernel writes at most
    // `len` bytes into it. The name is a NUL-terminated literal.
    let read = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (read == 0 && len == std::mem::size_of::<u64>() && value > 0).then_some(value)
}

#[cfg(not(target_os = "macos"))]
fn memory_bytes() -> Option<u64> {
    None
}

/// The room free on `folder`'s volume for an unprivileged user, in bytes
/// (`f_bavail` blocks of `f_bsize`): `statfs`, which asks the file system
/// how much room it has and opens nothing in the folder.
fn free_bytes(folder: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(folder.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statfs` is plain data, for which all zeros is a value; the
    // kernel fills it in for the NUL-terminated `path`.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    let read = unsafe { libc::statfs(path.as_ptr(), &mut stat) };
    (read == 0).then(|| (stat.f_bavail as u64).saturating_mul(u64::from(stat.f_bsize)))
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
            install: install_on(item, host),
            state: state_of(item, snapshot, host, can_install),
            installs_here: item.installs_here,
            free_bytes: match item.install {
                CatalogInstall::OllamaModel { .. } => host.free_bytes,
                _ => None,
            },
        })
        .collect()
}

/// What installs `item` on the Mac `host` describes: the catalog's own,
/// but for the starter model, the one its memory suits.
fn install_on(item: &CatalogItem, host: &Host) -> CatalogInstall {
    match item.install {
        CatalogInstall::OllamaModel { .. } => starter_model(host.memory_bytes),
        install => install,
    }
}

/// The state on this Mac of the catalog entry that installs this Homebrew
/// formula or cask, if one does: a search result that is a suggestion is
/// installed only where its entry would be (`Session::plan_found`).
pub fn state_installing(
    snapshot: &Snapshot,
    host: &Host,
    kind: ArtifactKind,
    name: &str,
) -> Option<CatalogState> {
    let id = id_installing(kind, name)?;
    entries(snapshot, host)
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.state)
}

/// The catalog entry that installs this Homebrew formula or cask, if one
/// does: what the Discover search marks as one of the suggestions.
pub fn id_installing(kind: ArtifactKind, name: &str) -> Option<&'static str> {
    CATALOG.iter().find_map(|item| match item.install {
        CatalogInstall::Homebrew {
            kind: its_kind,
            name: its_name,
        } if its_kind == kind && its_name == name => Some(item.id),
        _ => None,
    })
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
    let brew = snapshot.instances.iter().find(|i| homebrew_can_install(i));
    let state = state_of(item, snapshot, host, brew.is_some());
    if state != CatalogState::NotInstalled {
        return Err(InstallRefusal::State(state));
    }
    // The source that installs it: the first Homebrew that can, or the
    // first Ollama that can pull.
    let (source, kind, name) = match install_on(item, host) {
        CatalogInstall::HomebrewItself => return Err(InstallRefusal::NotOffered),
        CatalogInstall::Homebrew { kind, name } => (brew, kind, name),
        CatalogInstall::OllamaModel { name, .. } => (
            snapshot.instances.iter().find(|i| ollama_can_pull(i)),
            ArtifactKind::Model,
            name,
        ),
    };
    let source = source.ok_or(InstallRefusal::State(state))?;
    Ok(OpRequest {
        kind: OpKind::Install,
        instance_id: source.id.clone(),
        artifact_kind: kind,
        name: name.to_string(),
    })
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
        .any(|way| has(way, &snapshot.instances, &snapshot.artifacts, host));
    if installed {
        return CatalogState::Installed;
    }
    match item.install {
        CatalogInstall::HomebrewItself if host.apple_silicon => CatalogState::NotInstalled,
        CatalogInstall::HomebrewItself => CatalogState::HomebrewUnsupported,
        CatalogInstall::Homebrew { .. } => match (item.needs_macos, host.macos_major) {
            (Some(major), Some(mac)) if mac < major => CatalogState::NeedsMacOS { major },
            _ if can_install => CatalogState::NotInstalled,
            _ if snapshot.instances.iter().any(|i| i.adapter_id == "brew") => {
                CatalogState::HomebrewNotReady
            }
            _ => CatalogState::NeedsHomebrew,
        },
        CatalogInstall::OllamaModel { .. } => {
            let ollamas = || {
                snapshot
                    .instances
                    .iter()
                    .filter(|i| i.adapter_id == "ollama")
            };
            if ollamas().any(ollama_can_pull) {
                CatalogState::NotInstalled
            } else if ollamas().next().is_some() && ollamas().all(ollama_elsewhere) {
                CatalogState::OllamaElsewhere
            } else if ollamas().next().is_some() || host.ollama_app {
                // An Ollama source that cannot pull, or the app with no
                // `ollama` command yet, whose first run offers to put it
                // in place.
                CatalogState::OllamaNotReady
            } else if let Some(major) = CATALOG
                .iter()
                .find(|item| item.id == "ollama")
                .and_then(|ollama| ollama.needs_macos)
                .filter(|major| host.macos_major.is_some_and(|mac| mac < *major))
            {
                // No Ollama, and this Mac cannot have one: the macOS
                // Ollama needs, not "Needs Ollama" beside Ollama's own
                // "Needs macOS 14" (from the independent review of the
                // branch).
                CatalogState::NeedsMacOS { major }
            } else {
                CatalogState::NeedsOllama
            }
        }
    }
}

/// Whether `instance` is an Ollama that can pull a model now onto this
/// Mac: it answered, is not view only (an address with a login in it, an
/// `ollama` that will not run), and is this Mac's (`this_macs_ollama`).
fn ollama_can_pull(instance: &ManagerInstance) -> bool {
    this_macs_ollama(&instance.id)
        && instance.status.unavailable.is_none()
        && instance.read_only_reason.is_none()
}

/// Whether `instance` is an Ollama at an address not known to be this
/// Mac's: one whose id names an address (`ollama:<OLLAMA_HOST>`; one
/// Banager cannot use has none) that `host_is_this_mac` does not take.
fn ollama_elsewhere(instance: &ManagerInstance) -> bool {
    instance
        .id
        .strip_prefix("ollama:")
        .is_some_and(|host| !crate::adapters::ollama::host_is_this_mac(host))
}

/// Whether the Ollama source `instance_id` (`ollama:<OLLAMA_HOST>`) is this
/// Mac's own: a model it pulls runs here, which the starter model is for.
/// One at another machine's address is not (`host_is_this_mac`).
fn this_macs_ollama(instance_id: &str) -> bool {
    instance_id
        .strip_prefix("ollama:")
        .is_some_and(crate::adapters::ollama::host_is_this_mac)
}

/// Whether `instance` is a Homebrew that can install: it answered, and is
/// not view only (a prefix this user cannot write, Homebrew run as root).
pub(crate) fn homebrew_can_install(instance: &ManagerInstance) -> bool {
    instance.adapter_id == "brew"
        && instance.status.unavailable.is_none()
        && instance.read_only_reason.is_none()
}

fn has(
    way: &Recognizer,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    host: &Host,
) -> bool {
    match *way {
        Recognizer::Family(family) => artifacts
            .iter()
            .any(|a| a.facts.family.as_deref() == Some(family)),
        Recognizer::Source(adapter) => instances.iter().any(|i| i.adapter_id == adapter),
        Recognizer::OllamaApp => host.ollama_app,
        Recognizer::AnyModel => artifacts
            .iter()
            .any(|a| a.key.kind == ArtifactKind::Model && this_macs_ollama(&a.key.instance_id)),
        Recognizer::Formula(name) => artifacts.iter().any(|a| {
            a.key.kind == ArtifactKind::Formula
                && a.key.instance_id.starts_with("brew:")
                && (a.key.name == name
                    || (a.reason != InstallReason::Dependency
                        && a.key.name.strip_prefix(name).is_some_and(|rest| {
                            rest.strip_prefix('@').is_some_and(|v| {
                                !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')
                            })
                        })))
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
        memory_bytes: Some(16 * GIB),
        free_bytes: Some(100 * GIB),
        ollama_app: false,
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
                ("antigravity-cli", CatalogState::NeedsHomebrew),
                ("ollama", CatalogState::NeedsHomebrew),
                ("model", CatalogState::NeedsOllama),
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
            memory_bytes: Some(16 * GIB),
            free_bytes: Some(100 * GIB),
            ollama_app: false,
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
                ("antigravity-cli", CatalogState::NotInstalled),
                ("ollama", CatalogState::NotInstalled),
                ("model", CatalogState::NeedsOllama),
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
    fn test_the_model_on_a_mac_too_old_for_ollama_needs_the_macos_ollama_needs() {
        let old = Host {
            macos_major: Some(13),
            ..SILICON_15
        };
        let none = snapshot(vec![instance("brew", "brew:/opt/homebrew")], Vec::new());
        let states = states(&none, &old);
        assert_eq!(
            states[4],
            ("ollama", CatalogState::NeedsMacOS { major: 14 })
        );
        assert_eq!(states[5], ("model", CatalogState::NeedsMacOS { major: 14 }));
        // macOS unknown: as before, it needs Ollama.
        let unknown = Host {
            macos_major: None,
            ..SILICON_15
        };
        assert_eq!(
            entries(&none, &unknown)
                .iter()
                .find(|e| e.id == "model")
                .map(|e| e.state),
            Some(CatalogState::NeedsOllama)
        );
    }

    #[test]
    fn test_the_model_planned_is_the_one_the_row_shows_for_this_macs_memory() {
        // From the independent review of the branch: install_request
        // reading the catalog's fixed model, not this Mac's, passed every
        // test, since the one model test was on 16 GiB.
        let o = "ollama:http://127.0.0.1:11434";
        let running = snapshot(vec![instance("ollama", o)], Vec::new());
        for (memory, model) in [
            (Some(4u64 << 30), "gemma3:1b"),
            (Some(16 << 30), "gemma3:4b"),
            (Some(32 << 30), "gemma3:12b"),
            (Some(64 << 30), "gemma3:27b"),
            (None, "gemma3:4b"),
        ] {
            let host = Host {
                memory_bytes: memory,
                ..SILICON_15
            };
            let shown = entries(&running, &host)
                .into_iter()
                .find(|entry| entry.id == "model")
                .expect("the model's row");
            assert!(
                matches!(&shown.install, CatalogInstall::OllamaModel { name, .. } if *name == model),
                "{memory:?}: {:?}",
                shown.install
            );
            let planned = install_request(&running, &host, "model").expect("planned");
            assert_eq!(planned.name, model, "{memory:?}");
        }
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
            let node = states(&mac, &SILICON_15)[6];
            assert_eq!(node.1 == CatalogState::Installed, installed, "{name}");
        }
        // A versioned one brought in only for another (code-server's
        // node@22, keg-only and not linked): Node.js is still to install.
        // The formula itself, as a dependency, is Node all the same.
        for (name, installed) in [("node@22", false), ("node", true)] {
            let mut dependency = artifact(b, ArtifactKind::Formula, name, None);
            dependency.reason = InstallReason::Dependency;
            let mac = snapshot(vec![instance("brew", b)], vec![dependency]);
            let node = states(&mac, &SILICON_15)[6];
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
        assert_eq!(states[7], ("uv", CatalogState::NotInstalled));
        assert_eq!(states[8], ("ffmpeg", CatalogState::NotInstalled));
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
            // Homebrew is there and cannot install: its notice says why.
            (vec![stopped.clone()], CatalogState::HomebrewNotReady),
            (vec![view_only], CatalogState::HomebrewNotReady),
            (vec![stopped, answering], CatalogState::NotInstalled),
        ] {
            let mac = snapshot(brews, Vec::new());
            let states = states(&mac, &SILICON_15);
            // Homebrew is there either way.
            assert_eq!(states[0], ("homebrew", CatalogState::Installed));
            assert_eq!(states[3], ("antigravity-cli", state));
        }
    }

    #[test]
    fn test_reads_a_volumes_free_room_and_nothing_for_a_folder_that_is_not_there() {
        assert!(free_bytes(&std::env::temp_dir()).is_some_and(|room| room > 0));
        assert_eq!(
            free_bytes(std::path::Path::new("/no/such/folder/banager-free")),
            None
        );
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
                "installs_here": false,
                "free_bytes": null
            })
        );
        assert_eq!(
            json[4],
            serde_json::json!({
                "id": "ollama",
                "category": "LocalModels",
                "install": {"Homebrew": {"kind": "Cask", "name": "ollama-app"}},
                "state": "NeedsHomebrew",
                "installs_here": true,
                "free_bytes": null
            })
        );
        assert_eq!(
            json[5],
            serde_json::json!({
                "id": "model",
                "category": "LocalModels",
                "install": {"OllamaModel": {"name": "gemma3:4b", "download_bytes": 3_350_272_403_u64}},
                "state": "NeedsOllama",
                "installs_here": true,
                "free_bytes": 107_374_182_400_u64
            })
        );
        let old = Host {
            macos_major: Some(13),
            apple_silicon: true,
            memory_bytes: Some(16 * GIB),
            free_bytes: Some(100 * GIB),
            ollama_app: false,
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
    fn test_the_starter_model_is_the_gemma_3_this_macs_memory_suits() {
        let name = |memory| match starter_model(memory) {
            CatalogInstall::OllamaModel { name, .. } => name,
            other => panic!("{other:?}"),
        };
        assert_eq!(name(Some(4 * GIB)), "gemma3:1b");
        assert_eq!(name(Some(8 * GIB)), "gemma3:4b");
        assert_eq!(name(Some(16 * GIB)), "gemma3:4b");
        assert_eq!(name(Some(24 * GIB)), "gemma3:12b");
        assert_eq!(name(Some(36 * GIB)), "gemma3:12b");
        assert_eq!(name(Some(64 * GIB)), "gemma3:27b");
        assert_eq!(name(None), "gemma3:4b");
        // Each download is at most two-fifths of the least memory it is
        // chosen for, and each name passes the Ollama adapter's check.
        for (least, name, bytes) in STARTER_MODELS {
            assert!(least == 0 || bytes * 5 <= least * 2, "{name}");
            crate::adapters::ollama::validate_model_reference(name).expect(name);
        }
        // The catalog lists the one for the Mac it is asked about.
        let empty = snapshot(Vec::new(), Vec::new());
        let big = Host {
            memory_bytes: Some(64 * GIB),
            ..SILICON_15
        };
        assert_eq!(
            entries(&empty, &big)[5].install,
            CatalogInstall::OllamaModel {
                name: "gemma3:27b",
                download_bytes: 17_413_409_652
            }
        );
    }

    #[test]
    fn test_the_starter_model_installs_through_an_ollama_that_can_pull_and_counts_any_model() {
        let o = "ollama:http://127.0.0.1:11434";
        let not_running = ManagerInstance {
            status: InstanceStatus {
                unavailable: Some(Unavailable::NotRunning),
                notes: Vec::new(),
                no_answer: None,
            },
            ..instance("ollama", o)
        };
        assert_eq!(
            states(
                &snapshot(vec![not_running.clone()], Vec::new()),
                &SILICON_15
            )[5],
            ("model", CatalogState::OllamaNotReady)
        );
        assert_eq!(
            install_request(
                &snapshot(vec![not_running], Vec::new()),
                &SILICON_15,
                "model"
            ),
            Err(InstallRefusal::State(CatalogState::OllamaNotReady))
        );
        let running = snapshot(vec![instance("ollama", o)], Vec::new());
        assert_eq!(
            states(&running, &SILICON_15)[5],
            ("model", CatalogState::NotInstalled)
        );
        assert_eq!(
            install_request(&running, &SILICON_15, "model"),
            Ok(OpRequest {
                kind: OpKind::Install,
                instance_id: o.to_string(),
                artifact_kind: ArtifactKind::Model,
                name: "gemma3:4b".to_string(),
            })
        );
        // An Ollama at another machine's address pulls onto that machine,
        // and its models are not this Mac's: the row says where it is, as
        // its notice does not (from the review of abba6f7e).
        let remote = "ollama:http://studio.local:11434";
        let elsewhere = snapshot(
            vec![instance("ollama", remote)],
            vec![artifact(remote, ArtifactKind::Model, "qwen3:8b", None)],
        );
        assert_eq!(
            states(&elsewhere, &SILICON_15)[5],
            ("model", CatalogState::OllamaElsewhere)
        );
        assert_eq!(
            install_request(&elsewhere, &SILICON_15, "model"),
            Err(InstallRefusal::State(CatalogState::OllamaElsewhere))
        );
        // An `OLLAMA_HOST` Banager cannot use names no address: not
        // elsewhere, and its notice says why.
        let unusable = ManagerInstance {
            status: InstanceStatus {
                unavailable: Some(Unavailable::InvalidOllamaHost),
                notes: Vec::new(),
                no_answer: None,
            },
            ..instance("ollama", "ollama")
        };
        assert_eq!(
            states(&snapshot(vec![unusable], Vec::new()), &SILICON_15)[5],
            ("model", CatalogState::OllamaNotReady)
        );
        // Ollama.app dragged in and never opened: no source, no `ollama`.
        // Ollama is there; the model waits for it to be opened.
        let app_only = Host {
            ollama_app: true,
            ..SILICON_15
        };
        let none = snapshot(vec![instance("brew", "brew:/opt/homebrew")], Vec::new());
        assert_eq!(
            states(&none, &app_only)[4],
            ("ollama", CatalogState::Installed)
        );
        assert_eq!(
            states(&none, &app_only)[5],
            ("model", CatalogState::OllamaNotReady)
        );
        // Any model at all: a Mac with one has a model to start with.
        let with_one = snapshot(
            vec![instance("ollama", o)],
            vec![artifact(o, ArtifactKind::Model, "qwen3:8b", None)],
        );
        assert_eq!(
            states(&with_one, &SILICON_15)[5],
            ("model", CatalogState::Installed)
        );
        assert_eq!(
            install_request(&with_one, &SILICON_15, "model"),
            Err(InstallRefusal::State(CatalogState::Installed))
        );
    }

    #[test]
    fn test_every_entry_but_homebrew_installs_from_here() {
        let here: Vec<&str> = CATALOG
            .iter()
            .filter(|item| item.installs_here)
            .map(|item| item.id)
            .collect();
        assert_eq!(
            here,
            [
                "claude-code",
                "codex",
                "antigravity-cli",
                "ollama",
                "model",
                "node",
                "uv",
                "ffmpeg"
            ]
        );
        for item in CATALOG.iter().filter(|item| item.installs_here) {
            assert!(
                !matches!(item.install, CatalogInstall::HomebrewItself),
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
            install_request(&mac, &SILICON_15, "uv"),
            Ok(OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/usr/local".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "uv".to_string(),
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
        // Not offered from here: Homebrew, and what is no entry -- a
        // package or model name included.
        for id in ["homebrew", "jq", "", "ollama-app", "gemma3:4b", "Codex"] {
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
            Err(InstallRefusal::State(CatalogState::HomebrewNotReady))
        );
    }
}
