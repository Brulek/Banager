//! The tools, as data. One `pub static` per tool, `RECIPES` listing the
//! registered ones in registration order; `StandaloneAdapter::new` builds
//! one adapter per entry (`all()`). Each registered tool also has a meta
//! TOML (`adapters/meta/standalone-<id>.toml`) and a recorded fixture
//! directory (`adapters/fixtures/standalone-<id>/`), and the tests below
//! hold every registered constant to the invariants the code relies on.

use super::recipe::{
    no_extra_locks, CommandUninstall, Expect, KeepSpec, Latest, Recipe, ReleaseLink, RemoveSpec,
    Route, RouteKind, SelfUpdates, Uninstall, UpgradeCmd, VersionCmd, VersionParse, VersionSource,
};
use super::rustup;
use crate::adapters::cargo::RUSTUP_AUTO_INSTALL_OFF;
use crate::model::{CancelPolicy, KeptWhat, RemovedWhat};
use crate::scan::Glob;

/// Claude Code, the native install (`curl -fsSL https://claude.ai/install.sh
/// | bash`, run by the user; Banager never runs it).
///
/// Every value here is from `.superpowers/phase4/claude.md` (VERIFIED on
/// this Mac or in Anthropic's own documentation, 2026-09-24, unless
/// noted) and from the recording in
/// `adapters/fixtures/standalone-claude/<version>/`:
/// - the launcher `~/.local/bin/claude` is a symbolic link into
///   `~/.local/share/claude/versions/<version>`, one full executable per
///   installed version, kept after upgrades (§2a);
/// - `claude --version` prints `<version> (Claude Code)` (§1). Anthropic
///   documents that Claude Code checks for updates on startup and that
///   `DISABLE_AUTOUPDATER` stops only that background check (§5, doc
///   text); whether `--version` alone reaches the updater was not
///   observed, so the variable is set on every version read regardless,
///   while the upgrade plan adds no override (spec §3.4); the runner
///   inherits ambient environment, and manual updates still work with
///   `DISABLE_AUTOUPDATER=1` (§5);
/// - the newest published version is the channel pointer
///   `downloads.claude.ai/claude-code-releases/<latest|stable>`, a bare
///   version each; install.sh itself reads the `latest` one (§4). The
///   `stable` pointer is behind `latest` (2.1.273 vs 2.1.281 on
///   2026-09-24), which is why `check_updates` compares rather than tests
///   inequality;
/// - it updates itself in the background when its updater is on (§5);
///   the row says so unless `~/.claude/settings.json` turns that updater
///   off (`SelfUpdates::UnlessOffInClaudeSettings`,
///   `latest::claude_updater_off`: r39 S2);
/// - `claude update` (alias `upgrade`, no options) is the documented
///   updater (§6). The install script stages its download under
///   `~/.claude/downloads`, checks it against the release manifest's
///   checksum, and only then runs the new binary's own `install`, which
///   sets up the launcher (install.sh, read directly, §4 and §6);
///   `claude update` itself is compiled and its steps were not read
///   (§6, §8), so Banager assumes nothing about interruption:
///   `KillThenReconcile`, no claim in the preview, and stopped upgrades
///   remain `Unconfirmed` even if the version changes. After exit 0, a
///   readable version gates success. 1800 s is spec §4.1's upgrade
///   budget; the binary is about 220 MB.
///
/// There is no `claude uninstall` subcommand (§2a, `claude --help`). The
/// removal list is Anthropic's own "Uninstall Claude Code → Native"
/// instructions at code.claude.com/docs/en/setup (§7, VERIFIED: exactly
/// `rm -f ~/.local/bin/claude` and `rm -rf ~/.local/share/claude`), plus
/// `~/.claude/downloads`, the staging directory install.sh names as
/// `DOWNLOAD_DIR` for the native route's downloads (§2a, VERIFIED from
/// install.sh; optional -- it may not be there). The kept paths are the
/// same page's separate, explicitly optional step ("Removing configuration
/// files will delete all your settings…"; the VS Code extension, the
/// JetBrains plugin and the desktop app write to `~/.claude/` too, §7):
/// `~/.claude` and `~/.claude.json`, which Banager keeps (spec Q4) -- of
/// `~/.claude` it moves only `downloads`, the cache above. Order: program
/// files, cache, the launcher last (spec §6.2).
pub static CLAUDE: Recipe = Recipe {
    id: "claude",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-claude.toml"),
    route: Route {
        kind: RouteKind::SymlinkIntoRoot,
        launcher: "~/.local/bin/claude",
        root: "~/.local/share/claude",
    },
    version: VersionSource::Command(VersionCmd {
        args: &["--version"],
        env: &[("DISABLE_AUTOUPDATER", "1")],
        parse: VersionParse::FirstToken,
    }),
    latest: Latest::ClaudeChannel {
        base: "https://downloads.claude.ai/claude-code-releases",
    },
    self_updates: SelfUpdates::UnlessOffInClaudeSettings,
    upgrade: Some(UpgradeCmd {
        args: &["update"],
        timeout_secs: 1800,
        cancel: CancelPolicy::KillThenReconcile,
    }),
    uninstall: Some(Uninstall::Paths {
        remove: &[
            RemoveSpec {
                path: "~/.local/share/claude",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: false,
            },
            RemoveSpec {
                path: "~/.claude/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Cache,
                optional: true,
            },
            RemoveSpec {
                path: "~/.local/bin/claude",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            },
        ],
        keep: &[
            KeepSpec {
                path: "~/.claude",
                what: KeptWhat::SettingsAndHistory,
            },
            KeepSpec {
                path: "~/.claude.json",
                what: KeptWhat::Settings,
            },
        ],
    }),
    extra_locks: no_extra_locks,
    backup_globs: &[],
    other_commands: &[],
};

/// Antigravity CLI (`agy`), Google's terminal agent, installed by its own
/// script (`curl -fsSL https://antigravity.google/cli/install.sh | bash`,
/// run by the user; Banager never runs it).
///
/// Every value here is from `.superpowers/phase4/agy.md` (VERIFIED on this
/// Mac, in the install script read in full, or in Google's own
/// documentation, 2026-09-24, unless noted), from the phase 4 spec's agy
/// rows (§3.4, §3.5, §6.3), which re-checked the version read on 1.2.10,
/// and from the recording in `adapters/fixtures/standalone-agy/<version>/`
/// (the version line, the manifest, the updater's status file and the
/// layout; 1.2.11, 2026-09-26):
/// - the launcher `~/.local/bin/agy` is a regular Mach-O file (176 MB on
///   this Mac), the whole program; the installer copies it there
///   (`TARGET_DIR=$HOME/.local/bin`, `BINARY_PATH=$TARGET_DIR/agy`, §3a).
///   The root `~/.gemini/antigravity-cli` holds its conversations, logs,
///   cache, builtin skills and updater state together (§2); `~/.gemini`
///   itself is shared with Gemini CLI and is never touched. The Homebrew
///   cask's `agy` is a link into its Caskroom and is Homebrew's row (§3b);
/// - `agy --version` prints one bare version (`1.2.9`, §4). It is read with
///   `AGY_CLI_DISABLE_AUTO_UPDATE=true`, the switch Google documents for
///   its background updater (§4, doc text). On 1.2.10 (the spec) and on
///   1.2.11 (the recording, whose README has the numbers) `--version` did
///   not reach the updater at all -- no new log file, `update_status.json`
///   untouched, no updater process (spec §3.4, §十三 #10, which make every
///   recording of a `verified_versions` entry repeat that observation and
///   write it into its README) -- so the switch is a belt on top; the
///   spawns agy.md §4 logged came from runs with a prompt (spec §3.4);
/// - the newest published version is the `version` of the JSON manifest
///   the installer and the updater both read,
///   `…/manifests/darwin_arm64.json` (§3a, §4; VERIFIED live). Only that
///   file was fetched, so the lookup is made on Apple silicon only
///   (`latest::manifest_arch_allowed`); the amd64 manifest is spec §十一's;
/// - it installs its updates itself, in the background, at most every 15
///   minutes (§4: the documented debounce and this Mac's own log), so
///   `self_updates` is `Yes` and, since `agy update` is undocumented, takes
///   no options and has never been run (§4), there is no `upgrade`: every
///   newer version is `UpdateBlocked::SelfUpdatesOnly` -- a badge, no
///   button, and a sentence saying to open it once (spec §4.4, D5, Q3);
/// - there is no vendor uninstall document and no `agy uninstall` (§5).
///   The list combines the installer's own path with the cask's `zap`
///   (which trashes only `~/.gemini/antigravity-cli`) -- a synthesis, as
///   agy.md §5 says of its own list: the launcher is the list's one path
///   -- it is the whole program, listed as `RemovedWhat::Launcher`, "the
///   command itself" (phase 4 step D plan ruling 2) -- and goes after any
///   `agy.<time>.old` its updater left beside it (`backup_globs`, spec
///   §3.5). Kept, and said when present: the root (`ToolState`: no vendor
///   list says which of its folders could go alone, and the cask treats it
///   as one, spec §十三 #24), `~/.cache/antigravity` (the installer's
///   staging folder, directly in `~/.cache`, which check 1 never moves
///   from -- kept and said rather than excepted, phase 4 step D plan ruling
///   1), and the two shell files the installer added its PATH line to
///   (each marked `# Added by Antigravity CLI installer`, §2).
///
/// Registered in `RECIPES` together with what `fixtures_layout_test` and
/// `what_we_run_test` demand of a registered source: the recording in
/// `adapters/fixtures/standalone-agy/<version>/` and the
/// `## Antigravity CLI` section of `docs/what-we-run.md`.
pub static AGY: Recipe = Recipe {
    id: "agy",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-agy.toml"),
    route: Route {
        kind: RouteKind::FlatFile,
        launcher: "~/.local/bin/agy",
        root: "~/.gemini/antigravity-cli",
    },
    version: VersionSource::Command(VersionCmd {
        args: &["--version"],
        env: &[("AGY_CLI_DISABLE_AUTO_UPDATE", "true")],
        parse: VersionParse::FirstToken,
    }),
    latest: Latest::HttpJsonField {
        url: "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json",
        field: "version",
    },
    self_updates: SelfUpdates::Yes,
    upgrade: None,
    uninstall: Some(Uninstall::Paths {
        remove: &[RemoveSpec {
            path: "~/.local/bin/agy",
            expect: Expect::File,
            what: RemovedWhat::Launcher,
            optional: false,
        }],
        keep: &[
            KeepSpec {
                path: "~/.gemini/antigravity-cli",
                what: KeptWhat::ToolState,
            },
            KeepSpec {
                path: "~/.cache/antigravity",
                what: KeptWhat::InstallerCache,
            },
            KeepSpec {
                path: "~/.zshrc",
                what: KeptWhat::ShellConfigLines,
            },
            KeepSpec {
                path: "~/.zprofile",
                what: KeptWhat::ShellConfigLines,
            },
        ],
    }),
    extra_locks: no_extra_locks,
    backup_globs: &[Glob {
        dir: "~/.local/bin",
        prefix: "agy.",
        suffix: ".old",
        what: RemovedWhat::Backups,
    }],
    other_commands: &[],
};

