//! One tool = one `Recipe`: all `'static` data, no trait objects, one
//! table to read. Every field's doc names the reader that consumes it; a
//! field without a reader is not added (phase 4 spec §3.1, §十).
//!
//! Only the shapes something produces exist here. Step C added the
//! path-list uninstall (`uninstall: Option<Uninstall>`, `Uninstall::Paths`);
//! step E adds, for rustup, the `SecondToken` version parse, the
//! `HttpTomlVersion` source, the `FlatFile` route, `$CARGO_HOME` paths,
//! `extra_locks` and `Uninstall::Command`; step D added `backup_globs` and
//! adds `Expect::File`, `Expect::SymlinkToProgram` (grok's links besides its
//! launcher), the two `Latest` sources a manifest and a tool's own check
//! need, and an optional `upgrade` (agy updates itself only). A
//! variant or field defined before anything produces it is this project's
//! most common defect (spec §十三 #41).

use super::Detected;
use crate::model::{CancelPolicy, KeptWhat, RemovedWhat, ResourceLock, UninstallBlocked, Warning};
use crate::scan::Glob;
use std::path::PathBuf;

/// A tool installed by its own installer, as data.
#[derive(Debug)]
pub struct Recipe {
    /// `"claude"`. The adapter id is `standalone-{id}`; `ArtifactKey.name`
    /// is this; and it is the command the user types, which
    /// `route::shadow_note` resolves on `PATH`. Every first-batch tool's
    /// command name equals its id; a `binary` field arrives with the first
    /// tool whose does not. Read by `StandaloneAdapter::new`, `detect`,
    /// `inventory`, `plan`.
    pub id: &'static str,
    /// `include_str!` of `adapters/meta/standalone-<id>.toml`, parsed by
    /// `AdapterMeta::from_toml` in `StandaloneAdapter::new`. The display
    /// name and homepage are `meta.name` / `meta.homepage`, never a second
    /// copy here (spec §十三 #45).
    pub meta_toml: &'static str,
    /// Where the installer puts the launcher and the tool's root. Read by
    /// `detect` (expanded against `HostEnv.home` and the Cargo home,
    /// `route::expand_route`) and, through the instance's
    /// `exe_path`/`prefix`, by `inventory`.
    pub route: Route,
    /// How the installed version is read. Read by `detect` and
    /// `inventory` (and so by `reconcile`).
    pub version: VersionCmd,
    /// Where the newest published version comes from. Read by
    /// `check_updates`.
    pub latest: Latest,
    /// Whether the tool updates itself in the background when its own
    /// updater is on (claude: yes, VERIFIED in claude.md §5). Read by
    /// `inventory`, into `InstalledArtifact.auto_updates`, whose reader for
    /// a standalone tool, the Updates page's `selfUpdatingHint` sentence,
    /// arrives with Task 10 of the phase 4 step B plan.
    pub self_updates: bool,
    /// The tool's own documented update command, or `None` for a tool that
    /// installs its updates itself and offers nothing Canager may run
    /// (agy: `agy update` is undocumented, takes no options and has never
    /// been run, agy.md §4). `None` puts `UpdateBlocked::SelfUpdatesOnly`
    /// on every update candidate the recipe produces and makes
    /// `plan(Upgrade)` refuse with the same reason (spec §4.4, D5). Read by
    /// `check_updates` and `plan(Upgrade)`.
    pub upgrade: Option<UpgradeCmd>,
    /// How the tool is removed, or `None` when there is no safe way: the
    /// artifact then carries `UninstallBlocked::NoSafeMethod`, the gate
    /// refuses and the page says so (spec §6.1 "Neither"; no first-batch
    /// recipe since step C, the second batch's Ollama.app). Read by
    /// `inventory` (`uninstall_blocked`), `plan(Uninstall)` and `execute`.
    pub uninstall: Option<Uninstall>,
    /// Locks every plan of this tool holds besides its own instance lock,
    /// from what `detect` seated. rustup's is the cargo instance's
    /// (`rustup::extra_locks`, Task 5 of the phase 4 step E plan):
    /// `rustup self update` unlinks and re-copies the binary all thirteen
    /// `$CARGO_HOME/bin` proxies run, `cargo` among them, and `rustup self
    /// uninstall` deletes the `.crates2.json` cargo's inventory reads
    /// (spec §2.4). A tool with its own directory holds nothing else:
    /// `no_extra_locks`. Read by `StandaloneAdapter::locks`, for every
    /// plan.
    pub extra_locks: fn(&Detected) -> Vec<ResourceLock>,
    /// The file-name patterns of the backup copies the tool's own updater
    /// leaves beside its launcher (`~/.local/bin/agy.<time>.old`; spec
    /// §3.5), empty for a tool whose updater leaves none. Read by the
    /// path-list uninstall (`removal::listed_items`, check 5: each match
    /// is moved before the launcher and listed in the preview) and by the
    /// Unknown page's rule 4 (`recipes::backup_globs` →
    /// `Session::scan_unknown`), so a fresh backup is the tool's and not a
    /// stranger while the tool is installed.
    pub backup_globs: &'static [Glob],
}

