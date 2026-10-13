//! README.md says, in its English block and again in its Chinese one
//! under `## 中文`, which tools' update checks can show each of the two
//! reasons about the installed version that are still Banager's own
//! English. Prose cannot be compiled, so these pin both lists to the
//! recipes. `StandaloneAdapter::check_updates` gives "cannot read the
//! installed version now" before it asks for a published version at all,
//! so every checked recipe's row can show it (a `Latest::Unchecked` one,
//! Codex's or opencode's, returns before it looks); it gives "cannot compare the
//! installed version ... with the published ..." only when
//! `latest::compare_dotted` finds no order, and it asks that only of a
//! `Published::Version` -- never of a `Latest::Command` recipe's answer
//! (Grok Build's), which it trusts as the tool gave it. The README once
//! named Grok Build among the tools that can show the second reason. A
//! recipe added, removed or renamed, or switched between a compared answer
//! and a trusted one, without the README's lists following fails here; a
//! new `Latest` does not compile until `compares_versions` sorts it.

use banager_core::adapters::standalone::recipe::{Latest, Recipe};
use banager_core::adapters::standalone::recipes::RECIPES;
use banager_core::adapters::AdapterMeta;
use banager_core::runner::RealRunner;
use std::path::Path;

/// The README, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/banager-core).
fn read_readme() -> String {
    let path = Path::new("../../README.md");
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A language file of the window, `src/i18n/{name}.json`.
fn read_locale(name: &str) -> serde_json::Value {
    let path = format!("../../src/i18n/{name}.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path} is not JSON: {e}"))
}

/// The README's English block (before its `## 中文` heading) and its
/// Chinese one (after it).
fn blocks(readme: &str) -> (&str, &str) {
    readme
        .split_once("\n## 中文\n")
        .expect("README.md has no `## 中文` heading between its English and Chinese blocks")
}

#[test]
fn test_readme_scopes_confirmation_to_chosen_operations_and_discloses_refresh_migrations() {
    let readme = read_readme();
    let (english, chinese) = blocks(&readme);
    let english = english.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(english.contains("For updates and uninstalls you choose in Banager"));
    // r31 E5: the confirmation is what shows the command and says whether
    // it may ask for the password -- "you" can't be the subject of both.
    assert!(english.contains(
        "For updates and uninstalls you choose in Banager, the confirmation lets you see the exact command before it runs"
    ));
    // n1 skeptic: the button counts command lines (`commandPreview.show`
    // in CommandPreview.tsx), so an update with its cleanup, or any
    // batch, says "Show Commands"; the README names both, as en.json does.
    let en = read_locale("en");
    let press = format!(
        "one press on \"{}\" (\"{}\" when there is more than one)",
        en["commandPreview"]["show_one"].as_str().expect("show_one"),
        en["commandPreview"]["show_other"]
            .as_str()
            .expect("show_other"),
    );
    assert!(
        english.contains(&press),
        "README.md's preview bullet does not say {press:?}"
    );
    assert!(english.contains("without a preview or confirmation"));
    let chinese = squeeze(chinese);
    assert!(chinese.contains("在Banager里选择更新或卸载时"));
    assert!(chinese.contains("未经预览或确认就安装、移动或卸载软件包"));
}

/// r20 R56-1: the README restates the welcome sheet's three titles, and its
/// second one promises confirmation only of what is chosen in Banager --
/// the first check's `brew update`, running behind the sheet, asks nothing.
#[test]
fn test_readme_restates_the_welcome_confirmation_title_as_the_sheet_says_it() {
    let readme = read_readme();
    let (english, chinese) = blocks(&readme);
    let english = english.split_whitespace().collect::<Vec<_>>().join(" ");
    let en = read_locale("en");
    let title = en["welcome"]["confirmTitle"]
        .as_str()
        .expect("welcome.confirmTitle")
        .to_lowercase();
    assert!(title.ends_with("you choose"), "{title:?}");
    assert!(
        english.contains(&format!("see what's installed; {title}; no data collected")),
        "README.md's welcome sentence does not say {title:?}"
    );
    let zh = read_locale("zh-CN");
    let title = zh["welcome"]["confirmTitle"]
        .as_str()
        .expect("welcome.confirmTitle");
    assert!(title.starts_with("你选的"), "{title:?}");
    assert!(
        squeeze(chinese).contains(&format!("看清装了什么；{title}；不收集数据")),
        "README.md's Chinese welcome sentence does not say {title:?}"
    );
}

