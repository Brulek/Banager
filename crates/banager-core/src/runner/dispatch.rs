//! Version managers' dispatchers, which Banager never runs (R73-1 of the
//! round-24 review). A dispatcher -- a file in a version manager's shims
//! folder (mise's, pyenv's, asdf's), or anything that leads to the mise
//! program itself -- runs whichever copy the manager picks when it runs,
//! and mise's may first download and install that copy: a tool set to
//! install lazily is installed whatever `MISE_NOT_FOUND_AUTO_INSTALL` says
//! (mise's "lazy tools", dev-tools/shims.html). Nothing Banager can read
//! without running it says which copy that is, or whether it is there, so
//! a program that is one, or a script whose `#!` line starts one, is not
//! run at all: `RealRunner::run` refuses it before it starts anything, and
//! pip's discovery passes it over before it asks anything
//! (`PipAdapter::detect`). Neither runs mise, pyenv or asdf to ask.

use crate::protected::{self, look, Protected};
use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Whether `path` -- a program as it was found, or `real`, where it leads
/// -- is a version manager's dispatcher: in a folder named `shims`
/// (`commands::in_shims`), or the mise program, by its name as the disk
/// compares names (mise's shims are links to it; so is a link of another
/// name somewhere else).
pub(crate) fn is_dispatcher(path: &Path, real: Option<&Path>) -> bool {
    let in_shims = |path: &Path| path.parent().is_some_and(crate::commands::in_shims);
    let is_mise = |path: &Path| {
        path.file_name()
            .is_some_and(|name| protected::same_path(Path::new(name), Path::new("mise")))
    };
    in_shims(path) || real.is_some_and(|real| in_shims(real) || is_mise(real))
}

/// Why `program` (leading to `real`) is not to be run, as the error
/// `RealRunner::run` gives instead of starting it, or `None` when it may
/// be: it is a dispatcher; or it is a script whose `#!` line runs one --
/// named whole, or a name `/usr/bin/env` looks for along `path_var` (the
/// `PATH` the command is given) or the folders its line names -- or whose
/// `env` name may be found in a `PATH` folder in a protected place, before
/// any that holds it, or along a `PATH` Banager cannot know. A relative
/// `PATH` folder is passed over, as `resolve_exe` passes it over (one
/// written `~/...`, which bash expands but `env` does not, among them); so
/// is one with no such file, one that is not an executable file, and one
/// that cannot be looked in for another reason (an entry that is a file, a
/// folder that may not be searched), as `env` passes over it.
pub(crate) fn refusal(
    program: &Path,
    real: &Path,
    path_var: Option<&OsStr>,
    protected: &Protected,
) -> Option<std::io::Error> {
    if is_dispatcher(program, Some(real)) {
        return Some(refused(
            "it is a version manager's shim, which may install what it runs",
        ));
    }
    let interpreter = interpreter_of(real, protected)?;
    let found = match interpreter {
        Interpreter::Named(path) => {
            let real = look::real_path(&path, protected).ok();
            return is_dispatcher(&path, real.as_deref()).then(|| {
                refused("it runs with a version manager's shim, which may install what it runs")
            });
        }
        Interpreter::OnPath(name, search) => match search {
            Search::Given => first_on_path(&name, path_var, protected),
            Search::Default => first_on_path(&name, None, protected),
            Search::Folders(path) => {
                first_on_path(&name, Some(OsStr::from_bytes(&path)), protected)
            }
            Search::Unknown => {
                return Some(refused(
                    "the PATH it finds the program it runs with along is not known",
                ))
            }
        },
    };
    match found {
        OnPath::Found(path, real) if is_dispatcher(&path, Some(&real)) => Some(refused(
            "the program it runs with is a version manager's shim, which may install what it runs",
        )),
        OnPath::Found(..) | OnPath::Nowhere => None,
        OnPath::Unknown => Some(refused(
            "a PATH folder that may hold the program it runs with could not be looked in",
        )),
    }
}

fn refused(why: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!("not run: {why}"),
    )
}

