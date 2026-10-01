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

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::Metadata;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

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

/// Whether `a` and `b` are one path on a Mac's disk, whose names do not
/// tell ASCII case apart: `resolve` keeps each name as it was given or as
/// a link's text spells it, where `realpath` would answer the disk's own
/// spelling, so `~/.CARGO/bin` and `~/.cargo/bin` must compare equal.
/// For paths `resolve` built (no `.`, `..` or doubled `/`).
pub fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str()
        .as_bytes()
        .eq_ignore_ascii_case(b.as_os_str().as_bytes())
}

/// `path` as a key two spellings of it that `same_path` takes as one
/// share.
pub fn folded(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_ascii_lowercase()
}

/// `Path::strip_prefix`, as `same_path` compares: what is left of `path`
/// below `prefix`, or `None` when it is not below it.
pub fn strip_prefix_folded<'a>(path: &'a Path, prefix: &Path) -> Option<&'a Path> {
    if !starts_with_folded(path, prefix) {
        return None;
    }
    let mut rest = path.components();
    for _ in prefix.components() {
        rest.next();
    }
    Some(rest.as_path())
}

/// The most links followed on the way to one path, as the kernel's own
/// limit for a path (`MAXSYMLINKS`).
const MAX_LINKS: u32 = 32;

/// The places neither walk enters, for one home folder: each of
/// `PROTECTED_IN_HOME` under it -- spelled as given and with its own links
/// followed -- and `OTHER_VOLUMES`. What `resolve` checks every step
/// against.
#[derive(Clone, Debug, Default)]
pub struct Protected {
    places: Vec<PathBuf>,
}

impl Protected {
    pub fn new(home: &Path) -> Protected {
        let mut homes = vec![home.to_path_buf()];
        if let Resolution::Found(real, _) = resolve(home, &Protected::default(), true) {
            if real != home {
                homes.push(real);
            }
        }
        Protected {
            places: places(&homes),
        }
    }

    /// Whether `path` is one of the places or inside one.
    pub fn contains(&self, path: &Path) -> bool {
        is_within(path, &self.places)
    }

    /// Whether one of the places is inside `path` (or is it): walking
    /// `path` would reach it.
    pub fn under(&self, path: &Path) -> bool {
        self.places
            .iter()
            .any(|place| starts_with_folded(place, path))
    }
}

/// What `resolve` found at a path.
#[derive(Clone, Debug)]
pub enum Resolution {
    /// The path with every link on the way followed, and what is there
    /// (`lstat`: a link at the end, when not followed, is the link).
    Found(PathBuf, Metadata),
    Missing,
    /// It is, or leads, into a protected place: nothing there was looked
    /// at, so nobody knows what is there. The path it leads to as far as
    /// the links outside were followed, the rest of it taken as written
    /// (`..` folded by name).
    Protected(PathBuf),
    /// A folder on the way could not be read; or it is not absolute; or
    /// too many links.
    Refused,
}

/// `path`, with each link among its folders followed, one component at a
/// time, so that no step is ever taken into a protected place: each next
/// component is checked against `protected` before it is `lstat`ed, and a
/// link's text is read (`readlink`) and spliced in before anything it
/// names is looked at. The last component is followed only with
/// `follow_last`. Reads nothing but `lstat` and `readlink` of the folders
/// and links on the way.
pub fn resolve(path: &Path, protected: &Protected, follow_last: bool) -> Resolution {
    if !path.is_absolute() {
        return Resolution::Refused;
    }
    let mut pending: VecDeque<OsString> = names(path).collect();
    let mut resolved = PathBuf::from("/");
    let mut found: Option<Metadata> = None;
    let mut links = 0;
    while let Some(name) = pending.pop_front() {
        if name == ".." {
            resolved.pop();
            found = None;
            continue;
        }
        let candidate = resolved.join(&name);
        if protected.contains(&candidate) {
            let mut at = candidate;
            for name in pending {
                if name == ".." {
                    at.pop();
                } else {
                    at.push(name);
                }
            }
            return Resolution::Protected(at);
        }
        let meta = match std::fs::symlink_metadata(&candidate) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Resolution::Missing,
            Err(_) => return Resolution::Refused,
        };
        if meta.file_type().is_symlink() && (!pending.is_empty() || follow_last) {
            links += 1;
            if links > MAX_LINKS {
                return Resolution::Refused;
            }
            let Ok(target) = std::fs::read_link(&candidate) else {
                return Resolution::Refused;
            };
            if target.is_absolute() {
                resolved = PathBuf::from("/");
            }
            let spliced: Vec<OsString> = names(&target).collect();
            for name in spliced.into_iter().rev() {
                pending.push_front(name);
            }
            found = None;
            continue;
        }
        resolved = candidate;
        found = Some(meta);
    }
    match found {
        Some(meta) => Resolution::Found(resolved, meta),
        // Ended on `..` (or is the root): the folder reached, already
        // checked on the way down.
        None => match std::fs::symlink_metadata(&resolved) {
            Ok(meta) => Resolution::Found(resolved, meta),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Resolution::Missing,
            Err(_) => Resolution::Refused,
        },
    }
}

