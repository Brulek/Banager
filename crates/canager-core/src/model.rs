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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerInstance {
    pub id: InstanceId,
    pub adapter_id: AdapterId,
    pub exe_path: PathBuf,
    pub prefix: PathBuf,
    pub scope: Scope,
    pub version: Option<String>,
    pub healthy: bool,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCandidate {
    pub key: ArtifactKey,
    pub current: String,
    pub target: String,
    pub channel: UpdateChannel,
    pub checkable: bool,
    pub warnings: Vec<String>,
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
    pub warnings: Vec<String>,
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
            healthy: true,
            unverified_version: None,
            read_only_reason: None,
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
            healthy: true,
            unverified_version: Some("99.9.9".to_string()),
            read_only_reason: None,
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
            healthy: true,
            unverified_version: None,
            read_only_reason: None,
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
}
