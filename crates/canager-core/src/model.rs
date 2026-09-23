use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type InstanceId = String; // "brew:/opt/homebrew"
pub type AdapterId = String; // "brew"

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCandidate {
    pub key: ArtifactKey,
    pub current: String,
    pub target: String,
    pub channel: UpdateChannel,
    pub checkable: bool,
    pub warnings: Vec<Warning>,
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
    NoChange,
    PartialSuccess,
    NeedsAttention(String),
    Failed {
        exit_code: Option<i32>,
        summary: String,
    },
    Unconfirmed,
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
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
