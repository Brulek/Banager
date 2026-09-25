//! What `rustup self uninstall` does, when Canager may offer it, and what
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
//! Canager does not pass -- the line it added to the shell startup files
//! (:971-973, `do_remove_from_path`), then everything in `$CARGO_HOME`
//! except `bin/` (:977-993), then everything in `bin/` that is not one of
//! its own proxies or `rustup` itself (:996-1022), and finally the whole
//! `$CARGO_HOME` directory (`delete_rustup_and_cargo_home`, :1029;
//! unix.rs:50-53). Both homes come from `RUSTUP_HOME`/`CARGO_HOME` or
//! default under `HOME` (env.rs:67-79, :101-113) -- wherever they point,
//! and permanently: nothing here goes to the Trash. So Canager offers the
//! command only for the standard layout (`standard_roots`, ruling 18) and
//! the preview names both folders by path.
//!
//! Nothing here runs a command or writes a file: `plan` hands in what
//! `detect` seated, and this module lists `<rustup_home>/toolchains` and
//! `<cargo_home>/bin` by name, reads `.crates2.json`, looks for
//! Homebrew's `Cellar/rustup`, reads eight startup files under the home,
//! replays rustup's own cleanup on copies of them in memory, and answers
//! with `Warning`s.

use super::Detected;
use crate::adapters::cargo::{instance_id_for, parse_crates2_bins};
use crate::model::{ResourceLock, UninstallBlocked, Warning};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The startup files Canager reads (never writes) for a line about the
/// Cargo env file, home-relative, in the order they are reported (spec
/// §6.4). `$ZDOTDIR/.zshenv` and `$ZDOTDIR/.zprofile` are read only when
/// `ZDOTDIR` is the home (then they are these files); a zsh whose files
/// live elsewhere is not checked, and the trust file says so.
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
/// is not looked for. Read by `uninstall_warnings`.
pub const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// The two folders `rustup self uninstall` deletes, when they are the
/// standard ones (`standard_roots`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardRoots {
    pub cargo_home: PathBuf,
    pub rustup_home: PathBuf,
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_dir())
        .unwrap_or(false)
}

/// The gate (ruling 18): `Some` only when the Rust this instance belongs
/// to lives where a fresh `rustup-init` puts it. Both homes as rustup
/// computes them (`home` 0.5.12 over `RUSTUP_HOME`/`CARGO_HOME`/`HOME`,
/// seated by `detect`; `None` is a relative value, unsupported); each
/// exactly `<home>/.cargo` and `<home>/.rustup` -- compared lexically,
/// as rustup itself compares when it decides how to spell the Cargo home
/// in the shell line (`cargo_home_str_with_home`, shell.rs:43-58), over
/// the same `HOME`; the Cargo home a directory that is not a link; the
/// rustup home a directory that is not a link, or not there yet (rustup
/// creates it on its first run). Anything else -- a custom home, a link
/// to somewhere else, a relative variable -- and `uninstall()` would
/// `remove_dir` a place this preview did not name: not offered.
pub fn standard_roots(d: &Detected) -> Option<StandardRoots> {
    let cargo_home = d.cargo_home.as_deref()?;
    let rustup_home = d.rustup_home.as_deref()?;
    if !d.home.is_absolute()
        || cargo_home != d.home.join(".cargo")
        || rustup_home != d.home.join(".rustup")
    {
        return None;
    }
    if !is_real_dir(cargo_home) {
        return None;
    }
    match std::fs::symlink_metadata(rustup_home) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return None,
    }
    Some(StandardRoots {
        cargo_home: cargo_home.to_path_buf(),
        rustup_home: rustup_home.to_path_buf(),
    })
}

/// `CommandUninstall.blocked` of the `RUSTUP` recipe: `NoSafeMethod` for
/// any layout but the standard one. The variant is the one the gate
/// (`session/plans.rs`) and the Installed page already refuse and hide
/// the button for; rustup's row says why in its own sentence
/// (`installed.blocked.NoSafeMethod.standalone-rustup`, src/lib/sources.ts).
pub fn uninstall_blocked(d: &Detected) -> Option<UninstallBlocked> {
    standard_roots(d)
        .is_none()
        .then_some(UninstallBlocked::NoSafeMethod)
}

