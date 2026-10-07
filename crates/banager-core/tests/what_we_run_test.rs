//! `docs/what-we-run.md` is spec §12's trust file: the one place a person
//! who does not read Rust can see every command Banager runs and every
//! host it contacts. Prose cannot be compiled, so these pin the parts of it
//! the code can vouch for: a section per registered source, every host on
//! the https allowlist, every environment variable brew and npm set, that
//! the Cargo section says every cargo command is given `CargoAdapter::ENV`
//! and shows each entry, the three Homebrew flags the file promises are
//! never passed but for the one `--force` of U9, the `brew cleanup` after a
//! formula's update with its time limit and, in the never-list, the cache
//! downloads it deletes besides the formula's old versions, and the
//! unknown-source scan's section with the two limits `ScanBudget::default()`
//! enforces and every protected place it never reads, the section on which copy a command runs with the two
//! `CommandBudget::default()` enforces, the folders it reads and the words
//! that it runs no command, the one thing the allowlist refuses that a reader would not
//! expect (an `https://` `OLLAMA_HOST`), every path each path-list
//! uninstall (Claude Code's, Antigravity CLI's, Grok Build's, Codex's) moves or
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
//! document says whether the opener plugin is built in
//! (`src-tauri/Cargo.toml`) and names each permission the window is
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
fn test_what_we_run_intro_counts_and_names_every_registered_source() {
    // The opening paragraph says how many sources there are, and how many
    // of them are tools with their own installer, in words; it said
    // "eleven" and "four" after Codex's own install made twelve.
    let doc = read_doc();
    let intro: String = doc
        .lines()
        .skip(2)
        .take_while(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let session = Session::new(Arc::new(VecSink::new()), None);
    let ids = session.adapter_ids();
    let words = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
    ];
    let sources = format!("for the {} sources it manages today", words[ids.len()]);
    assert!(
        intro.contains(&sources),
        "the intro does not say {sources:?}"
    );
    let own = format!("{} tools with their own installer", words[RECIPES.len()]);
    assert!(intro.contains(&own), "the intro does not say {own:?}");
    for id in ids {
        let meta_path = format!("../../adapters/meta/{id}.toml");
        let meta = AdapterMeta::from_toml(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert!(
            intro.contains(&meta.name),
            "the intro does not name {:?}",
            meta.name
        );
    }
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
    // (`runner/path_env.rs`), and `host_allowed` exempts `http` only -- so
    // `OllamaAdapter::detect`, which asks that same function, does not
    // send `{host}/api/tags` and reports `HttpsHostRefused`, whose notice
    // says connecting to Ollama over https isn't supported. Both sections
    // of the document that describe that host have to say it is refused:
    // a reader who is told https is accepted and the daemon host is exempt
    // debugs their daemon instead of Banager.
    let refused = host_allowed("https://ollama.home.lan/api/tags");
    assert!(
        matches!(&refused, Err(HttpError::Refused(message)) if message.contains("host not allowed")),
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
        // And what the window shows for it, which is no longer a daemon
        // that did not answer.
        assert!(
            folded.contains("\"Connecting to Ollama over https isn't supported\""),
            "the `## {section}` section of docs/what-we-run.md does not say what the notice for that Ollama says"
        );
        assert!(
            !folded.contains("Nothing on screen says that it was Banager that refused"),
            "the `## {section}` section of docs/what-we-run.md still says nothing on screen names the refusal"
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
fn test_what_we_run_promises_the_three_brew_flags_are_never_passed_but_for_u9s_force() {
    let doc = read_doc();
    // The promise, not the flags' spellings: the Cargo section lists
    // `cargo install --force`, so `doc.contains("--force")` would pass
    // with the Homebrew sentence gone. One never-list bullet has to name
    // all three flags and Homebrew together -- and, since the author's
    // decision U9 (r6), the one `--force` Banager passes: to the uninstall
    // of a formula with more than one version installed and no pin,
    // which `test_plan_never_passes_zap_force_or_ignore_dependencies` and
    // the brew adapter's `old_versions` tests keep true.
    // y1-keg (r6) adds the second: `brew link --formula --force` of a keg-only
    // formula the person linked, after its update -- and the same bullet
    // promises `--overwrite`, which would delete another program's file,
    // is never passed, and that no `brew link` runs bare.
    let promised = never_list_bullets(&doc).into_iter().any(|bullet| {
        bullet.starts_with("Never passes")
            && bullet.contains("Homebrew")
            && ["--zap", "--force", "--ignore-dependencies", "--overwrite"]
                .iter()
                .all(|flag| bullet.contains(flag))
            && [
                "uninstall",
                "more than one version",
                "no pin",
                "`brew link`",
                "keg-only",
                "bare `brew link`",
            ]
            .iter()
            .all(|words| bullet.contains(words))
    });
    assert!(
        promised,
        "docs/what-we-run.md has no never-list bullet promising Homebrew is never passed --zap, --ignore-dependencies or --overwrite, --force only to uninstall a formula with more than one version and no pin and to brew link a keg-only formula, and never a bare brew link"
    );
}

#[test]
fn test_what_we_run_shows_the_link_after_a_keg_only_formulas_update_and_what_it_reads() {
    // y1-keg (r6): the author asked for a keg-only formula they linked by
    // hand to stay in Terminal through its update ("能否自动修复"). The
    // command Banager runs for it is in the write-command table with its
    // time limit, and the section says what is read to decide, and when
    // it refuses instead.
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    let link = homebrew
        .lines()
        .find(|line| line.starts_with('|') && line.contains("`<brew> link --formula --force {name}`"))
        .unwrap_or_else(|| {
            panic!("Homebrew's write-command table has no row for `<brew> link --formula --force {{name}}`")
        });
    assert!(
        link.contains(&format!("{} s", BrewAdapter::LINK_TIMEOUT_SECS)),
        "the link's row does not state its time limit: {link}"
    );
    let folded = homebrew.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "keg-only",
        "<prefix>/var/homebrew/linked/<name>",
        "<prefix>/opt/<name>",
        "`keg_only_reason`",
        "`--overwrite`",
        "`UpdateBlocked::LinkTaken`",
        "`Fault::LinkTaken`",
        "`LogNote::NoLongerLinked`",
        "`upgrade.rb:635-643`",
        "`install.rb:632-641`",
        // y1-keg review: only a recorded link is unlinked, and only the
        // keg's own one; what blocks the update is named; what is not
        // guarded is said.
        "`upgrade.rb:268-272`",
        "`keg.rb:376-377`",
        "`Warning::LinkPlacesHeld`",
        "`formula_installer.rb:891-931`",
        "`unlink.rb:8-17`",
    ] {
        assert!(
            folded.contains(words),
            "the `## Homebrew` section does not say {words:?} of the link after a keg-only formula's update"
        );
    }
    // And the files it reads are listed with the rest.
    let reads =
        section_body(&doc, "Files Banager reads").expect("a `## Files Banager reads` section");
    let reads = reads.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in ["<prefix>/var/homebrew/linked/<name>", "<prefix>/opt/<name>"] {
        assert!(
            reads.contains(words),
            "`## Files Banager reads` does not name {words:?}"
        );
    }
}

#[test]
fn test_what_we_run_shows_one_link_for_an_update_and_for_a_notices_fix() {
    // y2-int (r6): y1-keg's link after a keg-only formula's update and
    // y2-npmwhy's link a source's notice offers are one command, read and
    // refused the same way (`link_argv`, `brew::links`): both rows of the
    // write-command table spell it the same, with the same time limit, and
    // the one section says how the link on its own is previewed, refused
    // and read after.
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    let rows: Vec<&str> = homebrew
        .lines()
        .filter(|line| line.starts_with('|') && line.contains("<brew> link "))
        .collect();
    assert_eq!(rows.len(), 2, "{rows:#?}");
    for row in &rows {
        assert!(
            row.contains("`<brew> link --formula --force {name}`")
                && row.contains(&format!("{} s", BrewAdapter::LINK_TIMEOUT_SECS)),
            "a link row spells the command or its time limit otherwise: {row}"
        );
    }
    assert!(!doc.contains("`<brew> link --force {name}`"));
    assert!(!doc.contains("link::link_preview"));
    let folded = homebrew.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "`OpKind::Link`",
        "`Warning::LinkPutsCommands`",
        "`Warning::LinkConflicts`",
        "`KegLinks::held_paths`",
        "`KegLinks::fully_linked`",
        "`SubmitError::LinkBlocked`",
        "`Attention::NotLinkedAfterLink`",
        "`CommandInputs::link_recorded`",
        "`<brew> link --formula --force --overwrite {name}`",
        "Banager never runs it",
    ] {
        assert!(
            folded.contains(words),
            "the `## Homebrew` section does not say {words:?} of the link a source's notice offers"
        );
    }
}

#[test]
fn test_what_we_run_says_an_npm_operation_takes_the_lock_of_the_homebrew_at_its_prefix() {
    // y1-keg review: `NpmAdapter::plan` takes `brew:{prefix}` besides
    // npm's own lock (`test_every_plan_takes_the_lock_of_a_homebrew_at_its_prefix`).
    let doc = read_doc();
    let npm = section_body(&doc, "npm").expect("a `## npm` section");
    let folded = npm.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in ["two locks", "`brew:{prefix}`"] {
        assert!(
            folded.contains(words),
            "the `## npm` section does not say {words:?} of the lock it shares with Homebrew"
        );
    }
}

#[test]
fn test_what_we_run_quotes_the_update_preview_saying_a_keg_only_formula_is_linked_back() {
    // y1-keg (r6): the line the update's preview shows first, quoted as
    // the app shows it -- `kegLinks.relinks` in zh-CN.json.
    let locale: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("../../src/i18n/zh-CN.json").expect("read zh-CN.json"),
    )
    .expect("zh-CN.json is JSON");
    let line = locale["kegLinks"]["relinks"]
        .as_str()
        .expect("kegLinks.relinks")
        .replace("{{name}}", "node@22");
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    let joined: String = homebrew.lines().map(str::trim).collect();
    assert!(
        joined.contains(&format!("「{line}」")),
        "the `## Homebrew` section does not quote the update preview's line {line:?}"
    );
}

#[test]
fn test_what_we_run_shows_the_cleanup_after_a_formulas_update_and_the_uninstall_of_every_version() {
    // U9 (r6): the two commands Banager runs on its own account besides
    // the plain verb, kind flag and name, each in the write-command table
    // with its time limit, and what keeps the cleanup from running.
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    let row = |argv: &str| {
        homebrew
            .lines()
            .find(|line| line.starts_with('|') && line.contains(argv))
    };
    let cleanup = row("`<brew> cleanup {name}`").unwrap_or_else(|| {
        panic!("Homebrew's write-command table has no row for `<brew> cleanup {{name}}`")
    });
    assert!(
        cleanup.contains(&format!("{} s", BrewAdapter::CLEANUP_TIMEOUT_SECS)),
        "the cleanup's row does not state its time limit: {cleanup}"
    );
    assert!(
        row("`<brew> uninstall --formula --force {name}`").is_some(),
        "Homebrew's write-command table has no row for the uninstall of every version"
    );
    let folded = homebrew.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "HOMEBREW_NO_CLEANUP_FORMULAE",
        "HOMEBREW_NO_INSTALL_CLEANUP",
        "<prefix>/Cellar/<name>",
        "<prefix>/var/homebrew/pinned/<name>",
    ] {
        assert!(
            folded.contains(words),
            "the `## Homebrew` section does not say {words:?} of the cleanup after an update"
        );
    }
}

