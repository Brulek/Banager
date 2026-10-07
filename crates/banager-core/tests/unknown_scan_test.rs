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

use banager_core::adapters::brew::parse::parse_info_installed;
use banager_core::model::{
    ArtifactKey, ArtifactKind, InstalledArtifact, ManagerInstance, RemovedWhat,
};
use banager_core::runner::HostEnv;
use banager_core::scan::{scan_dirs, EntryKind, Glob, ScanBudget, ScanStop, ScannedDir};
use banager_core::testing::manager_instance;
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
            "banager-scan-{}-{}-{}",
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
        path: Some(path.to_path_buf()),
        ..banager_core::testing::installed_artifact(instance_id, ArtifactKind::Tool, name)
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

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
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
    // An app removed from the home folder's own `Applications`: a link
    // into `/Applications` would be looked up on this Mac's disk.
    let target = home
        .path()
        .join("Applications/Removed.app/Contents/Resources/scripts/index.js");
    let target = target.as_path();
    link(&bin, "old-script", target);

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

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

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.kind, EntryKind::Symlink);
    assert_eq!(entry.resolved.as_deref(), Some(real.as_path()));
    assert_eq!(entry.link_target.as_deref(), Some(hop.to_str().unwrap()));
    assert_eq!(entry.size_bytes, Some(6));
}

#[test]
fn test_each_resolved_path_keeps_which_file_the_scan_found_there_and_never_sends_it() {
    // What Show in Finder checks the path against before Finder is asked
    // (`UnknownEntry::seen`): the file at `resolved` -- a link's target,
    // not the link -- by its device and inode. Absent where `resolved` is,
    // and never in what the window is sent.
    let home = Home::new("identity");
    let bin = home.dir("bin");
    let lib = home.dir("lib");
    let file = exe(&bin, "a-file", b"binary");
    let target = exe(&lib, "target", b"binary");
    link(&bin, "b-link", &target);
    link(&bin, "c-broken", Path::new("/nonexistent/tool"));
    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );
    assert_eq!(scan.entries.len(), 3, "{:?}", scan.entries);
    let seen = |index: usize| {
        let entry = &scan.entries[index];
        entry.seen.map(|stat| (stat.dev(), stat.ino()))
    };
    let of = |path: &Path| {
        let meta = fs::metadata(path).unwrap();
        Some((meta.dev(), meta.ino()))
    };
    assert_eq!(seen(0), of(&file));
    assert_eq!(seen(1), of(&target));
    assert_eq!(scan.entries[2].resolved, None);
    assert_eq!(seen(2), None);
    let json = serde_json::to_string(&scan).unwrap();
    assert!(!json.contains(r#""seen""#), "{json}");
    let back: banager_core::scan::UnknownScan = serde_json::from_str(&json).unwrap();
    assert!(back.entries.iter().all(|entry| entry.seen.is_none()));
}

#[test]
fn test_skips_subdirectories_files_without_an_execute_bit_and_links_to_directories() {
    let home = Home::new("skips");
    let bin = home.dir(".local/bin");
    home.dir(".local/bin/store");
    plain(&bin, "notes.txt");
    link(&bin, "data", Path::new("store"));

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

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
        &[],
        ScanBudget::default(),
    );
    assert_eq!(full.stopped, None, "exactly the budget is not over it");
    assert_eq!(full.scanned[0].entries, 2000);
    assert_eq!(full.entries.len(), 2000);

    exe(&bin, "t2000", b"x");
    let over = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );
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

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], &[], budget);

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

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

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
    // everything that resolves to `rustup` is cargo's. `hexyl` is not
    // by this rule: with no artifact carrying its path it is listed,
    // honestly; the test after this one gives cargo's inventory its say.
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
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 14, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".cargo/bin/hexyl"));
}

#[test]
fn test_rule_1_claims_a_link_leading_nowhere_that_would_lead_where_a_launcher_leading_nowhere_would(
) {
    // Grok Build's launcher-only state, a stopped uninstall having moved
    // `~/.grok/downloads` to the Trash (the whole-step review of step D):
    // the launcher `~/.grok/bin/grok` -- rule 0's -- and `~/.grok/bin/agent`
    // beside it both still name the one download, which is gone; so do the
    // fallback links the installer makes in `~/.local/bin`, whether their
    // text names the download or the launcher. Grok's uninstall moves every
    // one of them, so none is a stranger: rule 1 claims a link that would
    // lead to the same missing file as an instance's own launcher that
    // leads nowhere too. A link to another missing file is still listed.
    let home = Home::new("rule-1-dangling");
    let grok_bin = home.dir(".grok/bin");
    let local_bin = home.dir(".local/bin");
    let downloads = home.path().join(".grok/downloads");
    let text = Path::new("../downloads/grok-1.0.41-macos-aarch64");
    let launcher = link(&grok_bin, "grok", text);
    link(&grok_bin, "agent", text);
    link(&local_bin, "grok", &launcher);
    link(
        &local_bin,
        "agent",
        &downloads.join("grok-1.0.41-macos-aarch64"),
    );
    link(
        &local_bin,
        "grok-old",
        &downloads.join("grok-1.0.40-macos-aarch64"),
    );
    let instance = ManagerInstance {
        exe_path: launcher,
        prefix: home.path().join(".grok"),
        ..manager_instance("standalone-grok", "standalone-grok")
    };

    let scan = scan_dirs(
        &[grok_bin, local_bin],
        &home.env(vec![]),
        &[instance],
        &[],
        &[],
        ScanBudget::default(),
    );

    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        listed,
        vec![tilde(".local/bin/grok-old")],
        "{:?}",
        scan.entries
    );
    assert_eq!(
        scan.attributed, 4,
        "the launcher (rule 0), `agent` and the two fallback links (rule 1)"
    );
}

