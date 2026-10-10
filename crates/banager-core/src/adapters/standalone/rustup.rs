//! What `rustup self uninstall` does, when Banager may offer it, and what
//! to tell the user before it runs (phase 4 spec §6.4): the facts here
//! were read from rustup's source at the tag the installed binary was
//! built from -- `1.29.1`, commit d95a37b6, the `d95a37b6a` in `rustup
//! --version` on the recording Mac -- and every line number below is that
//! tag's (`src/cli/self_update.rs`, `src/cli/self_update/unix.rs`,
//! `src/cli/self_update/shell.rs`), or `home` 0.5.12's
//! (`crates/home/src/env.rs`), the crate rustup reads its homes through.
//! Newer rustup keeps the programs `cargo install` installed
//! (`clean_cargo_home` on master); 1.29.1 does not, and this recipe is
//! verified against 1.29.1.
//!
//! `uninstall()` (self_update.rs:924-1032): removes every toolchain
//! (:955-958, the entries of `$RUSTUP_HOME/toolchains`), then
//! `$RUSTUP_HOME` (:960-966), then -- unless `--no-modify-path`, which
//! Banager does not pass -- the line it added to the shell startup files
//! (:971-973, `do_remove_from_path`), then everything in `$CARGO_HOME`
//! except `bin/` (:977-993), then everything in `bin/` that is not one of
//! its own proxies or `rustup` itself (:996-1022), and finally the whole
//! `$CARGO_HOME` directory (`delete_rustup_and_cargo_home`, :1029;
//! unix.rs:50-53). Both homes come from `RUSTUP_HOME`/`CARGO_HOME` or
//! default under `HOME` (env.rs:67-79, :101-113) -- wherever they point,
//! and permanently: nothing here goes to the Trash. So Banager offers the
//! command only for the standard layout (`standard_roots`, ruling 18),
//! asks that gate again right before the command runs
//! (`StandaloneAdapter::execute`), and the preview (`uninstall_preview`)
//! asks it once and names both folders by path from that one answer.
//!
//! Nothing here runs a command or writes a file: `plan` hands in what
//! `detect` seated, and this module lists `<rustup_home>/toolchains` and
//! `<cargo_home>/bin` by name, reads `.crates2.json` and `.crates.toml`, looks for
//! Homebrew's `Cellar/rustup`, reads eight startup files under the home
//! (and zsh's three under a `ZDOTDIR` that is another folder), replays
//! rustup's own cleanup on copies of them in memory, and answers with
//! `Warning`s.

use super::recipe::GateRefusal;
use super::Detected;
use crate::adapters::cargo::{instance_id_for, merge_crates_v1, parse_crates2_bins};
use crate::model::{ResourceLock, UninstallBlocked, Warning};
use crate::protected::{look, Protected};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The startup files Banager reads (never writes) for a line about the
/// Cargo env file, home-relative, in the order they are reported (spec
/// §6.4). When `ZDOTDIR` names another folder, zsh's three -- `.zshenv`,
/// `.zprofile`, `.zshrc` -- are read under it as well (`startup_files`);
/// a `ZDOTDIR` Banager cannot see, one set only inside a zsh startup
/// file, is not modelled, and the trust file says so.
pub const SHELL_RC_CANDIDATES: [&str; 8] = [
    ".zshenv",
    ".zprofile",
    ".zshrc",
    ".bash_profile",
    ".bash_login",
    ".bashrc",
    ".profile",
    ".config/fish/config.fish",
];

/// The names rustup 1.29.1's `uninstall()` spares in `<cargo_home>/bin`
/// besides `rustup` itself: its proxies, `TOOLS` + `DUP_TOOLS`
/// (`src/lib.rs:16-32`), which it compares by name
/// (self_update.rs:996-1022). Sorted. On this Mac all thirteen are
/// relative links to `rustup` (unknown-scan.md §2). Read by
/// `bin_programs_rustup_removes`, and by `testing::rustup_layout`, which
/// builds the same layout for the tests.
pub const RUSTUP_PROXIES: [&str; 13] = [
    "cargo",
    "cargo-clippy",
    "cargo-fmt",
    "cargo-miri",
    "clippy-driver",
    "rls",
    "rust-analyzer",
    "rust-gdb",
    "rust-gdbgui",
    "rust-lldb",
    "rustc",
    "rustdoc",
    "rustfmt",
];

/// Homebrew's two default prefixes (Apple Silicon, Intel), where a
/// `Cellar/rustup` directory means the `rustup` formula is installed. A
/// custom prefix is unsupported by Homebrew itself on Apple Silicon and
/// is not looked for. Read by `uninstall_preview`.
pub const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// The two folders `rustup self uninstall` deletes, when they are the
/// standard ones (`standard_roots`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardRoots {
    pub cargo_home: PathBuf,
    pub rustup_home: PathBuf,
}

/// Whether `path` is a real folder, not a link (`lstat`), never looked up
/// into or through a protected place (`protected::look`).
fn is_real_dir(path: &Path, protected: &Protected) -> bool {
    look::lstat(path, protected).is_ok_and(|meta| meta.is_dir())
}

/// Whether anything at the top of `root` is a link: `Ok(())` when nothing
/// is, else the path `standard_roots` refuses at -- the first link by
/// name, or `root` itself when it cannot be listed, since a folder Banager
/// cannot list may hold a link it cannot see. The folder's names and each
/// entry's own `lstat`, from the folder held open (`look::list`), never
/// in or through a protected place; nothing is opened or followed. Read
/// by `standard_roots`.
fn no_link_at_the_top(root: &Path, protected: &Protected) -> Result<(), PathBuf> {
    let unlistable = |_| root.to_path_buf();
    let listing = look::list(root, protected).map_err(unlistable)?;
    let mut links = Vec::new();
    for name in listing
        .names(&mut look::ListingBudget::default())
        .map_err(unlistable)?
    {
        let meta = listing.lstat(&name).map_err(unlistable)?;
        if meta.is_symlink() {
            links.push(root.join(name));
        }
    }
    links.sort();
    match links.into_iter().next() {
        Some(link) => Err(link),
        None => Ok(()),
    }
}

