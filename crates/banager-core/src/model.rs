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
/// `ArtifactKey`s of `Settings.ignored_updates` and
/// `Settings.skipped_versions`, so changing their shape would silently
/// bring back every update the user had hidden.
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

/// Why this source can be listed but never changed from Banager.
///
/// An enum rather than a string because these reasons are shown to the
/// user, and an English sentence assembled on the Rust side cannot be
/// localised -- the existing `UpdateCandidate.warnings` already fell into
/// that trap (spec §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadOnlyReason {
    /// The tool itself offers no install/uninstall path Banager could
    /// safely drive (pip).
    ByDesign,
    /// The tool can install and uninstall, but the directory it writes to
    /// is not writable by the current user (Node installed from the
    /// nodejs.org package, whose npm prefix is root-owned).
    PrefixNotWritable,
}

/// Why a source Banager knows about cannot answer right now. The state
/// axis, orthogonal to `ReadOnlyReason`: an Ollama that is not running is
/// still perfectly writable, it just has nothing to say until it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unavailable {
    /// The service is not running and Banager can start it: the notice
    /// carries a button that does. Today that is only an Ollama whose
    /// daemon is on this Mac and whose Ollama.app is installed -- see
    /// `OllamaAdapter::detect`, which gives a silent daemon it cannot start
    /// `NotResponding` instead.
    NotRunning,
    /// The executable is on PATH but would not run, or its version could
    /// not be recognised, or a service did not answer and Banager has no
    /// way to start it (an Ollama installed as the command-line tool only,
    /// or one whose `OLLAMA_HOST` names another machine) -- or was never
    /// asked: an `https://` `OLLAMA_HOST` is refused by `RealHttpClient`'s
    /// allowlist, and `OllamaAdapter::detect` cannot tell that refusal
    /// from a daemon that did not answer (`docs/what-we-run.md`, Ollama).
    NotResponding,
    /// The tool is installed but refuses to do anything while Banager is
    /// running as root, so Banager never even asked it (Homebrew).
    ///
    /// A third variant rather than a reuse of `NotResponding` because
    /// these divide by *what the user can do about it*: `NotRunning` means
    /// "start it", `NotResponding` means "reopen Banager, then consider
    /// reinstalling", and this one means "quit and open Banager again
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
    /// executable of that name on the `PATH` Banager sees is it (usually
    /// because the directory its launcher lives in is not on that `PATH`).
    /// The name then finds nothing, or another program with that name;
    /// either way this is the note, not a `ShadowedBy*` one, which would
    /// put that program earlier on `PATH` than a copy that is not on it.
    /// Produced by `StandaloneAdapter::detect` (`route::shadow_note`) for a
    /// tool installed by its own installer; read by `sourceNoticesFor` in
    /// src/lib/sources.ts.
    NotOnPath,
    /// Typing the name runs another program with that name instead of this
    /// copy, and this copy is on `PATH` behind it: the first executable of
    /// that name on `PATH` resolves under a `Cellar` or `Caskroom`
    /// directory (Homebrew's), and a later one is this copy. Where it
    /// resolves is all the note says: it may be another copy of the tool
    /// or a different program with the same name (Homebrew's formula
    /// `grok` is a regular-expression tool, not Grok Build), so its notice
    /// never calls it a copy. Same producer and reader as `NotOnPath`.
    ShadowedByHomebrew,
    /// As `ShadowedByHomebrew`, for one that resolves under a
    /// `node_modules` directory (npm's; the `grok` of its package
    /// `grok-cli`, a third-party wrapper, resolves there and is not Grok
    /// Build).
    ShadowedByNpm,
    /// As `ShadowedByHomebrew`, for one that resolves anywhere else, or
    /// that Banager could not resolve; the Unknown page may show where it
    /// is.
    ShadowedByOther,
    /// The launcher is still there but points at program files that are
    /// gone: the program directory was removed by hand or by another tool,
    /// or by a Banager uninstall that stopped after moving it and before
    /// moving the launcher -- the removal order (`removal::execute_removal`,
    /// launcher last) makes that the only state a stopped run leaves. The
    /// row stays, with no version, so the state is visible, and its
    /// artifact carries no `uninstall_blocked`: the row's Uninstall lists
    /// the program directory as already gone and moves the link (spec
    /// Q17). Produced by `StandaloneAdapter::detect` when `route::probe`
    /// answers `LauncherOnly`.
    LauncherOnly,
}

impl InstanceNote {
    /// Whether this note is the round's update check speaking about the
    /// source -- `CheckOutcome::notes` (brew's `IndexMayBeStale`) or the
    /// `IndexUpdating` that `Session::refresh_round` adds when a read
    /// declined -- rather than `detect` (`StandaloneAdapter::detect`'s five
    /// placement notes). Read by `refresh_round` when it skips an adapter's
    /// detect because an operation holds one of its instances: the
    /// instances it carries from last round that are not themselves held
    /// are still checked this round, so last round's check notes come off
    /// them first, and detect's stay, since detect did not run to write
    /// them again. An exhaustive match, so a new variant has to say which
    /// channel it comes from.
    pub(crate) fn is_from_update_check(self) -> bool {
        match self {
            InstanceNote::IndexMayBeStale | InstanceNote::IndexUpdating => true,
            InstanceNote::NotOnPath
            | InstanceNote::ShadowedByHomebrew
            | InstanceNote::ShadowedByNpm
            | InstanceNote::ShadowedByOther
            | InstanceNote::LauncherOnly => false,
        }
    }
}

/// The state axis of a source: can Banager talk to it at all, and is there
/// anything about this answer the user has to know to read it correctly.
///
/// Deliberately *without* a per-instance `refreshed_at` (spec §2.4's note):
/// `Snapshot::same_content` compares `instances` with the derived
/// `PartialEq`, so a unix second that moves every refresh would bump the
/// generation and rebroadcast `SnapshotChanged` on every poll, and the one
/// relative time `src/` renders -- the page header's "Checked 3 min ago" --
/// reads the snapshot's own `refreshed_at`.
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
    /// Whether Banager may offer operations on this source at all.
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
    /// What Banager knows about this artifact beyond the basics above. Its
    /// own struct so that a new fact is one field here and one default,
    /// not a new line in every inventory that builds an artifact.
    pub facts: ArtifactFacts,
}

/// Facts about an installed artifact that only some sources report or that
/// Banager works out itself after the inventory. Every field has an empty
/// default, which is what an inventory that knows nothing more leaves, and
/// `#[serde(default)]` keeps a payload without a newer field readable.
/// Mirrored by `ArtifactFacts` in src/lib/types.ts, whose `NO_FACTS` is
/// this type's `Default`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtifactFacts {
    /// The id of the AI coding tool this artifact is a copy of, from the
    /// bundled table in `families.rs` (for example `"claude-code"` for the
    /// npm package `@anthropic-ai/claude-code`, the cask `claude-code` and
    /// the standalone install alike). `None` for everything else.
    pub family: Option<String>,
    /// What Homebrew says about this formula or cask beyond its version:
    /// read by `parse_info_installed` (`adapters/brew/parse.rs`) from the
    /// `brew info --installed --json=v2` reply the inventory already
    /// fetches. `None` for every other source, and for a Homebrew package
    /// with nothing of the kind to say.
    pub homebrew: Option<HomebrewFacts>,
    /// The commands this artifact puts on the Mac, by name, and which copy
    /// runs when the user types each one in Terminal (`CommandFact`).
    /// Worked out after the inventory, once per refresh round, from the
    /// whole snapshot and the `PATH` Banager read at launch
    /// (`commands::judge`, through `Session::refresh`); sorted by name. A
    /// row carried from an earlier round keeps the ones it had
    /// (`commands::finish`); an inventory never fills this. Empty when the
    /// artifact provides no command Banager could find, for the rows of a
    /// round that could not read the folders (`commands::CommandBudget`),
    /// and for sources whose commands Banager does not look for (Ollama
    /// models, pip).
    pub commands: Vec<CommandFact>,
    /// What the inventory read about this artifact's commands, for
    /// `commands::judge`: never on the wire (the window has `commands`,
    /// which is the answer), so not in the TypeScript mirror either.
    #[serde(skip)]
    pub command_inputs: CommandInputs,
}

