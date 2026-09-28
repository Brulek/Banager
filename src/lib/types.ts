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
// reported success -- or, `BackAfterUninstall`, what a path-list
// uninstall's own last look found after it had moved everything. Worded by
// the front end, per variant.
export type Attention =
  | "NotInstalledAfterInstall"
  | "StillInstalledAfterUninstall"
  | "GoneAfterUpgrade"
  | "UnchangedAfterUpgrade"
  | "BackAfterUninstall";
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
  | { PathChanged: { path: string } }
  | "Internal";
// `Failed.summary` is another program's own words, never Canager's: the
// last lines of a tool's stderr, or macOS's own reason for refusing to move
// a path to the Trash (`exit_code` is then `null`: no command ran).
// Canager's own failures are `CanagerFailed`.
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
 * variants. `Pinned` is produced by brew's `parse_info_installed` (from
 * `brew info --installed --json=v2`'s `pinned: true`); `NoSafeMethod` by
 * the standalone adapter's inventory for a tool with no uninstall command
 * and no safe way yet to remove its files (a recipe with no uninstall
 * method: none in the first batch since phase 4 step C gave Claude Code
 * its path list; the second batch's Ollama.app); `UvToolDirSet` by uv's
 * inventory for every tool while `UV_TOOL_DIR` is set in Canager's
 * environment, since removing the last one would also delete the folder
 * above that one. Read through `UNINSTALL_BLOCKED_KEYS` in
 * src/lib/sources.ts, a `Record` over this union, so a variant added here
 * without copy fails `tsc`.
 */
export type UninstallBlocked = "Pinned" | "NoSafeMethod" | "UvToolDirSet";
/**
 * What one path a path-list uninstall moves to the Trash is. Mirrors
 * `RemovedWhat` in crates/canager-core/src/model.rs: bare-string unit
 * variants, the payload of `Warning.WillTrash`. Read through
 * `REMOVED_WHAT_KEYS` in src/lib/warnings.ts, a `Record` over this union,
 * so a variant added here without copy fails `tsc`.
 */
export type RemovedWhat = "Launcher" | "Program" | "Cache" | "Backups";
/**
 * What one path a path-list uninstall leaves alone is. Mirrors `KeptWhat`;
 * read through `KEPT_WHAT_KEYS` in src/lib/warnings.ts.
 */
export type KeptWhat =
  | "Settings"
  | "SettingsAndHistory"
  | "ToolState"
  | "ShellConfigLines"
  | "OutsideHome"
  | "NotOurs"
  | "InstallerCache";
/**
 * Which sentence an uninstall says under the tool about what goes and what
 * stays, the payload of `Warning.UninstallScope`. Mirrors `UninstallScope`
 * in crates/canager-core/src/model.rs: bare-string unit variants. Homebrew's
 * formula sentence says "only" unless a brew.env file brought autoremove
 * back (`HomebrewFormula`, beside `HomebrewAutoremoves`); a cask's is
 * `HomebrewCaskPlain`, `HomebrewCaskSteps` (with a `CaskUninstallStep` per
 * kind of extra step; `HomebrewCaskStepsAutoremoves` when a brew.env file
 * brought autoremove back), `HomebrewCaskStepsOnly` (the same, for a cask whose
 * record lists nothing Homebrew put down: a `pkg` or installer cask) or
 * `HomebrewCask` (its record could not be read, or lists nothing to go by:
 * the sentence claims no deletion).
 * Read through `UNINSTALL_SCOPE_KEYS` in src/lib/warnings.ts, a `Record`
 * over this union, so a variant added here without copy fails `tsc`.
 */
export type UninstallScope =
  | "HomebrewFormulaOnly"
  | "HomebrewFormula"
  | "HomebrewCaskPlain"
  | "HomebrewCaskSteps"
  | "HomebrewCaskStepsAutoremoves"
  | "HomebrewCaskStepsOnly"
  | "HomebrewCask"
  | "Npm"
  | "Pipx"
  | "Uv"
  | "Cargo"
  | "Ollama";
/**
 * One kind of extra step a cask's recorded uninstall takes, the `step` of
 * `Warning.CaskUninstallStep`. Mirrors `CaskStep` in
 * crates/canager-core/src/model.rs: bare-string unit variants, declared in
 * the order the lines are said. Read through `CASK_STEP_KEYS` in
 * src/lib/warnings.ts, a `Record` over this union.
 */
export type CaskStep =
  | "Deletes"
  | "DeletesUnnamed"
  | "Trashes"
  | "RemovesPackages"
  | "RunsScript"
  | "RunsOwnSteps"
  | "RemovesServices"
  | "RemovesKexts"
  | "DeletesCertificates"
  | "RemovesLoginItems"
  | "QuitsApps";
/**
 * The check an uninstall step of type `remove` makes of each path before it
 * deletes it, the `only_if` of a `Deletes` or `DeletesUnnamed`
 * `Warning.CaskUninstallStep`. Mirrors `RemoveCheck` in
 * crates/canager-core/src/model.rs: externally tagged, the text as the
 * record spells it -- only a link whose target contains it, only a file
 * whose contents contain it, or both. Read through `CHECKED_DELETE_KEYS`
 * and `warningArgs` in src/lib/warnings.ts.
 */
export type RemoveCheck =
  | { LinkTargetContains: string }
  | { ContentContains: string }
  | { LinkTargetAndContentContain: { link_target: string; content: string } };
/**
 * A specific warning `Plan` or `UpdateCandidate` carries. Mirrors `Warning`
 * in crates/canager-core/src/model.rs: bare-string unit variants,
 * externally tagged data variants (`WouldBreak`, whose `names` interpolate
 * and pluralise the copy in `src/lib/warnings.ts`, and
 * `ThirdPartyRegistry`, whose `host` interpolates it, and a path-list
 * uninstall's `WillTrash`, `WillKeep` and `AlreadyGone`, whose `path`
 * interpolates it and whose `what` picks the key, and rustup's own
 * uninstall's `RemovesToolchains`, `DeletesCargoHome`,
 * `RemovesCargoInstalled` and `LeavesShellConfigLine`, whose `path` and
 * `names` interpolate it and whose empty `names` or `certain` pick the
 * key -- with `HomebrewRustupLosesToolchains` and `EditsShellConfig` as
 * that uninstall's two bare-string ones), Homebrew's three bare-string
 * brew.env warnings (`HomebrewAutoremoves` on an uninstall, produced only
 * when a brew.env file takes back Canager's `HOMEBREW_NO_AUTOREMOVE=1`;
 * `HomebrewPeriodicCleanup` on an install or upgrade, when one takes back
 * `HOMEBREW_NO_INSTALL_CLEANUP=1`, and `HomebrewCleanupAutoremoves` right
 * after it when one takes back both), an
 * uninstall's one sentence about what goes and what stays
 * (`UninstallScope`, whose `what` picks the key and whose `{{name}}` is the
 * row's, given by the uninstall confirmation) and a cask's extra steps
 * (`CaskUninstallStep`, whose `step` -- with its `only_if`, when a
 * `remove` step checks each path first -- picks the key and whose `items`
 * interpolate it), and a `Message`
 * catch-all for warnings this phase does not localise (spec §6's
 * `show_technical_details` backlog item) -- rendered as the raw string it
 * carries, same as before this type existed. A variant added here without
 * copy fails `tsc` in `warningKey`'s `never` default
 * (src/lib/warnings.ts), the way a `Fault` does in `faultKey`;
 * `types.test.ts` keeps a shape test over all of them.
 */
export type Warning =
  | "DependentsUnknown"
  | { WouldBreak: { names: string[] } }
  | "CompilesLocally"
  | "NonRegistrySource"
  | { ThirdPartyRegistry: { host: string } }
  | { WillTrash: { path: string; what: RemovedWhat } }
  | { WillKeep: { path: string; what: KeptWhat } }
  | { AlreadyGone: { path: string } }
  | { RemovesToolchains: { path: string; names: string[] } }
  | { DeletesCargoHome: { path: string } }
  | { RemovesCargoInstalled: { names: string[] } }
  | "HomebrewRustupLosesToolchains"
  | "EditsShellConfig"
  | { LeavesShellConfigLine: { path: string; certain: boolean } }
  | "HomebrewAutoremoves"
  | "HomebrewPeriodicCleanup"
  | "HomebrewCleanupAutoremoves"
  | { UninstallScope: { what: UninstallScope } }
  | { CaskUninstallStep: { step: CaskStep; items: string[]; only_if?: RemoveCheck } }
  | { Message: string };
/**
 * Why the tool itself will refuse to update this one package, although its
 * source is writable and answering. Mirrors `UpdateBlocked` in
 * crates/canager-core/src/model.rs: bare-string unit variants. `Pinned` is
 * produced by brew's `parse_outdated` (from `brew outdated`'s
 * `pinned: true`) and pipx's (from `pipx list --outdated`'s
 * `name [pinned]:`); `SelfUpdatesOnly` by the standalone adapter's
 * `check_updates` for a tool that installs its updates itself and has no
 * update command Canager may run (Antigravity CLI, phase 4 step D). Read
 * through `UPDATE_BLOCKED_KEYS` in src/lib/sources.ts, a `Record` over
 * this union, so a variant added here without copy fails `tsc` rather
 * than rendering nothing.
 */
export type UpdateBlocked = "Pinned" | "SelfUpdatesOnly";
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
/**
 * Mirrors `InstanceNote` in crates/canager-core/src/model.rs; payload-free
 * on purpose, so a bare string. `sourceNoticesFor` in src/lib/sources.ts
 * ends its loop over these in a `never`, so a variant added here without
 * a branch there fails `tsc`. The last five are a standalone tool's
 * (phase 4): what runs when its name is typed, or that its launcher is
 * left without its program.
 */
export type InstanceNote =
  | "IndexMayBeStale"
  | "IndexUpdating"
  | "NotOnPath"
  | "ShadowedByHomebrew"
  | "ShadowedByNpm"
  | "ShadowedByOther"
  | "LauncherOnly";
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
/**
 * What the user's Cancel does to an operation. Mirrors `CancelPolicy` in
 * crates/canager-core/src/model.rs: bare-string unit variants.
 * `OperationBar.tsx` reads the copy `OpSummary` carries, with its
 * `status`, and offers no Cancel button for a Running `NoCancel` op,
 * which `OperationManager::cancel` would refuse; a Queued one keeps the
 * button, since nothing has started and the cancel is accepted. rustup's
 * `self update` and `self uninstall` produce `NoCancel` (the recipe in
 * crates/canager-core/src/adapters/standalone/recipes.rs); the update
 * confirmation and the uninstall dialog say so under the command
 * (`operations.noCancelHint`) before the click.
 */
export type CancelPolicy = "KillThenReconcile" | "NoCancel";
/**
 * Mirrors `PlanAction` in crates/canager-core/src/model.rs: what a plan
 * does when it runs. Externally tagged single-key objects. `Command` is
 * one program and one argv, spawned by `run_plan`; `TrashPaths` is a
 * path-list uninstall of a tool installed by its own installer, which
 * Canager carries out itself by moving each path to the Trash (phase 4
 * step C) -- no argv exists, so `CommandPreview` shows a sentence for it.
 * `CommandPreview` branches on `"Command" in action` with a `never`
 * default, so a third arm fails `tsc` until it has a preview.
 */
export type PlanAction =
  | { Command: { program: string; args: string[]; env: [string, string][] } }
  | { TrashPaths: { paths: string[] } };
export interface Plan {
  request: OpRequest;
  action: PlanAction;
  needs_password: boolean;
  locks: string[];
  cancel_policy: CancelPolicy;
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
  cancel_policy: CancelPolicy;
}
export interface SourceError {
  instance_id: string;
  message: string;
}
export interface Snapshot {
  generation: number;
  /**
   * The number of the refresh round that committed this snapshot (Rust
   * `Snapshot::round`): 0 before any has, and higher each round, whether
   * or not `generation` moved. The update notification's report names the
   * round it is about by it (`useUpdateNotification`).
   */
  round: number;
  detect: DetectOutcome;
  instances: ManagerInstance[];
  artifacts: InstalledArtifact[];
  updates: UpdateCandidate[];
  refreshed_at: number | null;
  stale: boolean;
  errors: SourceError[];
}
/**
 * Rust `EntryKind` (crates/canager-core/src/scan/mod.rs): what one entry
 * of a scanned bin directory is. Bare-string unit variants. Read through
 * `KIND_KEYS` in src/pages/UnknownPage.tsx, a `Record` over this union, so
 * a variant added here without a badge fails `tsc`.
 */
export type EntryKind = "File" | "Symlink" | "BrokenSymlink";
/**
 * Rust `ScanStop`: why a scan stopped before it had looked at everything.
 * Both variants carry data -- the limit the scan really enforced, so the
 * banner prints that number and never a second copy typed into the
 * locale files -- hence externally tagged single-key objects, like
 * `Fault`'s data variants. The page branches on `"FileLimit" in stopped`
 * with a `never` default (`stoppedText` in src/pages/UnknownPage.tsx).
 */
export type ScanStop = { FileLimit: { max_entries: number } } | { TimeLimit: { max_secs: number } };
/**
 * One directory a scan read and how many entries it examined there.
 * `path` has the home folder abbreviated to `~` on the Rust side: data,
 * not a sentence, and the front end has no `HOME` to strip.
 */
export interface ScannedDir {
  path: string;
  entries: number;
}
/** One program no registered source accounts for. Rust `UnknownEntry`. */
export interface UnknownEntry {
  /** `~`-abbreviated like `ScannedDir.path`; the row's name is its last component. */
  path: string;
  kind: EntryKind;
  /** Canonical and absolute, every link hop followed; `null` for a broken link. The technical detail. */
  resolved: string | null;
  /** `readlink`'s text as the installer wrote it, links only. */
  link_target: string | null;
  /** The target's; `null` for a broken link, which has none. */
  size_bytes: number | null;
  /** Unix seconds, the target's; `null` for a broken link. */
  modified_at: number | null;
  /** `st_uid == euid` of the entry itself: who put it here. */
  owned_by_me: boolean;
  /** The `.app` any component of the path runs inside, without `.app`. */
  app_bundle: string | null;
}
/**
 * Rust `UnknownScan`: the result of one `scan_unknown`. Not part of the
 * `Snapshot` and not written by `refresh`; held only by `useUnknownScan`.
 */
export interface UnknownScan {
  scanned: ScannedDir[];
  entries: UnknownEntry[];
  /** Examined programs a known source accounted for, and so not listed. */
  attributed: number;
  stopped: ScanStop | null;
}
export type Language = "System" | "En" | "ZhCn";
/**
 * One update the user skipped with "Skip this version": `key`'s update to
 * `version`, the `UpdateCandidate.target` its row offered. Mirrors
 * `SkippedVersion` in crates/canager-core/src/settings.rs, whose shape test
 * `types.test.ts` repeats. Hides that update only while the source still
 * offers `version`, and never on a row whose `target` does not name one
 * release (`canSkipVersion`, `hidingRule` in src/lib/updateState.ts); an
 * Ollama model's `version` is a digest, never shown (`shownSkippedVersion`).
 */
export interface SkippedVersion {
  key: ArtifactKey;
  version: string;
}
export interface Settings {
  language: Language;
  show_technical_details: boolean;
  /** "Never remind me": every update of each of these is hidden. */
  ignored_updates: ArtifactKey[];
  skipped_versions: SkippedVersion[];
  include_self_updating: boolean;
  /**
   * The daily check, 「每天自动检查」 in Settings: a refresh by itself once
   * a day while Canager runs (src-tauri/src/auto_check.rs). Off by default.
   */
  auto_check: boolean;
  /**
   * 「有可更新时通知我」 in Settings, which offers it only while
   * `auto_check` is on and turns it off with it. Off by default.
   */
  notify_updates: boolean;
}
/**
 * Rust `UpdatePair` (crates/canager-core/src/notify_updates.rs): one row
 * the Updates page's Update all would take, as the page reports it after
 * each snapshot for the update notification -- the row's key as
 * `artifactKeyId` spells it, and the version the row offers.
 */
export interface UpdatePair {
  key_id: string;
  target: string;
}
export type Stream = "Stdout" | "Stderr";
// A line of Canager's own in an operation's log (Rust `LogNote`): a key the
// front end localises, never text. `Log` lines are the tool's verbatim words.
// Every variant carries data (serde's external tagging of a struct variant,
// a one-key object). Two numbers are threaded through rather than
// hard-coded into the copy, so the two can never disagree: `minutes` is
// `BrewAdapter::OP_UPDATE_WAIT` (`operations.logNote.waitingForBrewUpdate`)
// and `seconds` is `Plan.timeout_secs` (`operations.logNote.outOfTime`).
export type LogNote =
  | { WaitingForBrewUpdate: { minutes: number } }
  | { ReadFailed: { stream: Stream; error: string } }
  | { MovedToTrash: { path: string; trashed_to: string } }
  | { TrashFailed: { path: string; error: string } }
  | { OutOfTime: { path: string; seconds: number } }
  | { BackAfterUninstall: { path: string } };
export type OperationEvent =
  | { Status: { op_id: number; status: OpStatus } }
  | { Log: { op_id: number; stream: Stream; line: string } }
  | { Note: { op_id: number; note: LogNote } }
  | { Finished: { op_id: number; outcome: Outcome } };
export type UiEvent = { Operation: OperationEvent } | { SnapshotChanged: { generation: number } };
