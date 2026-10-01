//! What an uninstall leaves behind: the folders and files a tool keeps its
//! own data in, which no source's uninstall command touches -- `npm
//! uninstall -g`, `brew uninstall` (no `--zap`), the path-list uninstalls
//! alike -- named in the uninstall preview so nobody is surprised that
//! `~/.claude` is still there, nor wonders where 40 GB of models went.
//!
//! Which paths: the data folders of the tool's family in the bundled table
//! (`families.rs`, `data_paths`, each from the vendor's own documents), and
//! for the Ollama family -- Homebrew's formula `ollama` and cask
//! `ollama-app` -- the models folder, `~/.ollama/models` (Ollama's FAQ; the
//! `OLLAMA_MODELS` a shell may set is not in Banager's environment, so a
//! models folder elsewhere is not named). Only a path that is there gets a
//! line, and only one the plan does not already name: Claude Code's own
//! installer's uninstall lists `~/.claude` and `~/.claude.json` among what
//! it keeps (`Warning::WillKeep`), which is said once, there. A folder two
//! tools share is measured without the part the table gives the other one
//! (`others_inside`): `~/.gemini` is Gemini CLI's, but Antigravity CLI
//! keeps everything of its own in `~/.gemini/antigravity-cli`, which on
//! the author's Mac was 99 % of `~/.gemini`; Gemini CLI's line leaves it
//! out and says so. The remainder is not shown to be Gemini CLI's alone:
//! on that Mac it was only `config/`, `tasks/` and `users/` (about 7.4 MB),
//! with none of Gemini CLI's documented files such as `settings.json`; the
//! line names only the part that is Antigravity CLI's and claims no more.
//!
//! How: read-only, as disk use is measured (`size::look_at`: `lstat`,
//! `readdir`, `readlink`; nothing opened, nothing written), under a budget
//! small enough that the preview stays quick (`BUDGET`), and never into
//! the places macOS asks about (`size::Protected`, built from the one list
//! in `crate::protected` that the command check uses too, whatever case
//! spells a place): a path that leads into one is named with no size. Nothing here deletes anything, and nothing
//! offers to.

use crate::families;
use crate::model::{KeptData, OthersData, Warning};
use crate::size::{look_at, LookBudget, Looked, Protected, SizeBudget};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The family whose tool's models are kept: Homebrew's `ollama` formula and
/// `ollama-app` cask (`ai-tools.json`).
pub const OLLAMA_FAMILY: &str = "ollama";

/// Where Ollama keeps the models it downloaded, by default.
pub const OLLAMA_MODELS: &str = "~/.ollama/models";

/// What one uninstall preview may spend measuring what it leaves behind,
/// across every path it names: a second, or this many entries. A path it
/// stopped short of says "at least"; one it did not reach has no size.
pub const BUDGET: SizeBudget = SizeBudget {
    max_entries: 100_000,
    max_duration: Duration::from_secs(1),
};

/// What a data path holds that is not the tool's data, by the path as the
/// table spells it, relative to it: another copy of the tool, installed by
/// its own installer. `~/.codex/packages/standalone` is Codex's own install
/// (`adapters/standalone/release_link.rs`), which uninstalling npm's
/// `@openai/codex` leaves where it is, and which the size of what stays as
/// "this tool's settings and data" must not count.
const LEFT_OUT: &[(&str, &str)] = &[("~/.codex", "packages/standalone")];