/// Grok Build (`grok`), xAI's terminal agent, installed by its own script
/// (`curl -fsSL https://x.ai/cli/install.sh | bash`, run by the user;
/// Banager never runs it).
///
/// Every value here is from `.superpowers/phase4/grok.md` (VERIFIED on
/// this Mac, in the install script, or in the README the tool ships,
/// 2026-09-24, unless noted), from the phase 4 spec's grok rows (§3.5,
/// §五, §6.3), and from the recording in
/// `adapters/fixtures/standalone-grok/<version>/` (the version line, grok's
/// own check and the layout; 1.0.41, 2026-09-26):
/// - the launcher `~/.grok/bin/grok` is a *relative* symbolic link,
///   `../downloads/grok-<version>-macos-aarch64`, into the root `~/.grok`
///   (spec §3.5, VERIFIED); `bin/agent` is a second link to the same file
///   (§1, §2). Old downloads stay in `downloads/` after an update (three
///   on this Mac, ~400 MB). The Homebrew cask `grok-build` puts its links
///   in `/opt/homebrew/bin` and is Homebrew's row; the Homebrew *formula*
///   named `grok` is an unrelated regex library (§2, §7);
/// - `grok --version` prints `grok 1.0.41 (4220f3b224a6)` (§1): the second
///   token, no environment (none is documented). Whether `--version` runs
///   grok's launch-time updater, and whether that updater installs or only
///   checks, are both UNVERIFIED (§5; phase 4 step D plan ruling 16). The
///   recording watched `~/.grok/version.json`'s mtime, the `bin/` links and
///   `downloads/` around the read it holds, and would have stopped had a
///   link or `downloads/` changed: on 1.0.41 none of the three moved, and
///   nothing under `~/.grok` was written by the read;
/// - the newest published version is asked of grok itself: `update --check
///   --json`, whose `--help` says "Check for updates without installing"
///   (§3, §4; run on this Mac) and which prints `{"currentVersion":…,
///   "latestVersion":…,"updateAvailable":…,…,"error":null}`.
///   `updateAvailable` is believed and `latestVersion` shown (spec §4.3);
///   a non-null `error` makes the row "could not check" with that text
///   (ruling 10 of the phase 4 step D plan); 60 s. The check writes inside
///   `~/.grok` when it runs -- grok's writes, not Banager's, once per
///   refresh: on the recording it replaced `version.json` with the time of
///   the check (`checked_at`, §1), added two lines to its log
///   `logs/unified.jsonl` and touched the user guide it ships in
///   `docs/user-guide/` (the files' modification times moved), and the
///   trust file's Grok Build section says so;
/// - `auto_update = true` in its config means "check for updates on
///   launch" (§5); whether it *installs* one is UNVERIFIED, so the row is
///   not called self-updating (spec §4.4, §十三 #25);
/// - `grok update` is the documented upgrade (§4): a new file in
///   `downloads/` and a re-pointed link, the old binary left in place.
///   1800 s, `KillThenReconcile`. How it behaves with its input closed
///   has not been observed (spec §五): the author records it on a CI
///   runner before this step merges (the step D plan's "pre-merge
///   verification");
/// - there is no `grok uninstall` and no vendor uninstall document (§6);
///   the de-facto `rm -rf ~/.grok` would take the login, sessions and
///   memory, which spec Q4 keeps. The list is the README's "File
///   Locations" table plus the install script: the two optional fallback
///   links the installer makes when `~/.grok/bin` is not on PATH *first*
///   (their link text is UNVERIFIED -- one hop or two -- so they go while
///   every folder it could pass through is still on the disk; a
///   precaution, since check 4 would accept them dangling too, by their
///   text, `GROK_PROGRAM_LINK`; step D plan ruling 3), then `downloads/`
///   (the program), `bundled/` and `completions/` (optional), the fish
///   completion the installer also writes (optional; spec §十三 #17), then
///   the two links the installer put in `~/.grok/bin`: `agent` (optional, a
///   second name for the same command) and `grok` -- the launcher -- last.
///   The folder `~/.grok/bin` itself is not moved (spec §6.3 listed it
///   whole; step D plan ruling 4): it is on the user's PATH, so a script of
///   their own may sit in it, and it stays inside the kept `~/.grok`, empty
///   unless they put something there. Kept, and said when present:
///   `~/.grok` (`config.toml`, `auth.json`, `sessions/`, `memory/`,
///   `skills/`, `plugins/`), `~/.zshrc`, where the installer writes its
///   marked PATH block for zsh, macOS's default shell (§2), and a link the
///   installer may have put in `/usr/local/bin`, reported only when it is a
///   link into `~/.grok` that the moves leave leading nowhere -- to the
///   download, or through `~/.grok/bin/grok` -- never Homebrew's, another
///   CLI's, or one to a plugin's program in the `~/.grok` this list keeps
///   (spec §6.3; step D plan ruling 6; `removal::dead_after`).
///
/// Registered in `RECIPES` together with what `fixtures_layout_test` and
/// `what_we_run_test` demand of a registered source: the recording in
/// `adapters/fixtures/standalone-grok/<version>/` and the `## Grok Build`
/// section of `docs/what-we-run.md`.
///
/// The three links besides the launcher are `GROK_PROGRAM_LINK`s: each is
/// moved only when it leads to the program in `~/.grok/downloads`, never
/// merely because it points into `~/.grok` -- the folder this list keeps,
/// plugins and skills included.
pub static GROK: Recipe = Recipe {
    id: "grok",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-grok.toml"),
    route: Route {
        kind: RouteKind::SymlinkIntoRoot,
        launcher: "~/.grok/bin/grok",
        root: "~/.grok",
    },
    version: VersionSource::Command(VersionCmd {
        args: &["--version"],
        env: &[],
        parse: VersionParse::SecondToken,
    }),
    latest: Latest::Command {
        args: &["update", "--check", "--json"],
        timeout_secs: 60,
        latest_field: "latestVersion",
        available_field: "updateAvailable",
        error_field: Some("error"),
    },
    self_updates: SelfUpdates::No,
    upgrade: Some(UpgradeCmd {
        args: &["update"],
        timeout_secs: 1800,
        cancel: CancelPolicy::KillThenReconcile,
    }),
    uninstall: Some(Uninstall::Paths {
        remove: &[
            RemoveSpec {
                path: "~/.local/bin/grok",
                expect: GROK_PROGRAM_LINK,
                what: RemovedWhat::Launcher,
                optional: true,
            },
            RemoveSpec {
                path: "~/.local/bin/agent",
                expect: GROK_PROGRAM_LINK,
                what: RemovedWhat::Launcher,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: false,
            },
            RemoveSpec {
                path: "~/.grok/bundled",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/completions",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.config/fish/completions/grok.fish",
                expect: Expect::File,
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/bin/agent",
                expect: GROK_PROGRAM_LINK,
                what: RemovedWhat::Launcher,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/bin/grok",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            },
        ],
        keep: &[
            KeepSpec {
                path: "~/.grok",
                what: KeptWhat::SettingsAndHistory,
            },
            KeepSpec {
                path: "~/.zshrc",
                what: KeptWhat::ShellConfigLines,
            },
            KeepSpec {
                path: "/usr/local/bin/grok",
                what: KeptWhat::OutsideHome,
            },
            KeepSpec {
                path: "/usr/local/bin/agent",
                what: KeptWhat::OutsideHome,
            },
        ],
    }),
    extra_locks: no_extra_locks,
    backup_globs: &[],
    // `~/.grok/bin/agent`: the installer's second name for the same
    // download (spec §3.5, the recorded `layout.txt`).
    other_commands: &["agent"],
};

/// What grok's links besides its launcher -- `~/.grok/bin/agent` and the
/// two fallback links in `~/.local/bin` -- must be (check 4,
/// `Expect::SymlinkToProgram`): a link to the program in
/// `~/.grok/downloads`, straight there, as the installer's `bin/grok` and
/// `bin/agent` point (`../downloads/grok-<version>-macos-aarch64`: spec
/// §3.5, and the recorded `layout.txt`), or through one of those two links,
/// since whether a fallback link's text takes one hop or two is unverified
/// (grok.md §2). "Into
/// `~/.grok`" would not do: `~/.grok` is what this uninstall keeps, and a
/// link of the user's to a plugin's or a skill's program there, or to a
/// script of theirs in `~/.grok/bin`, is not grok's -- such a link is kept
/// and said (`KeptWhat::NotOurs`), and still works afterwards.
const GROK_PROGRAM_LINK: Expect = Expect::SymlinkToProgram {
    program: "~/.grok/downloads",
    via: &["~/.grok/bin/grok", "~/.grok/bin/agent"],
};

/// rustup, the Rust toolchain installer, installed by its own script
/// (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`,
/// run by the user; Banager never runs it).
///
/// Every value here is from `.superpowers/phase4/rustup.md` (VERIFIED on
/// this Mac or in rustup's own source at tag 1.29.1, 2026-09-24/25,
/// unless noted); the meta TOML's `verified_versions` is what
/// `RUSTUP_AUTO_INSTALL=0 rustup --version` printed on this Mac on
/// 2026-09-26, and the recording in
/// `adapters/fixtures/standalone-rustup/<version>/` holds that line, its
/// stderr, the release file, the toolchain names and the layout:
/// - the launcher `$CARGO_HOME/bin/rustup` is a regular Mach-O file (11 MB
///   on this Mac); the thirteen proxies beside it (`cargo`, `rustc`,
///   `rustfmt`, …) are relative symlinks to it (§2; unknown-scan.md §2),
///   which the Unknown page's rule 1 attributes. The root is the Cargo
///   home: Banager reads nothing under `RUSTUP_HOME` except, during the
///   uninstall preview, the names in its `toolchains/` (spec §2.2, §3.2);
/// - `rustup --version` prints `rustup <version> (<hash> <date>)` on
///   stdout, and two `info:` lines on stderr that are never read (§3;
///   recorded as `version-stderr.txt`). It runs with
///   `RUSTUP_AUTO_INSTALL=0`: 1.29.1's `display_version`
///   (rustup_mode.rs:1819-1837) resolves the active toolchain and, with
///   none active and auto-install on (the default, config.rs:435-441),
///   installs one -- a download during a refresh. With the switch it
///   says `info: no rustc is currently active` and exits 0. Two side
///   effects of any rustup invocation remain and the trust file says so:
///   `Cfg::from_env` creates `$RUSTUP_HOME` when it is missing
///   (config.rs:321-323), and `cleanup_self_updater` deletes a leftover
///   `$CARGO_HOME/bin/rustup-init` (self_update.rs:1314-1323) -- which is
///   why a refresh must never run this read while rustup's own update
///   holds its locks (`Session::refresh_round`, plan ruling 19, Task 7 of
///   the phase 4 step E plan);
/// - the newest published version is `version = '…'` in
///   `static.rust-lang.org/rustup/release-stable.toml`, the file `rustup
///   self update` itself reads (`DEFAULT_UPDATE_ROOT`, §6);
/// - it does not update itself on its own (spec §3.5): rustup updates
///   itself only as part of `rustup update` and `rustup toolchain
///   install` (`SelfUpdateMode::update`, rustup_mode.rs:1042-1090), which
///   Banager never runs;
/// - `rustup self update` (never `rustup update`, which updates the
///   toolchains and, interrupted, leaves them half installed:
///   rust-lang/rustup#4724, §7) is `NoCancel` with the cargo instance's
///   lock: `install_bins` (1.29.1 `src/cli/self_update.rs:771-785`)
///   unlinks the running `rustup` and then copies the new one in, and in
///   between all thirteen proxies -- the cargo instance's `cargo` among
///   them -- are dangling. 600 s: one 11 MB download;
/// - `rustup self uninstall -y` is the official uninstall (§8; `-y` skips
///   the confirmation an EOF on stdin would otherwise decline), `NoCancel`
///   for the same reason, with the same lock (it deletes the
///   `.crates2.json` cargo's inventory reads). It is offered only when
///   `CARGO_HOME` and `RUSTUP_HOME` resolve to `~/.cargo` and `~/.rustup`
///   and both are real directories (`rustup::uninstall_blocked`, plan
///   ruling 18): 1.29.1's `uninstall()` removes both homes whole,
///   wherever they point, and never to the Trash. The preview runs no
///   command; it (`rustup::uninstall_preview`) asks that same gate once
///   and builds its warnings from the roots that one answer named, out of
///   the `toolchains/` listing, a listing of `$CARGO_HOME/bin`,
///   `.crates2.json`, Homebrew's Cellar and eight startup files -- read
///   from 1.29.1's source, which removes the whole Cargo home, every
///   program in its `bin/` included (`rustup.rs`'s module doc has the
///   lines). `--no-modify-path` is not passed (spec Q6): rustup removing
///   its own startup line beats leaving one that errors on every new
///   terminal;
/// - both commands run with Banager's own environment: only `PATH` and
///   the proxy and mirror settings (`login_path::IMPORTED`, none of them
///   a folder) are taken from the login shell (`runner::login_path`), and the runner
///   passes the rest as inherited. A `RUSTUP_HOME` or `CARGO_HOME` exported only in
///   a shell startup file is not seen by Banager or by the rustup it
///   runs -- the two agree, which is what the gate relies on -- so the
///   preview and the uninstall act on the default folders, and a Rust
///   kept only where the shell says is left alone, not deleted (plan
///   ruling 17).
///
/// Registered in `RECIPES` together with what `fixtures_layout_test` and
/// `what_we_run_test` demand of a registered source: the recording in
/// `adapters/fixtures/standalone-rustup/<version>/` and the `## rustup`
/// section of `docs/what-we-run.md`.
pub static RUSTUP: Recipe = Recipe {
    id: "rustup",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-rustup.toml"),
    route: Route {
        kind: RouteKind::FlatFile,
        launcher: "$CARGO_HOME/bin/rustup",
        root: "$CARGO_HOME",
    },
    version: VersionSource::Command(VersionCmd {
        args: &["--version"],
        env: &[RUSTUP_AUTO_INSTALL_OFF],
        parse: VersionParse::SecondToken,
    }),
    latest: Latest::HttpTomlVersion {
        url: "https://static.rust-lang.org/rustup/release-stable.toml",
    },
    self_updates: SelfUpdates::No,
    upgrade: Some(UpgradeCmd {
        args: &["self", "update"],
        timeout_secs: 600,
        cancel: CancelPolicy::NoCancel,
    }),
    uninstall: Some(Uninstall::Command(CommandUninstall {
        args: &["self", "uninstall", "-y"],
        timeout_secs: 600,
        cancel: CancelPolicy::NoCancel,
        blocked: rustup::uninstall_blocked,
        preview: rustup::uninstall_preview,
    })),
    extra_locks: rustup::extra_locks,
    backup_globs: &[],
    // `cargo`, `rustc`, `rustfmt`, …: what a person with Rust types most.
    other_commands: &rustup::RUSTUP_PROXIES,
};

/// Codex (`codex`), OpenAI's terminal agent, installed by its own script
/// (`curl -fsSL https://chatgpt.com/codex/install.sh | sh`, which redirects
/// to `releases.openai.com/codex/install.sh`; run by the user, never by
/// Banager). Banager reads what is on the disk and runs nothing for it --
/// no version command, no update check, no update; its uninstall moves the
/// script's files to the Trash (below).
///
/// Every value here is from that script, fetched as text on 2026-10-01 and
/// read, never executed -- fetched and read again on 2026-10-06, the same
/// 34,564 bytes, SHA-256 `150e3cf6…8bf6` (research S §3f; `adapters/fixtures/
/// standalone-codex/install-script-2026-10-01/README.md` names the lines):
/// - `BIN_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"`: the launcher is
///   `~/.local/bin/codex`, a symbolic link whose text is absolute,
///   `$CODEX_HOME/packages/standalone/current/bin/codex` (or `current/codex`
///   for an older release's layout; `update_visible_command`). On macOS
///   the script also links `~/.local/bin/codex-code-mode-host` to
///   `current/bin/codex-code-mode-host`, a helper, not a command people
///   type: it is not one of `other_commands`, and the Unknown page leaves
///   it alone because it resolves into the root (`scan::owned_roots`);
/// - `STANDALONE_ROOT="$CODEX_HOME_DIR/packages/standalone"` with
///   `CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"`: the root is
///   `~/.codex/packages/standalone`, never `~/.codex` itself, which holds
///   the user's settings, login and sessions. Releases are unpacked into
///   `releases/<version>-<target>` (`release_name="$resolved_version-
///   $vendor_target"`, the target `aarch64-apple-darwin` or
///   `x86_64-apple-darwin` on a Mac), and `current` is re-pointed at the
///   one in use (`update_current_link`, absolute text);
/// - detection: Banager looks only at `~/.local/bin/codex` and the root
///   under `~/.codex`. `CODEX_HOME` is not read: a Finder-launched app
///   inherits no variable from the user's shell except the `PATH` and the
///   proxy and mirror settings Banager asks the login shell for
///   (`runner::login_path`), so a `CODEX_HOME` exported
///   in `~/.zshrc` is invisible to it. A Codex installed under another
///   `CODEX_HOME` (or another `CODEX_INSTALL_DIR`) has a launcher that does
///   not lead into this root and is not listed here -- the Unknown page
///   lists that launcher instead. npm's `@openai/codex` (a link into
///   `node_modules`) and the Homebrew cask `codex` (a link into
///   `Caskroom`, in Homebrew's `bin`) are other paths and other sources'
///   rows; the families table puts all three in one family;
/// - the version is the release folder's name less its target, read from
///   the `current` link (`VersionSource::ReleaseLink`): `codex --version`
///   is never run;
/// - `AUTO_UPDATE_VERSION="$STANDALONE_ROOT/auto-update-version"`: the
///   script writes the release folder's name there when it installs the
///   latest release and deletes it for a pinned one (`CODEX_RELEASE`), and
///   a scheduled update re-runs the script only while that file names the
///   release `current` points at. So the row says it updates itself when
///   the file names the current release (`self_updates` plus
///   `ReleaseLink::follows_latest`), and nothing about updates otherwise.
///   The file shows the install follows the latest release, not that the
///   updater that re-runs the script (`app-server-daemon/*updater.pid`,
///   `CODEX_INSTALL_IF_LATEST`) is running: nothing read here says so, and
///   the details' text says only that it *can* install new versions;
/// - the newest version comes from `releases.openai.com` or GitHub, hosts
///   not on Banager's list, so nothing is asked (`Latest::Unchecked`) and
///   there is no `upgrade`: Banager must not re-run the script's `curl |
///   sh`;
/// - there is no `codex uninstall` and the script documents no removal, so
///   the uninstall is a path list moved to the Trash, as for Claude Code
///   and Grok Build -- the author's decision U8 (b), 2026-10-06 (D5,
///   synthesis §三). It is the script's own layout, read from the lines
///   above: first the helper link `~/.local/bin/codex-code-mode-host`
///   (optional: the script makes it only on macOS and only for a release
///   that has the helper, `update_visible_command`), moved only when it
///   leads into the package folder (`Expect::SymlinkToProgram`, through
///   `current`), while that folder is still there; then the package folder
///   `~/.codex/packages/standalone` -- every release, `current`, the
///   `auto-update-version` marker; then the launcher, last (spec §6.2).
///   `~/.local/bin` itself is shared and never moved. Kept, and said when
///   present: the rest of `~/.codex` -- `config.toml`, `auth.json`,
///   `sessions/`, history -- and `~/.zprofile`, where the script appends
///   its marked PATH block (`# >>> Codex installer >>>`, `add_to_path`) for
///   zsh on macOS when `~/.local/bin` is not on PATH (`pick_profile`;
///   `~/.bash_profile` for bash, not listed: zsh is macOS's default shell,
///   as for grok's `~/.zshrc`). Banager edits no shell file.
pub static CODEX: Recipe = Recipe {
    id: "codex",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-codex.toml"),
    route: Route {
        kind: RouteKind::SymlinkIntoRoot,
        launcher: "~/.local/bin/codex",
        root: "~/.codex/packages/standalone",
    },
    version: VersionSource::ReleaseLink(ReleaseLink {
        link: "current",
        releases: "releases",
        suffixes: &["-aarch64-apple-darwin", "-x86_64-apple-darwin"],
        follows_latest: "auto-update-version",
    }),
    latest: Latest::Unchecked,
    self_updates: SelfUpdates::Yes,
    upgrade: None,
    uninstall: Some(Uninstall::Paths {
        remove: &[
            RemoveSpec {
                path: "~/.local/bin/codex-code-mode-host",
                expect: Expect::SymlinkToProgram {
                    program: "~/.codex/packages/standalone",
                    via: &[],
                },
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.codex/packages/standalone",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: false,
            },
            RemoveSpec {
                path: "~/.local/bin/codex",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            },
        ],
        keep: &[
            KeepSpec {
                path: "~/.codex",
                what: KeptWhat::SettingsAndHistory,
            },
            KeepSpec {
                path: "~/.zprofile",
                what: KeptWhat::ShellConfigLines,
            },
        ],
    }),
    extra_locks: no_extra_locks,
    backup_globs: &[],
    other_commands: &[],
};

/// opencode (`opencode`), the terminal agent of the opencode project,
/// installed by its own script (`curl -fsSL https://opencode.ai/install |
/// bash`, run by the user; never by Banager). Listed only: Banager reads
/// what is on the disk and runs nothing for it -- no version command, no
/// update check, no update, no uninstall (the author's decision; Codex's
/// own install, listed only too until then, moves to the Trash on
/// uninstall since the author's decision U8, and opencode's was left as it
/// is: its root `~/.opencode` is a first-level folder of the home folder
/// and holds a plugin package besides the program).
///
/// Every value here is from that script, fetched as text on 2026-10-01 and
/// read, never executed (`adapters/fixtures/standalone-opencode/
/// install-script-2026-10-01/README.md` names the lines):
/// - `INSTALL_DIR=$HOME/.opencode/bin`, a fixed path (no variable moves
///   it): the script unpacks the release's one executable and moves it to
///   `$INSTALL_DIR/opencode` (`mv`, then `chmod 755`; or copies one with
///   `--binary`). So the launcher `~/.opencode/bin/opencode` is a regular
///   file, the whole program (`RouteKind::FlatFile`), and the root is
///   `~/.opencode`. A link at that path is not this install. npm's
///   `opencode-ai` and Homebrew's formula `opencode` are other paths and
///   other sources' rows; the families table puts all three in one family;
/// - the version: the script writes nothing that names it -- it checks
///   the installed version only by running `opencode --version`, which
///   Banager does not do. The `package.json`, `package-lock.json` and
///   `node_modules` found in `~/.opencode` on the author's Mac are not the
///   script's (it writes none of them); they name a plugin package's
///   version, not the program's. So no version is read
///   (`VersionSource::NotRead`): the row is listed with its version
///   unknown, and the details say why;
/// - opencode's own documentation (opencode.ai/docs/config, "Autoupdate",
///   read 2026-10-01): it "will automatically download any new updates
///   when it starts up" unless its `autoupdate` setting turns that off.
///   Banager does not read that setting, so the row says it updates itself
///   by default (`self_updates`);
/// - the newest version comes from GitHub (`api.github.com`, the script's
///   own lookup), a host not on Banager's list, so nothing is asked
///   (`Latest::Unchecked`) and there is no `upgrade`: Banager must not
///   re-run the script's `curl | bash`;
/// - no uninstall (`NoSafeMethod`), the author's decision: the script also
///   adds a marked `PATH` line to a shell file, which Banager does not
///   edit. opencode's own `opencode uninstall` exists (recorded in
///   `families.rs`), but Banager does not run it;
/// - the family's data and settings folders, `~/.local/share/opencode`
///   and `~/.config/opencode`, are outside `~/.opencode`: `kept_data`
///   names them (what an uninstall of opencode's npm or Homebrew row
///   leaves), not this recipe.
pub static OPENCODE: Recipe = Recipe {
    id: "opencode",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-opencode.toml"),
    route: Route {
        kind: RouteKind::FlatFile,
        launcher: "~/.opencode/bin/opencode",
        root: "~/.opencode",
    },
    version: VersionSource::NotRead,
    latest: Latest::Unchecked,
    self_updates: SelfUpdates::Yes,
    upgrade: None,
    uninstall: None,
    extra_locks: no_extra_locks,
    backup_globs: &[],
    other_commands: &[],
};

/// Every tool this adapter type registers, in registration order. The
/// refresh fans out alphabetically by adapter id regardless
/// (`refresh_round`), so this order is only the reading order.
pub static RECIPES: &[&Recipe] = &[&CLAUDE, &AGY, &GROK, &RUSTUP, &CODEX, &OPENCODE];

/// Every registered tool's backup-file patterns, keyed by its adapter id
/// (`standalone-<id>`), for the Unknown page's rule 4
/// (`Session::scan_unknown` → `scan::scan_unknown`). A tool with none
/// contributes an empty slice, which claims nothing.
pub fn backup_globs() -> Vec<(String, &'static [Glob])> {
    RECIPES
        .iter()
        .map(|recipe| (format!("standalone-{}", recipe.id), recipe.backup_globs))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::recipe::{Expect, Uninstall, SHARED_FOLDERS};
    use super::*;
    use crate::adapters::AdapterMeta;
    use crate::model::CancelPolicy;
    use crate::model::{KeptWhat, RemovedWhat};
    use std::path::Path;

    #[test]
    fn test_every_recipe_path_is_under_home_or_the_cargo_home() {
        // `route::expand_route` joins `~/` onto `HostEnv.home` and
        // `$CARGO_HOME` (bare, or with a `/`) onto the Cargo home, and
        // nothing else: a recipe path shaped any other way is a
        // programming error this test turns into a red build, not a
        // runtime surprise.
        for recipe in RECIPES {
            for path in [recipe.route.launcher, recipe.route.root] {
                assert!(
                    path.starts_with("~/")
                        || path == "$CARGO_HOME"
                        || path.starts_with("$CARGO_HOME/"),
                    "{}: recipe path {path:?} must start with ~/ or $CARGO_HOME",
                    recipe.id
                );
                assert!(
                    !path.contains("/../") && !path.ends_with("/.."),
                    "{}: recipe path {path:?} must not climb",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_every_recipe_names_its_commands_as_file_names_beside_its_launcher() {
        // `commands::judge` joins each of `other_commands` onto the
        // launcher's folder, and names the launcher's own command by `id`:
        // so the launcher's file name is `id`, and every other command is
        // a plain file name, said once and not the launcher's.
        for recipe in RECIPES {
            assert_eq!(
                Path::new(recipe.route.launcher)
                    .file_name()
                    .and_then(|name| name.to_str()),
                Some(recipe.id),
                "{}: the launcher's name is the command the user types",
                recipe.id
            );
            let mut seen = vec![recipe.id];
            for name in recipe.other_commands {
                assert!(
                    !name.is_empty() && !name.contains('/') && !name.starts_with('.'),
                    "{}: {name:?} must be a plain file name",
                    recipe.id
                );
                assert!(
                    !seen.contains(name),
                    "{}: {name:?} is named twice",
                    recipe.id
                );
                seen.push(name);
            }
        }
        assert_eq!(GROK.other_commands, &["agent"]);
        assert_eq!(RUSTUP.other_commands, &rustup::RUSTUP_PROXIES);
        assert!(CLAUDE.other_commands.is_empty() && AGY.other_commands.is_empty());
    }

    /// The kept paths of a `Paths` recipe that must live under home (every
    /// one but the report-only `OutsideHome` ones, ruling 6 of the step D
    /// plan).
    fn home_keeps(keep: &'static [KeepSpec]) -> impl Iterator<Item = &'static str> {
        keep.iter()
            .filter(|spec| spec.what != KeptWhat::OutsideHome)
            .map(|spec| spec.path)
    }

    #[test]
    fn test_a_paths_recipe_names_only_home_paths() {
        // The path-list uninstall (`removal.rs`) expands its recipe's route
        // and every remove/keep spec with B's two-argument `route::expand`,
        // which knows `~/` and nothing else -- except the report-only
        // `OutsideHome` keeps, which it never expands. A `$CARGO_HOME` tool
        // (rustup) uninstalls with its own command.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let mut paths = vec![recipe.route.launcher, recipe.route.root];
            paths.extend(remove.iter().map(|spec| spec.path));
            paths.extend(home_keeps(keep));
            paths.extend(recipe.backup_globs.iter().map(|glob| glob.dir));
            for spec in remove.iter() {
                if let Expect::SymlinkToProgram { program, via } = spec.expect {
                    paths.push(program);
                    paths.extend(via.iter().copied());
                }
            }
            for path in paths {
                assert!(
                    path.starts_with("~/"),
                    "{}: a Paths recipe may only name ~/ paths, got {path:?}",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_claude_holds_no_lock_but_its_own() {
        // `extra_locks` exists for rustup (step E), whose self update and
        // self uninstall touch what the cargo adapter reads; a tool with
        // its own directory holds only its own instance lock.
        let detected = crate::adapters::standalone::testing::detected(
            Path::new("/Users/someone"),
            Path::new("/Users/someone/.cargo"),
        );
        assert!((CLAUDE.extra_locks)(&detected).is_empty());
    }

    #[test]
    fn test_every_recipe_launcher_is_named_after_its_id() {
        // `id` is the command the user types and the launcher's file name
        // (spec §3.1); a `binary` field arrives with the first tool where
        // the two differ.
        for recipe in RECIPES {
            assert_eq!(
                Path::new(recipe.route.launcher)
                    .file_name()
                    .and_then(|n| n.to_str()),
                Some(recipe.id),
                "{}: launcher {:?} must be named after the id",
                recipe.id,
                recipe.route.launcher
            );
        }
    }

    #[test]
    fn test_every_recipe_meta_parses_and_names_the_standalone_id() {
        for recipe in RECIPES {
            let meta = AdapterMeta::from_toml(recipe.meta_toml)
                .unwrap_or_else(|e| panic!("{}: meta toml: {e}", recipe.id));
            assert_eq!(meta.id, format!("standalone-{}", recipe.id));
            assert!(!meta.id.contains(':'), "Session::build asserts no ':'");
            assert_eq!(meta.kind, "standalone");
            assert!(!meta.name.is_empty());
            assert!(meta.homepage.starts_with("https://"));
            if recipe.version.runs_the_launcher() {
                assert!(
                    !meta.verified_versions.is_empty(),
                    "{}: a recorded fixture backs verified_versions",
                    recipe.id
                );
            } else {
                // A tool Banager never runs (Codex) has no recorded version
                // output to vouch for one, and an empty list marks no
                // version as unverified (`AdapterMeta::unverified_version`).
                assert!(
                    meta.verified_versions.is_empty(),
                    "{}: nothing was recorded from a tool Banager never runs",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_claude_is_the_native_route_read_with_its_autoupdater_off() {
        assert_eq!(CLAUDE.id, "claude");
        assert_eq!(CLAUDE.route.kind, RouteKind::SymlinkIntoRoot);
        assert_eq!(CLAUDE.route.launcher, "~/.local/bin/claude");
        assert_eq!(CLAUDE.route.root, "~/.local/share/claude");
        assert_eq!(CLAUDE.version.command().args, &["--version"]);
        // Spec §3.4: the version read must not start a background update
        // check; the upgrade plan (`StandaloneAdapter::plan`) must not
        // carry this.
        assert_eq!(
            CLAUDE.version.command().env,
            &[("DISABLE_AUTOUPDATER", "1")]
        );
        assert_eq!(CLAUDE.version.command().parse, VersionParse::FirstToken);
        assert_eq!(CLAUDE.self_updates, SelfUpdates::UnlessOffInClaudeSettings);
    }

    #[test]
    fn test_claude_updates_with_its_own_updater() {
        let upgrade = CLAUDE
            .upgrade
            .as_ref()
            .expect("claude has an update command");
        assert_eq!(upgrade.args, &["update"]);
        assert_eq!(upgrade.timeout_secs, 1800);
        assert_eq!(upgrade.cancel, CancelPolicy::KillThenReconcile);
        assert_eq!(
            CLAUDE.latest,
            Latest::ClaudeChannel {
                base: "https://downloads.claude.ai/claude-code-releases"
            }
        );
    }

    #[test]
    fn test_recipes_lists_each_registered_tool_once_in_reading_order() {
        assert_eq!(RECIPES.len(), 6);
        assert!(std::ptr::eq(RECIPES[0], &CLAUDE));
        assert!(std::ptr::eq(RECIPES[1], &AGY));
        assert!(std::ptr::eq(RECIPES[2], &GROK));
        assert!(std::ptr::eq(RECIPES[3], &RUSTUP));
        assert!(std::ptr::eq(RECIPES[4], &CODEX));
        assert!(std::ptr::eq(RECIPES[5], &OPENCODE));
        let mut ids: Vec<&str> = RECIPES.iter().map(|r| r.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), RECIPES.len(), "one recipe per tool");
    }

    #[test]
    fn test_every_recipe_latest_url_is_an_allowed_https_host() {
        // Spec §4.2: a recipe that brings a new host and the allowlist
        // entry are one reviewed change; `RealHttpClient::send` refuses
        // anything else before connecting, so a recipe whose host is not
        // on the list would be a permanent "could not check" row.
        use crate::http::real::host_allowed;
        for recipe in RECIPES {
            let urls: Vec<String> = match recipe.latest {
                Latest::ClaudeChannel { base } => vec![
                    format!(
                        "{base}/{}",
                        crate::adapters::standalone::latest::CHANNEL_LATEST
                    ),
                    format!(
                        "{base}/{}",
                        crate::adapters::standalone::latest::CHANNEL_STABLE
                    ),
                ],
                Latest::HttpTomlVersion { url } => vec![url.to_string()],
                Latest::HttpJsonField { url, .. } => vec![url.to_string()],
                // The tool's own command makes its own connection, under its
                // own configuration (docs/what-we-run.md, the network
                // section's last paragraph): no host of Banager's.
                Latest::Command { .. } => Vec::new(),
                // Nothing is asked at all.
                Latest::Unchecked => Vec::new(),
            };
            for url in urls {
                host_allowed(&url).unwrap_or_else(|e| panic!("{}: {url}: {e}", recipe.id));
            }
        }
    }

    /// The remove and keep paths of a recipe with a path list; empty for
    /// one without (or with another kind of uninstall, step E on).
    fn path_lists(recipe: &Recipe) -> (Vec<&'static str>, Vec<&'static str>) {
        if let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall {
            (
                remove.iter().map(|spec| spec.path).collect(),
                keep.iter().map(|spec| spec.path).collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        }
    }

    #[test]
    fn test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder() {
        // `route::expand` panics on a path that does not start with `~/`,
        // and the removal's check 1 refuses a path whose folder is the home
        // folder or one of `SHARED_FOLDERS` -- a recipe listing one would
        // refuse every uninstall, and the never-list exists so no recipe
        // can quietly move `~/.local/bin` whole. The one exception is a
        // kept path outside the home folder, which is never expanded:
        // absolute, never `~/`, never under a user's home.
        for recipe in RECIPES {
            let (remove, keep) = path_lists(recipe);
            let Some(Uninstall::Paths {
                keep: keep_specs, ..
            }) = &recipe.uninstall
            else {
                continue;
            };
            for path in remove.iter().copied().chain(home_keeps(keep_specs)) {
                let rest = path
                    .strip_prefix("~/")
                    .unwrap_or_else(|| panic!("{}: {path:?} must start with ~/", recipe.id));
                assert!(
                    !rest.is_empty()
                        && !rest.ends_with('/')
                        && !rest.contains("..")
                        && !rest.contains("/./"),
                    "{}: {path:?} must name one plain path",
                    recipe.id
                );
            }
            for spec in keep_specs
                .iter()
                .filter(|spec| spec.what == KeptWhat::OutsideHome)
            {
                assert!(
                    spec.path.starts_with('/') && !spec.path.starts_with("/Users/"),
                    "{}: an OutsideHome keep names an absolute path outside every home, got {:?}",
                    recipe.id,
                    spec.path
                );
            }
            assert_eq!(keep.len(), keep_specs.len());
            for path in &remove {
                let folder = Path::new(path.strip_prefix("~/").unwrap())
                    .parent()
                    .unwrap_or(Path::new(""));
                assert!(
                    folder != Path::new("")
                        && !SHARED_FOLDERS.iter().any(|shared| folder == Path::new(shared)),
                    "{}: {path:?} sits directly in the home folder or in a shared folder; check 1 would refuse it",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another() {
        // Spec §6.2: the launcher itself last (claude, agy, grok's
        // `~/.grok/bin/grok`; never a folder holding it, ruling 4 of the
        // step D plan), so a run that stops partway leaves exactly the
        // launcher-only state a second run finishes -- and what it is must
        // match the route: a link for `SymlinkIntoRoot`, a file for
        // `FlatFile`. Spec §6.3's former check 7: no removed path is inside
        // another removed path, and no kept path is inside a removed one
        // -- properties of the constant, not of the Mac.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let last = remove
                .last()
                .unwrap_or_else(|| panic!("{}: an empty remove list", recipe.id));
            assert_eq!(
                last.path, recipe.route.launcher,
                "{}: the last path must be the launcher",
                recipe.id
            );
            assert_eq!(last.what, RemovedWhat::Launcher, "{}", recipe.id);
            assert!(
                !last.optional,
                "{}: the launcher is never optional",
                recipe.id
            );
            let expected = match recipe.route.kind {
                RouteKind::SymlinkIntoRoot => Expect::SymlinkIntoRoot,
                RouteKind::FlatFile => Expect::File,
            };
            assert_eq!(last.expect, expected, "{}", recipe.id);
            let removed: Vec<&str> = remove.iter().map(|spec| spec.path).collect();
            for a in &removed {
                for b in &removed {
                    assert!(
                        a == b || !b.starts_with(&format!("{a}/")),
                        "{}: {b:?} is inside {a:?}",
                        recipe.id
                    );
                }
                for kept in home_keeps(keep) {
                    assert!(
                        kept != *a && !kept.starts_with(&format!("{a}/")),
                        "{}: kept {kept:?} is inside removed {a:?}",
                        recipe.id
                    );
                }
            }
        }
    }

    #[test]
    fn test_every_link_a_paths_recipe_moves_besides_its_launcher_must_lead_to_its_program() {
        // Only the launcher is checked against the whole root
        // (`SymlinkIntoRoot`, as detect checks it). Any other link the list
        // moves is checked against the program (`SymlinkToProgram`): a
        // tool's root may be the very folder its uninstall keeps -- grok's
        // `~/.grok`, with the user's plugins and skills in it -- so a link
        // into it is not thereby the tool's. And what such a link may lead
        // to is the list's own: `program` a folder it moves as the program
        // and requires, `via` the launcher or another of its links to the
        // program -- so a recipe cannot widen the check to a folder it keeps.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, .. }) = &recipe.uninstall else {
                continue;
            };
            for spec in remove
                .iter()
                .filter(|spec| spec.path != recipe.route.launcher)
            {
                assert_ne!(
                    spec.expect,
                    Expect::SymlinkIntoRoot,
                    "{}: {:?} is not the launcher; a link to the program is SymlinkToProgram",
                    recipe.id,
                    spec.path
                );
                let Expect::SymlinkToProgram { program, via } = spec.expect else {
                    continue;
                };
                assert!(
                    remove.iter().any(|folder| folder.path == program
                        && folder.expect == Expect::Dir
                        && folder.what == RemovedWhat::Program
                        && !folder.optional),
                    "{}: {:?} must lead into a program folder the list requires, not {program:?}",
                    recipe.id,
                    spec.path
                );
                for link in via {
                    assert!(
                        *link == recipe.route.launcher
                            || remove.iter().any(|other| other.path == *link
                                && matches!(other.expect, Expect::SymlinkToProgram { .. })),
                        "{}: {:?} may point only at the launcher or another of the list's links to the program, not {link:?}",
                        recipe.id,
                        spec.path
                    );
                }
            }
        }
    }

    #[test]
    fn test_claude_codes_uninstall_is_anthropics_two_paths_plus_the_download_cache() {
        // The list, exactly, in execution order: what the dialog shows
        // (spec §6.3's claude row, §6.6). The provenance is the constant's
        // doc comment and the fixture README.
        let Some(Uninstall::Paths { remove, keep }) = &CLAUDE.uninstall else {
            panic!("claude has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![
                (
                    "~/.local/share/claude",
                    Expect::Dir,
                    RemovedWhat::Program,
                    false
                ),
                ("~/.claude/downloads", Expect::Dir, RemovedWhat::Cache, true),
                (
                    "~/.local/bin/claude",
                    Expect::SymlinkIntoRoot,
                    RemovedWhat::Launcher,
                    false
                ),
            ]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.claude", KeptWhat::SettingsAndHistory),
                ("~/.claude.json", KeptWhat::Settings),
            ]
        );
    }

    #[test]
    fn test_rustup_is_the_flat_file_route_under_cargo_home_read_as_the_second_token_with_auto_install_off(
    ) {
        assert_eq!(RUSTUP.id, "rustup");
        assert_eq!(RUSTUP.route.kind, RouteKind::FlatFile);
        assert_eq!(RUSTUP.route.launcher, "$CARGO_HOME/bin/rustup");
        assert_eq!(RUSTUP.route.root, "$CARGO_HOME");
        assert_eq!(RUSTUP.version.command().args, &["--version"]);
        // Ruling 20: `rustup --version` resolves the active toolchain and,
        // with none active and auto-install on (the default), installs
        // one -- a download during a refresh. The switch that stops it
        // is the same constant cargo's detect uses for the proxy.
        assert_eq!(
            RUSTUP.version.command().env,
            &[("RUSTUP_AUTO_INSTALL", "0")]
        );
        assert_eq!(
            RUSTUP.version.command().env,
            &[crate::adapters::cargo::RUSTUP_AUTO_INSTALL_OFF]
        );
        assert_eq!(RUSTUP.version.command().parse, VersionParse::SecondToken);
        assert_eq!(RUSTUP.self_updates, SelfUpdates::No);
        assert_eq!(
            RUSTUP.latest,
            Latest::HttpTomlVersion {
                url: "https://static.rust-lang.org/rustup/release-stable.toml"
            }
        );
    }

    #[test]
    fn test_rustup_updates_and_uninstalls_itself_with_no_cancel_the_gate_and_the_cargo_lock() {
        // `self update`, never `update` (spec §五, D6): the latter touches
        // the toolchains and an interruption leaves them half installed.
        let upgrade = RUSTUP
            .upgrade
            .as_ref()
            .expect("rustup has an update command");
        assert_eq!(upgrade.args, &["self", "update"]);
        assert_eq!(upgrade.timeout_secs, 600);
        assert_eq!(upgrade.cancel, CancelPolicy::NoCancel);
        let Some(Uninstall::Command(cmd)) = &RUSTUP.uninstall else {
            panic!("rustup uninstalls with its own command");
        };
        assert_eq!(cmd.args, &["self", "uninstall", "-y"]);
        assert_eq!(cmd.timeout_secs, 600);
        assert_eq!(cmd.cancel, CancelPolicy::NoCancel);
        assert!(
            !cmd.args.contains(&"--no-modify-path"),
            "spec Q6: rustup removes its own startup line"
        );
        // The functions, by identity: the recipe is data, and these three
        // are the data's only behaviour.
        assert!(std::ptr::fn_addr_eq(
            cmd.blocked,
            super::super::rustup::uninstall_blocked
                as fn(
                    &crate::adapters::standalone::Detected,
                ) -> Option<crate::adapters::standalone::recipe::GateRefusal>
        ));
        assert!(std::ptr::fn_addr_eq(
            cmd.preview,
            super::super::rustup::uninstall_preview
                as fn(
                    &crate::adapters::standalone::Detected,
                ) -> Result<
                    Vec<crate::model::Warning>,
                    crate::adapters::standalone::recipe::GateRefusal,
                >
        ));
        assert!(std::ptr::fn_addr_eq(
            RUSTUP.extra_locks,
            super::super::rustup::extra_locks
                as fn(&crate::adapters::standalone::Detected) -> Vec<crate::model::ResourceLock>
        ));
    }

    #[test]
    fn test_the_rustup_recipe_never_builds_rustup_update() {
        // Belt and braces over the assertion above, for every argv the
        // RUSTUP recipe holds -- upgrade, version read, uninstall:
        // `rustup update` is the one subcommand this recipe must never
        // build (spec D6, 附录 B). Scoped to rustup on purpose: `update`
        // is dangerous only as *rustup's* first argument, and it is
        // claude's and grok's documented upgrade (`claude update`, `grok
        // update`, spec §五; the args of B's `CLAUDE.upgrade` and of
        // `GROK.upgrade`) and the first word of grok's read-only check
        // (`GROK.latest`, `update --check --json`), so a ban over every
        // recipe would fail on the two recipes that are right to use it.
        let Some(Uninstall::Command(cmd)) = &RUSTUP.uninstall else {
            panic!("rustup uninstalls with its own command");
        };
        let upgrade = RUSTUP
            .upgrade
            .as_ref()
            .expect("rustup has an update command");
        for argv in [upgrade.args, RUSTUP.version.command().args, cmd.args] {
            assert_ne!(argv.first(), Some(&"update"), "rustup: {argv:?}");
        }
    }

    #[test]
    fn test_every_backup_glob_is_under_home_and_names_a_pattern() {
        // `Glob::dir_under` joins `~/` and nothing else, and check 1 refuses
        // a match whose folder is a shared one, so a pattern's folder must
        // be under home and deeper than the never-list; a pattern with an
        // empty prefix or suffix would match every file in the folder.
        for recipe in RECIPES {
            for glob in recipe.backup_globs {
                let rest = glob.dir.strip_prefix("~/").unwrap_or_else(|| {
                    panic!("{}: glob dir {:?} must start with ~/", recipe.id, glob.dir)
                });
                assert!(
                    !rest.is_empty() && !rest.ends_with('/') && !rest.contains(".."),
                    "{}: glob dir {:?} must name one plain folder",
                    recipe.id,
                    glob.dir
                );
                assert!(
                    !SHARED_FOLDERS.contains(&rest),
                    "{}: glob dir {:?} is a shared folder; check 1 would refuse every match",
                    recipe.id,
                    glob.dir
                );
                assert!(
                    !glob.prefix.is_empty(),
                    "{}: an empty prefix matches everything",
                    recipe.id
                );
                assert!(
                    !glob.suffix.is_empty(),
                    "{}: an empty suffix matches everything",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_backup_globs_lists_every_recipe_under_its_adapter_id() {
        // `Session::scan_unknown` hands this to the scan's rule 4, keyed the
        // way the scan keys instances: by adapter id.
        let listed = backup_globs();
        assert_eq!(listed.len(), RECIPES.len());
        for (recipe, (id, globs)) in RECIPES.iter().zip(&listed) {
            assert_eq!(id, &format!("standalone-{}", recipe.id));
            assert!(std::ptr::eq(*globs, recipe.backup_globs));
        }
    }

    #[test]
    fn test_agy_is_a_flat_file_read_with_its_auto_update_off_that_updates_itself() {
        // Spec §3.4/§3.5's agy column, and agy.md §2/§4 (VERIFIED on this
        // Mac): a regular Mach-O file at `~/.local/bin/agy`, root
        // `~/.gemini/antigravity-cli`; `--version` prints one bare version
        // and is read with Google's documented updater switch.
        assert_eq!(AGY.id, "agy");
        assert_eq!(AGY.route.kind, RouteKind::FlatFile);
        assert_eq!(AGY.route.launcher, "~/.local/bin/agy");
        assert_eq!(AGY.route.root, "~/.gemini/antigravity-cli");
        assert_eq!(AGY.version.command().args, &["--version"]);
        assert_eq!(
            AGY.version.command().env,
            &[("AGY_CLI_DISABLE_AUTO_UPDATE", "true")]
        );
        assert_eq!(AGY.version.command().parse, VersionParse::FirstToken);
        assert_eq!(AGY.self_updates, SelfUpdates::Yes);
        assert_eq!(
            AGY.latest,
            Latest::HttpJsonField {
                url: "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json",
                field: "version",
            }
        );
        let meta = AdapterMeta::from_toml(AGY.meta_toml).expect("meta");
        assert_eq!(meta.name, "Antigravity CLI");
        assert_eq!(
            meta.homepage,
            "https://antigravity.google/docs/cli/install/"
        );
    }

    #[test]
    fn test_agy_has_no_update_command_and_a_one_path_uninstall_with_a_backup_pattern() {
        // Spec §4.4 D5 item 4: `agy update` is undocumented and unrun, so no
        // upgrade -- every candidate is SelfUpdatesOnly. Spec §6.3's agy row
        // as the phase 4 step D plan rules it (its rulings 1 and 2): the
        // launcher is the whole program and goes alone, the updater's
        // `.old` copies go before it, the root and the staging folder stay
        // and are said.
        assert!(AGY.upgrade.is_none());
        let Some(Uninstall::Paths { remove, keep }) = &AGY.uninstall else {
            panic!("agy has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![(
                "~/.local/bin/agy",
                Expect::File,
                RemovedWhat::Launcher,
                false
            )]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.gemini/antigravity-cli", KeptWhat::ToolState),
                ("~/.cache/antigravity", KeptWhat::InstallerCache),
                ("~/.zshrc", KeptWhat::ShellConfigLines),
                ("~/.zprofile", KeptWhat::ShellConfigLines),
            ]
        );
        assert_eq!(
            AGY.backup_globs,
            &[Glob {
                dir: "~/.local/bin",
                prefix: "agy.",
                suffix: ".old",
                what: RemovedWhat::Backups,
            }]
        );
    }

    #[test]
    fn test_grok_is_a_relative_link_route_read_with_the_second_token_that_asks_itself_for_updates()
    {
        // Spec §3.5's grok column (the link text, relative, VERIFIED there)
        // and grok.md §1/§3/§4 (VERIFIED on this Mac):
        // `~/.grok/bin/grok -> ../downloads/grok-<v>-macos-aarch64`, a
        // relative link into `~/.grok`; `grok --version` prints
        // `grok 1.0.41 (4220f3b224a6)`; `update --check --json` is its own
        // read-only check ("without installing"); `grok update` is the
        // documented upgrade; whether it installs updates on its own is
        // UNVERIFIED, so it is not called self-updating (spec §4.4).
        assert_eq!(GROK.id, "grok");
        assert_eq!(GROK.route.kind, RouteKind::SymlinkIntoRoot);
        assert_eq!(GROK.route.launcher, "~/.grok/bin/grok");
        assert_eq!(GROK.route.root, "~/.grok");
        assert_eq!(GROK.version.command().args, &["--version"]);
        assert!(GROK.version.command().env.is_empty());
        assert_eq!(GROK.version.command().parse, VersionParse::SecondToken);
        assert_eq!(GROK.self_updates, SelfUpdates::No);
        assert_eq!(
            GROK.latest,
            Latest::Command {
                args: &["update", "--check", "--json"],
                timeout_secs: 60,
                latest_field: "latestVersion",
                available_field: "updateAvailable",
                error_field: Some("error"),
            }
        );
        let upgrade = GROK.upgrade.as_ref().expect("grok has an update command");
        assert_eq!(upgrade.args, &["update"]);
        assert_eq!(upgrade.timeout_secs, 1800);
        assert_eq!(upgrade.cancel, CancelPolicy::KillThenReconcile);
        assert!(GROK.backup_globs.is_empty());
        let meta = AdapterMeta::from_toml(GROK.meta_toml).expect("meta");
        assert_eq!(meta.name, "Grok Build");
        assert_eq!(meta.homepage, "https://x.ai/build");
    }

    #[test]
    fn test_grok_uninstall_moves_its_own_fallback_links_first_and_its_launcher_link_last() {
        // Spec §6.3's grok row, in the phase 4 step D plan's order (its
        // rulings 3 and 4): the two optional fallback links first (their
        // link text is unverified, so they go while every folder it could
        // pass through is still there), the program folders, the fish
        // completion, then the two links the installer put in
        // `~/.grok/bin` -- `agent`, and `grok` itself last -- never the
        // folder, which may hold the user's own scripts (it is on PATH).
        // `~/.grok` itself stays with its settings, login, sessions and
        // memory; the shell file stays; a fallback link in /usr/local/bin
        // is never touched, and reported when it links into `~/.grok` and
        // the moves leave it leading nowhere (its ruling 6). The three
        // links besides the launcher must lead to the program in
        // `~/.grok/downloads` -- straight there, or through one of the two
        // links in `~/.grok/bin` -- never merely into the kept `~/.grok`
        // (the whole-step review of step D, which also narrowed the
        // /usr/local/bin report to links left leading nowhere).
        let Some(Uninstall::Paths { remove, keep }) = &GROK.uninstall else {
            panic!("grok has a path list");
        };
        let to_program = Expect::SymlinkToProgram {
            program: "~/.grok/downloads",
            via: &["~/.grok/bin/grok", "~/.grok/bin/agent"],
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![
                ("~/.local/bin/grok", to_program, RemovedWhat::Launcher, true),
                (
                    "~/.local/bin/agent",
                    to_program,
                    RemovedWhat::Launcher,
                    true
                ),
                (
                    "~/.grok/downloads",
                    Expect::Dir,
                    RemovedWhat::Program,
                    false
                ),
                ("~/.grok/bundled", Expect::Dir, RemovedWhat::Program, true),
                (
                    "~/.grok/completions",
                    Expect::Dir,
                    RemovedWhat::Program,
                    true
                ),
                (
                    "~/.config/fish/completions/grok.fish",
                    Expect::File,
                    RemovedWhat::Program,
                    true
                ),
                ("~/.grok/bin/agent", to_program, RemovedWhat::Launcher, true),
                (
                    "~/.grok/bin/grok",
                    Expect::SymlinkIntoRoot,
                    RemovedWhat::Launcher,
                    false
                ),
            ]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.grok", KeptWhat::SettingsAndHistory),
                ("~/.zshrc", KeptWhat::ShellConfigLines),
                ("/usr/local/bin/grok", KeptWhat::OutsideHome),
                ("/usr/local/bin/agent", KeptWhat::OutsideHome),
            ]
        );
    }

    #[test]
    fn test_every_command_latest_source_only_checks() {
        // Spec §3.1: a `Latest::Command` may name only a subcommand whose own
        // --help says it installs nothing -- grok's `update --check --json`
        // ("Check for updates without installing", grok.md §4). It runs on
        // every refresh; `update` without `--check` would be an upgrade.
        for recipe in RECIPES {
            if let Latest::Command {
                args, timeout_secs, ..
            } = recipe.latest
            {
                assert!(
                    args.contains(&"--check"),
                    "{}: {args:?} must carry --check",
                    recipe.id
                );
                assert!(
                    args.contains(&"--json"),
                    "{}: {args:?} must ask for machine-readable output",
                    recipe.id
                );
                assert!(
                    timeout_secs <= 120,
                    "{}: a check is not an install",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_codex_is_listed_only_read_from_its_current_link() {
        // The install script, read as text (research S §3f): the launcher
        // `~/.local/bin/codex` links into `$CODEX_HOME/packages/standalone`,
        // whose `current` names `releases/<version>-<target>`.
        assert_eq!(CODEX.id, "codex");
        assert_eq!(CODEX.route.kind, RouteKind::SymlinkIntoRoot);
        assert_eq!(CODEX.route.launcher, "~/.local/bin/codex");
        // The package folder, never `~/.codex`, which holds the user's
        // settings, login and sessions.
        assert_eq!(CODEX.route.root, "~/.codex/packages/standalone");
        let VersionSource::ReleaseLink(link) = &CODEX.version else {
            panic!("Codex's version is read from a link, never by running codex");
        };
        assert!(!CODEX.version.runs_the_launcher());
        assert_eq!(link.link, "current");
        assert_eq!(link.releases, "releases");
        assert_eq!(
            link.suffixes,
            &["-aarch64-apple-darwin", "-x86_64-apple-darwin"]
        );
        assert_eq!(link.follows_latest, "auto-update-version");
        // Nothing asked and nothing run: no update check, no update.
        assert_eq!(CODEX.latest, Latest::Unchecked);
        assert_eq!(CODEX.self_updates, SelfUpdates::Yes);
        assert!(CODEX.upgrade.is_none());
        assert!(CODEX.backup_globs.is_empty());
        assert!(CODEX.other_commands.is_empty());
        assert!(
            (CODEX.extra_locks)(&crate::adapters::standalone::testing::detected(
                Path::new("/Users/someone"),
                Path::new("/Users/someone/.cargo"),
            ))
            .is_empty()
        );
    }

    #[test]
    fn test_codex_uninstall_moves_its_two_links_and_its_package_folder_and_keeps_the_rest_of_codex()
    {
        // The author's decision U8 (b), 2026-10-06: the two links the
        // install script makes in `~/.local/bin` and the package folder
        // they lead into go to the Trash; everything else in `~/.codex` --
        // settings, login, sessions -- stays, and so does the shell file
        // the script may have added its marked PATH block to. The helper
        // first, while the program it leads to is still there; the
        // launcher last (spec §6.2).
        let Some(Uninstall::Paths { remove, keep }) = &CODEX.uninstall else {
            panic!("Codex's own install has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![
                (
                    "~/.local/bin/codex-code-mode-host",
                    Expect::SymlinkToProgram {
                        program: "~/.codex/packages/standalone",
                        via: &[],
                    },
                    RemovedWhat::Program,
                    true
                ),
                (
                    "~/.codex/packages/standalone",
                    Expect::Dir,
                    RemovedWhat::Program,
                    false
                ),
                (
                    "~/.local/bin/codex",
                    Expect::SymlinkIntoRoot,
                    RemovedWhat::Launcher,
                    false
                ),
            ]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.codex", KeptWhat::SettingsAndHistory),
                ("~/.zprofile", KeptWhat::ShellConfigLines),
            ]
        );
    }
}