/// The gate (ruling 18): `Ok` only when the Rust this instance belongs
/// to lives where a fresh `rustup-init` puts it. Both homes as rustup
/// computes them (`home` 0.5.12 over `RUSTUP_HOME`/`CARGO_HOME`/`HOME`,
/// seated by `detect`; `None` is a relative value, unsupported); each
/// exactly `<home>/.cargo` and `<home>/.rustup` -- compared lexically,
/// as rustup itself compares when it decides how to spell the Cargo home
/// in the shell line (`cargo_home_str_with_home`, shell.rs:43-58), over
/// the same `HOME`; the Cargo home a directory that is not a link; the
/// rustup home a directory that is not a link, or not there yet (rustup
/// creates it on its first run); and nothing at the top of either a link
/// (`no_link_at_the_top`; step E's whole-step review). That last rule is
/// from how `uninstall()` walks the two folders. Three names it reaches
/// *through* their parent, deciding with `is_directory` (raw.rs:29-31,
/// `fs::metadata`) or `is_dir()`, both of which follow a link:
/// `toolchains/<name>` (`list_toolchains`, config.rs:922-940, then
/// `Toolchain::ensure_removed`, toolchain.rs:536-582, down to
/// `raw::remove_dir`, whose `symlink_metadata` sees a real folder at the
/// *end* of that path), `update-hashes/<name>` (`installed_paths`,
/// config.rs:464-476, one file per official toolchain) and `bin/<name>`
/// (self_update.rs:1003-1022). A link at one of those three would have it
/// delete the contents of wherever the link leads -- outside the two
/// folders the preview names; for `toolchains`, every folder there whose
/// name parses as a toolchain name, which most names do (names.rs:343-356).
/// Every other link at the top of a root it unlinks without following:
/// `raw::remove_dir` (raw.rs:277-311) `remove_file`s a path that is itself
/// a link, and the `remove_dir_all` 1.0.0 it hands a real folder to opens
/// each entry with `follow(false)` and `unlink_at`s a link
/// (`src/_impl.rs:133-213`). Banager keeps no list of which names rustup
/// follows: any link at the top refuses. Anything else -- a custom home, a
/// link to somewhere else, a relative variable, a link at the top of a
/// root -- and `uninstall()` would delete a place this preview did not
/// name: not offered, and the `Err` is `NoSafeMethod` at the path the
/// rule failed at -- `<home>/.cargo`, or `<home>/.rustup` once the Cargo
/// home passes, the link at the top of one (the first by name), or a root
/// that could not be listed -- so `execute`, asking again right before the
/// spawn, can say which is no longer what the preview showed. The disk is
/// read every time: the answer is for now, not for when `detect` seated
/// the homes.
pub fn standard_roots(d: &Detected) -> Result<StandardRoots, GateRefusal> {
    let refused = |path: PathBuf| GateRefusal {
        reason: UninstallBlocked::NoSafeMethod,
        path,
    };
    let protected = d.protected();
    let standard_cargo = d.home.join(".cargo");
    let standard_rustup = d.home.join(".rustup");
    let cargo_home = match d.cargo_home.as_deref() {
        Some(cargo_home)
            if d.home.is_absolute()
                && cargo_home == standard_cargo
                && is_real_dir(cargo_home, &protected) =>
        {
            cargo_home
        }
        _ => return Err(refused(standard_cargo)),
    };
    no_link_at_the_top(cargo_home, &protected).map_err(refused)?;
    let rustup_home = match d.rustup_home.as_deref() {
        Some(rustup_home) if rustup_home == standard_rustup => rustup_home,
        _ => return Err(refused(standard_rustup)),
    };
    match look::lstat(rustup_home, &protected) {
        Ok(meta) if meta.is_dir() => {
            no_link_at_the_top(rustup_home, &protected).map_err(refused)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(refused(standard_rustup)),
    }
    Ok(StandardRoots {
        cargo_home: cargo_home.to_path_buf(),
        rustup_home: rustup_home.to_path_buf(),
    })
}

/// `CommandUninstall.blocked` of the `RUSTUP` recipe: the gate's refusal
/// -- `NoSafeMethod`, at the folder that fails -- for any layout but the
/// standard one. The variant is the one the gate (`session/plans.rs`)
/// and the Installed page already refuse and hide the button for;
/// rustup's row says why in its own sentence
/// (`installed.blocked.NoSafeMethod.standalone-rustup`, src/lib/sources.ts).
pub fn uninstall_blocked(d: &Detected) -> Option<GateRefusal> {
    standard_roots(d).err()
}

/// Preserve the preview's existing fallback for a folder that cannot be
/// opened. Once names are read, any incomplete read refuses the preview.
fn preview_names(path: &Path, protected: &Protected) -> Result<Vec<std::ffi::OsString>, PathBuf> {
    let Ok(listing) = look::list(path, protected) else {
        return Ok(Vec::new());
    };
    listing
        .names(&mut look::ListingBudget::default())
        .map_err(|_| path.to_path_buf())
}

/// Every installed toolchain, by name: the entries of
/// `<rustup_home>/toolchains`, sorted, hidden names skipped. That
/// directory is what `uninstall()` removes toolchain by toolchain
/// (`cfg.list_toolchains()`, self_update.rs:955-958) before it deletes
/// the home whole, so its names are the toolchains that go; a linked
/// toolchain (`rustup toolchain link`) is an entry like any other. No
/// directory, or an unreadable one: no names, and the dialog says "every
/// toolchain" (ruling 15). Nothing is run, and nothing is looked up into
/// or through a protected place (`protected::look`). A listing that starts
/// but cannot finish within its budget refuses the preview at that folder.
pub fn toolchain_names(rustup_home: &Path, protected: &Protected) -> Result<Vec<String>, PathBuf> {
    let mut names: Vec<String> = preview_names(&rustup_home.join("toolchains"), protected)?
        .into_iter()
        .filter_map(|name| name.to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    Ok(names)
}

/// The programs in `<cargo_home>/bin` that rustup 1.29.1's `self
/// uninstall` deletes, by the names the Installed page gives them, sorted
/// and each once. rustup deletes every entry whose name is not `rustup`
/// or one of `RUSTUP_PROXIES` (self_update.rs:996-1022 compares names
/// only, so a program copied there by hand goes too), read here with
/// `read_dir` -- nothing is opened or run -- united with the binaries
/// Cargo's records list: `.crates2.json` brought up to date with
/// `.crates.toml` (`merge_crates_v1`, as the Cargo source reads them, so a
/// crate cargo-binstall recorded in `.crates.toml` alone counts too; a
/// `.crates.toml` that is missing or cannot be read or merged leaves
/// `.crates2.json` as it is), every crate's `bins` through
/// `parse_crates2_bins`, the parser cargo's own inventory uses. A
/// recorded program is named by its crate, as cargo's inventory names
/// that crate's row (`jj-cli`, whose program is `jj`; `ripgrep`, whose
/// is `rg`), so the list can be matched to the rows the user knows, and
/// a crate is named once however many programs it installed; a program
/// no record lists is named by its file name, in `unrecorded` when the
/// records were read in full (each one there merged, or none there at
/// all) -- Cargo did not install it, so reinstalling Rust will not bring
/// it back -- and in `recorded` when a record is there but could not be
/// read or merged, since then that is not known. The listing is what
/// rustup acts on; the record still names what cargo installed when the
/// directory cannot be listed, and a crate whose programs are already
/// gone is named although nothing is left to delete -- the safe
/// direction. Not named: an entry starting with `.` (`.DS_Store`:
/// deleted with the folder, but no program), and a name that is not
/// UTF-8 -- deleted with the folder too (`remove_dir`, :1029, takes
/// everything), but not spellable in a sentence. No directory, an
/// unreadable one, no record or a broken one each add nothing: a name is
/// better missing than invented, and `DeletesCargoHome` always says the
/// whole folder goes. Once enumeration starts, an incomplete read refuses
/// the preview rather than presenting a subset as the program list.
pub fn bin_programs_rustup_removes(
    cargo_home: &Path,
    protected: &Protected,
) -> Result<BinPrograms, PathBuf> {
    let removed =
        |name: &str| !name.starts_with('.') && name != "rustup" && !RUSTUP_PROXIES.contains(&name);
    let listed: Vec<String> = preview_names(&cargo_home.join("bin"), protected)?
        .into_iter()
        .filter_map(|name| name.to_str().map(str::to_string))
        .filter(|name| removed(name))
        .collect();
    // `Ok(None)`: no such file; `Err(())`: one there that cannot be read.
    let read =
        |name: &str| match crate::adapters::read_file::read_text(&cargo_home.join(name), protected)
        {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(()),
        };
    fn text(file: &Result<Option<String>, ()>) -> Option<&str> {
        file.as_ref().ok().and_then(Option::as_deref)
    }
    let crates2 = read(".crates2.json");
    let crates_toml = read(".crates.toml");
    let merged = text(&crates_toml).and_then(|crates_toml| {
        merge_crates_v1(text(&crates2).unwrap_or(r#"{"installs":{}}"#), crates_toml)
            .and_then(|json| parse_crates2_bins(&json))
            .ok()
    });
    let (recorded, complete): (Vec<(String, Vec<String>)>, bool) = match merged {
        Some(recorded) => (recorded, true),
        None => {
            let alone = text(&crates2).and_then(|json| parse_crates2_bins(json).ok());
            let complete = crates_toml == Ok(None) && (crates2 == Ok(None) || alone.is_some());
            (alone.unwrap_or_default(), complete)
        }
    };
    let mut crates: Vec<String> = recorded
        .iter()
        .filter(|(_, bins)| bins.iter().any(|bin| removed(bin)))
        .map(|(krate, _)| krate.clone())
        .collect();
    let mut others: Vec<String> = listed
        .into_iter()
        .filter(|name| !recorded.iter().any(|(_, bins)| bins.contains(name)))
        .collect();
    if !complete {
        crates.append(&mut others);
    }
    for names in [&mut crates, &mut others] {
        names.sort();
        names.dedup();
    }
    Ok(BinPrograms {
        recorded: crates,
        unrecorded: others,
    })
}

/// `bin_programs_rustup_removes`'s answer: the names for the
/// `RemovesCargoInstalled` line and for the `RemovesUnrecordedPrograms`
/// one, each sorted and each name once.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BinPrograms {
    pub recorded: Vec<String>,
    pub unrecorded: Vec<String>,
}

/// Whether Homebrew's `rustup` formula is installed: `<prefix>/Cellar/rustup`
/// exists under one of `prefixes` (`HOMEBREW_PREFIXES` in production).
/// rustup's homes depend only on `RUSTUP_HOME`/`CARGO_HOME`/`HOME`, never
/// on where the binary sits (`home::rustup_home_with_cwd_env`,
/// env.rs:101-113), so that rustup shares `~/.rustup` with the native
/// one and loses its toolchains when it goes (ruling 21). Read-only.
pub fn homebrew_rustup_present(prefixes: &[PathBuf], protected: &Protected) -> bool {
    prefixes.iter().any(|prefix| {
        look::target(&prefix.join("Cellar/rustup"), protected).is_ok_and(|(_, meta)| meta.is_dir())
    })
}

/// How rustup 1.29.1 spells the Cargo home in the line it writes and
/// looks for (`cargo_home_str_with_home`, shell.rs:43-58): `$HOME/.cargo`
/// when the Cargo home is `<home>/.cargo`, else the absolute path.
pub fn cargo_home_str(home: &Path, cargo_home: &Path) -> String {
    if cargo_home == home.join(".cargo") {
        "$HOME/.cargo".to_string()
    } else {
        cargo_home.display().to_string()
    }
}

/// One visit rustup 1.29.1's cleanup makes: remove the first exact copy
/// of `line` (followed by a newline, at a line start) from `file`, if the
/// file exists. The visits in order are `rustup_rc_visits`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RcVisit {
    pub file: PathBuf,
    pub line: String,
}

/// rustup 1.29.1's cleanup as a sequence of visits (ruling 2).
/// `do_remove_from_path` (unix.rs:55-77) takes the shells of
/// `enumerate_shells` in order (shell.rs:63-74) and, for each `rcfiles()`
/// that is a file, removes the first exact current line
/// (`. "<cargo_home_str>/env"`, `source_string`, shell.rs:138-140):
/// Posix's `~/.profile` (:163-168), Bash's `~/.bash_profile`,
/// `~/.bash_login`, `~/.bashrc` (:188-195), Zsh's `$ZDOTDIR/.zshenv`
/// when there is a ZDOTDIR and `~/.zshenv` (:240-245, no deduplication
/// -- `ZDOTDIR=$HOME` visits the same file twice); Fish, Nu, Tcsh, Pwsh
/// and Xonsh visit files Banager does not read. Then
/// `remove_legacy_paths` (unix.rs:174-194) removes the pre-1.23 line
/// `export PATH="<S>/bin:$PATH"` and then `source "<S>/env"`, each from
/// `legacy_paths` (shell.rs:564-574): `~/.bash_profile`, `~/.profile`,
/// `$ZDOTDIR/.zprofile` when there is a ZDOTDIR, `~/.zprofile`. Bash's
/// and Zsh's availability checks are folded in: a Bash file that is not
/// there is a no-op visit, and on a Mac zsh is at `/bin/zsh`. `zdotdir`
/// is `HostEnv.zdotdir` -- rustup itself asks `zsh -c 'echo -n $ZDOTDIR'`
/// when `SHELL` is not zsh (shell.rs:207-225), which Banager does not
/// (it runs nothing), so a ZDOTDIR set only inside a zsh startup file is
/// not modelled; an empty one is none (shell.rs:213). `~/.zshrc` and
/// fish's `config.fish` are visited by nothing.
pub fn rustup_rc_visits(home: &Path, zdotdir: Option<&Path>, cargo_home_str: &str) -> Vec<RcVisit> {
    let zdotdir = zdotdir.filter(|dir| !dir.as_os_str().is_empty());
    let current = format!(". \"{cargo_home_str}/env\"");
    let legacy_path = format!("export PATH=\"{cargo_home_str}/bin:$PATH\"");
    let legacy_source = format!("source \"{cargo_home_str}/env\"");
    let visit = |file: PathBuf, line: &str| RcVisit {
        file,
        line: line.to_string(),
    };
    let mut visits = Vec::new();
    for rc in [".profile", ".bash_profile", ".bash_login", ".bashrc"] {
        visits.push(visit(home.join(rc), &current));
    }
    if let Some(zdotdir) = zdotdir {
        visits.push(visit(zdotdir.join(".zshenv"), &current));
    }
    visits.push(visit(home.join(".zshenv"), &current));
    for line in [&legacy_path, &legacy_source] {
        for rc in [".bash_profile", ".profile"] {
            visits.push(visit(home.join(rc), line));
        }
        if let Some(zdotdir) = zdotdir {
            visits.push(visit(zdotdir.join(".zprofile"), line));
        }
        visits.push(visit(home.join(".zprofile"), line));
    }
    visits
}

/// rustup's `find_exact_line` (unix.rs:164-172) and the splice around it
/// (unix.rs:62-70, :150-158): `line` followed by a newline, at a line
/// start, byte for byte, first match only, removed. `false` when there is
/// no such line -- trailing space, another spelling, or the line last in
/// the file with no newline after it are not it.
pub fn remove_first_exact_line(contents: &mut String, line: &str) -> bool {
    let needle = format!("{line}\n");
    let at = {
        let bytes = contents.as_bytes();
        bytes
            .windows(needle.len())
            .enumerate()
            .find_map(|(at, window)| {
                (window == needle.as_bytes() && (at == 0 || bytes[at - 1] == b'\n')).then_some(at)
            })
    };
    match at {
        Some(at) => {
            contents.replace_range(at..at + needle.len(), "");
            true
        }
        None => false,
    }
}

/// How a startup file may speak of the Cargo env file (ruling 22).
/// `sourcing`: whole lines (trimmed) that fail whenever they run once the
/// file is gone -- the sourcing forms rustup itself writes (`. "<X>/env"`,
/// `source "<X>/env"`, fish's `source "<X>/env.fish"`) with `<X>` a
/// spelling whose target is this Cargo home. `needles`: substrings that
/// mention the env file at all, for the qualified tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftoverPatterns {
    pub sourcing: Vec<String>,
    pub needles: Vec<String>,
}

/// The patterns for `cargo_home` under `home`: for the default home the
/// spellings are `$HOME/.cargo` (what rustup writes) and the absolute
/// path (what rustup writes for a custom home that happens to be this
/// one), and the needle is `.cargo/env`; for a custom home only its
/// absolute path -- `$HOME/.cargo/env` then names a file this uninstall
/// leaves alone. `$CARGO_HOME/env` and `${CARGO_HOME}/env` are needles
/// always: what they load depends on the shell's own environment.
pub fn leftover_patterns(home: &Path, cargo_home: &Path) -> LeftoverPatterns {
    let absolute = cargo_home.display().to_string();
    let default = cargo_home == home.join(".cargo");
    let mut spellings = vec![absolute.clone()];
    if default {
        spellings.push("$HOME/.cargo".to_string());
    }
    let sourcing = spellings
        .iter()
        .flat_map(|spelling| {
            [
                format!(". \"{spelling}/env\""),
                format!("source \"{spelling}/env\""),
                format!("source \"{spelling}/env.fish\""),
            ]
        })
        .collect();
    let mut needles = vec![format!("{absolute}/env")];
    if default {
        needles.push(".cargo/env".to_string());
    }
    needles.push("$CARGO_HOME/env".to_string());
    needles.push("${CARGO_HOME}/env".to_string());
    LeftoverPatterns { sourcing, needles }
}

/// What a startup file will do about the Cargo env file after rustup's
/// cleanup: `Sources` -- a line rustup's own form spells with every line
/// above it standing alone (`stands_alone`), so, as far as the file's own
/// lines show, a shell that reads the file runs it and *will* print an
/// error; `Mentions` -- some other non-comment line naming the file, or
/// rustup's form below a line that does not stand alone, which *may*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leftover {
    Sources,
    Mentions,
}

/// The tier of a file's remaining contents, or `None` for a file that
/// says nothing about the env file outside comments. A comment is a
/// trimmed line starting with `#`; the certain tier wins over the
/// qualified one within a file. A line in one of rustup's sourcing forms
/// is certain only while every non-blank, non-comment line above it
/// stands alone (`stands_alone`): inside an `if`, a function body, a
/// here-document or a quote, or below any line Banager cannot vouch for,
/// whether it runs depends on shell syntax Banager does not follow --
/// once one line does not stand alone, none below it is certain.
pub fn classify_leftover(contents: &str, patterns: &LeftoverPatterns) -> Option<Leftover> {
    let mut mentions = false;
    let mut all_above_stand_alone = true;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let sources = patterns.sourcing.iter().any(|form| form == line);
        if sources && all_above_stand_alone {
            return Some(Leftover::Sources);
        }
        if sources
            || patterns
                .needles
                .iter()
                .any(|needle| line.contains(needle.as_str()))
        {
            mentions = true;
        }
        all_above_stand_alone = all_above_stand_alone && stands_alone(line);
    }
    mentions.then_some(Leftover::Mentions)
}