#[test]
fn test_rule_2_claims_a_cargo_installed_program_through_its_artifacts_path() {
    // The cargo adapter's inventory fills `InstalledArtifact.path` with
    // `<cargo_home>/bin/<binary>` for every crate (`parse_crates2` in
    // adapters/cargo.rs, phase 4 step E), so a `cargo install`ed program
    // is claimed by rule 2 -- the same rule uv's shims use -- and no
    // longer listed here.
    let home = Home::new("rule-2-cargo");
    let bin = home.dir(".cargo/bin");
    exe(&bin, "rustup", b"x");
    link(&bin, "cargo", Path::new("rustup"));
    let hexyl = exe(&bin, "hexyl", b"x");
    let cargo = ManagerInstance {
        exe_path: bin.join("cargo"),
        prefix: home.path().join(".cargo"),
        ..manager_instance(
            "cargo",
            &format!("cargo:{}", home.path().join(".cargo").display()),
        )
    };
    let hexyl_artifact = InstalledArtifact {
        key: ArtifactKey {
            instance_id: cargo.id.clone(),
            kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        },
        ..artifact(&cargo.id, "hexyl", &hexyl)
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[cargo],
        &[hexyl_artifact],
        &[],
        ScanBudget::default(),
    );

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 3);
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
        &[],
        ScanBudget::default(),
    );

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_2_claims_a_cask_binary_link_into_the_app_the_cask_installed() {
    // An Intel Mac, Homebrew at `/usr/local`: `brew install --cask
    // visual-studio-code` moves the `.app` into `/Applications` and links
    // `/usr/local/bin/code` to a file *inside* it -- under none of the
    // roots rule 3 gives Homebrew (`Cellar`, `Caskroom`, `opt`), in a
    // directory every scan reads. The cask's `path`, read by
    // `parse_info_installed` from the `app` stanza's `target` in `brew
    // info --installed --json=v2`, is what lets rule 2 claim the link, as
    // it claims uv's shim through the venv directory. The third-party
    // file beside it stays listed: the prefix's own `bin` is still
    // nobody's.
    let home = Home::new("rule-2-cask");
    let prefix = home.dir("usr/local");
    let prefix_bin = home.dir("usr/local/bin");
    let app = home.path().join("Applications/Visual Studio Code.app");
    let app_bin = home.dir("Applications/Visual Studio Code.app/Contents/Resources/app/bin");
    let code = exe(&app_bin, "code", b"#!/bin/sh\n");
    let code_link = link(&prefix_bin, "code", &code);
    exe(&prefix_bin, "dropped-in", b"x");
    let brew = ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.clone(),
        ..manager_instance("brew", &format!("brew:{}", prefix.display()))
    };
    let info = serde_json::json!({
        "formulae": [],
        "casks": [{
            "token": "visual-studio-code",
            "name": ["Visual Studio Code"],
            "installed": "1.104.0",
            "artifacts": [
                { "app": ["Visual Studio Code.app"], "target": app.display().to_string() },
                {
                    "binary": [code.display().to_string(), { "target": "code" }],
                    "target": code_link.display().to_string()
                }
            ]
        }]
    });
    let artifacts = parse_info_installed(&info.to_string(), &brew.id).expect("parse");

    let scan = scan_dirs(
        &[prefix_bin],
        &home.env(vec![]),
        &[brew],
        &artifacts,
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 1, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde("usr/local/bin/dropped-in"));
}