/// Homebrew's own state for one installed formula or cask. Every field is
/// copied from `brew info --installed --json=v2`; Banager adds no judgement
/// of its own. Mirrored by `HomebrewFacts` in src/lib/types.ts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HomebrewFacts {
    /// `deprecated: true`: Homebrew still installs and updates it but says
    /// it will go.
    pub deprecated: Option<HomebrewLifecycle>,
    /// `disabled: true`: Homebrew no longer installs or updates it. The
    /// copy already installed stays where it is.
    pub disabled: Option<HomebrewLifecycle>,
    /// `caveats`, Homebrew's own English notes, verbatim (they often hold
    /// shell lines, so the page shows them as text and offers no copy).
    pub caveats: Option<String>,
    /// A formula's other installed versions: its `installed` entries other
    /// than the one Banager shows, in Homebrew's order. Empty for a cask.
    pub other_versions: Vec<String>,
}

/// One of Homebrew's lifecycle marks (`deprecate!` / `disable!`), as its
/// JSON gives it: the date as Homebrew writes it (`"2026-09-01"`), the
/// reason (a known symbol such as `"fails_gatekeeper_check"` or the
/// maintainers' own sentence), and the name of the formula or cask
/// Homebrew suggests instead.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HomebrewLifecycle {
    pub date: Option<String>,
    pub reason: Option<String>,
    pub replacement: Option<String>,
}

/// One command an artifact provides: the name typed in Terminal, and what
/// typing it runs. Payload of `ArtifactFacts.commands`; mirrored by
/// `CommandFact` in src/lib/types.ts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandFact {
    /// `claude`, `grok`, `agent`, `ruff`.
    pub name: String,
    /// `None`: Banager says nothing about which copy runs -- for a Homebrew
    /// dependency or keg-only formula, whose commands are left off `PATH`
    /// on purpose or were never asked for; for a copy nothing on `PATH`
    /// leads to although its folder is on `PATH` (its link there replaced
    /// by another tool's), or whose folder Banager does not know (a pipx
    /// app with no link in `~/.local/bin`); and for every command while
    /// the `PATH` Banager has is not the login shell's
    /// (`Session::note_login_path`). The name is still listed, so two
    /// copies of one tool can be told apart from one. A file that cannot
    /// run (no execute bit) is no command at all and is not listed.
    pub state: Option<CommandState>,
}

/// What typing a command runs, judged against the `PATH` Banager read when
/// it opened, the way a shell looks a name up: the first folder on `PATH`
/// holding an executable file of that name wins (`commands::judge`).
/// Externally tagged on the wire: `"Runs"`, `{"ShadowedBy":{"by":…}}`,
/// `{"NotOnPath":{"dir":"~/.local/bin"}}`; mirrored by `CommandState` in
/// src/lib/types.ts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandState {
    /// The first executable of this name on `PATH` is this copy: a link
    /// that leads to the same file, or another of this artifact's files.
    Runs,
    /// This copy is on `PATH`, behind another executable of the same name
    /// that comes first. `by` is the artifact that one belongs to, when
    /// some artifact in the snapshot provides that very file; `None` when
    /// none does ("another program with this name").
    ShadowedBy { by: Option<ArtifactKey> },
    /// Nothing on `PATH` leads to this copy, and the folder its command is
    /// in is not on `PATH`. `dir` is that folder with the home folder as
    /// `~` (`scan::display_path`): text the page shows and its Copy Path
    /// copies, as the Other Programs page's paths are.
    NotOnPath { dir: String },
}

/// What an inventory read about an artifact's commands: the input
/// `commands::judge` turns into `ArtifactFacts.commands`. Not on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandInputs {
    /// The commands the source's own answer names: a Homebrew cask's
    /// `binary` stanzas (`brew/parse.rs`), a pipx tool's `app_paths`, the
    /// `- name (path)` lines of `uv tool list --show-paths`, the `bins` of
    /// Cargo's `.crates2.json`. Empty for the sources whose commands
    /// `commands::judge` finds itself: a Homebrew formula's and an npm
    /// package's links in their prefix's `bin`, and a tool with its own
    /// installer's launcher and the commands its recipe names.
    pub provided: Vec<ProvidedCommand>,
    /// Homebrew's `keg_only` for a formula: Homebrew keeps it out of its
    /// `bin` folder on purpose (macOS has its own `curl`), so no judgement
    /// is made about its commands, even when it was linked by hand.
    pub keg_only: bool,
}

/// One command a source's own answer names (`CommandInputs.provided`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvidedCommand {
    /// The name typed in Terminal.
    pub name: String,
    /// The file the source says the command is: a link in a `bin` folder
    /// (`/opt/homebrew/bin/grok`, `~/.local/bin/ruff`), or, for pipx, the
    /// program in the tool's own environment (`<venv>/bin/black`).
    pub path: PathBuf,
    /// Where `path` has to lead, every link followed, to be this
    /// artifact's: the uv tool's environment, a cask binary's own file.
    /// Empty when the source vouches for the file wherever it is. Links
    /// are followed and compared because two sources can name one path --
    /// pipx and uv both put `ruff` in `~/.local/bin`, and only one of them
    /// installed the file that is there now.
    pub within: Vec<PathBuf>,
}

/// Why the tool itself will refuse to uninstall this one package, although
/// its source is writable and answering. The uninstall twin of
/// `UpdateBlocked`: same rule for what belongs here (the tool reports the
/// state in the output Banager already reads, here the inventory), and
/// its own type because the two refusals have different producers. pipx
/// pins too, but `pipx uninstall` removes a pinned tool (pipx 1.17.3's
/// `commands/uninstall.py` never reads `pinned`), so pipx is a producer
/// of `UpdateBlocked::Pinned` and not of this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UninstallBlocked {
    /// `brew pin`, for a formula or a cask. Without `--force`, which
    /// Banager never passes, `brew uninstall` prints "Error: <name> is
    /// pinned. You must unpin it to uninstall." and skips it
    /// (`uninstall.rb:48-49`, `cask/uninstall.rb:40-44` in Homebrew 7.0.6).
    /// For a formula it still exits 0, because that message goes through
    /// `onoe`, not `ofail`, so without this Banager ran the command and
    /// then reported `StillInstalledAfterUninstall`. Read by
    /// `parse_info_installed` in `adapters/brew/parse.rs`, from the
    /// `pinned` key `brew info --installed --json=v2` writes for every
    /// formula (`formula.rb:3140`) and cask (`cask/cask.rb:574`).
    Pinned,
    /// The tool has no uninstall command and Banager has no safe way to
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
    /// `UV_TOOL_DIR` is set, and not empty, in Banager's environment, which
    /// every `uv` command inherits, so uv keeps its tools there
    /// (`InstalledTools::from_settings`, uv 0.12.17
    /// `crates/uv-tool/src/lib.rs:132-140`). When `uv tool uninstall`
    /// removes the last tool it deletes that folder, and then its parent,
    /// with every file in it, when the parent holds no folder but `.tmp*`
    /// ones (`crates/uv/src/commands/tool/uninstall.rs:40-52`,
    /// `crates/uv-fs/src/lib.rs:795-815`). In uv's own layout that parent
    /// is uv's data folder; under `UV_TOOL_DIR` it is whatever folder holds
    /// the user's, so Banager uninstalls no uv tool then. Produced by
    /// `UvAdapter::inventory` for every tool, and refused by
    /// `UvAdapter::plan` as well; the gate refuses it (`blocked_uninstall`
    /// in session/plans.rs), and the Installed page hides the button and
    /// says why (`UNINSTALL_BLOCKED_KEYS` in src/lib/sources.ts).
    UvToolDirSet,
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
    /// Check 4: the path is not the kind of thing the tool's uninstall list
    /// describes (its `Expect`) -- a launcher that is not one link into the
    /// tool's root, a program directory that is a link, a file where a
    /// directory is expected -- or a folder on its way from the home folder
    /// is a link (the ancestry rule, ruling 24 of the step C plan), or it
    /// could not be examined at all. The name is from step C, when Claude
    /// Code's list, built from Anthropic's removal steps, was the only one;
    /// Antigravity CLI and Grok Build publish no removal steps, so their
    /// lists are Banager's own reading of how each was installed, and the
    /// sentence the user reads (`notWhatInstructionsExpect`) cites no
    /// instructions.
    NotWhatInstructionsExpect,
    /// Moving the listed paths might take a path the preview says is kept,
    /// or part of the way to what one leads to: with every link resolved,
    /// a listed path is a kept path, holds one or what one leads to, is or
    /// holds a link or folder on the way from one to what it leads to
    /// (`~/.claude.json -> ~/.local/share/claude/settings-link ->
    /// ~/settings/claude.json`), or lies inside one other than where the
    /// recipe lists it (`~/.claude -> ~/.local/share/claude`) -- or a kept
    /// path that is there could not be placed, or the way from it to what
    /// it leads to could not be followed. `path` is the kept path (ruling
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
/// `removal::plan_removal` from the recipe's `RemoveSpec.what`, or from a
/// `Glob.what` for a backup file (`removal::listed_items`, check 5), read
/// by `REMOVED_WHAT_KEYS` in src/lib/warnings.ts, a `Record` over the
/// mirror, so a variant added here without copy fails `tsc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemovedWhat {
    /// The launcher: the command itself (`~/.local/bin/claude`;
    /// `~/.local/bin/agy`, which is the whole program; grok's
    /// `~/.grok/bin/grok` and `~/.grok/bin/agent`, two links to one
    /// download, and its optional fallback links in `~/.local/bin`).
    Launcher,
    /// The program's files (`~/.local/share/claude`, `~/.grok/downloads`).
    Program,
    /// Downloaded files the tool re-creates (`~/.claude/downloads`).
    Cache,
    /// A backup copy the tool's own updater left beside its launcher
    /// (`~/.local/bin/agy.<time>.old`, agy.md/spec §3.5), found through the
    /// recipe's `backup_globs`.
    Backups,
}

