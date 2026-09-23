use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type InstanceId = String; // "brew:/opt/homebrew"
pub type AdapterId = String; // "brew"

/// The one way an adapter builds a `ManagerInstance.id`: its own adapter id,
/// optionally followed by `:` and whatever tells its instances apart (a
/// prefix, a Python path, a host). `None` is for an adapter that only ever
/// has one instance, whose id is then the adapter id itself (`"pipx"`,
/// `"uv"`).
///
/// This is what makes ids unique *across* adapters by construction rather
/// than by convention: adapter ids are unique (`Session::with_adapters`
/// refuses a duplicate) and contain no `:`, so an id built here for one
/// adapter can never equal an id built here for another. Uniqueness
/// *within* an adapter is still that adapter's job -- a single-instance
/// adapter that one day returns two instances would repeat its id -- and
/// `Session::refresh` is what catches that case, loudly.
///
/// Every id this produces is byte-for-byte what the adapters wrote by hand
/// before it existed. That matters: ids are persisted, inside the
/// `ArtifactKey`s of `Settings.ignored_updates`, so changing their shape
/// would silently un-ignore every update the user had ignored.
pub fn instance_id(adapter_id: &str, qualifier: Option<&str>) -> InstanceId {
    debug_assert!(
        !adapter_id.contains(':'),
        "adapter id {adapter_id:?} must not contain ':'"
    );
    match qualifier {
        None => adapter_id.to_string(),
        Some(q) => format!("{adapter_id}:{q}"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    User,
    System,
}

/// Why this source can be listed but never changed from Canager.
///
/// An enum rather than a string because these reasons are shown to the
/// user, and an English sentence assembled on the Rust side cannot be
/// localised -- the existing `UpdateCandidate.warnings` already fell into
/// that trap (spec §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadOnlyReason {
    /// The tool itself offers no install/uninstall path Canager could
    /// safely drive (pip).
    ByDesign,
    /// The tool can install and uninstall, but the directory it writes to
    /// is not writable by the current user (Node installed from the
    /// nodejs.org package, whose npm prefix is root-owned).
    PrefixNotWritable,
}

/// Why a source Canager knows about cannot answer right now. The state
/// axis, orthogonal to `ReadOnlyReason`: an Ollama that is not running is
/// still perfectly writable, it just has nothing to say until it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unavailable {
    /// The service is not running and Canager can start it: the notice
    /// carries a button that does. Today that is only an Ollama whose
    /// daemon is on this Mac and whose Ollama.app is installed -- see
    /// `OllamaAdapter::detect`, which gives a silent daemon it cannot start
    /// `NotResponding` instead.
    NotRunning,
    /// The executable is on PATH but would not run, or its version could
    /// not be recognised, or a service did not answer and Canager has no
    /// way to start it (an Ollama installed as the command-line tool only,
    /// or one whose `OLLAMA_HOST` names another machine).
    NotResponding,
    /// The tool is installed but refuses to do anything while Canager is
    /// running as root, so Canager never even asked it (Homebrew).
    ///
    /// A third variant rather than a reuse of `NotResponding` because
    /// these divide by *what the user can do about it*: `NotRunning` means
    /// "start it", `NotResponding` means "reopen Canager, then consider
    /// reinstalling", and this one means "quit and open Canager again
    /// without `sudo`" -- a specific, different, and actually effective
    /// action, which is exactly what the notice says.
    RefusesAsRoot,
}

/// Something a source answered *with*, that changes how its answer should
/// be read. Deliberately payload-free: a data-carrying variant would turn
/// a bare-string unit variant into an externally tagged object on the
/// wire, and the TypeScript mirror is hand-written (spec §2.3's note). The
/// stderr text such a payload would carry is a known, accepted loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceNote {
    /// `brew update` failed, so the local catalogue may be behind and
    /// "no updates" may be wrong.
    IndexMayBeStale,
    /// `brew update` is still downloading, so the refresh did not read the
    /// catalogue it is rewriting (`AdapterError::IndexUpdating`). This
    /// source's update candidates are the previous snapshot's, and so are
    /// its installed packages unless the refresh read them before it
    /// started the update itself; with no previous snapshot, there are
    /// none. Nothing has failed. When the update ends the shell refreshes
    /// again (see `Session::background_change`), so this clears by itself.
    IndexUpdating,
}

