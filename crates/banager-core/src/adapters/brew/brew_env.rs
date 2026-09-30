//! What Homebrew's own launcher does to the environment Banager hands it,
//! for the two variables that decide whether a `brew` command Banager runs
//! also deletes or uninstalls software its preview never named.
//!
//! Every `brew` command Banager runs carries `HOMEBREW_NO_AUTOREMOVE=1` and
//! `HOMEBREW_NO_INSTALL_CLEANUP=1` (`BrewAdapter::ENV`). Before any of
//! Homebrew's Ruby runs, `bin/brew` (Homebrew 7.0.6-70,
//! `/opt/homebrew/bin/brew:128-180`) exports each `HOMEBREW_*` line of up to
//! three `brew.env` files over what it inherited, so one line in one of them
//! takes either variable back:
//!
//! - After `brew uninstall`, of a formula or a cask, Homebrew runs its
//!   autoremove unless `HOMEBREW_NO_AUTOREMOVE` is set
//!   (`cmd/uninstall.rb:129-136`). Autoremove uninstalls the formulae that
//!   were installed only as dependencies and that nothing installed needs
//!   any more -- any on the system, not only the uninstalled package's own
//!   (`cleanup.rb:1038-1077`).
//! - After `brew install` and `brew upgrade` (`cmd/install.rb:504-509`,
//!   `cmd/upgrade.rb:363-368`, `install.rb:325-329`), unless
//!   `HOMEBREW_NO_INSTALL_CLEANUP` is set, Homebrew deletes the older
//!   installed versions and old downloads of the package the command names,
//!   every time (`Cleanup.install_clean!`, `cleanup.rb:361-389`),
//!   and runs a full `brew cleanup` when the last one it recorded
//!   (`$HOMEBREW_CACHE/.cleaned`) is more than
//!   `HOMEBREW_CLEANUP_PERIODIC_FULL_DAYS` days old, 30 unless set
//!   (`cleanup.rb:418-445`). That one deletes the older installed versions
//!   of every installed formula and old downloads in Homebrew's cache
//!   (`cleanup.rb:448-465`, `:473`), and ends in the same autoremove unless
//!   `HOMEBREW_NO_AUTOREMOVE` is set (`cleanup.rb:471`).
//!
//! `after_brew_env` replays what `bin/brew` does to those variables for one
//! plan's environment and says what Homebrew's Ruby then makes of them;
//! `BrewAdapter::plan` turns the answer into `Warning::HomebrewAutoremoves`,
//! or `Warning::HomebrewPeriodicCleanup` and then, with autoremove back
//! too, `Warning::HomebrewCleanupAutoremoves`. Everything it reads --
//! Banager's environment, the files -- comes through the two functions it
//! is handed.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// Homebrew's autoremove switch: a `boolean: true` variable
/// (`env_config.rb:540-544`), so `0` or `false` reads as unset.
pub(crate) const NO_AUTOREMOVE: &str = "HOMEBREW_NO_AUTOREMOVE";
/// Homebrew's cleanup-after-install switch: a `boolean: :set` variable
/// (`env_config.rb:605-611`), on whenever it is not blank.
pub(crate) const NO_INSTALL_CLEANUP: &str = "HOMEBREW_NO_INSTALL_CLEANUP";
/// Where `bin/brew` looks for the user's `brew.env` when `XDG_CONFIG_HOME`
/// is not set (`bin/brew:168-170`). It can come from Banager's environment
/// or from either of the two files read before that choice.
const XDG_CONFIG_FALLBACK: &str = "HOMEBREW_XDG_CONFIG_HOME";
/// When set after the system file is read, `bin/brew` reads that file again
/// last, so it overrides the other two (`bin/brew:155-159`, `:178-181`).
const SYSTEM_TAKES_PRIORITY: &str = "HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY";

/// The system-wide file, the first `bin/brew` reads (`bin/brew:153`).
pub(crate) const SYSTEM_FILE: &str = "/etc/homebrew/brew.env";

/// The variables the replay follows, in `Vars`' order.
const FOLLOWED: [&str; 4] = [
    NO_AUTOREMOVE,
    NO_INSTALL_CLEANUP,
    XDG_CONFIG_FALLBACK,
    SYSTEM_TAKES_PRIORITY,
];

