//! The places Banager never reads into, whatever it is doing: the folders
//! macOS asks the user about before an app looks inside them, and every
//! other disk. One list, one rule for case, shared by the three read-only
//! walks that could otherwise reach them -- which copy a command runs
//! (`commands::read_folders`, over `PATH`'s folders), how much a tool
//! takes on disk (`size::Protected`, over a tool's own folders), and the
//! Other Programs scan (`scan::scan_dirs`, over the usual bin folders).
//!
//! A refresh or a scan must never put up a permission request, nor wait
//! on a disk that is not this Mac's own: reading one of these places can
//! do either. `docs/what-we-run.md` names each one in all three sections
//! (`what_we_run_test`).

use crate::dirfd::{Dir, Stat};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

mod round;
pub(crate) use round::Round;

/// The folders under the home folder macOS asks the user about before an
/// app reads them (System Settings > Privacy & Security): Files and
/// Folders' Desktop, Documents and Downloads; the media libraries in
/// Pictures, Movies and Music; iCloud Drive (`Library/Mobile Documents`)
/// and the folders of apps such as Dropbox that keep files in the cloud
/// (`Library/CloudStorage`); and other apps' data (`Library/Containers`,
/// `Library/Group Containers`, App Management's data from other apps).
/// Compared as APFS compares names by default (`starts_with_folded`):
/// without regard to case, and with the characters it takes for ASCII
/// letters taken for them (`AS_ASCII`).
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

/// Where macOS mounts the volume that holds everything that is not the
/// system itself: `/Users` and `/Volumes`, among others, are the same
/// folders spelled from `/` (firmlinks, `/usr/share/firmlinks`), so
/// `/System/Volumes/Data/Users/you/Documents` is `~/Documents` and
/// `/System/Volumes/Data/Volumes` is `/Volumes`. Neither `lstat` nor
/// `readlink` tells: a firmlink is not a symbolic link.
pub const DATA_VOLUME: &str = "/System/Volumes/Data";

/// `path` as `/` spells it: each leading `DATA_VOLUME` taken off, case
/// aside, so that a place is the same place whichever of its two
/// spellings names it. Every check against the places compares this.
pub fn without_data_volume(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();
    while let Some(rest) = strip_prefix_folded(&path, Path::new(DATA_VOLUME)) {
        let below = Path::new("/").join(rest);
        if below == path {
            break;
        }
        path = below;
    }
    path
}

/// Whether `path` is one of `places` or inside one, case aside, whichever
/// spelling of the data volume either names it with (`DATA_VOLUME`).
pub fn is_within(path: &Path, places: &[PathBuf]) -> bool {
    let path = without_data_volume(path);
    places
        .iter()
        .any(|place| starts_with_folded(&path, &without_data_volume(place)))
}

/// The characters an APFS volume that does not tell case apart -- a
/// Mac's own disk, by default -- takes for ASCII letters, as UTF-8, and
/// the letters it takes each for: its case folding is Unicode's full one,
/// so `~/Documentſ` is `~/Documents`, `~/DesKtop` (a Kelvin sign) is
/// `~/Desktop`, and `~/Library/Cloudﬆorage` is `~/Library/CloudStorage`.
/// Found by asking this Mac's disk for every code point
/// (`probe_every_code_point_apfs_takes_for_ascii_letters`): no other
/// character -- no full-width letter, no accented one, no joiner -- is
/// taken for an ASCII letter.
pub const AS_ASCII: [(&str, &str); 11] = [
    ("\u{00DF}", "ss"),  // ß
    ("\u{017F}", "s"),   // ſ, long s
    ("\u{1E9E}", "ss"),  // ẞ
    ("\u{212A}", "k"),   // K, Kelvin sign
    ("\u{FB00}", "ff"),  // ﬀ
    ("\u{FB01}", "fi"),  // ﬁ
    ("\u{FB02}", "fl"),  // ﬂ
    ("\u{FB03}", "ffi"), // ﬃ
    ("\u{FB04}", "ffl"), // ﬄ
    ("\u{FB05}", "st"),  // ﬅ
    ("\u{FB06}", "st"),  // ﬆ
];