/// The state axis of a source: can Canager talk to it at all, and is there
/// anything about this answer the user has to know to read it correctly.
///
/// Deliberately *without* a per-instance `refreshed_at` (spec §2.4's note):
/// `Snapshot::same_content` compares `instances` with the derived
/// `PartialEq`, so a unix second that moves every refresh would bump the
/// generation and rebroadcast `SnapshotChanged` on every poll, and there is
/// no renderer for a relative timestamp anywhere in `src/`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceStatus {
    /// `None` means the source answered.
    pub unavailable: Option<Unavailable>,
    pub notes: Vec<InstanceNote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerInstance {
    pub id: InstanceId,
    pub adapter_id: AdapterId,
    pub exe_path: PathBuf,
    pub prefix: PathBuf,
    pub scope: Scope,
    pub version: Option<String>,
    /// None when the adapter's metadata lists no verified versions, or when
    /// the detected version is among them. Some(detected) when it is not,
    /// so the UI can mark the source as running an unverified version (spec
    /// §4.1).
    pub unverified_version: Option<String>,
    /// `None` means writable. The single source of truth for the capability
    /// axis: there is deliberately no companion `writable: bool` for it to
    /// disagree with, and no setter -- every `detect()` builds this struct
    /// as a literal, so the compiler makes each adapter answer the question
    /// exactly once.
    pub read_only_reason: Option<ReadOnlyReason>,
    /// The state axis: whether this source answered, and anything about
    /// that answer the user has to know. Replaced `healthy: bool`, which
    /// was exactly `status.unavailable.is_none()` with no room for a
    /// reason or a note.
    pub status: InstanceStatus,
}

impl ManagerInstance {
    /// Whether Canager may offer operations on this source at all.
    ///
    /// The capability half of the actionability invariant (spec §2.5);
    /// `Session::issue_plan` is the single gate that enforces it, and
    /// `canWrite()` in `src/lib/sources.ts` is its front-end mirror.
    pub fn writable(&self) -> bool {
        self.read_only_reason.is_none()
    }