/// `Recipe.extra_locks` for a tool that touches nothing another source
/// reads: only its own instance lock, which every plan holds anyway.
pub fn no_extra_locks(_: &Detected) -> Vec<ResourceLock> {
    Vec::new()
}

/// The installer's fixed paths. Every path starts with `~/` or
/// `$CARGO_HOME` and is expanded by `route::expand_route` against
/// `HostEnv.home` and the Cargo home `cargo::cargo_home_of` answers --
/// never `std::env::var("HOME")`, which a Finder-launched app cannot be
/// tested against (spec §3.1).
/// `recipes::tests::test_every_recipe_path_is_under_home_or_the_cargo_home`
/// holds every recipe to that, and
/// `recipes::tests::test_a_paths_recipe_names_only_home_paths` holds a
/// recipe with a path-list uninstall to `~/` alone, which is all
/// `route::expand` knows.
#[derive(Debug)]
pub struct Route {
    pub kind: RouteKind,
    /// The launcher: `~/.local/bin/claude`, `$CARGO_HOME/bin/rustup`. The
    /// instance's `exe_path` and the program every plan runs.
    pub launcher: &'static str,
    /// The tool's own root: `~/.local/share/claude`, `$CARGO_HOME`. The
    /// instance's `prefix`; what a `SymlinkIntoRoot` launcher must resolve
    /// into.
    pub root: &'static str,
}

/// How a launcher is recognised as this route's and not a package
/// manager's copy. Read by `route::probe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    /// The launcher is a symbolic link that, fully resolved, lands under
    /// `root` (claude: `~/.local/bin/claude` → `~/.local/share/claude/
    /// versions/<v>`, VERIFIED on this Mac). Dangling, the link's own text
    /// decides, lexically normalised, whether this is the half-uninstalled
    /// `LauncherOnly` state (spec §3.3 step 2).
    SymlinkIntoRoot,
    /// The launcher is a regular file, not a link (rustup:
    /// `$CARGO_HOME/bin/rustup`, an 11 MB Mach-O executable, VERIFIED on
    /// this Mac; agy in step D). Its real path is itself; `root` is the
    /// instance's `prefix` and plays no part in the fingerprint. A link
    /// at that path is not this route's install, and a dangling link at
    /// it is not this install half-removed (no launcher-only state). Its
    /// producers are the `RUSTUP` recipe (Task 6 of the phase 4 step E
    /// plan) and `AGY` (Task 5 of the phase 4 step D plan).
    FlatFile,
}

/// The read-only version command, run against the launcher.
#[derive(Debug)]
pub struct VersionCmd {
    pub args: &'static [&'static str],
    /// Environment added to the version read only -- never to the upgrade
    /// plan: Claude Code is documented to check for updates on startup and
    /// `DISABLE_AUTOUPDATER=1` to stop only that background check, so it
    /// goes on every version read whether or not a bare `--version` would
    /// reach the updater (not observed; spec §3.4). Upgrade adds no
    /// override; the runner still inherits ambient environment. Manual
    /// `claude update` is documented to work with this variable set.
    pub env: &'static [(&'static str, &'static str)],
    pub parse: VersionParse,
}

