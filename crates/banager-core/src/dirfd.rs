//! Folders looked at through an open descriptor, never by their path
//! again: what keeps the read-only walks (`protected::resolve`, `size`'s
//! walk, `commands`' folder reads, the Other Programs scan's listing)
//! where they checked they were.
//!
//! A path is a name, looked up afresh at every call. Between the check
//! that `~/.cargo/registry` is a real folder outside the protected places
//! and the `readdir` of it, another program can put a symbolic link to
//! `~/Documents` in its place, and a call by path would follow it. Here a
//! folder is opened once, with `O_NOFOLLOW`, checked to be the very folder
//! (`st_dev`, `st_ino`) that was looked at before it was opened, and then
//! everything in it is looked at from that descriptor (`fstatat`,
//! `readlinkat`, `openat`, all without following a link at the end): a
//! name replaced in the meantime is either the link itself, never
//! followed, or a different folder, which is refused.
//!
//! Folders on the way are opened for search only (`O_SEARCH`: nothing in
//! them is listed), a folder to list for reading (`O_RDONLY`, as
//! `opendir` does). No file is opened here but by `read_file_at`, for the
//! one file a caller reads (`<CARGO_HOME>/.crates2.json`).

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

/// What `fstatat` or `fstat` said of one thing: the fields the walks use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    dev: u64,
    ino: u64,
    nlink: u64,
    blocks: u64,
    mode: u32,
    size: u64,
    mtime: i64,
    uid: u32,
}

impl Stat {
    // The fields' types differ from one platform to another.
    #[allow(clippy::unnecessary_cast)]
    fn from_raw(st: &libc::stat) -> Stat {
        // As `std::os::unix::fs::MetadataExt` widens them, so the numbers
        // compare equal to a `Metadata`'s.
        Stat {
            dev: st.st_dev as u64,
            ino: st.st_ino as u64,
            nlink: st.st_nlink as u64,
            blocks: st.st_blocks as u64,
            mode: st.st_mode as u32,
            size: st.st_size as u64,
            mtime: st.st_mtime as i64,
            uid: st.st_uid as u32,
        }
    }

    pub fn dev(&self) -> u64 {
        self.dev
    }

    pub fn ino(&self) -> u64 {
        self.ino
    }

    pub fn nlink(&self) -> u64 {
        self.nlink
    }

    /// 512-byte blocks the disk holds for it (`st_blocks`).
    pub fn blocks(&self) -> u64 {
        self.blocks
    }

    /// Its length in bytes (`st_size`): a link's own is its text's.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// When its contents last changed, unix seconds (`st_mtime`).
    pub fn mtime(&self) -> i64 {
        self.mtime
    }

    /// The account that owns it (`st_uid`).
    pub fn uid(&self) -> u32 {
        self.uid
    }

    /// `st_mode`: its kind and its permission bits.
    pub fn mode(&self) -> u32 {
        self.mode
    }

    fn kind(&self) -> u32 {
        self.mode & libc::S_IFMT as u32
    }

    pub fn is_dir(&self) -> bool {
        self.kind() == libc::S_IFDIR as u32
    }

    pub fn is_file(&self) -> bool {
        self.kind() == libc::S_IFREG as u32
    }

    pub fn is_symlink(&self) -> bool {
        self.kind() == libc::S_IFLNK as u32
    }

    /// Whether `other` is the same thing on the same disk.
    pub fn same_as(&self, other: &Stat) -> bool {
        self.dev == other.dev && self.ino == other.ino
    }
}

/// Open for search only: what a folder on the way needs, the execute
/// (search) permission a path lookup needs too, and no more.
#[cfg(target_os = "macos")]
const SEARCH: libc::c_int = libc::O_SEARCH;
#[cfg(not(target_os = "macos"))]
const SEARCH: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY;