    /// Whether this source answered the last refresh. The state half of
    /// the same invariant; `Session::issue_plan` requires both, because a
    /// carried-forward artifact from a stopped Ollama must offer no
    /// Uninstall button (spec §2.5).
    pub fn available(&self) -> bool {
        self.status.unavailable.is_none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactKind {
    Formula,
    Cask,
    Package,
    Tool,
    Model,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallReason {
    Requested,
    Dependency,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactKey {
    pub instance_id: InstanceId,
    pub kind: ArtifactKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledArtifact {
    pub key: ArtifactKey,
    pub display_name: String,
    pub version: String,
    pub reason: InstallReason,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub size_bytes: Option<u64>,
    pub installed_at: Option<i64>, // unix seconds
    pub path: Option<PathBuf>,
    pub auto_updates: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Native,
    Registry,
    Digest,
}

/// A specific warning `Plan` or `UpdateCandidate` carries, so the UI can
/// render it in the user's language rather than the English sentence Rust
/// would otherwise have to assemble -- the trap `UpdateCandidate.warnings`
/// was already in before this type existed (see `ReadOnlyReason`'s doc
/// comment) and, concretely, the reason the uninstall confirmation screen
/// used to show a Chinese user an English risk warning right above the
/// button that acts on it (spec §6).
///
/// `Message` is the deliberate escape hatch for warnings this step does
/// not localise: text built at runtime from something Canager cannot know
/// ahead of time (a subprocess's stderr, an HTTP error). A warning whose
/// only unknown is a value -- a registry host, a list of dependents --
/// does not belong here; it gets a variant with a payload, like
/// `ThirdPartyRegistry` and `WouldBreak`. Spec §6 backlogs the real fix
/// for the genuinely unknowable ones -- showing a localised
/// generic sentence by default and routing the raw text behind
/// `show_technical_details` -- so `Message` only preserves today's
/// behaviour (the raw string, unconditionally, in whatever language it
/// came in) rather than pretending those warnings are localised when they
/// are not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Warning {
    /// brew's `uses --installed` check itself failed or timed out. Not the
    /// same thing as "confirmed no dependents", and must not read like it.
    DependentsUnknown,
    /// Uninstalling would break these already-installed dependents.
    WouldBreak { names: Vec<String> },
    /// No `cargo-binstall` on PATH: install/upgrade compiles from source,
    /// which can take a while.
    CompilesLocally,
    /// Installed from a git repository or a local path, not the crates.io
    /// registry Canager checks for updates against.
    NonRegistrySource,
    /// Installing or upgrading this model downloads it from `host`, a
    /// registry other than Ollama's own library. Carried only on
    /// Install/Upgrade plans: where a model came from is a reason to look
    /// twice before fetching it, and no reason at all to hesitate before
    /// deleting it.
    ThirdPartyRegistry { host: String },
    /// Not yet localised -- see this type's doc comment.
    Message(String),
}

/// Why the tool itself will refuse to update this one package, although
/// its source is writable and answering. The per-package half of the
/// actionability gate (spec §8); `ReadOnlyReason` and `Unavailable` are the
/// per-source halves.
///
/// A variant belongs here only when the tool *reports* the state in the
/// output Canager already reads to list updates, so the row can be marked
/// before anyone clicks. States a tool only reveals by refusing (a
/// disabled formula, a cask whose installer must be run by hand) do not
/// qualify: `brew outdated --json=v2` carries no field for them
/// (`cmd/outdated.rb:196-200` in Homebrew 7.0.6 lists all five keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateBlocked {
    /// Someone pinned this package in its tool, which holds it at the
    /// version it has now. Two tools produce it:
    /// - Homebrew (`brew pin`), for a formula or a cask. `brew outdated`
    ///   still lists it, marked `pinned: true`, and a named `brew upgrade`
    ///   of it exits 1 with "Not upgrading 1 pinned package"
    ///   (`cmd/upgrade.rb:428-476`, `cask/upgrade.rb:82-90`). Read by
    ///   `parse_outdated` in `adapters/brew/parse.rs`.
    /// - pipx (`pipx pin`), for a tool. `pipx list --outdated` still lists
    ///   it, as `name [pinned]: old -> new` (pipx 1.17.3's
    ///   `commands/outdated.py:243`), and `pipx upgrade` of it changes
    ///   nothing but exits 0 (`commands/upgrade.py:408-409` and `:74-81`).
    ///   Read by `parse_outdated` in `adapters/pipx.rs`.
    Pinned,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCandidate {
    pub key: ArtifactKey,
    pub current: String,
    pub target: String,
    pub channel: UpdateChannel,
    pub checkable: bool,
    pub warnings: Vec<Warning>,
    /// `Some` when the tool will refuse to update this package even though
    /// Canager could check it -- `checkable` says nothing about this: a
    /// pinned formula's newer version is known exactly. `Session::issue_plan`
    /// refuses an `Upgrade` of a candidate that carries one, and the Updates
    /// page's `isActionable` hides the row's button and checkbox for it.
    pub blocked: Option<UpdateBlocked>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub adapter_id: AdapterId,
    pub kind: ArtifactKind,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    Install,
    Uninstall,
    Upgrade,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpRequest {
    pub kind: OpKind,
    pub instance_id: InstanceId,
    pub artifact_kind: ArtifactKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelPolicy {
    SafeKill,
    KillThenReconcile,
    NoCancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceLock(pub String); // "brew:/opt/homebrew"

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub request: OpRequest,
    pub program: PathBuf,
    pub args: Vec<String>, // argv without program; preview = program + args
    pub env: Vec<(String, String)>,
    pub needs_password: bool,
    pub locks: Vec<ResourceLock>,
    pub cancel_policy: CancelPolicy,
    pub warnings: Vec<Warning>,
    pub affected: Vec<String>, // dependents that would break on uninstall
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Succeeded,
    /// The user pressed Cancel and the request did not take effect: either
    /// the command never started, or it was stopped and reconcile shows the
    /// artifact exactly as it was before. A cancel that lost the race to the
    /// command finishing is `Succeeded`, not this.
    Cancelled,
    /// The command reported success but reconcile disagrees. Carries
    /// which disagreement, never a sentence: the front end words it in the
    /// user's language (the drawer and the operation bar both show it).
    NeedsAttention(Attention),
    /// The tool ran and failed. `summary` is the last lines of the tool's
    /// own stderr and nothing else: the front end shows it as-is, quoted
    /// inside a translated sentence, because it is another program's words.
    /// A failure of Canager's own is `CanagerFailed`, never this.
    Failed {
        exit_code: Option<i32>,
        summary: String,
    },
    /// Canager itself could not carry the operation out -- not the tool.
    /// Carries which reason, never a sentence: the front end words it in
    /// the user's language, the same way it does `NeedsAttention`.
    ///
    /// These used to be English sentences of Canager's own ("operation
    /// panicked", "runner: program not found: ...") inside `Failed`'s
    /// `summary`, sharing one string with a tool's stderr, so neither could
    /// be shown properly: the front end could not translate the first
    /// without mangling the second.
    CanagerFailed(Fault),
    Unconfirmed,
}

/// Why Canager itself could not carry an operation out. See
/// [`Outcome::CanagerFailed`]. Fields carry data, never Canager's prose:
/// a path, or the operating system's own reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    /// Canager crashed partway through. The command may or may not have
    /// run, so only a fresh look at the list can say what changed.
    Panicked,
    /// The program the plan names was not there when Canager went to run
    /// it. Nothing was started.
    ProgramMissing { program: String },
    /// macOS would not start the program; `detail` is the operating
    /// system's own reason, quoted as-is. Nothing was started.
    SpawnFailed { detail: String },
    /// A `brew update` was still running in the background after Canager
    /// had waited `minutes` minutes for it, so the command was not
    /// started: installing while Homebrew rewrites its own list of
    /// software is not something Homebrew guards against. Nothing was
    /// started.
    ///
    /// `minutes` is `BrewAdapter::OP_UPDATE_WAIT` outside tests, carried
    /// here rather than hard-coded into
    /// `operations.outcome.CanagerFailed.HomebrewStillUpdating` so the two
    /// can never disagree: see `BrewAdapter::execute`, the only production
    /// call site that builds this variant.
    HomebrewStillUpdating { minutes: u64 },
    /// Something on Canager's side did not add up (an unregistered
    /// adapter or instance, a queue that closed, an error `execute` has no
    /// business returning). A bug in Canager, not a state of the Mac.
    /// Nothing was started.
    Internal,
}

/// What reconcile found that the command's own success did not account
/// for. See [`Outcome::NeedsAttention`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attention {
    /// An install exited 0 and the item is not installed.
    NotInstalledAfterInstall,
    /// An uninstall exited 0 and the item is still installed.
    StillInstalledAfterUninstall,
    /// An upgrade exited 0 and the item is no longer installed at all.
    GoneAfterUpgrade,
    /// An upgrade exited 0 and the item is still installed at the version
    /// it was at before: the tool skipped it without saying so in its exit
    /// code. `run_operation` (`crates/canager-core/src/ops/mod.rs`) builds
    /// this only when two reads of the installed version, one taken before
    /// the command and one after, both succeeded and are equal.
    UnchangedAfterUpgrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpStatus {
    Queued,
    Running,
    CancelRequested,
    Cancelling,
    Verifying,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled {
    pub present: bool,
    /// The installed version, as the adapter's own inventory spells it.
    /// `run_operation` compares a reading taken before an upgrade with one
    /// taken after, so this only has to be read the same way twice, not
    /// to agree with any other spelling of the same version.
    ///
    /// `None` when the artifact is not present, and also when the string
    /// the inventory has cannot tell one install from another: a Homebrew
    /// `version :latest` cask is installed as "latest" before and after
    /// every upgrade (`BrewAdapter::reconcile`).
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_id_reproduces_every_shape_already_persisted() {
        // Ids live on disk inside `Settings.ignored_updates`; the shared
        // constructor must not change a single one of them.
        assert_eq!(instance_id("pipx", None), "pipx");
        assert_eq!(instance_id("uv", None), "uv");
        assert_eq!(
            instance_id("brew", Some("/opt/homebrew")),
            "brew:/opt/homebrew"
        );
        assert_eq!(
            instance_id("ollama", Some("127.0.0.1:11434")),
            "ollama:127.0.0.1:11434"
        );
    }

    #[test]
    fn test_manager_instance_round_trips_through_json() {
        let instance = ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            unverified_version: None,
            read_only_reason: None,
            status: InstanceStatus::default(),
        };
        let json = serde_json::to_string(&instance).expect("serialize");
        let back: ManagerInstance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(instance, back);
    }

    #[test]
    fn test_manager_instance_with_unverified_version_round_trips_through_json() {
        let instance = ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("99.9.9".to_string()),
            unverified_version: Some("99.9.9".to_string()),
            read_only_reason: None,
            status: InstanceStatus::default(),
        };
        let json = serde_json::to_string(&instance).expect("serialize");
        assert!(json.contains("\"unverified_version\":\"99.9.9\""));
        let back: ManagerInstance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(instance, back);
    }

    #[test]
    fn test_read_only_reason_is_a_bare_string_on_the_wire_and_drives_writable() {
        // The hand-written TypeScript mirror (`src/lib/types.ts`) spells
        // these as `"ByDesign" | "PrefixNotWritable" | null`, so the wire
        // shape is the contract, not an implementation detail: a bare
        // string for a reason, `null` for a writable source.
        let writable = ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            unverified_version: None,
            read_only_reason: None,
            status: InstanceStatus::default(),
        };
        assert!(writable.writable());
        let json = serde_json::to_string(&writable).expect("serialize");
        assert!(
            json.contains("\"read_only_reason\":null"),
            "writable instances carry an explicit null, not a missing key: {json}"
        );
        assert_eq!(
            serde_json::from_str::<ManagerInstance>(&json).expect("deserialize"),
            writable
        );

        for reason in [ReadOnlyReason::ByDesign, ReadOnlyReason::PrefixNotWritable] {
            let read_only = ManagerInstance {
                read_only_reason: Some(reason),
                ..writable.clone()
            };
            assert!(
                !read_only.writable(),
                "{reason:?} must make the instance non-writable"
            );
            let json = serde_json::to_string(&read_only).expect("serialize");
            assert!(
                json.contains(&format!("\"read_only_reason\":\"{reason:?}\"")),
                "a reason is a bare string on the wire: {json}"
            );
            assert_eq!(
                serde_json::from_str::<ManagerInstance>(&json).expect("deserialize"),
                read_only
            );
        }
    }

    #[test]
    fn test_update_blocked_is_a_bare_string_on_the_wire_and_null_when_absent() {
        // `src/lib/types.ts` spells this field `blocked: UpdateBlocked |
        // null` and the variant as the bare string "Pinned". Nothing checks
        // that at compile time across the IPC boundary, so the wire shape
        // is pinned down here.
        let candidate = UpdateCandidate {
            key: ArtifactKey {
                instance_id: "brew:/opt/homebrew".to_string(),
                kind: ArtifactKind::Formula,
                name: "glib".to_string(),
            },
            current: "2.88.3".to_string(),
            target: "2.90.0".to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
            blocked: None,
        };
        let json = serde_json::to_string(&candidate).expect("serialize");
        assert!(
            json.contains("\"blocked\":null"),
            "an updatable candidate carries an explicit null, not a missing key: {json}"
        );
        assert_eq!(
            serde_json::from_str::<UpdateCandidate>(&json).expect("deserialize"),
            candidate
        );

        let pinned = UpdateCandidate {
            blocked: Some(UpdateBlocked::Pinned),
            ..candidate
        };
        let json = serde_json::to_string(&pinned).expect("serialize");
        assert!(
            json.contains("\"blocked\":\"Pinned\""),
            "a reason is a bare string on the wire: {json}"
        );
        assert_eq!(
            serde_json::from_str::<UpdateCandidate>(&json).expect("deserialize"),
            pinned
        );
    }

    #[test]
    fn test_warning_wire_shapes_match_the_hand_written_ts_mirror() {
        // Unit variants are bare strings and the one data variant is
        // externally tagged, matching every other enum in this module and
        // the hand-written mirror in `src/lib/types.ts`.
        assert_eq!(
            serde_json::to_string(&Warning::DependentsUnknown).unwrap(),
            r#""DependentsUnknown""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::CompilesLocally).unwrap(),
            r#""CompilesLocally""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::NonRegistrySource).unwrap(),
            r#""NonRegistrySource""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::WouldBreak {
                names: vec!["python@3.13".to_string()]
            })
            .unwrap(),
            r#"{"WouldBreak":{"names":["python@3.13"]}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::ThirdPartyRegistry {
                host: "modelscope.cn".to_string()
            })
            .unwrap(),
            r#"{"ThirdPartyRegistry":{"host":"modelscope.cn"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::Message("boom".to_string())).unwrap(),
            r#"{"Message":"boom"}"#
        );
        let round_tripped: Warning =
            serde_json::from_str(r#"{"WouldBreak":{"names":["a","b"]}}"#).unwrap();
        assert_eq!(
            round_tripped,
            Warning::WouldBreak {
                names: vec!["a".to_string(), "b".to_string()]
            }
        );
    }