/// The name a recipe's source goes by (its meta file's `name`), which is
/// how the README spells it.
fn name_of(recipe: &Recipe) -> String {
    AdapterMeta::from_toml(recipe.meta_toml).expect("meta").name
}

/// Whether `check_updates` compares the installed version with what this
/// `Latest` answers: `StandaloneAdapter::published` turns the first three
/// into a `Published::Version`, which `check_updates` hands to
/// `latest::compare_dotted`, and a `Command` into a `Published::ToolSays`,
/// whose verdict it trusts as the tool gave it. No wildcard arm: a new
/// `Latest` has to be sorted here before this file compiles.
fn compares_versions(latest: &Latest) -> bool {
    match latest {
        Latest::ClaudeChannel { .. }
        | Latest::HttpTomlVersion { .. }
        | Latest::HttpJsonField { .. } => true,
        Latest::Command { .. } | Latest::Unchecked => false,
    }
}

/// Whether `check_updates` gets as far as the installed version at all: it
/// returns nothing for a `Latest::Unchecked` recipe (Codex, opencode)
/// before it looks, so that row never says "cannot read the installed
/// version now". No wildcard arm, as above.
fn checks_updates(latest: &Latest) -> bool {
    match latest {
        Latest::ClaudeChannel { .. }
        | Latest::HttpTomlVersion { .. }
        | Latest::HttpJsonField { .. }
        | Latest::Command { .. } => true,
        Latest::Unchecked => false,
    }
}

/// In `RECIPES` order: the name of every recipe whose updates are checked,
/// and the names of the recipes whose update check compares versions.
fn every_and_comparing() -> (Vec<String>, Vec<String>) {
    let every = RECIPES
        .iter()
        .copied()
        .filter(|recipe| checks_updates(&recipe.latest))
        .map(name_of)
        .collect();
    let comparing = RECIPES
        .iter()
        .copied()
        .filter(|recipe| compares_versions(&recipe.latest))
        .map(name_of)
        .collect();
    (every, comparing)
}

/// `names` the way the README's English lists them: "A, B and C".
fn english_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// `names` the way the README's Chinese lists them: "A、B 与 C".
fn chinese_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} 与 {last}", init.join("、")),
    }
}

/// `text` without a single space or line break. The Chinese block is
/// hard-wrapped too, and a line may break between two Chinese characters
/// where no space is meant, so both sides of a comparison are squeezed.
fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// What `text` says around its first `needle` -- `before` characters
/// ahead of it and `after` from it on -- for a failure message.
fn around(text: &str, needle: &str, before: usize, after: usize) -> String {
    let Some(at) = text.find(needle) else {
        return format!("nothing with {needle:?} at all");
    };
    let head: Vec<char> = text[..at].chars().rev().take(before).collect();
    let head: String = head.into_iter().rev().collect();
    let tail: String = text[at..].chars().take(after).collect();
    format!("{head}{tail}")
}