fn c_name(name: &OsStr) -> io::Result<CString> {
    if name.is_empty() || name.as_bytes().contains(&b'/') || name == "." || name == ".." {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    CString::new(name.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

/// One open folder.
#[derive(Debug)]
pub struct Dir {
    fd: OwnedFd,
}

impl Dir {
    /// `/`, open for search.
    pub fn root() -> io::Result<Dir> {
        // SAFETY: a NUL-terminated literal path; the result is checked.
        let fd = unsafe { libc::open(c"/".as_ptr(), SEARCH | libc::O_CLOEXEC) };
        Dir::owned(fd)
    }

    fn owned(fd: libc::c_int) -> io::Result<Dir> {
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` was just returned open by the kernel and is owned
        // by nothing else.
        Ok(Dir {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        })
    }

    /// The folder at `path` -- absolute, with no `.` or `..` -- reached
    /// from `/` one folder at a time with no symbolic link followed
    /// anywhere on the way (a link there is refused), each folder checked
    /// to be the one `fstatat` saw before it was opened. Opened to list
    /// (`list`) or for search only. With what `fstat` says of it.
    pub fn open_path(path: &Path, list: bool) -> io::Result<(Dir, Stat)> {
        let names = plain_names(path)?;
        let mut dir = Dir::root()?;
        let Some((last, above)) = names.split_last() else {
            let stat = dir.stat()?;
            return if list {
                Ok((Dir::reopen_root_to_list()?, stat))
            } else {
                Ok((dir, stat))
            };
        };
        for name in above {
            dir = dir.open_dir_at(name, None, false)?.0;
        }
        dir.open_dir_at(last, None, list)
    }

    fn reopen_root_to_list() -> io::Result<Dir> {
        // SAFETY: a NUL-terminated literal path; the result is checked.
        let fd = unsafe {
            libc::open(
                c"/".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        Dir::owned(fd)
    }

    /// The folder holding the last name of `path` -- reached as
    /// `open_path` reaches a folder -- and that name.
    pub fn open_parent(path: &Path) -> io::Result<(Dir, OsString)> {
        let names = plain_names(path)?;
        let Some((last, above)) = names.split_last() else {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        };
        let mut dir = Dir::root()?;
        for name in above {
            dir = dir.open_dir_at(name, None, false)?.0;
        }
        Ok((dir, last.clone()))
    }

    /// What is at `name` in this folder, a link itself rather than what it
    /// leads to (`fstatat` with `AT_SYMLINK_NOFOLLOW`).
    pub fn stat_at(&self, name: &OsStr) -> io::Result<Stat> {
        let name = c_name(name)?;
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `name` is NUL-terminated and `st` has room for a stat;
        // it is read only when the call succeeded.
        let rc = unsafe {
            libc::fstatat(
                self.fd.as_raw_fd(),
                name.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: filled in by the successful call above.
        Ok(Stat::from_raw(unsafe { &st.assume_init() }))
    }

    /// What this folder is (`fstat`).
    pub fn stat(&self) -> io::Result<Stat> {
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `st` has room for a stat; read only on success.
        let rc = unsafe { libc::fstat(self.fd.as_raw_fd(), st.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: filled in by the successful call above.
        Ok(Stat::from_raw(unsafe { &st.assume_init() }))
    }

    /// The text of the link `name` in this folder (`readlinkat`).
    pub fn read_link_at(&self, name: &OsStr) -> io::Result<PathBuf> {
        let name = c_name(name)?;
        let mut buf = vec![0u8; libc::PATH_MAX as usize + 1];
        // SAFETY: `name` is NUL-terminated and `buf` is writable for its
        // length; the count returned is checked against it.
        let len = unsafe {
            libc::readlinkat(
                self.fd.as_raw_fd(),
                name.as_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        let len = len as usize;
        if len >= buf.len() {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        buf.truncate(len);
        Ok(PathBuf::from(OsString::from_vec(buf)))
    }

    /// The folder `name` in this one, opened without following a link
    /// (`openat` with `O_NOFOLLOW` and `O_DIRECTORY`): to list (`list`),
    /// or for search only. With `expected` -- what `stat_at` said of it
    /// before -- it must be that very folder, or it is refused: something
    /// else was put there in between. With what `fstat` says of it.
    pub fn open_dir_at(
        &self,
        name: &OsStr,
        expected: Option<&Stat>,
        list: bool,
    ) -> io::Result<(Dir, Stat)> {
        let c = c_name(name)?;
        let flags = if list {
            libc::O_RDONLY | libc::O_DIRECTORY
        } else {
            SEARCH
        } | libc::O_NOFOLLOW
            | libc::O_CLOEXEC;
        // SAFETY: `c` is NUL-terminated; the result is checked.
        let fd = unsafe { libc::openat(self.fd.as_raw_fd(), c.as_ptr(), flags) };
        let dir = Dir::owned(fd)?;
        let stat = dir.stat()?;
        if !stat.is_dir() || expected.is_some_and(|expected| !expected.same_as(&stat)) {
            return Err(io::Error::other("replaced while it was looked at"));
        }
        Ok((dir, stat))
    }

    /// The names in this folder, as `readdir` gives them from this
    /// descriptor, `.` and `..` left out. The folder must have been opened
    /// to list.
    pub fn entries(&self) -> io::Result<Entries> {
        // SAFETY: duplicates a descriptor this `Dir` owns; checked.
        let dup = unsafe { libc::fcntl(self.fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        if dup < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `dup` is a fresh descriptor owned by nothing else; on
        // success the stream owns it and `closedir` closes it.
        let stream = unsafe { libc::fdopendir(dup) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: `fdopendir` failed, so `dup` is still ours to close.
            unsafe { libc::close(dup) };
            return Err(error);
        }
        Ok(Entries {
            stream,
            done: false,
        })
    }

    /// The regular file `name` in this folder, read whole: opened without
    /// following a link (`O_NOFOLLOW`) and without waiting on anything
    /// that is not a file (`O_NONBLOCK`), and read only when `fstat` says
    /// it is a regular file -- `expected`, when given, the very one.
    pub fn read_file_at(&self, name: &OsStr, expected: Option<&Stat>) -> io::Result<Vec<u8>> {
        let c = c_name(name)?;
        // SAFETY: `c` is NUL-terminated; the result is checked.
        let fd = unsafe {
            libc::openat(
                self.fd.as_raw_fd(),
                c.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a fresh descriptor owned by nothing else.
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        let meta = file.metadata()?;
        let stat = Stat {
            dev: std::os::unix::fs::MetadataExt::dev(&meta),
            ino: std::os::unix::fs::MetadataExt::ino(&meta),
            nlink: std::os::unix::fs::MetadataExt::nlink(&meta),
            blocks: std::os::unix::fs::MetadataExt::blocks(&meta),
            mode: std::os::unix::fs::MetadataExt::mode(&meta),
            size: std::os::unix::fs::MetadataExt::size(&meta),
            mtime: std::os::unix::fs::MetadataExt::mtime(&meta),
            uid: std::os::unix::fs::MetadataExt::uid(&meta),
        };
        if !stat.is_file() || expected.is_some_and(|expected| !expected.same_as(&stat)) {
            return Err(io::Error::other("not the file that was looked at"));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// `path`'s names, for a path that is absolute and has no `.`, `..` or
/// prefix: what `open_path` and `open_parent` take.
fn plain_names(path: &Path) -> io::Result<Vec<OsString>> {
    if !path.is_absolute() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    path.components()
        .filter_map(|component| match component {
            Component::RootDir => None,
            Component::Normal(name) => Some(Ok(name.to_os_string())),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                Some(Err(io::Error::from(io::ErrorKind::InvalidInput)))
            }
        })
        .collect()
}

/// The names `Dir::entries` reads, one at a time, so that a caller can
/// stop part way through a large folder. Closes its stream when dropped.
pub struct Entries {
    stream: *mut libc::DIR,
    done: bool,
}

// SAFETY: the stream is used by one thread at a time (`&mut self`), and
// a `DIR` belongs to no thread.
unsafe impl Send for Entries {}

#[cfg(target_os = "macos")]
fn clear_errno() {
    // SAFETY: the calling thread's own errno.
    unsafe { *libc::__error() = 0 };
}

#[cfg(not(target_os = "macos"))]
fn clear_errno() {
    // SAFETY: the calling thread's own errno.
    unsafe { *libc::__errno_location() = 0 };
}

impl Iterator for Entries {
    type Item = io::Result<OsString>;

    fn next(&mut self) -> Option<io::Result<OsString>> {
        while !self.done {
            clear_errno();
            // SAFETY: `stream` is open until this is dropped.
            let entry = unsafe { libc::readdir(self.stream) };
            if entry.is_null() {
                self.done = true;
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(0) | None => None,
                    Some(_) => Some(Err(error)),
                };
            }
            // SAFETY: `readdir` returned an entry whose `d_name` is a
            // NUL-terminated name, valid until the next call.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            let name = name.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            return Some(Ok(OsString::from_vec(name.to_vec())));
        }
        None
    }
}

impl Drop for Entries {
    fn drop(&mut self) {
        // SAFETY: opened by `fdopendir`, closed once, here.
        unsafe { libc::closedir(self.stream) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt};

    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Temp {
            let raw = std::env::temp_dir().join(format!(
                "banager-dirfd-{tag}-{}-{}",
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
    fn test_a_folder_is_listed_and_looked_at_from_its_descriptor() {
        let temp = Temp::new("list");
        std::fs::create_dir_all(temp.0.join("a/sub")).unwrap();
        std::fs::write(temp.0.join("a/file"), b"x").unwrap();
        symlink("/etc", temp.0.join("a/link")).unwrap();
        let (dir, stat) = Dir::open_path(&temp.0.join("a"), true).unwrap();
        assert!(stat.is_dir());
        assert_eq!(
            stat.ino(),
            std::fs::metadata(temp.0.join("a")).unwrap().ino()
        );
        let mut names: Vec<OsString> = dir.entries().unwrap().map(Result::unwrap).collect();
        names.sort();
        assert_eq!(names, vec!["file", "link", "sub"]);
        assert!(dir.stat_at(OsStr::new("link")).unwrap().is_symlink());
        assert!(dir.stat_at(OsStr::new("file")).unwrap().is_file());
        assert_eq!(
            dir.read_link_at(OsStr::new("link")).unwrap(),
            PathBuf::from("/etc")
        );
        assert_eq!(dir.read_file_at(OsStr::new("file"), None).unwrap(), b"x");
        // A link is never opened as a folder, nor read as a file.
        assert!(dir.open_dir_at(OsStr::new("link"), None, true).is_err());
        assert!(dir.read_file_at(OsStr::new("link"), None).is_err());
    }

    #[test]
    fn test_a_link_anywhere_on_the_way_is_refused() {
        let temp = Temp::new("way");
        std::fs::create_dir_all(temp.0.join("real/inside")).unwrap();
        symlink(temp.0.join("real"), temp.0.join("link")).unwrap();
        assert!(Dir::open_path(&temp.0.join("real/inside"), false).is_ok());
        assert!(Dir::open_path(&temp.0.join("link/inside"), false).is_err());
        assert!(Dir::open_path(&temp.0.join("link"), true).is_err());
        assert!(Dir::open_parent(&temp.0.join("link/inside")).is_err());
        assert!(Dir::open_path(Path::new("relative"), false).is_err());
        assert!(Dir::open_path(&temp.0.join("real/../real"), false).is_err());
    }

    #[test]
    fn test_a_folder_replaced_after_it_was_looked_at_is_refused() {
        // Looked at, then put aside and another folder put in its place:
        // the descriptor opened is not the folder that was checked.
        let temp = Temp::new("swap");
        std::fs::create_dir_all(temp.0.join("tool")).unwrap();
        let (parent, _) = Dir::open_path(&temp.0, false).unwrap();
        let checked = parent.stat_at(OsStr::new("tool")).unwrap();
        std::fs::rename(temp.0.join("tool"), temp.0.join("aside")).unwrap();
        std::fs::create_dir_all(temp.0.join("tool")).unwrap();
        assert!(parent
            .open_dir_at(OsStr::new("tool"), Some(&checked), true)
            .is_err());
        // And a link put there is not followed at all.
        std::fs::remove_dir(temp.0.join("tool")).unwrap();
        symlink(temp.0.join("aside"), temp.0.join("tool")).unwrap();
        assert!(parent
            .open_dir_at(OsStr::new("tool"), Some(&checked), true)
            .is_err());
    }
}
