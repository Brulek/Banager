import type { FailureCause } from "./failureCause";
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
// Rust `Fault`: why Banager itself could not carry an operation out.
// Unit variants are bare strings, data variants single-key objects (serde's
// external tagging). Worded by the front end, per variant; the fields are
// data (a path, the operating system's own reason, the minutes
// `BrewAdapter::OP_UPDATE_WAIT` is), never Banager's prose.
export type Fault =
  | "Panicked"
  | { ProgramMissing: { program: string } }
  | { SpawnFailed: { detail: string } }
  | { HomebrewStillUpdating: { minutes: number } }
  | { PathChanged: { path: string } }
  | "Internal";
// `Failed.summary` is another program's own words, never Banager's: the
// last lines of a tool's stderr, or macOS's own reason for refusing to move
// a path to the Trash (`exit_code` is then `null`: no command ran).
// Banager's own failures are `BanagerFailed`.
export type Outcome =
  | "Succeeded"
  | "Cancelled"
  | "Unconfirmed"
  | { NeedsAttention: Attention }
  | { Failed: { exit_code: number | null; summary: string } }
  | { BanagerFailed: Fault };
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
  facts: ArtifactFacts;
}
/**
 * What Banager knows about an artifact beyond the basics. Mirrors
 * `ArtifactFacts` in crates/banager-core/src/model.rs; every field has an
 * empty value, and `NO_FACTS` (the Rust `Default`) is what an artifact
 * with nothing more to say carries.
 */
export interface ArtifactFacts {
  /** The AI coding tool this is a copy of (`families.rs`), or null. */
  family: string | null;
  /** What Homebrew says about this formula or cask; null for other sources and for a package with nothing to say. */
  homebrew: HomebrewFacts | null;
  /**
   * The commands this artifact puts on the Mac, sorted by name, and which
   * copy runs when each is typed in Terminal (`commands::judge`). Empty
   * when Banager found none, or could not look this round.
   */
  commands: CommandFact[];
  /** Some command ownership paths could not be checked safely. */
  commands_unavailable: boolean;
}
/** Shared by every artifact with nothing more to say: never mutate it. */
export const NO_FACTS: ArtifactFacts = { family: null, homebrew: null, commands: [], commands_unavailable: false };
/**
 * Homebrew's own state for one formula or cask, copied from `brew info
 * --installed --json=v2`. Mirrors `HomebrewFacts` in
 * crates/banager-core/src/model.rs.
 */
export interface HomebrewFacts {
  deprecated: HomebrewLifecycle | null;
  disabled: HomebrewLifecycle | null;
  /** Homebrew's own English notes, verbatim. */
  caveats: string | null;
  /** A formula's other installed versions, in Homebrew's order. */
  other_versions: string[];
}
/**
 * One `deprecate!` / `disable!` mark: the date as Homebrew writes it
 * (`"2026-09-01"`), the reason (a symbol such as `fails_gatekeeper_check`,
 * or the maintainers' own words), and the name Homebrew suggests instead.
 */
export interface HomebrewLifecycle {
  date: string | null;
  reason: string | null;
  replacement: string | null;
}
/**
 * One command an artifact provides. Mirrors `CommandFact` in
 * crates/banager-core/src/model.rs. `state` is null where Banager says
 * nothing about which copy runs: a Homebrew dependency or keg-only
 * formula, a copy it could not place, and every command while the `PATH`
 * it has is not the login shell's.
 */
export interface CommandFact {
  name: string;
  state: CommandState | null;
}
/**
 * What typing a command runs, judged against the `PATH` Banager read when
 * it opened. Mirrors `CommandState` (externally tagged): `ShadowedBy.by` is
 * the artifact whose file comes first on `PATH`, null for one no artifact
 * provides; `NotOnPath.dir` is the folder the command is in, with the home
 * folder as `~`.
 */