#[test]
fn test_readme_english_block_names_the_tools_each_installed_version_reason_can_come_from() {
    let readme = read_readme();
    let (english, _) = blocks(&readme);
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = english.split_whitespace().collect::<Vec<_>>().join(" ");
    let (every, comparing) = every_and_comparing();

    let compare = format!(
        "\"cannot compare the installed version ... with the published ...\", from {} only",
        english_list(&comparing)
    );
    assert!(
        folded.contains(&compare),
        "README.md's English block does not say \"cannot compare the installed version ... with the published ...\" comes from {} only, the tools whose update check compares versions (a `Latest::Command` tool's answer is trusted, never compared); expected the words {compare:?}, and the README says {:?}",
        english_list(&comparing),
        around(&folded, "\"cannot compare the installed version", 40, 160)
    );

    let read = format!(
        "\"cannot read the installed version now\", from the code {} share",
        english_list(&every)
    );
    assert!(
        folded.contains(&read),
        "README.md's English block does not say \"cannot read the installed version now\" can come from every one of {}, whose rows check_updates can give it before it asks for any published version; expected the words {read:?}, and the README says {:?}",
        english_list(&every),
        around(&folded, "\"cannot read the installed version now", 40, 160)
    );
}

#[test]
fn test_readme_chinese_block_names_the_tools_each_installed_version_reason_can_come_from() {
    let readme = read_readme();
    let (_, chinese) = blocks(&readme);
    let squeezed = squeeze(chinese);
    let (every, comparing) = every_and_comparing();

    let compare = squeeze(&format!(
        "已安装版本与发布版本无法比较，只出自 {}，",
        chinese_list(&comparing)
    ));
    assert!(
        squeezed.contains(&compare),
        "README.md's Chinese block does not say that the reason the installed version cannot be compared with the published one comes from {} only, the tools whose update check compares versions (a `Latest::Command` tool's answer is trusted, never compared); expected the words {compare:?}, and the README says {:?}",
        chinese_list(&comparing),
        around(&squeezed, "无法比较", 100, 20)
    );

    let read = squeeze(&format!(
        "读不到已安装版本，出自 {} 共用的代码",
        chinese_list(&every)
    ));
    assert!(
        squeezed.contains(&read),
        "README.md's Chinese block does not say that the reason the installed version cannot be read now can come from every one of {}, whose rows check_updates can give it before it asks for any published version; expected the words {read:?}, and the README says {:?}",
        chinese_list(&every),
        around(&squeezed, "读不到已安装版本", 100, 60)
    );
}

/// r30 Z1: the README's Traditional Chinese summary (`### 執行與隱私`, the
/// end of the file) says 解除安裝 and 記錄 as the zh-Hant window does, not
/// 移除 for an uninstall or 歷程 for the history. 移除 stays where the
/// login is taken out of a web address before saving. A bare 記錄 does not
/// say which file it means, so the history goes by the FAQ's name for it,
/// 「更新和解除安裝記錄」 (n1 skeptic).
#[test]
fn test_readme_traditional_chinese_summary_uses_the_windows_words() {
    let readme = read_readme();
    let (_, summary) = readme
        .split_once("\n### 執行與隱私\n")
        .expect("README.md has a `### 執行與隱私` heading");
    assert!(summary.contains("安裝、解除安裝及垃圾桶測試"));
    let history = "更新和解除安裝記錄";
    let faq = read_locale("zh-Hant")["faq"]["questions"]["changesMac"]["answer"]
        .as_str()
        .expect("the zh-Hant FAQ's changesMac answer")
        .to_string();
    assert!(
        faq.contains(history),
        "the zh-Hant FAQ no longer calls the history {history:?}; say what it says"
    );
    assert!(summary.contains(&format!("並在{history}及設定儲存前移除")));
    assert!(!summary.contains("在記錄和設定"));
    for unlike in ["安裝移除", "歷程", "紀錄", "命令"] {
        assert!(
            !summary.contains(unlike),
            "README.md's `### 執行與隱私` says {unlike:?} where the zh-Hant window does not"
        );
    }
}

