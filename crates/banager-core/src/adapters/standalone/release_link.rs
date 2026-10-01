//! A version read with no command: the name of the release folder a link
//! inside the tool's root points at (`VersionSource::ReleaseLink`). Codex's
//! installer keeps every release in `<root>/releases/<version>-<target>`
//! and points `<root>/current` at the one in use, so the version is in
//! that folder's name and Banager never has to run `codex` to learn it.
//!
//! Read-only: `readlink`, `realpath`, `lstat` and one small file read.
//! Nothing here writes, and nothing here runs.

use super::recipe::ReleaseLink;
use super::route::lexical_join;
use std::io::Read;
use std::path::Path;

/// The longest marker file read. The installer writes one release folder's
/// name into it (`0.159.3-aarch64-apple-darwin`); anything much longer is
/// not that, and is not read past this.
const MARKER_LIMIT: u64 = 256;

/// The longest version a folder name may give.
const VERSION_LIMIT: usize = 64;

/// What one look at the link found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkReading {
    /// The version in the release folder's name; `None` when the link is
    /// missing, is not a link, leads nowhere, leads out of the releases
    /// folder, or names a folder without one of the recipe's endings.
    pub version: Option<String>,
    /// Whether the install follows the latest release: the marker file
    /// names the very folder the link points at. False whenever `version`
    /// is `None`.
    pub follows_latest: bool,
}

/// Reads `spec`'s link under `root` (the instance's `prefix`).
///
/// The link must be a symbolic link whose target, resolved, is a folder
/// directly inside `<root>/<releases>` (resolved too): the installer writes
/// it absolute, `$CODEX_HOME/packages/standalone/releases/<name>`. A
/// relative text is taken as seen from `root`. The folder's name, less one
/// of `spec.suffixes`, is the version, which must look like one: it starts
/// with a digit and holds only letters, digits, `.`, `+` and `-` (the
/// install script's own pattern for a version, `[0-9][0-9A-Za-z.+-]*`).
pub fn read(root: &Path, spec: &ReleaseLink) -> LinkReading {
    let Some(name) = release_name(root, spec) else {
        return LinkReading::default();
    };
    let Some(version) = version_in(&name, spec.suffixes) else {
        return LinkReading::default();
    };
    LinkReading {
        version: Some(version),
        follows_latest: marker_names(&root.join(spec.follows_latest), &name),
    }
}

/// The name of the release folder the link resolves to, when it resolves
/// to a folder directly inside the releases folder.
fn release_name(root: &Path, spec: &ReleaseLink) -> Option<String> {
    let link = root.join(spec.link);
    // `read_link` fails on anything that is not a symbolic link.
    let text = std::fs::read_link(&link).ok()?;
    let target = lexical_join(root, &text);
    let real = std::fs::canonicalize(&target).ok()?;
    if !real.is_dir() {
        return None;
    }
    let releases = std::fs::canonicalize(root.join(spec.releases)).ok()?;
    if real.parent()? != releases.as_path() {
        return None;
    }
    real.file_name()?.to_str().map(str::to_string)
}

/// `name` less the first of `suffixes` it ends with, when what is left
/// looks like a version.
fn version_in(name: &str, suffixes: &[&str]) -> Option<String> {
    let version = suffixes
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))?;
    looks_like_a_version(version).then(|| version.to_string())
}

/// `[0-9][0-9A-Za-z.+-]*`, at most `VERSION_LIMIT` bytes.
fn looks_like_a_version(s: &str) -> bool {
    s.len() <= VERSION_LIMIT
        && s.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
}

/// Whether the regular file at `marker` holds `name` and nothing else
/// (surrounding white space aside). A missing marker, a link, a folder or
/// an unreadable file is "no".
fn marker_names(marker: &Path, name: &str) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(marker) else {
        return false;
    };
    if !meta.file_type().is_file() {
        return false;
    }
    let Ok(file) = std::fs::File::open(marker) else {
        return false;
    };
    let mut text = String::new();
    if file.take(MARKER_LIMIT).read_to_string(&mut text).is_err() {
        return false;
    }
    text.trim() == name
}

#[cfg(test)]
mod tests {
    use super::super::recipes::CODEX;
    use super::super::testing::TempHome;
    use super::*;
    use crate::adapters::standalone::recipe::VersionSource;
    use std::path::PathBuf;