#[test]
fn test_rule_2_claims_a_cask_binary_link_into_a_second_app_or_an_app_with_no_target() {
    // A cask's `path` is one place, its first `app`'s. Two shapes Homebrew
    // installed were still listed (backlog, "cask 的命令行链接只认第一个
    // `app`"): a `binary` link into a second `.app` of the same cask, and
    // one of a cask whose `app` entry carries no absolute `target`. The
    // cask's own word for its commands -- the link a `binary` stanza put
    // in `<prefix>/bin`, from `brew info`'s `artifacts` -- claims them,
    // when the link resolves into the file the stanza names, the cask's
    // folder in `Caskroom`, or its app (`commands::cask_places`, the rule
    // "which copy runs" uses). A `pkg`-installed command no stanza names
    // stays listed, and so does a link of a stanza's name that now leads
    // somewhere else.
    let home = Home::new("rule-2-cask-commands");
    let prefix = home.dir("usr/local");
    let prefix_bin = home.dir("usr/local/bin");
    let main_app = home.path().join("Applications/Suite.app");
    home.dir("Applications/Suite.app/Contents/MacOS");
    let helper_bin = home.dir("Applications/Suite Helper.app/Contents/bin");
    let helper = exe(&helper_bin, "suite-helper", b"#!/bin/sh\n");
    let helper_link = link(&prefix_bin, "suite-helper", &helper);
    let bare_bin = home.dir("Applications/Bare.app/Contents/bin");
    let bare = exe(&bare_bin, "bare", b"#!/bin/sh\n");
    let bare_link = link(&prefix_bin, "bare", &bare);
    // Named by a stanza, but the link there now leads elsewhere.
    let elsewhere = exe(&home.dir("opt/other"), "moved", b"#!/bin/sh\n");
    let moved_link = link(&prefix_bin, "moved", &elsewhere);
    // A `pkg`'s command: no `binary` stanza names it.
    let pkg_bin = home.dir("Library/Suite/bin");
    let pkg_cmd = exe(&pkg_bin, "suite-pkg", b"#!/bin/sh\n");
    link(&prefix_bin, "suite-pkg", &pkg_cmd);
    let brew = ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.clone(),
        ..manager_instance("brew", &format!("brew:{}", prefix.display()))
    };
    let info = serde_json::json!({
        "formulae": [],
        "casks": [
            {
                "token": "suite",
                "name": ["Suite"],
                "installed": "3.0",
                "artifacts": [
                    { "app": ["Suite.app"], "target": main_app.display().to_string() },
                    {
                        "app": ["Suite Helper.app"],
                        "target": home.path().join("Applications/Suite Helper.app").display().to_string()
                    },
                    { "pkg": ["Suite.pkg"] },
                    {
                        "binary": [helper.display().to_string()],
                        "target": helper_link.display().to_string()
                    },
                    {
                        "binary": [main_app.join("Contents/MacOS/moved").display().to_string()],
                        "target": moved_link.display().to_string()
                    }
                ]
            },
            {
                "token": "bare",
                "name": ["Bare"],
                "installed": "1.0",
                "artifacts": [
                    { "app": ["Bare.app"] },
                    {
                        "binary": [bare.display().to_string()],
                        "target": bare_link.display().to_string()
                    }
                ]
            }
        ]
    });
    let artifacts = parse_info_installed(&info.to_string(), &brew.id).expect("parse");
    assert_eq!(
        artifacts
            .iter()
            .find(|a| a.key.name == "bare")
            .unwrap()
            .path,
        None,
        "no absolute target beside the app: no path"
    );

    let scan = scan_dirs(
        &[prefix_bin],
        &home.env(vec![]),
        &[brew],
        &artifacts,
        &[],
        ScanBudget::default(),
    );

    let mut listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    listed.sort();
    assert_eq!(
        listed,
        vec![
            tilde("usr/local/bin/moved"),
            tilde("usr/local/bin/suite-pkg")
        ]
    );
    assert_eq!(scan.attributed, 2, "suite-helper and bare");
}

#[test]
fn test_rule_2_claims_flutters_commands_through_the_suite_it_moved() {
    // r34 U1. `brew install --cask flutter` (homebrew/cask 3.47.6: `suite
    // "flutter", target: "#{HOMEBREW_PREFIX}/share/flutter"`, `binary
    // "flutter/bin/dart"`, `binary "flutter/bin/flutter"`) moves the suite
    // to `<prefix>/share/flutter`, leaves a link to it where it was staged
    // (`Caskroom/flutter/3.47.6/flutter`) and links each command to its
    // staged path. Where the link leads is in no place of the cask's, so
    // both were listed as programs no source installed while Installed
    // listed Flutter under Homebrew. A link whose own text names a place
    // in the cask's Caskroom folder is the cask's (Homebrew's own
    // `target_links_to_source?`, `commands::names_staged`); one whose
    // text climbs out of the suite with a `..` is followed, and npm's
    // file it leads to stays listed.
    let home = Home::new("rule-2-flutter");
    let prefix = home.dir("opt/homebrew");
    let prefix_bin = home.dir("opt/homebrew/bin");
    let suite = home.path().join("opt/homebrew/share/flutter");
    let suite_bin = home.dir("opt/homebrew/share/flutter/bin");
    exe(&suite_bin, "dart", b"#!/bin/sh\n");
    exe(&suite_bin, "flutter", b"#!/bin/sh\n");
    let staged = link(
        &home.dir("opt/homebrew/Caskroom/flutter/3.47.6"),
        "flutter",
        &suite,
    );
    let dart = link(&prefix_bin, "dart", &staged.join("bin/dart"));
    let flutter = link(&prefix_bin, "flutter", &staged.join("bin/flutter"));
    let brew = ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.clone(),
        ..manager_instance("brew", &format!("brew:{}", prefix.display()))
    };
    let info = serde_json::json!({
        "formulae": [],
        "casks": [{
            "token": "flutter",
            "full_token": "flutter",
            "name": ["Flutter SDK"],
            "installed": "3.47.6",
            "artifacts": [
                {
                    "suite": ["flutter", { "target": suite.display().to_string() }],
                    "target": suite.display().to_string()
                },
                { "binary": ["flutter/bin/dart"], "target": dart.display().to_string() },
                { "binary": ["flutter/bin/flutter"], "target": flutter.display().to_string() },
                { "zap": [{ "trash": "~/.flutter" }] }
            ]
        }]
    });
    let artifacts = parse_info_installed(&info.to_string(), &brew.id).expect("parse");
    let scan = |brew: &ManagerInstance| {
        scan_dirs(
            std::slice::from_ref(&prefix_bin),
            &home.env(vec![]),
            std::slice::from_ref(brew),
            &artifacts,
            &[],
            ScanBudget::default(),
        )
    };

    let found = scan(&brew);
    assert!(found.entries.is_empty(), "{:?}", found.entries);
    assert_eq!(found.attributed, 2);

    // By name still in the Caskroom folder; on the disk, out of the suite
    // into npm's package.
    let npm = exe(
        &home.dir("opt/homebrew/lib/node_modules/dart/bin"),
        "dart.js",
        b"#!/usr/bin/env node\n",
    );
    fs::remove_file(&dart).unwrap();
    link(
        &prefix_bin,
        "dart",
        &staged.join("../../lib/node_modules/dart/bin/dart.js"),
    );
    assert_eq!(fs::canonicalize(&dart).unwrap(), npm);
    let found = scan(&brew);
    let listed: Vec<PathBuf> = found.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(listed, vec![tilde("opt/homebrew/bin/dart")]);
    assert_eq!(found.attributed, 1, "flutter");
}

