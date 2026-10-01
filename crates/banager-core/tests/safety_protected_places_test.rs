//! Promise 1 of `docs/what-we-run.md`, as a property over every walker:
//! no read-only walk Banager makes ever looks inside one of the places it
//! never reads (`protected::PROTECTED_IN_HOME` under the home folder, and
//! `/Volumes`), whichever way the place is spelled -- as listed, in
//! another case, through the data volume (`/System/Volumes/Data/...`),
//! through a link whose own name is harmless, or with the home folder
//! itself named through a link.
//!
//! Each place is made in a temp home with something inside it the walker
//! would find if it looked (a program, a folder of files with a size).
//! A walker that took one step inside would therefore answer differently
//! -- read the folder, find the program, measure the size. For the places
//! that cannot be made (`/Volumes`), and for the place itself, the folder
//! named is missing instead: a walker that looked would find nothing
//! there and drop it, where one that keeps out names it as unread.
//!
//! Walkers covered: `commands::read_folders` (which copy a command runs),
//! `scan::scan_dirs` (Other Programs), `runner::resolve_exe` (finding a
//! package manager), `route::shadow_note` (a standalone tool's PATH note),
//! `kept_data::kept_data` (what an uninstall leaves) and `SizeMeter`
//! (disk use).

use banager_core::adapters::standalone::route::shadow_note;
use banager_core::commands::{read_folders, CommandBudget};
use banager_core::kept_data::kept_data;
use banager_core::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, Warning,
};
use banager_core::protected::{DATA_VOLUME, PROTECTED_IN_HOME};
use banager_core::runner::{resolve_exe, HostEnv};
use banager_core::scan::{scan_dirs, ScanBudget};
use banager_core::size::{SizeBudget, SizeMeter};
use banager_core::testing::manager_instance;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The program planted in every place, and its harmless twin outside.
const TOOL: &str = "banager-x3-tool";
/// A disk that is never there.
const NO_SUCH_DISK: &str = "/Volumes/Banager-x3-no-such-disk";

/// A fresh home folder, removed when the test ends. `path` is canonical;
/// `as_given` is how the system's temp folder spells it (`/var/...` on a
/// Mac, a link to `/private/var/...`), a home named through a link.
struct Home {
    path: PathBuf,
    as_given: PathBuf,
}

impl Home {
    fn new(tag: &str) -> Home {
        let as_given = std::env::temp_dir().join(format!(
            "banager-x3-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&as_given).unwrap();
        Home {
            path: fs::canonicalize(&as_given).unwrap(),
            as_given,
        }
    }

    fn exe(&self, rel: &str) -> PathBuf {
        let path = self.path.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn file(&self, rel: &str, len: usize) {
        let path = self.path.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![7u8; len]).unwrap();
    }

    /// `link` (replaced if there) leading to `target`.
    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let link = self.path.join(rel);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&link);
        symlink(target, &link).unwrap();
        link
    }