/// What one path a path-list uninstall leaves where it is, for the
/// sentence that lists it. Payload of `Warning::WillKeep`; produced by
/// `removal::plan_removal` from the recipe's `KeepSpec.what` (through
/// `kept_places` and, for `OutsideHome`, `outside_home_keeps`) and from its
/// optional-path skip (`NotOurs`); read by `KEPT_WHAT_KEYS` in
/// src/lib/warnings.ts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeptWhat {
    /// A settings file (`~/.claude.json`).
    Settings,
    /// Settings, login, history and working files, shared with other
    /// apps (`~/.claude`, which the tool's editor extensions and desktop
    /// app use too; `~/.grok`, with `config.toml`, `auth.json`, sessions
    /// and memory).
    SettingsAndHistory,
    /// The tool's own root, where its conversations, history and working
    /// files sit beside some of the program's own files, with no vendor
    /// list saying which could go alone (`~/.gemini/antigravity-cli`;
    /// spec §十三 #24).
    ToolState,
    /// A shell startup file the installer added lines to (`~/.zshrc`,
    /// `~/.zprofile`): Banager never edits one (spec §6.8), and does not
    /// read it to find the lines, so the sentence says "any lines".
    ShellConfigLines,
    /// A link outside the home folder the installer may have made into the
    /// tool's root (`/usr/local/bin/grok`): never touched, reported so the
    /// user knows it becomes a dead link. Reported only when it is a link
    /// into the root that leads nowhere once the uninstall's moves are done
    /// (`removal::dead_after`): a `/usr/local/bin/grok` that is Homebrew's,
    /// an `agent` that is another CLI's, or a link to a plugin's program in
    /// the `~/.grok` the uninstall keeps, gets no sentence. Report-only:
    /// `kept_places` does not protect it (a link into the program folder
    /// would otherwise refuse the uninstall it exists for).
    OutsideHome,
    /// An optional listed path that is there but Banager could not confirm
    /// is this install's -- the wrong shape, a link elsewhere, a folder on
    /// the way that is a link, or a place Banager never moves from
    /// (`~/.local/bin/agent` when another CLI owns it; spec §十三 #27) --
    /// so it stays and the uninstall goes on.
    NotOurs,
    /// The installer's download staging folder, directly in `~/.cache`
    /// (`~/.cache/antigravity`): Banager moves nothing that sits directly in
    /// a shared folder (check 1's never-list), so it stays, usually empty,
    /// and the user may delete it (phase 4 step D plan, ruling 1).
    InstallerCache,
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
/// not localise: text built at runtime from something Banager cannot know
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
    /// registry Banager checks for updates against.
    NonRegistrySource,
    /// Installing or upgrading this model downloads it from `host`, a
    /// registry other than Ollama's own library. Carried only on
    /// Install/Upgrade plans: where a model came from is a reason to look
    /// twice before fetching it, and no reason at all to hesitate before
    /// deleting it.
    ThirdPartyRegistry { host: String },
    /// Upgrading this model pulls it again (`ollama pull`), which fetches
    /// every layer of the model's current manifest that is not already on
    /// this Mac -- the files that changed since it was pulled, weights of
    /// several gigabytes when those changed -- so it can take a while. The
    /// model's note in the update confirmation, as `CompilesLocally` is a
    /// crate's. Carried only on Upgrade plans, after `ThirdPartyRegistry`
    /// when there is one: an Install downloads the whole model, which the
    /// person asked for, and an Uninstall downloads nothing. Produced by
    /// `OllamaAdapter::plan`; read by `warningKey` in src/lib/warnings.ts.
    DownloadsModelChanges,
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
    /// rustup's `self uninstall` deletes `path` (`$RUSTUP_HOME`, spelled
    /// `~/.rustup`; the standard layout is the only one Banager offers
    /// the uninstall for, `rustup::standard_roots`) permanently -- not
    /// to the Trash -- with every toolchain in it: `names` are the entry
    /// names of its `toolchains/` directory when the preview was built,
    /// empty when that directory is missing, empty or unreadable (the
    /// front end then says "every toolchain" without naming them).
    /// Produced by the rustup recipe's uninstall warnings
    /// (`adapters/standalone/rustup.rs`).
    RemovesToolchains { path: String, names: Vec<String> },
    /// rustup 1.29.1's `self uninstall` deletes the whole Cargo home,
    /// `path` (`$CARGO_HOME`, spelled `~/.cargo`), permanently -- not to
    /// the Trash: the registry and git caches, `.crates2.json` (its
    /// record of what `cargo install` installed), Cargo's own
    /// `config.toml` and `credentials.toml` (the crates.io login), `env`,
    /// and anything else kept there (self_update.rs:977-993, :1029;
    /// unix.rs:50-53). Always produced.
    DeletesCargoHome { path: String },
    /// rustup 1.29.1's `self uninstall` deletes everything in the Cargo
    /// home's `bin/` whose name is not `rustup` or one of its thirteen
    /// proxies -- by name, so a program copied there by hand goes too:
    /// `names` are the crates `.crates2.json` lists with a program among
    /// those, each by the crate's name, as cargo's inventory names its row
    /// on the Installed page (`ripgrep`, whose program is `rg`), and the
    /// other programs a read-only listing of `bin/` finds, minus those
    /// fourteen names, by their file names
    /// (`rustup::bin_programs_rustup_removes`) -- the programs named where
    /// known. Only produced when there are any.
    /// (The research read a newer rustup that keeps them; the tag this
    /// recipe is verified against does not -- see the recipe's doc.)
    RemovesCargoInstalled { names: Vec<String> },
    /// Homebrew's `rustup` formula is installed too (`Cellar/rustup`
    /// under one of Homebrew's default prefixes,
    /// `rustup::homebrew_rustup_present`), and rustup's homes depend
    /// only on `RUSTUP_HOME`/`CARGO_HOME`/`HOME`, never on where the
    /// binary sits (`home` 0.5.12), so it shares the folders this
    /// uninstall deletes and loses its toolchains with them. Produced
    /// only when the Cellar directory is there.
    HomebrewRustupLosesToolchains,
    /// rustup's `self uninstall` edits the shell startup files it added
    /// its `. "$HOME/.cargo/env"` line to. Banager itself never edits one.
    EditsShellConfig,
    /// After rustup's own cleanup, `path` (`$HOME` spelled `~`) will still
    /// hold a line about Cargo's env file, which is then gone. `certain`
    /// is true when that line is one of the sourcing forms rustup itself
    /// writes, its target is this Cargo home, and every line above it
    /// stands alone (`rustup::classify_leftover`), so a shell that reads
    /// the file *will* print an error until the user removes it (a file
    /// rustup does not edit, such as `~/.zshrc`, or a second copy of the
    /// line); false for any other mention rustup will not remove (a
    /// guarded `[ -f … ] && . …`, an `echo`, another spelling, rustup's
    /// own form inside an `if` or below any line that does not stand
    /// alone), which *may*. Which shells read which file is not decided.
    /// One per startup file name: two names of one file (a link, a hard
    /// link) share what rustup leaves in it, and each is named
    /// (`rustup::shell_config_leftovers`).
    LeavesShellConfigLine { path: String, certain: bool },
    /// After this `brew uninstall`, formula or cask, Homebrew also runs its
    /// autoremove, which uninstalls the formulae that were installed only as
    /// dependencies and that nothing installed needs any more -- any on the
    /// system (`cmd/uninstall.rb:129-136`, `cleanup.rb:1038-1077` in
    /// Homebrew 7.0.6-70). Banager runs every `brew` command with
    /// `HOMEBREW_NO_AUTOREMOVE=1` (`BrewAdapter::ENV`), so this is produced
    /// only when a `brew.env` file sets it back to a value Homebrew reads as
    /// unset -- `0`, `false`, nothing -- which `bin/brew` exports over the
    /// inherited one (`adapters/brew/brew_env.rs`). Produced by
    /// `BrewAdapter::plan` for an `Uninstall`; read by `warningKey` and
    /// `warningDetailKey` in src/lib/warnings.ts.
    HomebrewAutoremoves,
    /// After this `brew install` or `brew upgrade`, Homebrew cleans up
    /// every time (`Install.finish_installation`, `install.rb:325-329`,
    /// which both commands end in). `Cleanup.install_clean!`
    /// (`cleanup.rb:361-389`) deletes, for the formula the command names and
    /// each dependent Homebrew upgraded with it -- not the dependencies it
    /// installed or upgraded on the way -- its older installed versions that
    /// are not linked, pinned or still needed and its downloads in
    /// Homebrew's cache that are outdated or older than
    /// `HOMEBREW_CLEANUP_MAX_AGE_DAYS`, 120 unless set (`cleanup_formula`,
    /// `cleanup.rb:564-571`, `:736-773`; `Formula#eligible_kegs_for_cleanup`);
    /// for the cask the command names, its downloads there that are outdated
    /// or that old (`cleanup_cask`, `cleanup.rb:581-588`); and every download
    /// nothing in the cache refers to any more (`cleanup.rb:705-730`). Then,
    /// whenever a full `brew cleanup` is due -- the last one it recorded is
    /// more than `HOMEBREW_CLEANUP_PERIODIC_FULL_DAYS` days old, 30 unless
    /// set (`cleanup.rb:418-445`) -- it runs one, which does the same for
    /// every installed formula and cask and the whole cache
    /// (`Cleanup#clean!`, `cleanup.rb:448-465`, `:473`). The variant is named
    /// for the periodic clean-up; its line says both. Banager's
    /// `HOMEBREW_NO_INSTALL_CLEANUP=1` keeps both from starting
    /// (`cleanup.rb:341`, `:363`, `:419`), so this is produced only when a
    /// `brew.env` file sets that to nothing (`adapters/brew/brew_env.rs`).
    /// Produced by `BrewAdapter::plan` for an `Install` or an `Upgrade`;
    /// read by `warningKey` and `warningDetailKey` in src/lib/warnings.ts.
    HomebrewPeriodicCleanup,
    /// The periodic clean-up (`HomebrewPeriodicCleanup`) also ends in the
    /// same autoremove as `HomebrewAutoremoves` (`cleanup.rb:471`) unless
    /// `HOMEBREW_NO_AUTOREMOVE` is set; the clean-up after every install or
    /// upgrade does not autoremove. So this is produced only when
    /// `brew.env` files take back both of Banager's variables -- the first
    /// set to nothing, the second to a value Homebrew reads as unset
    /// (`adapters/brew/brew_env.rs`) -- and always right after
    /// `HomebrewPeriodicCleanup`. Produced by `BrewAdapter::plan` for an
    /// `Install` or an `Upgrade`; same readers.
    HomebrewCleanupAutoremoves,
    /// What this uninstall removes and what it leaves, in the one sentence
    /// the uninstall confirmation shows under the tool: which sentence is
    /// `what` (`UninstallScope`). At most one per plan, and only on an
    /// `Uninstall` plan of a source whose sentence holds for the exact argv
    /// and environment the plan runs -- the sources and conditions are
    /// `UninstallScope`'s. Read by `warningGroup` in src/lib/warnings.ts,
    /// which gives it its own group, and rendered by `UninstallDialog` with
    /// the row's name in it.
    UninstallScope { what: UninstallScope },
    /// One kind of extra step a cask's recorded uninstall takes beyond
    /// deleting what Homebrew installed for it (`CaskStep`), with what the
    /// record names for it: paths (`~` for the home folder), installer
    /// package ids, service labels, bundle ids, programs, certificate
    /// names -- empty only for `CaskStep::RunsOwnSteps` and
    /// `CaskStep::DeletesUnnamed`, which name nothing. `only_if` is the
    /// check a `remove` uninstall step makes of each path before it deletes
    /// it (`RemoveCheck`), set only on a `Deletes` or `DeletesUnnamed` that
    /// such a step gave; absent, and left out of the JSON, everywhere else.
    /// One per kind and check, in `CaskStep`'s order -- a kind's line with
    /// no check before its lines with one -- each name once.
    /// Produced by `BrewAdapter::plan` for a cask `Uninstall` whose recorded
    /// uninstall is not plain (`cask_receipt::classify`), beside
    /// `UninstallScope { what: HomebrewCaskSteps }` or another of the
    /// sentences for a cask with steps (`HomebrewCaskStepsAutoremoves`,
    /// `HomebrewCaskStepsUnseen`, `HomebrewCaskStepsOnly`,
    /// `HomebrewCaskStepsOnlyUnseen`); read by `warningKey` and
    /// `warningArgs` in src/lib/warnings.ts.
    CaskUninstallStep {
        step: CaskStep,
        items: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        only_if: Option<RemoveCheck>,
    },
    /// Not yet localised -- see this type's doc comment.
    Message(String),
}