/// Every installed toolchain, by name: the entries of
/// `<rustup_home>/toolchains`, sorted, hidden names skipped. That
/// directory is what `uninstall()` removes toolchain by toolchain
/// (`cfg.list_toolchains()`, self_update.rs:955-958) before it deletes
/// the home whole, so its names are the toolchains that go; a linked
/// toolchain (`rustup toolchain link`) is an entry like any other. No
/// directory, or an unreadable one: no names, and the dialog says "every
/// toolchain" (ruling 15). Nothing is run.
pub fn toolchain_names(rustup_home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(rustup_home.join("toolchains"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The programs in `<cargo_home>/bin` that rustup 1.29.1's `self
/// uninstall` deletes, sorted and named once: every entry whose name is
/// not `rustup` or one of `RUSTUP_PROXIES` (self_update.rs:996-1022
/// compares names only, so a program copied there by hand goes too),
/// read with `read_dir` -- nothing is opened or run -- united with the
/// binaries `.crates2.json` records (every crate's `bins`, through
/// `parse_crates2_bins`, the parser cargo's own inventory uses). The
/// listing is what rustup acts on; the record still names what cargo
/// installed when the directory cannot be listed, and a record entry
/// whose file is already gone is named although nothing is left to
/// delete -- the safe direction. Not named: an entry starting with `.`
/// (`.DS_Store`: deleted with the folder, but no program), and a name
/// that is not UTF-8 -- deleted with the folder too (`remove_dir`,
/// :1029, takes everything), but not spellable in a sentence. No
/// directory, an unreadable one, no record or a broken one each add
/// nothing: a name is better missing than invented, and
/// `DeletesCargoHome` always says the whole folder goes.
pub fn bin_programs_rustup_removes(cargo_home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(cargo_home.join("bin"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if let Ok(json) = std::fs::read_to_string(cargo_home.join(".crates2.json")) {
        names.extend(
            parse_crates2_bins(&json)
                .unwrap_or_default()
                .into_iter()
                .flat_map(|(_, bins)| bins),
        );
    }
    names.retain(|name| {
        !name.starts_with('.') && name != "rustup" && !RUSTUP_PROXIES.contains(&name.as_str())
    });
    names.sort();
    names.dedup();
    names
}

/// Whether Homebrew's `rustup` formula is installed: `<prefix>/Cellar/rustup`
/// exists under one of `prefixes` (`HOMEBREW_PREFIXES` in production).
/// rustup's homes depend only on `RUSTUP_HOME`/`CARGO_HOME`/`HOME`, never
/// on where the binary sits (`home::rustup_home_with_cwd_env`,
/// env.rs:101-113), so that rustup shares `~/.rustup` with the native
/// one and loses its toolchains when it goes (ruling 21). Read-only.
pub fn homebrew_rustup_present(prefixes: &[PathBuf]) -> bool {
    prefixes
        .iter()
        .any(|prefix| prefix.join("Cellar/rustup").is_dir())
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
/// and Xonsh visit files Canager does not read. Then
/// `remove_legacy_paths` (unix.rs:174-194) removes the pre-1.23 line
/// `export PATH="<S>/bin:$PATH"` and then `source "<S>/env"`, each from
/// `legacy_paths` (shell.rs:564-574): `~/.bash_profile`, `~/.profile`,
/// `$ZDOTDIR/.zprofile` when there is a ZDOTDIR, `~/.zprofile`. Bash's
/// and Zsh's availability checks are folded in: a Bash file that is not
/// there is a no-op visit, and on a Mac zsh is at `/bin/zsh`. `zdotdir`
/// is `HostEnv.zdotdir` -- rustup itself asks `zsh -c 'echo -n $ZDOTDIR'`
/// when `SHELL` is not zsh (shell.rs:207-225), which Canager does not
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
/// `sourcing`: whole lines (trimmed) that *will* fail once the file is
/// gone -- the sourcing forms rustup itself writes (`. "<X>/env"`,
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
/// cleanup: `Sources` -- a line rustup's own form spells, which *will*
/// print an error in every new terminal; `Mentions` -- some other
/// non-comment line naming the file, which *may*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leftover {
    Sources,
    Mentions,
}

/// The tier of a file's remaining contents, or `None` for a file that
/// says nothing about the env file outside comments. A comment is a
/// trimmed line starting with `#`; the certain tier wins over the
/// qualified one within a file.
pub fn classify_leftover(contents: &str, patterns: &LeftoverPatterns) -> Option<Leftover> {
    let mut mentions = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if patterns.sourcing.iter().any(|form| form == line) {
            return Some(Leftover::Sources);
        }
        if patterns
            .needles
            .iter()
            .any(|needle| line.contains(needle.as_str()))
        {
            mentions = true;
        }
    }
    mentions.then_some(Leftover::Mentions)
}

/// One `LeavesShellConfigLine` per startup file under `home` that will
/// still speak of the Cargo env file after `rustup self uninstall`, in
/// `SHELL_RC_CANDIDATES` order, `path` spelled `~/<file>`, `certain` by
/// tier. The eight files are read once (read-only: Canager never edits
/// a startup file, spec §6.8), rustup's visits (`rustup_rc_visits`) are
/// replayed on the copies -- so a second visit to the same file sees
/// the first's result, as rustup's does -- and what is left is
/// classified (`classify_leftover`). A visit to a file outside the eight
/// (a `$ZDOTDIR` that is not the home) has no copy to act on.
pub fn shell_config_leftovers(
    home: &Path,
    zdotdir: Option<&Path>,
    cargo_home: &Path,
) -> Vec<Warning> {
    let spelled = cargo_home_str(home, cargo_home);
    let mut files: BTreeMap<PathBuf, String> = SHELL_RC_CANDIDATES
        .iter()
        .filter_map(|rc| {
            let path = home.join(rc);
            std::fs::read_to_string(&path)
                .ok()
                .map(|contents| (path, contents))
        })
        .collect();
    for visit in rustup_rc_visits(home, zdotdir, &spelled) {
        if let Some(contents) = files.get_mut(&visit.file) {
            remove_first_exact_line(contents, &visit.line);
        }
    }
    let patterns = leftover_patterns(home, cargo_home);
    SHELL_RC_CANDIDATES
        .iter()
        .filter_map(|rc| {
            let contents = files.get(&home.join(rc))?;
            let tier = classify_leftover(contents, &patterns)?;
            Some(Warning::LeavesShellConfigLine {
                path: format!("~/{rc}"),
                certain: tier == Leftover::Sources,
            })
        })
        .collect()
}

/// The uninstall preview's list for `rustup self uninstall -y`, in the
/// order the dialog shows it (spec §6.6), over a caller-given list of
/// Homebrew prefixes so the tests are hermetic: the rustup home by path
/// with every toolchain in it, the Cargo home by path, the programs in
/// its `bin/` when there are any (rulings 1 and 16: 1.29.1 removes the
/// whole Cargo home), the Homebrew line when its Cellar has a rustup
/// (ruling 21), the shell edit, and each startup file left speaking of
/// Cargo's env file. Empty for a layout the gate refuses (`plan`
/// refuses first). Paths are spelled with `~` by the crate's one rule
/// (`scan::display_path`).
pub fn warnings_with(d: &Detected, homebrew_prefixes: &[PathBuf]) -> Vec<Warning> {
    let Some(roots) = standard_roots(d) else {
        return Vec::new();
    };
    let tilde = |path: &Path| {
        crate::scan::display_path(path, &d.home)
            .display()
            .to_string()
    };
    let mut warnings = vec![
        Warning::RemovesToolchains {
            path: tilde(&roots.rustup_home),
            names: toolchain_names(&roots.rustup_home),
        },
        Warning::DeletesCargoHome {
            path: tilde(&roots.cargo_home),
        },
    ];
    let bins = bin_programs_rustup_removes(&roots.cargo_home);
    if !bins.is_empty() {
        warnings.push(Warning::RemovesCargoInstalled { names: bins });
    }
    if homebrew_rustup_present(homebrew_prefixes) {
        warnings.push(Warning::HomebrewRustupLosesToolchains);
    }
    warnings.push(Warning::EditsShellConfig);
    warnings.extend(shell_config_leftovers(
        &d.home,
        d.zdotdir.as_deref(),
        &roots.cargo_home,
    ));
    warnings
}

/// `CommandUninstall.warnings` of the `RUSTUP` recipe: `warnings_with`
/// over Homebrew's real prefixes.
pub fn uninstall_warnings(d: &Detected) -> Vec<Warning> {
    let prefixes: Vec<PathBuf> = HOMEBREW_PREFIXES.iter().map(PathBuf::from).collect();
    warnings_with(d, &prefixes)
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
    use super::super::testing::{detected, rustup_layout, TempHome};
    use super::*;
    use crate::model::UninstallBlocked;
    use std::path::PathBuf;

    fn rc_line() -> String {
        ". \"$HOME/.cargo/env\"".to_string()
    }

    #[test]
    fn test_standard_roots_accepts_only_the_default_layout_of_real_directories() {
        // Ruling 18: both roots as rustup computes them (`home` 0.5.12),
        // both exactly `<home>/.cargo` and `<home>/.rustup`, the Cargo
        // home a real directory, the rustup home a real directory or not
        // there yet. Anything else is a layout Canager will not offer to
        // delete: rustup's `uninstall()` removes `$RUSTUP_HOME` and
        // `$CARGO_HOME` whole (self_update.rs:960-966, :1029), wherever
        // they point.
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
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_some());

        // Custom, absolute: not offered.
        let home = TempHome::new("roots-custom-cargo");
        let custom = home.dir("elsewhere/cargo");
        assert!(standard_roots(&detected(home.path(), &custom)).is_none());
        let home = TempHome::new("roots-custom-rustup");
        let cargo_home = home.dir(".cargo");
        let d = Detected {
            rustup_home: Some(home.dir("elsewhere/rustup")),
            ..detected(home.path(), &cargo_home)
        };
        assert!(standard_roots(&d).is_none());

        // Relative (unsupported, seated as `None`): not offered.
        let home = TempHome::new("roots-relative");
        let cargo_home = home.dir(".cargo");
        let d = Detected {
            cargo_home: None,
            ..detected(home.path(), &cargo_home)
        };
        assert!(standard_roots(&d).is_none());
        let d = Detected {
            rustup_home: None,
            ..detected(home.path(), &cargo_home)
        };
        assert!(standard_roots(&d).is_none());

        // A root that is a link: the path Canager would list is not the
        // directory that would go.
        let home = TempHome::new("roots-linked-cargo");
        let elsewhere = home.dir("Volumes/Data/cargo");
        home.link(".cargo", &elsewhere);
        assert!(standard_roots(&detected(home.path(), &home.path().join(".cargo"))).is_none());
        let home = TempHome::new("roots-linked-rustup");
        let cargo_home = home.dir(".cargo");
        let elsewhere = home.dir("Volumes/Data/rustup");
        home.link(".rustup", &elsewhere);
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_none());

        // No `~/.cargo` at all: nothing to offer.
        let home = TempHome::new("roots-no-cargo-home");
        assert!(standard_roots(&detected(home.path(), &home.path().join(".cargo"))).is_none());
    }

    #[test]
    fn test_uninstall_blocked_is_no_safe_method_for_anything_but_the_standard_layout() {
        let home = TempHome::new("blocked");
        let cargo_home = home.dir(".cargo");
        assert_eq!(uninstall_blocked(&detected(home.path(), &cargo_home)), None);
        let custom = home.dir("elsewhere/cargo");
        assert_eq!(
            uninstall_blocked(&detected(home.path(), &custom)),
            Some(UninstallBlocked::NoSafeMethod)
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
            toolchain_names(&home.path().join(".rustup")),
            Vec::<String>::new()
        );
        home.dir(".rustup");
        assert_eq!(
            toolchain_names(&home.path().join(".rustup")),
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
            toolchain_names(&rustup_home),
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
            bin_programs_rustup_removes(&cargo_home),
            vec!["hexyl".to_string()]
        );
    }

    #[test]
    fn test_bin_programs_rustup_removes_names_a_program_no_record_lists() {
        // rustup 1.29.1 deletes every entry of `bin/` whose *name* is not
        // one of its fourteen (self_update.rs:996-1022): a program copied
        // there by hand goes too, recorded or not, and with no record or
        // a broken one the listing still names it. `.DS_Store` is deleted
        // with the folder but is no program to name.
        let home = TempHome::new("rustup-bins-unrecorded");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/mytool"), b"x").expect("write mytool");
        std::fs::write(cargo_home.join("bin/.DS_Store"), b"x").expect("write .DS_Store");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home),
            vec!["mytool".to_string()]
        );
        std::fs::write(cargo_home.join(".crates2.json"), "{ not json").expect("write");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home),
            vec!["mytool".to_string()]
        );
    }

    #[test]
    fn test_bin_programs_rustup_removes_flattens_sorts_and_dedups_the_records_binaries() {
        // A crate's binaries by their file names (`rg`, not `ripgrep`),
        // several per crate, united with the listing and each named once.
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
            bin_programs_rustup_removes(&cargo_home),
            vec![
                "cargo-binstall".to_string(),
                "detect-targets".to_string(),
                "hexyl".to_string(),
                "rg".to_string(),
            ]
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
            bin_programs_rustup_removes(&cargo_home),
            Vec::<String>::new()
        );
        std::fs::write(cargo_home.join(".crates2.json"), "{ not json").expect("write");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home),
            Vec::<String>::new()
        );
        // rustup and its proxies alone: nothing else to name.
        rustup_layout(&cargo_home);
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home),
            Vec::<String>::new()
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
        assert!(!homebrew_rustup_present(std::slice::from_ref(&prefix)));
        home.dir("opt/homebrew/Cellar/rustup/1.29.1/bin");
        assert!(homebrew_rustup_present(std::slice::from_ref(&prefix)));
        assert!(homebrew_rustup_present(&[
            home.path().join("usr/local"),
            prefix
        ]));
        assert!(!homebrew_rustup_present(&[]));
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
        // Xonsh edit files Canager does not read, so they have no visit
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
        // and each visit removes one exact copy. Another ZDOTDIR is a
        // file Canager does not read: the visit is there, and
        // `shell_config_leftovers` has no contents for it. An empty
        // ZDOTDIR is no ZDOTDIR (shell.rs:213).
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
    fn test_shell_config_leftovers_reports_the_file_rustup_does_not_edit() {
        // This Mac (spec §6.4, re-checked 2026-09-25): `~/.zshenv:1` and
        // `~/.profile:1` hold rustup's line, `~/.zshrc:17` holds the same
        // line but rustup never edits `.zshrc` -- after the uninstall
        // every new zsh prints `no such file or directory: …/.cargo/env`.
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
        assert_eq!(warnings_with(&d, &[]), expected);
        let brew = home.dir("opt/homebrew");
        home.dir("opt/homebrew/Cellar/rustup/1.29.1");
        let mut with_brew = expected.clone();
        with_brew.insert(3, Warning::HomebrewRustupLosesToolchains);
        assert_eq!(warnings_with(&d, &[brew]), with_brew);

        // No toolchains directory, nothing cargo-installed, no startup
        // files: the three unconditional lines, the toolchain one without
        // names.
        let home = TempHome::new("rustup-warnings-bare");
        let cargo_home = home.dir(".cargo");
        let d = detected(home.path(), &cargo_home);
        assert_eq!(
            warnings_with(&d, &[]),
            vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: Vec::new()
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::EditsShellConfig,
            ]
        );
        // `uninstall_warnings` is the same function over the real
        // prefixes -- whatever this Mac's Cellar holds, the two answers
        // agree; it is exercised through `plan` in Task 6 (with the
        // Homebrew line filtered, since that depends on the test Mac).
        let real: Vec<PathBuf> = HOMEBREW_PREFIXES.iter().map(PathBuf::from).collect();
        assert_eq!(uninstall_warnings(&d), warnings_with(&d, &real));
    }

    #[test]
    fn test_warnings_are_empty_for_a_layout_the_gate_refuses() {
        // `plan` refuses before it asks; this is the function's own
        // answer for a layout it will not describe.
        let home = TempHome::new("rustup-warnings-refused");
        let custom = home.dir("elsewhere/cargo");
        assert!(warnings_with(&detected(home.path(), &custom), &[]).is_empty());
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