/// Words that, standing anywhere on a line -- quoted or not, delimited
/// by white space or by `;`, `&`, `|`, `(`, `)`, `{`, `}`, `<`, `>` --
/// keep it from standing alone (`stands_alone`): the reserved words of
/// sh's, bash's, zsh's and fish's conditionals, loops, `case` and
/// `switch`, functions, blocks and coprocesses, whose bodies run only
/// sometimes, more than once, when called or in the background; and the
/// commands that end the file or the shell before the lines below them
/// run -- `return`, `exit`, `logout`, zsh's `bye`, and `exec`, which
/// replaces the shell with the program it names.
const UNSURE_WORDS: [&str; 25] = [
    "if", "then", "elif", "else", "fi", "case", "esac", "for", "select", "while", "until", "do",
    "done", "repeat", "foreach", "function", "coproc", "begin", "end", "switch", "return", "exit",
    "logout", "bye", "exec",
];

/// Words that, last on a line, act on a command the line does not hold:
/// fish's `and`, `or` and `not`, and `!`. Read by `stands_alone`.
const DANGLING_WORDS: [&str; 4] = ["and", "or", "not", "!"];

/// Whether `line` -- one line of a startup file, trimmed, neither blank
/// nor a comment -- stands alone: read with sh's quoting, it is a whole
/// command that ends where the line ends, so the line below it is not
/// part of it or under its control. Not alone, read that way: a quote
/// left open (`'…'`, `"…"`, `$'…'`, `` `…` ``); a `(`, `{`, `$(` or `${`
/// left open, or a `)` or `}` that does not match the innermost one still
/// open on the line; `<<` outside quotes, a here-document, whose body is
/// the lines below; a `\` last, joining the next line on; a `\` inside
/// single quotes, which sh reads as itself and fish as an escape. Then,
/// over the line with its comment dropped and its quote marks and
/// escaping backslashes left out -- so a quoted `if` is still `if`: a `(`
/// and a `)` with nothing but blanks between them, as a function
/// definition has (zsh's body may be the next line); a `[[` word with no
/// `]]` word after it, or a `]]` with no `[[` before it; a `|`, `&&` or
/// `|&` at the end, or one of `DANGLING_WORDS` as the last word; and any
/// of `UNSURE_WORDS` as a word. A `#` first on the line or after a space
/// or tab, outside quotes and outside `${…}`, begins the comment; a `#`
/// anywhere else is read as part of the line. A small reader, not a
/// shell: it looks for what is listed here, and what a command on the
/// line does when it runs -- a file it loads, a string it evaluates, an
/// alias or an option it sets -- is not followed. Read by
/// `classify_leftover`.
fn stands_alone(line: &str) -> bool {
    /// What is open at a point of the line, innermost last.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Open {
        /// `(` or `$(`: commands inside.
        Paren,
        /// `{`: a group of commands, or a brace expansion.
        Brace,
        /// `${`: a parameter expansion, where `#` is an operator.
        Param,
        /// `"`: `\`, `$(`, `${` and `` ` `` still act inside.
        Double,
        /// `` ` ``: up to the next backtick not escaped, as POSIX reads it.
        Backtick,
    }
    let chars: Vec<char> = line.chars().collect();
    let mut open: Vec<Open> = Vec::new();
    // The line up to its comment, quote marks and escaping backslashes
    // left out, for the word checks below.
    let mut text = String::new();
    // Whether the character before this one was a space or tab outside
    // quotes (or this is the first): only then does `#` begin a comment.
    let mut after_blank = true;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let inside = open.last().copied();
        let mut blank = false;
        match c {
            '\\' => match next {
                Some(escaped) => {
                    text.push(escaped);
                    i += 1;
                }
                None => return false,
            },
            '`' if inside == Some(Open::Backtick) => {
                open.pop();
            }
            _ if inside == Some(Open::Backtick) => text.push(c),
            '"' if inside == Some(Open::Double) => {
                open.pop();
            }
            '`' => open.push(Open::Backtick),
            '$' if next == Some('(') => {
                open.push(Open::Paren);
                text.push_str("$(");
                i += 1;
            }
            '$' if next == Some('{') => {
                open.push(Open::Param);
                text.push_str("${");
                i += 1;
            }
            _ if inside == Some(Open::Double) => text.push(c),
            // Outside quotes from here on.
            '\'' => {
                let Some(len) = chars[i + 1..].iter().position(|&q| q == '\'') else {
                    return false;
                };
                let quoted = &chars[i + 1..i + 1 + len];
                if quoted.contains(&'\\') {
                    return false;
                }
                text.extend(quoted);
                i += len + 1;
            }
            '$' if next == Some('\'') => {
                // `$'…'`, where a `\` escapes the character after it.
                let mut j = i + 2;
                loop {
                    match chars.get(j) {
                        None => return false,
                        Some('\'') => break,
                        Some('\\') => match chars.get(j + 1) {
                            Some(&escaped) => {
                                text.push(escaped);
                                j += 2;
                            }
                            None => return false,
                        },
                        Some(&other) => {
                            text.push(other);
                            j += 1;
                        }
                    }
                }
                i = j;
            }
            // `$"…"` is `"…"` to this reader.
            '$' if next == Some('"') => {}
            '"' => open.push(Open::Double),
            '(' => {
                open.push(Open::Paren);
                text.push(c);
            }
            '{' => {
                open.push(Open::Brace);
                text.push(c);
            }
            ')' => {
                if open.pop() != Some(Open::Paren) {
                    return false;
                }
                text.push(c);
            }
            '}' => {
                if !matches!(open.pop(), Some(Open::Brace | Open::Param)) {
                    return false;
                }
                text.push(c);
            }
            '<' if next == Some('<') => return false,
            '#' if after_blank && inside != Some(Open::Param) => break,
            _ => {
                blank = c == ' ' || c == '\t';
                text.push(c);
            }
        }
        after_blank = blank;
        i += 1;
    }
    if !open.is_empty() {
        return false;
    }
    let code = text.trim_end();
    if code.ends_with('|') || code.ends_with("&&") || code.ends_with("|&") {
        return false;
    }
    let opens_function = code
        .char_indices()
        .any(|(at, c)| c == '(' && code[at + 1..].trim_start().starts_with(')'));
    let words: Vec<&str> = code
        .split(|c: char| c.is_whitespace() || ";&|(){}<>".contains(c))
        .filter(|word| !word.is_empty())
        .collect();
    let mut tests_open = 0usize;
    for word in &words {
        match *word {
            "[[" => tests_open += 1,
            "]]" if tests_open == 0 => return false,
            "]]" => tests_open -= 1,
            _ => {}
        }
    }
    !opens_function
        && tests_open == 0
        && !words
            .last()
            .is_some_and(|last| DANGLING_WORDS.contains(last))
        && !words.iter().any(|word| UNSURE_WORDS.contains(word))
}

/// The files `shell_config_leftovers` reads, in the order it reports
/// them: `SHELL_RC_CANDIDATES` under `home`, and, when `zdotdir` is an
/// absolute folder other than `home` (compared lexically, as
/// `rustup_rc_visits` spells its visits), zsh's `.zshenv`, `.zprofile`
/// and `.zshrc` under it, each just before the home's -- the order rustup
/// visits the first two in; rustup edits nobody's `.zshrc`, and a zsh
/// with a `ZDOTDIR` reads that folder's, not the home's. An empty
/// `ZDOTDIR` is none (shell.rs:213), the home's is these files (read
/// once, visited twice), and a relative one is not modelled: rustup
/// would resolve it against its own working directory, which this
/// preview does not know.
fn startup_files(home: &Path, zdotdir: Option<&Path>) -> Vec<PathBuf> {
    let zdotdir = zdotdir.filter(|dir| dir.is_absolute() && *dir != home);
    SHELL_RC_CANDIDATES
        .iter()
        .flat_map(|rc| {
            let under_zdotdir = zdotdir
                .filter(|_| matches!(*rc, ".zshenv" | ".zprofile" | ".zshrc"))
                .map(|dir| dir.join(rc));
            under_zdotdir.into_iter().chain([home.join(rc)])
        })
        .collect()
}

/// Which file a startup file's name leads to: `(st_dev, st_ino)` of the
/// file `read_startup_file` opened, links followed.
type FileIdentity = (u64, u64);

/// What `read_startup_file` found at a startup file's name.
enum StartupFile {
    /// Its contents and which file they are.
    Read(FileIdentity, String),
    /// Nothing to read: not there, not a regular file, not UTF-8, or one
    /// this account may not open.
    Skipped,
    /// In, or reached through, a place Banager never looks into
    /// (`protected::look`): not read, so what it holds is not known.
    Unread,
}

/// A startup file's contents and which file they are, or `Skipped` unless
/// `path` leads, links followed, to a regular file that reads as UTF-8 --
/// followed as rustup follows it: its cleanup visits a name only when
/// `is_file()` (unix.rs:60, :149), reads it with `fs::read_to_string`
/// (unix.rs:61, :150; utils/mod.rs:83-88) and, when it removes a line,
/// rewrites it by opening that name to truncate and write (unix.rs:69,
/// :158; utils/raw.rs:86-98), which changes the file the name leads to in
/// place. A name that is not a regular file is not opened (a named pipe
/// would wait for a writer), and the identity is the opened file's own
/// (`fstat`), the file whose bytes were read. One in or through a
/// protected place is `Unread`. Read by `shell_config_leftovers`.
fn read_startup_file(path: &Path, protected: &Protected) -> StartupFile {
    // Opened without waiting and read only when `fstat` says it is a
    // regular file, at most `read_file::LIMIT` bytes of it, and never in
    // or through a protected place: a `~/.zshrc` kept in iCloud Drive is
    // a name not read.
    match crate::adapters::read_file::read_regular(
        path,
        crate::adapters::read_file::LIMIT,
        protected,
    ) {
        Ok((meta, bytes)) => match String::from_utf8(bytes) {
            Ok(contents) => StartupFile::Read((meta.dev(), meta.ino()), contents),
            Err(_) => StartupFile::Skipped,
        },
        Err(error) if look::is_protected(&error) => StartupFile::Unread,
        Err(_) => StartupFile::Skipped,
    }
}

/// One `LeavesShellConfigLine` per startup file name (`startup_files`)
/// that will still speak of the Cargo env file after `rustup self
/// uninstall`, in that order, `path` spelled by the crate's one rule
/// (`scan::display_path`: `~/<file>` under the home, the full path
/// elsewhere), `certain` by tier. Each name is read once (read-only:
/// Banager never edits a startup file, spec §6.8), with the identity of
/// the file it leads to (`read_startup_file`), and the first name read
/// for a file gives that file its one copy; rustup's visits
/// (`rustup_rc_visits`) are replayed on the copy of the file the visited
/// name leads to -- so a second visit to the same file, under the same
/// name or another (`.bash_profile` a link to `.profile`, a `ZDOTDIR`
/// that is a link to the home), sees the first's result, as rustup's
/// does, and a line removed through one name is gone under every other
/// name of that file, `.zshrc` a link to `.zshenv` included -- and what
/// is left is classified (`classify_leftover`) and reported under each
/// name that leads to it. A visit to a name not read (a relative
/// `$ZDOTDIR`'s) has no copy to act on. A name in or through a protected
/// place (a `~/.zshrc` kept in iCloud Drive) is not read, and is said as
/// one Banager could not read (`ShellConfigUnread`), in its place in the
/// order: whether it will still speak of Cargo is not known, and saying
/// nothing would read as nothing being left.
pub fn shell_config_leftovers(
    home: &Path,
    zdotdir: Option<&Path>,
    cargo_home: &Path,
) -> Vec<Warning> {
    let spelled = cargo_home_str(home, cargo_home);
    let ordered = startup_files(home, zdotdir);
    let protected = Protected::new(home);
    let mut file_of: BTreeMap<PathBuf, FileIdentity> = BTreeMap::new();
    let mut copies: BTreeMap<FileIdentity, String> = BTreeMap::new();
    let mut unread: Vec<&PathBuf> = Vec::new();
    for path in &ordered {
        match read_startup_file(path, &protected) {
            StartupFile::Read(identity, contents) => {
                file_of.insert(path.clone(), identity);
                copies.entry(identity).or_insert(contents);
            }
            StartupFile::Unread => unread.push(path),
            StartupFile::Skipped => {}
        }
    }
    for visit in rustup_rc_visits(home, zdotdir, &spelled) {
        if let Some(contents) = file_of
            .get(&visit.file)
            .and_then(|identity| copies.get_mut(identity))
        {
            remove_first_exact_line(contents, &visit.line);
        }
    }
    let patterns = leftover_patterns(home, cargo_home);
    ordered
        .iter()
        .filter_map(|path| {
            let shown = || crate::scan::display_path(path, home).display().to_string();
            if unread.contains(&path) {
                return Some(Warning::ShellConfigUnread { path: shown() });
            }
            let contents = copies.get(file_of.get(path)?)?;
            let tier = classify_leftover(contents, &patterns)?;
            Some(Warning::LeavesShellConfigLine {
                path: shown(),
                certain: tier == Leftover::Sources,
            })
        })
        .collect()
}

