//! `docs/what-we-run.md` is spec §12's trust file: the one place a person
//! who does not read Rust can see every command Banager runs and every
//! host it contacts. Prose cannot be compiled, so these pin the parts of it
//! the code can vouch for: a section per registered source, every host on
//! the https allowlist, every environment variable brew and npm set, that
//! the Cargo section says every cargo command is given `CargoAdapter::ENV`
//! and shows each entry, the three Homebrew flags the file promises are
//! never passed, and the
//! unknown-source scan's section with the two limits `ScanBudget::default()`
//! enforces, the section on which copy a command runs with the two
//! `CommandBudget::default()` enforces, the folders it reads and the words
//! that it runs no command, the one thing the allowlist refuses that a reader would not
//! expect (an `https://` `OLLAMA_HOST`), every path each path-list
//! uninstall (Claude Code's, Antigravity CLI's, Grok Build's) moves or
//! keeps with that uninstall's time budget, and the never-list's promise
//! to keep each settings-and-state path those lists keep, the read-only
//! check command of a tool asked for its own update check with the words
//! that it installs nothing, the call Banager makes to move a file to the
//! Trash with the pause after each such move, the call Banager makes for a
//! cask app's icon with the size it is drawn at and the words that no
//! command runs for it, that the `PATH` look behind
//! Claude Code's notice goes on past the first executable `claude`, that
//! the never-list's bullet about rustup's own update or uninstall being
//! under way states the window in which a refresh's version read can
//! still overlap it, that the never-list holds a moved path to the tool's
//! uninstall list, not to vendor instructions, which Antigravity CLI and
//! Grok Build do not publish, that pip's section shows the
//! `xcode-select -p` it asks before running an interpreter in `/usr/bin`
//! and says one with no developer tools behind it is skipped, that the
//! document names each permission of the opener plugin the window is
//! given (`src-tauri/capabilities/default.json`) and the one call Show in Finder
//! makes, saying it runs nothing else, that the daily check's section
//! says it is off by default, states its tick, how long after a check it
//! checks again, the waits after daily checks in which every source failed
//! and how many checks they come to a day, says Banager itself runs no
//! install from it and that
//! `brew update` can install a package Homebrew moved between a formula
//! and a cask, names exactly the notification plugin's permissions the
//! window is given, and says the notification's switch is off by default,
//! and that Homebrew's read-only table leaves `brew update` out, whose own
//! paragraph cites the Homebrew lines that install, and that the disk-use
//! section states the two limits `SizeBudget::default()` keeps a round to,
//! names every place `size::Protected` never looks into and says nothing is
//! written, and that the diagnostic-info section names the two kernel
//! strings and the two variables `diagnostics::current` reads and says the
//! text never holds a variable's value. A source, host,
//! variable, limit, path, check, pause, icon size, opener or notification
//! permission or daily-check number added or changed, or that look
//! shortened, without its line in the document fails here.

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::cargo::CargoAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::pip::PipAdapter;
use banager_core::adapters::standalone::recipe::{Latest, Uninstall};
use banager_core::adapters::standalone::recipes::RECIPES;
use banager_core::adapters::standalone::removal::{PUT_BACK_SETTLE, TIMEOUT_SECS};
use banager_core::adapters::standalone::route::shadow_note;
use banager_core::adapters::AdapterMeta;
use banager_core::commands::CommandBudget;
use banager_core::events::VecSink;
use banager_core::http::real::{host_allowed, ALLOWED_HTTPS_HOSTS};
use banager_core::http::HttpError;
use banager_core::icon::ICON_PIXELS;
use banager_core::kept_data;
use banager_core::model::{InstanceNote, KeptWhat};
use banager_core::protected::{OTHER_VOLUMES, PROTECTED_IN_HOME};
use banager_core::runner::HostEnv;
use banager_core::scan::ScanBudget;
use banager_core::session::Session;
use banager_core::size::SizeBudget;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The document, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/banager-core).
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
    // The title line, exactly: the phase 0-1 file was headed "What Banager
    // Runs (Phase 0–1: Homebrew only)". A substring check for "Homebrew
    // only" would misfire on ordinary prose ("passed through to Homebrew
    // only when it was already set", in the never-list).
    assert_eq!(
        doc.lines().next(),
        Some("# What Banager Runs"),
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
    // debugs their daemon instead of Banager.
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
fn test_what_we_run_says_every_cargo_command_is_given_cargos_environment() {
    // `CargoAdapter::ENV` goes on every cargo command Banager runs:
    // `detect`'s `cargo --version` and the command of every plan,
    // cargo-binstall's included (`env_vec`;
    // `test_detect_reads_cargos_version_with_rustups_auto_install_off` and
    // `test_every_cargo_plan_carries_rustups_auto_install_off` in
    // adapters/cargo.rs keep that true). The `## Cargo` section used to say
    // the version read was the only cargo command given a variable and the
    // write commands were given none: it has to say every invocation,
    // under the constant's name, and show each entry -- in that section,
    // since `RUSTUP_AUTO_INSTALL=0` appears in rustup's too.
    let doc = read_doc();
    let body = section_body(&doc, "Cargo")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## Cargo` section"));
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        folded.contains("**Environment applied to every invocation** (`CargoAdapter::ENV`)"),
        "the `## Cargo` section of docs/what-we-run.md does not say that every cargo command is given `CargoAdapter::ENV`, which CargoAdapter::detect and CargoAdapter::plan both use"
    );
    for (name, value) in CargoAdapter::ENV {
        assert!(
            body.contains(&format!("{name}={value}")),
            "the `## Cargo` section of docs/what-we-run.md does not show {name}={value}, which CargoAdapter::ENV holds"
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
    let body = section_body(doc, "What Banager never does").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## What Banager never does` section")
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
    // never-list used to say Banager never runs rustup at all while its
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
fn test_what_we_run_never_list_holds_a_moved_path_to_the_tools_uninstall_list_not_to_vendor_instructions(
) {
    // Check 4 and the ancestry rule (`removal::check_item`) hold every path
    // a path-list uninstall moves to what the tool's list says is there
    // (each item's `Expect`). Claude Code's list is built from Anthropic's
    // removal steps; Antigravity CLI and Grok Build publish none, so their
    // lists are Banager's own reading of how each was installed
    // (`recipes::AGY`, `recipes::GROK` and their fixture READMEs). The
    // bullet saying what Banager never moves used to measure a path against
    // "the tool's uninstall instructions", which named, for those two
    // tools, a document that does not exist.
    let doc = read_doc();
    let bullets = never_list_bullets(&doc);
    let never_moves: Vec<&String> = bullets
        .iter()
        .filter(|b| b.starts_with("Never moves anything"))
        .collect();
    assert_eq!(
        never_moves.len(),
        1,
        "the never-list of docs/what-we-run.md has no single bullet saying what Banager never moves"
    );
    let bullet = never_moves[0];
    assert!(
        bullet.contains("anything that is not what the tool's uninstall list describes"),
        "this never-list bullet does not hold a moved path to the tool's uninstall list, which removal::check_item checks every item against: {bullet:?}"
    );
    assert!(
        bullet.contains("Banager's own reading"),
        "this never-list bullet does not say that the Antigravity CLI and Grok Build lists are Banager's own reading of how each was installed: {bullet:?}"
    );
    assert!(
        !bullet.contains("instructions"),
        "this never-list bullet cites uninstall instructions, which Antigravity CLI and Grok Build do not publish: {bullet:?}"
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

/// `n` with a comma between each three digits, as the document writes a
/// large number: `300000` is "300,000".
fn with_commas(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[test]
fn test_what_we_run_has_the_disk_use_section_with_its_limits_and_every_place_it_never_enters() {
    let doc = read_doc();
    let body = section_body(&doc, "Disk use").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Disk use` section for size::SizeMeter")
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    // The two limits a round keeps to, from `SizeBudget::default()`.
    let budget = SizeBudget::default();
    for limit in [
        format!("{} entries", with_commas(budget.max_entries)),
        format!("{} seconds", budget.max_duration.as_secs()),
    ] {
        assert!(
            folded.contains(&limit),
            "the `## Disk use` section of docs/what-we-run.md does not state the limit {limit:?}, which SizeBudget::default() enforces"
        );
    }
    // Every place it never looks into, as the code lists them.
    for place in PROTECTED_IN_HOME {
        assert!(
            folded.contains(&format!("`~/{place}`")),
            "the `## Disk use` section of docs/what-we-run.md does not name `~/{place}`, which protected::PROTECTED_IN_HOME keeps it out of"
        );
    }
    assert!(
        folded.contains(&format!("`{OTHER_VOLUMES}`")),
        "the `## Disk use` section of docs/what-we-run.md does not name `{OTHER_VOLUMES}`"
    );
    for words in [
        "Measuring runs no command, and the one file it opens is `<CARGO_HOME>/.crates2.json`",
        "a symbolic link is never followed",
        "Nothing is written",
        "`get_sizes`",
    ] {
        assert!(
            folded.contains(words),
            "the `## Disk use` section of docs/what-we-run.md does not say {words:?}"
        );
    }
}

#[test]
fn test_what_we_run_names_every_path_an_uninstall_preview_says_stays_and_its_limits() {
    let doc = read_doc();
    let body = section_body(&doc, "Data an uninstall leaves behind").unwrap_or_else(|| {
        panic!(
            "docs/what-we-run.md has no `## Data an uninstall leaves behind` section for kept_data"
        )
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    // Every path any family's uninstall preview may look at.
    let mut paths: Vec<&str> = banager_core::families::families()
        .iter()
        .flat_map(|family| kept_data::data_paths(&family.id))
        .map(|(path, _)| path)
        .collect();
    paths.dedup();
    assert!(paths.contains(&kept_data::OLLAMA_MODELS));
    for path in paths {
        assert!(
            folded.contains(&format!("`{path}`")),
            "the `## Data an uninstall leaves behind` section of docs/what-we-run.md does not name `{path}`, which kept_data::data_paths looks at"
        );
    }
    let budget = kept_data::BUDGET;
    for limit in [
        format!("{} entries", with_commas(budget.max_entries)),
        format!("{} second", budget.max_duration.as_secs()),
    ] {
        assert!(
            folded.contains(&limit),
            "the `## Data an uninstall leaves behind` section of docs/what-we-run.md does not state the limit {limit:?}, which kept_data::BUDGET enforces"
        );
    }
    for words in [
        "`lstat`, `readdir` and `readlink`; no file is opened",
        "Nothing is written, and nothing is deleted",
        "no button or command that removes these paths",
        "Banager never runs `brew uninstall --zap`",
    ] {
        assert!(
            folded.contains(words),
            "the `## Data an uninstall leaves behind` section of docs/what-we-run.md does not say {words:?}"
        );
    }
}

#[test]
fn test_what_we_run_has_the_command_check_section_with_its_folders_and_both_of_its_limits() {
    let doc = read_doc();
    // Not a registered source either: `commands` reads folders after each
    // refresh's inventories, and the per-source test never asks for it.
    let body = section_body(&doc, "Which copy a command runs").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Which copy a command runs` section for commands.rs")
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let budget = CommandBudget::default();
    for limit in [
        format!("{} entries", budget.max_entries),
        format!("{} seconds", budget.max_duration.as_secs()),
    ] {
        assert!(
            folded.contains(&limit),
            "the `## Which copy a command runs` section of docs/what-we-run.md does not state the limit {limit:?}, which CommandBudget::default() enforces"
        );
    }
    // What it reads, that it runs nothing, and the one flag that turns
    // its verdicts off.
    for phrase in [
        "every `PATH` folder",
        "`bin` and `sbin` folders of every Homebrew prefix",
        "`bin` folder of every npm prefix",
        "runs no command",
        "Nothing's contents are read",
        "`Session::note_login_path`",
        "is not read at all, as named or where it leads (`asks_first`)",
    ] {
        assert!(
            folded.contains(phrase),
            "the `## Which copy a command runs` section of docs/what-we-run.md does not say {phrase:?}"
        );
    }
    // Every place it never reads, as the shared list has them -- the same
    // list as the disk-use section's.
    for place in PROTECTED_IN_HOME {
        assert!(
            folded.contains(&format!("`~/{place}`")),
            "the `## Which copy a command runs` section of docs/what-we-run.md does not name `~/{place}`, which protected::PROTECTED_IN_HOME keeps it out of"
        );
    }
    assert!(
        folded.contains(&format!("`{OTHER_VOLUMES}`")),
        "the `## Which copy a command runs` section of docs/what-we-run.md does not name `{OTHER_VOLUMES}`"
    );
    // And the list of files Banager reads names it too.
    let reads =
        section_body(&doc, "Files Banager reads").expect("a `## Files Banager reads` section");
    let reads = reads.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        reads.contains("Which copy a command runs, at every refresh"),
        "`## Files Banager reads` does not list what the command check reads"
    );
}

#[test]
fn test_what_we_run_shows_the_question_pip_asks_before_running_a_usr_bin_shim() {
    // `PipAdapter::detect` runs an interpreter in `/usr/bin` -- the
    // developer-tool shim `/usr/bin/python3` -- only after asking
    // `xcode-select -p` where the tools are, and skips it, saying nothing,
    // when they are not there. pip's section has to show that command in
    // its read-only table and say the interpreter is then skipped.
    let doc = read_doc();
    let body =
        section_body(&doc, "pip").expect("docs/what-we-run.md has no `## pip` section for pip");
    let argv = PipAdapter::XCODE_SELECT_ARGV.join(" ");
    assert!(
        body.lines()
            .any(|line| line.starts_with('|') && line.contains(&format!("| `{argv}` |"))),
        "pip's read-only table in docs/what-we-run.md does not show `{argv}`, which PipAdapter::detect runs"
    );
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        folded.contains("the interpreter is skipped as if it were not on `PATH`"),
        "pip's section of docs/what-we-run.md does not say an interpreter in /usr/bin with no developer tools behind it is skipped"
    );
}

#[test]
fn test_what_we_run_states_the_read_only_check_command_of_every_tool_that_asks_itself() {
    // A `Latest::Command` recipe runs the tool's own subcommand on every
    // refresh (grok's `update --check --json`, which its --help calls a
    // check "without installing"). The section for that tool has to show
    // the argv and say it installs nothing -- a reader who sees `grok
    // update` in a refresh table and nothing more would think Banager
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
    // is a trust file that no longer says what Banager moves. Every
    // `Paths` recipe (Claude Code, Antigravity CLI, Grok Build since step
    // D), by the name its meta gives its section; each section states the
    // uninstall's budget; and every settings-and-state path a list keeps
    // (`Settings`, `SettingsAndHistory`, `ToolState`) is also named in the
    // never-list, whose promise is the one the reader relies on.
    let doc = read_doc();
    let never = section_body(&doc, "What Banager never does").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## What Banager never does` section")
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

#[test]
fn test_what_we_run_states_the_app_icon_call_its_size_and_that_no_command_runs() {
    let doc = read_doc();
    let body = section_body(&doc, "App icons").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## App icons` section for icon::RealIconRenderer")
    });
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        folded.contains("NSWorkspace iconForFile:"),
        "the `## App icons` section of docs/what-we-run.md does not name the call RealIconRenderer makes"
    );
    let size = format!("{ICON_PIXELS} × {ICON_PIXELS} pixels");
    assert!(
        folded.contains(&size),
        "the `## App icons` section of docs/what-we-run.md does not state the size {size:?} (icon::ICON_PIXELS)"
    );
    assert!(
        folded.contains("runs no command"),
        "the `## App icons` section of docs/what-we-run.md does not say that getting an icon runs no command"
    );
}

