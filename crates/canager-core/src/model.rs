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
    /// or one whose `OLLAMA_HOST` names another machine) -- or was never
    /// asked: an `https://` `OLLAMA_HOST` is refused by `RealHttpClient`'s
    /// allowlist, and `OllamaAdapter::detect` cannot tell that refusal
    /// from a daemon that did not answer (`docs/what-we-run.md`, Ollama).
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
    /// Typing this tool's name in Terminal would not find this copy: no
    /// executable of that name on the `PATH` Canager sees is it (usually
    /// because the directory its launcher lives in is not on that `PATH`).
    /// The name then finds nothing, or another copy; either way this is
    /// the note, not a `ShadowedBy*` one, which would put that copy
    /// earlier on `PATH` than one that is not on it. Produced by
    /// `StandaloneAdapter::detect` (`route::shadow_note`) for a tool
    /// installed by its own installer; read by `sourceNoticesFor` in
    /// src/lib/sources.ts.
    NotOnPath,
    /// Typing the name runs a copy Homebrew installed instead of this one,
    /// and this one is on `PATH` behind it: the first executable of that
    /// name on `PATH` resolves under a `Cellar` or `Caskroom` directory,
    /// and a later one is this copy. Same producer and reader as
    /// `NotOnPath`.
    ShadowedByHomebrew,
    /// As `ShadowedByHomebrew`, for a copy npm installed (it resolves under
    /// a `node_modules` directory).
    ShadowedByNpm,
    /// As `ShadowedByHomebrew`, for a copy Canager does not recognise; the
    /// Unknown page may show where it is.
    ShadowedByOther,
    /// The launcher is still there but points at program files that are
    /// gone: the program directory was removed by hand or by another tool,
    /// or by a Canager uninstall that stopped after moving it and before
    /// moving the launcher -- the removal order (`removal::execute_removal`,
    /// launcher last) makes that the only state a stopped run leaves. The
    /// row stays, with no version, so the state is visible, and its
    /// artifact carries no `uninstall_blocked`: the row's Uninstall lists
    /// the program directory as already gone and moves the link (spec
    /// Q17). Produced by `StandaloneAdapter::detect` when `route::probe`
    /// answers `LauncherOnly`.
    LauncherOnly,
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
    /// `Some` when the tool will refuse to uninstall this package. Read
    /// from the inventory, not the update check, so it is known for every
    /// installed package, up to date or not. `Session::issue_plan` and
    /// `Session::submit` refuse an `Uninstall` of an artifact that carries
    /// one (`blocked_uninstall` in session/plans.rs), and the Installed
    /// page hides that row's Uninstall button.
    pub uninstall_blocked: Option<UninstallBlocked>,
}

/// Why the tool itself will refuse to uninstall this one package, although
/// its source is writable and answering. The uninstall twin of
/// `UpdateBlocked`: same rule for what belongs here (the tool reports the
/// state in the output Canager already reads, here the inventory), and
/// its own type because the two refusals have different producers. pipx
/// pins too, but `pipx uninstall` removes a pinned tool (pipx 1.17.3's
/// `commands/uninstall.py` never reads `pinned`), so pipx is a producer
/// of `UpdateBlocked::Pinned` and not of this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UninstallBlocked {
    /// `brew pin`, for a formula or a cask. Without `--force`, which
    /// Canager never passes, `brew uninstall` prints "Error: <name> is
    /// pinned. You must unpin it to uninstall." and skips it
    /// (`uninstall.rb:48-49`, `cask/uninstall.rb:40-44` in Homebrew 7.0.6).
    /// For a formula it still exits 0, because that message goes through
    /// `onoe`, not `ofail`, so without this Canager ran the command and
    /// then reported `StillInstalledAfterUninstall`. Read by
    /// `parse_info_installed` in `adapters/brew/parse.rs`, from the
    /// `pinned` key `brew info --installed --json=v2` writes for every
    /// formula (`formula.rb:3140`) and cask (`cask/cask.rb:574`).
    Pinned,
    /// The tool has no uninstall command and Canager has no safe way to
    /// remove its files -- no verified list of them, or no way yet to move
    /// them to the Trash -- so it does not offer to. Per artifact, not the
    /// instance's `read_only_reason`: that would hide the upgrade too,
    /// which works. Produced by `StandaloneAdapter::inventory`
    /// (`adapters/standalone/mod.rs`) for a recipe whose `uninstall` is
    /// `None`. No first-batch recipe has one since phase 4 step C gave
    /// Claude Code its path list; the second batch's Ollama.app will (spec
    /// §十), and `NO_UNINSTALL` in that module's tests keeps the path
    /// exercised. The gate refuses it (`blocked_uninstall` in
    /// session/plans.rs), the Installed page hides the button and says why
    /// (`UNINSTALL_BLOCKED_KEYS` in src/lib/sources.ts).
    NoSafeMethod,
}

