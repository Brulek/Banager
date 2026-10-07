//! One look at one path, as the `std::fs` call it stands for looks --
//! `lstat`, `stat` and `realpath`, `readlink`, a folder's names, a small
//! file's bytes, `access(2)` -- taken one step at a time from `/` and
//! never into or through a protected place (`protected::resolve`), for
//! the code that looks at a few paths rather than walking a tree: where a
//! tool's own installer put it and the links around it
//! (`adapters::standalone`: `route`, `release_link`, `removal`, `rustup`),
//! the files a tool wrote (`adapters::read_file`), npm's prefix, pip's
//! developer folder, Ollama's app and models, Homebrew's prefix and the
//! apps its casks installed. A path there may be a link a person made --
//! `~/.codex` kept in iCloud Drive, `~/.local` in `~/Documents` -- and a
//! lookup by the path would follow it there, where macOS asks the user
//! before an app looks.
//!
//! Each answers what its `std::fs` call answers, but for two things:
//!
//! - a path that is, or leads into, a protected place is an error of its
//!   own (`is_protected`), of kind `PermissionDenied` -- what macOS's own
//!   refusal would be, never `NotFound`, so nobody takes it for "not
//!   there" -- and nothing in the place is looked at;
//! - a path is spelled as it was given and as each link's text spells it,
//!   not as the disk spells it (`realpath` answers the disk's case), so
//!   two answers are compared with `protected::same_path` and
//!   `protected::starts_with_folded`, as the disk compares names.
//!
//! The other errors keep `NotFound` -- a name on the way, or at the end,
//! that is not there, a link to nothing among them -- and otherwise the
//! system's own reason, kind and words (a folder on the way that may not
//! be searched, that is not a folder, or that was replaced while it was
//! looked at, too many links), each naming the path, for a log.

use super::{resolve_saying_why, Protected, Resolution};
use crate::dirfd::{Dir, Stat};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// The error of a path that is, or leads into, a protected place: what is
/// there was not looked at. `at` is the path as far as the walk got -- the
/// links outside the place followed -- the rest taken as written.
#[derive(Debug)]
pub struct InsideAProtectedPlace {
    pub at: PathBuf,
}

impl std::fmt::Display for InsideAProtectedPlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is in a place Banager never looks into",
            self.at.display()
        )
    }
}

impl std::error::Error for InsideAProtectedPlace {}

/// Whether `error` is a look refused because the path is, or leads into,
/// a protected place.
pub fn is_protected(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<InsideAProtectedPlace>())
}

/// Where a look refused as protected (`is_protected`) got to: the path as
/// far as the walk went, the links outside the place followed, the rest
/// as written.
pub fn protected_at(error: &io::Error) -> Option<&Path> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<InsideAProtectedPlace>())
        .map(|inside| inside.at.as_path())
}

fn protected_error(at: PathBuf) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        InsideAProtectedPlace { at },
    )
}

/// `resolve`'s answer as `std::fs` answers, naming `path` in the error,
/// and keeping the system's own reason for a refusal (its kind and its
/// words, for a log).
fn found(path: &Path, protected: &Protected, follow_last: bool) -> io::Result<(PathBuf, Stat)> {
    match resolve_saying_why(path, protected, follow_last) {
        Ok(Resolution::Found(path, stat)) => Ok((path, stat)),
        Ok(Resolution::Missing) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{}: no such file or folder", path.display()),
        )),
        Ok(Resolution::Protected(at)) => Err(protected_error(at)),
        Ok(Resolution::Refused) => Err(io::Error::other(format!(
            "{}: could not be looked up",
            path.display()
        ))),
        Err(why) => Err(io::Error::new(
            why.kind(),
            format!("{}: {why}", path.display()),
        )),
    }
}

/// Where `path` is, its folders' links followed and its own last name
/// not, and what is there -- a link itself, never what it leads to:
/// `lstat`, and where the entry it looked at is.
pub fn entry(path: &Path, protected: &Protected) -> io::Result<(PathBuf, Stat)> {
    found(path, protected, false)
}

/// `lstat(path)`.
pub fn lstat(path: &Path, protected: &Protected) -> io::Result<Stat> {
    entry(path, protected).map(|(_, stat)| stat)
}