/// The uninstall preview for `rustup self uninstall -y`, over a
/// caller-given list of Homebrew prefixes so the tests are hermetic: for
/// any layout but the standard one, the gate's refusal (`standard_roots`),
/// which `plan` refuses with; otherwise the dialog's list, in the order
/// it shows it (spec §6.6), built from the two roots that same answer
/// named -- the rustup home by path with every toolchain in it, the Cargo
/// home by path, the programs in its `bin/` when there are any (rulings 1
/// and 16: 1.29.1 removes the whole Cargo home), the Homebrew line when
/// its Cellar has a rustup (ruling 21), the shell edit, and each startup
/// file left speaking of Cargo's env file. The gate is asked here, once,
/// and not by `plan` before it: two readings of the disk let a layout
/// that changed between them pass the first and leave the second with
/// nothing to say, a plan with no warnings at all (step E's whole-step
/// review). Paths are spelled with `~` by the crate's one rule
/// (`scan::display_path`).
pub fn preview_with(
    d: &Detected,
    homebrew_prefixes: &[PathBuf],
) -> Result<Vec<Warning>, GateRefusal> {
    let roots = standard_roots(d)?;
    let protected = d.protected();
    let tilde = |path: &Path| {
        crate::scan::display_path(path, &d.home)
            .display()
            .to_string()
    };
    let mut warnings = vec![
        Warning::RemovesToolchains {
            path: tilde(&roots.rustup_home),
            names: toolchain_names(&roots.rustup_home, &protected).map_err(|path| GateRefusal {
                reason: UninstallBlocked::NoSafeMethod,
                path,
            })?,
        },
        Warning::DeletesCargoHome {
            path: tilde(&roots.cargo_home),
        },
    ];
    let bins =
        bin_programs_rustup_removes(&roots.cargo_home, &protected).map_err(|path| GateRefusal {
            reason: UninstallBlocked::NoSafeMethod,
            path,
        })?;
    if !bins.recorded.is_empty() {
        warnings.push(Warning::RemovesCargoInstalled {
            names: bins.recorded,
        });
    }
    if !bins.unrecorded.is_empty() {
        warnings.push(Warning::RemovesUnrecordedPrograms {
            names: bins.unrecorded,
        });
    }
    if homebrew_rustup_present(homebrew_prefixes, &protected) {
        warnings.push(Warning::HomebrewRustupLosesToolchains);
    }
    warnings.push(Warning::EditsShellConfig);
    warnings.extend(shell_config_leftovers(
        &d.home,
        d.zdotdir.as_deref(),
        &roots.cargo_home,
    ));
    Ok(warnings)
}

/// `CommandUninstall.preview` of the `RUSTUP` recipe: `preview_with`
/// over Homebrew's real prefixes, under the seat's `machine_root` (`/`
/// but in tests).
pub fn uninstall_preview(d: &Detected) -> Result<Vec<Warning>, GateRefusal> {
    let prefixes: Vec<PathBuf> = HOMEBREW_PREFIXES
        .iter()
        .map(|prefix| d.on_this_mac(prefix))
        .collect();
    preview_with(d, &prefixes)
}