/// `Homebrew::EnvConfig`'s `FALSY_VALUES` (`env_config.rb:871`): a
/// `boolean: true` variable set to one of these, in any case, reads as not
/// set (`env_config.rb:926`).
const FALSY_VALUES: [&str; 5] = ["false", "no", "off", "nil", "0"];

/// What Homebrew's Ruby makes of the two switches once `bin/brew` has read
/// the `brew.env` files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HomebrewSwitches {
    /// `Homebrew::EnvConfig.no_autoremove?`: false means Homebrew
    /// autoremoves after an uninstall, and in a cleanup.
    pub(crate) no_autoremove: bool,
    /// `Homebrew::EnvConfig.no_install_cleanup?`: false means an install or
    /// upgrade cleans up after the package it names, and ends in a full
    /// cleanup when one is due.
    pub(crate) no_install_cleanup: bool,
}

/// What Homebrew will make of `HOMEBREW_NO_AUTOREMOVE` and
/// `HOMEBREW_NO_INSTALL_CLEANUP` for a `brew` command started with
/// `plan_env` over Banager's own environment, from Homebrew's prefix
/// `prefix`.
///
/// `banager_var` reads a variable of Banager's environment, which the
/// command inherits under `plan_env` (`RealRunner::run` clears nothing);
/// `read_file` reads one `brew.env` file, `None` when there is none to read.
/// The files and their order are `bin/brew`'s: `/etc/homebrew/brew.env`,
/// then `<prefix>/etc/homebrew/brew.env`, then the user's --
/// `$XDG_CONFIG_HOME/homebrew/brew.env` when Banager's environment sets
/// `XDG_CONFIG_HOME`, else `$HOMEBREW_XDG_CONFIG_HOME/homebrew/brew.env`
/// when that is set by then, else `~/.homebrew/brew.env` -- and the system
/// file once more at the end when `HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY` was
/// set once it had been read. Each file's lines apply in order, so the last
/// line to set a variable is the one Homebrew sees.
pub(crate) fn after_brew_env(
    plan_env: &[(String, String)],
    prefix: &Path,
    banager_var: &dyn Fn(&str) -> Option<OsString>,
    read_file: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> HomebrewSwitches {
    let mut vars = Vars::inherited(plan_env, banager_var);
    let export = |vars: &mut Vars, path: &Path| {
        if let Some(bytes) = read_file(path) {
            vars.export_file(&bytes);
        }
    };
    export(&mut vars, Path::new(SYSTEM_FILE));
    let system_takes_priority = vars.non_empty(SYSTEM_TAKES_PRIORITY);
    export(
        &mut vars,
        &concat(prefix.as_os_str(), "/etc/homebrew/brew.env"),
    );
    if let Some(user_file) = user_file(&vars, banager_var) {
        export(&mut vars, &user_file);
    }
    if system_takes_priority {
        export(&mut vars, Path::new(SYSTEM_FILE));
    }
    HomebrewSwitches {
        no_autoremove: boolean_true(vars.get(NO_AUTOREMOVE)),
        no_install_cleanup: present(vars.get(NO_INSTALL_CLEANUP)),
    }
}

/// `${HOMEBREW_USER_CONFIG_HOME}/brew.env` as `bin/brew:165-175` builds it,
/// by joining strings as bash does. `None` when `HOME` is unset or empty
/// as well: `bin/brew` then stops before it reads any file
/// (`bin/brew:39-43`), and nothing runs.
fn user_file(vars: &Vars, banager_var: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |value: Option<OsString>| value.filter(|v| !v.is_empty());
    let config_home = if let Some(xdg) = set(banager_var("XDG_CONFIG_HOME")) {
        concat(&xdg, "/homebrew")
    } else if let Some(fallback) = vars.get(XDG_CONFIG_FALLBACK).filter(|v| !v.is_empty()) {
        concat(OsStr::from_bytes(fallback), "/homebrew")
    } else {
        concat(&set(banager_var("HOME"))?, "/.homebrew")
    };
    Some(concat(config_home.as_os_str(), "/brew.env"))
}

/// `"${head}${tail}"`, as bash joins them: no separator added or removed.
fn concat(head: &OsStr, tail: &str) -> PathBuf {
    let mut bytes = head.as_bytes().to_vec();
    bytes.extend_from_slice(tail.as_bytes());
    PathBuf::from(OsString::from_vec(bytes))
}

/// The followed variables as `bin/brew` holds them, as bytes: neither a
/// value in the environment nor one in a file need be UTF-8.
struct Vars([Option<Vec<u8>>; 4]);

impl Vars {
    /// What `bin/brew` starts from: the plan's own value, which the runner
    /// sets over Banager's environment, else Banager's own.
    fn inherited(
        plan_env: &[(String, String)],
        banager_var: &dyn Fn(&str) -> Option<OsString>,
    ) -> Vars {
        Vars(FOLLOWED.map(|name| {
            plan_env
                .iter()
                .rev()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone().into_bytes())
                .or_else(|| banager_var(name).map(OsStringExt::into_vec))
        }))
    }

    fn get(&self, name: &str) -> Option<&[u8]> {
        let index = FOLLOWED.iter().position(|followed| *followed == name)?;
        self.0[index].as_deref()
    }

    fn slot(&mut self, name: &[u8]) -> Option<&mut Option<Vec<u8>>> {
        let index = FOLLOWED
            .iter()
            .position(|followed| followed.as_bytes() == name)?;
        Some(&mut self.0[index])
    }

    /// `[[ -n "${NAME-}" ]]`.
    fn non_empty(&self, name: &str) -> bool {
        self.get(name).is_some_and(|value| !value.is_empty())
    }

    /// `export_homebrew_env_file` (`bin/brew:128-150`) over one file's bytes,
    /// for the followed variables, as `/bin/bash` 3.2 -- the shell
    /// `bin/brew`'s `#!` names -- runs it:
    ///
    /// - `while read -r line`: a line is what comes before a newline, and
    ///   `read` fails at the end of the file, so text after the last
    ///   newline -- a last line with none after it -- is never looked at.
    /// - A NUL byte ends the line where it stands; `read` then drops the
    ///   spaces and tabs at either end (the default `IFS`), and `-r` leaves
    ///   every backslash where it is.
    /// - `export "${line?}"`: the name is what comes before the first `=`
    ///   and the value everything after it, quotes, spaces and `#`s
    ///   included -- bash does not parse it again. `NAME+=value` appends.
    ///   A line with no `=` (`export NAME`) changes no value, and bash
    ///   refuses one whose name is not an identifier (`NAME =0`), which is
    ///   not one of these names either.
    ///
    /// The checks before the `export` (`bin/brew:137-146`) only skip lines,
    /// and every followed name starts with `HOMEBREW_` and is none of the
    /// five `bin/brew` keeps for itself, so a line that sets one is never
    /// skipped; a line such as `export HOMEBREW_NO_AUTOREMOVE=0` does not
    /// start with a followed name, and changes nothing here either.
    fn export_file(&mut self, bytes: &[u8]) {
        let mut lines: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
        lines.pop();
        for line in lines {
            let line = line.split(|&b| b == 0).next().unwrap_or_default();
            let line = trim_blanks(line);
            let Some(eq) = line.iter().position(|&b| b == b'=') else {
                continue;
            };
            let (name, value) = (&line[..eq], &line[eq + 1..]);
            if let Some(name) = name.strip_suffix(b"+") {
                if let Some(slot) = self.slot(name) {
                    slot.get_or_insert_with(Vec::new).extend_from_slice(value);
                }
            } else if let Some(slot) = self.slot(name) {
                *slot = Some(value.to_vec());
            }
        }
    }
}

