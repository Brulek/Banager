//! The unknown-source scan over directory trees each test builds itself
//! in a temp directory. No recorded fixture: `scan` is not an adapter
//! and has no fixture directory (`fixtures_layout_test` requires the
//! fixture set to equal the registered adapter ids), and every shape a
//! scan has to handle -- a broken link, a two-hop link, a subdirectory, a
//! file with no execute bit, 2001 files -- is something a test can make
//! in a millisecond and the research machine could not (its seven
//! directories held 26 entries; §8.4 says the budget is covered
//! synthetically, not by recording).
//!
//! Every name here is invented. The research file with the real ones is
//! deliberately not in the repository (phase 4 spec §十三 #30).

use canager_core::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance,
};
use canager_core::runner::HostEnv;
use canager_core::scan::{scan_dirs, EntryKind, ScanBudget, ScanStop, ScannedDir};
use canager_core::testing::manager_instance;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A fresh home directory for one test, removed when the test ends.
/// Canonical (`fs::canonicalize`) so the paths a test writes compare
/// equal to the ones the scan canonicalises: on macOS `temp_dir()` is
/// `/var/folders/…`, and `/var` is a symlink to `/private/var`.
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "canager-scan-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&raw).expect("create temp home");
        Home(fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// A directory under this home, created.
    fn dir(&self, rel: &str) -> PathBuf {
        let dir = self.0.join(rel);
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    /// `HostEnv` for this home: the scan's `euid` is the owner of the home
    /// itself, which is the user running the test.
    fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
        HostEnv {
            path_dirs,
            home: self.0.clone(),
            euid: fs::metadata(&self.0).expect("stat home").uid(),
            cargo_home: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// An executable regular file holding `bytes`.
fn exe(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// A regular file with no execute bit at all.
fn plain(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"text").expect("write file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
    path
}

/// A symlink `dir/name` whose text is exactly `target`.
fn link(dir: &Path, name: &str, target: &Path) -> PathBuf {
    let path = dir.join(name);
    symlink(target, &path).expect("symlink");
    path
}

fn artifact(instance_id: &str, name: &str, path: &Path) -> InstalledArtifact {
    InstalledArtifact {
        key: ArtifactKey {
            instance_id: instance_id.to_string(),
            kind: ArtifactKind::Tool,
            name: name.to_string(),
        },
        display_name: name.to_string(),
        version: "1.0".to_string(),
        reason: InstallReason::Requested,
        description: None,
        homepage: None,
        size_bytes: None,
        installed_at: None,
        path: Some(path.to_path_buf()),
        auto_updates: false,
        uninstall_blocked: None,
    }
}

fn tilde(rel: &str) -> PathBuf {
    PathBuf::from("~").join(rel)
}

#[test]
fn test_lists_an_executable_nobody_claims_with_kind_size_date_and_home_abbreviated() {
    let home = Home::new("plain-exe");
    let bin = home.dir(".local/bin");
    let tool = exe(&bin, "standalone-tool", b"#!/bin/sh\necho hi\n");

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(
        scan.scanned,
        vec![ScannedDir {
            path: tilde(".local/bin"),
            entries: 1
        }]
    );
    assert_eq!(scan.attributed, 0);
    assert_eq!(scan.stopped, None);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.path, tilde(".local/bin/standalone-tool"));
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(entry.resolved.as_deref(), Some(tool.as_path()));
    assert_eq!(entry.link_target, None);
    assert_eq!(entry.size_bytes, Some(18));
    assert!(entry.modified_at.is_some_and(|t| t > 0), "{entry:?}");
    assert!(entry.owned_by_me);
    assert_eq!(entry.app_bundle, None);
}

#[test]
fn test_a_broken_symlink_is_listed_with_its_link_text_no_size_and_the_app_it_named() {
    let home = Home::new("broken");
    let bin = home.dir(".local/bin");
    let target = Path::new("/Applications/Removed.app/Contents/Resources/scripts/index.js");
    link(&bin, "old-script", target);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.path, tilde(".local/bin/old-script"));
    assert_eq!(entry.kind, EntryKind::BrokenSymlink);
    assert_eq!(entry.resolved, None);
    assert_eq!(entry.link_target.as_deref(), Some(target.to_str().unwrap()));
    // Both describe the target, and a broken link has none.
    assert_eq!(entry.size_bytes, None);
    assert_eq!(entry.modified_at, None);
    assert_eq!(entry.app_bundle.as_deref(), Some("Removed"));
}

#[test]
fn test_a_two_hop_symlink_resolves_to_its_final_target() {
    // `python3.12 -> …/cpython-3.12-…/bin/python3.12`, where the
    // versionless directory is itself a link to the patch-versioned one:
    // one `readlink` is not enough, `canonicalize` is.
    let home = Home::new("two-hop");
    let bin = home.dir(".local/bin");
    let real_dir = home.dir(".local/share/runtime/versions/3.12.14/bin");
    let real = exe(&real_dir, "python3", b"binary");
    let versions = home.path().join(".local/share/runtime/versions");
    link(&versions, "3.12", Path::new("3.12.14"));
    let hop = versions.join("3.12/bin/python3");
    link(&bin, "python3", &hop);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.kind, EntryKind::Symlink);
    assert_eq!(entry.resolved.as_deref(), Some(real.as_path()));
    assert_eq!(entry.link_target.as_deref(), Some(hop.to_str().unwrap()));
    assert_eq!(entry.size_bytes, Some(6));
}

#[test]
fn test_skips_subdirectories_files_without_an_execute_bit_and_links_to_directories() {
    let home = Home::new("skips");
    let bin = home.dir(".local/bin");
    home.dir(".local/bin/store");
    plain(&bin, "notes.txt");
    link(&bin, "data", Path::new("store"));

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert!(
        scan.entries.is_empty(),
        "nothing here is a program: {:?}",
        scan.entries
    );
    // All three were examined -- they count against the budget and in the
    // footer -- they are just not listed.
    assert_eq!(scan.scanned[0].entries, 3);
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_a_directory_that_does_not_exist_is_skipped_and_not_reported() {
    let home = Home::new("missing-dir");
    let bin = home.dir(".local/bin");
    exe(&bin, "tool", b"x");
    let missing = home.path().join("bin");

    let scan = scan_dirs(
        &[missing, bin],
        &home.env(vec![]),
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(
        scan.scanned,
        vec![ScannedDir {
            path: tilde(".local/bin"),
            entries: 1
        }]
    );
}

#[test]
fn test_two_paths_to_the_same_directory_are_read_once() {
    // A PATH that names `~/.local/bin` through a link as well as directly.
    let home = Home::new("dedupe");
    let bin = home.dir(".local/bin");
    exe(&bin, "tool", b"x");
    let alias = link(home.path(), "linkbin", Path::new(".local/bin"));

    let scan = scan_dirs(
        &[bin, alias],
        &home.env(vec![]),
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.scanned.len(), 1, "{:?}", scan.scanned);
    assert_eq!(scan.scanned[0].path, tilde(".local/bin"));
    assert_eq!(scan.entries.len(), 1);
}

#[test]
fn test_stops_at_the_file_limit_and_says_which_limit() {
    let home = Home::new("file-limit");
    let bin = home.dir("bin");
    for i in 0..2000 {
        exe(&bin, &format!("t{i:04}"), b"x");
    }

    let full = scan_dirs(
        std::slice::from_ref(&bin),
        &home.env(vec![]),
        &[],
        &[],
        ScanBudget::default(),
    );
    assert_eq!(full.stopped, None, "exactly the budget is not over it");
    assert_eq!(full.scanned[0].entries, 2000);
    assert_eq!(full.entries.len(), 2000);

    exe(&bin, "t2000", b"x");
    let over = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());
    assert_eq!(
        over.stopped,
        Some(ScanStop::FileLimit { max_entries: 2000 })
    );
    // What was examined before the stop is still reported.
    assert_eq!(
        over.scanned,
        vec![ScannedDir {
            path: tilde("bin"),
            entries: 2000
        }]
    );
    assert_eq!(over.entries.len(), 2000);
}

#[test]
fn test_stops_at_a_zero_time_budget_before_reading_anything() {
    let home = Home::new("time-limit");
    let bin = home.dir(".local/bin");
    exe(&bin, "tool", b"x");
    let budget = ScanBudget {
        max_entries: 2000,
        max_duration: Duration::ZERO,
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], budget);

    assert_eq!(scan.stopped, Some(ScanStop::TimeLimit { max_secs: 0 }));
    assert!(scan.scanned.is_empty(), "{:?}", scan.scanned);
    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_a_path_component_ending_in_dot_app_names_the_bundle() {
    let home = Home::new("app-bundle");
    let bin = home.dir(".local/bin");
    let helpers = home.dir("Applications/Helper.app/Contents/Helpers");
    let real = exe(&helpers, "helper-cli", b"x");
    link(&bin, "helper-cli", &real);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].kind, EntryKind::Symlink);
    assert_eq!(scan.entries[0].app_bundle.as_deref(), Some("Helper"));
}

