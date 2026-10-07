use crate::protected::{resolve, Protected, Resolution};
use std::path::{Path, PathBuf};
use url::{Position, Url};

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
/// So the value is read the way Ollama itself reads it -- `envconfig.Var`
/// and `envconfig.Host` in Ollama 0.40.0 (`envconfig/config.go:22-60,
/// 378-380`), whose test table `test_ollama_host_is_read_as_ollamas_own_
/// envconfig_reads_it` repeats -- and only then made a url: spaces and any
/// `"` or `'` around it dropped; no scheme means `http`, and port 11434
/// where none is given, while an explicit `http://` or `https://` keeps the
/// scheme's own 80 or 443; a bare `ollama.com` is `https://ollama.com`;
/// everything from the first `/` after the host is its path; a host that
/// is an IP address without brackets (`::1`) is one; a port that is not a
/// number from 0 to 65535 is the default one; and a port with no host
/// (`:11500`) is this Mac -- Go's dialer reads an empty host so, and
/// `127.0.0.1` is how a url says it. Without this, `:11500`, `::1` or a
/// quoted value -- each one Ollama, and the `ollama` Banager runs, reads
/// as a daemon -- became the default daemon instead (r40 R40-2).
///
/// Three things are Banager's own. A login in front of the host
/// (`user:password@host`) is kept for its requests, though Ollama's own
/// command refuses one. A value with an `@` after its host -- a login
/// whose password has a raw `/`, `?`, `#` or `\` in it, which Ollama and
/// the url both cut short, taking its start for the host -- becomes `None`:
/// the host would be no daemon the user named, and the rest of the
/// password would sit where neither the window's id nor the preview masks
/// a login. And a value that still makes no http(s) url -- a `file://` or
/// `ftp://` one, a host a url cannot carry -- becomes `None` too: it could
/// only name something stranger than the default.
fn normalize_ollama_host(raw: &str) -> Option<String> {
    // `envconfig.Var`: spaces off, then every `"` and `'` at either end;
    // `Host` takes spaces off once more.
    let value = raw.trim().trim_matches(['"', '\'']).trim();
    if value.is_empty() {
        return None;
    }
    let (scheme, rest, default_port) = match value.split_once("://") {
        None if value == "ollama.com" => ("https", "ollama.com:443", 443),
        None => ("http", value, 11434),
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => ("http", rest, 80),
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => ("https", rest, 443),
        Some(_) => return None,
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (login, hostport) = match authority.rsplit_once('@') {
        Some((login, hostport)) => (Some(login), hostport),
        None => (None, authority),
    };
    let (host, port) = match split_host_port(hostport) {
        Some((host, port)) => (host.to_string(), port),
        // No port to split off: the whole of it is the host -- an IP
        // address, brackets or none, written as Go writes one -- or none.
        None => {
            let host = match hostport
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
            {
                Ok(ip) => ip.to_canonical().to_string(),
                Err(_) if hostport.is_empty() => "127.0.0.1".to_string(),
                Err(_) => hostport.to_string(),
            };
            (host, "")
        }
    };
    // `strconv.ParseInt(port, 10, 32)`, then 0 to 65535.
    let port = match port.parse::<i32>() {
        Ok(port) if (0..=65535).contains(&port) => port,
        _ => default_port,
    };
    let host = match host.as_str() {
        "" => "127.0.0.1".to_string(),
        // `net.JoinHostPort`.
        _ if host.contains(':') => format!("[{host}]"),
        _ => host,
    };
    let login = login.map(|login| format!("{login}@")).unwrap_or_default();
    let url = Url::parse(&format!("{scheme}://{login}{host}:{port}/{path}")).ok()?;
    url.host_str().filter(|h| !h.is_empty())?;
    // An `@` after the host is the rest of a login that a raw `/`, `?`, `#`
    // or `\` in its password cut short, the start of it taken for the host:
    // no daemon the user named, and a part of the password where neither
    // mask looks (`without_ollama_login`, `mask_ollama_host`).
    if url[Position::BeforePath..].contains('@') {
        return None;
    }
    Some(url.as_str().trim_end_matches('/').to_string())
}

/// Go's `net.SplitHostPort` (`net/ipsock.go`), which `envconfig.Host`
/// splits `OLLAMA_HOST` with: the host and the port after the last `:`,
/// a host in `[...]` given without its brackets -- or `None` where Go
/// answers with an error (no port, too many colons, a stray bracket),
/// and Ollama then takes the whole of it as the host.
fn split_host_port(hostport: &str) -> Option<(&str, &str)> {
    let colon = hostport.rfind(':')?;
    let (host, plain_from, bracket_from) = if hostport.starts_with('[') {
        let end = hostport.find(']')?;
        // The `]` must come right before the last `:`.
        if end + 1 != colon {
            return None;
        }
        (&hostport[1..end], 1, end + 1)
    } else {
        let host = &hostport[..colon];
        if host.contains(':') {
            return None;
        }
        (host, 0, 0)
    };
    if hostport[plain_from..].contains('[') || hostport[bracket_from..].contains(']') {
        return None;
    }
    Some((host, &hostport[colon + 1..]))
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
            // r40 R40-2: shapes Ollama reads as a daemon that once became
            // the default one. Port 11434 taken, Ollama's FAQ way:
            // `launchctl setenv OLLAMA_HOST :11500`.
            (":11500", "http://127.0.0.1:11500"),
            ("::1", "http://[::1]:11434"),
            ("\"127.0.0.1:11500\"", "http://127.0.0.1:11500"),
            ("'localhost:11500'", "http://localhost:11500"),
            // Ollama's own special case; then refused, as every https one
            // is (`https_refused`, adapters/ollama).
            ("ollama.com", "https://ollama.com"),
            // A login stays, Banager's own; a bare one still gets 11434.
            ("alice:pw@server", "http://alice:pw@server:11434"),
            (
                "http://alice:s%40cret@server:11434",
                "http://alice:s%40cret@server:11434",
            ),
            ("http://[::1]:11500/", "http://[::1]:11500"),
        ] {
            assert_eq!(
                normalize_ollama_host(raw).as_deref(),
                Some(expected),
                "{raw}"
            );
        }
        // Still nothing a url can say: Ollama's default.
        for raw in [
            "",
            "  ",
            "ftp://example.com",
            "file:///tmp/x",
            "exa mple.com",
        ] {
            assert_eq!(normalize_ollama_host(raw), None, "{raw:?}");
        }
    }

    /// Ollama's own table for `envconfig.Host` -- `TestHost`,
    /// `envconfig/config_test.go:20-42` at v0.40.0, read from Ollama's
    /// source outside this repository -- value and expected url as it
    /// writes them. Banager writes the same daemon its own way: the url
    /// crate leaves out a scheme's own port (`:80`, `:443`) and a bare `/`,
    /// a port with no host is `127.0.0.1` (Go's dialer reads an empty host
    /// as this Mac), and Ollama's default is `None`.
    const OLLAMA_TEST_HOST: &[(&str, &str, &str)] = &[
        ("empty", "", "http://127.0.0.1:11434"),
        ("only address", "1.2.3.4", "http://1.2.3.4:11434"),
        ("only port", ":1234", "http://:1234"),
        ("address and port", "1.2.3.4:1234", "http://1.2.3.4:1234"),
        ("hostname", "example.com", "http://example.com:11434"),
        (
            "hostname and port",
            "example.com:1234",
            "http://example.com:1234",
        ),
        ("zero port", ":0", "http://:0"),
        ("too large port", ":66000", "http://:11434"),
        ("too small port", ":-1", "http://:11434"),
        ("ipv6 localhost", "[::1]", "http://[::1]:11434"),
        ("ipv6 world open", "[::]", "http://[::]:11434"),
        ("ipv6 no brackets", "::1", "http://[::1]:11434"),
        ("ipv6 + port", "[::1]:1337", "http://[::1]:1337"),
        ("extra space", " 1.2.3.4 ", "http://1.2.3.4:11434"),
        ("extra quotes", "\"1.2.3.4\"", "http://1.2.3.4:11434"),
        (
            "extra space+quotes",
            " \" 1.2.3.4 \" ",
            "http://1.2.3.4:11434",
        ),
        ("extra single quotes", "'1.2.3.4'", "http://1.2.3.4:11434"),
        ("http", "http://1.2.3.4", "http://1.2.3.4:80"),
        ("http port", "http://1.2.3.4:4321", "http://1.2.3.4:4321"),
        ("https", "https://1.2.3.4", "https://1.2.3.4:443"),
        ("https port", "https://1.2.3.4:4321", "https://1.2.3.4:4321"),
        (
            "proxy path",
            "https://example.com/ollama",
            "https://example.com:443/ollama",
        ),
        ("ollama.com", "ollama.com", "https://ollama.com:443"),
    ];

    #[test]
    fn test_ollama_host_is_read_as_ollamas_own_envconfig_reads_it() {
        for (case, raw, ollama) in OLLAMA_TEST_HOST {
            let same_daemon = Url::parse(&ollama.replacen("://:", "://127.0.0.1:", 1))
                .unwrap()
                .as_str()
                .trim_end_matches('/')
                .to_string();
            let banager = normalize_ollama_host(raw)
                .unwrap_or_else(|| crate::adapters::ollama::DEFAULT_HOST.to_string());
            assert_eq!(
                banager, same_daemon,
                "Ollama's {case:?}: {raw:?} is {ollama}"
            );
        }
    }

    #[test]
    fn test_split_host_port_answers_as_gos_does() {
        // Go's own table, `TestSplitHostPort` in `src/net/ip_test.go:562-631`
        // at go1.25.0, read outside this repository: what splits, and
        // (`None`) what Go answers with an error.
        for (hostport, host, port) in [
            ("localhost:http", "localhost", "http"),
            ("localhost:80", "localhost", "80"),
            ("localhost%lo0:http", "localhost%lo0", "http"),
            ("localhost%lo0:80", "localhost%lo0", "80"),
            ("[localhost%lo0]:http", "localhost%lo0", "http"),
            ("[localhost%lo0]:80", "localhost%lo0", "80"),
            ("127.0.0.1:http", "127.0.0.1", "http"),
            ("127.0.0.1:80", "127.0.0.1", "80"),
            ("[::1]:http", "::1", "http"),
            ("[::1]:80", "::1", "80"),
            ("[::1%lo0]:http", "::1%lo0", "http"),
            ("[::1%lo0]:80", "::1%lo0", "80"),
            (":http", "", "http"),
            (":80", "", "80"),
            ("golang.org:", "golang.org", ""),
            ("127.0.0.1:", "127.0.0.1", ""),
            ("[::1]:", "::1", ""),
            ("golang.org:https%foo", "golang.org", "https%foo"),
        ] {
            assert_eq!(
                split_host_port(hostport),
                Some((host, port)),
                "{hostport:?}"
            );
        }
        for hostport in [
            "golang.org",
            "127.0.0.1",
            "[::1]",
            "[fe80::1%lo0]",
            "[localhost%lo0]",
            "localhost%lo0",
            "::1",
            "fe80::1%lo0",
            "fe80::1%lo0:80",
            "[foo:bar]",
            "[foo:bar]baz",
            "[foo]bar:baz",
            "[foo]:[bar]:baz",
            "[foo]:[bar]baz",
            "foo[bar]:baz",
            "foo]bar:baz",
            // Not in Go's table: an empty one, which has no colon either.
            "",
        ] {
            assert_eq!(split_host_port(hostport), None, "{hostport:?}");
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
        // environment (only PATH and the proxy and mirror settings are
        // taken from the login shell, and kept apart from it:
        // `login_path::accept`), `None` when unset. Interpreting them -- empty means
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
        // Not this one any more (r40 R40-2): Ollama reads a scheme with no
        // host as 127.0.0.1 on the scheme's port, and so does Banager, so
        // that it asks the daemon Ollama's own commands would.
        assert_eq!(
            normalize_ollama_host("http://").as_deref(),
            Some("http://127.0.0.1")
        );
        // Not http(s): the adapter can only speak to an http endpoint, and
        // silently prefixing `file://` or `ftp://` onto `/api/tags` would
        // produce something stranger than the default.
        assert_eq!(normalize_ollama_host("ftp://10.0.0.5:11434"), None);
        assert_eq!(normalize_ollama_host("file:///etc/passwd"), None);
    }
}

