//! `docs/what-we-run.md` is spec §12's trust file: the one place a person
//! who does not read Rust can see every command Canager runs and every
//! host it contacts. Prose cannot be compiled, so these pin the parts of it
//! the code can vouch for: a section per registered source, every host on
//! the https allowlist, every environment variable brew and npm set, and
//! the three Homebrew flags the file promises are never passed. A source,
//! host or variable added without its line in the document fails here.

use canager_core::adapters::brew::BrewAdapter;
use canager_core::adapters::npm::NpmAdapter;
use canager_core::adapters::AdapterMeta;
use canager_core::events::VecSink;
use canager_core::http::real::ALLOWED_HTTPS_HOSTS;
use canager_core::session::Session;
use std::path::Path;
use std::sync::Arc;

/// The document, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/canager-core).
fn read_doc() -> String {
    let path = Path::new("../../docs/what-we-run.md");
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A `## ` heading whose text is `name`, or `name` followed by a space or a
/// colon -- so `## pip` is found by "pip" and not by "pipx", and
/// `## pip (read-only)` still counts.
fn has_section(doc: &str, name: &str) -> bool {
    doc.lines().any(|line| {
        line.strip_prefix("## ").is_some_and(|text| {
            text == name
                || text.starts_with(&format!("{name} "))
                || text.starts_with(&format!("{name}:"))
        })
    })
}

#[test]
fn test_what_we_run_has_a_section_for_every_registered_source() {
    let doc = read_doc();
    let session = Session::new(Arc::new(VecSink::new()), None);
    for id in session.adapter_ids() {
        let meta_path = format!("../../adapters/meta/{id}.toml");
        let meta = AdapterMeta::from_toml(
            &std::fs::read_to_string(&meta_path)
                .unwrap_or_else(|e| panic!("read {meta_path}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse {meta_path}: {e}"));
        assert!(
            has_section(&doc, &meta.name),
            "docs/what-we-run.md has no `## {}` section for the registered source {id:?}",
            meta.name
        );
    }
    // The title line, exactly: the phase 0-1 file was headed "What Canager
    // Runs (Phase 0–1: Homebrew only)". A substring check for "Homebrew
    // only" would misfire on ordinary prose ("passed through to Homebrew
    // only when it was already set", in the never-list).
    assert_eq!(
        doc.lines().next(),
        Some("# What Canager Runs"),
        "docs/what-we-run.md's title still narrows the file to one source"
    );
}

#[test]
fn test_what_we_run_names_every_allowed_https_host() {
    let doc = read_doc();
    for host in ALLOWED_HTTPS_HOSTS {
        assert!(
            doc.contains(host),
            "docs/what-we-run.md does not name {host:?}, which ALLOWED_HTTPS_HOSTS allows"
        );
    }
}

#[test]
fn test_what_we_run_shows_every_environment_variable_brew_and_npm_set() {
    let doc = read_doc();
    for (name, value) in BrewAdapter::ENV.iter().chain(NpmAdapter::ENV.iter()) {
        assert!(
            doc.contains(&format!("{name}={value}")),
            "docs/what-we-run.md does not show {name}={value}"
        );
    }
}

#[test]
fn test_what_we_run_promises_the_three_brew_flags_are_never_passed() {
    let doc = read_doc();
    // The promise, not the flags' spellings: the Cargo section lists
    // `cargo install --force`, so `doc.contains("--force")` would pass
    // with the Homebrew sentence gone. One line has to name all three
    // flags, the word "never" and Homebrew together -- the never-list's
    // bullet does.
    let promised = doc.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("never")
            && lower.contains("homebrew")
            && ["--zap", "--force", "--ignore-dependencies"]
                .iter()
                .all(|flag| line.contains(flag))
    });
    assert!(
        promised,
        "docs/what-we-run.md has no line promising Homebrew is never passed --zap, --force and --ignore-dependencies, which test_plan_never_passes_zap_force_or_ignore_dependencies keeps true"
    );
}
