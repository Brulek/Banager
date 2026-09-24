//! One tool = one `Recipe`: all `'static` data, no trait objects, one
//! table to read. Every field's doc names the reader that consumes it; a
//! field without a reader is not added (phase 4 spec §3.1, §十).
//!
//! Only the shapes this step produces exist here. Step C adds the
//! uninstall method (`uninstall: Option<Uninstall>`), step D `backup_globs`,
//! a `FlatFile` route, a `SecondToken` version parse, the other `Latest`
//! sources and an optional `upgrade` (agy updates itself only), step E
//! `$CARGO_HOME` paths. A variant or field defined before anything
//! produces it is this project's most common defect (spec §十三 #41).

use crate::model::CancelPolicy;

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
    /// `detect` (expanded against `HostEnv.home`) and, through the
    /// instance's `exe_path`/`prefix`, by `inventory`.
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
    /// The tool's own documented update command. Read by `plan(Upgrade)`.
    pub upgrade: UpgradeCmd,
}

/// The installer's fixed paths. Every path starts with `~/` and is expanded
/// by `route::expand` against `HostEnv.home` -- never `std::env::var("HOME")`,
/// which a Finder-launched app cannot be tested against (spec §3.1).
/// `recipes::tests::test_every_recipe_path_is_under_home` holds every
/// recipe to that.
#[derive(Debug)]
pub struct Route {
    pub kind: RouteKind,
    /// The launcher: `~/.local/bin/claude`. The instance's `exe_path` and
    /// the program every plan runs.
    pub launcher: &'static str,
    /// The tool's own root: `~/.local/share/claude`. The instance's
    /// `prefix`; what the launcher must resolve into.
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
}

/// Where the newest published version is read from. Only VERIFIED
/// endpoints (spec D4); every host here is on `ALLOWED_HTTPS_HOSTS`
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
}

/// The tool's own update command, run against the launcher through
/// `run_plan`. Read by `plan(Upgrade)`.
#[derive(Debug)]
pub struct UpgradeCmd {
    pub args: &'static [&'static str],
    pub timeout_secs: u64,
    pub cancel: CancelPolicy,
}
