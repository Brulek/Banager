use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEnv {
    pub path_dirs: Vec<PathBuf>,
    pub home: PathBuf,
    pub euid: u32,
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
        HostEnv {
            path_dirs,
            home,
            euid,
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
        };
        assert_eq!(resolve_exe("sh", &env), Some(PathBuf::from("/bin/sh")));
    }

    #[test]
    fn test_resolve_exe_returns_none_for_a_missing_binary() {
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/bin")],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        assert_eq!(resolve_exe("definitely-not-a-real-binary-xyz", &env), None);
    }
}