/// Where `path` leads, every link on the way and at the end followed,
/// and what is there: `realpath` and `stat`.
pub fn target(path: &Path, protected: &Protected) -> io::Result<(PathBuf, Stat)> {
    found(path, protected, true)
}

/// `realpath(path)`.
pub fn real_path(path: &Path, protected: &Protected) -> io::Result<PathBuf> {
    target(path, protected).map(|(path, _)| path)
}

/// The text of the link `path` (`readlink`): read from the folder it is
/// in, held open, and only when what `lstat` found there is a link -- an
/// `InvalidInput` error otherwise, as `readlink` gives.
pub fn link_text(path: &Path, protected: &Protected) -> io::Result<PathBuf> {
    let (at, stat) = entry(path, protected)?;
    if !stat.is_symlink() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let (folder, name) = Dir::open_parent(&at)?;
    folder.read_link_at(&name)
}

/// A folder open to list (`list`): its names, and what each one is.
pub struct Listing<'p> {
    dir: Dir,
    path: PathBuf,
    protected: &'p Protected,
}

impl Listing<'_> {
    /// Where the folder is, every link on the way to it followed.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Names one at a time, so callers can check their budget before
    /// each read and stop without collecting the rest of the folder.
    pub fn entries(&self) -> io::Result<crate::dirfd::Entries> {
        self.dir.entries()
    }

    /// Every name in the folder, `.` and `..` left out, unsorted.
    pub fn names(&self) -> io::Result<Vec<OsString>> {
        self.dir.entries()?.collect()
    }

    /// What `name` in the folder is, a link itself rather than what it
    /// leads to (`lstat`): never asked of a protected place itself.
    pub fn lstat(&self, name: &std::ffi::OsStr) -> io::Result<Stat> {
        let path = self.path.join(name);
        if self.protected.contains(&path) {
            return Err(protected_error(path));
        }
        self.dir.stat_at(name)
    }
}

/// The folder `path` leads to, open to list (`opendir`): reached from `/`
/// with no link followed (`dirfd`), and the very folder `resolve` found
/// there. A path that leads to anything but a folder is `NotADirectory`.
pub fn list<'p>(path: &Path, protected: &'p Protected) -> io::Result<Listing<'p>> {
    let (real, stat) = target(path, protected)?;
    if !stat.is_dir() {
        return Err(io::Error::from(io::ErrorKind::NotADirectory));
    }
    let (dir, opened) = Dir::open_path(&real, true)?;
    if !opened.same_as(&stat) {
        return Err(io::Error::other("replaced while it was looked at"));
    }
    Ok(Listing {
        dir,
        path: real,
        protected,
    })
}

/// The bytes of the regular file `path` leads to, links followed, and
/// what `fstat` said of it: read from the folder it is in, held open,
/// without waiting on anything that is not a file and only when it is the
/// regular file `resolve` found (`Dir::read_file_at_most`); an error --
/// `NotFound` kept as it is -- when it is not a regular file
/// (`InvalidInput`) or holds more than `limit` bytes (`InvalidData`).
pub fn read_regular(path: &Path, protected: &Protected, limit: u64) -> io::Result<(Stat, Vec<u8>)> {
    let (real, stat) = target(path, protected)?;
    if !stat.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let (folder, name) = Dir::open_parent(&real)?;
    folder.read_file_at_most(&name, Some(&stat), limit)
}

/// `read_regular` of the file `path` itself: a link at its end is not
/// followed (`O_NOFOLLOW`), and is `InvalidInput`, as anything else that
/// is not a regular file is.
pub fn read_entry(path: &Path, protected: &Protected, limit: u64) -> io::Result<(Stat, Vec<u8>)> {
    let (at, stat) = entry(path, protected)?;
    if !stat.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let (folder, name) = Dir::open_parent(&at)?;
    folder.read_file_at_most(&name, Some(&stat), limit)
}

