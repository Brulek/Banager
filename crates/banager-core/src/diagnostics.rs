//! What the window cannot find out by itself for 「拷贝诊断信息」 ("Copy
//! Diagnostic Info"): which macOS this is, on which chip, whether `PATH`
//! is the login shell's, the `PATH` folders, and where each source's
//! program is -- every path with the home folder written as `~`.
//!
//! Read-only and quick: two `sysctlbyname` reads of the kernel's own
//! strings (`kern.osproductversion`, `machdep.cpu.brand_string`), the
//! process's `PATH` and `HOME`, and the snapshot's instances. No command
//! runs, no file is opened, nothing is written, and no other environment
//! variable is read: the text the window builds from this is meant to be
//! pasted to someone else, and a proxy setting can hold a password.
//! The window writes the text (src/lib/diagnostics.ts).

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::model::ManagerInstance;

/// What `get_system_facts` hands the window. Mirrored by `SystemFacts` in
/// src/lib/types.ts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemFacts {
    /// macOS's version as Settings' About shows it ("27.0"); `None` where
    /// the kernel would not say, or off a Mac.
    pub macos_version: Option<String>,
    /// The chip as the kernel names it ("Apple M2 Pro"); `None` where it
    /// would not say.
    pub chip: Option<String>,
    /// What Banager itself was built for: "aarch64" or "x86_64"
    /// (`std::env::consts::ARCH`). The window names the chip by it when
    /// `chip` is `None`.
    pub arch: String,
    /// Whether `PATH` is the one the login shell exports
    /// (`Session::login_path_restored`).
    pub login_path: bool,
    /// The folders on `PATH`, in order, home folder as `~`; empty entries
    /// left out.
    pub path_dirs: Vec<String>,
    /// Each source's program, home folder as `~`, by instance id, in the
    /// snapshot's order.
    pub sources: Vec<SourcePath>,
}

/// One source's program, as `SystemFacts::sources` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePath {
    pub instance_id: String,
    pub exe_path: String,
}

/// macOS's version and the chip's name, as the kernel reports them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OsFacts {
    pub macos_version: Option<String>,
    pub chip: Option<String>,
}

/// `SystemFacts` from what it is made of. Pure: `current` hands in the
/// process's own `PATH` and `HOME`, a test whatever it likes.
pub fn system_facts(
    os: OsFacts,
    path: Option<&OsStr>,
    home: Option<&Path>,
    login_path: bool,
    instances: &[ManagerInstance],
) -> SystemFacts {
    let shown = |p: &Path| shown_path(p, home);
    let path_dirs = path
        .map(|value| {
            std::env::split_paths(value)
                .filter(|dir| !dir.as_os_str().is_empty())
                .map(|dir| shown(&dir))
                .collect()
        })
        .unwrap_or_default();
    let sources = instances
        .iter()
        .map(|instance| SourcePath {
            instance_id: instance.id.clone(),
            exe_path: shown(&instance.exe_path),
        })
        .collect();
    SystemFacts {
        macos_version: os.macos_version,
        chip: os.chip,
        arch: std::env::consts::ARCH.to_string(),
        login_path,
        path_dirs,
        sources,
    }
}

/// `SystemFacts` for this process: the kernel's two strings, its own
/// `PATH` and `HOME` -- no other variable -- and `instances`.
pub fn current(login_path: bool, instances: &[ManagerInstance]) -> SystemFacts {
    let path = std::env::var_os("PATH");
    let home = std::env::var_os("HOME").map(PathBuf::from);
    system_facts(
        read_os(),
        path.as_deref(),
        home.as_deref(),
        login_path,
        instances,
    )
}

/// `path` with the home folder written as `~`, as Finder and Terminal show
/// one. A home that is missing, relative or the root folder abbreviates
/// nothing: every path would start with `/`.
fn shown_path(path: &Path, home: Option<&Path>) -> String {
    let home = home.filter(|home| home.is_absolute() && home.parent().is_some());
    match home.map(|home| path.strip_prefix(home)) {
        Some(Ok(rest)) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(Ok(rest)) => Path::new("~").join(rest).to_string_lossy().into_owned(),
        _ => path.to_string_lossy().into_owned(),
    }
}

