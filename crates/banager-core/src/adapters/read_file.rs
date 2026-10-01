//! Reading a file a tool wrote -- `.crates2.json`, an Ollama manifest,
//! `~/.claude/settings.json`, a cask's receipt, `brew.env`, a startup
//! file -- before a parser reads it, bounded both ways.
//!
//! In time: a name that leads to a named pipe would block `open` until
//! something writes to it, and a refresh would wait forever. The file is
//! opened without waiting (`O_NONBLOCK`) and read only when `fstat` on
//! the opened file says it is a regular file, so no swap between a check
//! and the open can slip a pipe in (`dirfd::Dir::read_file_at` does the
//! same inside a folder).
//!
//! In size: every one of these files is a few hundred kilobytes at most
//! (the recorded Ollama manifest, the largest, is 250 KB), and a parser
//! holds what it reads in memory more than once. A file past `LIMIT` is
//! refused rather than read, as an HTTP body past
//! `http::real::MAX_RESPONSE_BYTES` is.

use std::io::{Error, ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The most Banager reads of one such file: 16 MiB.
pub(crate) const LIMIT: u64 = 16 * 1024 * 1024;

/// `path`'s bytes and the opened file's metadata, links followed; an
/// error -- `NotFound` kept as it is -- when it is not a regular file or
/// is larger than `limit` bytes.
pub(crate) fn read_regular(
    path: &Path,
    limit: u64,
) -> std::io::Result<(std::fs::Metadata, Vec<u8>)> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
    }
    if meta.len() > limit {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("larger than {limit} bytes"),
        ));
    }
    let mut bytes = Vec::new();
    // The file may grow between `fstat` and the read: read one byte past
    // the limit to tell.
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("larger than {limit} bytes"),
        ));
    }
    Ok((meta, bytes))
}

/// `read_regular` under `LIMIT`, the bytes alone.
pub(crate) fn read_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    read_regular(path, LIMIT).map(|(_, bytes)| bytes)
}

/// `read_bytes` as UTF-8 text, `InvalidData` when it is not, as
/// `std::fs::read_to_string` answers.
pub(crate) fn read_text(path: &Path) -> std::io::Result<String> {
    String::from_utf8(read_bytes(path)?)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "stream did not contain valid UTF-8"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A fresh, empty folder of the test's own under the system's
    /// temporary folder.
    pub(crate) fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "banager-read-file-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// A named pipe at `path`, which nothing ever writes to.
    pub(crate) fn make_fifo(path: &Path) {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).expect("no NUL");
        // SAFETY: `c` is a valid NUL-terminated path for the call's duration.
        let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
        assert_eq!(rc, 0, "mkfifo {}", path.display());
    }

    /// `f` finishes within five seconds: a regression to a blocking open
    /// fails the test instead of hanging the suite.
    pub(crate) fn finishes<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("the read waited on a named pipe")
    }

    #[test]
    fn test_reads_a_regular_file_and_keeps_not_found() {
        let dir = temp_dir("regular");
        std::fs::write(dir.join("a.json"), "{}").unwrap();
        assert_eq!(read_text(&dir.join("a.json")).unwrap(), "{}");
        assert_eq!(
            read_text(&dir.join("missing.json")).unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert!(read_text(&dir).is_err(), "a folder is not read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn regression_a_named_pipe_is_refused_without_waiting_for_a_writer() {
        let dir = temp_dir("fifo");
        let fifo = dir.join("settings.json");
        make_fifo(&fifo);
        let err = finishes(move || read_text(&fifo).unwrap_err());
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn regression_a_file_past_the_limit_is_refused() {
        let dir = temp_dir("limit");
        std::fs::write(dir.join("big"), vec![b'a'; 101]).unwrap();
        std::fs::write(dir.join("small"), vec![b'a'; 100]).unwrap();
        assert_eq!(
            read_regular(&dir.join("big"), 100).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(read_regular(&dir.join("small"), 100).unwrap().1.len(), 100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_text_that_is_not_utf8_is_invalid_data() {
        let dir = temp_dir("utf8");
        std::fs::write(dir.join("x"), b"\xff\xfe").unwrap();
        assert_eq!(
            read_text(&dir.join("x")).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