/// The file `path` leads to, links followed, opened to read from the
/// folder it is in, held open, without waiting on anything that is not a
/// file (`Dir::open_file_at`), and what `fstat` says of what was opened:
/// for a caller that asks the open file itself (`fcntl`), and decides
/// what it may be. Only the very thing `resolve` found there is opened.
pub(crate) fn open(path: &Path, protected: &Protected) -> io::Result<(std::fs::File, Stat)> {
    let (real, stat) = target(path, protected)?;
    let (folder, name) = Dir::open_parent(&real)?;
    let (file, opened) = folder.open_file_at(&name)?;
    if !opened.same_as(&stat) {
        return Err(io::Error::other("replaced while it was looked at"));
    }
    Ok((file, opened))
}

/// Whether this account may write in the folder `path` leads to, as
/// `access(path, W_OK)` answers: `false` for anything that is not a
/// folder there, or cannot be looked at -- a protected place among them.
pub fn writable_folder(path: &Path, protected: &Protected) -> bool {
    let Ok((real, stat)) = target(path, protected) else {
        return false;
    };
    if !stat.is_dir() {
        return false;
    }
    Dir::open_path(&real, false)
        .is_ok_and(|(dir, opened)| opened.same_as(&stat) && dir.access(libc::W_OK))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirfd::calls;
    use std::os::unix::fs::{symlink, MetadataExt};

    /// A fresh home folder for one test, canonical, removed when it ends,
    /// with a `Documents` the test fills.
    struct Home(PathBuf);

    impl Home {
        fn new(tag: &str) -> Home {
            let raw = std::env::temp_dir().join(format!(
                "banager-look-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(raw.join("Documents")).unwrap();
            Home(std::fs::canonicalize(&raw).unwrap())
        }

        fn at(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }

        fn protected(&self) -> Protected {
            Protected::new(&self.0)
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// No call `calls` saw looked at a protected place or through one.
    fn nothing_protected_looked_at(calls: &calls::Calls, protected: &Protected) {
        for (call, path) in &calls.paths {
            for at in path.ancestors() {
                assert!(!protected.contains(at), "{call:?} looked at {at:?}");
            }
        }
    }

    /// `~/.tool`, a folder whose `current` links to a release, with a
    /// file and a link of its own; and `~/linked`, a link to the same
    /// layout kept in `~/Documents`.
    fn layout(home: &Home) {
        for top in [".tool", "Documents/tool"] {
            std::fs::create_dir_all(home.at(&format!("{top}/releases/1.0"))).unwrap();
            std::fs::write(home.at(&format!("{top}/marker")), "1.0").unwrap();
            symlink("releases/1.0", home.at(&format!("{top}/current"))).unwrap();
        }
        symlink(home.at("Documents/tool"), home.at("linked")).unwrap();
    }

    #[test]
    fn test_each_look_answers_as_std_fs_outside_the_places() {
        let home = Home::new("outside");
        layout(&home);
        let protected = home.protected();
        let current = home.at(".tool/current");
        let ((at, stat), made) = calls::measure(|| entry(&current, &protected).unwrap());
        assert_eq!(at, current);
        assert!(stat.is_symlink());
        assert_eq!(
            stat.ino(),
            std::fs::symlink_metadata(&current).unwrap().ino()
        );
        nothing_protected_looked_at(&made, &protected);
        assert_eq!(
            real_path(&current, &protected).unwrap(),
            std::fs::canonicalize(&current).unwrap()
        );
        assert!(target(&current, &protected).unwrap().1.is_dir());
        assert_eq!(
            link_text(&current, &protected).unwrap(),
            PathBuf::from("releases/1.0")
        );
        let (meta, bytes) = read_regular(&home.at(".tool/marker"), &protected, 16).unwrap();
        assert_eq!(bytes, b"1.0");
        assert_eq!(meta.size(), 3);
        let listing = list(&home.at(".tool"), &protected).unwrap();
        let mut names = listing.names().unwrap();
        names.sort();
        assert_eq!(names, ["current", "marker", "releases"]);
        assert!(listing.lstat("current".as_ref()).unwrap().is_symlink());
        assert!(writable_folder(&home.at(".tool"), &protected));
        // What `std::fs` says is not there, or is not what was asked.
        let missing = home.at(".tool/gone");
        assert_eq!(
            lstat(&missing, &protected).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            link_text(&home.at(".tool/marker"), &protected)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(read_regular(&home.at(".tool"), &protected, 16).is_err());
        assert_eq!(
            read_regular(&home.at(".tool/marker"), &protected, 2)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(list(&home.at(".tool/marker"), &protected).is_err());
        assert!(!writable_folder(&missing, &protected));
    }

    #[test]
    fn test_a_refused_look_keeps_the_systems_reason_and_names_the_path() {
        // For a log: a folder that may not be searched is the system's
        // permission error, a loop its own, a relative path bad input, and
        // a missing name `NotFound` -- each naming the path asked.
        use std::os::unix::fs::PermissionsExt;
        let home = Home::new("reasons");
        std::fs::create_dir_all(home.at("locked/inner")).unwrap();
        symlink(home.at("loop-b"), home.at("loop-a")).unwrap();
        symlink(home.at("loop-a"), home.at("loop-b")).unwrap();
        let protected = home.protected();
        let missing = lstat(&home.at("gone/x"), &protected).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        assert!(missing.to_string().contains("gone/x"), "{missing}");
        let looping = target(&home.at("loop-a"), &protected).unwrap_err();
        assert!(looping.to_string().contains("loop-a"), "{looping}");
        assert!(looping.to_string().contains("symbolic links"), "{looping}");
        assert_ne!(looping.kind(), io::ErrorKind::NotFound);
        let relative = lstat(Path::new("relative/x"), &protected).unwrap_err();
        assert_eq!(relative.kind(), io::ErrorKind::InvalidInput);
        std::fs::set_permissions(home.at("locked"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
        let locked = lstat(&home.at("locked/inner"), &protected);
        std::fs::set_permissions(home.at("locked"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        // Root may search anything: nothing to show then.
        if let Err(locked) = locked {
            assert_eq!(locked.kind(), io::ErrorKind::PermissionDenied, "{locked}");
            assert!(locked.to_string().contains("locked/inner"), "{locked}");
        }
    }

    #[test]
    fn test_no_look_goes_into_a_protected_place_or_through_a_link_into_one() {
        let home = Home::new("inside");
        layout(&home);
        let protected = home.protected();
        // Into `Documents` by its name, and through `~/linked`: each look
        // refused as protected, and nothing in it looked at.
        for path in [
            "Documents/tool/current",
            "linked/current",
            "linked/marker",
            "linked",
            "Documents",
        ] {
            let path = home.at(path);
            let (answers, made) = calls::measure(|| {
                [
                    lstat(&path, &protected).err(),
                    target(&path, &protected).err(),
                    link_text(&path, &protected).err(),
                    list(&path, &protected).err(),
                    read_regular(&path, &protected, 1024).err(),
                ]
            });
            nothing_protected_looked_at(&made, &protected);
            for (index, error) in answers.into_iter().enumerate() {
                // `lstat` and `readlink` of `~/linked` itself are of the
                // link, outside.
                if path == home.at("linked") && (index == 0 || index == 2) {
                    continue;
                }
                let error = error.unwrap_or_else(|| panic!("{path:?}: look {index} answered"));
                assert!(is_protected(&error), "{path:?}: look {index}: {error}");
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            }
            assert!(!writable_folder(&path, &protected), "{path:?}");
        }
        // The link itself is outside: looked at, never followed.
        assert!(lstat(&home.at("linked"), &protected).unwrap().is_symlink());
        assert_eq!(
            link_text(&home.at("linked"), &protected).unwrap(),
            home.at("Documents/tool")
        );
        // A listing of the home folder names `Documents` but never looks
        // at it.
        let listing = list(&home.0, &protected).unwrap();
        assert!(listing
            .names()
            .unwrap()
            .contains(&OsString::from("Documents")));
        assert!(is_protected(
            &listing.lstat("Documents".as_ref()).unwrap_err()
        ));
    }

    #[test]
    fn test_read_entry_refuses_a_link_but_read_regular_follows_it() {
        let home = Home::new("entry-link");
        std::fs::write(home.at("payload"), b"tool").unwrap();
        symlink("payload", home.at("alias")).unwrap();
        let protected = home.protected();
        let error = read_entry(&home.at("alias"), &protected, 4).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            read_regular(&home.at("alias"), &protected, 4).unwrap().1,
            b"tool"
        );
        assert_eq!(
            read_entry(&home.at("payload"), &protected, 4).unwrap().1,
            b"tool"
        );
    }
}
