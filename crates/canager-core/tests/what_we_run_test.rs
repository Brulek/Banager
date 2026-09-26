//! `docs/what-we-run.md` is spec §12's trust file: the one place a person
//! who does not read Rust can see every command Canager runs and every
//! host it contacts. Prose cannot be compiled, so these pin the parts of it
//! the code can vouch for: a section per registered source, every host on
//! the https allowlist, every environment variable brew and npm set, the
//! three Homebrew flags the file promises are never passed, and the
//! unknown-source scan's section with the two limits `ScanBudget::default()`
//! enforces, the one thing the allowlist refuses that a reader would not
//! expect (an `https://` `OLLAMA_HOST`), every path each path-list
//! uninstall (Claude Code's, Antigravity CLI's, Grok Build's) moves or
//! keeps with that uninstall's time budget, and the never-list's promise
//! to keep each settings-and-state path those lists keep, the read-only
//! check command of a tool asked for its own update check with the words
//! that it installs nothing, the call Canager makes to move a file to the
//! Trash with the pause after each such move, that the `PATH` look behind
//! Claude Code's notice goes on past the first executable `claude`, and
//! that the never-list's bullet about rustup's own update or uninstall
//! being under way states the window in which a refresh's version read
//! can still overlap it. A source, host, variable, limit, path, check or
//! pause added or changed, or that look shortened, without its line in
//! the document fails here.

use canager_core::adapters::brew::BrewAdapter;
use canager_core::adapters::npm::NpmAdapter;
use canager_core::adapters::standalone::recipe::{Latest, Uninstall};
use canager_core::adapters::standalone::recipes::RECIPES;
use canager_core::adapters::standalone::removal::{PUT_BACK_SETTLE, TIMEOUT_SECS};
use canager_core::adapters::standalone::route::shadow_note;
use canager_core::adapters::AdapterMeta;
use canager_core::events::VecSink;
use canager_core::http::real::{host_allowed, ALLOWED_HTTPS_HOSTS};
use canager_core::http::HttpError;
use canager_core::model::{InstanceNote, KeptWhat};
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

/// The never-list's bullets, each hard-wrapped bullet folded into one
/// line, so a phrase split across a line break is still found and a phrase
/// is attributed to the bullet it is in and not to its neighbour.
fn never_list_bullets(doc: &str) -> Vec<String> {
    let body = section_body(doc, "What Canager never does").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## What Canager never does` section")
    });
    let mut bullets: Vec<String> = Vec::new();
    for line in body.lines() {
        if let Some(first) = line.strip_prefix("- ") {
            bullets.push(first.trim().to_string());
        } else if let Some(last) = bullets.last_mut() {
            let rest = line.trim();
            if !rest.is_empty() {
                last.push(' ');
                last.push_str(rest);
            }
        }
    }
    bullets
}

