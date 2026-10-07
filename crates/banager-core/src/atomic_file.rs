//! Exclusive staging in the destination directory, shared by settings and
//! history. Each writer owns a regular file even across app processes.
use std::io::{self, Write};
use std::path::Path;

/// Lock the existing app-data directory itself before any persistence loads.
/// O_DIRECTORY prevents a substituted regular file or pipe from being opened.
/// The held descriptor is the lock; no additional file is written.
pub fn lock_directory(parent: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(parent)?;
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(parent)?;
    directory.try_lock().map_err(io::Error::from)?;
    Ok(directory)
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut prefix = path.file_name().unwrap_or_default().to_os_string();
    prefix.push(".tmp.");
    let mut staging = tempfile::Builder::new()
        .prefix(&prefix)
        .tempfile_in(parent)?;
    staging.write_all(bytes)?;
    staging.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

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