#[test]
fn test_what_we_run_never_list_says_the_cleanup_after_an_update_also_deletes_cache_downloads() {
    // U9 (r6): `brew cleanup {name}` deletes the old installed versions of
    // that one formula, and also downloads in Homebrew's cache -- the
    // formula's outdated ones and every one nothing refers to any more,
    // whatever package it was for (`cleanup.rb:564-567`, `:709-733`). The
    // never-list is the promise people read: it must not say "nothing
    // more", and it must name the cache.
    let doc = read_doc();
    let bullet = never_list_bullets(&doc)
        .into_iter()
        .find(|bullet| bullet.starts_with("Never runs a `brew` command without"))
        .expect(
            "the never-list's bullet on HOMEBREW_NO_AUTOREMOVE and HOMEBREW_NO_INSTALL_CLEANUP",
        );
    let at = bullet
        .find("What Banager runs in its place")
        .unwrap_or_else(|| {
            panic!("the bullet does not say what Banager runs in its place: {bullet}")
        });
    let in_its_place = &bullet[at..];
    assert!(
        !in_its_place.contains("nothing more"),
        "the never-list says the cleanup after an update deletes nothing more than the old versions, but it also deletes downloads in Homebrew's cache: {in_its_place}"
    );
    // Review F1 (r6): every unreferenced download goes, whatever package
    // it was for, and the preview says so before it runs.
    for words in [
        "cache",
        "unreferenced",
        "whichever package it was for",
        "which the update's preview says",
        "no other installed software",
    ] {
        assert!(
            in_its_place.contains(words),
            "the never-list's sentence on the cleanup after an update does not say {words:?}: {in_its_place}"
        );
    }
}