#[cfg(test)]
mod login_host_tests {
    use super::normalize_ollama_host;

    #[test]
    fn schemeless_login_colons_are_not_ports() {
        for (raw, expected) in [
            ("alice:secret@server", "http://alice:secret@server:11434"),
            ("alice:s%40cret@server:80", "http://alice:s%40cret@server"),
            ("alice:secret@[::1]", "http://alice:secret@[::1]:11434"),
            ("alice:secret@[::1]:1234", "http://alice:secret@[::1]:1234"),
            ("alice@server", "http://alice@server:11434"),
            (":secret@server", "http://:secret@server:11434"),
            ("http://alice:secret@server", "http://alice:secret@server"),
            ("https://alice:secret@server", "https://alice:secret@server"),
        ] {
            assert_eq!(
                normalize_ollama_host(raw).as_deref(),
                Some(expected),
                "{raw}"
            );
        }
    }

    /// A password with a raw `/`, `?`, `#` or `\` in it: Ollama's reading
    /// (`envconfig.Host` cuts the path off at the first `/`) and the url
    /// crate's (a host ends at any of the four) both take the start of the
    /// login for the host, and put the rest of the password, with the real
    /// host, in the path, query or fragment. Kept, that rest reached the
    /// window's id and the preview unmasked -- both masks stop at the first
    /// `/` (`without_ollama_login`, `mask_ollama_host`) -- and the daemon
    /// was asked at a host named `alice`. Such a value is `None`, Ollama's
    /// default, as the first two were before r40 R40-2.
    #[test]
    fn a_login_cut_short_by_a_raw_slash_or_query_names_no_host() {
        for raw in [
            "http://alice:se/cret@nas.local:11434",
            "alice:se/cret@nas.local",
            "http://alice:12/cret@nas.local",
            "alice:12/cret@nas.local:11434",
            "https://alice:se/cret@nas.local/ollama",
            "http://alice:12?cret@nas.local",
            "http://alice:12#cret@nas.local",
            "http://alice:12\\cret@nas.local",
        ] {
            assert_eq!(normalize_ollama_host(raw), None, "{raw}");
        }
        // Percent-encoded, as a url writes it, the same login is one.
        assert_eq!(
            normalize_ollama_host("http://alice:se%2Fcret@nas.local:11434").as_deref(),
            Some("http://alice:se%2Fcret@nas.local:11434")
        );
    }