#[test]
fn test_rule_3_claims_a_link_into_homebrews_cellar_but_not_into_the_rest_of_its_prefix() {
    // An Intel Mac: `/usr/local/bin` is both Homebrew's bin and where
    // third-party installers drop things. A link into `Cellar` is
    // Homebrew's; a program under the prefix's own `bin` is not thereby
    // Homebrew's -- `brew info --installed` would never list it.
    let home = Home::new("rule-3-brew");
    let bin = home.dir(".local/bin");
    let prefix = home.dir("opt/homebrew");
    let keg = home.dir("opt/homebrew/Cellar/jq/1.8.1/bin");
    let jq = exe(&keg, "jq", b"x");
    link(&bin, "jq", &jq);
    let prefix_bin = home.dir("opt/homebrew/bin");
    let dropped = exe(&prefix_bin, "dropped-in", b"x");
    link(&bin, "dropped-in", &dropped);
    let brew = ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.clone(),
        ..manager_instance("brew", &format!("brew:{}", prefix.display()))
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[brew],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 1, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/dropped-in"));
}

#[test]
fn test_rule_3_claims_what_resolves_into_a_root_an_instance_owns_outright() {
    // Ollama owns all of `~/.ollama`.
    let home = Home::new("rule-3-ollama");
    let bin = home.dir(".local/bin");
    let ollama_bin = home.dir(".ollama/bin");
    let real = exe(&ollama_bin, "model-tool", b"x");
    link(&bin, "model-tool", &real);
    let ollama = ManagerInstance {
        exe_path: home.path().join("elsewhere/ollama"),
        prefix: home.path().join(".ollama"),
        ..manager_instance("ollama", "ollama:http://127.0.0.1:11434")
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[ollama],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_3_claims_an_npm_global_cli_under_a_home_prefix() {
    // `npm config set prefix ~/.npm-global`, the setup npm's own docs
    // recommend over `sudo`: every global package unpacks under
    // `~/.npm-global/lib/node_modules`, and `~/.npm-global/bin/<tool>` is
    // a relative link into it. That bin directory is on `PATH`, so it is
    // scanned; without npm's root every global CLI on such a Mac would be
    // a row here and a row under npm on the Installed page at once
    // (ruling 10). The npm executable lives elsewhere so rules 0 and 1
    // cannot be why the link is claimed.
    let home = Home::new("rule-3-npm");
    let prefix = home.path().join(".npm-global");
    let package_bin = home.dir(".npm-global/lib/node_modules/some-tool/bin");
    exe(&package_bin, "cli.js", b"#!/usr/bin/env node\n");
    let bin = home.dir(".npm-global/bin");
    link(
        &bin,
        "some-tool",
        Path::new("../lib/node_modules/some-tool/bin/cli.js"),
    );
    let npm = ManagerInstance {
        exe_path: home.path().join("elsewhere/npm"),
        prefix: prefix.clone(),
        ..manager_instance("npm", &format!("npm:{}", prefix.display()))
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[npm],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_3_claims_the_links_to_pythons_uv_manages_and_nothing_else() {
    // `uv python install 3.12` puts the Python in uv's data folder and
    // links `~/.local/bin/python3.12` to it through the minor-version
    // folder, itself a link to the patch release (uv's docs, "Minor
    // version directories"). The shape of the author's Mac, names
    // invented. uv's own executable lives elsewhere and its prefix is
    // `~/.local/bin` (uv.rs takes `exe_path.parent()`), so neither rule 0
    // nor the prefix is why anything is claimed: only uv's Python folder.
    let home = Home::new("rule-3-uv-python");
    let bin = home.dir(".local/bin");
    let pythons = home.dir(".local/share/uv/python");
    let patch = home.dir(".local/share/uv/python/cpython-3.12.14-macos-aarch64-none/bin");
    exe(&patch, "python3.12", b"x");
    link(
        &pythons,
        "cpython-3.12-macos-aarch64-none",
        Path::new("cpython-3.12.14-macos-aarch64-none"),
    );
    link(
        &bin,
        "python3.12",
        &pythons.join("cpython-3.12-macos-aarch64-none/bin/python3.12"),
    );
    // A minor version pip never asks about, linked to its patch folder
    // straight away.
    let older = home.dir(".local/share/uv/python/cpython-3.9.23-macos-aarch64-none/bin");
    exe(&older, "python3.9", b"x");
    link(&bin, "python3.9", &older.join("python3.9"));
    // A Python uv does not manage, and a program of the user's own.
    let other = home.dir(".pyenv/versions/3.11.9/bin");
    exe(&other, "python3.11", b"x");
    link(&bin, "python3.11", &other.join("python3.11"));
    exe(&bin, "my-script", b"x");
    let uv = ManagerInstance {
        exe_path: home.path().join("elsewhere/uv"),
        prefix: bin.clone(),
        ..manager_instance("uv", "uv")
    };

    let scan = scan_dirs(
        std::slice::from_ref(&bin),
        &home.env(vec![]),
        &[uv],
        &[],
        &[],
        ScanBudget::default(),
    );

    let names: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        names,
        vec![
            tilde(".local/bin/my-script"),
            tilde(".local/bin/python3.11")
        ]
    );
    assert_eq!(scan.attributed, 2);

    // Without uv the same links are listed, as any owner's are once it
    // is gone.
    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );
    assert_eq!(scan.entries.len(), 4, "{:?}", scan.entries);
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_rule_3_does_not_place_a_uv_python_installed_outside_the_default_folder() {
    // `UV_PYTHON_INSTALL_DIR` moves uv's Pythons, but Banager is handed
    // only the shell's `PATH`: a Python elsewhere is not uv's to this
    // scan, and neither is a link uv left pointing at one it removed.
    let home = Home::new("rule-3-uv-elsewhere");
    let bin = home.dir(".local/bin");
    home.dir(".local/share/uv/python");
    let moved = home.dir("pythons/cpython-3.13.5-macos-aarch64-none/bin");
    exe(&moved, "python3.13", b"x");
    link(&bin, "python3.13", &moved.join("python3.13"));
    link(
        &bin,
        "python3.10",
        &home
            .path()
            .join(".local/share/uv/python/cpython-3.10.18-macos-aarch64-none/bin/python3.10"),
    );
    let uv = ManagerInstance {
        exe_path: home.path().join("elsewhere/uv"),
        prefix: bin.clone(),
        ..manager_instance("uv", "uv")
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[uv],
        &[],
        &[],
        ScanBudget::default(),
    );

    let rows: Vec<(PathBuf, EntryKind)> = scan
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.kind))
        .collect();
    assert_eq!(
        rows,
        vec![
            (tilde(".local/bin/python3.10"), EntryKind::BrokenSymlink),
            (tilde(".local/bin/python3.13"), EntryKind::Symlink),
        ]
    );
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_rule_3_never_treats_a_parent_derived_prefix_as_owned() {
    // The counter-example spec §8.3 is built around: a pip instance whose
    // prefix is `~/.local/bin` itself (pip.rs:125-128 takes
    // `exe_path.parent()`). Its executable lives elsewhere here so rule 0
    // cannot be why anything is or is not claimed; the prefix alone must
    // claim nothing, or this page would lose every program in the one
    // directory it exists to look at.
    let home = Home::new("rule-3-parent-prefix");
    let bin = home.dir(".local/bin");
    exe(&bin, "agy", b"x");
    exe(&bin, "standalone-tool", b"x");
    let pip = ManagerInstance {
        exe_path: home.path().join("elsewhere/python3"),
        prefix: bin.clone(),
        ..manager_instance("pip", "pip:elsewhere")
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[pip],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 0);
    assert_eq!(scan.entries.len(), 2, "{:?}", scan.entries);
}

/// agy's pattern, as its recipe declares it (spec §3.5): a `'static` slice
/// because the scan reads recipes' own constants.
static AGY_GLOBS: [Glob; 1] = [Glob {
    dir: "~/.local/bin",
    prefix: "agy.",
    suffix: ".old",
    what: RemovedWhat::Backups,
}];

#[test]
fn test_rule_4_claims_a_backup_the_updaters_pattern_names_only_while_the_tool_is_installed() {
    // Spec §8.3 rule 4: `agy.<time>.old` beside an installed agy is the
    // updater's leftover, not a stranger -- by name, in that directory, for
    // a regular file. A link with such a name is not (the pattern describes
    // the updater's copies, which are files) -- pointed at a file no rule
    // claims, so that it is rule 4's `File` guard and not rule 1 (a link
    // resolving to agy's own exe_path) that decides; a name without the
    // middle is not; and once agy is gone the pattern is gone with its
    // instance, so a leftover `.old` is listed, which is the truth.
    let home = Home::new("rule-4");
    let bin = home.dir(".local/bin");
    let agy = exe(&bin, "agy", b"x");
    exe(&bin, "agy.1727000000.old", b"x");
    exe(&bin, "agy.old", b"x");
    let elsewhere = home.dir("elsewhere");
    let other = exe(&elsewhere, "other-tool", b"y");
    link(&bin, "agy.2.old", &other);
    let instance = ManagerInstance {
        exe_path: agy.clone(),
        prefix: home.path().join(".gemini/antigravity-cli"),
        ..manager_instance("standalone-agy", "standalone-agy")
    };
    let globs = vec![("standalone-agy".to_string(), &AGY_GLOBS[..])];

    let scan = scan_dirs(
        std::slice::from_ref(&bin),
        &home.env(vec![]),
        std::slice::from_ref(&instance),
        &[],
        &globs,
        ScanBudget::default(),
    );

    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        listed,
        vec![tilde(".local/bin/agy.2.old"), tilde(".local/bin/agy.old")],
        "{:?}",
        scan.entries
    );
    assert_eq!(scan.attributed, 2, "agy (rule 0) and its backup (rule 4)");

    // agy uninstalled: no instance, no pattern; the backup is a stranger.
    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &globs,
        ScanBudget::default(),
    );
    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert!(
        listed.contains(&tilde(".local/bin/agy.1727000000.old")),
        "{listed:?}"
    );
    assert_eq!(scan.attributed, 0);
}