export type CommandState = "Runs" | { ShadowedBy: { by: ArtifactKey | null } } | { NotOnPath: { dir: string } };
/**
 * Why the tool itself will refuse to uninstall this one package. Mirrors
 * `UninstallBlocked` in crates/banager-core/src/model.rs: bare-string unit
 * variants. `Pinned` is produced by brew's `parse_info_installed` (from
 * `brew info --installed --json=v2`'s `pinned: true`); `NoSafeMethod` by
 * the standalone adapter's inventory for a tool with no uninstall command
 * and no safe way yet to remove its files (a recipe with no uninstall
 * method: none in the first batch since phase 4 step C gave Claude Code
 * its path list; the second batch's Ollama.app); `UvToolDirSet` by uv's
 * inventory for every tool while `UV_TOOL_DIR` is set in Banager's
 * environment, since removing the last one would also delete the folder
 * above that one; `SourceProgram` by npm's inventory for its own `npm`,
 * the program every npm package is updated and uninstalled with;
 * `NeededBySource` only by `Session::submit`, for a plan whose preview
 * named another source that runs on the package (`Warning.NeededBySource`),
 * so no row carries it. Read through `UNINSTALL_BLOCKED_KEYS` in
 * src/lib/sources.ts, a `Record` over this union, so a variant added here
 * without copy fails `tsc`.
 */
export type UninstallBlocked = "Pinned" | "NoSafeMethod" | "UvToolDirSet" | "SourceProgram" | "NeededBySource";
/**
 * What one path a path-list uninstall moves to the Trash is. Mirrors
 * `RemovedWhat` in crates/banager-core/src/model.rs: bare-string unit
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
 * What a folder an uninstall leaves behind holds (`Warning.KeepsData`, added
 * to an uninstall preview of an AI coding tool by `Session::issue_plan`).
 * Mirrors `KeptData` in crates/banager-core/src/model.rs; read through
 * `KEPT_DATA_KEYS` in src/lib/warnings.ts.
 */
export type KeptData = "ToolData" | "Models";
/**
 * Another tool's data inside a folder an uninstall leaves behind, which the
 * folder's size leaves out (`Warning.KeepsData.others`, absent on the wire
 * when there is none): the path as the table spells it, and the tool's name,
 * spelled the same in both languages. Mirrors `OthersData` in
 * crates/banager-core/src/model.rs.
 */
export interface OthersData {
  path: string;
  tool: string;
}
/**
 * Which sentence an uninstall says under the tool about what goes and what
 * stays, the payload of `Warning.UninstallScope`. Mirrors `UninstallScope`
 * in crates/banager-core/src/model.rs: bare-string unit variants. Homebrew's
 * formula sentence says "only" unless a brew.env file brought autoremove
 * back (`HomebrewFormula`, beside `HomebrewAutoremoves`); a cask's is
 * `HomebrewCaskPlain`, `HomebrewCaskSteps` (with a `CaskUninstallStep` per
 * kind of extra step; `HomebrewCaskStepsAutoremoves` when a brew.env file
 * brought autoremove back), `HomebrewCaskStepsOnly` (the same, for a cask whose
 * record lists nothing Homebrew put down: a `pkg` or installer cask),
 * `HomebrewCaskStepsUnseen` and `HomebrewCaskStepsOnlyUnseen` (the same two,
 * when a step runs a program or code whose deletions Banager cannot see: the
 * sentence says so, and nothing of what stays) or `HomebrewCask` (its record
 * could not be read, or lists nothing to go by: the sentence claims no
 * deletion).
 * Read through `UNINSTALL_SCOPE_KEYS` in src/lib/warnings.ts, a `Record`
 * over this union, so a variant added here without copy fails `tsc`.
 */
export type UninstallScope =
  | "HomebrewFormulaOnly"
  | "HomebrewFormula"
  | "HomebrewCaskPlain"
  | "HomebrewCaskSteps"
  | "HomebrewCaskStepsAutoremoves"
  | "HomebrewCaskStepsUnseen"
  | "HomebrewCaskStepsOnly"
  | "HomebrewCaskStepsOnlyUnseen"
  | "HomebrewCask"
  | "HomebrewCaskPlainThirdParty"
  | "HomebrewCaskRuby"
  | "HomebrewCaskStepsOnlyRuby"
  | "HomebrewCaskPlainRuby"
  | "HomebrewCaskStepsIfTrusted"
  | "HomebrewCaskStepsOnlyIfTrusted"
  | "Npm"
  | "Pipx"
  | "Uv"
  | "Cargo"
  | "Ollama";
