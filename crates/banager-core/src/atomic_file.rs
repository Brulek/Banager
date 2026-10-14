//! Exclusive staging in the destination directory, shared by settings and
//! history. App saves stay relative to the directory locked at startup.
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

// Weak entries do not extend the lock's lifetime. Keep expired entries so
// a save cannot silently fall back to an unlocked path after its owner exits.
fn owners() -> &'static Mutex<HashMap<PathBuf, Weak<File>>> {
    static OWNERS: OnceLock<Mutex<HashMap<PathBuf, Weak<File>>>> = OnceLock::new();
    OWNERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn open_directory(parent: &Path) -> io::Result<File> {
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(parent)?;
    Ok(directory)
}

/// Lock the app-data directory before persistence loads, and bind later
/// saves to that descriptor. No additional lock file is written.
pub fn lock_directory(parent: &Path) -> io::Result<Arc<File>> {
    let key = std::path::absolute(parent)?;
    let mut owners = owners().lock().unwrap();
    if owners.get(&key).and_then(Weak::upgrade).is_some() {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    std::fs::create_dir_all(parent)?;
    let directory = Arc::new(open_directory(parent)?);
    directory.try_lock().map_err(io::Error::from)?;
    owners.insert(key, Arc::downgrade(&directory));
    Ok(directory)
}

fn still_at(parent: &Path, directory: &File) -> io::Result<()> {
    let held = directory.metadata()?;
    let named = std::fs::metadata(parent)?;
    if held.dev() != named.dev() || held.ino() != named.ino() {
        return Err(io::Error::other(
            "app-data directory changed since it was opened",
        ));
    }
    Ok(())
}

/// An exclusively created staging file, removed relative to its original
/// directory even if that directory is renamed while a save is in progress.
struct Staging<'a> {
    directory: &'a File,
    name: CString,
    file: File,
}

impl Drop for Staging<'_> {
    fn drop(&mut self) {
        unsafe { libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0) };
    }
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let owner = owners()
        .lock()
        .unwrap()
        .get(&std::path::absolute(parent)?)
        .cloned();
    let directory = match owner {
        Some(owner) => owner
            .upgrade()
            .ok_or_else(|| io::Error::other("app-data directory lock was released"))?,
        None => {
            // Standalone core callers have no application directory lock.
            std::fs::create_dir_all(parent)?;
            Arc::new(open_directory(parent)?)
        }
    };
    write_in(parent, &directory, path, bytes)
}

fn write_in(parent: &Path, directory: &File, path: &Path, bytes: &[u8]) -> io::Result<()> {
    still_at(parent, directory)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing file name"))?;
    let destination = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid file name"))?;
    let mut staging = loop {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|error| io::Error::other(error.to_string()))?;
        let mut name = name.as_bytes().to_vec();
        name.extend_from_slice(b".tmp.");
        for byte in random {
            name.extend_from_slice(format!("{byte:02x}").as_bytes());
        }
        let name = CString::new(name).unwrap();
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK,
                0o600 as libc::c_uint,
            )
        };
        if fd >= 0 {
            break Staging {
                directory,
                name,
                file: unsafe { File::from_raw_fd(fd) },
            };
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    };
    staging.file.write_all(bytes)?;
    still_at(parent, directory)?;
    let result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            staging.name.as_ptr(),
            directory.as_raw_fd(),
            destination.as_ptr(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn replaced_or_retargeted_app_directory_refuses_settings_and_history_saves() {
        for retarget in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let configured = root.path().join("data");
            let original = root.path().join("original");
            if retarget {
                std::fs::create_dir(&original).unwrap();
                symlink(&original, &configured).unwrap();
            }
            let guard = lock_directory(&configured).unwrap();
            for name in ["settings.json", "history.json"] {
                write(&configured.join(name), b"original").unwrap();
            }
            if retarget {
                std::fs::remove_file(&configured).unwrap();
            } else {
                std::fs::rename(&configured, &original).unwrap();
            }
            // A missing directory must not be recreated by the first writer.
            assert!(write(&configured.join("settings.json"), b"lost").is_err());
            assert!(!configured.exists());
            let replacement = if retarget {
                let replacement = root.path().join("replacement");
                std::fs::create_dir(&replacement).unwrap();
                symlink(&replacement, &configured).unwrap();
                replacement
            } else {
                std::fs::create_dir(&configured).unwrap();
                configured.clone()
            };
            for name in ["settings.json", "history.json"] {
                std::fs::write(replacement.join(name), b"second writer").unwrap();
                assert!(write(&configured.join(name), b"first writer").is_err());
                assert_eq!(
                    std::fs::read(replacement.join(name)).unwrap(),
                    b"second writer"
                );
                assert_eq!(std::fs::read(original.join(name)).unwrap(), b"original");
            }
            assert_eq!(std::fs::read_dir(&replacement).unwrap().count(), 2);
            drop(guard);
        }
    }

    #[test]
    fn regression_existing_staging_symlinks_never_clobber_their_targets() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("unrelated");
        std::fs::write(&victim, b"keep me").unwrap();
        for name in ["settings.json", "history.json"] {
            let link = dir.path().join(format!("{name}.tmp.0"));
            symlink(&victim, &link).unwrap();
            let path = dir.path().join(name);
            write(&path, b"{}\n").unwrap();
            assert!(std::fs::symlink_metadata(&path).unwrap().is_file());
            assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");
            assert!(std::fs::symlink_metadata(link).unwrap().is_symlink());
        }
        assert_eq!(std::fs::read(victim).unwrap(), b"keep me");
    }

    #[test]
    fn regression_concurrent_atomic_writes_are_whole_and_leave_no_staging_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for i in 0..8 {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    let bytes = serde_json::to_vec(&vec![i; 16384]).unwrap();
                    barrier.wait();
                    for _ in 0..8 {
                        write(path, &bytes).unwrap();
                        let seen: Vec<usize> =
                            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                        assert_eq!(seen.len(), 16384);
                        assert!(seen.iter().all(|v| *v == seen[0]));
                    }
                });
            }
        });
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn regression_failed_rename_removes_owned_staging_file() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("directory");
        std::fs::create_dir(&destination).unwrap();
        assert!(write(&destination, b"{}").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