/// Which token of the version command's output is the version. Read by
/// `latest::parse_version`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionParse {
    /// The first whitespace-separated token of the first non-empty line:
    /// `2.1.281 (Claude Code)` → `2.1.281`.
    FirstToken,
    /// The second token: `rustup 1.29.1 (d95a37b6a 2026-08-13)` → `1.29.1`
    /// (rustup; grok's `grok 1.0.41 (4220f3b224a6)` in step D). The two
    /// `info:` lines rustup prints after that go to stderr, which the
    /// version read never looks at (recorded as
    /// `adapters/fixtures/standalone-rustup/<v>/version-stderr.txt`). Its
    /// producers are the `RUSTUP` and `GROK` recipes.
    SecondToken,
}

/// Where the newest published version is read from. Only VERIFIED
/// endpoints (spec D4), or the tool's own VERIFIED read-only check
/// (`Command`, which makes its own connection); every host here is on
/// `ALLOWED_HTTPS_HOSTS`
/// (`recipes::tests::test_every_recipe_latest_url_is_an_allowed_https_host`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latest {
    /// Claude Code only: `GET {base}/{channel}`, where `channel` is
    /// `latest` or `stable` as `~/.claude/settings.json`'s
    /// `autoUpdatesChannel` says (`latest::claude_channel`; anything but
    /// `"stable"` is `latest`). Both pointers answer one bare version
    /// (claude.md §4, VERIFIED). That the channel setting maps onto these
    /// two pointer URLs is inferred, not decompiled (claude.md §4,
    /// UNVERIFIED): a wrong inference costs a `stable` user a `latest`
    /// badge whose `claude update` then reports up to date --
    /// `UnchangedAfterUpgrade`, which is the truth. Tool-specific on
    /// purpose: one tool needs it, and a generic "read a JSON key" source
    /// would be a mechanism with one user.
    ClaudeChannel { base: &'static str },
    /// `GET url`, whose body is TOML with a top-level `version = '…'`:
    /// rustup's `release-stable.toml`, the file `rustup self update`
    /// itself reads (`DEFAULT_UPDATE_ROOT` in rustup 1.29.1's
    /// `src/cli/self_update.rs`; rustup.md §6, VERIFIED). Parsed by
    /// `latest::parse_release_stable_toml`, read by
    /// `StandaloneAdapter::published`. Its producer is the `RUSTUP`
    /// recipe (Task 6 of the phase 4 step E plan).
    HttpTomlVersion { url: &'static str },
    /// `GET url`, a JSON object whose top-level `field` is the newest
    /// version (agy's `manifests/darwin_arm64.json`, VERIFIED live, agy.md
    /// §4). Made only on the architectures `latest::MANIFEST_VERIFIED_ARCHES`
    /// names: on an Intel Mac, or under Rosetta, the row is "could not
    /// check" with the reason and no request is sent (spec §3.1; the amd64
    /// manifest is §十一's). Read by `StandaloneAdapter::published`
    /// (`latest::parse_json_field`). Its producer is the `AGY` recipe.
    HttpJsonField {
        url: &'static str,
        field: &'static str,
    },
    /// `<launcher> args`, whose stdout is a JSON object: `latest_field` the
    /// newest version, `available_field` whether the tool calls it an
    /// update -- trusted as answered, never compared (spec §4.3) -- and
    /// `error_field`, when the tool has one, the key whose non-null value
    /// means the check itself failed (grok's `"error":null` on success,
    /// grok.md §3): then the row is "could not check" with that text, never
    /// "up to date". Only a subcommand whose own `--help` says it installs
    /// nothing may be named here (grok's `update --check --json`: "Check for
    /// updates without installing", grok.md §4;
    /// `recipes::tests::test_every_command_latest_source_only_checks`),
    /// since it runs on every refresh. Read by `StandaloneAdapter::published`
    /// (`latest::parse_update_check`). Its producer is the `GROK` recipe.
    Command {
        args: &'static [&'static str],
        timeout_secs: u64,
        latest_field: &'static str,
        available_field: &'static str,
        error_field: Option<&'static str>,
    },
}