/**
 * One kind of extra step a cask's recorded uninstall takes, the `step` of
 * `Warning.CaskUninstallStep`. Mirrors `CaskStep` in
 * crates/banager-core/src/model.rs: bare-string unit variants, declared in
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
  | "QuitsApps"
  | "QuitsNamedApps";
/**
 * The check an uninstall step of type `remove` makes of each path before it
 * deletes it, the `only_if` of a `Deletes` or `DeletesUnnamed`
 * `Warning.CaskUninstallStep`. Mirrors `RemoveCheck` in
 * crates/banager-core/src/model.rs: externally tagged, the text as the
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
 * in crates/banager-core/src/model.rs: bare-string unit variants,
 * externally tagged data variants (`WouldBreak`, whose `names` interpolate
 * and pluralise the copy in `src/lib/warnings.ts`, and
 * `ThirdPartyRegistry`, whose `host` interpolates it, with
 * `DownloadsModelChanges` as an Ollama upgrade's bare-string one, and a path-list
 * uninstall's `WillTrash`, `WillKeep` and `AlreadyGone`, whose `path`
 * interpolates it and whose `what` picks the key, and rustup's own
 * uninstall's `RemovesToolchains`, `DeletesCargoHome`,
 * `RemovesCargoInstalled` and `LeavesShellConfigLine`, whose `path` and
 * `names` interpolate it and whose empty `names` or `certain` pick the
 * key -- with `HomebrewRustupLosesToolchains` and `EditsShellConfig` as
 * that uninstall's two bare-string ones), Homebrew's three bare-string
 * brew.env warnings (`HomebrewAutoremoves` on an uninstall, produced only
 * when a brew.env file takes back Banager's `HOMEBREW_NO_AUTOREMOVE=1`;
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
  /**
   * Another source runs on the Homebrew package an uninstall removes:
   * its program (`program`), or the Python of `tools` of its tools'
   * environments. `instance_id` is that source's. The uninstall
   * confirmation lists it with Homebrew's dependents and offers no
   * Uninstall (`neededBy` in src/lib/neededBy.ts); a batch leaves the
   * package out.
   */
  | { NeededBySource: { instance_id: string; program: boolean; tools: number } }
  | "CompilesLocally"
  | "NonRegistrySource"
  /**
   * On a `checkable: false` candidate, after the `Message` that says why:
   * its lookup failed in a way a later check can get past -- no answer,
   * a 408, 429 or 5xx, or the tool's words name the network. A failed
   * lookup without it is not known to mend itself (a 404, an answer that
   * would not parse, a tool not looked up on this Mac). Only these are
   * counted as "couldn't be checked" (`isFailedLookup`).
   */
  | "TransientLookupFailure"
  /**
   * On a `checkable: false` candidate, after the `Message` that says why
   * (rustls's own words): its lookup reached `host` but could not set up
   * a secure connection there -- a certificate rustls would not accept. Said
   * in a person's words (`warningKey`); never with
   * `TransientLookupFailure`, as the next check meets the same certificate.
   */
  | { SecureConnectionFailed: { host: string } }
  | { ThirdPartyRegistry: { host: string } }
  | "DownloadsModelChanges"
  | { WillTrash: { path: string; what: RemovedWhat } }
  | { WillKeep: { path: string; what: KeptWhat } }
  | { AlreadyGone: { path: string } }
  | { RemovesToolchains: { path: string; names: string[] } }
  | { DeletesCargoHome: { path: string } }
  | { RemovesCargoInstalled: { names: string[] } }
  | "HomebrewRustupLosesToolchains"
  | "EditsShellConfig"
  | { LeavesShellConfigLine: { path: string; certain: boolean } }
  /**
   * A startup file name rustup's preview could not read: it is in, or
   * leads into, a protected place, so whether it keeps a line about Cargo
   * is not known (`ShellConfigUnread` in crates/banager-core/src/model.rs).
   */
  | { ShellConfigUnread: { path: string } }
  | "HomebrewAutoremoves"
  | "HomebrewPeriodicCleanup"
  | "HomebrewCleanupAutoremoves"
  /**
   * The same three, when a brew.env file Homebrew reads is in a protected
   * place, which is not read: whether it takes Banager's switches back is
   * not known, so the preview says what Homebrew may do.
   */
  | "HomebrewMayAutoremove"
  | "HomebrewMayCleanUp"
  | "HomebrewCleanupMayAutoremove"
  /**
   * The formulae `HOMEBREW_NO_CLEANUP_FORMULAE` names, which the lines
   * before it leave out: their older versions (`old_versions`, after
   * `HomebrewPeriodicCleanup`), and they and what they need at run time
   * from the autoremove (`autoremove`, after `HomebrewAutoremoves` or
   * `HomebrewCleanupAutoremoves`).
   */
  | { HomebrewNoCleanupFormulae: { names: string[]; old_versions: boolean; autoremove: boolean } }
  /**
   * `brew uninstall` deletes the entry Homebrew's trust list holds for
   * `name` alone: a cask's full name, a formula's tap and name.
   */
  | { HomebrewForgetsTrust: { name: string } }
  | { UninstallScope: { what: UninstallScope } }
  | { CaskUninstallStep: { step: CaskStep; items: string[]; only_if?: RemoveCheck } }
  /**
   * `left_out`: folders inside `path` its size does not count, not being
   * this tool's data (`~/.codex/packages/standalone`, Codex's own install).
   * `others`: what another tool keeps inside `path`, also not counted
   * (`~/.gemini/antigravity-cli` in `~/.gemini`); off the wire when empty.
   */
  | {
      KeepsData: {
        path: string;
        what: KeptData;
        size: Measured | null;
        left_out: string[];
        others?: OthersData[];
      };
    }
  | { Message: string };