#[test]
fn test_what_we_run_quotes_the_update_preview_saying_other_tools_downloads_go_too() {
    // Review F1 (r6): `brew cleanup <name>` deletes every download in
    // Homebrew's cache that nothing there refers to any more, whatever
    // package it was for (`cleanup_unreferenced_downloads`,
    // `cleanup.rb:709-733`), and Homebrew has no way to keep it to the one
    // formula without another command. So the update's preview says so,
    // and the `## Homebrew` section quotes the line as the app shows it --
    // the same words as `brewVersions.cleansUp_other` in zh-CN.json.
    let locale: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("../../src/i18n/zh-CN.json").expect("read zh-CN.json"),
    )
    .expect("zh-CN.json is JSON");
    let line = locale["brewVersions"]["cleansUp_other"]
        .as_str()
        .expect("brewVersions.cleansUp_other")
        .replace("{{versions}}", "1.25.0");
    assert!(
        line.contains("其他工具"),
        "the preview's line does not say other tools' downloads go too: {line}"
    );
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    // Hard-wrapped Chinese has no spaces to fold a line break into.
    let joined: String = homebrew.lines().map(str::trim).collect();
    assert!(
        joined.contains(&format!("「{line}」")),
        "the `## Homebrew` section does not quote the update preview's line {line:?}"
    );
}