/// `line` without the spaces and tabs at either end.
fn trim_blanks(line: &[u8]) -> &[u8] {
    let blank = |b: &u8| *b == b' ' || *b == b'\t';
    let start = line.iter().position(|b| !blank(b)).unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|b| !blank(b))
        .map_or(start, |i| i + 1);
    &line[start..end]
}

/// Whether Homebrew's Ruby sees the variable as set at all: `bin/brew`
/// passes on no empty variable (`bin/brew:310-316`), and Homebrew counts a
/// value made only of whitespace as unset too (`String#blank?`,
/// `extend/blank/string.rb`). That is `boolean: :set`'s whole reading
/// (`env_config.rb:926`). A value that is not UTF-8 is not blank: Homebrew's
/// check raises on it, and nothing it guards runs.
fn present(value: Option<&[u8]>) -> bool {
    match value {
        None => false,
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => !text.chars().all(char::is_whitespace),
            Err(_) => true,
        },
    }
}

/// `boolean: true`'s reading (`env_config.rb:926`): set, and, lowercased,
/// none of `FALSY_VALUES`.
fn boolean_true(value: Option<&[u8]>) -> bool {
    present(value)
        && value
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .is_none_or(|text| !FALSY_VALUES.contains(&text.to_lowercase().as_str()))
}