/**
 * Why the tool itself will refuse to update this one package, although its
 * source is writable and answering. Mirrors `UpdateBlocked` in
 * crates/banager-core/src/model.rs: bare-string unit variants. `Pinned` is
 * produced by brew's `parse_outdated` (from `brew outdated`'s
 * `pinned: true`) and pipx's (from `pipx list --outdated`'s
 * `name [pinned]:`); `SelfUpdatesOnly` by the standalone adapter's
 * `check_updates` for a tool that installs its updates itself and has no
 * update command Banager may run (Antigravity CLI, phase 4 step D);
 * `Disabled` by brew's `check_updates` for a formula or cask whose `brew
 * info --installed --json=v2` entry carries Homebrew's `disabled` mark
 * (`facts.homebrew.disabled`), which `brew outdated` still lists but
 * `brew upgrade` will not update. Read through `UPDATE_BLOCKED_KEYS` in
 * src/lib/sources.ts, a `Record` over this union, so a variant added here
 * without copy fails `tsc` rather than rendering nothing.
 */
export type UpdateBlocked = "Pinned" | "SelfUpdatesOnly" | "Disabled";
export interface UpdateCandidate {
  key: ArtifactKey;
  current: string;
  target: string;
  channel: "Native" | "Registry" | "Digest";
  checkable: boolean;
  warnings: Warning[];
  blocked: UpdateBlocked | null;
  /**
   * The most this update can download, in bytes (Rust
   * `UpdateCandidate::download_bytes`): an Ollama model's changed files as
   * the registry sizes them -- an upper bound, since a file another model
   * shares is already on this Mac. `null` for every other source, and for
   * a model whose number Banager could not be sure of. Rust always sends
   * it; optional here only so a test's or the mock's candidate may leave
   * it out, which reads as `null`. Read by src/lib/modelDownload.ts.
   */
  download_bytes?: number | null;
}
/**
 * Why a source can be listed but never changed from Banager. Mirrors
 * `ReadOnlyReason` in crates/banager-core/src/model.rs: bare-string unit
 * variants, so a new Rust variant does *not* fail this union at compile
 * time -- it lands in whatever default branch reads it. `types.test.ts`
 * keeps a shape test over both spellings.
 */