/// r31 E4: Copy Diagnostic Info adds a source's error details whatever way
/// it gave no answer (`runner::no_answer::of` keeps them for all three), so
/// the README names all three states as the window does -- not only
/// "isn't responding", the one the window keeps for a source that ran out
/// of time.
#[test]
fn test_readme_names_every_state_whose_error_details_diagnostic_info_includes() {
    let readme = read_readme();
    let (english, chinese) = blocks(&readme);
    let english = english.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(english.contains(
        "For a source that can't run, ran into an error or isn't responding, it includes the error details"
    ));
    assert!(squeeze(chinese)
        .contains("无法运行、运行时出错或没有响应的来源会附上它的工具输出的错误详情"));
}

/// r20 R56-3: a quit cancels only what can be cancelled and waits only for
/// that (`quit::cancels`, `quit::waits_for` in src-tauri/src/quit.rs): a
/// running rustup self update or self uninstall (`CancelPolicy::NoCancel`)
/// is neither, and its command runs on after Banager exits, with nowhere
/// to write its output (docs/what-we-run.md: a write to its closed pipes
/// fails), so it can fail partway. The README said the question's reason,
/// "quitting now stops them", and then "it says so of one that has started
/// and can't be cancelled"; then (r20 R56 skeptic 2) that its command
/// "runs on after Banager quits", as if it would finish by itself.
#[test]
fn test_readme_says_quit_neither_stops_nor_waits_for_an_operation_that_cannot_be_cancelled() {
    let readme = read_readme();
    let (english, chinese) = blocks(&readme);
    let english = english.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(!english.contains("it says so of one"));
    assert!(english.contains("quitting now stops the ones it can"));
    assert!(!english.contains("runs on after Banager quits"));
    assert!(english.contains(
        "quitting neither stops nor waits for it, and its command is left running with nowhere \
         to write its output, so it can fail partway"
    ));
    let chinese = squeeze(chinese);
    assert!(chinese.contains("现在退出会中断能中断的那些"));
    assert!(!chinese.contains("照样运行"));
    assert!(chinese.contains(
        "退出既不中断它，也不等它，Banager退出后它的命令还在运行，输出却无处可写，可能中途失败"
    ));
}

/// r20 R56-5: the window cannot install anything new -- `ipc.rs` refuses
/// an `OpKind::Install` the window plans, and the README's "What it
/// deliberately does not do yet" says so -- yet the source table's column
/// read "Installs / updates / removes", with "yes" for six sources.
#[test]
fn test_readme_source_table_offers_no_install() {
    let readme = read_readme();
    let (english, _) = blocks(&readme);
    let header = english
        .lines()
        .find(|line| line.starts_with("| Source |"))
        .expect("README.md has no source table");
    assert_eq!(header, "| Source | Reads | Updates / removes |");
    assert!(english.contains("no way to install something new"));
}

#[test]
fn test_readme_preview_sentence_names_every_variable_the_runner_sets_on_every_command() {
    // R58-1: the preview shows the plan's own variables only
    // (`redact::preview_env`), and the README says it shows all Banager
    // sets but the ones it names as never shown. `RealRunner::EVERY_COMMAND_ENV`
    // goes on every command too (real.rs, R63-1), so each entry has to be
    // among those named, in both blocks.
    let readme = read_readme();
    let (english, chinese) = blocks(&readme);
    let english = english.split_whitespace().collect::<Vec<_>>().join(" ");
    let start = english
        .find("the confirmation lets you see the exact command before it runs")
        .expect("README.md's English block has no preview sentence");
    let english = &english[start..(start + 700).min(english.len())];
    let chinese = squeeze(chinese);
    let start = chinese
        .find("都能在它运行之前看到确切的命令")
        .expect("README.md's Chinese block has no preview sentence");
    let chinese: String = chinese[start..].chars().take(260).collect();
    for (name, value) in RealRunner::EVERY_COMMAND_ENV {
        let shown = format!("`{name}={value}`");
        assert!(
            english.contains(&shown),
            "README.md's English preview sentence does not name {shown}: {english}"
        );
        assert!(
            chinese.contains(&shown),
            "README.md's Chinese preview sentence does not name {shown}: {chinese}"
        );
    }
}