/// What a script's `#!` line runs.
#[derive(Debug, PartialEq, Eq)]
enum Interpreter {
    /// A program named whole (`#!/opt/homebrew/bin/python3`).
    Named(PathBuf),
    /// A name `/usr/bin/env` looks for (`#!/usr/bin/env node`,
    /// `#!/usr/bin/env -S node --flag`), and along what.
    OnPath(Vec<u8>, Search),
}

/// The first line of the file `real` leads to, when it starts with `#!`:
/// at most its first 512 bytes, read from the folder it is in, held open
/// (`protected::look::open`). `None` for anything else -- a program, a
/// file that cannot be read.
fn interpreter_of(real: &Path, protected: &Protected) -> Option<Interpreter> {
    let (mut file, _) = look::open(real, protected).ok()?;
    let mut head = [0u8; 512];
    // One read: a regular file gives what it has, up to the buffer.
    let read = file.read(&mut head).ok()?;
    parse_shebang(&head[..read])
}

fn parse_shebang(head: &[u8]) -> Option<Interpreter> {
    let line = head.strip_prefix(b"#!")?;
    let line = line.split(|byte| *byte == b'\n').next()?;
    let mut words = line
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty());
    let first = Path::new(OsStr::from_bytes(words.next()?));
    if first.file_name() != Some(OsStr::new("env")) {
        return Some(Interpreter::Named(first.to_path_buf()));
    }
    // macOS splits a `#!` line's words, so `env` reads them as separate
    // arguments: its own options (`getopt`'s "0iL:P:S:U:u:v"), then
    // `NAME=value` settings, then the name it looks for.
    let mut words: Vec<&[u8]> = words.collect();
    words.reverse();
    let mut search = Search::Given;
    let mut altpath = None;
    while let Some(word) = words.pop() {
        if word == b"--" {
            break;
        }
        let Some(cluster) = word.strip_prefix(b"-") else {
            words.push(word);
            break;
        };
        if cluster.is_empty() {
            search = Search::Default;
        }
        for (at, option) in cluster.iter().enumerate() {
            match option {
                b'0' | b'v' => continue,
                b'i' => {
                    search = Search::Default;
                    continue;
                }
                b'L' | b'P' | b'S' | b'U' | b'u' => {}
                _ => return None,
            }
            let rest = &cluster[at + 1..];
            let argument = if rest.is_empty() { words.pop()? } else { rest };
            match option {
                b'P' => altpath = Some(argument.to_vec()),
                b'u' if argument == b"PATH" => search = Search::Default,
                b'L' | b'U' => search = Search::Unknown,
                // `-S`'s string is split into arguments read in its place.
                b'S' => words.push(argument),
                _ => {}
            }
            break;
        }
    }
    while let Some(word) = words.pop() {
        if let Some(path) = word.strip_prefix(b"PATH=") {
            search = Search::Folders(path.to_vec());
        } else if !word.contains(&b'=') {
            let search = altpath.map(Search::Folders).unwrap_or(search);
            return Some(Interpreter::OnPath(word.to_vec(), search));
        }
    }
    None
}

/// The folders `env` looks along for a name.
#[derive(Debug, PartialEq, Eq)]
enum Search {
    /// The `PATH` the command is given.
    Given,
    /// `env`'s default, `PATH` taken away (`-i`, `-u PATH`).
    Default,
    /// Folders named on the line: `-P altpath`, or a `PATH=` setting.
    Folders(Vec<u8>),
    /// A `PATH` `env` may take from a login class (`-L`, `-U`), which
    /// Banager does not read.
    Unknown,
}

/// Where `env` finds a name along a `PATH`.
#[derive(Debug, PartialEq, Eq)]
enum OnPath {
    /// The first executable file of that name, as found and where it leads.
    Found(PathBuf, PathBuf),
    /// None of the folders holds one.
    Nowhere,
    /// A folder before any that holds one is in a protected place, never
    /// looked in.
    Unknown,
}