/// Why a path-list uninstall's preview refused one of the paths its
/// recipe names: which of the checks in `removal::plan_removal`
/// (`adapters/standalone/removal.rs`; phase 4 spec §6.3) failed. Payload
/// of `AdapterError::UninstallUnsafe`, beside the path (home folder
/// abbreviated). Not serialised by serde: `plan_operation_error` in
/// src-tauri/src/ipc.rs spells each reason by hand, in snake_case, and
/// `UNINSTALL_UNSAFE_KEYS` in src/lib/sources.ts indexes the copy by that
/// spelling -- one producer of the wire form, and an exhaustive `match`
/// there, so a reason added here without a spelling fails to compile. The
/// user sees one of six sentences (`planRefused.uninstallUnsafe.*`);
/// nothing was moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UninstallUnsafeReason {
    /// Check 1: the path's parent directory, fully resolved, is not inside
    /// the home folder (a `~/.local/bin` that is a link to another volume).
    OutsideHome,
    /// Check 1's never-list: the path's parent directory, fully resolved,
    /// is the home folder itself or one of the folders directly inside it
    /// that many tools share (`recipe::SHARED_FOLDERS`: `~/.local`,
    /// `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`) -- moving a path
    /// there, `~/.local/bin` say, could take other tools' files with it. A
    /// recipe cannot list such a path (`recipes::tests`); the resolved
    /// check also catches a folder that leads into one through a link.
    SharedFolder,
    /// Check 2: the path is not there, and the list needs it -- it is not
    /// optional, and it is not the already-gone program directory of a
    /// launcher-only install.
    Missing,
    /// Check 3: the path belongs to another user.
    NotOwnedByYou,
    /// Check 4: the path is not the kind of thing the tool's own uninstall
    /// instructions describe -- a launcher that is not one link into the
    /// tool's root, a program directory that is a link, a file where a
    /// directory is expected -- or a folder on its way from the home folder
    /// is a link (the ancestry rule, ruling 24 of the step C plan), or it
    /// could not be examined at all.
    NotWhatInstructionsExpect,
    /// With every link resolved, moving the listed paths might take a path
    /// the preview says is kept: a listed path is a kept path, holds one or
    /// what one leads to, or lies inside one other than where the recipe
    /// lists it (`~/.claude -> ~/.local/share/claude`) -- or a kept path
    /// that is there could not be placed. `path` is the kept path (ruling
    /// 25 of the step C plan).
    OverlapsKept,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Native,
    Registry,
    Digest,
}

/// What one path a path-list uninstall moves to the Trash is, for the
/// sentence that lists it. Payload of `Warning::WillTrash`; produced by
/// `removal::plan_removal` from the recipe's `RemoveSpec.what`, read by
/// `REMOVED_WHAT_KEYS` in src/lib/warnings.ts, a `Record` over the
/// mirror, so a variant added here without copy fails `tsc`. Only the
/// kinds Claude Code's list produces exist in this step; `Backups`
/// (Antigravity's `agy.<time>.old`) arrives with step D.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemovedWhat {
    /// The launcher: the command itself (`~/.local/bin/claude`).
    Launcher,
    /// The program's files (`~/.local/share/claude`).
    Program,
    /// Downloaded files the tool re-creates (`~/.claude/downloads`).
    Cache,
}

