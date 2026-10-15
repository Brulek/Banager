use banager_core::events::VecSink;
use banager_core::session::Session;
use std::path::Path;
use std::sync::Arc;

/// Regression guard: every adapter id `Session::new` registers must have a
/// recorded fixture directory under `adapters/fixtures/`, and every fixture
/// directory that exists must be for a real, registered adapter -- so a
/// fixture directory can never go stale (renamed adapter, removed source)
/// without this test catching it, and a newly added source can never ship
/// without at least one recorded, README'd fixture version.
#[test]
fn test_every_registered_adapter_has_a_documented_fixture_directory() {
    let sink = Arc::new(VecSink::new());
    let session = Session::new(sink, None);
    let adapter_ids = session.adapter_ids();

    // cargo runs tests with cwd = the package manifest directory
    // (crates/banager-core), matching every other fixture/meta path in this
    // crate.
    let fixtures_root = Path::new("../../adapters/fixtures");
    let mut fixture_ids: Vec<String> = std::fs::read_dir(fixtures_root)
        .expect("read adapters/fixtures")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    fixture_ids.sort();

    assert_eq!(
        fixture_ids, adapter_ids,
        "adapters/fixtures/* must have exactly one directory per registered adapter id"
    );

    for id in &adapter_ids {
        let source_dir = fixtures_root.join(id);
        let mut versions: Vec<_> = std::fs::read_dir(&source_dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", source_dir.display()))
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(
            !versions.is_empty(),
            "{} has no recorded version directory",
            source_dir.display()
        );
        versions.sort_by_key(|e| e.file_name());
        for version_dir in versions {
            let readme = version_dir.path().join("README.md");
            assert!(
                readme.is_file(),
                "{} is missing a README.md naming its commands and traps",
                version_dir.path().display()
            );
        }
    }
}

/// The samples made rather than recorded -- edited from a recording, or
/// built from a catalogue Homebrew downloaded, for a case that could not be
/// recorded without changing the recording Mac's own tools, such as a
/// pinned package -- live apart from the recordings, under
/// `adapters/fixtures-derived/<id>/<version>/`, so that none sits among the
/// recordings in `adapters/fixtures/` (the author's decision R7,
/// 2026-10-06). Each such folder is for a registered adapter, has a
/// README.md that names every sample in it and says how it was made, and
/// sits beside a recording of the same version, which stays under
/// `adapters/fixtures/<id>/<version>/`; and no sample there has the path
/// of a recording beside it, so the two cannot be mistaken for each other.
///
/// What this test cannot see is whether a file under `adapters/fixtures/`
/// was edited; each folder's README says so where it was (four
/// `layout.txt` listings have the account name or the home folder's path
/// masked).
#[test]
fn test_every_derived_fixture_has_a_readme_and_the_recording_it_was_made_from() {
    let sink = Arc::new(VecSink::new());
    let session = Session::new(sink, None);
    let adapter_ids = session.adapter_ids();

    let derived_root = Path::new("../../adapters/fixtures-derived");
    let fixtures_root = Path::new("../../adapters/fixtures");
    assert!(
        derived_root.join("README.md").is_file(),
        "adapters/fixtures-derived/ has no README.md saying what goes there"
    );
    let mut ids: Vec<_> = std::fs::read_dir(derived_root)
        .expect("read adapters/fixtures-derived")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    assert!(
        !ids.is_empty(),
        "adapters/fixtures-derived/ holds no sample"
    );
    ids.sort_by_key(|e| e.file_name());
    for id in ids {
        let name = id.file_name().to_string_lossy().into_owned();
        assert!(
            adapter_ids.contains(&name),
            "adapters/fixtures-derived/{name} is not a registered adapter id"
        );
        let versions: Vec<_> = std::fs::read_dir(id.path())
            .unwrap_or_else(|e| panic!("read {}: {e}", id.path().display()))
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(
            !versions.is_empty(),
            "{} has no version directory",
            id.path().display()
        );
        for version in versions {
            let dir = version.path();
            assert!(
                dir.join("README.md").is_file(),
                "{} is missing a README.md saying what was edited",
                dir.display()
            );
            let recording = fixtures_root.join(&name).join(version.file_name());
            assert!(
                recording.is_dir(),
                "{} has no recording of the same version at {}",
                dir.display(),
                recording.display()
            );
            let readme = std::fs::read_to_string(dir.join("README.md"))
                .unwrap_or_else(|e| panic!("read {}/README.md: {e}", dir.display()));
            let samples = files_under(&dir);
            assert!(!samples.is_empty(), "{} holds no sample", dir.display());
            for sample in samples
                .iter()
                .filter(|s| s.as_path() != Path::new("README.md"))
            {
                assert!(
                    !recording.join(sample).exists(),
                    "{} has the path of a recording in {}",
                    dir.join(sample).display(),
                    recording.display()
                );
                let file_name = sample.file_name().unwrap().to_string_lossy();
                assert!(
                    readme.contains(&format!("`{file_name}`")),
                    "{}/README.md does not name {file_name}",
                    dir.display()
                );
            }
        }
    }
}

/// Every file under `dir`, at any depth, as a path relative to it.
fn files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![std::path::PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let here = dir.join(&relative);
        for entry in std::fs::read_dir(&here)
            .unwrap_or_else(|e| panic!("read {}: {e}", here.display()))
            .filter_map(|entry| entry.ok())
        {
            let path = relative.join(entry.file_name());
            if entry.path().is_dir() {
                pending.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
