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
export type DetectOutcome = "Found" | "Missing";
// Rust `Attention`: which way reconcile contradicted a command that
// reported success. Worded by the front end, per variant.
export type Attention =
  | "NotInstalledAfterInstall"
  | "StillInstalledAfterUninstall"
  | "GoneAfterUpgrade"
  | "UnchangedAfterUpgrade";
// Rust `Fault`: why Canager itself could not carry an operation out.
// Unit variants are bare strings, data variants single-key objects (serde's
// external tagging). Worded by the front end, per variant; the fields are
// data (a path, the operating system's own reason, the minutes
// `BrewAdapter::OP_UPDATE_WAIT` is), never Canager's prose.
export type Fault =
  | "Panicked"
  | { ProgramMissing: { program: string } }
  | { SpawnFailed: { detail: string } }
  | { HomebrewStillUpdating: { minutes: number } }
  | "Internal";
// `Failed.summary` is only ever the tool's own stderr; Canager's own
// failures are `CanagerFailed`.
export type Outcome =
  | "Succeeded"
  | "Cancelled"
  | "Unconfirmed"
  | { NeedsAttention: Attention }
  | { Failed: { exit_code: number | null; summary: string } }
  | { CanagerFailed: Fault };
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
  uninstall_blocked: UninstallBlocked | null;
}
/**
 * Why the tool itself will refuse to uninstall this one package. Mirrors
 * `UninstallBlocked` in crates/canager-core/src/model.rs: bare-string unit
 * variants. Only `Pinned` exists, produced by brew's `parse_info_installed`
 * (from `brew info --installed --json=v2`'s `pinned: true`). Read through
 * `UNINSTALL_BLOCKED_KEYS` in src/lib/sources.ts, a `Record` over this
 * union, so a variant added here without copy fails `tsc`.
 */
export type UninstallBlocked = "Pinned";
/**
 * A specific warning `Plan` or `UpdateCandidate` carries. Mirrors `Warning`
 * in crates/canager-core/src/model.rs: bare-string unit variants,
 * externally tagged data variants (`WouldBreak`, whose `names` interpolate
 * and pluralise the copy in `src/lib/warnings.ts`, and
 * `ThirdPartyRegistry`, whose `host` interpolates it), and a `Message`
 * catch-all for warnings this phase does not localise (spec §6's
 * `show_technical_details` backlog item) -- rendered as the raw string it
 * carries, same as before this type existed. A new Rust variant this union
 * does not yet spell lands in `warningText`'s default branch rather than
 * failing at compile time; `types.test.ts` keeps a shape test over all of
 * them.
 */
export type Warning =
  | "DependentsUnknown"
  | { WouldBreak: { names: string[] } }
  | "CompilesLocally"
  | "NonRegistrySource"
  | { ThirdPartyRegistry: { host: string } }
  | { Message: string };
/**
 * Why the tool itself will refuse to update this one package, although its
 * source is writable and answering. Mirrors `UpdateBlocked` in
 * crates/canager-core/src/model.rs: bare-string unit variants. Only
 * `Pinned` exists, produced by brew's `parse_outdated` (from `brew
 * outdated`'s `pinned: true`) and pipx's (from `pipx list --outdated`'s
 * `name [pinned]:`). Read through `UPDATE_BLOCKED_KEYS` in src/lib/sources.ts,
 * a `Record` over this union, so a variant added here without copy fails
 * `tsc` rather than rendering nothing.
 */
export type UpdateBlocked = "Pinned";
export interface UpdateCandidate {
  key: ArtifactKey;
  current: string;
  target: string;
  channel: "Native" | "Registry" | "Digest";
  checkable: boolean;
  warnings: Warning[];
  blocked: UpdateBlocked | null;
}
/**
 * Why a source can be listed but never changed from Canager. Mirrors
 * `ReadOnlyReason` in crates/canager-core/src/model.rs: bare-string unit
 * variants, so a new Rust variant does *not* fail this union at compile
 * time -- it lands in whatever default branch reads it. `types.test.ts`
 * keeps a shape test over both spellings.
 */
export type ReadOnlyReason = "ByDesign" | "PrefixNotWritable";
/**
 * Why a source Canager knows about cannot answer right now. Mirrors
 * `Unavailable` in crates/canager-core/src/model.rs, same bare-string rule
 * as `ReadOnlyReason`: a new Rust variant does not fail this union at
 * compile time, it lands in whatever default branch reads it.
 */
export type Unavailable = "NotRunning" | "NotResponding" | "RefusesAsRoot";
/** Mirrors `InstanceNote`; payload-free on purpose, so a bare string. */
export type InstanceNote = "IndexMayBeStale" | "IndexUpdating";
/**
 * Mirrors `InstanceStatus`, which derives `Default` on the Rust side: this
 * is always an object, never null, and `notes` is `[]` rather than absent
 * when there are none. There is deliberately no per-instance
 * `refreshed_at` -- see the note in the spec's §2.4 for why one would
 * rebroadcast the snapshot on every refresh.
 */
export interface InstanceStatus {
  unavailable: Unavailable | null;
  notes: InstanceNote[];
}
export interface ManagerInstance {
  id: string;
  adapter_id: string;
  exe_path: string;
  prefix: string;
  scope: "User" | "System";
  version: string | null;
  unverified_version: string | null;
  /** `null` means writable; see `canWrite()` in src/lib/sources.ts. */
  read_only_reason: ReadOnlyReason | null;
  /**
   * The state axis, which replaced `healthy: boolean`: the same answer
   * with a reason attached. `isAvailable()` in src/lib/sources.ts is the
   * boolean it used to be.
   */
  status: InstanceStatus;
}
export interface Plan {
  request: OpRequest;
  program: string;
  args: string[];
  env: [string, string][];
  needs_password: boolean;
  locks: string[];
  cancel_policy: "SafeKill" | "KillThenReconcile" | "NoCancel";
  warnings: Warning[];
  affected: string[];
  timeout_secs: number;
}
export interface OpRequest {
  kind: OpKind;
  instance_id: string;
  artifact_kind: ArtifactKind;
  name: string;
}
/**
 * A random 128-bit token (32 hex chars), not a sequential counter -- see
 * `PlanId` in crates/canager-core/src/session/mod.rs. A plan the user
 * previewed and declined used to have a guessable next-in-sequence id that
 * `submit_operation` would still fire; this makes a declined preview
 * unfireable by anything short of a renderer that can call
 * `plan_operation` itself, which a random token cannot defend against
 * either -- it only ever had to raise the bar on a *guess*.
 */
export type PlanId = string;
export interface IssuedPlan {
  id: PlanId;
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
export type Stream = "Stdout" | "Stderr";
// A line of Canager's own in an operation's log (Rust `LogNote`): a key the
// front end localises, never text. `Log` lines are the tool's verbatim words.
// Both variants carry data now (serde's external tagging of a struct
// variant, a one-key object): `minutes` is `BrewAdapter::OP_UPDATE_WAIT`,
// threaded through rather than hard-coded into
// `operations.logNote.waitingForBrewUpdate` so the two can never disagree.
export type LogNote =
  | { WaitingForBrewUpdate: { minutes: number } }
  | { ReadFailed: { stream: Stream; error: string } };
export type OperationEvent =
  | { Status: { op_id: number; status: OpStatus } }
  | { Log: { op_id: number; stream: Stream; line: string } }
  | { Note: { op_id: number; note: LogNote } }
  | { Finished: { op_id: number; outcome: Outcome } };
export type UiEvent = { Operation: OperationEvent } | { SnapshotChanged: { generation: number } };
