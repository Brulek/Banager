use std::path::PathBuf;
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
    /// `OLLAMA_HOST` when the host environment sets it, normalised by
    /// `normalize_ollama_host` into an absolute http(s) url with no trailing
    /// slash; `None` means Ollama's own default, `http://127.0.0.1:11434`,
    /// and is also what an unusable value becomes. Same reasoning as
    /// `cargo_home`; consumed by Task 10.
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
    let url = Url::parse(&candidate).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    // A url with no authority (`http://`) would compose to nonsense.
    url.host_str().filter(|h| !h.is_empty())?;
    Some(url.as_str().trim_end_matches('/').to_string())
}

impl HostEnv {
    /// Reads `PATH`/`HOME` from the process environment. Call this only
    /// after `fix_path_env::fix()` has already run (in the Tauri shell's
    /// `run()`), since apps launched from Finder start with a minimal
    /// default `PATH` that doesn't include Homebrew's `bin` directories.
    pub fn discover() -> HostEnv {
        let path_dirs = std::env::var_os("PATH")
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_default();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let euid = unsafe { libc::geteuid() };
        let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
        let ollama_host = std::env::var("OLLAMA_HOST")
            .ok()
            .as_deref()
            .and_then(normalize_ollama_host);
        HostEnv {
            path_dirs,
            home,
            euid,
            cargo_home,
            ollama_host,
        }
    }
}

pub fn resolve_exe(name: &str, env: &HostEnv) -> Option<PathBuf> {
    for dir in &env.path_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ollama_host: None,
        };
        assert_eq!(resolve_exe("sh", &env), Some(PathBuf::from("/bin/sh")));
    }

    #[test]
    fn test_resolve_exe_returns_none_for_a_missing_binary() {
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/bin")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert_eq!(resolve_exe("definitely-not-a-real-binary-xyz", &env), None);
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