export type ReadOnlyReason = "ByDesign" | "PrefixNotWritable";
/**
 * Why a source Banager knows about cannot answer right now. Mirrors
 * `Unavailable` in crates/banager-core/src/model.rs, same bare-string rule
 * as `ReadOnlyReason`: a new Rust variant does not fail this union at
 * compile time, it lands in whatever default branch reads it.
 */
export type Unavailable = "NotRunning" | "NotResponding" | "RefusesAsRoot" | "HttpsHostRefused" | "NoPip";
/**
 * Mirrors `InstanceNote` in crates/banager-core/src/model.rs; payload-free
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
 * when there are none. When the source last answered is not here but on
 * the instance (`ManagerInstance.answered_at`).
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
  /**
   * Mirrors `ManagerInstance::answered_at`: when this source last answered
   * this session, Unix seconds -- the time its refresh task began asking
   * it, in the latest round in which both its list and its update check
   * answered, so its rows are from then or later; and only while all its
   * rows are still from that answer. `null` otherwise: the first check
   * after launch, a source that has not answered in full since, one whose
   * list answered and update check did not (or the other way round), one
   * Banager has just found. Not written anywhere; a round in which
   * only this moved keeps its `generation`. Said only where a source did
   * not answer (`sourceNoticesFor` in src/lib/sources.ts).
   */
  answered_at: number | null;
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
 * crates/banager-core/src/model.rs: bare-string unit variants.
 * `OperationBar.tsx` reads the copy `OpSummary` carries, with its
 * `status`, and offers no Cancel button for a Running `NoCancel` op,
 * which `OperationManager::cancel` would refuse; a Queued one keeps the
 * button, since nothing has started and the cancel is accepted. rustup's
 * `self update` and `self uninstall` produce `NoCancel` (the recipe in
 * crates/banager-core/src/adapters/standalone/recipes.rs); the update
 * confirmation and the uninstall dialog say so under the command
 * (`operations.noCancelHint`) before the click.
 */
export type CancelPolicy = "KillThenReconcile" | "NoCancel";
/**
 * Mirrors `PlanAction` in crates/banager-core/src/model.rs: what a plan
 * does when it runs. Externally tagged single-key objects. `Command` is
 * one program and one argv, spawned by `run_plan`; `TrashPaths` is a
 * path-list uninstall of a tool installed by its own installer, which
 * Banager carries out itself by moving each path to the Trash (phase 4
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
 * `PlanId` in crates/banager-core/src/session/mod.rs. A plan the user
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
  /**
   * The variables the plan's command is given (Rust `OpSummary::env_preview`,
   * crates/banager-core/src/ops/mod.rs), in its order; empty for a plan that
   * runs no command. Always sent; optional here only so the many operation
   * fixtures that predate it need not spell out an empty list -- a reader
   * takes a missing one as empty (`PasswordCommand.tsx`).
   */
  env_preview?: [string, string][];
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
   * round it is about by it (`useUpdateNotification`), and the snapshot
   * cache keeps whichever of two snapshots has the higher one
   * (`isNewerSnapshot`), never the one whose `refreshed_at` reads later.
   */
  round: number;
  detect: DetectOutcome;
  instances: ManagerInstance[];
  artifacts: InstalledArtifact[];
  updates: UpdateCandidate[];
  refreshed_at: number | null;
  stale: boolean;
  errors: SourceError[];
  /**
   * When the daily check is next due, Unix seconds on the wall clock (Rust
   * `Snapshot::next_auto_check_at`, from `auto_check::next_check_due`): a
   * day after the last check that counted -- the window's own Check again
   * included -- or sooner after failed daily checks. The shell fills it in
   * on every snapshot it hands the window and always sends it; `null`
   * while no check has counted yet (due at the next look). Optional here
   * only so a test's or the mock's snapshot may leave it out, which reads
   * as `null`. Settings shows it under the daily check's switch, while the
   * switch is on (`NextAutoCheck`).
   */
  next_auto_check_at?: number | null;
}
/**
 * Rust `EntryKind` (crates/banager-core/src/scan/mod.rs): what one entry
 * of a scanned bin directory is. Bare-string unit variants. Read through
 * `STATUS_KEYS` in src/pages/UnknownPage.tsx, a `Record` over this union, so
 * a variant added here without an entry there fails `tsc`. `ProtectedSymlink`: a link
 * that leads into a protected place (`~/Documents`, iCloud Drive, `/Volumes`,
 * …), listed by its own name and never followed -- no `resolved`, size or
 * date; the page says 「指向受保护的位置」 in place of a path.
 */
