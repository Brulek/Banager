//! The places Banager never reads into, whatever it is doing: the folders
//! macOS asks the user about before an app looks inside them, and every
//! other disk. One list, one rule for case, shared by the two read-only
//! walks that could otherwise reach them -- which copy a command runs
//! (`commands::read_folders`, over `PATH`'s folders) and how much a tool
//! takes on disk (`size::Protected`, over a tool's own folders).
//!
//! A refresh must never put up a permission request, nor wait on a disk
//! that is not this Mac's own: reading one of these places can do either.
//! `docs/what-we-run.md` names each one in both sections
//! (`what_we_run_test`).

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// The folders under the home folder macOS asks the user about before an
/// app reads them (System Settings > Privacy & Security): Files and
/// Folders' Desktop, Documents and Downloads; the media libraries in
/// Pictures, Movies and Music; iCloud Drive (`Library/Mobile Documents`)
/// and the folders of apps such as Dropbox that keep files in the cloud
/// (`Library/CloudStorage`); and other apps' data (`Library/Containers`,
/// `Library/Group Containers`, App Management's data from other apps).
/// Compared without regard to case (`starts_with_folded`), as APFS
/// compares names by default.
pub const PROTECTED_IN_HOME: [&str; 10] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Library/Containers",
    "Library/Group Containers",
];

/// Where macOS mounts every other volume -- external disks, disk images,
/// network shares -- which are never read either: reading one can make
/// macOS ask, a network disk that went away does not answer at all, and
/// none of it is this Mac's own disk.
pub const OTHER_VOLUMES: &str = "/Volumes";

/// Every protected place for the home folder as each of `homes` names it
/// (the home folder as given, and where it leads): `OTHER_VOLUMES`, then
/// each of `PROTECTED_IN_HOME` under each home.
pub fn places(homes: &[PathBuf]) -> Vec<PathBuf> {
    let mut places = vec![PathBuf::from(OTHER_VOLUMES)];
    for home in homes {
        places.extend(PROTECTED_IN_HOME.iter().map(|place| home.join(place)));
    }
    places
}

/// Whether `path` is one of `places` or inside one, case aside.
pub fn is_within(path: &Path, places: &[PathBuf]) -> bool {
    places.iter().any(|place| starts_with_folded(path, place))
}

/// `Path::starts_with`, comparing each component without regard to ASCII
/// case: on a case-insensitive APFS volume `~/documents` is `~/Documents`,
/// and `/volumes/Backup` is `/Volumes/Backup`.
pub fn starts_with_folded(path: &Path, prefix: &Path) -> bool {
    let mut components = path.components();
    prefix.components().all(|wanted| {
        components.next().is_some_and(|component| {
            component
                .as_os_str()
                .as_bytes()
                .eq_ignore_ascii_case(wanted.as_os_str().as_bytes())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn homes() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/Users/you"),
            PathBuf::from("/System/Volumes/Data/Users/you"),
        ]
    }

    #[test]
    fn test_every_place_in_the_list_and_inside_it_is_protected_under_each_home() {
        let places = places(&homes());
        for home in homes() {
            for place in PROTECTED_IN_HOME {
                let folder = home.join(place);
                assert!(is_within(&folder, &places), "{}", folder.display());
                assert!(
                    is_within(&folder.join("bin/tool"), &places),
                    "{}",
                    folder.display()
                );
            }
        }
        assert!(is_within(Path::new("/Volumes"), &places));
        assert!(is_within(Path::new("/Volumes/Backup/bin"), &places));
    }

    #[test]
    fn test_case_does_not_matter_anywhere_in_a_protected_place() {
        let places = places(&homes());
        for path in [
            "/Users/you/documents/bin",
            "/Users/you/DESKTOP",
            "/users/YOU/Downloads/x",
            "/Users/you/library/containers/com.example/Data/bin",
            "/Users/you/Library/group containers/group.example",
            "/Users/you/LIBRARY/Mobile documents/com~apple~CloudDocs/bin",
            "/volumes/Backup/bin",
            "/VOLUMES",
        ] {
            assert!(is_within(Path::new(path), &places), "{path}");
        }
    }

    #[test]
    fn test_a_name_that_only_begins_like_a_protected_one_is_not_protected() {
        let places = places(&homes());
        for path in [
            "/Users/you/DocumentsBackup/bin",
            "/Users/you/Library/ContainersOld",
            "/Users/you/Library/Application Support/bin",
            "/Users/you/.local/bin",
            "/Users/someone-else/Documents/bin",
            "/VolumesX/bin",
            "/opt/homebrew/bin",
            "/Users/you",
            "/",
        ] {
            assert!(!is_within(Path::new(path), &places), "{path}");
        }
    }

    #[test]
    fn test_the_list_holds_both_walks_places_each_once() {
        // The union of what `commands` and `size` kept apart before they
        // shared this list: none dropped, none twice.
        for place in [
            "Desktop",
            "Documents",
            "Downloads",
            "Movies",
            "Music",
            "Pictures",
            "Library/CloudStorage",
            "Library/Containers",
            "Library/Group Containers",
            "Library/Mobile Documents",
        ] {
            assert_eq!(
                PROTECTED_IN_HOME.iter().filter(|p| **p == place).count(),
                1,
                "{place}"
            );
        }
    }
}