    fn spec() -> &'static ReleaseLink {
        match &CODEX.version {
            VersionSource::ReleaseLink(spec) => spec,
            VersionSource::Command(_) | VersionSource::NotRead => {
                panic!("Codex reads its version from a link")
            }
        }
    }

    /// The installer's layout under `<home>/.codex/packages/standalone`:
    /// `releases/<name>/bin/codex`, and `current` linking (absolute text)
    /// to the release folder. Returns the root.
    fn installed(home: &TempHome, name: &str) -> PathBuf {
        let root = home.dir(".codex/packages/standalone");
        home.executable(&format!(
            ".codex/packages/standalone/releases/{name}/bin/codex"
        ));
        home.link(
            ".codex/packages/standalone/current",
            &root.join("releases").join(name),
        );
        root
    }

    #[test]
    fn test_the_current_links_release_folder_gives_the_version() {
        let home = TempHome::new("codex-link-version");
        let root = installed(&home, "0.159.3-aarch64-apple-darwin");
        assert_eq!(
            read(&root, spec()),
            LinkReading {
                version: Some("0.159.3".to_string()),
                follows_latest: false,
            }
        );
    }

    #[test]
    fn test_an_intel_release_and_a_prerelease_version_read_too() {
        let home = TempHome::new("codex-link-intel");
        let root = installed(&home, "0.160.0-alpha.2-x86_64-apple-darwin");
        assert_eq!(
            read(&root, spec()).version.as_deref(),
            Some("0.160.0-alpha.2")
        );
    }

    #[test]
    fn test_the_marker_naming_the_current_release_says_it_follows_latest() {
        let home = TempHome::new("codex-link-marker");
        let root = installed(&home, "0.159.3-aarch64-apple-darwin");
        std::fs::write(
            root.join("auto-update-version"),
            "0.159.3-aarch64-apple-darwin",
        )
        .unwrap();
        assert!(read(&root, spec()).follows_latest);
        // A marker left from an older release (the link has moved on to a
        // pinned one) is not this install following latest.
        std::fs::write(
            root.join("auto-update-version"),
            "0.158.0-aarch64-apple-darwin\n",
        )
        .unwrap();
        let reading = read(&root, spec());
        assert_eq!(reading.version.as_deref(), Some("0.159.3"));
        assert!(!reading.follows_latest);
    }

    #[test]
    fn test_a_marker_that_is_a_link_or_a_folder_is_not_read() {
        let home = TempHome::new("codex-link-marker-shape");
        let root = installed(&home, "0.159.3-aarch64-apple-darwin");
        let elsewhere = home.path().join("elsewhere.txt");
        std::fs::write(&elsewhere, "0.159.3-aarch64-apple-darwin").unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("auto-update-version")).unwrap();
        assert!(!read(&root, spec()).follows_latest);
        std::fs::remove_file(root.join("auto-update-version")).unwrap();
        std::fs::create_dir(root.join("auto-update-version")).unwrap();
        assert!(!read(&root, spec()).follows_latest);
    }

    #[test]
    fn test_no_link_gives_no_version() {
        let home = TempHome::new("codex-link-missing");
        let root = home.dir(".codex/packages/standalone");
        home.executable(
            ".codex/packages/standalone/releases/0.159.3-aarch64-apple-darwin/bin/codex",
        );
        assert_eq!(read(&root, spec()), LinkReading::default());
    }

    #[test]
    fn test_a_broken_link_gives_no_version() {
        let home = TempHome::new("codex-link-broken");
        let root = home.dir(".codex/packages/standalone");
        home.dir(".codex/packages/standalone/releases");
        home.link(
            ".codex/packages/standalone/current",
            &root.join("releases/0.159.3-aarch64-apple-darwin"),
        );
        assert_eq!(read(&root, spec()), LinkReading::default());
    }

    #[test]
    fn test_a_current_that_is_a_folder_not_a_link_gives_no_version() {
        let home = TempHome::new("codex-link-folder");
        let root = home.dir(".codex/packages/standalone");
        home.dir(".codex/packages/standalone/current/bin");
        assert_eq!(read(&root, spec()), LinkReading::default());
    }

    #[test]
    fn test_a_link_out_of_the_releases_folder_gives_no_version() {
        let home = TempHome::new("codex-link-outside");
        let root = home.dir(".codex/packages/standalone");
        home.dir(".codex/packages/standalone/releases");
        let other = home.dir("Downloads/0.159.3-aarch64-apple-darwin");
        home.link(".codex/packages/standalone/current", &other);
        assert_eq!(read(&root, spec()), LinkReading::default());
        // Nor one two levels down inside it.
        std::fs::remove_file(root.join("current")).unwrap();
        let deep = home.dir(".codex/packages/standalone/releases/x/0.159.3-aarch64-apple-darwin");
        home.link(".codex/packages/standalone/current", &deep);
        assert_eq!(read(&root, spec()), LinkReading::default());
    }

    #[test]
    fn test_a_relative_link_is_read_from_the_root() {
        let home = TempHome::new("codex-link-relative");
        let root = home.dir(".codex/packages/standalone");
        home.executable(
            ".codex/packages/standalone/releases/0.159.3-aarch64-apple-darwin/bin/codex",
        );
        home.link(
            ".codex/packages/standalone/current",
            Path::new("releases/0.159.3-aarch64-apple-darwin"),
        );
        assert_eq!(read(&root, spec()).version.as_deref(), Some("0.159.3"));
    }

    #[test]
    fn test_a_folder_name_without_a_known_ending_or_version_gives_none() {
        for name in [
            "0.159.3",
            "0.159.3-aarch64-unknown-linux-musl",
            "latest-aarch64-apple-darwin",
            "-aarch64-apple-darwin",
            "0.159.3 beta-aarch64-apple-darwin",
            ".staging.0.159.3-aarch64-apple-darwin",
        ] {
            let home = TempHome::new("codex-link-name");
            let root = installed(&home, name);
            assert_eq!(read(&root, spec()).version, None, "{name:?}");
        }
        let long = format!("{}-aarch64-apple-darwin", "1".repeat(VERSION_LIMIT + 1));
        assert_eq!(version_in(&long, spec().suffixes), None);
    }
}