    #[test]
    fn test_outcome_failed_round_trips_through_json() {
        let outcome = Outcome::Failed {
            exit_code: Some(1),
            summary: "boom".to_string(),
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let back: Outcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(outcome, back);
    }

    #[test]
    fn test_canager_failed_is_externally_tagged_on_the_wire() {
        // `src/lib/types.ts` mirrors `Fault` as a union of bare strings
        // (unit variants) and single-key objects (data variants), and
        // `format.ts` builds the locale key from the variant name.
        assert_eq!(
            serde_json::to_string(&Outcome::CanagerFailed(Fault::Panicked)).unwrap(),
            r#"{"CanagerFailed":"Panicked"}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::CanagerFailed(Fault::ProgramMissing {
                program: "/opt/homebrew/bin/brew".to_string()
            }))
            .unwrap(),
            r#"{"CanagerFailed":{"ProgramMissing":{"program":"/opt/homebrew/bin/brew"}}}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::CanagerFailed(Fault::SpawnFailed {
                detail: "Permission denied (os error 13)".to_string()
            }))
            .unwrap(),
            r#"{"CanagerFailed":{"SpawnFailed":{"detail":"Permission denied (os error 13)"}}}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::CanagerFailed(Fault::HomebrewStillUpdating {
                minutes: 10
            }))
            .unwrap(),
            r#"{"CanagerFailed":{"HomebrewStillUpdating":{"minutes":10}}}"#
        );
        for fault in [
            Fault::Panicked,
            Fault::HomebrewStillUpdating { minutes: 10 },
            Fault::Internal,
        ] {
            let json = serde_json::to_string(&Outcome::CanagerFailed(fault.clone())).unwrap();
            let back: Outcome = serde_json::from_str(&json).unwrap();
            assert_eq!(back, Outcome::CanagerFailed(fault));
        }
    }

    #[test]
    fn test_needs_attention_is_a_bare_variant_name_on_the_wire() {
        // `src/lib/types.ts` mirrors this as `{ NeedsAttention: Attention }`
        // with `Attention` a union of bare strings, and `format.ts` builds
        // the locale key from that string.
        assert_eq!(
            serde_json::to_string(&Outcome::NeedsAttention(Attention::GoneAfterUpgrade)).unwrap(),
            r#"{"NeedsAttention":"GoneAfterUpgrade"}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade))
                .unwrap(),
            r#"{"NeedsAttention":"UnchangedAfterUpgrade"}"#
        );
    }

    #[test]
    fn test_plan_round_trips_through_json() {
        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/opt/homebrew".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".to_string(),
            },
            program: PathBuf::from("/opt/homebrew/bin/brew"),
            args: vec![
                "install".to_string(),
                "--formula".to_string(),
                "jq".to_string(),
            ],
            env: vec![("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())],
            needs_password: false,
            locks: vec![ResourceLock("brew:/opt/homebrew".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: vec![],
            affected: vec![],
            timeout_secs: 1800,
        };
        let json = serde_json::to_string(&plan).expect("serialize");
        let back: Plan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(plan, back);
    }

    #[test]
    fn test_instance_status_is_default_empty_and_bare_strings_on_the_wire() {
        // The hand-written TypeScript mirror (`src/lib/types.ts`) spells
        // this as `{ unavailable: Unavailable | null; notes: InstanceNote[] }`
        // with bare-string variants, so the wire shape is the contract:
        // `null` for an available source, `[]` for no notes, and never a
        // missing key.
        let status = InstanceStatus::default();
        assert_eq!(status.unavailable, None);
        assert!(status.notes.is_empty());
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, r#"{"unavailable":null,"notes":[]}"#);

        for unavailable in [
            Unavailable::NotRunning,
            Unavailable::NotResponding,
            Unavailable::RefusesAsRoot,
        ] {
            let status = InstanceStatus {
                unavailable: Some(unavailable),
                notes: vec![InstanceNote::IndexMayBeStale],
            };
            let json = serde_json::to_string(&status).expect("serialize");
            assert_eq!(
                json,
                format!(r#"{{"unavailable":"{unavailable:?}","notes":["IndexMayBeStale"]}}"#)
            );
            assert_eq!(
                serde_json::from_str::<InstanceStatus>(&json).expect("deserialize"),
                status
            );
        }

        // Each note is a bare string too.
        let status = InstanceStatus {
            unavailable: None,
            notes: vec![InstanceNote::IndexUpdating],
        };
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, r#"{"unavailable":null,"notes":["IndexUpdating"]}"#);
        assert_eq!(
            serde_json::from_str::<InstanceStatus>(&json).expect("deserialize"),
            status
        );
    }
}