/// macOS's version and the chip's name, from the kernel
/// (`sysctlbyname`). Runs nothing and opens nothing.
#[cfg(target_os = "macos")]
pub fn read_os() -> OsFacts {
    OsFacts {
        macos_version: sysctl_string("kern.osproductversion"),
        chip: sysctl_string("machdep.cpu.brand_string"),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_os() -> OsFacts {
    OsFacts::default()
}

/// One string the kernel keeps under `name`, trimmed; `None` when it has
/// none or it is empty.
#[cfg(target_os = "macos")]
fn sysctl_string(name: &str) -> Option<String> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut len: libc::size_t = 0;
    // SAFETY: a null buffer asks only for the length, written to `len`.
    let asked = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if asked != 0 || len == 0 || len > 4096 {
        return None;
    }
    let mut buffer = vec![0u8; len];
    // SAFETY: `buffer` holds `len` bytes, and the kernel writes at most
    // `len` and says how many in `len`.
    let read = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if read != 0 {
        return None;
    }
    buffer.truncate(len);
    let text = String::from_utf8_lossy(&buffer);
    let text = text.trim_end_matches('\0').trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InstanceStatus, Scope};

    fn instance(id: &str, exe: &str) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: id.split(':').next().unwrap().to_string(),
            exe_path: PathBuf::from(exe),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: None,
            unverified_version: None,
            read_only_reason: None,
            status: InstanceStatus::default(),
        }
    }

    fn os() -> OsFacts {
        OsFacts {
            macos_version: Some("27.0".to_string()),
            chip: Some("Apple M2 Pro".to_string()),
        }
    }

    #[test]
    fn test_paths_under_home_are_written_with_a_tilde_and_others_as_they_are() {
        let facts = system_facts(
            os(),
            Some(OsStr::new(
                "/opt/homebrew/bin:/Users/alice/.local/bin::/Users/alice:/usr/bin:/Users/alicebob/bin",
            )),
            Some(Path::new("/Users/alice")),
            true,
            &[
                instance("brew:/opt/homebrew", "/opt/homebrew/bin/brew"),
                instance(
                    "standalone-claude-code:/Users/alice/.local/bin/claude",
                    "/Users/alice/.local/bin/claude",
                ),
            ],
        );
        assert_eq!(
            facts.path_dirs,
            [
                "/opt/homebrew/bin",
                "~/.local/bin",
                "~",
                "/usr/bin",
                // Another folder whose name only starts with the home's.
                "/Users/alicebob/bin",
            ]
        );
        assert_eq!(
            facts.sources,
            [
                SourcePath {
                    instance_id: "brew:/opt/homebrew".to_string(),
                    exe_path: "/opt/homebrew/bin/brew".to_string(),
                },
                SourcePath {
                    instance_id: "standalone-claude-code:/Users/alice/.local/bin/claude"
                        .to_string(),
                    exe_path: "~/.local/bin/claude".to_string(),
                },
            ]
        );
        assert!(facts.login_path);
        assert_eq!(facts.macos_version.as_deref(), Some("27.0"));
        assert_eq!(facts.chip.as_deref(), Some("Apple M2 Pro"));
        assert_eq!(facts.arch, std::env::consts::ARCH);
    }

    #[test]
    fn test_a_root_or_missing_home_abbreviates_nothing() {
        for home in [None, Some(Path::new("/")), Some(Path::new("relative"))] {
            let facts = system_facts(
                OsFacts::default(),
                Some(OsStr::new("/usr/bin:/bin")),
                home,
                false,
                &[],
            );
            assert_eq!(facts.path_dirs, ["/usr/bin", "/bin"], "home {home:?}");
            assert!(!facts.login_path);
        }
    }

    #[test]
    fn test_no_path_at_all_lists_no_folder() {
        let facts = system_facts(OsFacts::default(), None, None, true, &[]);
        assert!(facts.path_dirs.is_empty());
        assert!(facts.sources.is_empty());
    }

    #[test]
    fn test_the_wire_format_is_the_one_src_lib_types_ts_mirrors() {
        let facts = system_facts(
            os(),
            Some(OsStr::new("/usr/bin")),
            Some(Path::new("/Users/alice")),
            true,
            &[instance("npm:/opt/homebrew", "/opt/homebrew/bin/npm")],
        );
        let json = serde_json::to_string(&facts).unwrap();
        assert_eq!(
            json,
            format!(
                concat!(
                    r#"{{"macos_version":"27.0","chip":"Apple M2 Pro","arch":"{}","#,
                    r#""login_path":true,"path_dirs":["/usr/bin"],"#,
                    r#""sources":[{{"instance_id":"npm:/opt/homebrew","exe_path":"/opt/homebrew/bin/npm"}}]}}"#
                ),
                std::env::consts::ARCH
            )
        );
        let back: SystemFacts = serde_json::from_str(&json).unwrap();
        assert_eq!(back, facts);
        let empty = serde_json::to_string(&SystemFacts::default()).unwrap();
        assert_eq!(
            empty,
            r#"{"macos_version":null,"chip":null,"arch":"","login_path":false,"path_dirs":[],"sources":[]}"#
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_the_kernel_names_this_macs_version_and_chip() {
        // Two read-only sysctl reads; nothing runs.
        let os = read_os();
        let version = os.macos_version.expect("kern.osproductversion");
        assert!(
            version.split('.').all(|part| part.parse::<u32>().is_ok()),
            "{version:?}"
        );
        assert!(os.chip.is_some());
        assert_eq!(sysctl_string("no.such.name"), None);
    }

    #[test]
    fn test_current_reads_no_variable_but_path_and_home() {
        // The process's own; what matters is that nothing else reaches the
        // facts: no field could hold another variable's value.
        let facts = current(true, &[]);
        let json = serde_json::to_value(&facts).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            // serde_json's map, in key order.
            [
                "arch",
                "chip",
                "login_path",
                "macos_version",
                "path_dirs",
                "sources"
            ]
        );
        if let Some(home) = std::env::var_os("HOME").filter(|home| home.len() > 1) {
            let home = home.to_string_lossy().into_owned();
            for dir in &facts.path_dirs {
                assert!(
                    !dir.starts_with(&format!("{home}/")) && dir != &home,
                    "{dir:?} still names the home folder"
                );
            }
        }
    }
}