// ------------------------------------------------- protected places
//
// The scan keeps the command check's and the disk-use measurement's
// promise: nothing inside `~/Documents`, `~/Desktop`, iCloud, `/Volumes`
// and the rest of `protected::PROTECTED_IN_HOME` is listed, `lstat`ed,
// read as a link or resolved, through any link. Each test's home is a temp
// folder, and `Protected::new(home)` puts its own `Documents` and the like
// out of bounds; `/Volumes` is out of bounds by name, so a test can name a
// disk that is not there and nothing is looked up on it.

/// A disk no Mac running these tests has: `/Volumes` is never entered, so
/// nothing finds out.
const NO_SUCH_DISK: &str = "/Volumes/Banager-test-no-such-disk";

/// Where macOS mounts the data volume, when this Mac has it: the second
/// spelling of `/Users` and `/Volumes` (`protected::DATA_VOLUME`).
fn data_volume() -> Option<PathBuf> {
    let data = Path::new(banager_core::protected::DATA_VOLUME);
    data.is_dir().then(|| data.to_path_buf())
}

#[test]
fn test_a_scanned_folder_in_a_protected_place_is_not_read_and_is_named_once() {
    let home = Home::new("protected-dirs");
    let local = home.dir(".local/bin");
    exe(&local, "mine", b"x");
    // A `PATH` entry in Documents, and the same folder spelled in lower
    // case, as a case-insensitive disk takes it: one place, named once.
    let scripts = home.dir("Documents/scripts");
    exe(&scripts, "in-documents", b"x");
    let scripts_lower = home.path().join("documents/scripts");
    // `~/bin` as a link to a folder on the Desktop.
    let desktop_bin = home.dir("Desktop/bin");
    exe(&desktop_bin, "on-desktop", b"x");
    let bin = link(home.path(), "bin", &desktop_bin);
    // `$CARGO_HOME/bin` on another disk, by name only.
    let cargo_bin = PathBuf::from(NO_SUCH_DISK).join("cargo/bin");
    let mut dirs = vec![
        local.clone(),
        scripts.clone(),
        scripts_lower,
        bin,
        cargo_bin.clone(),
    ];
    // And the data volume's spelling of `/Volumes`.
    let aliased = data_volume().map(|data| data.join("Volumes/Banager-test-no-such-disk/bin"));
    if let Some(aliased) = &aliased {
        dirs.push(aliased.clone());
    }

    let scan = scan_dirs(
        &dirs,
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

    // Only `~/.local/bin` was read, and only its program is listed.
    assert_eq!(
        scan.scanned,
        vec![ScannedDir {
            path: tilde(".local/bin"),
            entries: 1
        }]
    );
    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(listed, vec![tilde(".local/bin/mine")]);
    let mut expected = vec![tilde("Documents/scripts"), tilde("bin"), cargo_bin];
    if let Some(aliased) = aliased {
        expected.push(aliased);
    }
    assert_eq!(scan.protected_dirs, expected);
}

#[test]
fn test_a_link_into_documents_is_listed_by_its_own_name_and_not_followed() {
    let home = Home::new("protected-link");
    let bin = home.dir(".local/bin");
    let project_bin = home.dir("Documents/proj/bin");
    let tool = exe(&project_bin, "tool", b"#!/bin/sh\n");
    // Straight in, by a relative text, and through a folder that is itself
    // a link into Documents.
    link(&bin, "tool", &tool);
    link(&bin, "relative", Path::new("../../Documents/proj/bin/tool"));
    link(home.path(), "proj", &home.path().join("Documents/proj"));
    link(&bin, "through", &home.path().join("proj/bin/tool"));

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 0);
    assert!(scan.protected_dirs.is_empty(), "{:?}", scan.protected_dirs);
    let mut rows: Vec<_> = scan.entries.iter().collect();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    let names: Vec<PathBuf> = rows.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        names,
        vec![
            tilde(".local/bin/relative"),
            tilde(".local/bin/through"),
            tilde(".local/bin/tool"),
        ]
    );
    for entry in rows {
        // Its own name and its own text; nothing of what it leads to.
        assert_eq!(entry.kind, EntryKind::ProtectedSymlink, "{entry:?}");
        assert_eq!(entry.resolved, None, "{entry:?}");
        assert_eq!(entry.size_bytes, None, "{entry:?}");
        assert_eq!(entry.modified_at, None, "{entry:?}");
        assert!(entry.link_target.is_some(), "{entry:?}");
        assert!(entry.owned_by_me);
    }
}