#[test]
fn test_what_we_run_says_brew_cleanup_skips_a_pinned_version_but_not_its_other_old_ones() {
    // Review of v1-brew's fixes (r6): `brew cleanup <name>` ignores
    // `HOMEBREW_NO_INSTALL_CLEANUP` (`cleanup.rb:497-519`), but it does
    // skip the pinned version itself (`Formula#eligible_kegs_for_cleanup`,
    // `formula.rb:3787`); what it still deletes are the formula's other old
    // versions. So the `## Homebrew` section must not say it checks no pin,
    // and says what it does, with Homebrew's lines.
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    let folded = homebrew.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !folded.contains("checks neither `HOMEBREW_NO_INSTALL_CLEANUP` nor a pin"),
        "the `## Homebrew` section says `brew cleanup` with a name checks no pin, but Homebrew skips the pinned version itself"
    );
    for words in [
        "skips only the pinned version itself",
        "other old versions",
        "`formula.rb:3760-3793`",
    ] {
        assert!(
            folded.contains(words),
            "the `## Homebrew` section does not say {words:?} of what `brew cleanup` with a name does for a pinned formula"
        );
    }
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
fn test_what_we_run_states_detection_and_rustup_operations_share_locks() {
    // The refresh tests cover both orderings, including first detection
    // and detection cancellation. The trust document must describe the
    // actual exclusion instead of the old snapshot-only race window.
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
            bullet.contains("Detection holds") && bullet.contains("waits"),
            "the never-list must state detection owns locks and a later operation waits: {bullet:?}"
        );
    }
    let rustup = section_body(&doc, "rustup")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## rustup` section"));
    assert!(
        rustup.contains("atomically acquires") && rustup.contains("first snapshot"),
        "the rustup section must document atomic exclusion including first detection"
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

#[test]
fn test_what_we_run_says_the_unknown_scan_never_reads_into_a_protected_place() {
    let doc = read_doc();
    let body = section_body(&doc, "Unknown-source scan").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Unknown-source scan` section for scan::scan_unknown")
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    // Every place it never reads, as the shared list has them -- the same
    // list as the command check's and the disk-use measurement's.
    for place in PROTECTED_IN_HOME {
        assert!(
            folded.contains(&format!("`~/{place}`")),
            "the `## Unknown-source scan` section of docs/what-we-run.md does not name `~/{place}`, which protected::PROTECTED_IN_HOME keeps it out of"
        );
    }
    assert!(
        folded.contains(&format!("`{OTHER_VOLUMES}`")),
        "the `## Unknown-source scan` section of docs/what-we-run.md does not name `{OTHER_VOLUMES}`"
    );
    for words in [
        "as named or where it leads (`protected::resolve`)",
        "`/System/Volumes/Data`",
        "`UnknownScan.protected_dirs`",
        "`EntryKind::ProtectedSymlink`",
        "by name alone",
    ] {
        assert!(
            folded.contains(words),
            "the `## Unknown-source scan` section of docs/what-we-run.md does not say {words:?}"
        );
    }
    // The exception t27 disclosed is closed: no sentence may say the scan
    // reads by path into those places.
    for words in ["known exception", "no list of places it never looks into"] {
        assert!(
            !folded.contains(words),
            "the `## Unknown-source scan` section of docs/what-we-run.md still says {words:?}"
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
    // And every folder inside them that a size leaves out.
    for (of, inside) in kept_data::LEFT_OUT {
        let left_out = format!("`{of}/{inside}`");
        assert!(
            folded.contains(&left_out),
            "the `## Data an uninstall leaves behind` section of docs/what-we-run.md does not name {left_out}, which kept_data::LEFT_OUT leaves out of a size"
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
fn test_what_we_run_says_what_the_uninstall_preview_follows_for_what_runs_on_a_homebrew_package() {
    // `needed_by::needed_by`, run by `Session::issue_plan` for a Homebrew
    // formula's or cask's uninstall: not a registered source, so the
    // per-source test never asks for it.
    let doc = read_doc();
    let body = section_body(&doc, "What runs on a Homebrew package").unwrap_or_else(|| {
        panic!(
            "docs/what-we-run.md has no `## What runs on a Homebrew package` section for needed_by"
        )
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    // Every kind of source it looks at, by the name its section has.
    for (adapter, name) in [
        ("npm", "npm"),
        ("pip", "pip"),
        ("pipx", "pipx"),
        ("uv", "uv"),
        ("cargo", "Cargo"),
        ("ollama", "Ollama"),
    ] {
        assert!(
            banager_core::needed_by::HOSTED.contains(&adapter),
            "{adapter} is no longer one the look covers; say so in the section"
        );
        assert!(
            folded.contains(name),
            "the `## What runs on a Homebrew package` section does not name {name}"
        );
    }
    assert_eq!(
        banager_core::needed_by::HOSTED.len(),
        6,
        "a kind of source added to the look needs its line"
    );
    let budget = banager_core::needed_by::BUDGET;
    for limit in [
        format!("{} paths", with_commas(budget.max_looks as u64)),
        format!("{} second", budget.max_duration.as_secs()),
    ] {
        assert!(
            folded.contains(&limit),
            "the `## What runs on a Homebrew package` section does not state the limit {limit:?}, which needed_by::BUDGET enforces"
        );
    }
    for words in [
        "during the uninstall preview of a formula or cask only",
        "`<prefix>/Cellar/<name>`",
        "`<prefix>/Caskroom/<token>`",
        "`bin/python`",
        "the `node` first on the `PATH` of the last refresh",
        "`lstat` and `readlink`",
        "Only folders are opened, to follow each link and to read the names in the few listed above: no file's contents are read, nothing is written, and no command runs.",
        "An Ollama whose `OLLAMA_HOST` names another machine is not looked at",
        "never into the places macOS asks about first nor onto another disk",
        "`UninstallBlocked::NeededBySource`",
        "never that nothing runs on it",
    ] {
        assert!(
            folded.contains(words),
            "the `## What runs on a Homebrew package` section of docs/what-we-run.md does not say {words:?}"
        );
    }
    // And the list of files Banager reads names it, as Homebrew's own
    // section points to it.
    let reads =
        section_body(&doc, "Files Banager reads").expect("a `## Files Banager reads` section");
    let reads = reads.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        reads.contains(
            "What runs on a Homebrew package, during the uninstall preview of a formula or cask"
        ),
        "`## Files Banager reads` does not list what the uninstall preview follows"
    );
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    let homebrew = homebrew.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        homebrew.contains("(What runs on a Homebrew package, below)"),
        "`## Homebrew` does not point at what its uninstall preview looks at besides `brew uses`"
    );
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
        format!("{} entries", with_commas(budget.max_entries as u64)),
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
        "is not read at all, as named or where it leads (`protected::resolve`)",
        "is followed only as far as the place, never into it",
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
fn test_what_we_run_shows_the_environment_of_pips_outdated_check_and_no_verbosity_flag() {
    // `PipAdapter::check_updates` runs `pip list --outdated` with
    // `PipAdapter::OUTDATED_ENV` and no `-v` (opus-int findings 3 and 4:
    // `-vv` counted an extra index's 404s as failed lookups and printed a
    // line for every file pip skipped). pip's section has to show each
    // variable in the outdated row of its table, and that row must not
    // show a verbosity flag the check no longer passes.
    let doc = read_doc();
    let body =
        section_body(&doc, "pip").expect("docs/what-we-run.md has no `## pip` section for pip");
    let row = body
        .lines()
        .find(|line| line.starts_with('|') && line.contains("pip list --outdated --format=json"))
        .expect("pip's table in docs/what-we-run.md has no `pip list --outdated` row");
    for (name, value) in PipAdapter::OUTDATED_ENV {
        assert!(
            row.contains(&format!("`{name}={value}`")),
            "pip's outdated row in docs/what-we-run.md does not show {name}={value}, which PipAdapter::OUTDATED_ENV holds: {row}"
        );
    }
    assert!(
        !row.contains("-v"),
        "pip's outdated row in docs/what-we-run.md shows a verbosity flag check_updates does not pass: {row}"
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
    // D, Codex since the author's decision U8), by the name its meta gives
    // its section; each section states the uninstall's budget; and every
    // settings-and-state path a list keeps
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
        paths_recipes, 4,
        "Claude Code, Antigravity CLI, Grok Build and Codex uninstall by path list"
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
    // says whether the window may call, and how far.
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
    // Whether the plugin is in the app at all (`src-tauri/Cargo.toml`), as
    // the Network section says: it may not call a plugin registered that
    // is not, or leave out one that is.
    let path = Path::new("../../src-tauri/Cargo.toml");
    let cargo =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let built_in = cargo
        .lines()
        .any(|line| line.trim_start().starts_with("tauri-plugin-opener"));
    let network = section_body(&doc, "Network").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Network` section for the hosts Banager connects to")
    });
    let network = network.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        network.contains("The Tauri opener plugin — the one that opens a URL or a path in another application — is not built into Banager"),
        !built_in,
        "the `## Network` section of docs/what-we-run.md does not say whether the opener plugin is built in, as src-tauri/Cargo.toml has it"
    );
    assert_eq!(
        network.contains("is registered (`run()` in `src-tauri/src/lib.rs`)"),
        built_in,
        "the `## Network` section of docs/what-we-run.md says the opener plugin is registered, which src-tauri/Cargo.toml does not build in"
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

#[test]
fn test_what_we_run_names_every_permission_the_window_is_given() {
    // The window's permissions are what a page that ran someone else's
    // script could ask of Tauri. The `## Network` section names each one,
    // so a permission added to `src-tauri/capabilities/default.json`
    // without a word there fails here.
    let path = Path::new("../../src-tauri/capabilities/default.json");
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let capability: serde_json::Value = serde_json::from_str(&text).expect("default.json is JSON");
    let permissions: Vec<&str> = capability["permissions"]
        .as_array()
        .expect("default.json lists its permissions")
        .iter()
        .map(|p| {
            p.as_str()
                .or_else(|| p["identifier"].as_str())
                .expect("a permission is a string or has an identifier")
        })
        .collect();
    assert!(
        !permissions.is_empty(),
        "default.json gives the window no permission"
    );
    let doc = read_doc();
    let network = section_body(&doc, "Network").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Network` section for the hosts Banager connects to")
    });
    let network = network.split_whitespace().collect::<Vec<_>>().join(" ");
    for permission in &permissions {
        assert!(
            network.contains(&format!("`{permission}`")),
            "the `## Network` section of docs/what-we-run.md does not name {permission:?}, which src-tauri/capabilities/default.json gives the window"
        );
    }
}

#[test]
fn test_what_we_run_says_what_the_homepage_link_opens_and_that_it_adds_no_permission() {
    // The command the details panel's homepage link calls
    // (`src-tauri/src/homepage.rs`), registered in `run()`.
    let path = Path::new("../../src-tauri/src/lib.rs");
    let lib =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let registered = lib.contains("homepage::open_homepage,");
    let doc = read_doc();
    let network = section_body(&doc, "Network").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Network` section for the hosts Banager connects to")
    });
    let network = network.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        network.contains("`open_homepage`"),
        registered,
        "the `## Network` section of docs/what-we-run.md does not say whether the window can have a homepage opened, as src-tauri/src/lib.rs registers it"
    );
    if !registered {
        return;
    }
    for said in [
        "NSWorkspace URLForApplicationToOpenURL:",
        "scheme-only `https:`",
        "openURLs:withApplicationAtURL:configuration:completionHandler:",
        "allowsRunningApplicationSubstitution = false",
        "no fallback to",
        "only when it is, exactly, the homepage of a tool in the current snapshot",
        "an `https` address with a host",
        "a plain `http` homepage is shown to copy",
        "No command runs and Banager connects to nothing",
        "The window is given no new permission for it",
    ] {
        assert!(
            network.contains(said),
            "the `## Network` section of docs/what-we-run.md does not say {said:?} of the homepage link"
        );
    }
    // What it says of the permissions holds: no app manifest that would
    // put Banager's own commands behind permissions, and no permission of
    // the opener's or the shell's.
    let path = Path::new("../../src-tauri/build.rs");
    let build =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(
        !build.contains("app_manifest"),
        "src-tauri/build.rs declares an app manifest"
    );
    let path = Path::new("../../src-tauri/capabilities/default.json");
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(
        !text.contains("\"opener:") && !text.contains("\"shell:"),
        "src-tauri/capabilities/default.json gives the window an opener or shell permission"
    );
    let never = section_body(&doc, "What Banager never does")
        .expect("a `## What Banager never does` section");
    let never = never.split_whitespace().collect::<Vec<_>>().join(" ");
    // Bounded, not denied: it does open one address the window names.
    assert!(
        never.contains("Never opens just any web address the window names"),
        "the `## What Banager never does` section of docs/what-we-run.md does not bound the homepage link"
    );
    assert!(
        !never.contains("Never opens a web address the window names"),
        "the `## What Banager never does` section of docs/what-we-run.md says Banager never opens a web address the window names, which the homepage link does"
    );
    assert!(
        never.contains("the default browser is started only by a click on a tool's homepage"),
        "the `## What Banager never does` section of docs/what-we-run.md does not say when the homepage link may start the browser"
    );
    // The Open Ollama button's paragraph, which once called `open -a
    // Ollama` the app's one launch outside the package managers: the
    // homepage link can start the browser too, as Network says.
    let ollama = doc
        .split("\n\n")
        .find(|paragraph| paragraph.starts_with("**The Open Ollama button**"))
        .expect("docs/what-we-run.md has a paragraph on the Open Ollama button");
    let ollama = ollama.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !ollama.contains("the one launch in the app"),
        "docs/what-we-run.md calls the Open Ollama button the one launch in the app, though the homepage link can start the default browser"
    );
    assert!(
        ollama.contains("the homepage link") && ollama.contains("(Network)"),
        "docs/what-we-run.md's Open Ollama button paragraph does not name the homepage link as the other launch"
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
fn test_what_we_run_says_the_weekly_check_waits_a_week_and_an_old_daily_switch_reads_as_daily() {
    // The popup's three choices, the week `CheckEvery::Week` waits, and
    // that a settings.json from the switch's days reads as Daily -- which
    // `auto_check_every`'s `#[serde(default)]`, `Day`, makes true.
    use banager_core::auto_check::{CheckEvery, WEEKLY_DUE_AFTER_SECS};
    use banager_core::settings::Settings;
    assert_eq!(CheckEvery::Week.due_after_secs(), WEEKLY_DUE_AFTER_SECS);
    assert_eq!(WEEKLY_DUE_AFTER_SECS % (24 * 60 * 60), 0);
    assert_eq!(Settings::default().auto_check_schedule(), None);
    let old: Settings = serde_json::from_str(
        r#"{"language":"En","show_technical_details":false,"ignored_updates":[],"include_self_updating":false,"auto_check":true}"#,
    )
    .expect("a settings.json from the switch's days");
    assert_eq!(old.auto_check_schedule(), Some(CheckEvery::Day));
    let doc = read_doc();
    let body = section_body(&doc, "The daily check")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## The daily check` section"));
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "\"Manually\" (「不自动检查」), \"Daily\" (「每天」) or \"Weekly\" (「每周」), set to Manually, and so off by default".to_string(),
        format!(
            "everything below holds with {} days",
            WEEKLY_DUE_AFTER_SECS / (24 * 60 * 60)
        ),
        "reads as Daily".to_string(),
    ] {
        assert!(
            folded.contains(&phrase),
            "the `## The daily check` section of docs/what-we-run.md does not say {phrase:?}"
        );
    }
}

#[test]
fn test_what_we_run_says_the_notification_when_operations_finish_is_off_by_default_and_asks_no_permission_of_its_own(
) {
    use banager_core::settings::Settings;
    assert!(
        !Settings::default().notify_operations,
        "the notification when operations finish is off by default today; if that has changed, the section must say so"
    );
    let doc = read_doc();
    let body = section_body(&doc, "The notification when operations finish").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## The notification when operations finish` section")
    });
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "\"Notify me when operations finish\" (「操作完成时通知」), off by default (`Settings::notify_operations`)",
        "no permission of its own",
        "never for a run that finished while the window had the focus",
        "no command runs, nothing connects, and Banager writes no file for it",
        "in memory only",
        "a target, not a bound, since the record of an operation not yet finished is never dropped",
        "accepted telling of nothing, so it never holds back the runs after it",
    ] {
        assert!(
            folded.contains(phrase),
            "the `## The notification when operations finish` section of docs/what-we-run.md does not say {phrase:?}"
        );
    }
    // The two bounds, as the constants the manager enforces.
    use banager_core::ops::{DEFAULT_MAX_RECORDS, MAX_EVICTED};
    for limit in [
        format!(
            "drops its oldest finished operation records until it holds {} (`DEFAULT_MAX_RECORDS`",
            with_commas(DEFAULT_MAX_RECORDS as u64)
        ),
        format!("at most {} of those", with_commas(MAX_EVICTED as u64)),
    ] {
        assert!(
            folded.contains(&limit),
            "the `## The notification when operations finish` section does not state the limit {limit:?}"
        );
    }
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
        "the Help menu's Copy Diagnostic Info… (「拷贝诊断信息…」) only opens Settings on that button, focused, so the copy is always the button's click",
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
        "Never a line of a log, a command line or any other path, and of an error message one line at most".to_string(),
        format!(
            "at most {} characters, its label (`Error:`) taken off (with the line after it where it ends with a colon), with any home folder but `/Users/Shared` written as `~` and any login, query, fragment or token-like part of the path in an address masked",
            banager_core::history::DETAIL_CHARS
        ),
        "An operation cancelled before Banager began carrying it out (while it waited for its turn, or while Banager read the installed version) is not recorded.".to_string(),
        "or cancelled once Banager had handed it to the tool's adapter, which can be before the tool's own command started".to_string(),
        format!(
            "The file keeps the newest {} records and nothing older than {} days: as Banager starts it drops the rest and, if it dropped any, writes the file again straight away;",
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

#[test]
fn test_what_we_run_never_says_a_folder_the_uninstall_preview_walks_is_never_read() {
    // `kept_data` walks `~/.codex` (names and sizes) in the preview of
    // uninstalling a Codex. The Codex section and the list of files read
    // may say nothing else there is read for the row, but not that the
    // folder is never read, and both point at the section that walks it.
    let doc = read_doc();
    for section in ["Codex", "Files Banager reads"] {
        let body = section_body(&doc, section)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {section}` section"));
        let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            !folded.contains("`~/.codex` itself, with your settings, login and sessions, is never read"),
            "the `## {section}` section says `~/.codex` is never read, but the uninstall preview walks it"
        );
        assert!(
            folded.contains("walks `~/.codex`"),
            "the `## {section}` section does not say the uninstall preview walks `~/.codex`"
        );
    }
}

/// U12 of the decisions round: every setting taken from the login shell
/// besides `PATH` (`login_path::IMPORTED`) is named where the document
/// says how Banager runs anything, so the list and the document change
/// together; so are the ones deliberately left out, which decide where
/// things are installed or add an index beside PyPI (the U12 review).
/// And no section still says that `PATH` is the
/// only thing taken from the shell.
#[test]
fn test_what_we_run_names_every_setting_taken_from_the_login_shell() {
    use banager_core::runner::login_path::IMPORTED;
    let doc = read_doc();
    let body = section_body(&doc, "How Banager runs anything")
        .expect("docs/what-we-run.md has a `## How Banager runs anything` section");
    for name in IMPORTED {
        assert!(
            body.contains(&format!("`{name}`")),
            "the `## How Banager runs anything` section of docs/what-we-run.md does not name `{name}`, which login_path::IMPORTED takes from the login shell"
        );
    }
    for kept_out in [
        "CARGO_HOME",
        "RUSTUP_HOME",
        "UV_TOOL_DIR",
        "PIP_EXTRA_INDEX_URL",
        "UV_EXTRA_INDEX_URL",
    ] {
        assert!(
            body.contains(&format!("`{kept_out}`")),
            "the `## How Banager runs anything` section of docs/what-we-run.md does not say `{kept_out}` is left out"
        );
    }
    let folded = doc.split_whitespace().collect::<Vec<_>>().join(" ");
    for stale in [
        "No other shell variable is imported",
        "the only variable taken from the login shell",
        "restores only `PATH` from your login shell",
        "inherits no variable from your shell except the `PATH` Banager asks",
        "inherits nothing from your shell but the `PATH` Banager asks",
    ] {
        assert!(
            !folded.contains(stale),
            "docs/what-we-run.md still says {stale:?}, but the proxy and mirror settings are taken too"
        );
    }
}

/// U12: the Network section, which names every host Banager connects to,
/// says that its own requests go through the proxy the login shell names,
/// failing that the one this Mac's network settings name (the U12
/// review: what a proxy app in its "system proxy" mode sets), that this
/// Mac -- the Ollama daemon -- never does, and that `no_proxy` is
/// followed (`http::proxy::proxy_for`).
#[test]
fn test_what_we_run_says_own_requests_go_through_the_login_shells_proxy_but_not_for_this_mac() {
    let doc = read_doc();
    let body = section_body(&doc, "Network").expect("a `## Network` section");
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "`https_proxy`",
        "`http_proxy`",
        "`all_proxy`",
        "`no_proxy`",
        "`localhost`",
        "`127.0.0.1`",
        "`::1`",
        "`0.0.0.0`",
        "never through a proxy",
        "`socks5://`",
        "System Settings → Network",
        "\"system proxy\" mode",
        "when opening a new connection",
        "Existing pooled connections can keep their previous route",
        "not guaranteed to apply to the next check",
        // Where `proxy_for` parts from curl (the U12 review).
        "rules like curl's",
        "`HTTP_PROXY`",
        "`*.`",
    ] {
        assert!(
            folded.contains(phrase),
            "the `## Network` section of docs/what-we-run.md does not say {phrase:?} about the proxy Banager's own requests go through"
        );
    }
    assert!(
        !folded.contains("by curl's rules"),
        "the `## Network` section says Banager's own requests pick a proxy by curl's rules, but `proxy_for` also reads `HTTP_PROXY` and a `*.` in `no_proxy`, which curl does not"
    );
    for stale in [
        "looked up at each request",
        "at each request, so",
        "counts from the next check",
    ] {
        assert!(
            !folded.contains(stale),
            "stale proxy routing promise: {stale}"
        );
    }
}

/// F2 of the decisions-round review, and R1-R3 of its re-check: the
/// section that lists the proxy and mirror settings taken from the login
/// shell says that a login in them is masked in what tools print
/// (`runner::redact`), by which rules a value is read, in which forms, with
/// which mark, which words are masked only in their login, and what is not
/// masked -- and the Passwords paragraph no longer promises Banager never
/// shows one without saying how.
#[test]
fn test_what_we_run_says_a_login_in_a_setting_is_masked_in_what_tools_print() {
    use banager_core::runner::redact::{COMMON_WORDS, MASK, SHORTEST_MASKED_ANYWHERE};
    let doc = read_doc();
    let body = section_body(&doc, "How Banager runs anything")
        .expect("docs/what-we-run.md has a `## How Banager runs anything` section");
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        SHORTEST_MASKED_ANYWHERE, 3,
        "the document says a user name or password of fewer than three characters is masked only in its login"
    );
    // Every word masked only in its login is listed, word for word.
    for word in COMMON_WORDS {
        assert!(
            folded.contains(&format!("`{word}`")),
            "the `## How Banager runs anything` section of docs/what-we-run.md does not list `{word}`, one of `COMMON_WORDS` in runner/redact.rs"
        );
    }
    for phrase in [
        "**What a tool prints about a login.**",
        "`runner::redact`",
        &format!("`{MASK}`"),
        "Unsupported proxy syntax in",
        "Failed to parse:",
        "percent-decoded",
        "percent-encoded",
        "HTTP Basic",
        "fewer than three characters",
        "any `scheme://user:password@`",
        "`scheme://user@` in the output is masked",
        "including Git configuration",
        "This generic rule ends at the authority",
        "split across two reads",
        "the word on each side of the cut",
        "What a parser reads",
        "handed to the commands unchanged",
        // The F2 review's fixes: a password with a `/`, `?` or `#`, a
        // token in the name slot, and a quoted reason masked.
        "all before the last `@` of its value",
        "`/`, `?`, `#` or `@` written into its password",
        "x-oauth-basic",
        "the quote is masked first",
        // The re-check's (R1-R3): fixed rules, both parts whatever they
        // look like, git's and npm's lines, an `@` in a path.
        "could not read Password for",
        "Invalid protocol",
        "never by what its parts look like",
        "a scheme counts only where the value starts with one",
        "all before the last `@` of its authority",
        "an `@` in its path is the path's",
        "is no host and port",
        "the user name and the password are both secrets, whatever they look like",
        "the whole login and the whole value",
        "more than needed rather than less",
        "`/Users/****/…`",
        "`COMMON_WORDS` in `runner/redact.rs`",
        // Re-check 2: any case (N3), one-label and absolute hosts (N2),
        // and the cause read before the mask (N1).
        "found ignoring the case of its letters",
        "prints it lowercased",
        "`https://nexus:8081/repository/npm/@scope/pkg`",
        "`https://mirror.example.:8443/…`",
        "is not masked by this rule",
        "Why an operation failed is not read off what the mask left",
        "before the mask (`CommandOutput::failure_cause`",
        "(`Outcome::Failed`'s `cause`)",
        "a ****word is required",
        "What a check or a source failed with is still read off its masked words",
    ] {
        assert!(
            folded.contains(phrase),
            "the `## How Banager runs anything` section of docs/what-we-run.md does not say {phrase:?} about masking a login in what tools print"
        );
    }
    // The rules the re-check found leaking: a name kept for not looking
    // like a token, and a proxy's name never masked.
    for gone in [
        "A proxy's user name is not",
        "its user name is left",
        "not letters alone or digits alone",
        "16 characters or more",
        "only when it looks like a token",
        "a proxy's user name included",
        // Re-check 2's N1: the cause no longer depends on sudo's words
        // being left unmasked.
        "which Banager reads to say an operation needs Terminal (`needsPassword` in `src/lib/failureCause.ts`): `password`",
    ] {
        assert!(
            !folded.contains(gone),
            "the section still says {gone:?}, which the decisions-round re-checks found wrong (R1, N1)"
        );
    }
    assert!(
        !folded.contains("the user name included, as the tool wrote it"),
        "the section still says every user name is left as written, but a mirror's may be a token and is masked"
    );
    assert!(
        !folded.contains("it is never shown, and masking it"),
        "the section still says what a parser reads is never shown, but a standalone update check's reason quotes it"
    );
    assert!(
        !folded.contains("and never shows or records it"),
        "the Passwords paragraph still says Banager never shows or records a proxy's login without saying it is masked in what tools print"
    );
}