/// One `brew.env` file's bytes, or `None` unless `path` leads, links
/// followed, to a regular file Banager can read: `bin/brew` reads one only
/// when `[[ -r … ]]` holds (`bin/brew:131-132`). Nothing but a regular file
/// is opened -- a named pipe would wait for a writer -- and the open file is
/// checked again, as rustup's startup files are (`read_startup_file` in
/// adapters/standalone/rustup.rs).
pub(crate) fn read_brew_env_file(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// What every `brew` command carries (`BrewAdapter::ENV`), the two
    /// followed variables among them.
    fn plan_env() -> Vec<(String, String)> {
        crate::adapters::brew::BrewAdapter::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const PREFIX_FILE: &str = "/opt/homebrew/etc/homebrew/brew.env";
    const HOME_FILE: &str = "/Users/someone/.homebrew/brew.env";

    /// `after_brew_env` for `/opt/homebrew` with `files` on disk and
    /// `vars` as Banager's environment, `HOME` included unless `vars`
    /// names it.
    fn switches(files: &[(&str, &str)], vars: &[(&str, &str)]) -> HomebrewSwitches {
        let files: HashMap<PathBuf, Vec<u8>> = files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
            .collect();
        let mut env: HashMap<String, OsString> =
            HashMap::from([("HOME".to_string(), OsString::from("/Users/someone"))]);
        for (name, value) in vars {
            env.insert(name.to_string(), OsString::from(value));
        }
        after_brew_env(
            &plan_env(),
            Path::new("/opt/homebrew"),
            &|name| env.get(name).cloned(),
            &|path| files.get(path).cloned(),
        )
    }

    fn autoremoves(files: &[(&str, &str)], vars: &[(&str, &str)]) -> bool {
        !switches(files, vars).no_autoremove
    }

    #[test]
    fn test_with_no_brew_env_file_banagers_two_switches_stand() {
        assert_eq!(
            switches(&[], &[]),
            HomebrewSwitches {
                no_autoremove: true,
                no_install_cleanup: true,
            }
        );
    }

    #[test]
    fn test_every_value_homebrew_reads_as_unset_turns_autoremove_back_on() {
        // `FALSY_VALUES` in any case (`env_config.rb:871`, `:926`); nothing
        // at all, which `bin/brew` does not pass on; spaces or tabs alone,
        // which `read` trims to nothing; and a carriage return alone -- a
        // file saved with Windows line ends -- which `read` keeps and
        // Homebrew's `blank?` counts as whitespace.
        for value in [
            "0", "false", "no", "off", "nil", "FALSE", "No", "OFF", "Nil", "", "   ", "\t", "\r",
        ] {
            let file = format!("HOMEBREW_NO_AUTOREMOVE={value}\n");
            assert!(
                autoremoves(&[(SYSTEM_FILE, &file)], &[]),
                "HOMEBREW_NO_AUTOREMOVE={value:?} in brew.env turns autoremove back on"
            );
        }
    }

    #[test]
    fn test_any_other_value_leaves_autoremove_off() {
        // bash keeps quotes, inner spaces, a `#` and a trailing `\r` as
        // part of the value, and Homebrew compares the whole of it.
        for value in [
            "1", "true", "yes", "\"0\"", "'false'", " 0", "0 # off", "0\r", "0\\", "nope",
        ] {
            let file = format!("HOMEBREW_NO_AUTOREMOVE={value}\n");
            assert!(
                !autoremoves(&[(SYSTEM_FILE, &file)], &[]),
                "HOMEBREW_NO_AUTOREMOVE={value:?} keeps autoremove off"
            );
        }
    }

    #[test]
    fn test_each_of_the_three_files_is_read() {
        let off = "HOMEBREW_NO_AUTOREMOVE=0\n";
        assert!(
            autoremoves(&[(SYSTEM_FILE, off)], &[]),
            "/etc/homebrew/brew.env"
        );
        assert!(
            autoremoves(&[(PREFIX_FILE, off)], &[]),
            "the prefix's etc/homebrew/brew.env"
        );
        assert!(
            autoremoves(&[(HOME_FILE, off)], &[]),
            "~/.homebrew/brew.env"
        );
        assert!(
            !autoremoves(&[("/usr/local/etc/homebrew/brew.env", off)], &[]),
            "another prefix's file is not this Homebrew's"
        );
    }

    #[test]
    fn test_the_users_file_is_under_xdg_config_home_when_banagers_environment_sets_it() {
        let off = "HOMEBREW_NO_AUTOREMOVE=0\n";
        let xdg = [("XDG_CONFIG_HOME", "/Users/someone/.config")];
        assert!(autoremoves(
            &[("/Users/someone/.config/homebrew/brew.env", off)],
            &xdg
        ));
        assert!(
            !autoremoves(&[(HOME_FILE, off)], &xdg),
            "with XDG_CONFIG_HOME set, ~/.homebrew/brew.env is not read"
        );
        // An empty XDG_CONFIG_HOME counts as unset (`[[ -n … ]]`).
        assert!(autoremoves(&[(HOME_FILE, off)], &[("XDG_CONFIG_HOME", "")]));
        // bash joins the strings: a trailing slash stays, and names the same file.
        assert!(autoremoves(
            &[("/cfg//homebrew/brew.env", off)],
            &[("XDG_CONFIG_HOME", "/cfg/")]
        ));
    }

    #[test]
    fn test_homebrew_xdg_config_home_is_the_users_folder_when_xdg_config_home_is_not() {
        // `bin/brew:168-170`: from Banager's environment, or from a file
        // read before the choice -- the system's or the prefix's.
        let off = "HOMEBREW_NO_AUTOREMOVE=0\n";
        assert!(autoremoves(
            &[("/hb/homebrew/brew.env", off)],
            &[("HOMEBREW_XDG_CONFIG_HOME", "/hb")]
        ));
        assert!(autoremoves(
            &[
                (SYSTEM_FILE, "HOMEBREW_XDG_CONFIG_HOME=/hb\n"),
                ("/hb/homebrew/brew.env", off),
            ],
            &[]
        ));
        assert!(
            !autoremoves(
                &[
                    (SYSTEM_FILE, "HOMEBREW_XDG_CONFIG_HOME=/hb\n"),
                    ("/hb/homebrew/brew.env", off),
                ],
                &[("XDG_CONFIG_HOME", "/cfg")]
            ),
            "XDG_CONFIG_HOME comes first"
        );
    }

    #[test]
    fn test_without_home_no_users_file_is_read() {
        // `bin/brew` stops before reading any file when HOME is empty.
        let off = "HOMEBREW_NO_AUTOREMOVE=0\n";
        assert!(!autoremoves(
            &[("/.homebrew/brew.env", off)],
            &[("HOME", "")]
        ));
    }

    #[test]
    fn test_the_last_line_to_set_the_variable_is_what_homebrew_sees() {
        let off = "HOMEBREW_NO_AUTOREMOVE=0\n";
        let on = "HOMEBREW_NO_AUTOREMOVE=1\n";
        // Within a file, and across the files in bin/brew's order.
        assert!(!autoremoves(
            &[(
                SYSTEM_FILE,
                "HOMEBREW_NO_AUTOREMOVE=0\nHOMEBREW_NO_AUTOREMOVE=1\n"
            )],
            &[]
        ));
        assert!(!autoremoves(&[(SYSTEM_FILE, off), (HOME_FILE, on)], &[]));
        assert!(autoremoves(&[(SYSTEM_FILE, on), (PREFIX_FILE, off)], &[]));
        assert!(autoremoves(&[(PREFIX_FILE, on), (HOME_FILE, off)], &[]));
    }

    #[test]
    fn test_the_system_file_comes_last_when_it_asks_to() {
        let system = "HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY=1\nHOMEBREW_NO_AUTOREMOVE=0\n";
        let on = "HOMEBREW_NO_AUTOREMOVE=1\n";
        assert!(autoremoves(&[(SYSTEM_FILE, system), (HOME_FILE, on)], &[]));
        // Or when Banager's own environment sets it.
        assert!(autoremoves(
            &[
                (SYSTEM_FILE, "HOMEBREW_NO_AUTOREMOVE=0\n"),
                (PREFIX_FILE, on)
            ],
            &[("HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY", "1")]
        ));
        // Only as the system file leaves it: set later, it is too late.
        assert!(!autoremoves(
            &[
                (SYSTEM_FILE, "HOMEBREW_NO_AUTOREMOVE=0\n"),
                (PREFIX_FILE, "HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY=1\n"),
                (HOME_FILE, on),
            ],
            &[]
        ));
    }

    #[test]
    fn test_lines_bash_reads_the_way_bin_brew_does() {
        // Spaces and tabs at either end go; the line is still read.
        assert!(autoremoves(
            &[(SYSTEM_FILE, "  \tHOMEBREW_NO_AUTOREMOVE=0 \t\n")],
            &[]
        ));
        // A last line with no newline after it is never looked at.
        assert!(!autoremoves(
            &[(SYSTEM_FILE, "HOMEBREW_NO_AUTOREMOVE=0")],
            &[]
        ));
        assert!(autoremoves(
            &[(
                SYSTEM_FILE,
                "HOMEBREW_NO_AUTOREMOVE=0\nHOMEBREW_NO_AUTOREMOVE=1"
            )],
            &[]
        ));
        // A NUL ends the line where it stands.
        assert!(autoremoves(
            &[(SYSTEM_FILE, "HOMEBREW_NO_AUTOREMOVE=0\0x\n")],
            &[]
        ));
        assert!(!autoremoves(
            &[(SYSTEM_FILE, "HOMEBREW_NO_AUTO\0REMOVE=0\n")],
            &[]
        ));
        // `NAME+=value` appends.
        assert!(!autoremoves(
            &[(SYSTEM_FILE, "HOMEBREW_NO_AUTOREMOVE+=0\n")],
            &[]
        ));
        assert!(autoremoves(
            &[(
                SYSTEM_FILE,
                "HOMEBREW_NO_AUTOREMOVE=\nHOMEBREW_NO_AUTOREMOVE+=off\n"
            )],
            &[]
        ));
        // Lines that change nothing: a comment, `export`, a name bash
        // refuses, no `=`, another variable.
        for line in [
            "# HOMEBREW_NO_AUTOREMOVE=0\n",
            "export HOMEBREW_NO_AUTOREMOVE=0\n",
            "HOMEBREW_NO_AUTOREMOVE =0\n",
            "HOMEBREW_NO_AUTOREMOVE\n",
            "HOMEBREW_NO_AUTOREMOVEX=0\n",
        ] {
            assert!(
                !autoremoves(&[(SYSTEM_FILE, line)], &[]),
                "{line:?} changes nothing"
            );
        }
    }

    #[test]
    fn test_banagers_plan_variable_is_what_bin_brew_starts_from() {
        // The runner sets the plan's variables over Banager's environment,
        // so a `HOMEBREW_NO_AUTOREMOVE=0` Banager inherited never reaches
        // Homebrew; only a brew.env line does.
        assert!(!autoremoves(&[], &[("HOMEBREW_NO_AUTOREMOVE", "0")]));
    }

    #[test]
    fn test_install_cleanup_comes_back_only_when_a_file_sets_it_to_nothing() {
        // `boolean: :set`: `0` and `false` still mean "set".
        for value in ["0", "false", "no"] {
            let file = format!("HOMEBREW_NO_INSTALL_CLEANUP={value}\n");
            assert!(
                switches(&[(SYSTEM_FILE, &file)], &[]).no_install_cleanup,
                "{value:?}"
            );
        }
        for value in ["", "  ", "\r"] {
            let file = format!("HOMEBREW_NO_INSTALL_CLEANUP={value}\n");
            assert!(
                !switches(&[(SYSTEM_FILE, &file)], &[]).no_install_cleanup,
                "{value:?}"
            );
        }
    }

    /// A folder under the system temp dir, removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "banager-brew-env-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_read_brew_env_file_reads_a_regular_file_and_nothing_else() {
        let dir = TempDir::new();
        let file = dir.0.join("brew.env");
        std::fs::write(&file, b"HOMEBREW_NO_AUTOREMOVE=0\n").expect("write brew.env");
        assert_eq!(
            read_brew_env_file(&file).as_deref(),
            Some(&b"HOMEBREW_NO_AUTOREMOVE=0\n"[..])
        );
        let link = dir.0.join("link.env");
        std::os::unix::fs::symlink(&file, &link).expect("link");
        assert!(
            read_brew_env_file(&link).is_some(),
            "a link to a file is followed"
        );
        assert_eq!(read_brew_env_file(&dir.0.join("missing.env")), None);
        assert_eq!(read_brew_env_file(&dir.0), None, "a folder is not read");
    }
}
