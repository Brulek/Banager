//! What an uninstall leaves behind: the folders and files a tool keeps its
//! own data in, which no source's uninstall command touches -- `npm
//! uninstall -g`, `brew uninstall` (no `--zap`), the path-list uninstalls
//! alike -- named in the uninstall preview so nobody is surprised that
//! `~/.claude` is still there, nor wonders where 40 GB of models went.
//!
//! Which paths: the data folders of the tool's family in the bundled table
//! (`families.rs`, `data_paths`, each from the vendor's own docs or
//! source), and
//! for the Ollama family -- Homebrew's formula `ollama` and cask
//! `ollama-app` -- the models folder, `~/.ollama/models` (Ollama's FAQ; the
//! `OLLAMA_MODELS` a shell may set is not in Banager's environment, so a
//! models folder elsewhere is not named). Only a path that is there gets a
//! line, and only one the plan does not already name: Claude Code's own
//! installer's uninstall lists `~/.claude` and `~/.claude.json` among what
//! it keeps (`Warning::WillKeep`), which is said once, there, as Grok
//! Build's own lists `~/.grok`. A folder two
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
//! `readdir`, `readlink`, from folders held open with `O_NOFOLLOW`; no
//! file opened, nothing written), under a budget
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
/// table spells it, relative to it: another copy of the tool, put there by
/// its own installer or updater, which uninstalling the copy a source
/// manages leaves where it is, and which the size of what stays as "this
/// tool's settings and data" must not count (`families.rs` module doc has
/// each one's source):
/// - `~/.codex/packages/standalone`: Codex's own install
///   (`adapters/standalone/release_link.rs`), beside npm's `@openai/codex`;
/// - `~/.qoder/bin/qodercli`: the versioned programs Qoder CLI's install
///   script puts there, beside npm's `@qoder-ai/qodercli`;
/// - `~/.copilot/pkg`: the copies of the program GitHub Copilot CLI's
///   updater downloads;
/// - `~/.grok/downloads`: Grok Build's install script's program (Banager's
///   `grok` recipe), beside Homebrew's cask `grok-build`.
pub const LEFT_OUT: &[(&str, &str)] = &[
    ("~/.codex", "packages/standalone"),
    ("~/.qoder", "bin/qodercli"),
    ("~/.copilot", "pkg"),
    ("~/.grok", "downloads"),
];

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
        // opencode's, from its docs: data first, then settings.
        assert_eq!(
            data_paths("opencode"),
            vec![
                ("~/.local/share/opencode", KeptData::ToolData),
                ("~/.config/opencode", KeptData::ToolData)
            ]
        );
        // Aider's folder, then the three settings files its docs say it
        // reads from the home folder.
        assert_eq!(
            data_paths("aider"),
            vec![
                ("~/.aider", KeptData::ToolData),
                ("~/.aider.conf.yml", KeptData::ToolData),
                ("~/.aider.model.settings.yml", KeptData::ToolData),
                ("~/.aider.model.metadata.json", KeptData::ToolData)
            ]
        );
        assert_eq!(
            data_paths("grok-build"),
            vec![("~/.grok", KeptData::ToolData)]
        );
        // Every family has its folders now but Ollama, whose models folder
        // is kept_data's own; and no family at all has none.
        for family in families::families() {
            assert!(!data_paths(&family.id).is_empty(), "{}", family.id);
        }
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
    fn test_a_link_among_the_folders_of_a_path_is_never_followed_into_a_protected_place() {
        use std::os::unix::fs::PermissionsExt;
        // `~/.ollama/models` is the one path two levels below home: with
        // `~/.ollama` a link into a locked folder in ~/Documents, an
        // `lstat` of the whole path would follow that link and fail on
        // the lock (or, unlocked, raise the macOS prompt). Nothing past
        // the link is looked at: the path is named, with no size.
        let home = Home::new("ollama-protected");
        home.file("Documents/ollama/models/blobs/sha256-1", 70_000);
        let locked = home.0.join("Documents/ollama");
        symlink(&locked, home.0.join(".ollama")).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let warnings = kept_data(&home.0, "ollama", &[], BUDGET);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(paths_of(&warnings), vec!["~/.ollama/models".to_string()]);
        assert_eq!(size_of(&warnings[0]), None);
    }

    #[test]
    fn test_a_link_among_the_folders_of_a_path_onto_another_disk_is_never_followed() {
        let home = Home::new("ollama-volumes");
        symlink(
            "/Volumes/Banager-test-no-such-disk/ollama",
            home.0.join(".ollama"),
        )
        .unwrap();
        let warnings = kept_data(&home.0, "ollama", &[], BUDGET);
        assert_eq!(paths_of(&warnings), vec!["~/.ollama/models".to_string()]);
        assert_eq!(size_of(&warnings[0]), None);
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
        // And Grok Build's: its own uninstall keeps `~/.grok`, the folder
        // the table names for the family.
        assert!(matched.contains(&"~/.grok"), "{matched:?}");
    }

    #[test]
    fn test_opencodes_own_install_neither_names_nor_holds_its_familys_data() {
        // The other half of the spelling check above, for the one recipe
        // with data paths and no path list: opencode's own install is
        // listed only (no uninstall, so no preview names a path twice),
        // and its root `~/.opencode` holds none of the family's data, so
        // the row's size and a kept line never count the same bytes.
        use crate::adapters::standalone::recipes::OPENCODE;
        use crate::model::{ArtifactKey, ArtifactKind};
        assert!(OPENCODE.uninstall.is_none());
        let key = ArtifactKey {
            instance_id: "x".to_string(),
            kind: ArtifactKind::Binary,
            name: OPENCODE.id.to_string(),
        };
        let family = families::family_for("standalone-opencode", &key).expect("in the table");
        assert_eq!(family.id, "opencode");
        let root = format!("{}/", OPENCODE.route.root.trim_end_matches('/'));
        let paths = data_paths(&family.id);
        assert_eq!(paths.len(), 2);
        for (path, _) in paths {
            let path = format!("{}/", path.trim_end_matches('/'));
            assert!(
                !path.starts_with(&root) && !root.starts_with(&path),
                "{path}"
            );
        }
    }

    #[test]
    fn test_opencode_uninstalled_from_npm_names_both_folders_that_are_there() {
        let home = Home::new("opencode");
        home.file(".local/share/opencode/auth.json", 2_000);
        home.file(".local/share/opencode/project/a/session.json", 30_000);
        home.file(".config/opencode/opencode.json", 1_000);
        let warnings = kept_data(&home.0, "opencode", &[], BUDGET);
        assert_eq!(
            paths_of(&warnings),
            vec![
                "~/.local/share/opencode".to_string(),
                "~/.config/opencode".to_string()
            ]
        );
        // Only the one that is there, when settings were never written.
        let home = Home::new("opencode-data-only");
        home.file(".local/share/opencode/log/x.log", 500);
        let warnings = kept_data(&home.0, "opencode", &[], BUDGET);
        assert_eq!(
            paths_of(&warnings),
            vec!["~/.local/share/opencode".to_string()]
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

    #[test]
    fn test_every_left_out_folder_is_inside_a_path_the_table_names() {
        // A `LEFT_OUT` entry whose folder the table no longer names would
        // never be used; one spelled differently would never match.
        for (of, inside) in LEFT_OUT {
            assert!(
                families::families()
                    .iter()
                    .any(|f| f.data_paths.iter().any(|p| p == of)),
                "{of} is no family's data path"
            );
            assert!(
                !inside.is_empty() && !inside.starts_with('/') && !inside.ends_with('/'),
                "{of}: {inside}"
            );
        }
    }

    #[test]
    fn test_grok_builds_folder_stays_after_homebrews_cask_without_the_install_scripts_program() {
        // Homebrew's cask `grok-build` uninstalled while grok's own
        // install is there too: `~/.grok` stays, and its size is the
        // settings, login and sessions, not that copy's program in
        // `downloads/`.
        let home = Home::new("grok-cask");
        home.file(".grok/config.toml", 2_000);
        home.file(".grok/auth.json", 500);
        home.file(".grok/sessions/a/session.jsonl", 40_000);
        home.file(".grok/downloads/grok-1.0.41-macos-aarch64", 900_000);
        let warnings = kept_data(&home.0, "grok-build", &[], BUDGET);
        match &warnings[..] {
            [Warning::KeepsData {
                path,
                what,
                size,
                left_out,
                others,
            }] => {
                assert_eq!(path, "~/.grok");
                assert_eq!(*what, KeptData::ToolData);
                assert_eq!(left_out, &vec!["~/.grok/downloads".to_string()]);
                assert!(others.is_empty(), "{others:?}");
                let bytes = size.expect("measured").bytes;
                assert!((42_500..900_000).contains(&bytes), "{bytes}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_grok_builds_own_uninstall_names_its_folder_once_where_it_keeps_it() {
        // Banager's `grok` recipe keeps `~/.grok` and says so
        // (`Warning::WillKeep`): the line kept_data would add is not said
        // twice.
        use crate::adapters::standalone::recipe::Uninstall;
        use crate::adapters::standalone::recipes::GROK;
        let Some(Uninstall::Paths { keep, .. }) = &GROK.uninstall else {
            panic!("grok's recipe uninstalls by paths");
        };
        let plan: Vec<Warning> = keep
            .iter()
            .map(|spec| Warning::WillKeep {
                path: spec.path.to_string(),
                what: spec.what,
            })
            .collect();
        let home = Home::new("grok-own");
        home.file(".grok/config.toml", 2_000);
        assert!(kept_data(&home.0, "grok-build", &named_paths(&plan), BUDGET).is_empty());
        assert_eq!(kept_data(&home.0, "grok-build", &[], BUDGET).len(), 1);
    }

    #[test]
    fn test_qoder_and_copilot_leave_their_programs_copies_out_of_what_stays() {
        let home = Home::new("qoder");
        home.file(".qoder/settings.json", 1_000);
        home.file(".qoder/projects/a/session.jsonl", 30_000);
        home.file(".qoder/bin/qodercli/qodercli-1.1.65", 800_000);
        match &kept_data(&home.0, "qoder-cli", &[], BUDGET)[..] {
            [Warning::KeepsData {
                path,
                size,
                left_out,
                ..
            }] => {
                assert_eq!(path, "~/.qoder");
                assert_eq!(left_out, &vec!["~/.qoder/bin/qodercli".to_string()]);
                let bytes = size.expect("measured").bytes;
                assert!((31_000..800_000).contains(&bytes), "{bytes}");
            }
            other => panic!("{other:?}"),
        }

        let home = Home::new("copilot");
        home.file(".copilot/settings.json", 1_000);
        home.file(".copilot/session-state/a/events.jsonl", 20_000);
        home.file(".copilot/pkg/universal/1.0.91/index.js", 700_000);
        match &kept_data(&home.0, "copilot-cli", &[], BUDGET)[..] {
            [Warning::KeepsData {
                path,
                size,
                left_out,
                ..
            }] => {
                assert_eq!(path, "~/.copilot");
                assert_eq!(left_out, &vec!["~/.copilot/pkg".to_string()]);
                let bytes = size.expect("measured").bytes;
                assert!((21_000..700_000).contains(&bytes), "{bytes}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_aider_names_its_folder_and_the_settings_files_that_are_there_in_the_tables_order() {
        let home = Home::new("aider");
        home.file(".aider/analytics.json", 300);
        home.file(".aider/caches/model_prices.json", 50_000);
        home.file(".aider.conf.yml", 400);
        // No model settings or metadata file: no line for them.
        let warnings = kept_data(&home.0, "aider", &[], BUDGET);
        assert_eq!(
            paths_of(&warnings),
            vec!["~/.aider".to_string(), "~/.aider.conf.yml".to_string()]
        );
        assert!(size_of(&warnings[0]).unwrap().bytes >= 50_300);
        assert!(size_of(&warnings[1]).unwrap().bytes >= 400);
    }

    #[test]
    fn test_kimi_code_names_its_folder_and_the_older_kimi_clis() {
        let home = Home::new("kimi");
        home.file(".kimi-code/config.toml", 500);
        home.file(".kimi-code/sessions/wd_a/s1/state.json", 3_000);
        home.file(".kimi/config.json", 400);
        assert_eq!(
            paths_of(&kept_data(&home.0, "kimi-code", &[], BUDGET)),
            vec!["~/.kimi-code".to_string(), "~/.kimi".to_string()]
        );
    }

    #[test]
    fn test_cursor_cli_names_only_its_own_file_in_the_editors_folder() {
        // `~/.cursor` is the Cursor editor's too: its extensions are not
        // the CLI's data, and are neither named nor counted.
        let home = Home::new("cursor");
        home.file(".cursor/extensions/some.ext/big.bin", 900_000);
        home.file(".cursor/cli-config.json", 700);
        let warnings = kept_data(&home.0, "cursor-cli", &[], BUDGET);
        assert_eq!(
            paths_of(&warnings),
            vec!["~/.cursor/cli-config.json".to_string()]
        );
        let bytes = size_of(&warnings[0]).unwrap().bytes;
        assert!(bytes < 100_000, "{bytes}");
    }
}