#[test]
fn test_adapter_directory_budgets_and_incomplete_outcomes_are_documented() {
    // `look::ListingBudget` (4096 names, 2 seconds) and what each caller
    // makes of a check that ran out: never the names it did read.
    let doc = read_doc();
    let reads = section_body(&doc, "Files Banager reads").expect("the Files Banager reads section");
    let folded = reads.split_whitespace().collect::<Vec<_>>().join(" ");
    for expected in [
        "**4096 names and 2 seconds**",
        "**one extra name**",
        "never uses the names it did read",
        "one budget for a keg's `bin` and `sbin` together",
        "a caskfile chosen among part of the versions",
        "refused (`NoSafeMethod`) at that folder",
        "only a missing folder means no backups",
        "`~/.local/bin/agy.*.old`",
        "leaves the result after the last move unconfirmed",
        "add no command, network request or written file",
    ] {
        assert!(
            folded.contains(expected),
            "the Files Banager reads section no longer says {expected:?}"
        );
    }
}

/// R5 of the f19 review: the promise that a proxy's login goes to that
/// proxy alone holds when the settings change while Banager is open,
/// because `RealHttpClient` leaves the connections made under other proxy
/// settings (`tests/http_proxy_pool_test.rs`); the Network section says
/// so, and says what is not compared.
#[test]
fn test_what_we_run_says_a_proxy_settings_change_gets_new_connections() {
    let doc = read_doc();
    let body = section_body(&doc, "Network").expect("a `## Network` section");
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "gives that login to that proxy alone, also when the settings change while Banager is open",
        "at each of its own requests Banager reads the login shell's and its own environment's proxy settings again",
        "it leaves those connections and opens new ones (`http::real::RealHttpClient`)",
        "This Mac's network settings hold no login",
        "are not part of that comparison",
    ] {
        assert!(
            folded.contains(phrase),
            "the `## Network` section of docs/what-we-run.md does not say {phrase:?} about a change to the proxy settings"
        );
    }
}