#[test]
fn test_a_homebrew_prefix_on_volumes_still_claims_its_programs_by_name() {
    // Homebrew installed on an external disk: its roots are on `/Volumes`,
    // which is never entered, so its links are placed by name -- and are
    // Homebrew's, not other programs.
    let home = Home::new("protected-brew");
    let bin = home.dir(".local/bin");
    let prefix = PathBuf::from(NO_SUCH_DISK).join("homebrew");
    link(&bin, "jq", &prefix.join("Cellar/jq/1.8.1/bin/jq"));
    link(&bin, "brew", &prefix.join("bin/brew"));
    // Under the prefix, but in none of its roots: not Homebrew's by name.
    link(&bin, "dropped-in", &prefix.join("bin/dropped-in"));
    // The same disk, named from the data volume, into Homebrew's `opt`.
    if let Some(data) = data_volume() {
        let aliased = data.join("Volumes/Banager-test-no-such-disk/homebrew/opt/jq/bin/jq");
        link(&bin, "jq-aliased", &aliased);
    }
    let brew = ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.clone(),
        ..manager_instance("brew", &format!("brew:{}", prefix.display()))
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[brew],
        &[],
        &[],
        ScanBudget::default(),
    );

    let expected_attributed = if data_volume().is_some() { 3 } else { 2 };
    assert_eq!(scan.attributed, expected_attributed, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.path, tilde(".local/bin/dropped-in"));
    assert_eq!(entry.kind, EntryKind::ProtectedSymlink);
    assert_eq!(entry.resolved, None);
}

