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
use std::path::Path;

/// The README, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/banager-core).
fn read_readme() -> String {
    let path = Path::new("../../README.md");
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
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
    assert!(english.contains("without a preview or confirmation"));
    let chinese = squeeze(chinese);
    assert!(chinese.contains("在Banager里选择更新或卸载时"));
    assert!(chinese.contains("未经预览或确认就安装、移动或卸载软件包"));
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
/// login is taken out of a web address before saving.
#[test]
fn test_readme_traditional_chinese_summary_uses_the_windows_words() {
    let readme = read_readme();
    let (_, summary) = readme
        .split_once("\n### 執行與隱私\n")
        .expect("README.md has a `### 執行與隱私` heading");
    assert!(summary.contains("安裝、解除安裝及垃圾桶測試"));
    assert!(summary.contains("並在記錄和設定儲存前移除"));
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