/// r26 D1: a pre-run read of npm or uv that does not answer ends as that
/// program's own failure (`adapters::read_before_run`, r20 R20-2); only
/// an answer that differs, or that cannot be used, is "changed since
/// shown" with its reopen. The English sections say so; the Chinese
/// summaries, a Chinese reader's only account of it, say both cases
/// apart and no longer say an unreadable read ends as 「未能开始」.
#[test]
fn test_what_we_run_says_an_npm_or_uv_read_before_running_that_does_not_answer_ends_as_its_own_failure(
) {
    let doc = read_doc();
    for (source, phrase) in [
        ("npm", "the operation ends as npm's own failure would"),
        ("uv", "ends as uv's own failure would"),
    ] {
        let body = section_body(&doc, source).unwrap_or_else(|| panic!("a `## {source}` section"));
        let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            folded.contains(phrase),
            "the `## {source}` section of docs/what-we-run.md does not say {phrase:?}"
        );
    }
    for (section, old, phrases) in [
        (
            "简体中文：运行与隐私要点",
            "与预览时不一致或无法读取时",
            [
                "读到的与预览时不一致，或读到了却用不上",
                "结果显示为未能开始，请重新打开确认窗口。",
                "npm 或 uv 的读取命令本身没有回答时，同样不运行写入命令，但按该程序自己的失败结束",
                "程序已不在或无法启动，显示为未能开始、没有改动",
                "非零退出，是该程序的失败，带有退出码和它最后写到 stderr 的几行",
                "超过时限（npm 30 秒、uv 60 秒）没有回答，是运行超时，没有退出码",
            ],
        ),
        (
            "繁體中文：執行與隱私要點",
            "與預覽時不一致或無法讀取時",
            [
                "讀到的與預覽時不一致，或讀到了卻無法使用",
                "結果顯示為未能開始，請重新開啟確認視窗。",
                "npm 或 uv 的讀取命令本身沒有回答時，同樣不執行寫入命令，但按該程式自己的失敗結束",
                "程式已不在或無法啟動，顯示為未能開始、沒有改動",
                "非零結束，是該程式的失敗，帶有結束代碼和它最後寫到 stderr 的幾行",
                "超過時限（npm 30 秒、uv 60 秒）沒有回答，是執行逾時，沒有結束代碼",
            ],
        ),
    ] {
        let body =
            section_body(&doc, section).unwrap_or_else(|| panic!("a `## {section}` section"));
        let bullet = body
            .lines()
            .find(|line| line.starts_with("- npm 的操作"))
            .unwrap_or_else(|| {
                panic!(
                    "`## {section}` has no bullet on npm's, Cargo's and uv's read before running"
                )
            });
        assert!(
            !bullet.contains(old),
            "`## {section}` still says a read that does not answer ends as 「未能开始」: {bullet}"
        );
        // A read stopped at its deadline ends `Failed`, `TimedOut`; a write
        // command stopped at its 600 s limit, or by a signal it did not
        // send, ends `Unconfirmed` (`run_plan`), 「结果未确认」. So the read
        // does not end "as its write command would, failing the same way".
        for unlike in [
            "和它的写入命令这样失败时一样",
            "和它的寫入命令這樣失敗時相同",
        ] {
            assert!(
                !bullet.contains(unlike),
                "`## {section}` says a read that does not answer ends as its write command \
                 would ({unlike:?}), which is untrue of one that runs out of time: {bullet}"
            );
        }
        for phrase in phrases {
            assert!(
                bullet.contains(phrase),
                "`## {section}`'s bullet on the read before running does not say {phrase:?}"
            );
        }
    }
}