/// What one path a path-list uninstall leaves where it is, for the
/// sentence that lists it. Payload of `Warning::WillKeep`; produced by
/// `removal::plan_removal` from the recipe's `KeepSpec.what`, read by
/// `KEPT_WHAT_KEYS` in src/lib/warnings.ts. `ToolState`,
/// `ShellConfigLines`, `OutsideHome` and `NotOurs` arrive with the
/// recipes that produce them (agy and grok, step D).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeptWhat {
    /// A settings file (`~/.claude.json`).
    Settings,
    /// Settings, login, history and working files, shared with other
    /// apps (`~/.claude`, which the tool's editor extensions and desktop
    /// app use too).
    SettingsAndHistory,
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
    /// A path-list uninstall will move this to the Trash: one per path, in
    /// the order they will be moved (the launcher last). `path` has `$HOME`
    /// abbreviated to `~` (`scan::display_path`): data for a sentence, not
    /// a path to act on -- the plan's `PlanAction::TrashPaths.paths` keep
    /// the absolute ones. Produced by `removal::plan_removal`
    /// (`StandaloneAdapter::plan`); read by `warningKey`/`warningArgs` in
    /// src/lib/warnings.ts for the uninstall dialog's list.
    WillTrash { path: String, what: RemovedWhat },
    /// A path-list uninstall will leave this where it is. Same producer and
    /// reader as `WillTrash`; listed only when the path exists.
    WillKeep { path: String, what: KeptWhat },
    /// A path the list names is already gone -- an earlier uninstall
    /// stopped after moving it and before moving the launcher (the
    /// launcher-only state) -- so there is nothing to move there. Said, so
    /// the list adds up. Same producer and reader as `WillTrash`.
    AlreadyGone { path: String },
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

/// What the user's Cancel does to an operation built from this plan.
/// `OperationManager::cancel` (ops/mod.rs) reads it, and the front end
/// reads the copy `OpSummary` carries (`OperationBar.tsx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelPolicy {
    /// Cancel fires the op's token. A command already running is stopped
    /// by the runner and `run_operation` reconciles what is installed
    /// afterwards; one not yet started never starts. Every `Plan` an
    /// adapter builds today says this (pip's `plan()` builds none).
    /// A path-list uninstall (`PlanAction::TrashPaths`) has no process to
    /// stop: `removal::execute_removal` watches the token between items and
    /// stops there -- a move already handed to the system is waited for --
    /// and `run_operation` reads the disk the same way.
    KillThenReconcile,
    /// Cancel is refused once the op is Running, and its command then ends
    /// on its own or at `Plan::timeout_secs`, which the runner counts from
    /// spawn. While the op is still Queued nothing has started and no
    /// timeout is counting, so Cancel is accepted as under
    /// `KillThenReconcile` and the command never starts. No adapter
    /// produces this yet: a standalone self-updating installer (`rustup
    /// self update`) is the expected first.
    NoCancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceLock(pub String); // "brew:/opt/homebrew"

/// What a `Plan` does when it runs. Every plan an adapter built before
/// phase 4 was one program and one argv (`Command`), and `run_plan` is
/// still the only thing that spawns one. The path-list uninstall of a
/// tool installed by its own installer (`StandaloneAdapter`, phase 4
/// step C) runs no command at all: `execute` hands each path to the
/// system's "move to Trash" (`Trasher::trash`) in order -- `TrashPaths`.
/// Two arms rather than an invented argv, because a preview that names a
/// command that will not run is a lie about what is about to happen (spec
/// Q16), and because one `mv` cannot express two paths with the same
/// basename (`~/.local/bin/claude` and `~/.local/share/claude`: `mv -n`
/// skips the second and exits 0), and an item a rename puts in the Trash
/// gets no Finder "Put Back" record (spec §6.2; the Trash spike, which also
/// found that a rename does reach `~/.Trash` without Full Disk Access).
///
/// Readers, each matching both arms: `run_plan` (`adapters/mod.rs`;
/// `Command` only, it refuses the other), `OperationManager::summaries`'s
/// `argv_preview` (`ops/mod.rs`; empty for `TrashPaths`),
/// `CommandPreview.tsx` (one sentence for `TrashPaths`) and the
/// hand-written mirror in `src/lib/types.ts`. Externally tagged on the
/// wire like every other enum here: `{"Command":{"program":…,"args":[…],
/// "env":[…]}}` and `{"TrashPaths":{"paths":[…]}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanAction {
    /// One program, one argv, one environment: what `run_plan` spawns.
    /// `args` is the argv without the program; the preview is `program`
    /// followed by `args`.
    Command {
        program: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// No command. `execute` moves each path to the Trash, in this order
    /// (the tool's launcher last, spec §6.2), after re-checking it. The
    /// paths are absolute; the same paths, `$HOME` abbreviated to `~`,
    /// are the plan's `Warning::WillTrash` items, which is what the dialog
    /// lists. Built only by `StandaloneAdapter::plan` for a recipe whose
    /// `uninstall` is `Uninstall::Paths`.
    TrashPaths {
        paths: Vec<PathBuf>,
        /// What the preview saw at each of `paths`, in the same order
        /// (`removal::plan_removal`): `removal::execute_removal` refuses
        /// with `Fault::PathChanged` when any path is no longer that file,
        /// before anything moves and again right before each move (spec
        /// §6.3). Skipped by serde: the `IssuedPlan` the window receives
        /// carries the paths alone, the TypeScript mirror has no such
        /// field, and a plan read back from JSON has none -- which
        /// `execute_removal` refuses rather than moving what nobody looked
        /// at. It rides in the plan `Session` keeps (`StoredPlan`) and
        /// hands to `OperationManager::submit`, so `Adapter::execute`
        /// reads it with no parameter of its own.
        #[serde(skip)]
        previewed: Vec<ItemIdentity>,
    },
}

