//! README.md says, in its English block and again in its Chinese one
//! under `## 中文`, which tools' update checks can show each of the two
//! reasons about the installed version that are still Canager's own
//! English. Prose cannot be compiled, so these pin both lists to the
//! recipes. `StandaloneAdapter::check_updates` gives "cannot read the
//! installed version now" before it asks for a published version at all,
//! so every recipe's row can show it; it gives "cannot compare the
//! installed version ... with the published ..." only when
//! `latest::compare_dotted` finds no order, and it asks that only of a
//! `Published::Version` -- never of a `Latest::Command` recipe's answer
//! (Grok Build's), which it trusts as the tool gave it. The README once
//! named Grok Build among the tools that can show the second reason. A
//! recipe added, removed or renamed, or switched between a compared answer
//! and a trusted one, without the README's lists following fails here; a
//! new `Latest` does not compile until `compares_versions` sorts it.

use canager_core::adapters::standalone::recipe::{Latest, Recipe};
use canager_core::adapters::standalone::recipes::RECIPES;
use canager_core::adapters::AdapterMeta;
use std::path::Path;

/// The README, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/canager-core).
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
        Latest::Command { .. } => false,
    }
}

/// In `RECIPES` order: every recipe's name, and the names of the recipes
/// whose update check compares versions.
fn every_and_comparing() -> (Vec<String>, Vec<String>) {
    let every = RECIPES.iter().copied().map(name_of).collect();
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