/// r26 D3: a cask's uninstall preview, and its check just before the
/// uninstall runs, look at where each link the cask's record names leads
/// (`cask_link_conflict` in `adapters/brew/mod.rs`, `cask_links::conflict`).
/// Both lists of what Homebrew's adapter reads name those reads, and
/// point at the paragraph that says why.
#[test]
fn test_what_we_run_lists_the_cask_link_reads_of_an_uninstall_with_the_rest_of_homebrews() {
    let doc = read_doc();
    let homebrew = section_body(&doc, "Homebrew").expect("a `## Homebrew` section");
    assert!(
        homebrew.contains("\n**A cask's links.** Before offering a cask uninstall"),
        "the `## Homebrew` section has no `A cask's links` paragraph for the read lists to point at"
    );
    let own = homebrew
        .split("\n\n")
        .find(|paragraph| paragraph.starts_with("**Files this adapter reads.**"))
        .expect("Homebrew's `Files this adapter reads` paragraph");
    let own = own.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "A cask's uninstall preview, and its check just before the uninstall runs, also look at where the links its record names lead",
        "(`cask_links::conflict` in `crates/banager-core/src/adapters/brew/cask_links.rs`)",
        "`binary`, `command_wrapper`, `manpage` and completion link",
        "`<prefix>/bin`, `<prefix>/share/man/man<section>`, Homebrew's four completion folders",
        "`<prefix>/Caskroom/<token>` folder, each app it recorded",
        "`/Applications` and in `~/Applications`",
        "`<prefix>/Cellar` lead",
        "`lstat` and `readlink`",
        "never a file's contents",
    ] {
        assert!(
            own.contains(words),
            "Homebrew's `Files this adapter reads` does not say {words:?} of a cask's links"
        );
    }
    let reads =
        section_body(&doc, "Files Banager reads").expect("a `## Files Banager reads` section");
    let homebrew_reads = reads
        .split("\n- ")
        .find(|bullet| bullet.starts_with("Homebrew:"))
        .expect("the Homebrew bullet of `## Files Banager reads`");
    let homebrew_reads = homebrew_reads
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for words in [
        "`INSTALL_RECEIPT.json`, and where each link that record names leads",
        "`binary`, `command_wrapper`, `manpage` and completion links in `<prefix>/bin`, `<prefix>/share/man/man<section>`",
        "where its Caskroom folder, its recorded apps",
        "`<prefix>/Cellar`",
        "Homebrew's section, \"A cask's links\"",
    ] {
        assert!(
            homebrew_reads.contains(words),
            "the Homebrew bullet of `## Files Banager reads` does not say {words:?} of a cask's links"
        );
    }
}