/// Which sentence `Warning::UninstallScope` says: what one source's
/// uninstall removes and what it leaves. Each is true for every package the
/// plan can name under the argv and environment that source's plan runs,
/// on the conditions below -- where a condition does not hold, the plan
/// carries no sentence, or a weaker one. Read by `UNINSTALL_SCOPE_KEYS` in
/// src/lib/warnings.ts, a `Record` over the mirror, so a variant added here
/// without copy fails `tsc`. Each variant names the tool's own source it
/// rests on; `docs/what-we-run.md` says what each plan runs.
///
/// No sentence for pip (Banager never uninstalls from pip), for npm older
/// than 7 or of an unknown version (npm 6 ran a package's uninstall
/// scripts), for uv while `UV_TOOL_DIR` is set (the plan is refused), or for
/// the four tools with their own installer, whose uninstall confirmation
/// already lists what goes to the Trash, what stays and what rustup deletes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UninstallScope {
    /// `brew uninstall --formula` with Homebrew's autoremove off: it removes
    /// only one installed version -- the one `opt` links to, else the
    /// linked one, else the only one, else the newest
    /// (`cli/named_args.rb:567-593` in Homebrew 7.0.6-70) -- and the links
    /// into it (`keg.rb:325-393`), and leaves its config under `etc`
    /// (`uninstall.rb:72-80`) and its data under `var`, which is outside
    /// the keg. Produced when the `brew.env` files leave
    /// `HOMEBREW_NO_AUTOREMOVE=1` in force (`brew_env::after_brew_env`).
    HomebrewFormulaOnly,
    /// The same uninstall with autoremove back on through a `brew.env`
    /// file: the same sentence without "only", beside
    /// `Warning::HomebrewAutoremoves`, which says what else goes.
    HomebrewFormula,
    /// `brew uninstall --cask` whose recorded uninstall is plain: it deletes
    /// what Homebrew itself put down and linked -- the record lists at
    /// least one such artifact -- and otherwise only quits apps, removes
    /// folders once nothing but empty folders is left in them, and runs
    /// steps that change a path's owner or permissions or end a process
    /// (`cask_receipt::classify`). Its settings and data stay: the cask's
    /// `zap` stanza runs only with `--zap` (`cmd/uninstall.rb:90-117`),
    /// which Banager never passes.
    HomebrewCaskPlain,
    /// `brew uninstall --cask` whose recorded uninstall deletes what
    /// Homebrew put down and linked and takes extra steps, each kind of
    /// which the plan names in a `Warning::CaskUninstallStep` that says what
    /// it does -- none a step whose deletions Banager cannot see, which
    /// makes it `HomebrewCaskStepsUnseen` -- with Homebrew's autoremove off.
    /// The sentence says Homebrew deletes the files it placed for the cask
    /// -- what it moved into place, linked or generated, and its own copy
    /// and records in the Caskroom (`Cask::Installer#uninstall`,
    /// `cask/installer.rb:622-640`, `:642-659`, `:814-835`, `:1049-1061`) --
    /// and not every file the cask's installer put down: a `pkg` or an
    /// installer beside them is not in the record (`cask/cask.rb:709-732`).
    /// It runs the recorded steps, and nothing else is deleted: `zap` runs
    /// only with `--zap`, which Banager never passes, and the autoremove is
    /// off (`cmd/uninstall.rb:89-136`). When the current definition names
    /// an old token the cask still has another installation under,
    /// Homebrew first uninstalls that one -- all but what it shares with
    /// this one -- and deletes its Caskroom folder
    /// (`Cask::Migrator.migrate_if_needed` from `cask/installer.rb:988`,
    /// `cask/migrator.rb:24-66`, `:85-119`): again files Homebrew placed for
    /// the cask and steps it recorded for it.
    HomebrewCaskSteps,
    /// The same uninstall with autoremove back on through a `brew.env`
    /// file: the same sentence with "the cask's other files stay" in place
    /// of "nothing else is deleted", beside `Warning::HomebrewAutoremoves`,
    /// which says what else goes.
    HomebrewCaskStepsAutoremoves,
    /// `HomebrewCaskSteps` for a record with at least one step whose
    /// deletions Banager cannot see (`cask_receipt::runs_unseen`): a program
    /// the cask names (`early_script:`, `script:`, an uninstall step of type
    /// `run`; `CaskStep::RunsScript`), or Ruby around the uninstall or an
    /// uninstall step Banager does not name (`CaskStep::RunsOwnSteps`).
    /// Banager knows the step is there, and names the program, but not what
    /// it deletes: a vendor's uninstaller may take the app's settings and
    /// data with it. So the sentence says Homebrew deletes the files it
    /// placed for the cask and runs the uninstall steps it recorded, and
    /// that Banager cannot see what else some of those steps delete -- never
    /// that anything stays. It claims nothing about other files, so it
    /// holds with Homebrew's autoremove on as well, beside
    /// `Warning::HomebrewAutoremoves`.
    HomebrewCaskStepsUnseen,
    /// `brew uninstall --cask` whose record lists nothing Homebrew put down
    /// or linked -- a cask installed with a `pkg` or an installer, neither
    /// of which the record lists (`cask/cask.rb:709-732`) -- but takes extra
    /// steps, each kind of which the plan names in a
    /// `Warning::CaskUninstallStep` that says what it does, none a step
    /// whose deletions Banager cannot see (`HomebrewCaskStepsOnlyUnseen`):
    /// nothing else deletes any of what the installer put down
    /// (little-snitch@4's only step removes its background services).
    HomebrewCaskStepsOnly,
    /// `HomebrewCaskStepsOnly` for a record with at least one step whose
    /// deletions Banager cannot see, as for `HomebrewCaskStepsUnseen`
    /// (wireshark-chmodbpf's `early_script:` runs its vendor's uninstaller
    /// package). The sentence says Homebrew runs the uninstall steps it
    /// recorded, and that Banager cannot see what else some of those steps
    /// delete -- never that the other files the installer put down stay.
    HomebrewCaskStepsOnlyUnseen,
    /// `brew uninstall --cask` whose recorded uninstall Banager could not
    /// read (`cask_receipt::read_recorded`): no Caskroom folder, no saved
    /// caskfile, one saved in a form it does not read, a record Homebrew
    /// would replace with the cask's current definition -- no list of its
    /// own and an empty one or none in the receipt -- or a kind of artifact
    /// it does not know; or a record that lists neither anything Homebrew
    /// put down or linked nor any step -- an empty list, whatever the
    /// receipt says of Ruby blocks, or `zap` alone -- which cannot tell what
    /// the install left. The sentence says only that Banager could not read
    /// from Homebrew's records what the uninstall deletes, and claims no
    /// deletion it cannot back: with an empty list Homebrew runs no
    /// artifact's uninstall at all (`cask/installer.rb:714-761`), and a
    /// record Banager does not read can list anything.
    HomebrewCask,
    /// `npm uninstall -g`, when the npm Banager detected is 7 or later: npm
    /// deletes the package's folder, with the dependencies inside it, and
    /// its command and man-page links, and runs no script of the package's
    /// (npm 10.9.9 `lib/commands/uninstall.js:38-52`, arborist's
    /// `reify.js:1308-1341`; npm 12.0.2 the same). npm 6 ran the package's
    /// `uninstall` scripts (`docs/content/using-npm/scripts.md:216-228`).
    Npm,
    /// `pipx uninstall`: pipx deletes the tool's own virtual environment
    /// and the links into it, and nothing else of the tool's (pipx 1.17.3
    /// `commands/uninstall.py:67-125`).
    Pipx,
    /// `uv tool uninstall`, while `UV_TOOL_DIR` is unset: uv deletes the
    /// tool's environment and the executables its receipt records (uv
    /// 0.12.17 `crates/uv/src/commands/tool/uninstall.rs:187-226`).
    Uv,
    /// `cargo uninstall`: cargo deletes the binaries its install record
    /// lists for the crate and rewrites that record, and runs no crate code
    /// (cargo 1.98.1 `src/cargo/ops/cargo_uninstall.rs`).
    Cargo,
    /// `ollama rm`: Ollama deletes the model's manifest and each of its
    /// layers no other model uses (Ollama 0.34.1
    /// `server/routes.go:1249-1299`, `manifest/manifest.go:74-115`).
    Ollama,
}

