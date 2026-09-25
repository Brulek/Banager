//! `docs/what-we-run.md` is spec §12's trust file: the one place a person
//! who does not read Rust can see every command Canager runs and every
//! host it contacts. Prose cannot be compiled, so these pin the parts of it
//! the code can vouch for: a section per registered source, every host on
//! the https allowlist, every environment variable brew and npm set, the
//! three Homebrew flags the file promises are never passed, and the
//! unknown-source scan's section with the two limits `ScanBudget::default()`
//! enforces, the one thing the allowlist refuses that a reader would not
//! expect (an `https://` `OLLAMA_HOST`), every path Claude Code's uninstall
//! moves or keeps with that uninstall's time budget, the call Canager
//! makes to move a file to the Trash with the pause after each such move,
//! and that the `PATH` look behind Claude Code's notice goes on past the
//! first executable `claude`. A source, host, variable, limit, path or
//! pause added or changed, or that look shortened, without its line in the
//! document fails here.

use canager_core::adapters::brew::BrewAdapter;
use canager_core::adapters::npm::NpmAdapter;
use canager_core::adapters::standalone::recipe::Uninstall;
use canager_core::adapters::standalone::recipes::CLAUDE;
use canager_core::adapters::standalone::removal::{PUT_BACK_SETTLE, TIMEOUT_SECS};
use canager_core::adapters::standalone::route::shadow_note;
use canager_core::adapters::AdapterMeta;
use canager_core::events::VecSink;
use canager_core::http::real::{host_allowed, ALLOWED_HTTPS_HOSTS};
use canager_core::http::HttpError;
use canager_core::model::InstanceNote;
use canager_core::runner::HostEnv;
use canager_core::scan::ScanBudget;
use canager_core::session::Session;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The document, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/canager-core).
fn read_doc() -> String {
    let path = Path::new("../../docs/what-we-run.md");
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Whether `line` is a `## ` heading whose text is `name`, or `name`
/// followed by a space or a colon -- so `## pip` is found by "pip" and not
/// by "pipx", and `## pip (read-only)` still counts.
fn is_heading_for(line: &str, name: &str) -> bool {
    line.strip_prefix("## ").is_some_and(|text| {
        text == name
            || text.starts_with(&format!("{name} "))
            || text.starts_with(&format!("{name}:"))
    })
}

fn has_section(doc: &str, name: &str) -> bool {
    doc.lines().any(|line| is_heading_for(line, name))
}

/// The lines under the `## ` heading for `name`, up to the next `## `
/// heading; `None` when there is no such heading. A phrase found here was
/// stated in that section, not somewhere else in the file.
fn section_body(doc: &str, name: &str) -> Option<String> {
    let mut lines = doc.lines().skip_while(|line| !is_heading_for(line, name));
    lines.next()?;
    Some(
        lines
            .take_while(|line| !line.starts_with("## "))
            .collect::<Vec<_>>()
            .join("\n"),
    )
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
fn test_what_we_run_says_an_https_ollama_host_is_refused_and_it_is() {
    // `normalize_ollama_host` keeps an `https://` OLLAMA_HOST as it is
    // (`runner/path_env.rs`), `OllamaAdapter::detect` then asks
    // `{host}/api/tags`, and `host_allowed` exempts `http` only -- so the
    // request is refused before it is sent, and `detect`, which discards
    // the error, reports the daemon as one that did not answer. Both
    // sections of the document that describe that host have to say so:
    // a reader who is told https is accepted and the daemon host is exempt
    // debugs their daemon instead of Canager.
    let refused = host_allowed("https://ollama.home.lan/api/tags");
    assert!(
        matches!(&refused, Err(HttpError::Network(message)) if message.contains("host not allowed")),
        "an https OLLAMA_HOST is refused by the allowlist today; if that has changed, the sentences this test looks for are now false and must go with it: {refused:?}"
    );
    let doc = read_doc();
    for section in ["Ollama", "Network"] {
        let body = section_body(&doc, section)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {section}` section"));
        // Hard-wrapped prose: compare with the line breaks folded away.
        let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            folded.contains("an `https://` `OLLAMA_HOST` is refused"),
            "the `## {section}` section of docs/what-we-run.md does not say that an `https://` `OLLAMA_HOST` is refused, which host_allowed does"
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

#[test]
fn test_what_we_run_has_the_unknown_scan_section_stating_both_of_its_limits() {
    let doc = read_doc();
    // The scan is not a registered source (it is not an `Adapter`), so the
    // per-source test above never asks for its section.
    let body = section_body(&doc, "Unknown-source scan").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Unknown-source scan` section for scan::scan_unknown")
    });
    // The two numbers the page's banner prints come from
    // `ScanBudget::default()`; the section has to state those same two,
    // in its own text, so a change to the budget without its line here
    // fails.
    let budget = ScanBudget::default();
    for limit in [
        format!("{} entries", budget.max_entries),
        format!("{} seconds", budget.max_duration.as_secs()),
    ] {
        assert!(
            body.contains(&limit),
            "the `## Unknown-source scan` section of docs/what-we-run.md does not state the limit {limit:?}, which ScanBudget::default() enforces"
        );
    }
}

#[test]
fn test_what_we_run_names_every_path_claude_codes_uninstall_moves_or_keeps() {
    // The list is the recipe's, and a reader deciding whether to press
    // Uninstall reads it here: a path added to or dropped from
    // `CLAUDE.uninstall` without this section changing is a trust file
    // that no longer says what Canager moves.
    let doc = read_doc();
    let body = section_body(&doc, "Claude Code")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## Claude Code` section"));
    let Some(Uninstall::Paths { remove, keep }) = &CLAUDE.uninstall else {
        panic!("CLAUDE carries a path-list uninstall since phase 4 step C");
    };
    let listed = remove
        .iter()
        .map(|spec| spec.path)
        .chain(keep.iter().map(|spec| spec.path));
    for path in listed {
        assert!(
            body.contains(&format!("`{path}`")),
            "the `## Claude Code` section of docs/what-we-run.md does not name `{path}`, which CLAUDE.uninstall lists"
        );
    }
    let budget = format!("{TIMEOUT_SECS} s");
    assert!(
        body.contains(&budget),
        "the `## Claude Code` section of docs/what-we-run.md does not state the uninstall's budget, {budget:?} (removal::TIMEOUT_SECS)"
    );
}

#[test]
fn test_what_we_run_states_the_trash_call_and_the_pause_after_each_move() {
    let doc = read_doc();
    let body = section_body(&doc, "Moving files to the Trash").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Moving files to the Trash` section for trash::RealTrasher")
    });
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        folded.contains("trashItemAtURL:"),
        "the `## Moving files to the Trash` section of docs/what-we-run.md does not name the call RealTrasher makes"
    );
    let pause = format!("{} seconds", PUT_BACK_SETTLE.as_secs());
    assert!(
        folded.contains(&pause),
        "the `## Moving files to the Trash` section of docs/what-we-run.md does not state the pause {pause:?} (removal::PUT_BACK_SETTLE)"
    );
}

/// A fresh, canonical directory under the system temp dir, removed when
/// the test ends, for the synthetic `claude` files the `PATH`-look test
/// below builds: real links and real executable files, never run, since
/// `route::shadow_note` answers from `stat` and `realpath` and nothing
/// else. Canonical, so the paths built from it compare equal to what
/// `canonicalize` answers (macOS's `/var/folders` is `/private/var/…`).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "canager-what-we-run-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp home");
        Home(std::fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// An executable regular file at `rel` under the home (parents
    /// created). It is never run.
    fn executable(&self, rel: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
        std::fs::write(&path, b"#!/bin/sh\n").expect("write file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("executable file");
        path
    }

    /// A symbolic link at `rel` under the home whose text is `target`
    /// (parents created).
    fn link(&self, rel: &str, target: &Path) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
        std::os::unix::fs::symlink(target, &path).expect("symlink");
    }

    /// `HostEnv` for this home, as the user who owns it (the one running
    /// the test), whose `PATH` is `path_dirs`.
    fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
        HostEnv {
            path_dirs,
            home: self.0.clone(),
            euid: std::fs::metadata(&self.0).expect("stat home").uid(),
            cargo_home: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_what_we_run_says_the_path_look_goes_on_past_the_first_claude_and_it_does() {
    // Since b93cacd `route::shadow_note` does not stop at the first
    // executable `claude` on `PATH`: when that one is not this copy it
    // looks on down `PATH` for an entry that resolves to this copy, and
    // which note the source gets depends on whether one is there. Shown
    // here with one `PATH` head, Homebrew's copy, and this copy's launcher
    // behind it or not: the same first executable, two different notes.
    // Both sections of the document that describe the look -- the source's
    // own and the list of files Canager reads -- have to say it goes on: a
    // reader told it stops at the first executable would not know that
    // every later `PATH` directory's `claude` may be read as well.
    let home = Home::new("path-look");
    let real = home.executable(".local/share/claude/versions/2.1.281");
    home.link(".local/bin/claude", &real);
    let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
    home.link("opt/homebrew/bin/claude", &cask);
    let brew_bin = home.path().join("opt/homebrew/bin");
    let local_bin = home.path().join(".local/bin");
    let behind = shadow_note(
        "claude",
        &home.env(vec![brew_bin.clone(), local_bin]),
        &real,
    );
    let alone = shadow_note("claude", &home.env(vec![brew_bin]), &real);
    assert_eq!(
        (behind, alone),
        (
            Some(InstanceNote::ShadowedByHomebrew),
            Some(InstanceNote::NotOnPath)
        ),
        "shadow_note looks on down PATH past the first executable claude today (b93cacd); if that has changed, the sentences this test looks for are now false and must go with it"
    );
    let doc = read_doc();
    for section in ["Claude Code", "Files Canager reads"] {
        let body = section_body(&doc, section)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {section}` section"));
        // Hard-wrapped prose: compare with the line breaks folded away.
        let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            folded.contains("on down `PATH`"),
            "the `## {section}` section of docs/what-we-run.md does not say the look goes on down `PATH` past the first executable `claude`, which route::shadow_note does"
        );
    }
}