#[test]
fn test_what_we_run_names_the_opener_permission_the_window_has_and_what_show_in_finder_calls() {
    let doc = read_doc();
    // The window's permissions (`src-tauri/capabilities/default.json`),
    // read from the repository as the document is: the plugin that opens a
    // URL or a path in another application is the one the Network section
    // says the window may call, and how far.
    let path = Path::new("../../src-tauri/capabilities/default.json");
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let capability: serde_json::Value = serde_json::from_str(&text).expect("default.json is JSON");
    let permissions = capability["permissions"]
        .as_array()
        .expect("default.json lists its permissions");
    let opener: Vec<&str> = permissions
        .iter()
        .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
        .filter(|p| p.starts_with("opener:"))
        .collect();
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = doc.split_whitespace().collect::<Vec<_>>().join(" ");
    for permission in &opener {
        assert!(
            folded.contains(&format!("`{permission}`")),
            "docs/what-we-run.md does not name {permission:?}, which src-tauri/capabilities/default.json gives the window"
        );
    }
    assert!(
        opener.contains(&"opener:default") || !folded.contains("`opener:default`"),
        "docs/what-we-run.md names `opener:default`, which src-tauri/capabilities/default.json no longer gives the window"
    );
    // What Show in Finder, the one caller, asks of macOS, and that it asks
    // nothing more.
    let body = section_body(&doc, "Unknown-source scan").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Unknown-source scan` section for scan::scan_unknown")
    });
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        body.contains("NSWorkspace activateFileViewerSelectingURLs:"),
        "the `## Unknown-source scan` section of docs/what-we-run.md does not name the call Show in Finder makes"
    );
    assert!(
        body.contains("runs nothing else"),
        "the `## Unknown-source scan` section of docs/what-we-run.md does not say that Show in Finder runs nothing else"
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
            "banager-what-we-run-{tag}-{}-{}",
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
    // own and the list of files Banager reads -- have to say it goes on: a
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
    for section in ["Claude Code", "Files Banager reads"] {
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

#[test]
fn test_what_we_run_says_the_daily_check_is_off_how_often_it_looks_when_it_checks_and_that_banager_runs_no_install_from_it(
) {
    // The daily check's section states the numbers `auto_check::tick` and
    // the shell's task run on -- a look every `TICK`, a check once
    // `DUE_AFTER_SECS` have passed since the last one ended, and after a
    // daily check in which every source failed the waits
    // `retry_after_secs` gives, from the first up to the cap, with the
    // minute of `RETRY_SLACK_SECS` and how many checks those waits come to
    // a day -- that `Settings::default()` leaves it off, and that Banager
    // itself runs no install from it (it runs the refresh Check again
    // runs, and no refresh runs a write command of Banager's), with the
    // exception Homebrew's `brew update` makes: it can install a package
    // Homebrew moved between a formula and a cask. A number changed, or
    // the default turned on, without the section following fails here.
    use banager_core::auto_check::{
        retry_after_secs, DUE_AFTER_SECS, RETRY_CAP_SECS, RETRY_FIRST_SECS, RETRY_SLACK_SECS, TICK,
    };
    use banager_core::settings::Settings;
    assert!(
        !Settings::default().auto_check,
        "the daily check is off by default today; if that has changed, the section's \"off by default\" is now false and must go with it"
    );
    assert_eq!(TICK.as_secs() % 60, 0, "TICK is a whole number of minutes");
    assert_eq!(
        DUE_AFTER_SECS % 3600,
        0,
        "DUE_AFTER_SECS is a whole number of hours"
    );
    assert_eq!(
        RETRY_SLACK_SECS, 60,
        "the section says a look up to a minute short of a wait checks"
    );
    // The waits after 1, 2, 3... failed daily checks in a row, in minutes,
    // short of the cap: the first, then each doubling.
    let waits: Vec<i64> = (1..)
        .map(retry_after_secs)
        .take_while(|&wait| wait < RETRY_CAP_SECS)
        .map(|wait| {
            assert_eq!(wait % 60, 0, "every wait is a whole number of minutes");
            wait / 60
        })
        .collect();
    assert_eq!(
        waits.first().copied(),
        Some(RETRY_FIRST_SECS / 60),
        "the first wait is RETRY_FIRST_SECS"
    );
    let (last, between) = waits[1..]
        .split_last()
        .expect("at least two doublings before the cap");
    let doublings = format!(
        "{} and {last} minutes",
        between
            .iter()
            .map(|minutes| minutes.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    // How many checks those waits come to, on a Mac that stays awake and
    // offline: in the 24 hours from the first that fails, and in each 24
    // hours once the waits are at the cap.
    let day = 24 * 60 * 60;
    let mut checks = vec![0_i64];
    for in_a_row in 1.. {
        let next = checks.last().copied().unwrap_or(0) + retry_after_secs(in_a_row);
        if next >= 3 * day {
            break;
        }
        checks.push(next);
    }
    let first_day = checks.iter().filter(|&&at| at < day).count();
    let later_day = checks
        .iter()
        .filter(|&&at| (2 * day..3 * day).contains(&at))
        .count();
    assert_eq!(
        later_day as i64,
        day / RETRY_CAP_SECS,
        "at the cap, a day holds as many checks as it holds caps"
    );
    let doc = read_doc();
    let body = section_body(&doc, "The daily check")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## The daily check` section"));
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "off by default".to_string(),
        format!("every {} minutes", TICK.as_secs() / 60),
        format!("{} hours", DUE_AFTER_SECS / 3600),
        format!(
            "the look {} minutes after the one that started it checks again",
            RETRY_FIRST_SECS / 60
        ),
        doublings,
        format!("up to {} minutes", RETRY_CAP_SECS / 60),
        "up to a minute of a wait left".to_string(),
        format!(
            "{first_day} times in the 24 hours from the first check that fails and {later_day} times a day after that"
        ),
        "Banager itself runs no install".to_string(),
        "between a formula and a cask".to_string(),
    ] {
        assert!(
            folded.contains(&phrase),
            "the `## The daily check` section of docs/what-we-run.md does not say {phrase:?}, which auto_check::TICK, auto_check::DUE_AFTER_SECS, auto_check::retry_after_secs, Settings::default() and the refresh it runs make true"
        );
    }
}

#[test]
fn test_what_we_run_keeps_brew_update_out_of_homebrews_read_only_table_and_cites_the_lines_that_install(
) {
    // `brew update` carries out what Homebrew's new index says has moved:
    // a cask moved to a formula gets the formula installed, a formula moved
    // to a cask gets unlinked, a `brew cleanup` and the cask installed
    // (Homebrew 7.0.6, cmd/update_report/reporter.rb:257-261, :288-295).
    // It is no read-only command, and Homebrew's section says what it
    // installs, citing those lines.
    let doc = read_doc();
    let body = section_body(&doc, "Homebrew")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## Homebrew` section"));
    let read_only = body
        .split("**Read-only commands**")
        .nth(1)
        .and_then(|rest| rest.split("\n\n**").next())
        .expect("Homebrew's section has a read-only table");
    assert!(
        read_only.contains("`<brew> outdated --json=v2`"),
        "precondition: this is Homebrew's read-only table"
    );
    assert!(
        !read_only.contains("`<brew> update`"),
        "Homebrew's read-only table lists `brew update`, which can install"
    );
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "`<brew> update`",
        "`cmd/update_report/reporter.rb:257-261`",
        "(`:288-295`)",
    ] {
        assert!(
            folded.contains(phrase),
            "the `## Homebrew` section of docs/what-we-run.md does not show {phrase:?}"
        );
    }
}

#[test]
fn test_what_we_run_names_the_notification_plugins_permissions_and_says_the_switch_is_off_by_default(
) {
    // The daily check's section names every permission of the notification
    // plugin the window has (`src-tauri/capabilities/default.json`) and no
    // other, and says that "Notify me when there are updates" is off by
    // default, which `Settings::default()` makes true.
    use banager_core::settings::Settings;
    assert!(
        !Settings::default().notify_updates,
        "notifications are off by default today; if that has changed, the section's \"off by default too\" is now false and must go with it"
    );
    let path = Path::new("../../src-tauri/capabilities/default.json");
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let capability: serde_json::Value = serde_json::from_str(&text).expect("default.json is JSON");
    let given: std::collections::BTreeSet<&str> = capability["permissions"]
        .as_array()
        .expect("default.json lists its permissions")
        .iter()
        .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
        .filter(|p| p.starts_with("notification:"))
        .collect();
    let doc = read_doc();
    let body = section_body(&doc, "The daily check")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## The daily check` section"));
    // Hard-wrapped prose: compare with the line breaks folded away. What
    // sits between backticks is every odd piece.
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let named: std::collections::BTreeSet<&str> = folded
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|quoted| quoted.starts_with("notification:"))
        .collect();
    assert_eq!(
        named, given,
        "the `## The daily check` section of docs/what-we-run.md must name exactly the notification plugin's permissions src-tauri/capabilities/default.json gives the window"
    );
    assert!(
        folded.contains("\"Notify me when there are updates\" (「有更新时通知我」), off by default too (`Settings::notify_updates`)"),
        "the `## The daily check` section of docs/what-we-run.md does not say the notification's switch is off by default"
    );
}

#[test]
fn test_what_we_run_has_the_diagnostic_info_section_saying_what_it_reads_and_never_holds() {
    let doc = read_doc();
    let body = section_body(&doc, "Diagnostic info").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Diagnostic info` section for diagnostics::current")
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "`get_system_facts`",
        "`kern.osproductversion` and `machdep.cpu.brand_string`, with `sysctlbyname`",
        "the process's own `PATH` and `HOME`, and no other environment variable",
        "No command runs, no file is opened, nothing is written to disk, and no connection is made",
        "the home folder written as `~`",
        "The folders on `PATH` are the one environment variable's value the text holds",
        "never holds any other environment variable's value",
        "the Help menu's item never does",
    ] {
        assert!(
            folded.contains(words),
            "the `## Diagnostic info` section of docs/what-we-run.md does not say {words:?}"
        );
    }
}

