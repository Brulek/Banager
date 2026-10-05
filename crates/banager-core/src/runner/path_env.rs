use crate::protected::{resolve, Protected, Resolution};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEnv {
    pub path_dirs: Vec<PathBuf>,
    pub home: PathBuf,
    pub euid: u32,
    /// `CARGO_HOME` when the host environment sets it; `None` means "use the
    /// default", `home/.cargo`. Carried here rather than read from
    /// `std::env` inside the cargo adapter, for the same reason `path_dirs`
    /// is: a Finder-launched app's process environment is minimal, and an
    /// adapter that reaches around `HostEnv` cannot be tested.
    pub cargo_home: Option<PathBuf>,
    /// `RUSTUP_HOME` when the host environment sets it, raw; `None` means
    /// "use the default", `home/.rustup`. Same reasoning as `cargo_home`.
    /// Declared deferral: written by `discover` here, read from this
    /// step's Task 4 on by `StandaloneAdapter::detect`
    /// (adapters/standalone/mod.rs), which seats it for the rustup recipe:
    /// its uninstall is offered only when this resolves to the default
    /// (`rustup::standard_roots`, Task 5), and its toolchain names are
    /// read under it. Interpreted by `tool_home`, never taken as a path
    /// directly.
    pub rustup_home: Option<PathBuf>,
    /// `ZDOTDIR` when the host environment sets it, raw; `None` when
    /// unset. rustup's own uninstall (1.29.1 `shell.rs:207-225`) edits
    /// `$ZDOTDIR/.zshenv` and `$ZDOTDIR/.zprofile` as well as the ones
    /// under `HOME`, so the rustup recipe's preview follows the same
    /// visits (`rustup::rustup_rc_visits`, Task 5). Declared deferral, as
    /// `rustup_home`: read from Task 4 on by `StandaloneAdapter::detect`,
    /// which seats it.
    pub zdotdir: Option<PathBuf>,
    /// `OLLAMA_HOST` when the host environment sets it, normalised by
    /// `normalize_ollama_host` into an absolute http(s) url with no trailing
    /// slash; `None` means Ollama's own default, `http://127.0.0.1:11434`,
    /// and is also what an unusable value becomes. Same reasoning as
    /// `cargo_home`; consumed by Task 10. An `https` value is kept here
    /// but never reaches a daemon: `RealHttpClient::send` exempts only
    /// `http` from `ALLOWED_HTTPS_HOSTS` and refuses it (the Ollama
    /// section of `docs/what-we-run.md` says so).
    pub ollama_host: Option<String>,
}

/// Turns a raw `OLLAMA_HOST` into a url the Ollama adapter can prefix onto
/// `/api/tags`, or `None` to mean "use Ollama's own default".
///
/// The adapter builds every request as `format!("{host}/api/tags")`, so the
/// value has to be an absolute url with no trailing slash. Ollama's own
/// documentation gives `OLLAMA_HOST` as a bare `127.0.0.1:11434`, which
/// taken verbatim composes to the *relative* `127.0.0.1:11434/api/tags` —
/// the http client cannot fetch that at all (reqwest answers "builder
/// error"), so on a machine configured the documented way the daemon looks
/// permanently down, every refresh, forever.
///
/// The scheme is therefore supplied before parsing rather than after: a
/// named bare host is the case a bare `Url::parse` gets wrong, since
/// `localhost:11434` parses happily as scheme `localhost` with path
/// `11434`. Anything that still does not parse, or that is not http(s),
/// becomes `None` — a `file://` or `ftp://` value could only produce
/// something stranger than the default.
fn normalize_ollama_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let mut url = Url::parse(&candidate).ok()?;
    // Ollama uses 11434 for a schemeless host, but the scheme's default
    // (80/443) for an explicitly supplied http(s) URL. Preserve an explicit
    // port 80 too: url::Url elides it, so inspect the original authority.
    let authority = trimmed.split('/').next().unwrap_or(trimmed);
    let explicit_port = if authority.starts_with('[') {
        authority
            .split_once(']')
            .is_some_and(|(_, tail)| tail.starts_with(':'))
    } else {
        authority.contains(':')
    };
    if !trimmed.contains("://") && !explicit_port {
        url.set_port(Some(11434)).ok()?;
    }
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    // A url with no authority (`http://`) would compose to nonsense.
    url.host_str().filter(|h| !h.is_empty())?;
    Some(url.as_str().trim_end_matches('/').to_string())
}