/// The tool's own update command, run against the launcher through
/// `run_plan`. Read by `plan(Upgrade)`.
#[derive(Debug)]
pub struct UpgradeCmd {
    pub args: &'static [&'static str],
    pub timeout_secs: u64,
    pub cancel: CancelPolicy,
}

/// How a tool is removed (phase 4 spec §6.1). Only the arms something
/// produces exist: `Paths` (Claude Code's list, step C) and `Command`
/// (rustup's own `self uninstall`, gated to the standard layout and with a
/// read-only preview of what it deletes; its second lock is
/// `Recipe.extra_locks`, on the recipe because the upgrade holds it too).
#[derive(Debug)]
pub enum Uninstall {
    /// No command exists; the vendor's own instructions are a list of
    /// paths. `removal::execute_removal` moves each of `remove` to the
    /// Trash in this order -- the launcher last, so a run that stops
    /// partway leaves the one state a second run finishes (spec §6.2) --
    /// and `keep` is listed in the preview so the user sees what stays.
    /// Where the list comes from is the recipe constant's doc comment and
    /// the fixture README, not a field: nothing in production would read
    /// it (spec §十三 #8/#36). Read by `removal::plan_removal`,
    /// `removal::execute_removal` and the invariants tests in
    /// `recipes.rs`.
    Paths {
        remove: &'static [RemoveSpec],
        keep: &'static [KeepSpec],
    },
    /// The tool's own official uninstall command (rustup: `self uninstall
    /// -y`), run against the launcher through `run_plan` unchanged; when
    /// it may be offered is said by `blocked`, and what it removes by
    /// `warnings`, since the argv alone cannot (spec §6.4). Read by
    /// `plan(Uninstall)` (`StandaloneAdapter::command_uninstall_plan`),
    /// `inventory` (`rows`) and `execute` (the gate, asked again before
    /// the spawn); produced by `recipes::RUSTUP`.
    Command(CommandUninstall),
}

/// `Uninstall::Command`'s data. `cancel` is `NoCancel` for rustup: its
/// uninstall removes directories one after another and a kill partway
/// leaves a broken Rust. The preview runs no command: everything it says
/// comes from what `detect` seated and the disk.
#[derive(Debug)]
pub struct CommandUninstall {
    pub args: &'static [&'static str],
    pub timeout_secs: u64,
    pub cancel: CancelPolicy,
    /// Whether this install may be offered the command at all, from the
    /// seat and the disk: `Some` puts its `reason` on the artifact
    /// (`inventory`), which the gate refuses and the page hides the
    /// button for, and -- asked once more by `execute` right before the
    /// spawn, since the command deletes its folders wherever they resolve
    /// at run time -- stops the run with `Fault::PathChanged` naming its
    /// `path`. `plan(Uninstall)` does not read it: `preview` asks the same
    /// gate and answers from that one reading. rustup's is
    /// `rustup::uninstall_blocked`: `NoSafeMethod` unless Rust lives in
    /// its standard folders (plan ruling 18).
    pub blocked: fn(&Detected) -> Option<GateRefusal>,
    /// The preview, from what `detect` seated and the disk: the gate's
    /// refusal, which `plan(Uninstall)` refuses with, or the warnings the
    /// dialog lists. One function and one reading of the disk, so the
    /// gate cannot pass on one look while the warnings come from another
    /// -- a layout that changed between two readings gave a plan with no
    /// warnings at all (step E's whole-step review). Read by
    /// `plan(Uninstall)`; rustup's is `rustup::uninstall_preview`.
    pub preview: fn(&Detected) -> Result<Vec<Warning>, GateRefusal>,
}

/// What a `Command` uninstall's gate answers when it refuses
/// (`CommandUninstall.blocked`'s `Some`, `CommandUninstall.preview`'s
/// `Err`). `reason` is what the artifact carries (`inventory`) and what
/// `plan(Uninstall)` refuses with (`AdapterError::UninstallBlocked`).
/// `path` is the folder, or the link at the top of one, that the rule
/// failed at, absolute, for `execute`: it asks the gate again right
/// before the spawn, and when a gate the preview passed refuses now, that
/// path is no longer where, or what, the preview said, and the run stops
/// with `Fault::PathChanged` naming it. Produced by
/// `rustup::standard_roots`, through `rustup::uninstall_blocked` and
/// `rustup::uninstall_preview`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateRefusal {
    pub reason: UninstallBlocked,
    pub path: PathBuf,
}