#[test]
fn test_a_folder_that_cannot_be_read_is_left_untouched_and_not_reported() {
    let home = Home::new("mode-000");
    let local = home.dir(".local/bin");
    let locked = home.dir("bin");
    exe(&locked, "inside", b"x");
    // A link through a locked folder leads nowhere Banager can see.
    let shut = home.dir("shut");
    exe(&shut, "tool", b"x");
    link(&local, "through-shut", &shut.join("tool"));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock");
    fs::set_permissions(&shut, fs::Permissions::from_mode(0o000)).expect("lock");

    let scan = scan_dirs(
        &[locked.clone(), local],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );
    let locked_mode = fs::symlink_metadata(&locked).expect("stat").mode() & 0o777;
    let shut_mode = fs::symlink_metadata(&shut).expect("stat").mode() & 0o777;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
    fs::set_permissions(&shut, fs::Permissions::from_mode(0o755)).expect("unlock");

    // Left as it was, not reported as read, and not a protected place.
    assert_eq!(locked_mode, 0o000);
    assert_eq!(shut_mode, 0o000);
    assert_eq!(
        scan.scanned,
        vec![ScannedDir {
            path: tilde(".local/bin"),
            entries: 1
        }]
    );
    assert!(scan.protected_dirs.is_empty(), "{:?}", scan.protected_dirs);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/through-shut"));
    assert_eq!(scan.entries[0].kind, EntryKind::BrokenSymlink);
}

