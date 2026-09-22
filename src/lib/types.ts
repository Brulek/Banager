export type ArtifactKind = "Formula" | "Cask" | "Package" | "Tool" | "Model" | "Binary";
export type InstallReason = "Requested" | "Dependency" | "Unknown";
export type OpKind = "Install" | "Uninstall" | "Upgrade";
export type OpStatus =
  | "Queued"
  | "Running"
  | "CancelRequested"
  | "Cancelling"
  | "Verifying"
  | "Done";
export type DetectOutcome = "Found" | "Missing" | "RefusedAsRoot";
export type Outcome =
  | "Succeeded"
  | "NoChange"
  | "PartialSuccess"
  | "Unconfirmed"
  | { NeedsAttention: string }
  | { Failed: { exit_code: number | null; summary: string } };
export interface ArtifactKey {
  instance_id: string;
  kind: ArtifactKind;
  name: string;
}
export interface InstalledArtifact {
  key: ArtifactKey;
  display_name: string;
  version: string;
  reason: InstallReason;
  description: string | null;
  homepage: string | null;
  size_bytes: number | null;
  installed_at: number | null;
  path: string | null;
  auto_updates: boolean;
}
export interface UpdateCandidate {
  key: ArtifactKey;
  current: string;
  target: string;
  channel: "Native" | "Registry" | "Digest";
  checkable: boolean;
  warnings: string[];
}
/**
 * Why a source can be listed but never changed from Canager. Mirrors
 * `ReadOnlyReason` in crates/canager-core/src/model.rs: bare-string unit
 * variants, so a new Rust variant does *not* fail this union at compile
 * time -- it lands in whatever default branch reads it. `types.test.ts`
 * keeps a shape test over both spellings.
 */
export type ReadOnlyReason = "ByDesign" | "PrefixNotWritable";
export interface ManagerInstance {
  id: string;
  adapter_id: string;
  exe_path: string;
  prefix: string;
  scope: "User" | "System";
  version: string | null;
  healthy: boolean;
  unverified_version: string | null;
  /** `null` means writable; see `canWrite()` in src/lib/sources.ts. */
  read_only_reason: ReadOnlyReason | null;
}
export interface Plan {
  request: OpRequest;
  program: string;
  args: string[];
  env: [string, string][];
  needs_password: boolean;
  locks: string[];
  cancel_policy: "SafeKill" | "KillThenReconcile" | "NoCancel";
  warnings: string[];
  affected: string[];
  timeout_secs: number;
}
export interface OpRequest {
  kind: OpKind;
  instance_id: string;
  artifact_kind: ArtifactKind;
  name: string;
}
export interface IssuedPlan {
  id: number;
  plan: Plan;
  issued_at: number;
}
export interface OpSummary {
  id: number;
  kind: OpKind;
  instance_id: string;
  artifact_kind: ArtifactKind;
  name: string;
  status: OpStatus;
  outcome: Outcome | null;
  argv_preview: string[];
}
export interface SourceError {
  instance_id: string;
  message: string;
}
export interface Snapshot {
  generation: number;
  detect: DetectOutcome;
  instances: ManagerInstance[];
  artifacts: InstalledArtifact[];
  updates: UpdateCandidate[];
  refreshed_at: number | null;
  stale: boolean;
  errors: SourceError[];
}
export type Language = "System" | "En" | "ZhCn";
export interface Settings {
  language: Language;
  show_technical_details: boolean;
  ignored_updates: ArtifactKey[];
  include_self_updating: boolean;
}
export type OperationEvent =
  | { Status: { op_id: number; status: OpStatus } }
  | { Log: { op_id: number; stream: "Stdout" | "Stderr"; line: string } }
  | { Finished: { op_id: number; outcome: Outcome } };
export type UiEvent = { Operation: OperationEvent } | { SnapshotChanged: { generation: number } };