/// One path a path-list uninstall moves to the Trash.
#[derive(Debug)]
pub struct RemoveSpec {
    /// `~/…`, expanded by `route::expand` against the detected home.
    /// Never directly in the home folder or in one of `SHARED_FOLDERS`
    /// (check 1; `recipes::tests`), and, on the disk, reached only through
    /// real folders (`removal::check_item`).
    pub path: &'static str,
    /// What must be there for the move to be safe (check 4).
    pub expect: Expect,
    /// What it is, for the preview's sentence (`Warning::WillTrash`).
    pub what: RemovedWhat,
    /// Whether its absence is fine (a cache the tool may not have made):
    /// skipped silently when missing. Never the launcher
    /// (`recipes::tests` hold every recipe to that).
    pub optional: bool,
}

/// The folders a `RemoveSpec.path` must never sit directly in, besides the
/// home folder itself: the ones directly under it that many tools share
/// (spec §6.3 check 1's never-list). Moving `~/.local/bin` or
/// `~/.config/fish` whole would take other tools' files with it. Read by
/// `removal::plan_removal` (check 1, against the resolved folder) and by
/// `recipes::tests` (against the recipe's spelling).
pub const SHARED_FOLDERS: [&str; 5] = [".local", ".config", ".cache", "Library", ".cargo"];

/// What check 4 requires at a `RemoveSpec.path` (spec §6.3) -- at the
/// path itself: every folder above it must be a real folder whatever it
/// expects (the ancestry rule, `removal::check_item`). The two link kinds
/// are the only listed paths that may be links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// The launcher of a link-shaped route: a symbolic link whose own text
    /// points into the recipe's root and which resolves there, or,
    /// dangling, whose own text points into it (the launcher-only state):
    /// exactly as `route::probe` decides for the launcher. Only the
    /// launcher (`recipes::tests`): a root may be the very folder the
    /// uninstall keeps, so for any other link "points into the root" says
    /// nothing about whose it is.
    SymlinkIntoRoot,
    /// Another symbolic link to the launcher's program -- grok's
    /// `~/.grok/bin/agent`, and the fallback links its installer may make
    /// in `~/.local/bin` -- checked against the program, never merely the
    /// root: grok's root, `~/.grok`, is also what its uninstall keeps, with
    /// the user's plugins and skills in it. The link's own text must land
    /// inside `program`, the folder the program's files are in, or name one
    /// of `via`, the tool's own links such a link may point at instead; and
    /// where it resolves, if it does, must be inside `program` or be the
    /// file the launcher runs (`route::leads_to_program`). Dangling -- once
    /// `program` is in the Trash -- the text alone decides, as for the
    /// launcher. Both are `~/` paths of the same list: `program` a folder it
    /// requires and moves as the program, `via` the launcher or another of
    /// its links to the program (`recipes::tests`). Produced by
    /// `recipes::GROK`; read by `removal::check_item`.
    SymlinkToProgram {
        program: &'static str,
        via: &'static [&'static str],
    },
    /// A real directory, not a link.
    Dir,
    /// A regular file, not a link: Antigravity's launcher (`~/.local/bin/agy`,
    /// the whole program), grok's fish completion file, and every backup
    /// copy a `Glob` matches (`removal::listed_items`).
    File,
}

/// One path a path-list uninstall leaves alone, named in the preview so
/// the user knows their settings stay (`Warning::WillKeep`); listed only
/// when it exists. `path` is `~/…` and protected by the checks
/// (`removal::kept_places`, `disturbed`) -- except when `what` is
/// `KeptWhat::OutsideHome`: then it is an absolute path outside the home
/// folder (`/usr/local/bin/grok`), reported when it exists and never
/// protected (`removal::outside_home_keeps`): a fallback link *into* the
/// program folder would otherwise refuse the uninstall it exists for
/// (`recipes::tests` hold the two spellings apart).
#[derive(Debug)]
pub struct KeepSpec {
    pub path: &'static str,
    pub what: KeptWhat,
}
