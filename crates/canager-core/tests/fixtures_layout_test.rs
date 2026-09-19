use canager_core::events::VecSink;
use canager_core::session::Session;
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
    // (crates/canager-core), matching every other fixture/meta path in this
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
