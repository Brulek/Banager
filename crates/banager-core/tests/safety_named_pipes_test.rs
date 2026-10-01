//! Promise 6 of `docs/what-we-run.md`: a refresh -- and the walks that
//! follow one -- never waits on a named pipe. A FIFO that nothing writes
//! to blocks a plain `open` for reading forever, so wherever a walk meets
//! names, one of them is made a FIFO here, and each walk must finish
//! within a few seconds all the same (`finishes`): the PATH folders read
//! for which copy runs, the Other Programs scan, finding a package
//! manager, a standalone tool's PATH note, what an uninstall leaves, disk
//! use, and Codex's version marker.
//!
//! A dead network disk cannot be made in a test; the walks never step
//! onto `/Volumes` at all (`safety_protected_places_test`), which is what
//! keeps one from stalling them.

use banager_core::adapters::standalone::recipe::VersionSource;
use banager_core::adapters::standalone::recipes::CODEX;
use banager_core::adapters::standalone::release_link;
use banager_core::adapters::standalone::route::shadow_note;
use banager_core::commands::{read_folders, CommandBudget};
use banager_core::kept_data::kept_data;
use banager_core::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance,
};
use banager_core::runner::{resolve_exe, HostEnv};
use banager_core::scan::{scan_dirs, ScanBudget};
use banager_core::size::{SizeBudget, SizeMeter};
use banager_core::testing::manager_instance;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-x3-pipes-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&raw).unwrap();
        Home(fs::canonicalize(&raw).unwrap())
    }

    /// A named pipe at `rel`, which nothing ever writes to.
    fn fifo(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `c` is a valid NUL-terminated path for the call.
        let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o700) };
        assert_eq!(rc, 0, "mkfifo {}", path.display());
        path
    }

    fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
        HostEnv {
            path_dirs,
            home: self.0.clone(),
            euid: fs::metadata(&self.0).unwrap().uid(),
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `f` finishes within five seconds: a walk that opened the pipe would
/// wait for a writer that never comes, and fail here instead of hanging
/// the suite.
fn finishes<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| panic!("{what} waited on a named pipe"))
}

/// A home whose every place a walk looks holds a pipe: `bin/<TOOL>` and
/// `bin/link` (a link to the pipe) on PATH, `~/.claude` (kept data), an
/// npm package's folder holding a pipe (disk use), and `link-to-pipe` in
/// a scanned folder.
const TOOL: &str = "banager-x3-pipe";

fn piped_home(tag: &str) -> Home {
    let home = Home::new(tag);
    let pipe = home.fifo(&format!("bin/{TOOL}"));
    symlink(&pipe, home.0.join("bin/link-to-pipe")).unwrap();
    home.fifo(".claude/projects/session.jsonl");
    home.fifo(".claude.json");
    home.fifo("npm/lib/node_modules/pkg/index.js");
    home
}

#[test]
fn test_no_walk_waits_on_a_named_pipe_where_it_looks() {
    let home = piped_home("walks");
    let bin = home.0.join("bin");

    let (path, h) = (vec![bin.clone()], home.0.clone());
    let folders = finishes("which copy runs", move || {
        read_folders(&path, &[], &h, CommandBudget::default())
    });
    assert_eq!(folders.path_folders(), vec![bin.as_path()]);

    let env = home.env(vec![bin.clone()]);
    let scan = finishes("the Other Programs scan", move || {
        scan_dirs(&[bin], &env, &[], &[], &[], ScanBudget::default())
    });
    assert_eq!(scan.scanned.len(), 1, "the folder was read: {scan:?}");

    let env = home.env(vec![home.0.join("bin")]);
    let found = finishes("finding a package manager", move || resolve_exe(TOOL, &env));
    assert_eq!(found, None, "a pipe is not a program");

    let env = home.env(vec![home.0.join("bin")]);
    let real = home.0.join("bin").join(TOOL);
    finishes("a standalone tool's PATH note", move || {
        shadow_note(TOOL, &env, &real)
    });

    let h = home.0.clone();
    let kept = finishes("what an uninstall leaves", move || {
        kept_data(&h, "claude-code", &[], SizeBudget::default())
    });
    assert_eq!(kept.len(), 2, "both of Claude Code's paths are named");
}

#[test]
fn test_disk_use_does_not_wait_on_a_named_pipe_in_a_tool() {
    let home = piped_home("sizes");
    let id = "npm:pipes";
    let instance = ManagerInstance {
        prefix: home.0.join("npm"),
        ..manager_instance("npm", id)
    };
    let artifact = InstalledArtifact {
        key: ArtifactKey {
            instance_id: id.to_string(),
            kind: ArtifactKind::Package,
            name: "pkg".to_string(),
        },
        display_name: "pkg".to_string(),
        version: "1.0.0".to_string(),
        reason: InstallReason::Requested,
        description: None,
        homepage: None,
        size_bytes: None,
        installed_at: None,
        path: None,
        auto_updates: false,
        uninstall_blocked: None,
        facts: Default::default(),
    };
    let meter = SizeMeter::new(SizeBudget::default(), |_| {});
    meter.measure(1, &[instance], &[artifact], &home.0);
    let started = Instant::now();
    while !meter.sizes().done {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "disk use waited on a named pipe"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(meter.sizes().artifacts[0].measured.is_some());
}

#[test]
fn test_codexs_version_marker_is_not_waited_on_when_it_is_a_named_pipe() {
    // The marker (`auto-update-version`) is opened without waiting and
    // read only when `fstat` on the opened file says it is a regular file:
    // a pipe there, even one swapped in just before the open, is "does not
    // follow the latest", at once.
    let VersionSource::ReleaseLink(spec) = &CODEX.version else {
        panic!("Codex reads its version from a link");
    };
    let home = Home::new("codex-marker");
    let root = home.0.join(".codex/packages/standalone");
    let release = root.join("releases/0.159.3-aarch64-apple-darwin");
    fs::create_dir_all(&release).unwrap();
    symlink(&release, root.join(spec.link)).unwrap();
    home.fifo(&format!(
        ".codex/packages/standalone/{}",
        spec.follows_latest
    ));
    let reading = finishes("Codex's version marker", move || {
        release_link::read(&root, spec)
    });
    assert_eq!(reading.version.as_deref(), Some("0.159.3"));
    assert!(!reading.follows_latest);
}

#[test]
fn test_a_named_pipe_in_place_of_a_path_folder_is_not_waited_on() {
    // A PATH entry that is itself a pipe, and one leading to a pipe.
    let home = Home::new("pipe-folder");
    let pipe = home.fifo("not-a-folder");
    symlink(&pipe, home.0.join("link-to-pipe")).unwrap();
    let path = vec![pipe.clone(), home.0.join("link-to-pipe")];
    let (p, h) = (path.clone(), home.0.clone());
    let folders = finishes("which copy runs", move || {
        read_folders(&p, &[], &h, CommandBudget::default())
    });
    assert!(folders.path_folders().is_empty());
    let env = home.env(vec![]);
    let scan = finishes("the Other Programs scan", move || {
        scan_dirs(&path, &env, &[], &[], &[], ScanBudget::default())
    });
    assert!(scan.scanned.is_empty());
}