fn first_on_path(name: &[u8], path_var: Option<&OsStr>, protected: &Protected) -> OnPath {
    // `env`'s own default when no `PATH` is set (`_PATH_DEFPATH`).
    let path_var = path_var.unwrap_or(OsStr::new("/usr/bin:/bin"));
    let name = OsStr::from_bytes(name);
    if name.as_bytes().contains(&b'/') {
        // A name with a slash is run as it is, from the command's folder.
        let path = Path::new(name);
        return match look::real_path(path, protected) {
            Ok(real) if path.is_absolute() => OnPath::Found(path.to_path_buf(), real),
            _ => OnPath::Nowhere,
        };
    }
    for dir in std::env::split_paths(path_var).filter(|dir| dir.is_absolute()) {
        let candidate = dir.join(name);
        match look::target(&candidate, protected) {
            Ok((real, stat)) if stat.is_file() && stat.mode() & 0o111 != 0 => {
                return OnPath::Found(candidate, real)
            }
            Err(error) if look::is_protected(&error) => return OnPath::Unknown,
            // `execvp`, which `env` runs, passes over a folder that is not
            // one, may not be searched, or loops, as over one without it.
            Ok(_) | Err(_) => {}
        }
    }
    OnPath::Nowhere
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Temp {
            let dir = std::env::temp_dir().join(format!(
                "banager-dispatch-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Temp(std::fs::canonicalize(&dir).unwrap())
        }

        fn file(&self, at: &str, text: &str, mode: u32) -> PathBuf {
            let path = self.0.join(at);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            path
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_a_shebang_names_its_interpreter_or_the_name_env_looks_for() {
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env node\nrest"),
            Some(Interpreter::OnPath(b"node".to_vec(), Search::Given))
        );
        assert_eq!(
            parse_shebang(b"#! /usr/bin/env -S NODE_OPTIONS=x node --flag\n"),
            Some(Interpreter::OnPath(b"node".to_vec(), Search::Given))
        );
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env -i PATH=/a:/b node\n"),
            Some(Interpreter::OnPath(
                b"node".to_vec(),
                Search::Folders(b"/a:/b".to_vec())
            ))
        );
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env -P /c PATH=/a:/b node\n"),
            Some(Interpreter::OnPath(
                b"node".to_vec(),
                Search::Folders(b"/c".to_vec())
            ))
        );
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env -L someone node\n"),
            Some(Interpreter::OnPath(b"node".to_vec(), Search::Unknown))
        );
        assert_eq!(parse_shebang(b"#!/usr/bin/env -x node\n"), None);
        assert_eq!(
            parse_shebang(b"#!/opt/homebrew/bin/python3 -E\n"),
            Some(Interpreter::Named(PathBuf::from(
                "/opt/homebrew/bin/python3"
            )))
        );
        assert_eq!(parse_shebang(b"\xcf\xfa\xed\xfe"), None);
        assert_eq!(parse_shebang(b"#!/usr/bin/env\n"), None);
    }

    #[test]
    fn test_a_dispatcher_is_a_shim_or_leads_to_mise_however_it_is_named() {
        let temp = Temp::new("which");
        let mise = temp.file("bin/mise", "", 0o755);
        let shims = temp.0.join("SHIMS");
        std::fs::create_dir_all(&shims).unwrap();
        symlink(&mise, shims.join("python3")).unwrap();
        let alias = temp.0.join("elsewhere/python3");
        std::fs::create_dir_all(alias.parent().unwrap()).unwrap();
        symlink(&mise, &alias).unwrap();
        let pyenv = temp.file(".pyenv/shims/python3", "#!/bin/sh\n", 0o755);
        let python = temp.file("opt/bin/python3", "", 0o755);
        let protected = Protected::default();
        let real = |path: &Path| look::real_path(path, &protected).ok();
        for dispatcher in [shims.join("python3"), alias.clone(), pyenv, mise.clone()] {
            assert!(
                is_dispatcher(&dispatcher, real(&dispatcher).as_deref()),
                "{dispatcher:?}"
            );
        }
        assert!(!is_dispatcher(&python, real(&python).as_deref()));
    }

    #[test]
    fn test_a_script_is_refused_when_env_would_find_a_dispatcher_first() {
        let temp = Temp::new("env");
        let script = temp.file("npm/bin/npm", "#!/usr/bin/env node\n", 0o755);
        let mise = temp.file("bin/mise", "", 0o755);
        let shims = temp.0.join("mise/shims");
        std::fs::create_dir_all(&shims).unwrap();
        symlink(&mise, shims.join("node")).unwrap();
        let direct = temp.file("node/bin/node", "", 0o755);
        let not_executable = temp.file("plain/node", "", 0o644);
        let documents = temp.0.join("home/Documents/bin");
        std::fs::create_dir_all(&documents).unwrap();
        let path = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
        let protected = Protected::new(&temp.0.join("home"));
        let shims_first = path(&[&shims, direct.parent().unwrap()]);
        let direct_first = path(&[direct.parent().unwrap(), &shims]);
        let plain_then_shims = path(&[not_executable.parent().unwrap(), &shims]);
        let protected_first = path(&[&documents, direct.parent().unwrap()]);
        let relative_first = OsStr::new("~/node/bin:relative").to_owned();
        for (path_var, refused) in [
            (shims_first.as_os_str(), true),
            (plain_then_shims.as_os_str(), true),
            (protected_first.as_os_str(), true),
            (direct_first.as_os_str(), false),
            (relative_first.as_os_str(), false),
        ] {
            assert_eq!(
                refusal(&script, &script, Some(path_var), &protected).is_some(),
                refused,
                "{path_var:?}"
            );
        }
        let named = temp.file(
            "named",
            &format!("#!{}\n", shims.join("node").display()),
            0o755,
        );
        assert!(refusal(&named, &named, None, &protected).is_some());
        let error = refusal(&shims.join("node"), &mise, None, &protected).unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn regression_skeptic_env_options_taking_an_argument_still_find_the_dispatcher() {
        let temp = Temp::new("envopts");
        let mise = temp.file("bin/mise", "", 0o755);
        let shims = temp.0.join("mise/shims");
        std::fs::create_dir_all(&shims).unwrap();
        symlink(&mise, shims.join("node")).unwrap();
        let direct = temp.file("node/bin/node", "", 0o755);
        let protected = Protected::default();
        let path = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
        let shims_first = path(&[&shims, direct.parent().unwrap()]);
        let direct_first = path(&[direct.parent().unwrap(), &shims]);
        let shims_at = shims.display();
        let direct_at = direct.parent().unwrap().display();
        for (line, path_var, refused) in [
            ("-u NODE_OPTIONS node".to_owned(), &shims_first, true),
            ("-uNODE_OPTIONS node".to_owned(), &shims_first, true),
            ("-i -U someone node".to_owned(), &direct_first, true),
            ("-i node".to_owned(), &shims_first, false),
            (format!("-P {shims_at} node"), &direct_first, true),
            (format!("-iP{shims_at} node"), &direct_first, true),
            (format!("PATH={shims_at} node"), &direct_first, true),
            (format!("-P {direct_at} node"), &shims_first, false),
            ("-u PATH node".to_owned(), &shims_first, false),
            ("-- node".to_owned(), &shims_first, true),
        ] {
            let script = temp.file("script", &format!("#!/usr/bin/env {line}\n"), 0o755);
            assert_eq!(
                refusal(&script, &script, Some(path_var.as_os_str()), &protected).is_some(),
                refused,
                "{line}"
            );
        }
    }

    #[test]
    fn regression_skeptic_a_path_entry_env_passes_over_is_passed_over() {
        let temp = Temp::new("skip");
        let script = temp.file("npm/bin/npm", "#!/usr/bin/env node\n", 0o755);
        let direct = temp.file("node/bin/node", "", 0o755);
        let file = temp.file("a-file", "", 0o644);
        let locked = temp.0.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let protected = Protected::new(&temp.0.join("home"));
        let path = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
        let file_first = path(&[&file, direct.parent().unwrap()]);
        let locked_first = path(&[&locked, direct.parent().unwrap()]);
        let results = [&file_first, &locked_first].map(|path_var| {
            refusal(&script, &script, Some(path_var.as_os_str()), &protected).map(|e| e.to_string())
        });
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(results, [None, None]);
    }
}