/// One kind of extra step a cask's recorded uninstall takes, for the line
/// `Warning::CaskUninstallStep` puts under 「请注意」. Declared in the order
/// the lines are said. Each maps to directives of the cask's `uninstall`
/// stanza or its `uninstall_*` steps as Homebrew 7.0.6 runs them
/// (`cask/artifact/abstract_uninstall.rb`, `install_steps.rb`); produced by
/// `cask_receipt::classify`; read by `CASK_STEP_KEYS` in
/// src/lib/warnings.ts, a `Record` over the mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CaskStep {
    /// `delete:` (`sudo rm -r -f`, globs expanded, `~` the home folder), an
    /// `artifact` Homebrew placed in the home folder, which its uninstall
    /// deletes again, and each path an uninstall step of type `remove`
    /// names outright (`FileUtils.rm_f`/`rm_rf`, or a removal with `sudo`,
    /// globs expanded; `install_steps.rb:1049-1070`): gone for good, not to
    /// the Trash. A `remove` step that records a check deletes a path only
    /// where it passes it; its paths are a line of their own, with the
    /// check (`Warning::CaskUninstallStep`'s `only_if`, `RemoveCheck`).
    Deletes,
    /// A `remove` uninstall step's path the record does not spell out: one
    /// Homebrew resolves against a folder it knows only when it runs the
    /// step -- the cask's own staged folder, the folders it looks for
    /// commands in, its working folder -- or through a `{{…}}` template.
    /// Gone for good like `Deletes`, but named by nothing; like `Deletes`,
    /// its line carries the step's check when the step records one.
    DeletesUnnamed,
    /// `trash:`: moved to the Trash.
    Trashes,
    /// `pkgutil:`: every file each matching installer package recorded is
    /// deleted, whatever else uses it, and the package is forgotten
    /// (`cask/pkg.rb`). The items are the ids or patterns the cask names.
    RemovesPackages,
    /// `early_script:` and `script:`, and an uninstall step of type `run`
    /// whose program the record names: a program the cask names is run.
    /// What it deletes Banager cannot see, so the sentence beside it is
    /// `UninstallScope::HomebrewCaskStepsUnseen` or
    /// `HomebrewCaskStepsOnlyUnseen`.
    RunsScript,
    /// An `uninstall_preflight`/`uninstall_postflight` block of Ruby, or an
    /// uninstall step Banager does not name (anything but the ones that set
    /// ownership or permissions or end a process, which stay plain, and the
    /// ones the other kinds name). Names nothing; what it deletes Banager
    /// cannot see, as for `RunsScript`.
    RunsOwnSteps,
    /// `launchctl:`: each service is removed with `launchctl remove` and
    /// its plist deleted from the LaunchAgents and LaunchDaemons folders.
    RemovesServices,
    /// `kext:`: each kernel extension is unloaded and deleted.
    RemovesKexts,
    /// An uninstall step of type `delete_keychain_certificate`, which runs
    /// `security find-certificate -a -c <name> -Z` with `sudo` and deletes
    /// each certificate it lists (`install_steps.rb:1179-1210`): every
    /// certificate in the keychain whose name contains the item, not only
    /// the cask's own -- `-a` lists all that match, and `-c` matches a
    /// name that includes it (security(1), `find-certificate`). A step
    /// that also names a `matching_certificate` file deletes only the one
    /// certificate with that file's hash, which this line would overstate:
    /// it is `RunsOwnSteps`.
    DeletesCertificates,
    /// `login_item:`: those login items are deleted, and so are the
    /// cask's own apps' (`uninstall_login_item`).
    RemovesLoginItems,
    /// `quit:` and `signal:`: running apps with those bundle ids (`*` a
    /// wildcard) are quit or signalled. Plain on its own; said only beside
    /// another kind. The items are the bundle ids of the apps Banager
    /// could not find on this Mac; the line counts them, the ids behind
    /// its ⓘ.
    QuitsApps,
    /// `QuitsApps`, for the apps it quits that Banager found: an app the
    /// cask's record puts down whose `CFBundleIdentifier` is one the step
    /// names, found where Homebrew puts apps
    /// (`BrewAdapter::quit_app_names`). The items are the apps' names, as
    /// Finder shows their bundles ("Visual Studio Code"), not their bundle
    /// ids. Never produced by `cask_receipt::classify`, which reads the
    /// record alone: the preview turns a `QuitsApps` line into this one,
    /// and a `QuitsApps` for the rest, once it has looked.
    QuitsNamedApps,
}