/// The cargo instance's lock, spelled by the one function that spells
/// its id (`cargo::instance_id_for`, spec §2.4), for both of rustup's
/// plans: `self update` unlinks and re-copies `$CARGO_HOME/bin/rustup`
/// (`install_bins`, self_update.rs:771-785), the file the cargo
/// instance's `cargo` proxy runs, so a cargo read in that window would
/// find a missing or half-written binary; `self uninstall` deletes the
/// `.crates2.json` cargo's inventory reads. None when there is no usable
/// Cargo home (then there is no rustup instance either).
/// `RUSTUP.extra_locks`.
pub fn extra_locks(d: &Detected) -> Vec<ResourceLock> {
    d.cargo_home
        .as_deref()
        .map(|cargo_home| ResourceLock(instance_id_for(cargo_home)))
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::testing::{detected, rustup_layout, TempHome, Unreadable};
    use super::*;
    use crate::model::UninstallBlocked;
    use std::path::PathBuf;

    #[test]
    fn bounded_rustup_roots_refuse_an_unchecked_tail() {
        // Exactly the budget's 4096 names is a complete check; one more
        // and the names after the 4096th -- where a link could be -- were
        // never looked at, so the gate refuses at the root.
        let home = TempHome::new("bounded-rustup-root");
        let cargo_home = home.dir(".cargo");
        for n in 0..4096 {
            home.file(&format!(".cargo/file-{n}"));
        }
        let d = detected(home.path(), &cargo_home);
        assert!(standard_roots(&d).is_ok());
        home.file(".cargo/file-4096");
        assert_eq!(
            standard_roots(&d),
            Err(GateRefusal {
                reason: UninstallBlocked::NoSafeMethod,
                path: cargo_home,
            })
        );
    }

    #[test]
    fn bounded_rustup_preview_refuses_incomplete_toolchains_or_programs() {
        for folder in [".rustup/toolchains", ".cargo/bin"] {
            let home = TempHome::new("bounded-rustup-preview");
            let cargo_home = home.dir(".cargo");
            home.dir(folder);
            for n in 0..4097 {
                home.file(&format!("{folder}/file-{n}"));
            }
            assert_eq!(
                preview_with(&detected(home.path(), &cargo_home), &[]),
                Err(GateRefusal {
                    reason: UninstallBlocked::NoSafeMethod,
                    path: home.path().join(folder),
                }),
                "{folder}"
            );
        }
    }

    #[test]
    fn regression_a_named_pipe_for_a_cargo_record_is_not_waited_on() {
        use crate::adapters::read_file::tests::{finishes, make_fifo, temp_dir};
        let cargo_home = temp_dir("rustup-crates2-fifo");
        std::fs::create_dir_all(cargo_home.join("bin")).unwrap();
        make_fifo(&cargo_home.join(".crates2.json"));
        let home = cargo_home.clone();
        let names = finishes(move || {
            bin_programs_rustup_removes(&home, &Protected::of_this_process()).unwrap()
        });
        assert_eq!(names, BinPrograms::default());
        let _ = std::fs::remove_dir_all(&cargo_home);
    }

    fn rc_line() -> String {
        ". \"$HOME/.cargo/env\"".to_string()
    }

    /// The folder a refused layout is refused at, for the assertions
    /// below: `execute` names it when the gate the preview passed refuses
    /// right before the spawn.
    fn refused_at(d: &Detected) -> PathBuf {
        let refusal = standard_roots(d).expect_err("not the standard layout");
        assert_eq!(refusal.reason, UninstallBlocked::NoSafeMethod);
        refusal.path
    }

    #[test]
    fn test_standard_roots_accepts_only_the_default_layout_of_real_directories() {
        // Ruling 18: both roots as rustup computes them (`home` 0.5.12),
        // both exactly `<home>/.cargo` and `<home>/.rustup`, the Cargo
        // home a real directory, the rustup home a real directory or not
        // there yet (and nothing at the top of either a link: the next
        // two tests). Anything else is a layout Banager will not offer to
        // delete: rustup's `uninstall()` removes `$RUSTUP_HOME` and
        // `$CARGO_HOME` whole (self_update.rs:960-966, :1029), wherever
        // they point. Each refusal here names the standard folder the
        // rule failed at: the Cargo home first, the rustup home once the
        // Cargo home passes.
        let home = TempHome::new("roots-default");
        let cargo_home = home.dir(".cargo");
        let rustup_home = home.dir(".rustup");
        let roots =
            standard_roots(&detected(home.path(), &cargo_home)).expect("the default layout");
        assert_eq!(roots.cargo_home, cargo_home);
        assert_eq!(roots.rustup_home, rustup_home);

        // No `~/.rustup` yet (rustup itself creates it on first run):
        // still the standard layout.
        let home = TempHome::new("roots-no-rustup-home");
        let cargo_home = home.dir(".cargo");
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_ok());

        // Custom, absolute: not offered.
        let home = TempHome::new("roots-custom-cargo");
        let custom = home.dir("elsewhere/cargo");
        assert_eq!(
            refused_at(&detected(home.path(), &custom)),
            home.path().join(".cargo")
        );
        let home = TempHome::new("roots-custom-rustup");
        let cargo_home = home.dir(".cargo");
        let d = Detected {
            rustup_home: Some(home.dir("elsewhere/rustup")),
            ..detected(home.path(), &cargo_home)
        };
        assert_eq!(refused_at(&d), home.path().join(".rustup"));

        // Relative (unsupported, seated as `None`): not offered.
        let home = TempHome::new("roots-relative");
        let cargo_home = home.dir(".cargo");
        let d = Detected {
            cargo_home: None,
            ..detected(home.path(), &cargo_home)
        };
        assert_eq!(refused_at(&d), home.path().join(".cargo"));
        let d = Detected {
            rustup_home: None,
            ..detected(home.path(), &cargo_home)
        };
        assert_eq!(refused_at(&d), home.path().join(".rustup"));

        // A root that is a link: the path Banager would list is not the
        // directory that would go.
        let home = TempHome::new("roots-linked-cargo");
        let elsewhere = home.dir("Volumes/Data/cargo");
        home.link(".cargo", &elsewhere);
        assert_eq!(
            refused_at(&detected(home.path(), &home.path().join(".cargo"))),
            home.path().join(".cargo")
        );
        let home = TempHome::new("roots-linked-rustup");
        let cargo_home = home.dir(".cargo");
        let elsewhere = home.dir("Volumes/Data/rustup");
        home.link(".rustup", &elsewhere);
        assert_eq!(
            refused_at(&detected(home.path(), &cargo_home)),
            home.path().join(".rustup")
        );

        // No `~/.cargo` at all: nothing to offer.
        let home = TempHome::new("roots-no-cargo-home");
        assert_eq!(
            refused_at(&detected(home.path(), &home.path().join(".cargo"))),
            home.path().join(".cargo")
        );
    }

    #[test]
    fn test_standard_roots_refuses_a_link_at_the_top_of_either_root() {
        // Step E's whole-step review. rustup 1.29.1 reaches
        // `toolchains/<name>` (`list_toolchains`, config.rs:922-940, then
        // `Toolchain::ensure_removed`, toolchain.rs:536-582),
        // `update-hashes/<name>` (`installed_paths`, config.rs:464-476) and
        // `bin/<name>` (self_update.rs:1003-1022) *through* their parent,
        // deciding with `is_directory`/`is_dir()`, which follow a link: a
        // link at one of those three names sends it deleting inside
        // wherever the link leads -- outside the two folders the preview
        // names. Every other link at the top of a root it unlinks without
        // following (`raw::remove_dir`, raw.rs:277-311; `remove_dir_all`
        // 1.0.0, `src/_impl.rs:133-213`, `.follow(false)`). Banager keeps
        // no list of which names rustup follows: any link at the top of
        // either root refuses, at that link -- the first by name when
        // there are several -- so `execute` can name it.
        let home = TempHome::new("roots-linked-bin");
        let cargo_home = home.dir(".cargo");
        home.dir(".rustup");
        let shared = home.dir("Volumes/Data/bin");
        home.link(".cargo/bin", &shared);
        assert_eq!(
            refused_at(&detected(home.path(), &cargo_home)),
            cargo_home.join("bin")
        );

        let home = TempHome::new("roots-linked-toolchains");
        let cargo_home = home.dir(".cargo");
        home.dir(".rustup");
        let elsewhere = home.dir("Volumes/Data/toolchains");
        home.link(".rustup/toolchains", &elsewhere);
        assert_eq!(
            refused_at(&detected(home.path(), &cargo_home)),
            home.path().join(".rustup/toolchains")
        );

        // A link rustup would only unlink -- dangling, or to a file --
        // refuses too.
        let home = TempHome::new("roots-linked-registry");
        let cargo_home = home.dir(".cargo");
        home.link(".cargo/registry", Path::new("/Volumes/Gone/registry"));
        assert_eq!(
            refused_at(&detected(home.path(), &cargo_home)),
            cargo_home.join("registry")
        );
        let home = TempHome::new("roots-linked-settings");
        let cargo_home = home.dir(".cargo");
        home.dir(".rustup");
        let settings = home.file("elsewhere/settings.toml");
        home.link(".rustup/settings.toml", &settings);
        assert_eq!(
            refused_at(&detected(home.path(), &cargo_home)),
            home.path().join(".rustup/settings.toml")
        );

        // Several links: the first by name, so the refusal is the same
        // every time; and the Cargo home is asked before the rustup home.
        let home = TempHome::new("roots-several-links");
        let cargo_home = home.dir(".cargo");
        home.dir(".rustup");
        home.link(".cargo/registry", Path::new("/Volumes/Gone/registry"));
        home.link(".cargo/bin", Path::new("/Volumes/Gone/bin"));
        home.link(".rustup/toolchains", Path::new("/Volumes/Gone/toolchains"));
        assert_eq!(
            refused_at(&detected(home.path(), &cargo_home)),
            cargo_home.join("bin")
        );

        // Links deeper down are the standard layout itself: rustup's
        // thirteen proxies are links in `bin/`, and a linked toolchain
        // (`rustup toolchain link`) is a link in `toolchains/`; rustup
        // unlinks each without following, and the gate passes.
        let home = TempHome::new("roots-deeper-links");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        let linked = home.dir("src/my-toolchain");
        home.link(".rustup/toolchains/custom", &linked);
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_ok());
    }

    #[test]
    fn test_standard_roots_refuses_a_root_it_cannot_list() {
        // A root whose entries cannot be listed may hold a link the gate
        // cannot see: refused, at the root, rather than passed on a guess
        // -- "could not tell" is not "nothing there". Each root in turn;
        // with the permissions back, the layout passes. Skipped as root,
        // whom permissions do not stop (`Unreadable`).
        let home = TempHome::new("roots-unlistable");
        let cargo_home = home.dir(".cargo");
        let rustup_home = home.dir(".rustup");
        {
            let Some(_locked) = Unreadable::new(&cargo_home) else {
                return;
            };
            assert_eq!(refused_at(&detected(home.path(), &cargo_home)), cargo_home);
        }
        {
            let Some(_locked) = Unreadable::new(&rustup_home) else {
                return;
            };
            assert_eq!(refused_at(&detected(home.path(), &cargo_home)), rustup_home);
        }
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_ok());
    }

    #[test]
    fn test_uninstall_blocked_is_the_gates_refusal_for_anything_but_the_standard_layout() {
        let home = TempHome::new("blocked");
        let cargo_home = home.dir(".cargo");
        assert_eq!(uninstall_blocked(&detected(home.path(), &cargo_home)), None);
        let custom = home.dir("elsewhere/cargo");
        assert_eq!(
            uninstall_blocked(&detected(home.path(), &custom)),
            Some(GateRefusal {
                reason: UninstallBlocked::NoSafeMethod,
                path: home.path().join(".cargo"),
            })
        );
        // The same seat, the disk changed: the folder that is now a link
        // is the one named, which is what `execute` shows when this
        // happens between the preview and the click.
        let elsewhere = home.dir("Volumes/Data/rustup");
        home.link(".rustup", &elsewhere);
        assert_eq!(
            uninstall_blocked(&detected(home.path(), &cargo_home)),
            Some(GateRefusal {
                reason: UninstallBlocked::NoSafeMethod,
                path: home.path().join(".rustup"),
            })
        );
    }

    #[test]
    fn test_toolchain_names_is_empty_for_no_directory_and_lists_entries_sorted() {
        // `uninstall()` removes each entry of `<rustup_home>/toolchains`
        // (`cfg.list_toolchains()`, self_update.rs:955-958) and then the
        // whole home, so the entry names are the toolchains that go. A
        // linked toolchain (`rustup toolchain link`) is an entry too;
        // `.DS_Store` is not a toolchain. None, or an unreadable
        // directory: no names, and the dialog says "every toolchain"
        // (ruling 15).
        let home = TempHome::new("toolchains-none");
        assert_eq!(
            toolchain_names(&home.path().join(".rustup"), &Protected::of_this_process()).unwrap(),
            Vec::<String>::new()
        );
        home.dir(".rustup");
        assert_eq!(
            toolchain_names(&home.path().join(".rustup"), &Protected::of_this_process()).unwrap(),
            Vec::<String>::new()
        );

        let home = TempHome::new("toolchains-some");
        let rustup_home = home.dir(".rustup");
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        home.dir(".rustup/toolchains/nightly-2026-09-01-aarch64-apple-darwin");
        home.dir(".rustup/toolchains/1.90.0-aarch64-apple-darwin");
        home.file(".rustup/toolchains/.DS_Store");
        let linked = home.dir("src/my-toolchain");
        home.link(".rustup/toolchains/custom", &linked);
        assert_eq!(
            toolchain_names(&rustup_home, &Protected::of_this_process()).unwrap(),
            vec![
                "1.90.0-aarch64-apple-darwin".to_string(),
                "custom".to_string(),
                "nightly-2026-09-01-aarch64-apple-darwin".to_string(),
                "stable-aarch64-apple-darwin".to_string(),
            ]
        );
    }

    #[test]
    fn test_rustup_proxies_are_the_thirteen_names_rustup_keeps() {
        // `TOOLS` (10) + `DUP_TOOLS` (3), rustup 1.29.1 `src/lib.rs:16-32`:
        // the names `uninstall()` spares in `bin/` besides `rustup`
        // itself (self_update.rs:996-1022). Sorted and unique, so a
        // missing or doubled name shows here, not as a program the
        // preview wrongly names.
        assert_eq!(RUSTUP_PROXIES.len(), 13);
        let mut sorted = RUSTUP_PROXIES.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, RUSTUP_PROXIES.to_vec());
        for name in [
            "cargo",
            "rustc",
            "rustdoc",
            "rustfmt",
            "cargo-fmt",
            "rust-analyzer",
        ] {
            assert!(RUSTUP_PROXIES.contains(&name), "{name}");
        }
        assert!(!RUSTUP_PROXIES.contains(&"rustup"));
    }

    #[test]
    fn test_bin_programs_rustup_removes_lists_the_recorded_programs_and_not_rustups_own() {
        // This Mac's layout: rustup, its thirteen proxies, and `hexyl`,
        // which the recorded `.crates2.json` lists.
        let home = TempHome::new("rustup-bins");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/hexyl"), b"x").expect("write hexyl");
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy the recorded record");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            programs(&["hexyl"], &[])
        );
    }

    fn programs(recorded: &[&str], unrecorded: &[&str]) -> BinPrograms {
        let owned = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
        BinPrograms {
            recorded: owned(recorded),
            unrecorded: owned(unrecorded),
        }
    }

    #[test]
    fn test_bin_programs_rustup_removes_names_a_recorded_program_by_its_crate() {
        // The Installed page names a cargo row by its crate: `jj-cli`,
        // whose program is `jj`, and `ripgrep`, whose is `rg`. The
        // uninstall's list names them the same way, once a crate however
        // many programs it installed, so that each can be matched to its
        // row; `mytool`, which no record lists, keeps its file's name.
        let home = TempHome::new("rustup-bins-by-crate");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        for program in ["jj", "rg", "mytool"] {
            std::fs::write(cargo_home.join("bin").join(program), b"x").expect("write a program");
        }
        std::fs::write(
            cargo_home.join(".crates2.json"),
            r#"{"installs":{
                "jj-cli 0.40.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["jj"]},
                "ripgrep 15.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["rg"]},
                "cargo-binstall 1.17.4 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["cargo-binstall","detect-targets"]}
            }}"#,
        )
        .expect("write the record");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            programs(&["cargo-binstall", "jj-cli", "ripgrep"], &["mytool"])
        );
    }

    #[test]
    fn regression_bin_programs_rustup_removes_names_a_crate_only_crates_toml_records_by_its_crate()
    {
        // cargo-binstall records what it installs in `.crates.toml` alone,
        // and the Cargo source reads that file merged with `.crates2.json`
        // (`merge_crates_v1`), so its rows are `ripgrep` and `du-dust`,
        // whose programs are `rg` and `dust`. The list names them the same
        // way, also with no `.crates2.json` at all; a `.crates.toml` that
        // cannot be read leaves `.crates2.json` alone.
        let home = TempHome::new("rustup-bins-crates-toml");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        for program in ["hexyl", "rg", "dust", "cargo-binstall"] {
            std::fs::write(cargo_home.join("bin").join(program), b"x").expect("write a program");
        }
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy the recorded record");
        std::fs::write(
            cargo_home.join(".crates.toml"),
            r#"[v1]
"cargo-binstall 1.17.4 (registry+https://github.com/rust-lang/crates.io-index)" = ["cargo-binstall"]
"du-dust 1.2.3 (registry+https://github.com/rust-lang/crates.io-index)" = ["dust"]
"hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)" = ["hexyl"]
"ripgrep 15.1.0 (registry+https://github.com/rust-lang/crates.io-index)" = ["rg"]
"#,
        )
        .expect("write .crates.toml");
        let by_crate = programs(&["cargo-binstall", "du-dust", "hexyl", "ripgrep"], &[]);
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            by_crate
        );
        std::fs::remove_file(cargo_home.join(".crates2.json")).expect("remove .crates2.json");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            by_crate
        );
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy the recorded record");
        // Whether Cargo installed `rg` is then not known: on the Cargo line.
        std::fs::write(cargo_home.join(".crates.toml"), "[v1\n").expect("break .crates.toml");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            programs(&["cargo-binstall", "dust", "hexyl", "rg"], &[])
        );
    }

    #[test]
    fn test_bin_programs_rustup_removes_names_a_program_no_record_lists() {
        // rustup 1.29.1 deletes every entry of `bin/` whose *name* is not
        // one of its fourteen (self_update.rs:996-1022): a program copied
        // there by hand goes too, recorded or not, and with no record or
        // a broken one the listing still names it: with no record as one
        // Cargo did not install, with a broken one as one it may have.
        // `.DS_Store` is deleted with the folder but is no program to name.
        let home = TempHome::new("rustup-bins-unrecorded");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/mytool"), b"x").expect("write mytool");
        std::fs::write(cargo_home.join("bin/.DS_Store"), b"x").expect("write .DS_Store");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            programs(&[], &["mytool"])
        );
        std::fs::write(cargo_home.join(".crates2.json"), "{ not json").expect("write");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            programs(&["mytool"], &[])
        );
    }

    #[test]
    fn test_bin_programs_rustup_removes_names_each_recorded_crate_once_sorted() {
        // A crate by its own name (`ripgrep`, not `rg`), once however many
        // binaries it installed, whether or not the listing has them.
        let home = TempHome::new("rustup-bins-many");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        for bin in ["hexyl", "rg"] {
            std::fs::write(cargo_home.join("bin").join(bin), b"x").expect("write bin");
        }
        std::fs::write(
            cargo_home.join(".crates2.json"),
            r#"{"installs":{
                "ripgrep 15.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["rg"]},
                "cargo-binstall 1.16.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["detect-targets","cargo-binstall"]},
                "hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["hexyl"]}
            }}"#,
        )
        .expect("write record");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            programs(&["cargo-binstall", "hexyl", "ripgrep"], &[])
        );
    }

    #[test]
    fn test_bin_programs_rustup_removes_is_empty_for_no_directory_and_no_record() {
        // Nothing to list and nothing recorded (or a broken record): no
        // names, so no `RemovesCargoInstalled` line -- never a refused
        // preview; the Cargo-folder sentence is always there.
        let home = TempHome::new("rustup-bins-none");
        let cargo_home = home.dir(".cargo");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            BinPrograms::default()
        );
        std::fs::write(cargo_home.join(".crates2.json"), "{ not json").expect("write");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            BinPrograms::default()
        );
        // rustup and its proxies alone: nothing else to name.
        rustup_layout(&cargo_home);
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home, &Protected::of_this_process()).unwrap(),
            BinPrograms::default()
        );
    }

    #[test]
    fn test_homebrew_rustup_present_looks_for_the_formulas_cellar_directory() {
        // Homebrew's keg-only `rustup` formula lives under
        // `<prefix>/Cellar/rustup/<version>/`; that directory existing is
        // the one read-only, local sign of it (ruling 21). The real
        // prefixes are `HOMEBREW_PREFIXES`; the tests hand in their own.
        let home = TempHome::new("brew-present");
        let prefix = home.dir("opt/homebrew");
        assert!(!homebrew_rustup_present(
            std::slice::from_ref(&prefix),
            &Protected::of_this_process()
        ));
        home.dir("opt/homebrew/Cellar/rustup/1.29.1/bin");
        assert!(homebrew_rustup_present(
            std::slice::from_ref(&prefix),
            &Protected::of_this_process()
        ));
        assert!(homebrew_rustup_present(
            &[home.path().join("usr/local"), prefix],
            &Protected::of_this_process()
        ));
        assert!(!homebrew_rustup_present(&[], &Protected::of_this_process()));
        assert_eq!(HOMEBREW_PREFIXES, ["/opt/homebrew", "/usr/local"]);
    }

    #[test]
    fn test_cargo_home_str_spells_the_default_home_as_rustup_writes_it() {
        // rustup 1.29.1 `cargo_home_str_with_home`
        // (src/cli/self_update/shell.rs:43-58): `$HOME/.cargo` when the
        // Cargo home is the default, so that is the text in the line it
        // wrote and the line it looks for.
        assert_eq!(
            cargo_home_str(
                Path::new("/Users/someone"),
                Path::new("/Users/someone/.cargo")
            ),
            "$HOME/.cargo"
        );
    }

    #[test]
    fn test_cargo_home_str_spells_a_custom_home_absolutely() {
        assert_eq!(
            cargo_home_str(
                Path::new("/Users/someone"),
                Path::new("/Volumes/Data/cargo")
            ),
            "/Volumes/Data/cargo"
        );
    }

    #[test]
    fn test_rustup_rc_visits_follow_do_remove_from_path_then_remove_legacy_paths() {
        // Ruling 2: `do_remove_from_path` (unix.rs:55-77) over the shells
        // in `enumerate_shells` order (shell.rs:63-74) -- Posix `.profile`
        // (:163-168), Bash `.bash_profile`/`.bash_login`/`.bashrc`
        // (:188-195), Zsh `~/.zshenv` (:240-245; `$ZDOTDIR/.zshenv` first
        // when there is one) -- each visit removing the current line; then
        // `remove_legacy_paths` (unix.rs:174-194): the pre-1.23 PATH line
        // and then the `source` line, each over `legacy_paths`
        // (shell.rs:564-574: `.bash_profile`, `.profile`,
        // `$ZDOTDIR/.zprofile`, `~/.zprofile`). Fish, Nu, Tcsh, Pwsh and
        // Xonsh edit files Banager does not read, so they have no visit
        // here. `.zshrc` and fish's `config.fish` are visited by nothing.
        let home = Path::new("/Users/someone");
        let current = rc_line();
        let legacy_path = "export PATH=\"$HOME/.cargo/bin:$PATH\"".to_string();
        let legacy_source = "source \"$HOME/.cargo/env\"".to_string();
        let visits = rustup_rc_visits(home, None, "$HOME/.cargo");
        let as_pairs: Vec<(String, String)> = visits
            .iter()
            .map(|v| {
                (
                    v.file.strip_prefix(home).unwrap().display().to_string(),
                    v.line.clone(),
                )
            })
            .collect();
        assert_eq!(
            as_pairs,
            vec![
                (".profile".to_string(), current.clone()),
                (".bash_profile".to_string(), current.clone()),
                (".bash_login".to_string(), current.clone()),
                (".bashrc".to_string(), current.clone()),
                (".zshenv".to_string(), current.clone()),
                (".bash_profile".to_string(), legacy_path.clone()),
                (".profile".to_string(), legacy_path.clone()),
                (".zprofile".to_string(), legacy_path.clone()),
                (".bash_profile".to_string(), legacy_source.clone()),
                (".profile".to_string(), legacy_source.clone()),
                (".zprofile".to_string(), legacy_source.clone()),
            ]
        );
        assert!(visits.iter().all(|v| !v.file.ends_with(".zshrc")));
        // A custom Cargo home is spelled absolutely in every line.
        let visits = rustup_rc_visits(home, None, "/Volumes/Data/cargo");
        assert_eq!(visits[0].line, ". \"/Volumes/Data/cargo/env\"");
        assert_eq!(
            visits[5].line,
            "export PATH=\"/Volumes/Data/cargo/bin:$PATH\""
        );
        assert_eq!(visits[8].line, "source \"/Volumes/Data/cargo/env\"");
    }

    #[test]
    fn test_rustup_rc_visits_visit_zshenv_twice_when_zdotdir_is_home() {
        // Zsh's `rcfiles()` is `[$ZDOTDIR/.zshenv, ~/.zshenv]` with no
        // deduplication (shell.rs:240-245), and `legacy_paths` chains
        // `$ZDOTDIR/.zprofile` before `~/.zprofile` (shell.rs:564-574):
        // with `ZDOTDIR=$HOME` the same file is visited twice per line,
        // and each visit removes one exact copy. Another ZDOTDIR gets its
        // own visits, to its `.zshenv` and `.zprofile`, which
        // `shell_config_leftovers` reads under their own names -- one copy
        // per file, so a ZDOTDIR that is a link to the home shares the
        // home's. An empty ZDOTDIR is no ZDOTDIR (shell.rs:213).
        let home = Path::new("/Users/someone");
        let visits = rustup_rc_visits(home, Some(home), "$HOME/.cargo");
        let zshenv: Vec<_> = visits
            .iter()
            .filter(|v| v.file == home.join(".zshenv"))
            .collect();
        assert_eq!(zshenv.len(), 2);
        let zprofile: Vec<_> = visits
            .iter()
            .filter(|v| v.file == home.join(".zprofile"))
            .collect();
        assert_eq!(zprofile.len(), 4, "two legacy lines, two visits each");

        let elsewhere = Path::new("/Users/someone/.config/zsh");
        let visits = rustup_rc_visits(home, Some(elsewhere), "$HOME/.cargo");
        assert_eq!(
            visits
                .iter()
                .filter(|v| v.file == home.join(".zshenv"))
                .count(),
            1
        );
        assert_eq!(
            visits
                .iter()
                .filter(|v| v.file == elsewhere.join(".zshenv"))
                .count(),
            1
        );
        assert_eq!(
            visits
                .iter()
                .filter(|v| v.file == elsewhere.join(".zprofile"))
                .count(),
            2
        );

        let visits = rustup_rc_visits(home, Some(Path::new("")), "$HOME/.cargo");
        assert_eq!(
            visits
                .iter()
                .filter(|v| v.file == home.join(".zshenv"))
                .count(),
            1
        );
    }

    #[test]
    fn test_remove_first_exact_line_is_find_exact_line() {
        // `find_exact_line` (unix.rs:164-172): the line *with* its
        // newline, at a line start, byte for byte, first match only.
        let line = rc_line();
        let mut s = format!("{line}\n");
        assert!(remove_first_exact_line(&mut s, &line));
        assert_eq!(s, "");
        // Two copies: one goes per call.
        let mut s = format!("{line}\n{line}\n");
        assert!(remove_first_exact_line(&mut s, &line));
        assert_eq!(s, format!("{line}\n"));
        // Trailing whitespace is not the exact line.
        let mut s = format!("{line} \n");
        assert!(!remove_first_exact_line(&mut s, &line));
        // The same text as the last line with no newline after it stays.
        let mut s = format!("export A=1\n{line}");
        assert!(!remove_first_exact_line(&mut s, &line));
        // Not at a line start: stays.
        let mut s = format!("x {line}\n");
        assert!(!remove_first_exact_line(&mut s, &line));
        // At the start of a later line: goes, the rest intact.
        let mut s = format!("export A=1\n{line}\nexport B=2\n");
        assert!(remove_first_exact_line(&mut s, &line));
        assert_eq!(s, "export A=1\nexport B=2\n");
    }

    #[test]
    fn test_leftover_patterns_name_rustups_sourcing_forms_and_the_env_files_spellings() {
        // The certain tier is only a form rustup itself writes whose target
        // is this Cargo home (ruling 22); the needles are how the env file
        // may be mentioned. With a custom home, `$HOME/.cargo/env` is
        // neither: that file survives this uninstall.
        let home = Path::new("/Users/someone");
        let p = leftover_patterns(home, Path::new("/Users/someone/.cargo"));
        for form in [
            ". \"$HOME/.cargo/env\"",
            "source \"$HOME/.cargo/env\"",
            "source \"$HOME/.cargo/env.fish\"",
            ". \"/Users/someone/.cargo/env\"",
            "source \"/Users/someone/.cargo/env\"",
        ] {
            assert!(p.sourcing.iter().any(|s| s == form), "{form}");
        }
        assert!(p.needles.contains(&".cargo/env".to_string()));
        assert!(p.needles.contains(&"$CARGO_HOME/env".to_string()));
        assert!(p.needles.contains(&"${CARGO_HOME}/env".to_string()));

        let p = leftover_patterns(home, Path::new("/Volumes/Data/cargo"));
        assert!(p
            .sourcing
            .contains(&". \"/Volumes/Data/cargo/env\"".to_string()));
        assert!(!p.sourcing.iter().any(|s| s.contains("$HOME")));
        assert!(p.needles.contains(&"/Volumes/Data/cargo/env".to_string()));
        assert!(!p.needles.contains(&".cargo/env".to_string()));
    }

    #[test]
    fn test_classify_leftover_puts_rustups_own_forms_in_the_certain_tier_and_the_rest_in_the_qualified_one(
    ) {
        // Astra's counterexamples (finding 7), each decided on its own.
        let home = Path::new("/Users/someone");
        let p = leftover_patterns(home, Path::new("/Users/someone/.cargo"));
        // rustup's own line, left behind: will error.
        assert_eq!(
            classify_leftover(". \"$HOME/.cargo/env\"\n", &p),
            Some(Leftover::Sources)
        );
        assert_eq!(
            classify_leftover("  source \"$HOME/.cargo/env\"  \n", &p),
            Some(Leftover::Sources)
        );
        assert_eq!(
            classify_leftover("export A=1\n. \"$HOME/.cargo/env\"", &p),
            Some(Leftover::Sources),
            "last line, no newline: rustup leaves it, the shell runs it"
        );
        // A comment: nothing.
        assert_eq!(classify_leftover("# . \"$HOME/.cargo/env\"\n", &p), None);
        // An echo, a guard, another spelling, a variable: may.
        assert_eq!(
            classify_leftover("echo \"run . $HOME/.cargo/env\"\n", &p),
            Some(Leftover::Mentions)
        );
        assert_eq!(
            classify_leftover(
                "[ -f \"$HOME/.cargo/env\" ] && . \"$HOME/.cargo/env\"\n",
                &p
            ),
            Some(Leftover::Mentions)
        );
        assert_eq!(
            classify_leftover("source ~/.cargo/env\n", &p),
            Some(Leftover::Mentions)
        );
        assert_eq!(
            classify_leftover(". \"$CARGO_HOME/env\"\n", &p),
            Some(Leftover::Mentions)
        );
        // Certain beats qualified within one file.
        assert_eq!(
            classify_leftover("source ~/.cargo/env\n. \"$HOME/.cargo/env\"\n", &p),
            Some(Leftover::Sources)
        );
        // Lines about something else: nothing.
        assert_eq!(
            classify_leftover("export PATH=\"$HOME/.local/bin:$PATH\"\n", &p),
            None
        );
        assert_eq!(
            classify_leftover("export PATH=\"$HOME/.cargo/bin:$PATH\"\n", &p),
            None
        );
        assert_eq!(classify_leftover("", &p), None);

        // A custom Cargo home: the default's env file is not this
        // uninstall's business.
        let p = leftover_patterns(home, Path::new("/Volumes/Data/cargo"));
        assert_eq!(classify_leftover(". \"$HOME/.cargo/env\"\n", &p), None);
        assert_eq!(classify_leftover("source ~/.cargo/env\n", &p), None);
        assert_eq!(
            classify_leftover(". \"/Volumes/Data/cargo/env\"\n", &p),
            Some(Leftover::Sources)
        );
        assert_eq!(
            classify_leftover(
                "[ -f /Volumes/Data/cargo/env ] && . /Volumes/Data/cargo/env\n",
                &p
            ),
            Some(Leftover::Mentions)
        );
    }

    #[test]
    fn test_classify_leftover_is_certain_only_below_lines_that_stand_alone() {
        // Step E's code review: rustup's exact line proves nothing on its
        // own -- inside an `if` it runs only when the test passes, in a
        // function body only when the function is called, in a
        // here-document or a quote never -- so it is "may" unless every
        // line above it stands alone (`stands_alone`). Banager does not
        // track where a block closes, so a closed one above it makes it
        // "may" too.
        let home = Path::new("/Users/someone");
        let p = leftover_patterns(home, Path::new("/Users/someone/.cargo"));
        let line = rc_line();
        for (why, contents) in [
            (
                "a guard",
                format!("if [ -f \"$HOME/.cargo/env\" ]; then\n    {line}\nfi\n"),
            ),
            ("a function body", format!("rust_env() {{\n  {line}\n}}\n")),
            (
                "zsh's function whose body is the next command",
                format!("rust_env()\n{line}\n"),
            ),
            ("a here-document", format!("cat <<'EOF'\n{line}\nEOF\n")),
            (
                "a line joined on by &&",
                format!("[ -d \"$HOME/.cargo\" ] &&\n{line}\n"),
            ),
            (
                "a line joined on by a backslash",
                format!("test -d \"$HOME/.cargo\" \\\n{line}\n"),
            ),
            (
                "a quote left open",
                format!("alias rust_env='\n{line}\n'\n"),
            ),
            (
                "a return above it",
                format!("[[ $- != *i* ]] && return\n{line}\n"),
            ),
            (
                "a closed block above it",
                format!("if [ -n \"$ZSH_VERSION\" ]; then\n  setopt no_beep\nfi\n{line}\n"),
            ),
            (
                "fish's if",
                "if status is-interactive\n    source \"$HOME/.cargo/env.fish\"\nend\n".to_string(),
            ),
        ] {
            assert_eq!(
                classify_leftover(&contents, &p),
                Some(Leftover::Mentions),
                "{why}"
            );
        }
        // Lines that stand alone above it -- this Mac's `~/.zshrc` has its
        // line below ones like these -- leave it certain; a comment counts
        // for nothing, whatever it holds.
        let plain = [
            "alias ll=\"ls -l\"",
            "# >>> an installer's block >>>",
            "export PATH=\"$HOME/.local/bin:$PATH\"",
            "# <<< an installer's block <<<",
            "fpath=(/Users/someone/.docker/completions $fpath)",
            "autoload -Uz compinit",
            "compinit",
            "eval \"$(/opt/homebrew/bin/brew shellenv)\"",
            "[ -s \"$NVM_DIR/nvm.sh\" ] && \\. \"$NVM_DIR/nvm.sh\"  # it's nvm's",
            "export GREETING='it'\\''s here'",
            "",
            &line,
            "",
        ]
        .join("\n");
        assert_eq!(classify_leftover(&plain, &p), Some(Leftover::Sources));
    }

    #[test]
    fn test_stands_alone_accepts_whole_commands_and_refuses_what_it_cannot_follow() {
        for line in [
            ". \"$HOME/.cargo/env\"",
            "export PATH=\"$HOME/.local/bin:$PATH\"",
            "alias ll='ls -l'",
            "fpath=(/Users/someone/.docker/completions $fpath)",
            "eval \"$(/opt/homebrew/bin/brew shellenv)\"",
            "[ -s \"$NVM_DIR/nvm.sh\" ] && \\. \"$NVM_DIR/nvm.sh\"  # This loads nvm",
            // A comment's own quotes and words are not read.
            "export GREETING='it'\\''s here' # don't: if unsure, exit",
            "source <(kubectl completion zsh)",
            "[[ -f ~/.fzf.zsh ]] && source ~/.fzf.zsh",
            "print -P '%F{blue}hi%f'",
            "export NVM_DIR=\"$([ -z \"${XDG_CONFIG_HOME-}\" ] && printf %s \"${HOME}/.nvm\" || printf %s \"${XDG_CONFIG_HOME}/nvm\")\"",
            // `#` inside `${…}`, inside a word, after `(` (zsh's glob
            // flags): not a comment, and read on.
            "echo ${#path} ${PATH##*/} a#b *(#qN)",
            "x=$'a\\'b'",
            "sleep 1 &",
            "set -gx PATH $HOME/.local/bin $PATH",
            "status is-interactive; and fish_vi_key_bindings",
        ] {
            assert!(stands_alone(line), "{line}");
        }
        for line in [
            "if [ -f \"$HOME/.cargo/env\" ]; then",
            "fi",
            "while read line; do",
            "case \":$PATH:\" in",
            // Closed on one line, but Banager does not follow a loop.
            "for f in ~/.zsh/*.zsh; do source \"$f\"; done",
            "rust_env() {",
            "rust_env()",
            "function rust_env {",
            "{",
            "}",
            "cat <<'EOF'",
            "alias rust_env='",
            "echo \"unclosed",
            "x=$(",
            // A comment inside `$(…)` swallows its `)`.
            "echo $(echo hi # a comment)",
            "[ -d \"$HOME/.cargo\" ] &&",
            "ls |",
            "test -d \"$HOME/.cargo\" \\",
            "[[ -f ~/.fzf.zsh",
            "]] && [[ -f ~/.fzf.zsh",
            "[[ $- != *i* ]] && return",
            "exit",
            "exec zsh",
            // Quoted or not, a word is a word.
            "eval 'return 0'",
            "e\"xit\"",
            "set -x FOO 'a\\'b'",
            "x=$'a\\'",
            "test -f ~/.cargo/env.fish; and",
            "if status is-interactive",
            "end",
            // A word counts wherever it stands.
            "zstyle ':completion:*' menu select",
        ] {
            assert!(!stands_alone(line), "{line}");
        }
    }

    #[test]
    fn test_shell_config_leftovers_reports_the_file_rustup_does_not_edit() {
        // This Mac (spec §6.4, re-checked 2026-09-25): `~/.zshenv:1` and
        // `~/.profile:1` hold rustup's line, `~/.zshrc:17` holds the same
        // line but rustup never edits `.zshrc` -- after the uninstall
        // every new interactive zsh, which reads `.zshrc`, prints `no such
        // file or directory: …/.cargo/env`.
        let home = TempHome::new("rustup-rc-zshrc");
        for rc in [".zshenv", ".profile", ".zshrc"] {
            std::fs::write(home.path().join(rc), format!("{}\n", rc_line())).expect("write rc");
        }
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn test_shell_config_leftovers_never_reads_a_startup_file_kept_in_a_protected_place() {
        // `~/.zshrc` a link to dotfiles kept in `~/Documents` or iCloud
        // Drive: not read there, and said as a file Banager could not read
        // -- rustup, which Banager runs, may still read and edit it. Kept
        // anywhere else, it is read and said as it is.
        for (keep, protected_place) in [
            ("dotfiles", false),
            ("Documents/dotfiles", true),
            (
                "Library/Mobile Documents/com~apple~CloudDocs/dotfiles",
                true,
            ),
        ] {
            let home = TempHome::new("rustup-rc-kept");
            let kept = home.dir(keep).join("zshrc");
            std::fs::write(&kept, format!("{}\n", rc_line())).expect("write rc");
            home.link(".zshrc", &kept);
            let (warnings, made) = crate::dirfd::calls::measure(|| {
                shell_config_leftovers(home.path(), None, &home.path().join(".cargo"))
            });
            let protected = home.protected();
            for (call, path) in &made.paths {
                assert!(!protected.contains(path), "{keep}: {call:?} {path:?}");
            }
            // Not read there, and not taken as holding nothing: said as
            // a file Banager could not read.
            let expected = if protected_place {
                vec![Warning::ShellConfigUnread {
                    path: "~/.zshrc".to_string(),
                }]
            } else {
                vec![Warning::LeavesShellConfigLine {
                    path: "~/.zshrc".to_string(),
                    certain: true,
                }]
            };
            assert_eq!(warnings, expected, "{keep}");
        }
    }

    #[test]
    fn test_shell_config_leftovers_qualifies_rustups_line_inside_a_guard() {
        // Step E's code review: a `.zshrc` whose line sits inside an
        // `if [ -f … ]; then … fi`. rustup leaves `.zshrc` alone, and the
        // guard skips the missing file, so "will print an error" would be
        // wrong: "may".
        let home = TempHome::new("rustup-rc-guarded");
        std::fs::write(
            home.path().join(".zshrc"),
            format!(
                "if [ -f \"$HOME/.cargo/env\" ]; then\n    {}\nfi\n",
                rc_line()
            ),
        )
        .expect("write");
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: false
            }]
        );
    }

    #[test]
    fn test_shell_config_leftovers_is_silent_when_only_rustups_own_lines_exist() {
        let home = TempHome::new("rustup-rc-clean");
        std::fs::write(home.path().join(".zshenv"), format!("{}\n", rc_line())).expect("write");
        std::fs::write(
            home.path().join(".zprofile"),
            "source \"$HOME/.cargo/env\"\nexport PATH=\"$HOME/.cargo/bin:$PATH\"\n",
        )
        .expect("write the two pre-1.23 lines, which rustup also removes");
        std::fs::write(
            home.path().join(".bash_profile"),
            format!("{}\n", rc_line()),
        )
        .expect("write");
        assert!(shell_config_leftovers(home.path(), None, &home.path().join(".cargo")).is_empty());
        // A comment about the env file is not a line that loads it.
        std::fs::write(
            home.path().join(".zshrc"),
            "# added by rustup: . \"$HOME/.cargo/env\"\n",
        )
        .expect("write");
        assert!(shell_config_leftovers(home.path(), None, &home.path().join(".cargo")).is_empty());
        // And with no startup files at all.
        let home = TempHome::new("rustup-rc-none");
        assert!(shell_config_leftovers(home.path(), None, &home.path().join(".cargo")).is_empty());
    }

    #[test]
    fn test_shell_config_leftovers_qualifies_a_mention_rustup_will_not_remove() {
        // A hand-written spelling beside rustup's line in a file it edits:
        // rustup removes its own, the other stays, and whether it errors
        // depends on what it is -- so "may", not "will" (ruling 22). fish's
        // own config with a `source` of `env.fish`: rustup's fish file is
        // `conf.d/rustup.fish`, never `config.fish`, and that form is one
        // rustup writes, so "will".
        let home = TempHome::new("rustup-rc-handwritten");
        std::fs::write(
            home.path().join(".zshenv"),
            format!("{}\nsource ~/.cargo/env\n", rc_line()),
        )
        .expect("write");
        std::fs::write(
            home.path().join(".bashrc"),
            "[ -f \"$HOME/.cargo/env\" ] && . \"$HOME/.cargo/env\"\n",
        )
        .expect("write");
        home.file(".config/fish/config.fish");
        std::fs::write(
            home.path().join(".config/fish/config.fish"),
            "source \"$HOME/.cargo/env.fish\"\n",
        )
        .expect("write");
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![
                Warning::LeavesShellConfigLine {
                    path: "~/.zshenv".to_string(),
                    certain: false
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.bashrc".to_string(),
                    certain: false
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.config/fish/config.fish".to_string(),
                    certain: true
                },
            ]
        );
    }

    #[test]
    fn test_shell_config_leftovers_removes_two_copies_when_zdotdir_is_home_and_one_otherwise() {
        // Review Focus #2. Two copies of rustup's line in `~/.zshenv`:
        // with `ZDOTDIR=$HOME` rustup visits the file twice and both go;
        // with no ZDOTDIR one stays, and it is rustup's own form, so it
        // *will* error. The same for the legacy `source` line in
        // `~/.zprofile`, which `legacy_paths` visits under `$ZDOTDIR` and
        // under `~`. Its line last with no newline stays either way.
        let home = TempHome::new("rustup-rc-zdotdir");
        let two = format!("{}\n{}\n", rc_line(), rc_line());
        std::fs::write(home.path().join(".zshenv"), &two).expect("write");
        std::fs::write(
            home.path().join(".zprofile"),
            "source \"$HOME/.cargo/env\"\nsource \"$HOME/.cargo/env\"\n",
        )
        .expect("write");
        assert!(shell_config_leftovers(
            home.path(),
            Some(home.path()),
            &home.path().join(".cargo")
        )
        .is_empty());
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![
                Warning::LeavesShellConfigLine {
                    path: "~/.zshenv".to_string(),
                    certain: true
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.zprofile".to_string(),
                    certain: true
                },
            ]
        );
        std::fs::write(
            home.path().join(".zshenv"),
            format!("export A=1\n{}", rc_line()),
        )
        .expect("write");
        std::fs::remove_file(home.path().join(".zprofile")).expect("remove");
        assert_eq!(
            shell_config_leftovers(home.path(), Some(home.path()), &home.path().join(".cargo")),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshenv".to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn test_startup_files_put_zshs_three_under_another_zdotdir_each_before_its_home_namesake() {
        // The eight under the home alone when there is no ZDOTDIR, when it
        // is the home (spelled with or without a trailing slash: its files
        // are then those files, read once and visited twice), when it is
        // empty (no ZDOTDIR, shell.rs:213), and when it is relative (rustup
        // would resolve it against its own working directory, which this
        // preview does not know). Another absolute folder adds its
        // `.zshenv`, `.zprofile` and `.zshrc`, each just before the home's,
        // in the order rustup visits them.
        let home = Path::new("/Users/someone");
        let eight: Vec<PathBuf> = SHELL_RC_CANDIDATES.iter().map(|rc| home.join(rc)).collect();
        assert_eq!(startup_files(home, None), eight);
        assert_eq!(startup_files(home, Some(home)), eight);
        assert_eq!(
            startup_files(home, Some(Path::new("/Users/someone/"))),
            eight
        );
        assert_eq!(startup_files(home, Some(Path::new(""))), eight);
        assert_eq!(startup_files(home, Some(Path::new(".config/zsh"))), eight);
        let zdotdir = Path::new("/Users/someone/.config/zsh");
        assert_eq!(
            startup_files(home, Some(zdotdir)),
            vec![
                zdotdir.join(".zshenv"),
                home.join(".zshenv"),
                zdotdir.join(".zprofile"),
                home.join(".zprofile"),
                zdotdir.join(".zshrc"),
                home.join(".zshrc"),
                home.join(".bash_profile"),
                home.join(".bash_login"),
                home.join(".bashrc"),
                home.join(".profile"),
                home.join(".config/fish/config.fish"),
            ]
        );
    }

    #[test]
    fn test_shell_config_leftovers_reads_zshs_files_under_another_zdotdir() {
        // A zsh whose files live under `ZDOTDIR=$HOME/.config/zsh`: rustup
        // visits `$ZDOTDIR/.zshenv` and `$ZDOTDIR/.zprofile` (and `~/.zshenv`
        // once, not twice), zsh reads that folder's `.zshenv`, `.zprofile`
        // and `.zshrc`, and rustup never edits a `.zshrc` -- so each of the
        // three, a file of its own here, is read, replayed on, and named by
        // its own path just before its home namesake. The `source` line rustup
        // leaves in `$ZDOTDIR/.zshenv` (it removes that spelling only from
        // the `legacy_paths` files) *will* error whenever zsh reads that
        // file: the line the preview must not stay silent about.
        let home = TempHome::new("rustup-rc-zdotdir-elsewhere");
        let zdotdir = home.dir(".config/zsh");
        std::fs::write(
            zdotdir.join(".zshenv"),
            format!("{}\nsource \"$HOME/.cargo/env\"\n", rc_line()),
        )
        .expect("write");
        std::fs::write(
            zdotdir.join(".zprofile"),
            "export PATH=\"$HOME/.cargo/bin:$PATH\"\nsource \"$HOME/.cargo/env\"\n[ -f \"$HOME/.cargo/env\" ] && . \"$HOME/.cargo/env\"\n",
        )
        .expect("write");
        std::fs::write(zdotdir.join(".zshrc"), format!("{}\n", rc_line())).expect("write");
        std::fs::write(
            home.path().join(".zshenv"),
            format!("{}\n{}\n", rc_line(), rc_line()),
        )
        .expect("write");
        assert_eq!(
            shell_config_leftovers(home.path(), Some(&zdotdir), &home.path().join(".cargo")),
            vec![
                Warning::LeavesShellConfigLine {
                    path: "~/.config/zsh/.zshenv".to_string(),
                    certain: true
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.zshenv".to_string(),
                    certain: true
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.config/zsh/.zprofile".to_string(),
                    certain: false
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.config/zsh/.zshrc".to_string(),
                    certain: true
                },
            ]
        );
        // A ZDOTDIR outside the home is named by its full path, and the
        // folder that is no longer ZDOTDIR is no longer read.
        let outside = TempHome::new("rustup-rc-zdotdir-outside");
        std::fs::write(outside.path().join(".zshrc"), format!("{}\n", rc_line())).expect("write");
        std::fs::remove_file(home.path().join(".zshenv")).expect("remove");
        assert_eq!(
            shell_config_leftovers(
                home.path(),
                Some(outside.path()),
                &home.path().join(".cargo")
            ),
            vec![Warning::LeavesShellConfigLine {
                path: outside.path().join(".zshrc").display().to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn test_shell_config_leftovers_sees_a_line_removed_through_one_name_gone_under_the_others() {
        // Step E's code review: rustup reads and rewrites a startup file
        // through whichever name it visits -- `is_file()` and
        // `read_to_string` follow links, and `raw::write_file`
        // (src/utils/raw.rs:86-98 at 1.29.1) opens that name to truncate
        // and write -- so the line it removes through `.zshenv` is gone
        // from a `.zshrc` that is the same file, although rustup never
        // visits `.zshrc`.
        let one_line = format!("{}\n", rc_line());
        let home = TempHome::new("rustup-rc-alias-link");
        std::fs::write(home.path().join(".zshenv"), &one_line).expect("write");
        home.link(".zshrc", Path::new(".zshenv"));
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            Vec::new(),
            "a link"
        );
        let home = TempHome::new("rustup-rc-alias-hard");
        std::fs::write(home.path().join(".zshenv"), &one_line).expect("write");
        std::fs::hard_link(home.path().join(".zshenv"), home.path().join(".zshrc"))
            .expect("hard link");
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            Vec::new(),
            "a hard link"
        );
        // What is left is left under every name, each named by its own
        // path in the order the names are read.
        let home = TempHome::new("rustup-rc-alias-left");
        std::fs::write(
            home.path().join(".zshenv"),
            format!("{}\n{}\n", rc_line(), rc_line()),
        )
        .expect("write");
        home.link(".zshrc", Path::new(".zshenv"));
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![
                Warning::LeavesShellConfigLine {
                    path: "~/.zshenv".to_string(),
                    certain: true
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.zshrc".to_string(),
                    certain: true
                },
            ]
        );
    }

    #[test]
    fn test_shell_config_leftovers_counts_visits_through_two_names_of_one_file_against_one_copy() {
        // Each visit removes one copy from the file its name leads to, so
        // two names of one file visited once each remove two copies, as
        // `ZDOTDIR=$HOME` does with one name visited twice.
        let two_lines = format!("{}\n{}\n", rc_line(), rc_line());
        // `.bash_profile` a link to `.profile`: rustup visits `.profile`,
        // then `.bash_profile`.
        let home = TempHome::new("rustup-rc-alias-two-visits");
        std::fs::write(home.path().join(".profile"), &two_lines).expect("write");
        home.link(".bash_profile", Path::new(".profile"));
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            Vec::new(),
            ".bash_profile and .profile"
        );
        // A `ZDOTDIR` that is a link to the home: `$ZDOTDIR/.zshenv` and
        // `~/.zshenv`, read under two paths, are one file.
        let home = TempHome::new("rustup-rc-alias-zdotdir");
        std::fs::write(home.path().join(".zshenv"), &two_lines).expect("write");
        let zdotdir = home.link("zdot", home.path());
        assert_eq!(
            shell_config_leftovers(home.path(), Some(&zdotdir), &home.path().join(".cargo")),
            Vec::new(),
            "a ZDOTDIR linked to the home"
        );
    }

    #[test]
    fn test_shell_config_leftovers_follows_a_custom_cargo_home() {
        // With CARGO_HOME set, rustup wrote and looks for the absolute
        // path; a line spelled through $HOME/.cargo from an earlier
        // default install is about a file this uninstall does not touch,
        // so it is not reported at all (Astra's counterexample). The
        // gate keeps a custom home from ever reaching a preview; the
        // function is right on its own regardless.
        let home = TempHome::new("rustup-rc-custom");
        let cargo_home = home.dir("elsewhere/cargo");
        let line = format!(". \"{}/env\"\n", cargo_home.display());
        std::fs::write(home.path().join(".zshenv"), &line).expect("write");
        assert!(shell_config_leftovers(home.path(), None, &cargo_home).is_empty());
        std::fs::write(home.path().join(".profile"), format!("{}\n", rc_line())).expect("write");
        assert!(shell_config_leftovers(home.path(), None, &cargo_home).is_empty());
        std::fs::write(home.path().join(".zshrc"), &line).expect("write");
        assert_eq!(
            shell_config_leftovers(home.path(), None, &cargo_home),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn regression_preview_gives_the_programs_no_cargo_record_lists_their_own_line() {
        // uv's installer put `uv` and `uvx` in ~/.cargo/bin before uv
        // 0.5.0, and no Cargo record lists them: rustup deletes them too,
        // but reinstalling Rust will not bring them back, so they are not
        // on the line that says cargo install can.
        let home = TempHome::new("rustup-warnings-unrecorded");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        for program in ["hexyl", "uv", "uvx"] {
            std::fs::write(cargo_home.join("bin").join(program), b"x").expect("write a program");
        }
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy");
        let d = detected(home.path(), &cargo_home);
        assert_eq!(
            preview_with(&d, &[]),
            Ok(vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: Vec::new()
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::RemovesCargoInstalled {
                    names: vec!["hexyl".to_string()]
                },
                Warning::RemovesUnrecordedPrograms {
                    names: vec!["uv".to_string(), "uvx".to_string()]
                },
                Warning::EditsShellConfig,
            ])
        );
    }

    #[test]
    fn test_warnings_come_in_the_dialogs_order_with_the_conditional_ones_only_when_true() {
        // Spec §6.6's rustup dialog, on this Mac's layout: toolchains (with
        // the rustup home's path), the Cargo home, hexyl, the shell edit,
        // ~/.zshrc -- and, when Homebrew's Cellar has a rustup, its line
        // before the shell edit.
        let home = TempHome::new("rustup-warnings-full");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/hexyl"), b"x").expect("write hexyl");
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy");
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        std::fs::write(home.path().join(".zshrc"), format!("{}\n", rc_line())).expect("write");
        let d = detected(home.path(), &cargo_home);
        let expected = vec![
            Warning::RemovesToolchains {
                path: "~/.rustup".to_string(),
                names: vec!["stable-aarch64-apple-darwin".to_string()],
            },
            Warning::DeletesCargoHome {
                path: "~/.cargo".to_string(),
            },
            Warning::RemovesCargoInstalled {
                names: vec!["hexyl".to_string()],
            },
            Warning::EditsShellConfig,
            Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true,
            },
        ];
        assert_eq!(preview_with(&d, &[]), Ok(expected.clone()));
        let brew = home.dir("opt/homebrew");
        home.dir("opt/homebrew/Cellar/rustup/1.29.1");
        let mut with_brew = expected;
        with_brew.insert(3, Warning::HomebrewRustupLosesToolchains);
        assert_eq!(preview_with(&d, &[brew]), Ok(with_brew));

        // No toolchains directory, nothing cargo-installed, no startup
        // files: the three unconditional lines, the toolchain one without
        // names.
        let home = TempHome::new("rustup-warnings-bare");
        let cargo_home = home.dir(".cargo");
        let d = detected(home.path(), &cargo_home);
        assert_eq!(
            preview_with(&d, &[]),
            Ok(vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: Vec::new()
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::EditsShellConfig,
            ])
        );
        // `uninstall_preview` is the same function over the real
        // prefixes, each under the seat's `machine_root` -- `/` in the
        // app, the test's own folder here, so this Mac's Cellar is never
        // read: with no `Cellar/rustup` under either, no Homebrew line;
        // with one under the second (an Intel Mac's `/usr/local`), the
        // line. It is exercised through `plan` in Task 6.
        assert_eq!(uninstall_preview(&d), preview_with(&d, &[]));
        home.dir("machine-root/usr/local/Cellar/rustup/1.29.1");
        let both = [
            home.path().join("machine-root/opt/homebrew"),
            home.path().join("machine-root/usr/local"),
        ];
        let with_brew = preview_with(&d, &both);
        assert_eq!(uninstall_preview(&d), with_brew);
        assert!(with_brew
            .unwrap()
            .contains(&Warning::HomebrewRustupLosesToolchains));
    }

    #[test]
    fn test_preview_refuses_a_layout_the_gate_refuses_with_the_gates_refusal() {
        // The one answer `plan` takes for both the gate and the warnings:
        // for a layout it will not describe, the refusal `uninstall_blocked`
        // gives over the same seat and disk -- never `Ok` with nothing in
        // it.
        let home = TempHome::new("rustup-preview-refused");
        let custom = home.dir("elsewhere/cargo");
        let d = detected(home.path(), &custom);
        assert_eq!(
            preview_with(&d, &[]),
            Err(GateRefusal {
                reason: UninstallBlocked::NoSafeMethod,
                path: home.path().join(".cargo"),
            })
        );
        assert_eq!(preview_with(&d, &[]).err(), uninstall_blocked(&d));
    }

    #[test]
    fn test_extra_locks_is_the_cargo_instances_lock_spelled_by_its_one_producer() {
        let d = detected(
            Path::new("/Users/someone"),
            Path::new("/Users/someone/.cargo"),
        );
        assert_eq!(
            extra_locks(&d),
            vec![ResourceLock(crate::adapters::cargo::instance_id_for(
                Path::new("/Users/someone/.cargo")
            ))]
        );
        assert_eq!(
            extra_locks(&d),
            vec![ResourceLock("cargo:/Users/someone/.cargo".to_string())]
        );
        // No usable Cargo home (a relative CARGO_HOME): no cargo instance
        // exists to lock -- and no rustup instance either, since its
        // launcher is under that home; the function still answers.
        let d = Detected {
            cargo_home: None,
            ..d
        };
        assert!(extra_locks(&d).is_empty());
    }
}