export type EntryKind = "File" | "Symlink" | "BrokenSymlink" | "ProtectedSymlink";
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
  /** Absolute, every link hop followed; `null` for a broken link and for a link into a protected place. The technical detail. */
  resolved: string | null;
  /** `readlink`'s text as the installer wrote it, links only. */
  link_target: string | null;
  /** The target's; `null` for a broken link, which has none, and for a link into a protected place, never looked at. */
  size_bytes: number | null;
  /** Unix seconds, the target's; `null` for a broken link and for a link into a protected place. */
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
  /**
   * The folders to scan that are, or lead into, a protected place, and so
   * were not read: `~`-abbreviated like `ScannedDir.path`, each place once.
   * Rust always sends it (`#[serde(default)]` on its side); optional here
   * only so a test's or the mock's scan may leave it out, which reads as
   * none. The page's 「有N个文件夹在受保护的位置，没有读取。」.
   */
  protected_dirs?: string[];
  entries: UnknownEntry[];
  /** Examined programs a known source accounted for, and so not listed. */
  attributed: number;
  stopped: ScanStop | null;
}
export type Language = "System" | "En" | "ZhCn" | "ZhHant";
/**
 * One update the user skipped with "Skip this version": `key`'s update to
 * `version`, the `UpdateCandidate.target` its row offered. Mirrors
 * `SkippedVersion` in crates/banager-core/src/settings.rs, whose shape test
 * `types.test.ts` repeats. Hides that update only while the source still
 * offers `version`, and never on a row whose `target` does not name one
 * release (`canSkipVersion`, `hidingRule` in src/lib/updateState.ts); an
 * Ollama model's `version` is a digest, never shown (`shownSkippedVersion`).
 */
export interface SkippedVersion {
  key: ArtifactKey;
  version: string;
}
/**
 * One update put off with "Remind Me in 30 Days" (「30天内不提醒」): every
 * update of `key` hidden until `until`, Unix seconds. Mirrors
 * `SnoozedUpdate` in crates/banager-core/src/settings.rs, whose shape test
 * `types.test.ts` repeats. `hidingRule` (src/lib/updateState.ts) hides
 * only while `until` is ahead of the clock; Rust drops one that has run out
 * as it loads the settings.
 */
export interface SnoozedUpdate {
  key: ArtifactKey;
  until: number;
}
/**
 * Rust `CheckEvery` (crates/banager-core/src/settings.rs): how often the
 * automatic check runs while `auto_check` is on -- Settings' 「每天」 or
 * 「每周」. Its 「不自动检查」 is `auto_check` off.
 */