/// The check an uninstall step of type `remove` makes of each path it
/// lists, once globs are expanded, before it deletes it: with
/// `symlink_target_contains`, only a path that is a link whose target, as
/// the link spells it, contains the text (`path.symlink? &&
/// path.readlink.to_s.include?`); with `content_contains`, only a path that
/// is a file -- a link to one counts -- Homebrew can read and whose
/// contents contain the text (`path.file? && path.readable? &&
/// path.read.include?`); with both, only a path that passes both
/// (`install_steps.rb:1051-1060` in Homebrew 7.0.6-70). The text is taken
/// as the record spells it: Homebrew fills in no `{{…}}` template there
/// (`step_string`, `:1499-1501`). Set on `Warning::CaskUninstallStep`'s
/// `only_if` by `cask_receipt::classify`; read by `warningKey` and
/// `warningArgs` in src/lib/warnings.ts, which pick the line and fill in
/// the text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RemoveCheck {
    /// `symlink_target_contains`.
    LinkTargetContains(String),
    /// `content_contains`.
    ContentContains(String),
    /// Both, on one step.
    LinkTargetAndContentContain {
        link_target: String,
        content: String,
    },
}

/// Why the tool itself will refuse to update this one package, although
/// its source is writable and answering. The per-package half of the
/// actionability gate (spec §8); `ReadOnlyReason` and `Unavailable` are the
/// per-source halves.
///
/// A variant belongs here only when the tool *reports* the state in the
/// output Banager already reads to list updates, so the row can be marked
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
    /// The tool installs its updates itself and has no update command
    /// Banager may run for it, so a newer version is listed with no
    /// button. Produced by `StandaloneAdapter::check_updates`
    /// (`adapters/standalone/mod.rs`) for a recipe whose `upgrade` is
    /// `None` -- Antigravity CLI, whose `agy update` is undocumented, takes
    /// no options and has never been run (agy.md §4; spec §4.4) -- and, for
    /// the same recipe, by `StandaloneAdapter::plan`'s `Upgrade` arm inside
    /// `AdapterError::UpdateBlocked`: the gate's late twin for a stale
    /// snapshot (spec §五). Not "no
    /// candidate": the Installed row would then say "up to date", which is
    /// false while 1.2.11 exists; not `checkable: false`: Banager did
    /// check. Read by the gate (`blocked_upgrade` in session/plans.rs,
    /// generic over this enum), by `updateStateOf` in
    /// src/lib/updateState.ts (no button, no checkbox) and by
    /// `UPDATE_BLOCKED_KEYS.SelfUpdatesOnly` in src/lib/sources.ts, whose
    /// sentence tells the user to open the tool once and that it checks at
    /// most every 15 minutes.
    SelfUpdatesOnly,
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
    /// Banager could check it -- `checkable` says nothing about this: a
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
    /// `KillThenReconcile` and the command never starts. Produced by the
    /// rustup recipe (`adapters/standalone/recipes.rs`) for `rustup self
    /// update`, which unlinks `$CARGO_HOME/bin/rustup` -- the one binary
    /// its thirteen proxies run -- and copies the new one in, not
    /// atomically (rustup 1.29.1 `install_bins`), and for `rustup self
    /// uninstall`, which removes Rust directory by directory; a kill
    /// partway leaves no working Rust.
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
    /// The command reported success but reconcile disagrees -- or a
    /// path-list uninstall moved everything on its list and then found part
    /// of what the list names there (`Attention::BackAfterUninstall`, from
    /// its own last look). Carries which disagreement, never a sentence: the
    /// front end words it in the user's language (the drawer and the
    /// operation bar both show it).
    NeedsAttention(Attention),
    /// Another program failed the operation, and `summary` is that
    /// program's own words and nothing of Banager's: the front end shows it
    /// as-is but for surrounding whitespace, quoted inside a translated
    /// sentence, or says the program gave no reason when it is blank
    /// (`outcomeKey` and `outcomeArgs` in `src/lib/format.ts`).
    /// Two places build it. A command that ran and exited non-zero:
    /// `exit_code` is the command's, and `summary` the last five lines of
    /// its stderr (`run_plan` in `adapters/mod.rs`). A path-list uninstall
    /// the system refused: no command ran, so `exit_code` is `None`, and
    /// `summary` is macOS's own description of the refusal -- the
    /// `NSError`'s localized description, `TrashError::Refused`
    /// (`removal::execute_removal`, which also writes it to the log as a
    /// `LogNote::TrashFailed`). A failure of Banager's own is
    /// `BanagerFailed`, never this. A tool a signal ended before it could
    /// exit reported no failure, and is `Unconfirmed`, never this
    /// (`run_plan` in `adapters/mod.rs`).
    Failed {
        exit_code: Option<i32>,
        summary: String,
    },
    /// Banager itself could not carry the operation out -- not the tool.
    /// Carries which reason, never a sentence: the front end words it in
    /// the user's language, the same way it does `NeedsAttention`.
    ///
    /// These used to be English sentences of Banager's own ("operation
    /// panicked", "runner: program not found: ...") inside `Failed`'s
    /// `summary`, sharing one string with a tool's stderr, so neither could
    /// be shown properly: the front end could not translate the first
    /// without mangling the second.
    BanagerFailed(Fault),
    /// Banager cannot tell what the operation did: the reading after it
    /// failed, or the command did not reach its exit (a Cancel, the
    /// timeout, or a signal Banager did not send -- Activity Monitor,
    /// `kill`, a crash) and what is installed now does not show whether it
    /// took effect. Every upgrade stopped partway ends here, whatever its
    /// version reads (`run_operation` in `ops/mod.rs` says why).
    Unconfirmed,
}

/// Why Banager itself could not carry an operation out. See
/// [`Outcome::BanagerFailed`]. Fields carry data, never Banager's prose:
/// a path, or the operating system's own reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    /// Banager crashed partway through. The command may or may not have
    /// run, so only a fresh look at the list can say what changed.
    Panicked,
    /// The program the plan names was not there when Banager went to run
    /// it. Nothing was started.
    ProgramMissing { program: String },
    /// macOS would not start the program; `detail` is the operating
    /// system's own reason, quoted as-is. Nothing was started.
    SpawnFailed { detail: String },
    /// A `brew update` was still running in the background after Banager
    /// had waited `minutes` minutes for it, so the command was not
    /// started: installing while Homebrew rewrites its own list of
    /// software is not something Homebrew guards against. Nothing was
    /// started.
    ///
    /// `minutes` is `BrewAdapter::OP_UPDATE_WAIT` outside tests, carried
    /// here rather than hard-coded into
    /// `operations.outcome.BanagerFailed.HomebrewStillUpdating` so the two
    /// can never disagree: see `BrewAdapter::execute`, the only production
    /// call site that builds this variant.
    HomebrewStillUpdating { minutes: u64 },
    /// A path is not what the preview showed, so Banager stopped and left
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
    /// launcher left in place so the row stays. Banager stopped without
    /// moving the item whose turn it was; whatever it moved before is in
    /// the Trash, one `LogNote::MovedToTrash` each in the log. For a
    /// standalone tool's upgrade, the launcher the plan
    /// would run: looked at again right before the spawn, it is no longer
    /// the native install's -- gone, dangling, a plain file, or a link
    /// resolving outside the tool's own root (at Homebrew's or npm's copy,
    /// say) -- so the command was not started; a launcher re-pointed at a
    /// newer version inside that root is still the native install's, and
    /// the command runs. For a standalone tool's command uninstall
    /// (rustup's), the launcher likewise, and a folder the command
    /// deletes: the recipe's gate that passed at the preview is asked
    /// again right before the spawn, and a `~/.rustup` or `~/.cargo` that
    /// is now a link to somewhere else, or otherwise not the real folder
    /// the gate accepted (a `~/.rustup` that is simply gone still passes:
    /// rustup finds nothing there), is `path`, the command not started --
    /// rustup deletes wherever those resolve when it runs. `path` has the home
    /// folder abbreviated to `~`; it is the kept path when a kept path is
    /// what changed. Built by
    /// `removal::execute_removal` (`adapters/standalone/removal.rs`) and
    /// `StandaloneAdapter::execute` (`adapters/standalone/mod.rs`); read by
    /// `faultKey`/`faultArgs` in src/lib/format.ts.
    PathChanged { path: String },
    /// Something on Banager's side did not add up (an unregistered
    /// adapter or instance, a queue that closed, an error `execute` has no
    /// business returning). A bug in Banager, not a state of the Mac.
    /// Nothing was started.
    Internal,
}