/// `path`'s names, `..` kept as a name and `.` and the root dropped.
fn names(path: &Path) -> impl Iterator<Item = OsString> + '_ {
    path.components().filter_map(|component| match component {
        Component::Normal(name) => Some(name.to_os_string()),
        Component::ParentDir => Some(OsString::from("..")),
        Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
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

    /// A fresh folder for one test, canonical (`/var` is a link on a Mac),
    /// removed when the test ends.
    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Temp {
            let raw = std::env::temp_dir().join(format!(
                "banager-protected-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&raw).unwrap();
            Temp(std::fs::canonicalize(&raw).unwrap())
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_two_spellings_that_differ_only_in_case_are_one_path() {
        let a = Path::new("/Users/x/.CARGO/bin/rg");
        let b = Path::new("/Users/x/.cargo/bin/rg");
        assert!(same_path(a, b));
        assert_eq!(folded(a), folded(b));
        assert!(!same_path(a, Path::new("/Users/x/.cargo/bin/fd")));
        assert_eq!(
            strip_prefix_folded(a, Path::new("/users/X/.cargo")),
            Some(Path::new("bin/rg"))
        );
        assert_eq!(strip_prefix_folded(a, Path::new("/Users/x/.cargo2")), None);
    }

    #[test]
    fn test_resolve_stops_at_a_protected_place_without_looking_inside() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let home = Temp::new("resolve");
        let inside = home.0.join("Documents/proj");
        std::fs::create_dir_all(inside.join("bin")).unwrap();
        symlink(&inside, home.0.join("proj")).unwrap();
        symlink(
            "Documents/proj/bin/../bin/cli",
            home.0.join("proj-cli-relative"),
        )
        .unwrap();
        std::fs::create_dir_all(home.0.join("lib")).unwrap();
        // Locked: a step into it would fail (`Refused`), not be refused
        // before it is taken (`Protected`).
        std::fs::set_permissions(&inside, std::fs::Permissions::from_mode(0o000)).unwrap();
        let protected = Protected::new(&home.0);
        let through_link = resolve(&home.0.join("proj/bin/cli"), &protected, true);
        let relative = resolve(&home.0.join("lib/../proj-cli-relative"), &protected, true);
        let onto_volumes = resolve(
            Path::new("/Volumes/Banager-test-no-such-disk/x"),
            &protected,
            true,
        );
        std::fs::set_permissions(&inside, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(&through_link, Resolution::Protected(at) if *at == inside.join("bin/cli")),
            "{through_link:?}"
        );
        assert!(
            matches!(&relative, Resolution::Protected(at) if *at == inside.join("bin/cli")),
            "{relative:?}"
        );
        assert!(
            matches!(&onto_volumes, Resolution::Protected(at) if at == Path::new("/Volumes/Banager-test-no-such-disk/x")),
            "{onto_volumes:?}"
        );
        // Outside the places, where a path leads is found as `realpath`
        // finds it, a path ending in `..` too.
        let lib = resolve(&home.0.join("lib/.."), &protected, true);
        assert!(
            matches!(&lib, Resolution::Found(at, meta) if *at == home.0 && meta.is_dir()),
            "{lib:?}"
        );
        assert!(matches!(
            resolve(&home.0.join("gone"), &protected, true),
            Resolution::Missing
        ));
    }
}