/// A name's bytes as APFS compares them, as far as ASCII goes: each ASCII
/// letter in lower case, each of `AS_ASCII` as its letters, and every
/// other byte as it is. Other letters are compared as spelled: the names
/// macOS gives accounts, and so their home folders, are ASCII.
struct Fold<'a> {
    rest: &'a [u8],
    letters: &'static [u8],
}

impl Iterator for Fold<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if let Some((&letter, more)) = self.letters.split_first() {
            self.letters = more;
            return Some(letter);
        }
        let (&byte, rest) = self.rest.split_first()?;
        if !byte.is_ascii() {
            let rest = self.rest;
            if let Some((utf8, letters)) = AS_ASCII
                .iter()
                .find(|(utf8, _)| rest.starts_with(utf8.as_bytes()))
            {
                self.rest = &rest[utf8.len()..];
                let (&first, more) = letters.as_bytes().split_first()?;
                self.letters = more;
                return Some(first);
            }
        }
        self.rest = rest;
        Some(byte.to_ascii_lowercase())
    }
}

fn fold(bytes: &[u8]) -> Fold<'_> {
    Fold {
        rest: bytes,
        letters: &[],
    }
}

/// Whether `a` and `b` are one name, or one path, as APFS compares names
/// (`Fold`): `~/documents` is `~/Documents`, and `~/Documentſ` is too.
pub fn same_name(a: &[u8], b: &[u8]) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    fold(a).eq(fold(b))
}

/// `name` as a key two spellings of it that `same_name` takes as one
/// share.
pub fn folded_name(name: &[u8]) -> Vec<u8> {
    if name.is_ascii() {
        return name.to_ascii_lowercase();
    }
    fold(name).collect()
}

/// `Path::starts_with`, comparing each component as APFS compares names
/// (`same_name`): on a case-insensitive APFS volume `~/documents` is
/// `~/Documents`, so is `~/Documentſ`, and `/volumes/Backup` is
/// `/Volumes/Backup`.
pub fn starts_with_folded(path: &Path, prefix: &Path) -> bool {
    let mut components = path.components();
    prefix.components().all(|wanted| {
        components.next().is_some_and(|component| {
            same_name(
                component.as_os_str().as_bytes(),
                wanted.as_os_str().as_bytes(),
            )
        })
    })
}

/// Whether `a` and `b` are one path on a Mac's disk, which compares names
/// as `same_name` does: `resolve` keeps each name as it was given or as
/// a link's text spells it, where `realpath` would answer the disk's own
/// spelling, so `~/.CARGO/bin` and `~/.cargo/bin` must compare equal.
/// For paths `resolve` built (no `.`, `..` or doubled `/`).
pub fn same_path(a: &Path, b: &Path) -> bool {
    same_name(a.as_os_str().as_bytes(), b.as_os_str().as_bytes())
}

/// `path` as a key two spellings of it that `same_path` takes as one
/// share.
pub fn folded(path: &Path) -> Vec<u8> {
    folded_name(path.as_os_str().as_bytes())
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
            places: places(&homes)
                .iter()
                .map(|place| without_data_volume(place))
                .collect(),
        }
    }

    /// Whether `path` is one of the places or inside one: `is_within`,
    /// with the places spelled from `/` once, when they were made, rather
    /// than at every call (`without_data_volume` of a path already
    /// spelled from `/` is that path).
    ///
    /// If a path is not inside a place, neither is any folder on its way:
    /// a place a folder is in, the path below that folder is in too.
    pub fn contains(&self, path: &Path) -> bool {
        let path = without_data_volume(path);
        self.places
            .iter()
            .any(|place| starts_with_folded(&path, place))
    }

    /// Whether one of the places is inside `path` (or is it): walking
    /// `path` would reach it, whichever spelling of the data volume either
    /// names it with (`DATA_VOLUME`).
    pub fn under(&self, path: &Path) -> bool {
        let path = without_data_volume(path);
        self.places
            .iter()
            .any(|place| starts_with_folded(place, &path))
    }
}