/// The paths an uninstall of a tool of `family` leaves behind, if they are
/// there, as the table spells them, with what each holds: the family's
/// `data_paths`, then the models folder for Ollama's.
pub fn data_paths(family: &str) -> Vec<(&'static str, KeptData)> {
    let mut paths: Vec<(&'static str, KeptData)> = families::family(family)
        .map(|f| {
            f.data_paths
                .iter()
                .map(|path| (path.as_str(), KeptData::ToolData))
                .collect()
        })
        .unwrap_or_default();
    if family == OLLAMA_FAMILY {
        paths.push((OLLAMA_MODELS, KeptData::Models));
    }
    paths
}

/// What other families keep inside `path`, a data path of `family`: each
/// other family's data path that lies under it, as the table spells it,
/// with that family's name -- `~/.gemini/antigravity-cli`, Antigravity
/// CLI's, inside Gemini CLI's `~/.gemini`. Left out of `path`'s size and
/// named on its line, so one tool's uninstall does not call another's data
/// its own.
pub fn others_inside(family: &str, path: &str) -> Vec<(&'static str, &'static str)> {
    let folder = format!("{}/", path.trim_end_matches('/'));
    families::families()
        .iter()
        .filter(|other| other.id != family)
        .flat_map(|other| {
            other
                .data_paths
                .iter()
                .filter(|inner| inner.starts_with(&folder) && inner.len() > folder.len())
                .map(move |inner| (inner.as_str(), other.name_en.as_str()))
        })
        .collect()
}

/// `~/…` under `home`; `None` for anything not spelled from the home folder.
fn under_home(home: &Path, path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix("~/")?;
    (!rest.is_empty()).then(|| home.join(rest))
}

/// The paths `warnings` already name, as they spell them: what a path-list
/// uninstall moves, keeps or found gone.
pub fn named_paths(warnings: &[Warning]) -> Vec<String> {
    warnings
        .iter()
        .filter_map(|warning| match warning {
            Warning::WillTrash { path, .. }
            | Warning::WillKeep { path, .. }
            | Warning::AlreadyGone { path }
            | Warning::KeepsData { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect()
}

/// A `Warning::KeepsData` for each of `family`'s data paths (`data_paths`)
/// that is there under `home` and not in `already_named`, in order, each
/// measured within `budget` (shared by all of them). Blocking: it reads
/// the disk.
pub fn kept_data(
    home: &Path,
    family: &str,
    already_named: &[String],
    budget: SizeBudget,
) -> Vec<Warning> {
    let paths = data_paths(family);
    if paths.is_empty() {
        return Vec::new();
    }
    let protected = Protected::new(home);
    let mut budget = LookBudget::new(budget);
    let mut warnings = Vec::new();
    for (path, what) in paths {
        if already_named.iter().any(|named| named == path) {
            continue;
        }
        let Some(absolute) = under_home(home, path) else {
            continue;
        };
        // What the size leaves out: another copy of this tool
        // (`LEFT_OUT`, said as `left_out`), then what other tools keep
        // inside (`others_inside`, said as `others`), relative to `path`.
        let copies: Vec<&str> = LEFT_OUT
            .iter()
            .filter(|(of, _)| *of == path)
            .map(|(_, inside)| *inside)
            .collect();
        let inside = others_inside(family, path);
        let leave_out: Vec<&str> = copies
            .iter()
            .copied()
            .chain(inside.iter().map(|(inner, _)| {
                inner[path.trim_end_matches('/').len() + 1..].trim_end_matches('/')
            }))
            .collect();
        let (looked, met) = look_at(&absolute, &protected, &mut budget, &leave_out);
        if let Looked::There(size) = looked {
            let left_out = met
                .iter()
                .filter(|index| **index < copies.len())
                .map(|index| format!("{path}/{}", copies[*index]))
                .collect();
            let others = met
                .iter()
                .filter(|index| **index >= copies.len())
                .map(|index| {
                    let (inner, tool) = inside[index - copies.len()];
                    OthersData {
                        path: inner.to_string(),
                        tool: tool.to_string(),
                    }
                })
                .collect();
            warnings.push(Warning::KeepsData {
                path: path.to_string(),
                what,
                size,
                left_out,
                others,
            });
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeptWhat;
    use std::os::unix::fs::symlink;

    /// A home folder of a test's own, removed when dropped.
    struct Home(PathBuf);

    impl Home {
        fn new(tag: &str) -> Home {
            let dir = std::env::temp_dir().join(format!(
                "banager-kept-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            // The resolved spelling: the system's temporary folder is
            // behind a link (`/var` -> `/private/var`).
            Home(std::fs::canonicalize(&dir).unwrap())
        }

        fn file(&self, relative: &str, len: usize) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, vec![7u8; len]).unwrap();
            path
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn size_of(warning: &Warning) -> Option<crate::size::Measured> {
        match warning {
            Warning::KeepsData { size, .. } => *size,
            other => panic!("expected KeepsData, got {other:?}"),
        }
    }

    fn paths_of(warnings: &[Warning]) -> Vec<String> {
        named_paths(warnings)
    }

    #[test]
    fn test_the_paths_are_the_tables_and_ollamas_models_folder() {
        assert_eq!(
            data_paths("claude-code"),
            vec![
                ("~/.claude", KeptData::ToolData),
                ("~/.claude.json", KeptData::ToolData)
            ]
        );
        assert_eq!(data_paths("codex"), vec![("~/.codex", KeptData::ToolData)]);
        assert_eq!(
            data_paths("gemini-cli"),
            vec![("~/.gemini", KeptData::ToolData)]
        );
        assert_eq!(
            data_paths("qwen-code"),
            vec![("~/.qwen", KeptData::ToolData)]
        );
        assert_eq!(
            data_paths("ollama"),
            vec![("~/.ollama/models", KeptData::Models)]
        );
        assert_eq!(
            data_paths("antigravity-cli"),
            vec![("~/.gemini/antigravity-cli", KeptData::ToolData)]
        );
        // A family with no verified folder, and no family at all.
        assert!(data_paths("aider").is_empty());
        assert!(data_paths("no-such-family").is_empty());
    }

    #[test]
    fn test_a_path_that_is_there_is_named_with_its_size_and_one_that_is_not_is_not() {
        let home = Home::new("present");
        home.file(".claude/projects/a.jsonl", 40_000);
        home.file(".claude/settings.json", 300);
        // No `~/.claude.json`.
        let warnings = kept_data(&home.0, "claude-code", &[], BUDGET);
        assert_eq!(paths_of(&warnings), vec!["~/.claude".to_string()]);
        let size = size_of(&warnings[0]).expect("a folder it could read has a size");
        assert!(size.bytes >= 40_000, "{size:?}");
        assert!(!size.partial && !size.at_least, "{size:?}");
        assert!(matches!(
            warnings[0],
            Warning::KeepsData {
                what: KeptData::ToolData,
                ..
            }
        ));

        home.file(".claude.json", 10);
        let warnings = kept_data(&home.0, "claude-code", &[], BUDGET);
        assert_eq!(
            paths_of(&warnings),
            vec!["~/.claude".to_string(), "~/.claude.json".to_string()]
        );
    }

    #[test]
    fn test_nothing_there_means_no_line() {
        let home = Home::new("absent");
        assert!(kept_data(&home.0, "claude-code", &[], BUDGET).is_empty());
        assert!(kept_data(&home.0, "ollama", &[], BUDGET).is_empty());
        assert!(kept_data(&home.0, "aider", &[], BUDGET).is_empty());
        // A link that leads nowhere keeps nothing.
        symlink(home.0.join("gone"), home.0.join(".codex")).unwrap();
        assert!(kept_data(&home.0, "codex", &[], BUDGET).is_empty());
    }

    #[test]
    fn test_ollamas_models_folder_is_named_as_models() {
        let home = Home::new("ollama");
        home.file(".ollama/models/blobs/sha256-1", 70_000);
        home.file(".ollama/id_ed25519", 10);
        let warnings = kept_data(&home.0, "ollama", &[], BUDGET);
        assert_eq!(warnings.len(), 1);
        match &warnings[0] {
            Warning::KeepsData {
                path, what, size, ..
            } => {
                assert_eq!(path, "~/.ollama/models");
                assert_eq!(*what, KeptData::Models);
                assert!(size.unwrap().bytes >= 70_000);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_a_spent_budget_says_at_least_and_one_never_reached_has_no_size() {
        let home = Home::new("budget");
        for i in 0..20 {
            home.file(&format!(".claude/projects/{i}.jsonl"), 100);
        }
        home.file(".claude.json", 10);
        let tight = SizeBudget {
            max_entries: 5,
            max_duration: Duration::from_secs(1),
        };
        let warnings = kept_data(&home.0, "claude-code", &[], tight);
        assert_eq!(
            paths_of(&warnings),
            vec!["~/.claude".to_string(), "~/.claude.json".to_string()]
        );
        let first = size_of(&warnings[0]).expect("part of it was reached");
        assert!(first.at_least, "{first:?}");
        // The budget was gone before `~/.claude.json` was reached: still
        // named, with no size rather than "at least about 0 KB".
        assert_eq!(size_of(&warnings[1]), None);
    }

    #[test]
    fn test_a_path_leading_into_a_protected_place_is_named_and_never_measured() {
        let home = Home::new("protected");
        home.file("Documents/claude-data/big.bin", 90_000);
        symlink(home.0.join("Documents/claude-data"), home.0.join(".claude")).unwrap();
        let warnings = kept_data(&home.0, "claude-code", &[], BUDGET);
        assert_eq!(paths_of(&warnings), vec!["~/.claude".to_string()]);
        assert_eq!(
            size_of(&warnings[0]),
            None,
            "a folder in ~/Documents is never walked"
        );
    }

    #[test]
    fn test_a_path_leading_into_any_place_on_the_shared_list_whatever_its_case_is_never_measured() {
        // `size::Protected` is built from `crate::protected`, the one list
        // the command check keeps out of too: the two Containers folders
        // were on its list only, and a place is matched whatever its case.
        let home = Home::new("shared-list");
        home.file("Library/Group Containers/group.claude/big.bin", 90_000);
        home.file("Library/Containers/codex/big.bin", 90_000);
        symlink(
            home.0.join("Library/Group Containers/group.claude"),
            home.0.join(".claude"),
        )
        .unwrap();
        symlink(
            home.0.join("library/containers/codex"),
            home.0.join(".codex"),
        )
        .unwrap();
        for (family, path) in [("claude-code", "~/.claude"), ("codex", "~/.codex")] {
            let warnings = kept_data(&home.0, family, &[], BUDGET);
            assert_eq!(paths_of(&warnings), vec![path.to_string()], "{family}");
            assert_eq!(size_of(&warnings[0]), None, "{path} is named, never walked");
        }
    }

    #[test]
    fn test_a_path_the_plan_already_names_is_not_said_twice() {
        let home = Home::new("named");
        home.file(".claude/settings.json", 300);
        home.file(".claude.json", 10);
        let standalone = vec![
            Warning::WillKeep {
                path: "~/.claude".to_string(),
                what: KeptWhat::SettingsAndHistory,
            },
            Warning::WillKeep {
                path: "~/.claude.json".to_string(),
                what: KeptWhat::Settings,
            },
        ];
        assert!(kept_data(&home.0, "claude-code", &named_paths(&standalone), BUDGET).is_empty());
        // Only one named: the other still gets its line.
        let warnings = kept_data(
            &home.0,
            "claude-code",
            &named_paths(&standalone[..1]),
            BUDGET,
        );
        assert_eq!(paths_of(&warnings), vec!["~/.claude.json".to_string()]);
    }

    #[test]
    fn test_a_standalone_recipe_spells_its_familys_data_paths_as_the_table_does() {
        // `kept_data` skips what the plan names by comparing spellings, so
        // a recipe that kept `~/.claude/` while the table says `~/.claude`
        // would have the dialog list the folder twice. Pinned for every
        // standalone recipe that removes by paths.
        use crate::adapters::standalone::recipe::Uninstall;
        use crate::adapters::standalone::recipes::RECIPES;
        use crate::model::{ArtifactKey, ArtifactKind};
        let same = |a: &str, b: &str| a.trim_end_matches('/') == b.trim_end_matches('/');
        let mut matched = Vec::new();
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let key = ArtifactKey {
                instance_id: "x".to_string(),
                kind: ArtifactKind::Binary,
                name: recipe.id.to_string(),
            };
            let Some(family) = families::family_for(&format!("standalone-{}", recipe.id), &key)
            else {
                continue;
            };
            let named = remove
                .iter()
                .map(|spec| spec.path)
                .chain(keep.iter().map(|spec| spec.path));
            for path in named {
                for (data, _) in data_paths(&family.id) {
                    if same(path, data) {
                        assert_eq!(path, data, "{} spells it differently", recipe.id);
                        matched.push(data);
                    }
                }
            }
        }
        // Claude Code's two, at least: not a test that compares nothing.
        assert!(matched.contains(&"~/.claude"), "{matched:?}");
        assert!(matched.contains(&"~/.claude.json"), "{matched:?}");
        // And Antigravity CLI's, which its own uninstall keeps, so the
        // dialog names it once, there.
        assert!(
            matched.contains(&"~/.gemini/antigravity-cli"),
            "{matched:?}"
        );
    }

    #[test]
    fn test_antigravitys_folder_is_the_only_other_tools_data_inside_one() {
        assert_eq!(
            others_inside("gemini-cli", "~/.gemini"),
            vec![("~/.gemini/antigravity-cli", "Antigravity CLI")]
        );
        // Its own family's path is not "another tool's", and a sibling
        // spelled with the same start is not inside.
        assert!(others_inside("antigravity-cli", "~/.gemini/antigravity-cli").is_empty());
        assert!(others_inside("claude-code", "~/.claude").is_empty());
        // Every pair in the table: the name on the line is the family's,
        // and the table spells it the same in both languages, since the
        // wire carries one spelling.
        for family in families::families() {
            for path in &family.data_paths {
                for (inner, tool) in others_inside(&family.id, path) {
                    let owner = families::families()
                        .iter()
                        .find(|f| f.data_paths.iter().any(|p| p == inner))
                        .unwrap();
                    assert_eq!(owner.name_en, tool);
                    assert_eq!(owner.name_en, owner.name_zh, "{}", owner.id);
                }
            }
        }
    }

    #[test]
    fn test_gemini_clis_line_leaves_antigravitys_folder_out_and_names_it() {
        let home = Home::new("gemini");
        home.file(".gemini/users/a.json", 3_000);
        home.file(".gemini/config/settings.json", 1_000);
        home.file(".gemini/antigravity-cli/log/big.log", 900_000);
        home.file(".gemini/antigravity-cli/cache/blob", 400_000);
        let warnings = kept_data(&home.0, "gemini-cli", &[], BUDGET);
        assert_eq!(warnings.len(), 1);
        match &warnings[0] {
            Warning::KeepsData {
                path, size, others, ..
            } => {
                assert_eq!(path, "~/.gemini");
                let size = size.expect("measured");
                assert!(size.bytes >= 4_000, "{size:?}");
                assert!(
                    size.bytes < 100_000,
                    "Antigravity's 1.3 MB is not counted: {size:?}"
                );
                assert_eq!(
                    others,
                    &vec![OthersData {
                        path: "~/.gemini/antigravity-cli".to_string(),
                        tool: "Antigravity CLI".to_string(),
                    }]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_gemini_clis_line_names_no_other_tool_when_antigravitys_folder_is_not_there() {
        let home = Home::new("gemini-alone");
        home.file(".gemini/users/a.json", 3_000);
        // A file of Gemini CLI's own whose name only starts the same.
        home.file(".gemini/antigravity-cli-notes.txt", 2_000);
        let warnings = kept_data(&home.0, "gemini-cli", &[], BUDGET);
        match &warnings[..] {
            [Warning::KeepsData { size, others, .. }] => {
                assert!(others.is_empty(), "{others:?}");
                assert!(size.unwrap().bytes >= 5_000, "{size:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_antigravity_as_a_link_is_left_out_and_never_followed() {
        // `~/.gemini/antigravity-cli` as a link elsewhere: the link is
        // skipped like the folder would be, and what it leads to is not
        // walked as part of `~/.gemini` (a walk never follows links).
        let home = Home::new("gemini-link");
        home.file(".gemini/users/a.json", 3_000);
        home.file("elsewhere/big.bin", 800_000);
        symlink(
            home.0.join("elsewhere"),
            home.0.join(".gemini/antigravity-cli"),
        )
        .unwrap();
        let warnings = kept_data(&home.0, "gemini-cli", &[], BUDGET);
        match &warnings[..] {
            [Warning::KeepsData { size, others, .. }] => {
                assert_eq!(others.len(), 1);
                assert!(size.unwrap().bytes < 100_000, "{size:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_codexs_own_install_inside_its_folder_is_left_out_of_what_stays_and_named() {
        // npm's @openai/codex uninstalled while Codex's own install is
        // there: ~/.codex stays, and its size is the settings and data,
        // not the other copy's program under packages/standalone.
        let home = Home::new("codex-twin");
        home.file(".codex/config.toml", 4_000);
        home.file(".codex/sessions/a.jsonl", 20_000);
        home.file(
            ".codex/packages/standalone/releases/0.159.3-aarch64-apple-darwin/bin/codex",
            900_000,
        );
        let warnings = kept_data(&home.0, "codex", &[], BUDGET);
        assert_eq!(warnings.len(), 1);
        match &warnings[0] {
            Warning::KeepsData {
                path,
                size,
                left_out,
                ..
            } => {
                assert_eq!(path, "~/.codex");
                assert_eq!(left_out, &vec!["~/.codex/packages/standalone".to_string()]);
                let bytes = size.expect("measured").bytes;
                assert!((24_000..900_000).contains(&bytes), "{bytes}");
            }
            other => panic!("{other:?}"),
        }

        // Without Codex's own install, nothing is left out.
        let home = Home::new("codex-alone");
        home.file(".codex/config.toml", 4_000);
        match &kept_data(&home.0, "codex", &[], BUDGET)[0] {
            Warning::KeepsData { left_out, .. } => assert!(left_out.is_empty()),
            other => panic!("{other:?}"),
        }
        // Nor for another family's folder.
        let home = Home::new("claude-standalone-name");
        home.file(".claude/packages/standalone/x", 4_000);
        match &kept_data(&home.0, "claude-code", &[], BUDGET)[0] {
            Warning::KeepsData { left_out, size, .. } => {
                assert!(left_out.is_empty());
                assert!(size.unwrap().bytes >= 4_000);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_reading_what_stays_writes_and_removes_nothing() {
        let home = Home::new("untouched");
        let file = home.file(".codex/config.toml", 50);
        let before = std::fs::read(&file).unwrap();
        let warnings = kept_data(&home.0, "codex", &[], BUDGET);
        assert_eq!(warnings.len(), 1);
        assert_eq!(std::fs::read(&file).unwrap(), before);
        let names: Vec<_> = std::fs::read_dir(home.0.join(".codex"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("config.toml")]);
    }
}