impl HostEnv {
    /// Reads `PATH` -- the login shell's once a read has found it, else the
    /// process's own (`login_path::path`) -- and `HOME` and the rest from
    /// the process environment. A refresh calls it after waiting for the
    /// login shell's `PATH` to be read (`LoginPath::ensure`), so it sees
    /// the one read, or the inherited one when no read has worked.
    pub fn discover() -> HostEnv {
        HostEnv::discover_along(super::login_path::path())
    }

    /// `discover`, with the `PATH` given (`path`) rather than looked up:
    /// `login_path::round_env` reads it with whether it is the login
    /// shell's, in one look.
    pub fn discover_along(path: Option<std::ffi::OsString>) -> HostEnv {
        let path_dirs = path
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_default();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let euid = unsafe { libc::geteuid() };
        let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
        let rustup_home = std::env::var_os("RUSTUP_HOME").map(PathBuf::from);
        let zdotdir = std::env::var_os("ZDOTDIR").map(PathBuf::from);
        let ollama_host = std::env::var("OLLAMA_HOST")
            .ok()
            .as_deref()
            .and_then(normalize_ollama_host);
        HostEnv {
            path_dirs,
            home,
            euid,
            cargo_home,
            rustup_home,
            zdotdir,
            ollama_host,
        }
    }
}

/// Where a tool that reads its home through the `home` crate will look --
/// `home` 0.5.12, the version rustup 1.29.1 and cargo pin,
/// `cargo_home_with_cwd_env` and `rustup_home_with_cwd_env`
/// (crates/home/src/env.rs:67-79, :101-113): `setting` when it is set,
/// not empty and absolute; `<home>/<default_dir>` when it is unset or
/// empty (the crate filters an empty value out before it looks at it);
/// `None` when it is relative. The crate joins a relative value onto the
/// *tool's* current directory, which Banager neither knows nor shares --
/// a Finder-launched app's is `/` -- so nothing Banager could read or
/// lock would be the directory the tool uses, and "unsupported" is the
/// only honest answer. Reader: `cargo::cargo_home_of` (cargo's instance;
/// from this step's Task 4 on also the rustup recipe's `$CARGO_HOME`
/// paths and cargo lock), joined in Task 4 by `StandaloneAdapter::detect`
/// reading `rustup_home` (the rustup recipe's uninstall gate and
/// toolchain listing).
pub(crate) fn tool_home(setting: Option<&Path>, home: &Path, default_dir: &str) -> Option<PathBuf> {
    match setting {
        Some(p) if p.as_os_str().is_empty() => Some(home.join(default_dir)),
        Some(p) if p.is_absolute() => Some(p.to_path_buf()),
        Some(_) => None,
        None => Some(home.join(default_dir)),
    }
}