/// What kind of file `lstat` found at a path -- a symbolic link is itself,
/// never what it points at. Produced by the path-list uninstall's checks
/// (`removal::identity_of`, adapters/standalone/removal.rs) from the item's
/// last `lstat`; part of an `ItemIdentity`, and what `Trasher::trash` is
/// told about the item it moves, so `RealTrasher` builds its URL from the
/// check made immediately before the call instead of looking again
/// (trash/real.rs). No serde: it never crosses IPC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Dir,
    Symlink,
    /// A socket, a pipe, a device: nothing a recipe lists.
    Other,
}

/// Which file a path named at one moment: `(st_dev, st_ino)` and the kind,
/// from `lstat` -- a link's own, never its target's. A link re-pointed (as
/// `ln -sf` and Claude Code's updater re-point one) or a folder replaced by
/// another of the same name is a new identity. Recorded by
/// `removal::plan_removal` for every path it lists; the preview's travel
/// with the plan (`PlanAction::TrashPaths.previewed`, stage 6e), and
/// `removal::execute_removal` compares them with what is there at the
/// confirmation and again immediately before each move. Server-side only:
/// no serde, and the field that carries it is skipped on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemIdentity {
    pub dev: u64,
    pub ino: u64,
    pub kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub request: OpRequest,
    pub action: PlanAction,
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
    /// the command never started, or an install or uninstall was stopped and
    /// reconcile shows the artifact still absent or still present. An
    /// upgrade stopped partway is never this but `Unconfirmed`
    /// (`run_operation`). A cancel that lost the race to the command
    /// finishing is `Succeeded`, not this.
    Cancelled,
    /// The command reported success but reconcile disagrees. Carries
    /// which disagreement, never a sentence: the front end words it in the
    /// user's language (the drawer and the operation bar both show it).
    NeedsAttention(Attention),
    /// The tool ran and failed. `summary` is the last lines of the tool's
    /// own stderr and nothing else: the front end shows it as-is, quoted
    /// inside a translated sentence, because it is another program's words.
    /// A failure of Canager's own is `CanagerFailed`, never this. A tool a
    /// signal ended before it could exit reported no failure, and is
    /// `Unconfirmed`, never this (`run_plan` in `adapters/mod.rs`).
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
    /// Canager cannot tell what the operation did: the reading after it
    /// failed, or the command did not reach its exit (a Cancel, the
    /// timeout, or a signal Canager did not send -- Activity Monitor,
    /// `kill`, a crash) and what is installed now does not show whether it
    /// took effect. Every upgrade stopped partway ends here, whatever its
    /// version reads (`run_operation` in `ops/mod.rs` says why).
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
    /// A path is not what the preview showed, so Canager stopped and left
    /// it as it is. For a path-list uninstall, a path it was about to move:
    /// at the confirmation, or when its turn came after the moves before
    /// it, it fails one of the preview's checks (a folder on its way became
    /// a link, say, or a kept path now leads into it), the list itself
    /// changed (a path that was absent is there now, or one the preview
    /// listed is gone), or it is no longer the file the preview recorded
    /// (`st_dev`, `st_ino` and the kind): re-pointed -- as a tool that
    /// updates itself re-points its launcher -- or replaced by another of
    /// the same name; or, when the launcher's turn came (the last), another
    /// listed path was there again -- the program folder recreated during a
    /// pause by a copy still running, say -- and `path` is that one, the
    /// launcher left in place so the row stays. Canager stopped without
    /// moving the item whose turn it was; whatever it moved before is in
    /// the Trash, one `LogNote::MovedToTrash` each in the log. For a
    /// standalone tool's upgrade, the launcher the plan
    /// would run: looked at again right before the spawn, it is no longer
    /// the native install's -- gone, dangling, a plain file, or a link
    /// resolving outside the tool's own root (at Homebrew's or npm's copy,
    /// say) -- so the command was not started; a launcher re-pointed at a
    /// newer version inside that root is still the native install's, and
    /// the command runs. `path` has the home folder abbreviated to `~`; it
    /// is the kept path when a kept path is what changed. Built by
    /// `removal::execute_removal` (`adapters/standalone/removal.rs`) and
    /// `StandaloneAdapter::execute` (`adapters/standalone/mod.rs`); read by
    /// `faultKey`/`faultArgs` in src/lib/format.ts.
    PathChanged { path: String },
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
    fn test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent() {
        // `src/lib/types.ts` spells this field `uninstall_blocked:
        // UninstallBlocked | null` and the variant as the bare string
        // "Pinned".
        let artifact = InstalledArtifact {
            key: ArtifactKey {
                instance_id: "brew:/opt/homebrew".to_string(),
                kind: ArtifactKind::Formula,
                name: "glib".to_string(),
            },
            display_name: "glib".to_string(),
            version: "2.88.3".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
        };
        let json = serde_json::to_string(&artifact).expect("serialize");
        assert!(
            json.contains("\"uninstall_blocked\":null"),
            "a removable artifact carries an explicit null, not a missing key: {json}"
        );
        assert_eq!(
            serde_json::from_str::<InstalledArtifact>(&json).expect("deserialize"),
            artifact
        );

        let pinned = InstalledArtifact {
            uninstall_blocked: Some(UninstallBlocked::Pinned),
            ..artifact
        };
        let json = serde_json::to_string(&pinned).expect("serialize");
        assert!(
            json.contains("\"uninstall_blocked\":\"Pinned\""),
            "a reason is a bare string on the wire: {json}"
        );
        assert_eq!(
            serde_json::from_str::<InstalledArtifact>(&json).expect("deserialize"),
            pinned
        );

        // Phase 4: a tool with no uninstall command and no safe way yet
        // to remove its files (Claude Code until step C). Same wire
        // shape, a second spelling for `UNINSTALL_BLOCKED_KEYS` in
        // src/lib/sources.ts.
        let no_safe_method = InstalledArtifact {
            uninstall_blocked: Some(UninstallBlocked::NoSafeMethod),
            ..pinned.clone()
        };
        let json = serde_json::to_string(&no_safe_method).expect("serialize");
        assert!(
            json.contains("\"uninstall_blocked\":\"NoSafeMethod\""),
            "a reason is a bare string on the wire: {json}"
        );
        assert_eq!(
            serde_json::from_str::<InstalledArtifact>(&json).expect("deserialize"),
            no_safe_method
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

        // Phase 4 step C: what a path-list uninstall moves, keeps, and
        // finds already gone. Struct variants carrying a unit enum,
        // spelled as `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS` in
        // src/lib/warnings.ts index them.
        assert_eq!(
            serde_json::to_string(&Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher,
            })
            .unwrap(),
            r#"{"WillTrash":{"path":"~/.local/bin/claude","what":"Launcher"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::WillKeep {
                path: "~/.claude".to_string(),
                what: KeptWhat::SettingsAndHistory,
            })
            .unwrap(),
            r#"{"WillKeep":{"path":"~/.claude","what":"SettingsAndHistory"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string(),
            })
            .unwrap(),
            r#"{"AlreadyGone":{"path":"~/.local/share/claude"}}"#
        );
        for what in [
            RemovedWhat::Launcher,
            RemovedWhat::Program,
            RemovedWhat::Cache,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
        for what in [KeptWhat::Settings, KeptWhat::SettingsAndHistory] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
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
        // Phase 4 step C: a path-list uninstall found a path changed
        // between the preview and the run. `path` has `$HOME` abbreviated.
        assert_eq!(
            serde_json::to_string(&Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            }))
            .unwrap(),
            r#"{"CanagerFailed":{"PathChanged":{"path":"~/.local/bin/claude"}}}"#
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
            action: PlanAction::Command {
                program: PathBuf::from("/opt/homebrew/bin/brew"),
                args: vec![
                    "install".to_string(),
                    "--formula".to_string(),
                    "jq".to_string(),
                ],
                env: vec![("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())],
            },
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

        let trash = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: "standalone-claude".to_string(),
                artifact_kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            },
            action: PlanAction::TrashPaths {
                paths: vec![
                    PathBuf::from("/Users/someone/.local/share/claude"),
                    PathBuf::from("/Users/someone/.local/bin/claude"),
                ],
                previewed: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("standalone-claude".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: vec![],
            affected: vec![],
            timeout_secs: 120,
        };
        let json = serde_json::to_string(&trash).expect("serialize");
        let back: Plan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(trash, back);
    }

    #[test]
    fn test_plan_action_is_externally_tagged_on_the_wire() {
        // `src/lib/types.ts` mirrors `PlanAction` as a union of two
        // single-key objects, and `CommandPreview.tsx` branches on
        // `"Command" in action`; the spellings below are the contract.
        assert_eq!(
            serde_json::to_string(&PlanAction::Command {
                program: PathBuf::from("/opt/homebrew/bin/brew"),
                args: vec!["install".to_string()],
                env: vec![("A".to_string(), "1".to_string())],
            })
            .unwrap(),
            r#"{"Command":{"program":"/opt/homebrew/bin/brew","args":["install"],"env":[["A","1"]]}}"#
        );
        // What the preview saw rides in the plan on this side only
        // (`previewed`, stage 6e of the step C plan): the wire, and so the
        // TypeScript mirror, carries the paths alone; a plan read back has
        // none; and a payload that names the field is not read.
        let trash = PlanAction::TrashPaths {
            paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
            previewed: vec![ItemIdentity {
                dev: 1,
                ino: 2,
                kind: ItemKind::Symlink,
            }],
        };
        let json = serde_json::to_string(&trash).unwrap();
        assert_eq!(
            json,
            r#"{"TrashPaths":{"paths":["/Users/someone/.local/bin/claude"]}}"#
        );
        assert_eq!(
            serde_json::from_str::<PlanAction>(&json).unwrap(),
            PlanAction::TrashPaths {
                paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
                previewed: Vec::new(),
            }
        );
        assert_eq!(
            serde_json::from_str::<PlanAction>(
                r#"{"TrashPaths":{"paths":[],"previewed":[{"dev":1,"ino":2}]}}"#
            )
            .unwrap(),
            PlanAction::TrashPaths {
                paths: Vec::new(),
                previewed: Vec::new(),
            }
        );
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

        // Phase 4's five standalone-installer notes are bare strings too,
        // spelled exactly as `src/lib/types.ts` mirrors them: the front
        // end's `sourceNoticesFor` matches these strings, and a spelling
        // that drifted would fall through every branch and show nothing.
        for (note, wire) in [
            (InstanceNote::NotOnPath, "NotOnPath"),
            (InstanceNote::ShadowedByHomebrew, "ShadowedByHomebrew"),
            (InstanceNote::ShadowedByNpm, "ShadowedByNpm"),
            (InstanceNote::ShadowedByOther, "ShadowedByOther"),
            (InstanceNote::LauncherOnly, "LauncherOnly"),
        ] {
            let status = InstanceStatus {
                unavailable: None,
                notes: vec![note],
            };
            let json = serde_json::to_string(&status).expect("serialize");
            assert_eq!(
                json,
                format!(r#"{{"unavailable":null,"notes":["{wire}"]}}"#)
            );
            assert_eq!(
                serde_json::from_str::<InstanceStatus>(&json).expect("deserialize"),
                status
            );
        }
    }
}