export type CheckEvery = "Day" | "Week";
export interface Settings {
  language: Language;
  show_technical_details: boolean;
  /** "Never remind me": every update of each of these is hidden. */
  ignored_updates: ArtifactKey[];
  skipped_versions: SkippedVersion[];
  include_self_updating: boolean;
  /**
   * The automatic check, on when Settings' 「检查更新」 is 「每天」 or
   * 「每周」 (`auto_check_every`): a refresh by itself once a day or a week
   * while Banager runs (src-tauri/src/auto_check.rs). Off by default.
   */
  auto_check: boolean;
  /**
   * 「有更新时通知我」 in Settings, which offers it only while
   * `auto_check` is on and turns it off with it. Off by default.
   */
  notify_updates: boolean;
  /**
   * How often the automatic check runs while `auto_check` is on. Rust
   * always sends it; optional here, read as "Day" when missing
   * (`autoCheckChoice` in src/lib/checkFrequency.ts), only so
   * that the settings a page or test builds by hand need not spell it --
   * Rust reads a settings.json without it the same way (`#[serde(default)]`).
   */
  auto_check_every?: CheckEvery;
  /**
   * 「操作完成时通知」 in Settings: a notification when a run of operations
   * finishes while the window does not have the focus
   * (src-tauri/src/notify_ops.rs). Off by default. Optional for the reason
   * `auto_check_every` is: missing reads as off, in Rust as here.
   */
  notify_operations?: boolean;
  /**
   * The updates put off for 30 days, one entry a package. Optional for the
   * reason `auto_check_every` is: missing reads as none, in Rust as here.
   */
  snoozed_updates?: SnoozedUpdate[];
  /**
   * Whether the welcome sheet (src/components/WelcomeSheet.tsx) has been
   * shown: false until it is first closed. Rust always sends it, and reads
   * a settings.json without it as false; optional here only so that the
   * settings a page or test builds by hand need not spell it, and the
   * sheet shows only for an explicit false (`welcomeDue`), so those never
   * show it.
   */
  welcome_seen?: boolean;
}
/**
 * Rust `UpdatePair` (crates/banager-core/src/notify_updates.rs): one row
 * the Updates page's Update all would take, as the page reports it after
 * each snapshot for the update notification -- the row's key as
 * `artifactKeyId` spells it, and the version the row offers.
 */
export interface UpdatePair {
  key_id: string;
  target: string;
}
/** Rust `RunKind` (crates/banager-core/src/notify_operations.rs): what a finished run's operations did. */
export type RunKind = "Upgrade" | "Uninstall" | "Other";
/**
 * Rust `FinishedRun` (crates/banager-core/src/notify_operations.rs): a run
 * of operations that has finished, as the page reports it for the
 * notification when operations finish -- its newest operation's id, what
 * they did, and how many worked, failed or need a look. A cancelled one is
 * in none of the three.
 */
export interface FinishedRun {
  last_op: number;
  kind: RunKind;
  succeeded: number;
  failed: number;
  attention: number;
}
export type Stream = "Stdout" | "Stderr";
// A line of Banager's own in an operation's log (Rust `LogNote`): a key the
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
/**
 * Rust `InventoryPreview` (crates/banager-core/src/session/mod.rs): what
 * the first refresh round since launch found installed, sent before its
 * update checks are done (`UiEvent.InventoryPreview`). Never a snapshot:
 * nothing was committed, and it says nothing about updates, errors or
 * staleness. Held apart from the snapshot cache, for the Installed page
 * alone, while the cache has only the startup placeholder
 * (`writeInventoryPreview` in src/lib/events.ts, `useInventoryPreview` in
 * src/lib/inventoryPreview.ts).
 */
export interface InventoryPreview {
  /** The round still running, whose snapshot will carry this number. */
  round: number;
  /** Every source the round detected, before any check added a note. */
  instances: ManagerInstance[];
  /** What each source that read its list listed; one whose read failed is absent. */
  artifacts: InstalledArtifact[];
}
/**
 * How much one thing takes on disk. Mirrors `Measured` in
 * crates/banager-core/src/size.rs: `bytes` is what the disk holds for it,
 * each hard-linked file once; `partial` when part of it could not be read
 * or went away while it was measured, `at_least` when the round's budget
 * ran out first -- both mean it takes more.
 */
export interface Measured {
  bytes: number;
  partial: boolean;
  at_least: boolean;
}
/** Rust `ArtifactSize`: one installed thing's size, by its key. */
export interface ArtifactSize {
  key: ArtifactKey;
  /** The version it was measured at; shown only beside that version. */
  version: string;
  /** Null while it is still being measured. */
  measured: Measured | null;
  /** A Homebrew formula's other kegs, together; null when it has none. */
  old_versions: Measured | null;
}
/**
 * Rust `SourceSize`: everything measured of one source together -- its
 * tools, a formula's other versions, an Ollama's models folder -- a file
 * with several hard links once.
 */
export interface SourceSize {
  instance_id: string;
  measured: Measured;
}
/** Rust `ModelsSize`: one Ollama's models, as the folder they are in. */
export interface ModelsSize {
  instance_id: string;
  measured: Measured | null;
}
/**
 * Rust `Sizes` (crates/banager-core/src/size.rs): what the newest round of
 * measuring says so far, from `get_sizes`. Not part of the `Snapshot`; an
 * artifact it does not list has no size to show.
 */