#[test]
fn test_an_entry_named_like_a_protected_place_is_passed_over_unlooked_at() {
    // A home folder that is itself on `PATH`, and `~/Library` scanned too:
    // their entries `Documents`, `Desktop`, `downloads` (any case) and
    // `Containers` are protected places themselves. Not one is looked at,
    // as a folder or as a link, so none is listed and no link's text is
    // read -- even where the link leads to a program outside.
    let home = Home::new("protected-entry-names");
    let elsewhere = home.dir("elsewhere");
    let tool = exe(&elsewhere, "tool", b"x");
    exe(home.path(), "mine", b"x");
    link(home.path(), "Documents", &tool);
    home.dir("Desktop");
    link(home.path(), "downloads", Path::new("elsewhere/tool"));
    let library = home.dir("Library");
    link(&library, "Containers", &tool);
    home.dir("Library/CloudStorage");
    exe(&library, "lib-tool", b"x");

    let scan = scan_dirs(
        &[home.path().to_path_buf(), library],
        &home.env(vec![]),
        &[],
        &[],
        &[],
        ScanBudget::default(),
    );

    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        listed,
        vec![tilde("mine"), tilde("Library/lib-tool")],
        "{:?}",
        scan.entries
    );
    assert!(
        scan.entries.iter().all(|e| e.link_target.is_none()),
        "{:?}",
        scan.entries
    );
    assert!(scan.protected_dirs.is_empty(), "{:?}", scan.protected_dirs);
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_a_dot_dot_out_of_a_protected_place_is_not_taken_by_name() {
    // `~/Documents` is a link elsewhere, so `~/Documents/..` is not the
    // home folder; by name alone the link below would pass for the source's
    // own program. Once a `..` climbs back out of the place, the link is
    // compared with nothing and listed.
    let home = Home::new("protected-dot-dot");
    let elsewhere = home.dir("elsewhere/deep");
    link(home.path(), "Documents", &elsewhere);
    let share = home.dir(".local/share/thing");
    let program = exe(&share, "tool", b"x");
    let bin = home.dir(".local/bin");
    link(
        &bin,
        "odd",
        &home.path().join("Documents/../.local/share/thing/tool"),
    );
    // A `..` that stays inside the place is still folded by name.
    link(
        &bin,
        "inside",
        &home.path().join("Documents/x/../.hidden/tool"),
    );
    let instance = ManagerInstance {
        exe_path: program,
        prefix: home.path().join(".local/share/thing"),
        ..manager_instance("standalone-thing", "standalone-thing")
    };
    let hidden = ManagerInstance {
        exe_path: home.path().join("Documents/.hidden/tool"),
        prefix: home.path().join("Documents/.hidden"),
        ..manager_instance("standalone-hidden", "standalone-hidden")
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[instance, hidden],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 1, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/odd"));
    assert_eq!(scan.entries[0].kind, EntryKind::ProtectedSymlink);
}

#[test]
fn test_a_sources_own_paths_in_documents_claim_their_programs_by_name_unread() {
    // A source whose own executable, and an artifact whose path, are in
    // `~/Documents`. Nothing there exists: had the scan looked, it would
    // have found nothing and listed the links as broken. By name alone,
    // they are the source's.
    let home = Home::new("protected-sources");
    let bin = home.dir(".local/bin");
    let documents = home.path().join("Documents");
    let exe_path = documents.join("tools/foo");
    link(&bin, "foo", &exe_path);
    link(&bin, "bar", &documents.join("venvs/bar/bin/bar"));
    let foo = ManagerInstance {
        exe_path,
        prefix: documents.join("tools"),
        ..manager_instance("standalone-foo", "standalone-foo")
    };
    let venv = artifact("uv:test", "bar", &documents.join("venvs/bar"));

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[foo],
        &[venv],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.attributed, 2, "{:?}", scan.entries);
    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
}

#[test]
fn test_a_launcher_whose_downloads_lead_into_documents_is_compared_by_name_unread() {
    // Grok Build's layout with `~/.grok/downloads` a link into
    // `~/Documents`, where nothing is looked at -- not even whether the
    // download is there (it is not). The launcher leads there by name, so
    // `agent` beside it and the fallback link in `~/.local/bin` are
    // Grok's; a link naming another download is listed by its own name.
    let home = Home::new("protected-grok");
    let grok_bin = home.dir(".grok/bin");
    let local_bin = home.dir(".local/bin");
    link(
        &home.path().join(".grok"),
        "downloads",
        &home.path().join("Documents/grok-dl"),
    );
    let downloads = home.path().join(".grok/downloads");
    let text = Path::new("../downloads/grok-1.0.41-macos-aarch64");
    let launcher = link(&grok_bin, "grok", text);
    link(&grok_bin, "agent", text);
    link(
        &local_bin,
        "grok",
        &downloads.join("grok-1.0.41-macos-aarch64"),
    );
    link(
        &local_bin,
        "grok-old",
        &downloads.join("grok-1.0.40-macos-aarch64"),
    );
    let instance = ManagerInstance {
        exe_path: launcher,
        prefix: home.path().join(".grok"),
        ..manager_instance("standalone-grok", "standalone-grok")
    };

    let scan = scan_dirs(
        &[grok_bin, local_bin],
        &home.env(vec![]),
        &[instance],
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(
        scan.attributed, 3,
        "the launcher, `agent` and the fallback link: {:?}",
        scan.entries
    );
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/grok-old"));
    assert_eq!(scan.entries[0].kind, EntryKind::ProtectedSymlink);
    assert!(!home.path().join("Documents").exists());
}
