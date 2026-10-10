//! What the app's adapters look at where the tests give folders of their
//! own (r23 of the 10-07 review): this crate's unit tests build each of
//! these with a stand-in under `cfg(test)`, so only a build without it --
//! these integration tests, as the app -- can hold the real one in place.
//! Each is read off the adapter, or tried on the test's own folders:
//! nothing here looks at `/`, a Homebrew prefix or anything else of the
//! Mac running it.

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::standalone::recipes::RECIPES;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::http::MockHttpClient;
use banager_core::runner::MockRunner;
use banager_core::trash::MockTrasher;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;

#[test]
fn test_a_standalone_tool_looks_for_its_recipes_absolute_paths_from_the_root_of_this_mac() {
    // Homebrew's prefixes rustup's preview looks in, grok's fallback links
    // in `/usr/local/bin` (`Detected::on_this_mac`): under `/`, as the
    // recipe spells them, for every tool the app registers.
    for recipe in RECIPES {
        let adapter = StandaloneAdapter::new(
            recipe,
            Arc::new(MockRunner::new()),
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        assert_eq!(adapter.machine_root(), Path::new("/"), "{}", recipe.id);
        let moved = adapter.with_machine_root(Path::new("/stand-in"));
        assert_eq!(
            moved.machine_root(),
            Path::new("/stand-in"),
            "{}",
            recipe.id
        );
    }
}

#[test]
fn test_the_homebrew_queue_key_tells_a_folder_by_what_it_is_not_how_it_is_spelled() {
    // `brew::prefix_lock` keys an npm or Homebrew plan on a discovery
    // prefix reached under another spelling (a link, `/usr/local/../`) by
    // reading the folder itself: device and inode, through any link,
    // never into a protected place. Tried on the test's own folders.
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("prefix");
    std::fs::create_dir(&folder).unwrap();
    let link = dir.path().join("prefix-link");
    std::os::unix::fs::symlink(&folder, &link).unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"").unwrap();
    let missing = dir.path().join("missing");
    let found = std::fs::metadata(&folder).unwrap();
    let folder_itself = Some((found.dev(), found.ino()));

    let brew = BrewAdapter::new(Arc::new(MockRunner::new()));
    let npm = NpmAdapter::new(Arc::new(MockRunner::new()));
    let paths = [&folder, &link, &file, &missing];
    for (adapter, read) in [
        ("brew", paths.map(|path| brew.prefix_identity(path))),
        ("npm", paths.map(|path| npm.prefix_identity(path))),
    ] {
        assert_eq!(
            read,
            [folder_itself, folder_itself, None, None],
            "{adapter}: the folder, through a link; a file is no prefix; nothing there"
        );
    }

    // The integration tests' hooks look at no folder at all.
    let quiet_brew = BrewAdapter::new(Arc::new(MockRunner::new())).reading_nothing_of_this_mac();
    let quiet_npm = NpmAdapter::new(Arc::new(MockRunner::new())).looking_at_no_homebrew_prefix();
    assert_eq!(quiet_brew.prefix_identity(&folder), None);
    assert_eq!(quiet_npm.prefix_identity(&folder), None);
}