/// What reconcile found that the command's own success did not account
/// for -- or, for `BackAfterUninstall`, what a path-list uninstall's own
/// last look found. See [`Outcome::NeedsAttention`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attention {
    /// An install exited 0 and the item is not installed.
    NotInstalledAfterInstall,
    /// An uninstall ended as if it had succeeded -- its command exited 0,
    /// or a path-list uninstall moved every listed path to the Trash -- and
    /// the item is still installed: for a path-list uninstall, its launcher
    /// or another path on its list is there
    /// (`StandaloneAdapter::reconcile_after_uninstall`).
    StillInstalledAfterUninstall,
    /// An upgrade exited 0 and the item is no longer installed at all.
    GoneAfterUpgrade,
    /// An upgrade exited 0 and the item is still installed at the version
    /// it was at before: the tool skipped it without saying so in its exit
    /// code. `run_operation` (`crates/banager-core/src/ops/mod.rs`) builds
    /// this only when two reads of the installed version, one taken before
    /// the command and one after, both succeeded and are equal.
    UnchangedAfterUpgrade,
    /// A path-list uninstall moved every path on its list to the Trash,
    /// and when it looked once more, after the pause that follows its last
    /// move (`removal::left_behind`), part of what its list names was
    /// there: a path it moved, back again -- a copy of the tool still
    /// running can put its program folder or its download cache back --
    /// or one it never moved, there now. Nothing is moved again, and each
    /// such path has its own `LogNote::BackAfterUninstall` line. The
    /// launcher moved too, so unless it is back as well no row shows what
    /// came back: this outcome and those lines do. Built by
    /// `removal::execute_removal` (`adapters/standalone/removal.rs`), which
    /// `run_operation` passes on unchanged; read by `attentionKey` in
    /// src/lib/format.ts.
    BackAfterUninstall,
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
        // Ids live on disk inside `Settings.ignored_updates` and
        // `Settings.skipped_versions`; the shared constructor must not
        // change a single one of them.
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

        // Phase 4 step D: the second reason, a tool that installs its updates
        // itself and offers no command Banager may run
        // (`StandaloneAdapter::check_updates` for a recipe with no `upgrade`).
        // `UPDATE_BLOCKED_KEYS.SelfUpdatesOnly` in src/lib/sources.ts indexes
        // this spelling.
        assert_eq!(
            serde_json::to_string(&UpdateBlocked::SelfUpdatesOnly).unwrap(),
            r#""SelfUpdatesOnly""#
        );
    }

    #[test]
    fn test_facts_is_an_object_with_explicit_nulls_on_the_wire_and_optional_when_read() {
        // `src/lib/types.ts` spells it `facts: ArtifactFacts` with
        // `family: string | null`, `homebrew: HomebrewFacts | null` and
        // `commands: CommandFact[]`, and `NO_FACTS` is this default.
        let facts = ArtifactFacts::default();
        assert_eq!(
            serde_json::to_string(&facts).unwrap(),
            r#"{"family":null,"homebrew":null,"commands":[]}"#
        );
        // A payload written before a fact existed still reads.
        assert_eq!(
            serde_json::from_str::<ArtifactFacts>("{}").unwrap(),
            ArtifactFacts::default()
        );
        let claude = ArtifactFacts {
            family: Some("claude-code".to_string()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&claude).unwrap(),
            r#"{"family":"claude-code","homebrew":null,"commands":[]}"#
        );
    }

    #[test]
    fn test_homebrew_facts_spell_every_field_on_the_wire_and_read_back() {
        // `src/lib/types.ts` mirrors this as `HomebrewFacts` /
        // `HomebrewLifecycle`, every optional field an explicit `null`, and
        // `other_versions` an array. The same literal is round-tripped in
        // src/lib/types.test.ts.
        let facts = ArtifactFacts {
            homebrew: Some(HomebrewFacts {
                deprecated: None,
                disabled: Some(HomebrewLifecycle {
                    date: Some("2026-09-01".to_string()),
                    reason: Some("fails_gatekeeper_check".to_string()),
                    replacement: Some("onyx".to_string()),
                }),
                caveats: Some("Turn on \"Launch at login\".\n".to_string()),
                other_versions: vec!["3.6.3".to_string()],
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&facts).unwrap();
        assert_eq!(
            json,
            r#"{"family":null,"homebrew":{"deprecated":null,"disabled":{"date":"2026-09-01","reason":"fails_gatekeeper_check","replacement":"onyx"},"caveats":"Turn on \"Launch at login\".\n","other_versions":["3.6.3"]},"commands":[]}"#
        );
        assert_eq!(serde_json::from_str::<ArtifactFacts>(&json).unwrap(), facts);
        // Fields a payload leaves out read as empty.
        assert_eq!(
            serde_json::from_str::<HomebrewFacts>("{}").unwrap(),
            HomebrewFacts::default()
        );
        assert_eq!(
            serde_json::from_str::<HomebrewLifecycle>("{}").unwrap(),
            HomebrewLifecycle::default()
        );
    }

    #[test]
    fn test_command_facts_are_externally_tagged_and_their_inputs_never_reach_the_wire() {
        // `src/lib/types.ts` spells `CommandState` as the bare string
        // "Runs" and single-key objects for the two with a payload, and
        // `CommandFact.state` as `CommandState | null`.
        let key = ArtifactKey {
            instance_id: "npm:/opt/homebrew".to_string(),
            kind: ArtifactKind::Package,
            name: "@anthropic-ai/claude-code".to_string(),
        };
        let facts = ArtifactFacts {
            family: None,
            homebrew: None,
            commands: vec![
                CommandFact {
                    name: "agent".to_string(),
                    state: Some(CommandState::ShadowedBy {
                        by: Some(key.clone()),
                    }),
                },
                CommandFact {
                    name: "claude".to_string(),
                    state: Some(CommandState::Runs),
                },
                CommandFact {
                    name: "grok".to_string(),
                    state: Some(CommandState::NotOnPath {
                        dir: "~/.grok/bin".to_string(),
                    }),
                },
                CommandFact {
                    name: "rg".to_string(),
                    state: Some(CommandState::ShadowedBy { by: None }),
                },
                CommandFact {
                    name: "curl".to_string(),
                    state: None,
                },
            ],
            command_inputs: CommandInputs {
                provided: vec![ProvidedCommand {
                    name: "claude".to_string(),
                    path: PathBuf::from("/opt/homebrew/bin/claude"),
                    within: Vec::new(),
                }],
                keg_only: true,
            },
        };
        let json = serde_json::to_string(&facts).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"family":null,"homebrew":null,"commands":["#,
                r#"{"name":"agent","state":{"ShadowedBy":{"by":{"instance_id":"npm:/opt/homebrew","kind":"Package","name":"@anthropic-ai/claude-code"}}}},"#,
                r#"{"name":"claude","state":"Runs"},"#,
                r#"{"name":"grok","state":{"NotOnPath":{"dir":"~/.grok/bin"}}},"#,
                r#"{"name":"rg","state":{"ShadowedBy":{"by":null}}},"#,
                r#"{"name":"curl","state":null}"#,
                "]}"
            )
        );
        // The inputs stay behind: what comes back is the answer alone.
        let back: ArtifactFacts = serde_json::from_str(&json).unwrap();
        assert_eq!(back.commands, facts.commands);
        assert_eq!(back.command_inputs, CommandInputs::default());
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
            facts: Default::default(),
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
            facts: Default::default(),
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
            facts: Default::default(),
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

        // Round 2: a uv tool while `UV_TOOL_DIR` is set. A third spelling.
        assert_eq!(
            serde_json::to_string(&UninstallBlocked::UvToolDirSet).unwrap(),
            r#""UvToolDirSet""#
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
            serde_json::to_string(&Warning::DownloadsModelChanges).unwrap(),
            r#""DownloadsModelChanges""#
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
        // Every kind, as `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS` in
        // src/lib/warnings.ts spell them (step C's three and two, step D's
        // `Backups` and five more).
        for what in [
            RemovedWhat::Launcher,
            RemovedWhat::Program,
            RemovedWhat::Cache,
            RemovedWhat::Backups,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
        for what in [
            KeptWhat::Settings,
            KeptWhat::SettingsAndHistory,
            KeptWhat::ToolState,
            KeptWhat::ShellConfigLines,
            KeptWhat::OutsideHome,
            KeptWhat::NotOurs,
            KeptWhat::InstallerCache,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }

        // Phase 4 step E: what rustup's own uninstall does (adapters/
        // standalone/rustup.rs). Two payload-free, four with a payload;
        // the same two spellings as above.
        assert_eq!(
            serde_json::to_string(&Warning::RemovesToolchains {
                path: "~/.rustup".to_string(),
                names: vec!["stable-aarch64-apple-darwin".to_string()]
            })
            .unwrap(),
            r#"{"RemovesToolchains":{"path":"~/.rustup","names":["stable-aarch64-apple-darwin"]}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::DeletesCargoHome {
                path: "~/.cargo".to_string()
            })
            .unwrap(),
            r#"{"DeletesCargoHome":{"path":"~/.cargo"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::RemovesCargoInstalled {
                names: vec!["hexyl".to_string(), "rg".to_string()]
            })
            .unwrap(),
            r#"{"RemovesCargoInstalled":{"names":["hexyl","rg"]}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::HomebrewRustupLosesToolchains).unwrap(),
            r#""HomebrewRustupLosesToolchains""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::EditsShellConfig).unwrap(),
            r#""EditsShellConfig""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true
            })
            .unwrap(),
            r#"{"LeavesShellConfigLine":{"path":"~/.zshrc","certain":true}}"#
        );

        // Round 2: what a brew.env that takes Banager's switches back
        // makes Homebrew do (adapters/brew/brew_env.rs). Three bare
        // strings, as `warningKey` in src/lib/warnings.ts spells them.
        assert_eq!(
            serde_json::to_string(&Warning::HomebrewAutoremoves).unwrap(),
            r#""HomebrewAutoremoves""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::HomebrewPeriodicCleanup).unwrap(),
            r#""HomebrewPeriodicCleanup""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::HomebrewCleanupAutoremoves).unwrap(),
            r#""HomebrewCleanupAutoremoves""#
        );

        // Round 2: an uninstall's one sentence about what goes and what
        // stays, and a cask's extra steps. Two externally tagged objects
        // whose `what` and `step` are bare strings, as `UNINSTALL_SCOPE_KEYS`
        // and `CASK_STEP_KEYS` in src/lib/warnings.ts spell them.
        assert_eq!(
            serde_json::to_string(&Warning::UninstallScope {
                what: UninstallScope::HomebrewCaskPlain
            })
            .unwrap(),
            r#"{"UninstallScope":{"what":"HomebrewCaskPlain"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::CaskUninstallStep {
                step: CaskStep::RemovesPackages,
                items: vec!["com.microsoft.pkg.licensing".to_string()],
                only_if: None,
            })
            .unwrap(),
            r#"{"CaskUninstallStep":{"step":"RemovesPackages","items":["com.microsoft.pkg.licensing"]}}"#
        );
        // A `remove` step's check: `only_if`, externally tagged, as
        // `RemoveCheck` in src/lib/types.ts spells it -- and a step read
        // back without one has none.
        for (check, json) in [
            (
                RemoveCheck::LinkTargetContains("playdate".to_string()),
                r#"{"LinkTargetContains":"playdate"}"#,
            ),
            (
                RemoveCheck::ContentContains("SocketLock".to_string()),
                r#"{"ContentContains":"SocketLock"}"#,
            ),
            (
                RemoveCheck::LinkTargetAndContentContain {
                    link_target: "MacGPG2".to_string(),
                    content: "gpg".to_string(),
                },
                r#"{"LinkTargetAndContentContain":{"link_target":"MacGPG2","content":"gpg"}}"#,
            ),
        ] {
            let warning = Warning::CaskUninstallStep {
                step: CaskStep::Deletes,
                items: vec!["/usr/local/bin/arm-*".to_string()],
                only_if: Some(check),
            };
            let wire = serde_json::to_string(&warning).unwrap();
            assert_eq!(
                wire,
                format!(
                    r#"{{"CaskUninstallStep":{{"step":"Deletes","items":["/usr/local/bin/arm-*"],"only_if":{json}}}}}"#
                )
            );
            assert_eq!(serde_json::from_str::<Warning>(&wire).unwrap(), warning);
        }
        assert_eq!(
            serde_json::from_str::<Warning>(
                r#"{"CaskUninstallStep":{"step":"Trashes","items":["~/.nvs"]}}"#
            )
            .unwrap(),
            Warning::CaskUninstallStep {
                step: CaskStep::Trashes,
                items: vec!["~/.nvs".to_string()],
                only_if: None,
            }
        );
        for what in [
            UninstallScope::HomebrewFormulaOnly,
            UninstallScope::HomebrewFormula,
            UninstallScope::HomebrewCaskPlain,
            UninstallScope::HomebrewCaskSteps,
            UninstallScope::HomebrewCaskStepsAutoremoves,
            UninstallScope::HomebrewCaskStepsUnseen,
            UninstallScope::HomebrewCaskStepsOnly,
            UninstallScope::HomebrewCaskStepsOnlyUnseen,
            UninstallScope::HomebrewCask,
            UninstallScope::Npm,
            UninstallScope::Pipx,
            UninstallScope::Uv,
            UninstallScope::Cargo,
            UninstallScope::Ollama,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
        for step in [
            CaskStep::Deletes,
            CaskStep::DeletesUnnamed,
            CaskStep::Trashes,
            CaskStep::RemovesPackages,
            CaskStep::RunsScript,
            CaskStep::RunsOwnSteps,
            CaskStep::RemovesServices,
            CaskStep::RemovesKexts,
            CaskStep::DeletesCertificates,
            CaskStep::RemovesLoginItems,
            CaskStep::QuitsApps,
            CaskStep::QuitsNamedApps,
        ] {
            assert_eq!(
                serde_json::to_string(&step).unwrap(),
                format!("\"{step:?}\"")
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
    fn test_banager_failed_is_externally_tagged_on_the_wire() {
        // `src/lib/types.ts` mirrors `Fault` as a union of bare strings
        // (unit variants) and single-key objects (data variants), and
        // `format.ts` builds the locale key from the variant name.
        assert_eq!(
            serde_json::to_string(&Outcome::BanagerFailed(Fault::Panicked)).unwrap(),
            r#"{"BanagerFailed":"Panicked"}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::BanagerFailed(Fault::ProgramMissing {
                program: "/opt/homebrew/bin/brew".to_string()
            }))
            .unwrap(),
            r#"{"BanagerFailed":{"ProgramMissing":{"program":"/opt/homebrew/bin/brew"}}}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::BanagerFailed(Fault::SpawnFailed {
                detail: "Permission denied (os error 13)".to_string()
            }))
            .unwrap(),
            r#"{"BanagerFailed":{"SpawnFailed":{"detail":"Permission denied (os error 13)"}}}"#
        );
        assert_eq!(
            serde_json::to_string(&Outcome::BanagerFailed(Fault::HomebrewStillUpdating {
                minutes: 10
            }))
            .unwrap(),
            r#"{"BanagerFailed":{"HomebrewStillUpdating":{"minutes":10}}}"#
        );
        // Phase 4 step C: a path-list uninstall found a path changed
        // between the preview and the run. `path` has `$HOME` abbreviated.
        assert_eq!(
            serde_json::to_string(&Outcome::BanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            }))
            .unwrap(),
            r#"{"BanagerFailed":{"PathChanged":{"path":"~/.local/bin/claude"}}}"#
        );
        for fault in [
            Fault::Panicked,
            Fault::HomebrewStillUpdating { minutes: 10 },
            Fault::Internal,
        ] {
            let json = serde_json::to_string(&Outcome::BanagerFailed(fault.clone())).unwrap();
            let back: Outcome = serde_json::from_str(&json).unwrap();
            assert_eq!(back, Outcome::BanagerFailed(fault));
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
        // A path-list uninstall's own last look (`removal::execute_removal`).
        assert_eq!(
            serde_json::to_string(&Outcome::NeedsAttention(Attention::BackAfterUninstall)).unwrap(),
            r#"{"NeedsAttention":"BackAfterUninstall"}"#
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