#[test]
fn test_what_we_run_never_list_states_the_window_in_which_a_refresh_can_still_read_rustup_during_its_update(
) {
    // `Session::refresh_round` reads the set of held locks once, before its
    // detection fan-out, and runs each adapter's `detect` under no lock
    // (session/refresh.rs, the comment above its `locks_held()` call). A
    // refresh whose read found rustup's update or uninstall holding
    // rustup's and cargo's locks runs neither `rustup --version` nor
    // `cargo --version`, which
    // `test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo`
    // in session/refresh.rs keeps true for the update (the uninstall holds
    // the same two locks); an operation that acquires those locks after
    // that read can overlap the version reads that refresh is already
    // making, which the `## rustup` section discloses. The
    // never-list used to say Canager never runs rustup at all while its
    // update or uninstall is under way, which that window makes false: the
    // bullet that speaks of rustup's update or uninstall being under way
    // has to make the promise the refresh test keeps and state the overlap
    // the section states, in the same breath.
    let doc = read_doc();
    let bullets = never_list_bullets(&doc);
    let about_rustup: Vec<&String> = bullets
        .iter()
        .filter(|b| b.contains("rustup") && b.contains("under way"))
        .collect();
    assert!(
        !about_rustup.is_empty(),
        "the never-list of docs/what-we-run.md has no bullet about rustup's update or uninstall being under way"
    );
    for bullet in about_rustup {
        assert!(
            bullet.contains("neither `rustup` nor `cargo`"),
            "this never-list bullet does not say a refresh that finds rustup's update or uninstall under way runs neither `rustup` nor `cargo`, which test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo keeps true for the update, whose two locks the uninstall holds too: {bullet:?}"
        );
        assert!(
            bullet.contains("overlap"),
            "this never-list bullet does not state that an update or uninstall starting after a refresh's one look at the held locks can overlap that refresh's version reads, which Session::refresh_round's one read of locks_held() leaves possible: {bullet:?}"
        );
    }
    let rustup = section_body(&doc, "rustup")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## rustup` section"));
    assert!(
        rustup.contains("overlap"),
        "the `## rustup` section of docs/what-we-run.md no longer discloses the overlap the never-list's bullet points at"
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
fn test_what_we_run_states_the_read_only_check_command_of_every_tool_that_asks_itself() {
    // A `Latest::Command` recipe runs the tool's own subcommand on every
    // refresh (grok's `update --check --json`, which its --help calls a
    // check "without installing"). The section for that tool has to show
    // the argv and say it installs nothing -- a reader who sees `grok
    // update` in a refresh table and nothing more would think Canager
    // upgrades grok behind their back.
    let doc = read_doc();
    for recipe in RECIPES {
        let Latest::Command { args, .. } = recipe.latest else {
            continue;
        };
        let meta = AdapterMeta::from_toml(recipe.meta_toml).expect("meta");
        let body = section_body(&doc, &meta.name)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {}` section", meta.name));
        let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            folded.contains(&args.join(" ")),
            "the `## {}` section does not show `{}`",
            meta.name,
            args.join(" ")
        );
        assert!(
            folded.contains("without installing"),
            "the `## {}` section does not say the check installs nothing",
            meta.name
        );
    }
}

#[test]
fn test_what_we_run_names_every_path_a_path_list_uninstall_moves_or_keeps() {
    // The lists are the recipes', and a reader deciding whether to press
    // Uninstall reads them here: a path added to or dropped from a
    // recipe's `uninstall` or `backup_globs` without its section changing
    // is a trust file that no longer says what Canager moves. Every
    // `Paths` recipe (Claude Code, Antigravity CLI, Grok Build since step
    // D), by the name its meta gives its section; each section states the
    // uninstall's budget; and every settings-and-state path a list keeps
    // (`Settings`, `SettingsAndHistory`, `ToolState`) is also named in the
    // never-list, whose promise is the one the reader relies on.
    let doc = read_doc();
    let never = section_body(&doc, "What Canager never does").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## What Canager never does` section")
    });
    let budget = format!("{TIMEOUT_SECS} s");
    let mut paths_recipes = 0;
    for recipe in RECIPES {
        let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
            continue;
        };
        paths_recipes += 1;
        let meta = AdapterMeta::from_toml(recipe.meta_toml).expect("meta");
        let body = section_body(&doc, &meta.name)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {}` section", meta.name));
        let listed = remove
            .iter()
            .map(|spec| spec.path)
            .chain(keep.iter().map(|spec| spec.path))
            .chain(recipe.backup_globs.iter().map(|glob| glob.dir));
        for path in listed {
            assert!(
                body.contains(&format!("`{path}`")),
                "the `## {}` section of docs/what-we-run.md does not name `{path}`, which {}'s uninstall lists",
                meta.name,
                recipe.id
            );
        }
        assert!(
            body.contains(&budget),
            "the `## {}` section of docs/what-we-run.md does not state the uninstall's budget, {budget:?} (removal::TIMEOUT_SECS)",
            meta.name
        );
        for spec in keep.iter().filter(|spec| {
            matches!(
                spec.what,
                KeptWhat::Settings | KeptWhat::SettingsAndHistory | KeptWhat::ToolState
            )
        }) {
            assert!(
                never.contains(&format!("`{}`", spec.path)),
                "the never-list of docs/what-we-run.md does not promise `{}` stays, which {}'s uninstall keeps",
                spec.path,
                recipe.id
            );
        }
    }
    assert_eq!(
        paths_recipes, 3,
        "Claude Code, Antigravity CLI and Grok Build uninstall by path list"
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
            rustup_home: None,
            zdotdir: None,
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