    fn env(&self, home: &Path, path_dirs: Vec<PathBuf>) -> HostEnv {
        HostEnv {
            path_dirs,
            home: home.to_path_buf(),
            euid: fs::metadata(&self.path).unwrap().uid(),
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }

    /// Each spelling of the home folder a walk may be handed: canonical,
    /// and through the link the system's temp folder is reached by.
    fn homes(&self) -> Vec<PathBuf> {
        let mut homes = vec![self.path.clone()];
        if self.as_given != self.path {
            homes.push(self.as_given.clone());
        }
        homes
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// `path` spelled from the data volume, when this Mac has one.
fn on_data_volume(path: &Path) -> Option<PathBuf> {
    let aliased = Path::new(DATA_VOLUME).join(path.strip_prefix("/").ok()?);
    aliased.exists().then_some(aliased)
}

/// Every way a walk could be handed the place `<home>/<place>`: as
/// listed, all lower case, all upper case, through the data volume, and
/// through a link in the home folder whose own name is not protected
/// (made here, numbered by `n`). Each with what it is, for the message.
fn spellings(home: &Home, place: &str, n: usize) -> Vec<(String, PathBuf)> {
    let exact = home.path.join(place);
    let mut all = vec![
        (format!("{place}"), exact.clone()),
        (
            format!("{place} in lower case"),
            home.path.join(place.to_lowercase()),
        ),
        (
            format!("{place} in upper case"),
            home.path.join(place.to_uppercase()),
        ),
        (
            format!("{place} through a link"),
            home.link(&format!("via-{n}"), &exact),
        ),
    ];
    if let Some(aliased) = on_data_volume(&exact) {
        all.push((format!("{place} through the data volume"), aliased));
    }
    all
}

/// `/Volumes` under each of its spellings, and through a link: all missing.
fn volume_spellings(home: &Home) -> Vec<(String, PathBuf)> {
    let mut all = vec![
        ("/Volumes".to_string(), PathBuf::from(NO_SUCH_DISK)),
        (
            "/volumes".to_string(),
            PathBuf::from(NO_SUCH_DISK.to_lowercase()),
        ),
        (
            "/Volumes through a link".to_string(),
            home.link("via-volumes", Path::new(NO_SUCH_DISK)),
        ),
    ];
    if Path::new(DATA_VOLUME).join("Volumes").is_dir() {
        all.push((
            "/Volumes through the data volume".to_string(),
            Path::new(DATA_VOLUME).join(NO_SUCH_DISK.trim_start_matches('/')),
        ));
    }
    all
}

/// Plants, inside each protected place, a program in `banager-bin`, an
/// npm prefix with a package of some size in `banager-npm`, and a data
/// folder in `banager-data`; and outside, the program's twin in
/// `outside/bin`. Returns the twin.
fn plant(home: &Home) -> PathBuf {
    for place in PROTECTED_IN_HOME {
        home.exe(&format!("{place}/banager-bin/{TOOL}"));
        home.file(
            &format!("{place}/banager-npm/lib/node_modules/pkg/index.js"),
            64 * 1024,
        );
        home.file(&format!("{place}/banager-data/big.bin"), 64 * 1024);
    }
    home.exe(&format!("outside/bin/{TOOL}"))
}

/// Every spelling of every place, with a walk-ready folder under it:
/// `(what, folder that holds something, folder that is not there)`.
fn every_place(home: &Home) -> Vec<(String, PathBuf, PathBuf)> {
    let mut all = Vec::new();
    for (n, place) in PROTECTED_IN_HOME.iter().enumerate() {
        for (what, at) in spellings(home, place, n) {
            all.push((what, at.join("banager-bin"), at.join("banager-missing")));
        }
    }
    all
}

#[test]
fn test_which_copy_runs_never_reads_a_folder_in_any_protected_place_however_spelled() {
    // `commands::read_folders`: a folder it read is in `path_folders`, one
    // it kept out of in `unread_path_folders`. Had it looked inside, the
    // planted folder would have been read, and the missing one dropped as
    // not there.
    let home = Home::new("commands");
    let outside = plant(&home);
    let outside_bin = outside.parent().unwrap().to_path_buf();
    let mut cases = every_place(&home);
    for (what, missing) in volume_spellings(&home) {
        cases.push((what, missing.join("bin"), missing.join("other")));
    }
    for given in home.homes() {
        for (what, there, missing) in &cases {
            for folder in [there, missing] {
                let found = read_folders(
                    &[folder.clone(), outside_bin.clone()],
                    &[],
                    &given,
                    CommandBudget::default(),
                );
                assert!(found.complete(), "{what}: {}", folder.display());
                assert_eq!(
                    found.unread_path_folders(),
                    vec![folder.as_path()],
                    "{what} (home {}): {} must be kept unread",
                    given.display(),
                    folder.display()
                );
                assert_eq!(
                    found.path_folders(),
                    vec![outside_bin.as_path()],
                    "{what}: only the folder outside is read"
                );
            }
        }
    }
}

#[test]
fn test_the_other_programs_scan_never_reads_a_folder_in_any_protected_place_however_spelled() {
    // `scan::scan_dirs`: a folder it kept out of is named in
    // `protected_dirs` and nothing in it is listed. Had it looked, the
    // planted program would be listed and the missing folder skipped
    // without a word.
    let home = Home::new("scan");
    let outside = plant(&home);
    let outside_bin = outside.parent().unwrap().to_path_buf();
    let mut cases = every_place(&home);
    for (what, missing) in volume_spellings(&home) {
        cases.push((what, missing.join("bin"), missing.join("other")));
    }
    for given in home.homes() {
        for (what, there, missing) in &cases {
            for folder in [there, missing] {
                let scan = scan_dirs(
                    &[folder.clone(), outside_bin.clone()],
                    &home.env(&given, vec![]),
                    &[],
                    &[],
                    &[],
                    ScanBudget::default(),
                );
                assert_eq!(
                    scan.protected_dirs.len(),
                    1,
                    "{what} (home {}): {} must be named as not read, got {:?}",
                    given.display(),
                    folder.display(),
                    scan.protected_dirs
                );
                assert_eq!(scan.scanned.len(), 1, "{what}: only the folder outside");
                assert!(
                    scan.entries
                        .iter()
                        .all(|entry| !entry.path.to_string_lossy().contains("banager-bin")),
                    "{what}: nothing inside is listed: {:?}",
                    scan.entries
                );
            }
        }
    }
}

#[test]
fn test_finding_a_package_manager_never_takes_one_from_any_protected_place_however_spelled() {
    // `resolve_exe`: the planted program comes first on PATH. Had it been
    // looked at, it would be the one found; kept out of, the twin outside
    // is.
    let home = Home::new("resolve-exe");
    let outside = plant(&home);
    let outside_bin = outside.parent().unwrap().to_path_buf();
    for given in home.homes() {
        for (what, there, _) in every_place(&home) {
            let env = home.env(&given, vec![there.clone(), outside_bin.clone()]);
            assert_eq!(
                resolve_exe(TOOL, &env),
                Some(outside.clone()),
                "{what} (home {}): {} must be passed over",
                given.display(),
                there.display()
            );
        }
    }
}

#[test]
fn test_a_standalone_tools_path_note_never_rests_on_any_protected_place_however_spelled() {
    // `route::shadow_note`: the planted program comes first on PATH, then
    // this copy. Kept out of, the first entry may be anything, so no note
    // is given; looked at, the planted program would be found first and
    // this copy called shadowed by it.
    let home = Home::new("shadow-note");
    let outside = plant(&home);
    let outside_bin = outside.parent().unwrap().to_path_buf();
    for given in home.homes() {
        for (what, there, _) in every_place(&home) {
            let env = home.env(&given, vec![there.clone(), outside_bin.clone()]);
            assert_eq!(
                shadow_note(TOOL, &env, &outside),
                None,
                "{what} (home {}): {} must not be looked into",
                given.display(),
                there.display()
            );
        }
    }
}

fn keeps_data(warnings: &[Warning]) -> Vec<(String, bool)> {
    warnings
        .iter()
        .map(|warning| match warning {
            Warning::KeepsData { path, size, .. } => (path.clone(), size.is_some()),
            other => panic!("expected KeepsData, got {other:?}"),
        })
        .collect()
}

#[test]
fn test_what_an_uninstall_leaves_is_never_measured_in_any_protected_place_however_spelled() {
    // `kept_data`: Claude Code keeps `~/.claude`. Made a link into each
    // place, it is named with no size. Had the walk gone in, the planted
    // folder would have a size, and the missing one would not be named
    // at all (nothing there).
    let home = Home::new("kept-data");
    plant(&home);
    let mut cases: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for (n, place) in PROTECTED_IN_HOME.iter().enumerate() {
        for (what, at) in spellings(&home, place, n) {
            cases.push((what, at.join("banager-data"), at.join("banager-missing")));
        }
    }
    for (what, missing) in volume_spellings(&home) {
        cases.push((what, missing.join("claude"), missing.join("other")));
    }
    for given in home.homes() {
        for (what, there, missing) in &cases {
            for target in [there, missing] {
                home.link(".claude", target);
                let warnings = kept_data(&given, "claude-code", &[], SizeBudget::default());
                assert_eq!(
                    keeps_data(&warnings),
                    vec![("~/.claude".to_string(), false)],
                    "{what} (home {}): ~/.claude -> {} is named, never measured",
                    given.display(),
                    target.display()
                );
            }
        }
    }
}

fn npm_instance(id: &str, prefix: &Path) -> ManagerInstance {
    ManagerInstance {
        prefix: prefix.to_path_buf(),
        ..manager_instance("npm", id)
    }
}

fn npm_package(id: &str, name: &str) -> InstalledArtifact {
    InstalledArtifact {
        key: ArtifactKey {
            instance_id: id.to_string(),
            kind: ArtifactKind::Package,
            name: name.to_string(),
        },
        display_name: name.to_string(),
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
    }
}

#[test]
fn test_disk_use_never_measures_a_tool_in_any_protected_place_however_spelled() {
    // `SizeMeter`: one npm prefix per spelling of each place, each with a
    // package of 64 KB, and one outside to show the round ran. Had a walk
    // gone in, the package would have a size.
    let home = Home::new("sizes");
    plant(&home);
    home.file("outside/npm/lib/node_modules/pkg/index.js", 64 * 1024);
    let mut instances = vec![npm_instance("npm:outside", &home.path.join("outside/npm"))];
    let mut artifacts = vec![npm_package("npm:outside", "pkg")];
    let mut whats = Vec::new();
    for (n, place) in PROTECTED_IN_HOME.iter().enumerate() {
        for (what, at) in spellings(&home, place, n) {
            let id = format!("npm:{}", whats.len());
            instances.push(npm_instance(&id, &at.join("banager-npm")));
            artifacts.push(npm_package(&id, "pkg"));
            whats.push((id, what));
        }
    }
    for given in home.homes() {
        let meter = SizeMeter::new(SizeBudget::default(), |_| {});
        meter.measure(1, &instances, &artifacts, &given);
        let started = Instant::now();
        let sizes = loop {
            let sizes = meter.sizes();
            if sizes.done {
                break sizes;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "the round never ended"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let measured = |id: &str| {
            sizes
                .artifacts
                .iter()
                .find(|size| size.key.instance_id == id)
                .and_then(|size| size.measured)
        };
        assert!(
            measured("npm:outside").is_some_and(|m| m.bytes > 0),
            "the round ran: {sizes:?}"
        );
        for (id, what) in &whats {
            assert_eq!(
                measured(id),
                None,
                "{what} (home {}): never measured",
                given.display()
            );
        }
    }
}