/// The first `<dir>/<name>` on `PATH` that is, or leads to, a regular
/// file, as `PATH` spells it. Each one is followed a step at a time
/// (`protected::resolve`): a `PATH` folder in, or a `name` that leads
/// into, one of the places Banager never looks into (`~/Documents`,
/// iCloud Drive, `/Volumes`, ...) is passed over without being looked at,
/// as if that file were not there -- so finding a package manager never
/// raises the macOS prompt for those places, nor waits on a disk that is
/// gone.
pub fn resolve_exe(name: &str, env: &HostEnv) -> Option<PathBuf> {
    let protected = Protected::new(&env.home);
    env.path_dirs
        .iter()
        .map(|dir| dir.join(name))
        .find(|candidate| {
            matches!(
                resolve(candidate, &protected, true),
                Resolution::Found(_, meta) if meta.is_file()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn regression_bare_ollama_hosts_use_the_daemons_default_port() {
        for (raw, expected) in [
            ("localhost", "http://localhost:11434"),
            ("127.0.0.1", "http://127.0.0.1:11434"),
            ("[::1]", "http://[::1]:11434"),
            ("localhost:80", "http://localhost"),
            ("http://localhost", "http://localhost"),
            ("https://localhost", "https://localhost"),
            ("localhost:1234", "http://localhost:1234"),
        ] {
            assert_eq!(
                normalize_ollama_host(raw).as_deref(),
                Some(expected),
                "{raw}"
            );
        }
    }

    #[test]
    fn test_discover_reads_a_nonempty_path() {
        let env = HostEnv::discover();
        assert!(!env.path_dirs.is_empty());
    }

    #[test]
    fn test_resolve_exe_finds_sh_on_a_real_mac() {
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/bin"), PathBuf::from("/usr/bin")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert_eq!(resolve_exe("sh", &env), Some(PathBuf::from("/bin/sh")));
    }

    #[test]
    fn test_resolve_exe_never_looks_into_a_protected_place_on_path() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = crate::testing::unique_temp_path("resolve-exe");
        let home = std::fs::canonicalize({
            std::fs::create_dir_all(&root).unwrap();
            &root
        })
        .unwrap();
        let mk = |relative: &str| {
            let path = home.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        // A `PATH` folder in ~/Documents (locked, so a look inside would
        // fail rather than find it), a `~/bin` that is a link into iCloud
        // Drive, a link onto another disk, then the real one.
        let in_documents = mk("Documents/scripts/npm");
        mk("Library/Mobile Documents/com~apple~CloudDocs/bin/npm");
        symlink(
            home.join("Library/Mobile Documents/com~apple~CloudDocs/bin"),
            home.join("bin"),
        )
        .unwrap();
        std::fs::create_dir_all(home.join("links")).unwrap();
        symlink(
            "/Volumes/Banager-test-no-such-disk/npm",
            home.join("links/npm"),
        )
        .unwrap();
        let real = mk("tools/npm");
        let locked = in_documents.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let env = HostEnv {
            path_dirs: vec![
                locked.clone(),
                home.join("bin"),
                home.join("links"),
                home.join("tools"),
            ],
            home: home.clone(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let found = resolve_exe("npm", &env);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(found, Some(real));
    }

    #[test]
    fn test_resolve_exe_returns_none_for_a_missing_binary() {
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/bin")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert_eq!(resolve_exe("definitely-not-a-real-binary-xyz", &env), None);
    }

    #[test]
    fn test_tool_home_follows_the_home_crates_rule() {
        // `home` 0.5.12 (the crate rustup 1.29.1 and cargo read their homes
        // through), `cargo_home_with_cwd_env` / `rustup_home_with_cwd_env`
        // (crates/home/src/env.rs:67-79, :101-113): an unset or *empty*
        // variable means `<home>/<default>`; an absolute one is taken as
        // is; a relative one is joined onto the tool's own current
        // directory, which Banager neither knows nor shares -- so for
        // Banager it is unsupported, and nothing pretends to know where
        // the tool will look.
        let home = Path::new("/Users/someone");
        assert_eq!(
            tool_home(None, home, ".cargo"),
            Some(PathBuf::from("/Users/someone/.cargo"))
        );
        assert_eq!(
            tool_home(Some(Path::new("")), home, ".cargo"),
            Some(PathBuf::from("/Users/someone/.cargo")),
            "an empty CARGO_HOME is filtered out before the crate looks at it"
        );
        assert_eq!(
            tool_home(Some(Path::new("/Volumes/Data/cargo")), home, ".cargo"),
            Some(PathBuf::from("/Volumes/Data/cargo"))
        );
        assert_eq!(
            tool_home(Some(Path::new("cargo-home")), home, ".rustup"),
            None,
            "relative: the crate joins it onto the tool's cwd, not Banager's"
        );
        assert_eq!(
            tool_home(None, home, ".rustup"),
            Some(PathBuf::from("/Users/someone/.rustup"))
        );
    }

    #[test]
    fn test_discover_reads_rustup_home_and_zdotdir_like_cargo_home() {
        // Both are read the way `cargo_home` is: raw, from the process
        // environment (only PATH is taken from the login shell, and kept
        // apart from it: `login_path::accept`), `None` when unset. Interpreting them -- empty means
        // default, relative means unsupported -- is `tool_home`'s and the
        // rustup recipe's job, not this reader's. The test does not set
        // the variables (a test must not change the process environment
        // other tests read); it pins the shape against the variables as
        // they are.
        let env = HostEnv::discover();
        assert_eq!(
            env.rustup_home,
            std::env::var_os("RUSTUP_HOME").map(PathBuf::from)
        );
        assert_eq!(env.zdotdir, std::env::var_os("ZDOTDIR").map(PathBuf::from));
    }

    #[test]
    fn test_normalize_ollama_host_defaults_the_scheme_for_the_documented_bare_form() {
        // Ollama's own docs give OLLAMA_HOST as a bare host:port, and that
        // is what most machines that set it at all are set to. Kept
        // verbatim it composes to `127.0.0.1:11434/api/tags`, a relative
        // url the http client cannot fetch at all -- so the daemon looks
        // permanently down on a machine configured the documented way.
        assert_eq!(
            normalize_ollama_host("127.0.0.1:11434").as_deref(),
            Some("http://127.0.0.1:11434")
        );
        assert_eq!(
            normalize_ollama_host("0.0.0.0:11434").as_deref(),
            Some("http://0.0.0.0:11434")
        );
        // A named host is the case a naive `Url::parse` gets wrong:
        // `localhost:11434` parses happily as scheme `localhost`, path
        // `11434`, so the scheme has to be supplied before parsing.
        assert_eq!(
            normalize_ollama_host("localhost:11434").as_deref(),
            Some("http://localhost:11434")
        );
    }

    #[test]
    fn test_normalize_ollama_host_keeps_a_full_url_as_it_is() {
        assert_eq!(
            normalize_ollama_host("http://10.0.0.5:11434").as_deref(),
            Some("http://10.0.0.5:11434")
        );
        assert_eq!(
            normalize_ollama_host("https://ollama.example.com").as_deref(),
            Some("https://ollama.example.com")
        );
    }

    #[test]
    fn test_normalize_ollama_host_drops_a_trailing_slash() {
        // `format!("{host}/api/tags")` on a host that keeps its slash gives
        // `http://127.0.0.1:11434//api/tags`.
        let host = normalize_ollama_host("http://127.0.0.1:11434/").expect("a valid url");
        assert_eq!(host, "http://127.0.0.1:11434");
        assert_eq!(
            format!("{host}/api/tags"),
            "http://127.0.0.1:11434/api/tags"
        );
        assert_eq!(
            normalize_ollama_host("127.0.0.1:11434/").as_deref(),
            Some("http://127.0.0.1:11434")
        );
    }

    #[test]
    fn test_normalize_ollama_host_refuses_what_it_cannot_use() {
        // `None` means "use Ollama's default", which is the right answer for
        // a value that could only produce an unfetchable url.
        assert_eq!(normalize_ollama_host("not a url"), None);
        assert_eq!(normalize_ollama_host(":::"), None);
        assert_eq!(normalize_ollama_host(""), None);
        assert_eq!(normalize_ollama_host("   "), None);
        assert_eq!(normalize_ollama_host("http://"), None);
        // Not http(s): the adapter can only speak to an http endpoint, and
        // silently prefixing `file://` or `ftp://` onto `/api/tags` would
        // produce something stranger than the default.
        assert_eq!(normalize_ollama_host("ftp://10.0.0.5:11434"), None);
        assert_eq!(normalize_ollama_host("file:///etc/passwd"), None);
    }
}
