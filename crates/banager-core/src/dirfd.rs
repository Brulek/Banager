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
//! followed, or a different folder, which is refused. Several names below
//! a folder can be looked at in one call that follows no link anywhere
//! (`stat_beneath`): a link on the way is an error, never followed.
//!
//! Folders on the way are opened for search only (`O_SEARCH`: nothing in
//! them is listed), a folder to list for reading (`O_RDONLY`, as
//! `opendir` does). No file is read here but by `read_file_at_most`, which
//! reads a regular file only up to the size its caller names. A file is
//! opened by `open_file_at`, which reads nothing: for `read_file_at_most`,
//! and for `protected::look::open`, whose one caller asks the open file
//! about a lock (`fcntl`) and reads none of it. Both are this crate's
//! alone, and `safety_source_test` holds production code to the one read
//! to a file's end, `read_file_at_most`'s, which stops past its limit.

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
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
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
            mtime_nsec: st.st_mtime_nsec as i64,
            ctime: st.st_ctime as i64,
            ctime_nsec: st.st_ctime_nsec as i64,
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

    /// The nanoseconds past `mtime` (`st_mtime_nsec`).
    pub fn mtime_nsec(&self) -> i64 {
        self.mtime_nsec
    }

    /// When its own record last changed, unix seconds and the nanoseconds
    /// past them (`st_ctime`, `st_ctime_nsec`).
    pub fn ctime(&self) -> (i64, i64) {
        (self.ctime, self.ctime_nsec)
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

/// `fstatat`'s `AT_SYMLINK_NOFOLLOW_ANY` (`<sys/fcntl.h>`, macOS 11 and
/// later), which the `libc` crate does not name: a symbolic link met
/// anywhere on the way is an error (`ELOOP`), never followed; one at the
/// end is looked at itself. A kernel that does not know the flag refuses
/// the call (`EINVAL`) rather than ignore it.
#[cfg(target_os = "macos")]
const NOFOLLOW_ANY: libc::c_int = 0x0800;

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
        #[cfg(test)]
        calls::note(calls::Call::Root, None, &[]);
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
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
        // SAFETY: a NUL-terminated literal path; the result is checked.
        let fd = unsafe { libc::open(c"/".as_ptr(), flags) };
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
        #[cfg(test)]
        calls::note(calls::Call::StatAt, Some(self), &[name]);
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

    /// What is at `names` below this folder, each one inside the one
    /// before it, with no symbolic link followed anywhere: a link among
    /// the folders on the way is an error (`ELOOP`), and one at the end is
    /// looked at itself (`fstatat` with `AT_SYMLINK_NOFOLLOW_ANY`). The
    /// kernel's one lookup does what a `stat_at` of each name and an
    /// `open_dir_at` of each folder on the way would, with no moment
    /// between a look and the next step for a name to be swapped: it
    /// takes no folder by a name it has not just looked at, and no link
    /// at all. Each name is checked as `stat_at` checks its one (no `/`,
    /// `.` or `..`). Not where the flag is not known (`Unsupported`, or
    /// `EINVAL` from an older kernel): the caller then takes one step at a
    /// time.
    pub fn stat_beneath(&self, names: &[OsString]) -> io::Result<Stat> {
        #[cfg(test)]
        calls::note(
            calls::Call::StatBeneath,
            Some(self),
            &names.iter().map(OsString::as_os_str).collect::<Vec<_>>(),
        );
        if names.is_empty() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut joined: Vec<u8> = Vec::new();
        for name in names {
            c_name(name)?;
            if !joined.is_empty() {
                joined.push(b'/');
            }
            joined.extend_from_slice(name.as_bytes());
        }
        let joined =
            CString::new(joined).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        self.stat_no_link_anywhere(&joined)
    }

    #[cfg(target_os = "macos")]
    fn stat_no_link_anywhere(&self, path: &CStr) -> io::Result<Stat> {
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `path` is NUL-terminated and `st` has room for a stat;
        // it is read only when the call succeeded.
        let rc = unsafe {
            libc::fstatat(
                self.fd.as_raw_fd(),
                path.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW | NOFOLLOW_ANY,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: filled in by the successful call above.
        Ok(Stat::from_raw(unsafe { &st.assume_init() }))
    }

    #[cfg(not(target_os = "macos"))]
    fn stat_no_link_anywhere(&self, _path: &CStr) -> io::Result<Stat> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    /// What this folder is (`fstat`).
    pub fn stat(&self) -> io::Result<Stat> {
        #[cfg(test)]
        calls::note(calls::Call::Stat, Some(self), &[]);
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `st` has room for a stat; read only on success.
        let rc = unsafe { libc::fstat(self.fd.as_raw_fd(), st.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: filled in by the successful call above.
        Ok(Stat::from_raw(unsafe { &st.assume_init() }))
    }

    /// Where this folder is now, as the kernel names it (`fcntl` with
    /// `F_GETPATH`): nothing in the folder is looked at, and no name is
    /// looked up -- a folder renamed since it was opened answers where it
    /// was moved to. Not on other systems (`Unsupported`).
    pub fn path(&self) -> io::Result<PathBuf> {
        let path = self.path_now();
        #[cfg(test)]
        if let Ok(path) = &path {
            calls::note_path(calls::Call::GetPath, path);
        }
        path
    }

    #[cfg(target_os = "macos")]
    fn path_now(&self) -> io::Result<PathBuf> {
        let mut buf = [0u8; libc::PATH_MAX as usize];
        // SAFETY: `buf` has room for `MAXPATHLEN` bytes, as `F_GETPATH`
        // requires; the result is checked.
        let rc = unsafe { libc::fcntl(self.fd.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(PathBuf::from(OsStr::from_bytes(&buf[..len])))
    }

    #[cfg(not(target_os = "macos"))]
    fn path_now(&self) -> io::Result<PathBuf> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    /// The text of the link `name` in this folder (`readlinkat`).
    pub fn read_link_at(&self, name: &OsStr) -> io::Result<PathBuf> {
        #[cfg(test)]
        calls::note(calls::Call::ReadLinkAt, Some(self), &[name]);
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
        #[cfg(test)]
        calls::note(calls::Call::OpenDirAt, Some(self), &[name]);
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

    /// The regular file `name` in this folder, of at most `limit` bytes:
    /// opened without following a link (`O_NOFOLLOW`) and without waiting
    /// on anything that is not a file (`O_NONBLOCK`), and read only when
    /// `fstat` says it is a regular file -- `expected`, when given, the
    /// very one. A larger one is refused (`InvalidData`) rather than read,
    /// by what `fstat` says on the opened file and by what the read finds,
    /// should the file have grown in between. With what `fstat` said of it.
    ///
    /// The limit is the caller's to name, every time: there is no reader
    /// without one, so a file a tool wrote -- or one put in its place, a
    /// sparse file of gigabytes -- is never read whole into memory by
    /// accident (`adapters::read_file::LIMIT` is the usual one).
    pub fn read_file_at_most(
        &self,
        name: &OsStr,
        expected: Option<&Stat>,
        limit: u64,
    ) -> io::Result<(Stat, Vec<u8>)> {
        #[cfg(test)]
        calls::note(calls::Call::ReadFileAt, Some(self), &[name]);
        let (file, stat) = self.open_file_at(name)?;
        if !stat.is_file() || expected.is_some_and(|expected| !expected.same_as(&stat)) {
            return Err(io::Error::other("not the file that was looked at"));
        }
        let too_large = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("larger than {limit} bytes"),
            )
        };
        if stat.size > limit {
            return Err(too_large());
        }
        let mut bytes = Vec::new();
        // One byte past the limit, to tell a file that grew since `fstat`.
        file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(too_large());
        }
        Ok((stat, bytes))
    }

    /// Whatever is at `name` in this folder, opened to read without
    /// following a link (`O_NOFOLLOW`) and without waiting on anything
    /// that is not a file (`O_NONBLOCK`), with what `fstat` says of what
    /// was opened: the caller decides what it may be. Reads nothing of it:
    /// what is read is read by `read_file_at_most`, under a limit.
    pub(crate) fn open_file_at(&self, name: &OsStr) -> io::Result<(std::fs::File, Stat)> {
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
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `st` has room for a stat; read only on success.
        let rc = unsafe { libc::fstat(file.as_raw_fd(), st.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: filled in by the successful call above.
        let stat = Stat::from_raw(unsafe { &st.assume_init() });
        Ok((file, stat))
    }

    /// Whether this account may `mode` (`libc::W_OK`, ...) this folder
    /// itself, as `access(2)` answers for its path: `faccessat` of `.`
    /// from the descriptor, which looks nothing else up.
    pub fn access(&self, mode: libc::c_int) -> bool {
        // SAFETY: a NUL-terminated literal name, looked up from a
        // descriptor this `Dir` owns.
        unsafe { libc::faccessat(self.fd.as_raw_fd(), c".".as_ptr(), mode, 0) == 0 }
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
            #[cfg(test)]
            calls::note_entry();
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

/// What a test sees of the calls made here: how many of each, and the
/// path each one looked at -- the folder's own path as the kernel names
/// it (`fcntl(F_GETPATH)`), joined with the name or names asked of it.
/// Kept per thread and only while a test asks for it (`measure`), so
/// tests running beside one another never see each other's calls. Each
/// counted call is one system call: `open_dir_at` counts as itself and,
/// for the `fstat` it makes of what it opened, as a `stat`; the `close`
/// of a dropped `Dir` is not counted.
#[cfg(test)]
pub(crate) mod calls {
    use super::Dir;
    use std::cell::RefCell;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Call {
        Root,
        StatAt,
        StatBeneath,
        Stat,
        ReadLinkAt,
        OpenDirAt,
        ReadFileAt,
        /// Where a folder held open now is (`F_GETPATH`): no name looked
        /// up, nothing in it looked at. Its path is where the folder is.
        GetPath,
    }

    #[derive(Clone, Debug, Default)]
    pub(crate) struct Calls {
        pub entries: usize,
        pub root: usize,
        pub stat_at: usize,
        pub stat_beneath: usize,
        pub stat: usize,
        pub read_link_at: usize,
        pub open_dir_at: usize,
        pub read_file_at: usize,
        pub get_path: usize,
        /// Each call and the path it looked at, in the order made.
        pub paths: Vec<(Call, PathBuf)>,
    }

    impl Calls {
        pub fn total(&self) -> usize {
            self.root
                + self.stat_at
                + self.stat_beneath
                + self.stat
                + self.read_link_at
                + self.open_dir_at
                + self.read_file_at
                + self.get_path
        }
    }

    thread_local! {
        static ACTIVE: RefCell<Option<Calls>> = const { RefCell::new(None) };
    }

    pub(super) fn note_entry() {
        ACTIVE.with(|active| {
            if let Some(calls) = active.borrow_mut().as_mut() {
                calls.entries += 1;
            }
        });
    }

    pub(super) fn note(call: Call, dir: Option<&Dir>, names: &[&OsStr]) {
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let Some(calls) = active.as_mut() else {
                return;
            };
            *match call {
                Call::Root => &mut calls.root,
                Call::StatAt => &mut calls.stat_at,
                Call::StatBeneath => &mut calls.stat_beneath,
                Call::Stat => &mut calls.stat,
                Call::ReadLinkAt => &mut calls.read_link_at,
                Call::OpenDirAt => &mut calls.open_dir_at,
                Call::ReadFileAt => &mut calls.read_file_at,
                Call::GetPath => &mut calls.get_path,
            } += 1;
            let mut path = dir.map_or_else(|| PathBuf::from("/"), path_of);
            for name in names {
                path.push(name);
            }
            calls.paths.push((call, path));
        });
    }

    /// A call whose path is already known: `F_GETPATH`'s answer.
    pub(super) fn note_path(call: Call, path: &std::path::Path) {
        ACTIVE.with(|active| {
            if let Some(calls) = active.borrow_mut().as_mut() {
                calls.get_path += usize::from(call == Call::GetPath);
                calls.paths.push((call, path.to_path_buf()));
            }
        });
    }

    /// Where the kernel says `dir` is.
    #[cfg(target_os = "macos")]
    fn path_of(dir: &Dir) -> PathBuf {
        use std::ffi::OsString;
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStringExt;
        let mut buf = vec![0u8; libc::PATH_MAX as usize];
        // SAFETY: `buf` has room for `MAXPATHLEN` bytes, as `F_GETPATH`
        // requires; the result is checked.
        let rc = unsafe { libc::fcntl(dir.fd.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) };
        assert!(rc >= 0, "F_GETPATH: {}", std::io::Error::last_os_error());
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(len);
        PathBuf::from(OsString::from_vec(buf))
    }

    #[cfg(not(target_os = "macos"))]
    fn path_of(dir: &Dir) -> PathBuf {
        use std::os::fd::AsRawFd;
        std::fs::read_link(format!("/proc/self/fd/{}", dir.fd.as_raw_fd())).unwrap()
    }

    /// `run`'s answer and the calls it made here, on this thread.
    pub(crate) fn measure<T>(run: impl FnOnce() -> T) -> (T, Calls) {
        struct Off;
        impl Drop for Off {
            fn drop(&mut self) {
                ACTIVE.with(|active| *active.borrow_mut() = None);
            }
        }
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            assert!(active.is_none(), "one measure at a time");
            *active = Some(Calls::default());
        });
        let off = Off;
        let out = run();
        let calls = ACTIVE.with(|active| active.borrow_mut().take().unwrap_or_default());
        drop(off);
        (out, calls)
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
    fn test_equal_inode_numbers_on_different_devices_are_not_the_same_file() {
        let temp = Temp::new("identity-device");
        let (_, original) = Dir::open_path(&temp.0, false).unwrap();
        let mut on_another_device = original;
        on_another_device.dev = original.dev.wrapping_add(1);
        assert!(original.same_as(&original));
        assert!(!original.same_as(&on_another_device));
    }

    #[test]
    fn test_a_file_replaced_after_it_was_looked_at_is_not_read() {
        let temp = Temp::new("file-swap");
        std::fs::write(temp.0.join("file"), b"old").unwrap();
        let (parent, _) = Dir::open_path(&temp.0, false).unwrap();
        let checked = parent.stat_at(OsStr::new("file")).unwrap();
        assert_eq!(
            parent
                .read_file_at_most(OsStr::new("file"), Some(&checked), 3)
                .unwrap()
                .1,
            b"old"
        );
        std::fs::rename(temp.0.join("file"), temp.0.join("aside")).unwrap();
        std::fs::write(temp.0.join("file"), b"new").unwrap();
        let error = parent
            .read_file_at_most(OsStr::new("file"), Some(&checked), 3)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn test_a_link_to_a_regular_file_is_never_opened_or_read() {
        let temp = Temp::new("regular-link");
        std::fs::write(temp.0.join("payload"), b"secret").unwrap();
        symlink("payload", temp.0.join("alias")).unwrap();
        let (parent, _) = Dir::open_path(&temp.0, false).unwrap();
        assert_eq!(
            parent
                .read_file_at_most(OsStr::new("payload"), None, 6)
                .unwrap()
                .1,
            b"secret"
        );
        let error = parent.open_file_at(OsStr::new("alias")).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ELOOP));
        assert!(parent
            .read_file_at_most(OsStr::new("alias"), None, 6)
            .is_err());
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
        assert_eq!(
            dir.read_file_at_most(OsStr::new("file"), None, 1)
                .unwrap()
                .1,
            b"x"
        );
        assert!(dir.read_file_at_most(OsStr::new("file"), None, 0).is_err());
        // A link is never opened as a folder, nor read as a file.
        assert!(dir.open_dir_at(OsStr::new("link"), None, true).is_err());
        assert!(dir.read_file_at_most(OsStr::new("link"), None, 1).is_err());
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

    fn names(path: &str) -> Vec<OsString> {
        path.split('/').map(OsString::from).collect()
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_several_names_are_looked_at_with_no_link_followed_anywhere() {
        let temp = Temp::new("beneath");
        std::fs::create_dir_all(temp.0.join("a/b")).unwrap();
        std::fs::create_dir_all(temp.0.join("real")).unwrap();
        std::fs::write(temp.0.join("a/b/f"), b"x").unwrap();
        std::fs::write(temp.0.join("real/g"), b"y").unwrap();
        symlink("../real", temp.0.join("a/lnk")).unwrap();
        symlink("f", temp.0.join("a/b/flink")).unwrap();
        let (dir, _) = Dir::open_path(&temp.0, false).unwrap();
        // Through real folders: what `stat_at` says from the last one.
        let (b, _) = Dir::open_path(&temp.0.join("a/b"), false).unwrap();
        assert_eq!(
            dir.stat_beneath(&names("a/b/f")).unwrap(),
            b.stat_at(OsStr::new("f")).unwrap()
        );
        assert!(dir.stat_beneath(&names("a/b")).unwrap().is_dir());
        // A link at the end is the link itself.
        assert!(dir.stat_beneath(&names("a/b/flink")).unwrap().is_symlink());
        assert!(dir.stat_beneath(&names("a/lnk")).unwrap().is_symlink());
        // A link on the way is never followed, whatever lies beyond it.
        for path in ["a/lnk/g", "a/lnk/missing", "a/b/flink/x"] {
            let error = dir.stat_beneath(&names(path)).unwrap_err();
            assert_eq!(error.raw_os_error(), Some(libc::ELOOP), "{path}");
        }
        let missing = dir.stat_beneath(&names("a/missing/x")).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        let through_a_file = dir.stat_beneath(&names("a/b/f/x")).unwrap_err();
        assert_eq!(through_a_file.raw_os_error(), Some(libc::ENOTDIR));
        // A name `stat_at` would refuse is refused before any lookup.
        for bad in [
            names("a/.."),
            names("a/."),
            vec![OsString::from("a/b")],
            names(""),
        ] {
            let error = dir.stat_beneath(&bad).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad:?}");
        }
        assert_eq!(
            dir.stat_beneath(&[]).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn test_calls_are_counted_with_the_path_each_one_looked_at() {
        let temp = Temp::new("calls");
        std::fs::create_dir_all(temp.0.join("a")).unwrap();
        std::fs::write(temp.0.join("a/f"), b"x").unwrap();
        let (dir, _) = Dir::open_path(&temp.0, false).unwrap();
        let (_, calls) = calls::measure(|| {
            let (a, _) = dir.open_dir_at(OsStr::new("a"), None, false).unwrap();
            a.stat_at(OsStr::new("f")).unwrap();
        });
        assert_eq!(calls.open_dir_at, 1);
        assert_eq!(calls.stat, 1, "the fstat open_dir_at makes");
        assert_eq!(calls.stat_at, 1);
        assert_eq!(calls.total(), 3);
        let paths: Vec<&Path> = calls.paths.iter().map(|(_, p)| p.as_path()).collect();
        let a = temp.0.join("a");
        let f = temp.0.join("a/f");
        assert_eq!(paths, vec![a.as_path(), a.as_path(), f.as_path()]);
        // Nothing is kept once the test stops asking.
        let (_, none) = calls::measure(|| ());
        assert_eq!(none.total(), 0);
        dir.stat_at(OsStr::new("a")).unwrap();
    }
}