/// What `resolve` found at a path.
#[derive(Clone, Debug)]
pub enum Resolution {
    /// The path with every link on the way followed, and what is there
    /// (`fstatat` without following: a link at the end, when not
    /// followed, is the link).
    Found(PathBuf, Stat),
    Missing,
    /// It is, or leads, into a protected place: nothing there was looked
    /// at, so nobody knows what is there. The path it leads to as far as
    /// the links outside were followed, the rest of it taken as written
    /// (`..` folded by name).
    Protected(PathBuf),
    /// A folder on the way could not be read or searched, or was replaced
    /// while it was looked at; or it is not absolute; or too many links.
    Refused,
}

/// A test's hook, called with the path reached before each step of
/// `resolve`: where a test changes the disk between two of them.
#[cfg(test)]
type StepHook = Box<dyn FnMut(&Path)>;

#[cfg(test)]
thread_local! {
    static STEP: std::cell::RefCell<Option<StepHook>> = const { std::cell::RefCell::new(None) };
}

/// `path`, with each link among its folders followed, one component at a
/// time, so that no step is ever taken into a protected place: each next
/// component is checked against `protected` before it is looked at, and a
/// link's text is read and spliced in before anything it names is looked
/// at. The last component is followed only with `follow_last`.
///
/// Every step is taken from the folder before it, held open (`dirfd::Dir`,
/// for search only): `fstatat` and `readlinkat` of a name in it, and
/// `openat` of the next folder with `O_NOFOLLOW`, checked to be the folder
/// `fstatat` saw. No folder already checked is looked up again by its
/// path, so one replaced by a link in the meantime is never followed:
/// the step is refused instead. Reads nothing but those, of the folders
/// and links on the way.
///
/// Judging which copy a command runs asks this of thousands of paths in
/// one round; `Round` answers the same for each, looking each folder and
/// name up once in the round.
pub fn resolve(path: &Path, protected: &Protected, follow_last: bool) -> Resolution {
    if !path.is_absolute() {
        return Resolution::Refused;
    }
    let Ok(root) = Dir::root() else {
        return Resolution::Refused;
    };
    // `dirs[i]` is open on the folder `resolved` names at depth `i`.
    let mut dirs: Vec<Dir> = vec![root];
    let mut pending: VecDeque<OsString> = names(path).collect();
    let mut resolved = PathBuf::from("/");
    let mut found: Option<Stat> = None;
    let mut links = 0;
    while let Some(name) = pending.pop_front() {
        #[cfg(test)]
        STEP.with(|step| {
            if let Some(step) = step.borrow_mut().as_mut() {
                step(&resolved);
            }
        });
        if name == ".." {
            resolved.pop();
            if dirs.len() > 1 {
                dirs.pop();
            }
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
        let Some(here) = dirs.last() else {
            return Resolution::Refused;
        };
        let stat = match here.stat_at(&name) {
            Ok(stat) => stat,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Resolution::Missing,
            Err(_) => return Resolution::Refused,
        };
        if stat.is_symlink() && (!pending.is_empty() || follow_last) {
            links += 1;
            if links > MAX_LINKS {
                return Resolution::Refused;
            }
            let Ok(target) = here.read_link_at(&name) else {
                return Resolution::Refused;
            };
            if target.is_absolute() {
                resolved = PathBuf::from("/");
                dirs.truncate(1);
            }
            let spliced: Vec<OsString> = names(&target).collect();
            for name in spliced.into_iter().rev() {
                pending.push_front(name);
            }
            found = None;
            continue;
        }
        if !pending.is_empty() {
            // A folder on the way: stepped into through its descriptor,
            // the very one just looked at. Not a folder, or not one this
            // may search, is where the kernel's own lookup stops too.
            if !stat.is_dir() {
                return Resolution::Refused;
            }
            match here.open_dir_at(&name, Some(&stat), false) {
                Ok((next, _)) => dirs.push(next),
                Err(_) => return Resolution::Refused,
            }
        }
        resolved = candidate;
        found = Some(stat);
    }
    match found {
        Some(stat) => Resolution::Found(resolved, stat),
        // Ended on `..` (or is the root): the folder reached, already
        // checked on the way down, and open.
        None => match dirs.last().map(Dir::stat) {
            Some(Ok(stat)) => Resolution::Found(resolved, stat),
            _ => Resolution::Refused,
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
            // The characters APFS takes for ASCII letters (`AS_ASCII`).
            "/Users/you/Documents/bin",
            "/Users/you/Document\u{17F}/bin",
            "/Users/you/DOCUMENT\u{17F}",
            "/Users/you/De\u{17F}ktop",
            "/Users/you/Des\u{212A}top/x",
            "/Users/you/Download\u{17F}",
            "/Users/you/Picture\u{17F}",
            "/Users/you/Movie\u{17F}",
            "/Users/you/Mu\u{17F}ic",
            "/Users/you/Library/Mobile Document\u{17F}/x",
            "/Users/you/Library/Cloud\u{FB06}orage/Dropbox",
            "/Users/you/Library/Cloud\u{FB05}orage",
            "/Users/you/Library/Cloud\u{17F}torage",
            "/Users/you/Library/Container\u{17F}/com.example",
            "/Users/you/Library/Group Container\u{17F}",
            "/Volume\u{17F}/Backup/bin",
            "/Sy\u{17F}tem/Volume\u{17F}/Data/Users/you/Documents",
        ] {
            assert!(is_within(Path::new(path), &places), "{path}");
        }
        // A home folder whose own name has such letters, spelled either
        // way: `ſtrauß` is `strauss` to the disk.
        let strauss = super::places(&[PathBuf::from("/Users/strauss")]);
        for path in [
            "/Users/\u{17F}trau\u{DF}/Documents",
            "/Users/STRAU\u{1E9E}/Desktop",
        ] {
            assert!(is_within(Path::new(path), &strauss), "{path}");
        }
        let protected = Protected {
            places: super::places(&[PathBuf::from("/Users/\u{17F}trau\u{DF}")])
                .iter()
                .map(|place| without_data_volume(place))
                .collect(),
        };
        assert!(protected.contains(Path::new("/Users/strauss/Documents/bin")));
    }

    #[test]
    fn test_contains_answers_a_table_written_out_by_hand() {
        // Each answer written here, not worked out by the code under test:
        // what a Mac's disk takes for the same folder as a protected one.
        let protected = Protected {
            places: places(&[PathBuf::from("/Users/you")])
                .iter()
                .map(|place| without_data_volume(place))
                .collect(),
        };
        for (path, inside) in [
            ("/Users/you/Documents", true),
            ("/Users/you/Documents/proj/bin/tool", true),
            ("/users/YOU/documents", true),
            ("/Users/you/Document\u{17F}", true),
            ("/Users/you/Des\u{212A}top/bin", true),
            ("/Users/you/DE\u{17F}\u{212A}TOP", true),
            ("/Users/you/Library/Cloud\u{FB06}orage", true),
            ("/Users/you/Library/CLOUD\u{FB05}ORAGE/x", true),
            ("/Users/you/Library/Mobile Documents", true),
            (
                "/Users/you/Library/Mobile Document\u{17F}/com~apple~CloudDocs",
                true,
            ),
            ("/Users/you/Library/Containers/x", true),
            ("/Users/you/Library/Group Containers", true),
            ("/Users/you/Pictures", true),
            ("/Users/you/Movies", true),
            ("/Users/you/Music", true),
            ("/Users/you/Downloads", true),
            ("/Volumes", true),
            ("/Volumes/Backup/bin", true),
            ("/Volume\u{17F}/Backup", true),
            ("/System/Volumes/Data/Users/you/Desktop", true),
            ("/System/Volumes/Data/Volumes/x", true),
            ("/Sy\u{17F}tem/Volumes/Data/Users/you/Music/x", true),
            (
                "/System/Volumes/Data/System/Volumes/Data/Users/you/Movies",
                true,
            ),
            ("/Users/you", false),
            ("/Users/you/Library", false),
            ("/Users/you/Library/Application Support/bin", false),
            ("/Users/you/DocumentsBackup", false),
            ("/Users/you/Document", false),
            // Not the same name to the disk: an extra letter, a full-width
            // or an accented one.
            ("/Users/you/Document\u{DF}", false),
            ("/Users/you/Document\u{FB06}", false),
            ("/Users/you/\u{FF24}ocuments", false),
            ("/Users/you/Docume\u{301}nts", false),
            ("/Users/you/Docum\u{E9}nts", false),
            ("/Users/you/.cargo/bin", false),
            ("/Users/someone/Documents", false),
            ("/VolumesX", false),
            ("/System/Volumes", false),
            ("/System/Volumes/Data", false),
            ("/System/Volumes/Data/Users/you/.local/bin", false),
            ("/opt/homebrew/bin", false),
            ("/", false),
        ] {
            assert_eq!(protected.contains(Path::new(path)), inside, "{path}");
        }
    }

    #[test]
    fn test_each_character_apfs_takes_for_ascii_letters_names_that_folder() {
        // On this Mac's own disk, as `AS_ASCII` says: the folder named
        // with the letters is found under the other spelling. Nothing to
        // show on a disk that tells case apart.
        let temp = Temp::new("as-ascii");
        std::fs::create_dir(temp.0.join("case")).unwrap();
        if !temp.0.join("CASE").exists() {
            return;
        }
        for (character, letters) in AS_ASCII {
            // `ss` and `st` are each two characters' letters.
            let folder = temp.0.join(format!("x{letters}x"));
            std::fs::create_dir_all(&folder).unwrap();
            let other = temp.0.join(format!("x{character}x"));
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::symlink_metadata(&other)
                    .map(|meta| meta.ino())
                    .ok(),
                Some(std::fs::symlink_metadata(&folder).unwrap().ino()),
                "{character:?} is not taken for {letters:?}"
            );
            assert!(same_name(
                other.as_os_str().as_bytes(),
                folder.as_os_str().as_bytes()
            ));
        }
    }

    /// How `AS_ASCII` was found: every code point, asked of this Mac's
    /// disk between two `x`s, against folders named by every ASCII letter,
    /// every two, and `ffi` and `ffl`. About a million lookups, so not run
    /// by default:
    /// `cargo test -p banager-core --release --lib probe_every_code_point -- --ignored`.
    #[test]
    #[ignore = "probe of this Mac's disk: about a million lookups"]
    fn probe_every_code_point_apfs_takes_for_ascii_letters() {
        use std::os::unix::fs::MetadataExt;
        let temp = Temp::new("every-code-point");
        let mut by_ino = std::collections::HashMap::new();
        let letters: Vec<String> = ('a'..='z').map(String::from).collect();
        let mut names: Vec<String> = letters.clone();
        for a in &letters {
            for b in &letters {
                names.push(format!("{a}{b}"));
            }
        }
        names.extend(["ffi".to_string(), "ffl".to_string()]);
        for name in &names {
            let folder = temp.0.join(format!("x{name}x"));
            std::fs::create_dir(&folder).unwrap();
            by_ino.insert(
                std::fs::symlink_metadata(&folder).unwrap().ino(),
                name.clone(),
            );
        }
        let mut found: Vec<(String, String)> = Vec::new();
        for code in 0x80..=0x10FFFFu32 {
            let Some(character) = char::from_u32(code) else {
                continue;
            };
            if let Ok(meta) = std::fs::symlink_metadata(temp.0.join(format!("x{character}x"))) {
                found.push((character.to_string(), by_ino[&meta.ino()].clone()));
            }
        }
        let table: Vec<(String, String)> = AS_ASCII
            .iter()
            .map(|(c, l)| (c.to_string(), l.to_string()))
            .collect();
        assert_eq!(found, table);
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

    #[test]
    fn test_contains_answers_as_is_within_and_never_for_a_folder_alone() {
        // `contains` compares with the places as `Protected::new` spelled
        // them, from `/`; `is_within` spells them again at every call.
        // And no folder on the way to a path outside the places is inside
        // one: what lets one check stand for every step of a lookup.
        let homes = [
            "/Users/you",
            "/System/Volumes/Data/Users/you",
            "/system/volumes/data/System/Volumes/Data/Users/you",
            "/",
            "/System",
        ];
        let mut paths: Vec<PathBuf> = Vec::new();
        for start in [
            "/",
            "/System/Volumes/Data/",
            "/system/VOLUMES/data/System/Volumes/Data/",
        ] {
            for rest in [
                "",
                "System",
                "System/Volumes",
                "System/Volumes/Data",
                "Users/you",
                "users/YOU/documents/bin/tool",
                "Users/you/Library",
                "Users/you/Library/Mobile Documents/x",
                "Users/you/LibraryX/Containers",
                "Users/you/.cargo/bin/rg",
                "Volumes",
                "volumes/Backup/bin",
                "VolumesX",
                "Desktop",
                "System/Desktop",
                "System/Volumes/Desktop",
                "opt/homebrew/Cellar/x/1.0/bin/x",
            ] {
                paths.push(PathBuf::from(format!("{start}{rest}")));
            }
        }
        for home in homes {
            let raw = places(&[PathBuf::from(home)]);
            let protected = Protected {
                places: raw.iter().map(|place| without_data_volume(place)).collect(),
            };
            for path in &paths {
                assert_eq!(
                    protected.contains(path),
                    is_within(path, &raw),
                    "{home}: {path:?}"
                );
                if !protected.contains(path) {
                    for folder in path.ancestors() {
                        assert!(!protected.contains(folder), "{home}: {path:?} {folder:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn test_the_data_volume_spelling_of_a_protected_place_is_protected() {
        // `/Users` and `/Volumes` are firmlinks into the data volume: the
        // same folders, which neither `lstat` nor `readlink` says.
        for home in ["/Users/you", "/System/Volumes/Data/Users/you"] {
            let protected = Protected {
                places: places(&[PathBuf::from(home)])
                    .iter()
                    .map(|place| without_data_volume(place))
                    .collect(),
            };
            for path in [
                "/System/Volumes/Data/Users/you/Documents",
                "/System/Volumes/Data/Users/you/Documents/proj/bin",
                "/system/volumes/DATA/users/you/library/containers/x",
                "/System/Volumes/Data/Volumes",
                "/System/Volumes/Data/Volumes/Backup/bin",
                "/System/Volumes/Data/System/Volumes/Data/Users/you/Desktop",
                "/Users/you/Documents/bin",
                "/Volumes/Backup",
            ] {
                assert!(protected.contains(Path::new(path)), "{home}: {path}");
            }
            for path in [
                "/System/Volumes/Data/Users/you/.cargo/bin",
                "/System/Volumes/Data/opt/homebrew/bin",
                "/System/Volumes/DataX/Users/you/Documents",
                "/System/Volumes/Data",
            ] {
                assert!(!protected.contains(Path::new(path)), "{home}: {path}");
            }
            // Walking the data volume, or a home spelled on it, reaches them.
            for path in [
                "/System/Volumes/Data",
                "/System/Volumes/Data/Users",
                "/System/Volumes/Data/Users/you",
                "/System/Volumes/Data/Users/you/Library",
            ] {
                assert!(protected.under(Path::new(path)), "{home}: {path}");
            }
            assert!(!protected.under(Path::new("/System/Volumes/Data/opt/homebrew")));
        }
    }

    #[test]
    fn test_resolve_stops_at_the_data_volume_spelling_of_the_real_home() {
        // On a Mac, `/System/Volumes/Data<home>` is the home folder itself;
        // `resolve` must stop before `Documents` there, as it does under
        // `<home>`. Only folders above it are `lstat`ed.
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let Ok(below_root) = home.strip_prefix("/") else {
            return;
        };
        let aliased = Path::new(DATA_VOLUME).join(below_root);
        if !aliased.is_dir() {
            return;
        }
        let protected = Protected::new(&home);
        let path = aliased.join("Documents/banager-test-no-such-folder/bin");
        let resolution = resolve(&path, &protected, true);
        assert!(
            matches!(&resolution, Resolution::Protected(at) if *at == path),
            "{resolution:?}"
        );
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

    /// `resolve` of `path` while `swap_at` is reached: there, `<home>/a`
    /// is moved aside and a link to `<home>/Documents` put in its place.
    fn resolve_while_a_is_swapped(home: &Temp, path: &Path, swap_at: &Path) -> Resolution {
        let root = home.0.clone();
        let swap_at = swap_at.to_path_buf();
        STEP.with(|step| {
            *step.borrow_mut() = Some(Box::new(move |reached: &Path| {
                if reached == swap_at && root.join("a").is_dir() && !root.join("a").is_symlink() {
                    std::fs::rename(root.join("a"), root.join("a-moved")).unwrap();
                    std::os::unix::fs::symlink(root.join("Documents"), root.join("a")).unwrap();
                }
            }));
        });
        let found = resolve(path, &Protected::new(&home.0), true);
        STEP.with(|step| *step.borrow_mut() = None);
        found
    }

    #[test]
    fn test_resolve_never_looks_an_ancestor_up_again_after_it_is_swapped_for_a_link() {
        use std::os::unix::fs::MetadataExt;
        // `<home>/a` and `<home>/Documents` hold the same names; once `a`
        // is a link to `Documents`, a lookup by path would land there.
        let set_up = |tag: &str| {
            let home = Temp::new(tag);
            for top in ["a", "Documents"] {
                std::fs::create_dir_all(home.0.join(top).join("b")).unwrap();
                std::fs::write(home.0.join(top).join("c"), top).unwrap();
                std::fs::write(home.0.join(top).join("b/d"), top).unwrap();
            }
            let c = std::fs::metadata(home.0.join("a/c")).unwrap().ino();
            let d = std::fs::metadata(home.0.join("a/b/d")).unwrap().ino();
            (home, c, d)
        };

        // Back up through `..` to a folder swapped since it was entered:
        // the folder held open, not the link now at its path.
        let (home, c, _) = set_up("swap-dotdot");
        let path = home.0.join("a/b/../c");
        let found = resolve_while_a_is_swapped(&home, &path, &home.0.join("a/b"));
        assert!(home.0.join("a").is_symlink(), "the swap happened");
        // By its path, `a/c` is now `Documents/c`.
        assert_ne!(std::fs::metadata(home.0.join("a/c")).unwrap().ino(), c);
        assert!(
            matches!(&found, Resolution::Found(at, stat) if *at == home.0.join("a/c") && stat.ino() == c),
            "{found:?}"
        );

        // On down from a folder swapped once it was entered.
        let (home, _, d) = set_up("swap-down");
        let path = home.0.join("a/b/d");
        let found = resolve_while_a_is_swapped(&home, &path, &home.0.join("a"));
        assert!(home.0.join("a").is_symlink(), "the swap happened");
        assert_ne!(std::fs::metadata(home.0.join("a/b/d")).unwrap().ino(), d);
        assert!(
            matches!(&found, Resolution::Found(at, stat) if *at == home.0.join("a/b/d") && stat.ino() == d),
            "{found:?}"
        );
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