/// r26 D5: `Session::issue_listed_plan` plans an upgrade the last check
/// no longer offers when the window previewed it less than ten minutes
/// before and the tool is still listed installed (`ListedUpgrade`), so
/// the never-list's promise that the window cannot install by another
/// name says that exception, as When commands run does, and no longer
/// that only an update the last check listed can be upgraded.
#[test]
fn test_what_we_run_never_list_says_a_previewed_update_no_longer_offered_may_still_be_planned() {
    let doc = read_doc();
    let bullets = never_list_bullets(&doc);
    let bullet = bullets
        .iter()
        .find(|bullet| bullet.starts_with("Never lets the window ask for an install"))
        .expect("the never-list's bullet that the window never asks for an install");
    for words in [
        "Nor by another name: the window may ask for an upgrade only of an update the last check listed, or of one the window previewed less than ten minutes before whose tool the last check still lists installed under the same source, kind and name, aimed at the version offered then (When commands run); for an uninstall only of a tool the last check listed installed; and for a link only of a formula a source's reason offers (`Session::issue_listed_plan`)",
    ] {
        assert!(
            bullet.contains(words),
            "the never-list's bullet on asking for an install does not say {words:?}: {bullet}"
        );
    }
    // Each "it" named a different subject -- the window that previewed,
    // the check that listed -- three clauses apart: each is named.
    for unclear in ["of one it previewed", "of a tool it listed installed"] {
        assert!(
            !bullet.contains(unclear),
            "the never-list's bullet on asking for an install still says {unclear:?}, whose \
             \"it\" reads as the window: {bullet}"
        );
    }
    let when = section_body(&doc, "When commands run").expect("a `## When commands run` section");
    let when = when.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        when.contains("is still planned again while it is installed under the same source, kind and name and its update was previewed less than ten minutes before"),
        "`## When commands run` no longer states the exception the never-list points to"
    );
}

/// r26 D3, "also missing there": before an uninstall preview says an AI
/// coding tool's data stays, `kept_data::check_kept_paths` follows each
/// data path that is there and each link on the way (`the_way_to`), and
/// looks up where each path the uninstall removes is (`removal_roots`),
/// the links among its folders followed and its last name not
/// (`protected::resolve(root, .., false)`). `## Files Banager reads`
/// names those reads with the rest of what that preview reads, and the
/// section it points to says the removed paths are looked up.
#[test]
fn test_what_we_run_lists_the_kept_data_overlap_reads_with_what_an_uninstall_leaves_behind() {
    let doc = read_doc();
    let reads =
        section_body(&doc, "Files Banager reads").expect("a `## Files Banager reads` section");
    let bullet = reads
        .split("\n- ")
        .find(|bullet| bullet.starts_with("What an uninstall leaves behind"))
        .expect("the `What an uninstall leaves behind` bullet of `## Files Banager reads`");
    let bullet = bullet.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "before saying one that is there stays, where it and each link on the way there lead, and where each path the uninstall removes is, the links among that path's folders followed",
        "(`kept_data::check_kept_paths`; `lstat` and `readlink`, one step at a time, never into a protected place)",
        "never a file's contents",
    ] {
        assert!(
            bullet.contains(words),
            "the `What an uninstall leaves behind` bullet of `## Files Banager reads` does not say {words:?}: {bullet}"
        );
    }
    let kept = section_body(&doc, "Data an uninstall leaves behind")
        .expect("a `## Data an uninstall leaves behind` section");
    let kept = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        kept.contains("To compare, Banager looks up where each path the uninstall removes is, following the links among its folders but not its own last name (`protected::resolve` in `kept_data::check_kept_paths`)."),
        "`## Data an uninstall leaves behind` does not say the paths an uninstall removes are looked up for the overlap check"
    );
}