#[test]
fn test_rule_0_claims_an_instances_launcher_by_its_raw_path_even_when_dangling() {
    // The half-uninstalled state step B calls `LauncherOnly`: the program
    // directory is gone, the launcher link is still there. `canonicalize`
    // fails on it, so rules 1-3 cannot see it; without rule 0 it would be
    // a "broken link" row here and a source on the Installed page at once.
    let home = Home::new("rule-0-dangling");
    let bin = home.dir(".local/bin");
    let launcher = link(
        &bin,
        "claude",
        &home.path().join(".local/share/claude/versions/2.1.281"),
    );
    let instance = ManagerInstance {
        exe_path: launcher,
        prefix: home.path().join(".local/share/claude"),
        ..manager_instance("standalone-claude", "standalone-claude")
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[instance],
        &[],
        ScanBudget::default(),
    );

    assert!(
        scan.entries.is_empty(),
        "the dangling launcher is the source's, not unknown: {:?}",
        scan.entries
    );
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_0_claims_an_instances_launcher_and_nothing_else_in_its_directory() {
    // The research machine's `~/.local/bin/python3.12`: a link into uv's
    // Python that the pip adapter detects as an interpreter, giving a pip
    // instance whose `exe_path` is that link and whose `prefix` is
    // `~/.local/bin` itself (pip.rs:125-128 takes `exe_path.parent()`).
    // Rule 0 claims the interpreter -- it really is a listed source's
    // executable. Nothing claims `agy` beside it: the prefix is not an
    // owned root (Task 3 makes that explicit; this test must keep
    // passing once rule 3 exists).
    let home = Home::new("rule-0-raw");
    let bin = home.dir(".local/bin");
    let python_dir = home.dir(".local/share/uv/python/cpython-3.12.14/bin");
    let python = exe(&python_dir, "python3.12", b"x");
    let interpreter = link(&bin, "python3.12", &python);
    exe(&bin, "agy", b"x");
    let pip = ManagerInstance {
        exe_path: interpreter.clone(),
        prefix: bin.clone(),
        ..manager_instance("pip", &format!("pip:{}", interpreter.display()))
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[pip],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 1);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/agy"));
}

#[test]
fn test_rule_1_claims_everything_that_resolves_to_an_instances_launcher() {
    // `~/.cargo/bin`: rustup itself, thirteen proxies that are relative
    // symlinks to it, and one crate installed with `cargo install`. The
    // cargo instance's own executable is one of the proxies, so
    // everything that resolves to `rustup` is cargo's. `hexyl` is not --
    // until step E fills `InstalledArtifact.path` for cargo binaries it
    // is listed here, honestly (spec §8.3; Task 10's delivery note).
    let home = Home::new("rule-1");
    let bin = home.dir(".cargo/bin");
    exe(&bin, "rustup", b"x");
    let proxies = [
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
    for proxy in proxies {
        link(&bin, proxy, Path::new("rustup"));
    }
    exe(&bin, "hexyl", b"x");
    let cargo = ManagerInstance {
        exe_path: bin.join("cargo"),
        prefix: home.path().join(".cargo"),
        ..manager_instance(
            "cargo",
            &format!("cargo:{}", home.path().join(".cargo").display()),
        )
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[cargo],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 14, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".cargo/bin/hexyl"));
}

#[test]
fn test_rule_2_claims_a_shim_that_resolves_under_an_artifacts_path() {
    // uv fills `InstalledArtifact.path` with the tool's venv directory
    // (uv.rs:65); its shim in `~/.local/bin` resolves to a file *under*
    // that directory, never to it -- so the rule is "starts with", not
    // "equals" (§十三 #35).
    let home = Home::new("rule-2");
    let bin = home.dir(".local/bin");
    let venv = home.path().join(".local/share/uv/tools/ruff");
    let venv_bin = home.dir(".local/share/uv/tools/ruff/bin");
    let real = exe(&venv_bin, "ruff", b"x");
    link(&bin, "ruff", &real);
    let uv = ManagerInstance {
        exe_path: home.path().join("elsewhere/uv"),
        prefix: bin.clone(),
        ..manager_instance("uv", "uv")
    };
    let ruff = artifact("uv", "ruff", &venv);

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[uv],
        &[ruff],
        ScanBudget::default(),
    );

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}
