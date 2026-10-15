//! What the app's adapters look at where the tests give folders of their
//! own (r23 of the 10-07 review): the adapters' production readers are
//! built here, without `cfg(test)`, and tried on the test's own folders.
//! The home folders they keep out are the test's too: with the
//! `test-support` feature these tests are built with, the process's
//! `HOME` (`protected::as_if_home`) and the account's
//! (`protected::as_if_account`) are each the test's own stand-in, or none,
//! and what the app reads for `HOME` is tried with a value the test gives
//! (`Protected::of_home_variable`) -- so nothing here looks at the
//! tester's home folder, a Homebrew prefix or anything else of the Mac
//! running it, past the folders on the way to the test's own (R73-4).

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::standalone::recipes::RECIPES;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::http::MockHttpClient;
use banager_core::protected::{as_if_account, as_if_home, auditing_lookups, Protected};
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

/// Stops, before it is made, every lookup of a path neither inside one of
/// `roots` nor a folder on the way to one (as spelled, and as `/var` leads
/// to `/private/var`).
fn only_looking_within(roots: &[&Path]) -> banager_core::protected::AuditingLookups {
    let mut allowed: Vec<std::path::PathBuf> = Vec::new();
    for root in roots {
        allowed.push(root.to_path_buf());
        allowed.push(std::fs::canonicalize(root).unwrap());
    }
    auditing_lookups(move |path| {
        assert!(
            allowed
                .iter()
                .any(|root| path.starts_with(root) || root.starts_with(path)),
            "looked up {} outside the test's folders",
            path.display()
        );
    })
}

#[test]
fn test_regression_r73_4_the_process_home_the_production_readers_keep_out_is_the_tests_own() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let account = dir.path().join("account");
    let prefix = dir.path().join("prefix");
    let kept_out = home.join("Documents/prefix");
    for folder in [&prefix, &kept_out, &account.join("Documents")] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let found = std::fs::metadata(&prefix).unwrap();
    let _audit = only_looking_within(&[dir.path()]);

    // With no stand-in set, no process home at all -- never `HOME`'s.
    let brew = BrewAdapter::new(Arc::new(MockRunner::new()));
    let npm = NpmAdapter::new(Arc::new(MockRunner::new()));
    assert_eq!(
        brew.prefix_identity(&prefix),
        Some((found.dev(), found.ino()))
    );
    assert_eq!(
        npm.prefix_identity(&prefix),
        Some((found.dev(), found.ino()))
    );

    // With the test's homes, their places are kept out, by the same
    // readers the app uses.
    let _home = as_if_home(&home);
    let _account = as_if_account(&account);
    let protected = Protected::of_this_process();
    assert!(protected.contains(&home.join("Documents")));
    assert!(protected.contains(&account.join("Documents")));
    assert_eq!(
        brew.prefix_identity(&prefix),
        Some((found.dev(), found.ino()))
    );
    assert_eq!(brew.prefix_identity(&kept_out), None);
    assert_eq!(npm.prefix_identity(&kept_out), None);
}

#[test]
fn test_regression_r73_4_what_the_app_reads_for_home_is_tried_with_the_tests_value() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let other = dir.path().join("other");
    for folder in [&home, &other] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let _audit = only_looking_within(&[dir.path()]);

    // The app's reading of `HOME`, with the value given rather than the
    // process environment's.
    let given = home.clone();
    let protected = Protected::of_home_variable(move |name| {
        (name == "HOME").then(|| given.clone().into_os_string())
    });
    assert!(protected.contains(&home.join("Documents")));
    assert!(!Protected::of_home_variable(|_| None).contains(&home.join("Documents")));

    // A test that sets no stand-in has no process home: not the one
    // `HOME` names (its name is read; nothing there is looked at).
    if let Some(real) = std::env::var_os("HOME") {
        let real = std::path::PathBuf::from(real);
        assert!(!Protected::of_this_process().contains(&real.join("Documents")));
    }

    // A stand-in set inside another puts the outer one back when dropped.
    {
        let _outer = as_if_home(&home);
        {
            let _inner = as_if_home(&other);
            let protected = Protected::of_this_process();
            assert!(protected.contains(&other.join("Documents")));
            assert!(!protected.contains(&home.join("Documents")));
        }
        assert!(Protected::of_this_process().contains(&home.join("Documents")));
    }
    assert!(!Protected::of_this_process().contains(&home.join("Documents")));
}