    /// Whatever `normalize_ollama_host` keeps, the window's id and the
    /// preview hold no part of its login: every `@` in it stands in front
    /// of its host, where both masks look.
    #[test]
    fn no_part_of_a_kept_login_reaches_an_id_or_a_preview() {
        use crate::runner::redact::{mask_ollama_host, without_ollama_login};
        for raw in [
            "alice:secret@server",
            "http://alice:secret@server:11434",
            "http://alice:s%40cret@server",
            "http://alice:se%2Fcret@nas.local:11434/ollama",
            "alice:p@ss@cret@server",
            "http://alice:se/cret@nas.local:11434",
            "alice:se/cret@nas.local",
            "alice:12/cret@nas.local",
            "http://alice:12?cret@nas.local",
            "http://alice:12#cret@nas.local",
            "http://alice:12\\cret@nas.local",
            "http://nas.local:11434/alice@cret",
        ] {
            let Some(host) = normalize_ollama_host(raw) else {
                continue;
            };
            let id = format!("ollama:{host}");
            for shown in [
                without_ollama_login(&id).into_owned(),
                mask_ollama_host(&host).into_owned(),
            ] {
                assert!(
                    !shown.contains("cret") && !shown.contains("alice"),
                    "{raw:?} is kept as {host:?} and shown as {shown:?}"
                );
            }
        }
    }
}