export interface Sizes {
  round: number;
  done: boolean;
  artifacts: ArtifactSize[];
  models: ModelsSize[];
  total: Measured | null;
  /** `total`, one source at a time; empty until `done`. */
  sources: SourceSize[];
}
/** `Sizes::default()`: before any round, and nothing to show. */
export const NO_SIZES: Sizes = { round: 0, done: false, artifacts: [], models: [], total: null, sources: [] };
/** Rust `HistoryKind` (crates/banager-core/src/history/mod.rs). */
export type HistoryKind = "Update" | "Uninstall";
/**
 * Rust `HistoryResult`: how a kept operation ended, as a category --
 * `Outcome` without the programs' words or Banager's paths. A failure's
 * cause is the word `failureCause` would have read off the tool's lines
 * (src/lib/failureCause.ts), or null.
 */
export type HistoryResult =
  | "Succeeded"
  | "Unconfirmed"
  | "Cancelled"
  | { NeedsAttention: Attention }
  | { Failed: { cause: FailureCause | null } };
/**
 * Rust `HistoryRecord`: one finished update or uninstall, kept in
 * `history.json` across launches. `finished_at` is in milliseconds.
 * `verified`: Banager read the installed version before the update and
 * after it, and the two differ (for an uninstall: the reading after found
 * it gone).
 */
export interface HistoryRecord {
  run: string;
  op_id: number;
  finished_at: number;
  key: ArtifactKey;
  display_name: string;
  adapter_id: string;
  kind: HistoryKind;
  from_version: string | null;
  to_version: string | null;
  result: HistoryResult;
  verified: boolean;
}
/**
 * Rust `HistoryView`, from `get_history` and `clear_history`: this launch's
 * id, when the Updates page's Clear was last pressed (milliseconds), and
 * every record, newest first.
 */
export interface HistoryView {
  run: string;
  cleared_before: number | null;
  records: HistoryRecord[];
}
/** No history: before `get_history` answers, or a command that answered nothing. */
export const NO_HISTORY: HistoryView = { run: "", cleared_before: null, records: [] };
export type UiEvent =
  | { Operation: OperationEvent }
  | { SnapshotChanged: { generation: number } }
  | { InventoryPreview: InventoryPreview }
  | { SizesChanged: { round: number } };
/**
 * What the window cannot read itself for 「拷贝诊断信息」 and 「检查工具环境」 (`get_system_facts`
 * in src-tauri/src/ipc.rs). Mirrors `SystemFacts` in
 * crates/banager-core/src/diagnostics.rs: every path with the home folder
 * as `~`, and no environment variable's value but the `PATH` folders.
 */
export interface SystemFacts {
  /** "27.0"; null where the kernel would not say. */
  macos_version: string | null;
  /** "Apple M2 Pro"; null where the kernel would not say. */
  chip: string | null;
  /** What Banager was built for: "aarch64" or "x86_64". */
  arch: string;
  /** Whether `PATH` is the login shell's. */
  login_path: boolean;
  /** The `PATH` folders, in order, home folder as `~`. */
  path_dirs: string[];
  /** Each source's program, home folder as `~`, by instance id. */
  sources: SourcePath[];
  /**
   * What the last refresh round made of the `PATH` folders when it read
   * them to say which copy of a command runs (`Session::path_folders`):
   * null before a round has, and after one that did not read them in full.
   * Always sent; optional only so the facts in tests that predate it need
   * not spell it out -- a reader takes a missing one as null.
   */
  path_folders?: PathFolders | null;
}
/**
 * Rust `PathFolders` (crates/banager-core/src/diagnostics.rs): how many of
 * the `PATH` folders the last round read, and those it left unread -- in a
 * protected place, or not listable -- as named, home folder as `~`.
 */
export interface PathFolders {
  read: number;
  unread: string[];
}
/** Rust `SourcePath`: one source's program, as `SystemFacts.sources` lists it. */
export interface SourcePath {
  instance_id: string;
  exe_path: string;
}