#[test]
fn test_what_we_run_names_the_history_file_what_it_keeps_its_bounds_and_how_to_remove_it() {
    // `history.json` (crates/banager-core/src/history/mod.rs) is the one
    // file Banager writes besides its settings and its window's state:
    // every place that lists what Banager writes names it, and the section
    // says what is in it, what never is, its two bounds -- the constants
    // themselves -- and how to remove it.
    use banager_core::history::{MAX_AGE_MS, MAX_RECORDS};
    let doc = read_doc();
    let writes = section_body(&doc, "Files Banager writes")
        .expect("a `## Files Banager writes` section")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for phrase in [
        "Three, all in Banager's application data directory".to_string(),
        "`history.json`, Banager's record of the updates and uninstalls it ran".to_string(),
        "Never a line of a log, a command line, an error message or any other path".to_string(),
        "An operation cancelled before its command started is not recorded.".to_string(),
        format!(
            "The file keeps the newest {} records and nothing older than {} days.",
            with_commas(MAX_RECORDS as u64),
            MAX_AGE_MS / 86_400_000
        ),
        "renamed into place, on a thread of its own".to_string(),
        "a file a newer Banager wrote is left exactly as it is".to_string(),
        "To remove the history, quit Banager and delete `history.json`".to_string(),
        "Clear does not delete it.".to_string(),
    ] {
        assert!(
            writes.contains(&phrase),
            "`## Files Banager writes` does not say {phrase:?}"
        );
    }
    let reads = section_body(&doc, "Files Banager reads")
        .expect("a `## Files Banager reads` section")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        reads.contains("Banager's own `history.json` beside it, once, as Banager starts"),
        "`## Files Banager reads` does not list the history"
    );
    let never = never_list_bullets(&doc);
    assert!(
        never.iter().any(|b| b.contains(
            "Never writes a file on the Mac itself other than its own `settings.json`, `history.json` and `.window-state.json`"
        )),
        "the never-list does not name the history among the files Banager writes"
    );
    let trash = section_body(&doc, "Moving files to the Trash")
        .expect("a `## Moving files to the Trash` section")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(trash.contains("other than its own settings and history."));
    // No place still promises settings.json and the window's state alone.
    let folded = doc.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(!folded.contains("other than its own `settings.json` and `.window-state.json`"));
    assert!(!folded.contains("Two, both in Banager's application data directory"));
}
