//! Which AI coding tool an installed artifact is a copy of.
//!
//! The table is `data/ai-tools.json`, bundled into the binary with
//! `include_str!` -- Banager reads no file and asks no server for it. Each
//! family (Claude Code, Codex, Gemini CLI, …) lists its members: where it
//! can be installed from and the exact name it has there (an npm package,
//! a Homebrew formula or cask, a PyPI project, or one of Banager's own
//! standalone recipes), the command names it provides, and the folders it
//! keeps its data in where that is verified.
//!
//! # How the table was verified (2026-10-01)
//!
//! Every name was looked up on that day, read-only, from the registry that
//! owns it (research synthesis, appendix B; critique §2 item 1):
//!
//! - npm: `registry.npmjs.org/<name>` -- the `bin` field gives the command
//!   names. For the three *unscoped* names, where a scope does not already
//!   say who publishes them, the publisher was checked too: `droid`
//!   (maintainers `@factory.ai`, repository `Factory-AI/factory`, the same
//!   as `@factory/cli`), `openclaw` (repository `openclaw/openclaw`, author
//!   "OpenClaw Foundation", published from GitHub Actions) and
//!   `opencode-ai` (no repository field, but the opencode project's own
//!   README installs it with `npm i -g opencode-ai`, and Homebrew's
//!   `opencode` formula builds from the same project). None was dropped.
//! - Homebrew: `formulae.brew.sh/api/{formula,cask}/<name>.json`; command
//!   names from the formula's install/test block or the cask's `binary`
//!   stanza. A name that 404'd there (formula `codex`, formula
//!   `claude-code`, cask `ollama`) is deliberately absent: the cask
//!   `codexbar` or a tap's `codex` is someone else's.
//! - PyPI: `pypi.org/pypi/<name>/json` (project URLs name the vendor's
//!   repository); command names from that repository's `pyproject.toml`
//!   `[project.scripts]`.
//! - Standalone: the recipe ids in `adapters/standalone/recipes.rs`.
//! - Data folders: the ones the vendors document (Claude Code, Codex,
//!   Gemini CLI, Qwen Code, and opencode, below), and Antigravity CLI's
//!   `~/.gemini/antigravity-cli` -- named by the Homebrew cask's `zap`
//!   (the only vendor-side source; it trashes only that folder), and seen
//!   in this Mac's directory listing and the 1.2.11 recording's README;
//!   the install script does not name it. Banager's own recipe keeps it on
//!   uninstall (`recipes::AGY`), a keep list agy.md calls a synthesis, not
//!   a vendor list. It sits inside Gemini CLI's `~/.gemini`, so Gemini
//!   CLI's line leaves it out (`kept_data::others_inside`). And opencode's
//!   two (added 2026-10-01): `~/.local/share/opencode` (sessions, the
//!   `auth.json` of its logins, logs) and `~/.config/opencode` (its global
//!   settings), from its own docs, read as text (opencode.ai/docs/
//!   troubleshooting, "Storage"; opencode.ai/docs/config, "Global"),
//!   matching its source (`packages/core/src/global.ts`: `xdgData` and
//!   `xdgConfig` joined with `opencode`) and its own `opencode uninstall`,
//!   which keeps exactly these two on `--keep-data` / `--keep-config`
//!   and always removes its cache and state folders (not listed: not the
//!   user's data). Those are the defaults: Banager does not read the
//!   `XDG_DATA_HOME` or `XDG_CONFIG_HOME` a shell may set (a Finder
//!   launch has only the shell's `PATH`), so data moved elsewhere is not
//!   named. Nothing of them is inside the
//!   folder its own install uses (`~/.opencode`, `recipes::OPENCODE`),
//!   whose row Banager never uninstalls. The rest are below.
//!
//! # Data folders added 2026-10-02
//!
//! Each from that vendor's own docs or source, read as text and never run:
//! a docs page fetched with `curl`, a file of its public repository read
//! through the GitHub API, or the files of its published npm or PyPI
//! package, unpacked and read. Every one is the default: Banager inherits
//! only the shell's `PATH`, so the variable a tool lets a shell move its
//! folder with (named below) is never read, and data moved that way is not
//! named.
//!
//! - Kimi Code: `~/.kimi-code`, where `@moonshot-ai/kimi-code` keeps its
//!   settings, sessions, logins and logs (MoonshotAI/kimi-code,
//!   `docs/en/configuration/data-locations.md`; `apps/kimi-code/src/
//!   utils/paths.ts`, `getDataDir`; `KIMI_CODE_HOME`), and `~/.kimi`, the
//!   older Python Kimi CLI's (`kimi-cli` on PyPI and Homebrew:
//!   `kimi_cli/share.py`, `Path.home() / ".kimi"`, in the published
//!   1.52.0 wheel; `KIMI_SHARE_DIR`), which Kimi Code's migration guide
//!   (moonshotai.github.io/kimi-code/en/guides/migration) says it never
//!   modifies or deletes.
//! - iFlow CLI: `~/.iflow` (the published 0.5.19 bundle,
//!   `bundle/iflow.js`: `homedir()` joined with `.iflow` unless
//!   `IFLOW_HOME`; its README: `~/.iflow/settings.json`).
//! - CodeBuddy Code: `~/.codebuddy` (the docs it ships, `dist/web-ui/docs/
//!   en/cli/installation.md`, "Configuration Directory", and
//!   `codebuddy-dir.md`: settings, sessions, history, logs;
//!   `CODEBUDDY_CONFIG_DIR`). Not named: `~/.local/share/codebuddy`, which
//!   its bundle uses for its own installer's program versions.
//! - Qoder CLI: `~/.qoder` (docs.qoder.com/cli/settings, /cli/config-scope
//!   and /cli/installation: settings, login, plugins, sessions and
//!   memories; `QODER_CONFIG_DIR`). Its install script's copy keeps its versioned
//!   programs in `~/.qoder/bin/qodercli` (the published 1.1.65 bundle,
//!   `bundle/qodercli.js`, decoded: `join(homedir(), ".qoder", "bin",
//!   "qodercli")`), left out of the size (`kept_data::LEFT_OUT`).
//! - Crush: `~/.local/share/crush` and `~/.config/crush` (charmbracelet/
//!   crush, `internal/config/load.go`, `GlobalConfigData` and
//!   `GlobalConfig`, with `internal/home/home.go`; its README;
//!   `CRUSH_GLOBAL_DATA`, `CRUSH_GLOBAL_CONFIG`, `XDG_*`). Not named:
//!   `~/.cache/crush`, a cache, and the sessions it keeps in each
//!   project's own `.crush`.
//! - Amp: `~/.config/amp` (ampcode.com/docs/cli/settings: "macOS:
//!   ~/.config/amp/settings.json"; skills and plugins there too). Not
//!   named: `~/.local/share/amp`, which its program defines but no doc
//!   says what it holds.
//! - Kilo: `~/.local/share/kilo` and `~/.config/kilo` (Kilo-Org/kilocode,
//!   `packages/core/src/global.ts`: `xdgData` and `xdgConfig` joined with
//!   `kilo`, as opencode's; kilo.ai/docs/cli: config in
//!   `~/.config/kilo/`). Not named: its cache and state folders, as
//!   opencode's.
//! - GitHub Copilot CLI: `~/.copilot` (docs.github.com, "GitHub Copilot
//!   CLI configuration directory": configuration, session history, logs
//!   and customizations; `COPILOT_HOME`). Its updater's copies of the
//!   program in `~/.copilot/pkg` (github/copilot-cli `changelog.md`,
//!   0.0.421: "Use consistent ~/.copilot/pkg path for auto-update") are
//!   left out of the size.
//! - Auggie: `~/.augment` (docs.augmentcode.com/cli/config:
//!   `~/.augment/settings.json`; the published 0.36.0 `augment.mjs`:
//!   sessions in `~/.augment/sessions`).
//! - Factory Droid: `~/.factory` (docs.factory.ai/cli/configuration/
//!   settings: `~/.factory/settings.json`, specs and worktrees there).
//! - Cursor CLI: the file `~/.cursor/cli-config.json` only
//!   (cursor.com/docs/cli/reference/configuration). `~/.cursor` itself is
//!   the Cursor editor's folder too, and no Cursor doc names the CLI's
//!   other files; Homebrew's `zap` for the cask names three more folders,
//!   a list Homebrew's, not Cursor's.
//! - Aider: `~/.aider` (the published 0.86.2 wheel: `oauth-keys.env` and
//!   `installs.json` in `aider/main.py`, `analytics.json` in
//!   `aider/analytics.py`, `caches` in `aider/models.py`) and the three
//!   settings files its docs say it reads from the home folder,
//!   `~/.aider.conf.yml`, `~/.aider.model.settings.yml` and
//!   `~/.aider.model.metadata.json` (aider.chat/docs/config/aider_conf.html
//!   and adv-model-settings.html). Not named: the chat and input history
//!   it keeps in each git repository.
//! - Goose: `~/.local/share/goose` (sessions) and `~/.config/goose`
//!   (settings and command history) (goose-docs.ai/docs/guides/logs and
//!   config-files; aaif-goose/goose `crates/goose/src/config/paths.rs`
//!   with etcetera 0.11, whose `choose_app_strategy` is XDG on macOS;
//!   `GOOSE_PATH_ROOT`). Not named: `~/.local/state/goose`, logs it
//!   deletes itself after two weeks.
//! - Mistral Vibe: `~/.vibe` (the published 2.25.8 wheel,
//!   `vibe/utils/paths.py`: `Path.home() / ".vibe"` unless `VIBE_HOME`;
//!   its README: `config.toml`, the `.env` holding its key, skills).
//! - OpenClaw: `~/.openclaw`, and `~/.clawdbot`, the older name it still
//!   uses when `~/.openclaw` is not there (openclaw/openclaw,
//!   `src/config/state-dir.ts`; docs.openclaw.ai/install/uninstall:
//!   "Delete state + config: rm -rf ... $HOME/.openclaw";
//!   `OPENCLAW_STATE_DIR`). Its own `install-cli.sh` installs a Node and
//!   a copy of OpenClaw under `~/.openclaw/tools` and `~/.openclaw/bin`
//!   in a folder named after the Node version, which a fixed name in
//!   `kept_data::LEFT_OUT` cannot match, and `tools/` also holds what its
//!   skills download: the size counts both, and docs/what-we-run.md says
//!   so.
//! - Grok Build: `~/.grok` (xai-org/grok-build, `crates/codegen/
//!   xai-grok-shell/README.md`, "File Locations", and
//!   `crates/codegen/xai-dirs/src/lib.rs`; `GROK_HOME`), which Banager's
//!   own recipe keeps too (`recipes::GROK`), so its preview names it
//!   once, there. Its install script's program in `~/.grok/downloads`
//!   (x.ai/cli/install.sh, read as text: `DOWNLOAD_DIR`) is left out of
//!   the size when Homebrew's cask `grok-build` is the one uninstalled.
//!
//! Ollama has none in the table: its models folder is named by
//! `kept_data::OLLAMA_MODELS`, and the rest of `~/.ollama` is not named.
//!
//! Versions are deliberately not in the table: they change weekly.
//!
//! Matching is by source kind (`family_for`): an npm package name only on
//! an npm instance, a formula name only against formulae and a cask name
//! only against casks, a PyPI name on pipx, uv and pip (normalised as PEP
//! 503 does, so `Aider_Chat` is `aider-chat`), and a recipe id only on that
//! recipe's own standalone source. Ollama *models* are not members of
//! anything; the formula `ollama` and the cask `ollama-app` are.
//!
//! The family is set once per refresh round, where the snapshot is put
//! together (`assign`, called from `session/refresh.rs`), not by each
//! adapter. The front end only reads `ArtifactFacts.family`.

use crate::model::{ArtifactKey, ArtifactKind, InstalledArtifact, ManagerInstance};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Where a member is installed from, which decides which sources it can
/// match on (`family_for`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberSource {
    /// An npm package, on an npm instance.
    Npm,
    /// A Homebrew formula (never a cask of the same name).
    Formula,
    /// A Homebrew cask (never a formula of the same name).
    Cask,
    /// A PyPI project, on pipx, uv or pip.
    Pypi,
    /// One of Banager's standalone recipes, by its id (`claude`, `agy`,
    /// `grok`, `codex`, `opencode`), on that recipe's own source.
    Standalone,
}

/// One way a family can be installed: where from, and its exact name there.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub source: MemberSource,
    pub name: String,
}

/// One AI coding tool, whichever way it was installed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    /// What `ArtifactFacts.family` carries: kebab-case, stable.
    pub id: String,
    pub name_en: String,
    pub name_zh: String,
    pub members: Vec<Member>,
    /// The command names it puts on PATH, for working out which copy a
    /// typed name runs.
    pub commands: Vec<String>,
    /// Folders (or files) it keeps the user's data in, `~/`-relative, for
    /// saying what an uninstall leaves behind. Empty where unverified.
    pub data_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    /// The day every name in `families` was checked.
    verified: String,
    /// How they were checked, in a sentence.
    #[allow(dead_code)]
    method: String,
    families: Vec<Family>,
}

const TABLE_JSON: &str = include_str!("../data/ai-tools.json");

static TABLE: LazyLock<Table> =
    LazyLock::new(|| serde_json::from_str(TABLE_JSON).expect("data/ai-tools.json is valid"));

/// `(source, normalised name)` → index into `TABLE.families`.
static INDEX: LazyLock<HashMap<(MemberSource, String), usize>> = LazyLock::new(|| {
    let mut index = HashMap::new();
    for (i, family) in TABLE.families.iter().enumerate() {
        for member in &family.members {
            index.insert((member.source, normalise(member.source, &member.name)), i);
        }
    }
    index
});

/// Every family in the bundled table, in its order.
pub fn families() -> &'static [Family] {
    &TABLE.families
}

/// The day the bundled table was verified (`YYYY-MM-DD`).
pub fn verified_on() -> &'static str {
    &TABLE.verified
}

/// A family by its id.
pub fn family(id: &str) -> Option<&'static Family> {
    TABLE.families.iter().find(|f| f.id == id)
}

/// PyPI names compare as PEP 503 normalises them: lower case, every run of
/// `-`, `_` and `.` one `-`. Every other source compares exactly.
fn normalise(source: MemberSource, name: &str) -> String {
    match source {
        MemberSource::Pypi => {
            let mut out = String::with_capacity(name.len());
            let mut in_run = false;
            for c in name.chars() {
                if matches!(c, '-' | '_' | '.') {
                    if !in_run {
                        out.push('-');
                    }
                    in_run = true;
                } else {
                    out.extend(c.to_lowercase());
                    in_run = false;
                }
            }
            out
        }
        _ => name.to_string(),
    }
}

/// Which member source an artifact of `adapter_id` with this key would be,
/// and the name to look it up by -- or `None` where no family can match
/// (Cargo, Ollama models, rustup, a kind the source does not list).
fn lookup_key(adapter_id: &str, key: &ArtifactKey) -> Option<(MemberSource, String)> {
    let source = match (adapter_id, key.kind) {
        ("npm", ArtifactKind::Package) => MemberSource::Npm,
        ("brew", ArtifactKind::Formula) => MemberSource::Formula,
        ("brew", ArtifactKind::Cask) => MemberSource::Cask,
        ("pipx" | "uv", ArtifactKind::Tool) | ("pip", ArtifactKind::Package) => MemberSource::Pypi,
        (adapter, ArtifactKind::Binary) => {
            // A recipe's own source only: `standalone-claude` lists
            // `claude`, and only that is Claude Code's recipe.
            let recipe = adapter.strip_prefix("standalone-")?;
            if recipe != key.name {
                return None;
            }
            MemberSource::Standalone
        }
        _ => return None,
    };
    Some((source, normalise(source, &key.name)))
}

/// The family an artifact of a source with `adapter_id` belongs to, if any.
pub fn family_for(adapter_id: &str, key: &ArtifactKey) -> Option<&'static Family> {
    let lookup = lookup_key(adapter_id, key)?;
    INDEX.get(&lookup).map(|&i| &TABLE.families[i])
}

/// Sets `facts.family` on every artifact from the table, by its instance's
/// adapter -- and clears it on any that matches nothing, so the answer is
/// this table's whatever an artifact carried in. An artifact whose instance
/// is not in `instances` gets none. Called once per round, where the
/// snapshot is assembled.
pub fn assign(instances: &[ManagerInstance], artifacts: &mut [InstalledArtifact]) {
    let adapters: HashMap<&str, &str> = instances
        .iter()
        .map(|i| (i.id.as_str(), i.adapter_id.as_str()))
        .collect();
    for artifact in artifacts {
        artifact.facts.family = adapters
            .get(artifact.key.instance_id.as_str())
            .and_then(|adapter| family_for(adapter, &artifact.key))
            .map(|f| f.id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtifactFacts, InstallReason, InstanceStatus, Scope};
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn key(instance_id: &str, kind: ArtifactKind, name: &str) -> ArtifactKey {
        ArtifactKey {
            instance_id: instance_id.to_string(),
            kind,
            name: name.to_string(),
        }
    }

    fn id_for(adapter: &str, kind: ArtifactKind, name: &str) -> Option<&'static str> {
        family_for(adapter, &key("x", kind, name)).map(|f| f.id.as_str())
    }

    fn instance(adapter_id: &str, id: &str) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: adapter_id.to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: None,
            unverified_version: None,
            read_only_reason: None,
            status: InstanceStatus::default(),
        }
    }

    fn artifact(instance_id: &str, kind: ArtifactKind, name: &str) -> InstalledArtifact {
        InstalledArtifact {
            key: key(instance_id, kind, name),
            display_name: name.to_string(),
            version: "1.0.0".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
            facts: ArtifactFacts::default(),
        }
    }

    /// The names research appendix B (2026-10-01) verified, by source.
    /// Appendix B is not in this repository, so this list is its copy: a
    /// member added to the table without being verified and added here
    /// fails `test_every_member_is_one_appendix_b_verified`.
    const APPENDIX_B: &[(MemberSource, &str)] = &[
        (MemberSource::Npm, "@anthropic-ai/claude-code"),
        (MemberSource::Cask, "claude-code"),
        (MemberSource::Standalone, "claude"),
        (MemberSource::Npm, "@openai/codex"),
        (MemberSource::Cask, "codex"),
        // Not in appendix B: Codex's own install, from its install script
        // read as text on 2026-10-01 (research S §3f; recipes::CODEX).
        (MemberSource::Standalone, "codex"),
        (MemberSource::Npm, "@google/gemini-cli"),
        (MemberSource::Formula, "gemini-cli"),
        (MemberSource::Npm, "@qwen-code/qwen-code"),
        (MemberSource::Formula, "qwen-code"),
        (MemberSource::Npm, "@moonshot-ai/kimi-code"),
        (MemberSource::Formula, "kimi-cli"),
        (MemberSource::Pypi, "kimi-cli"),
        (MemberSource::Npm, "@iflow-ai/iflow-cli"),
        (MemberSource::Npm, "@tencent-ai/codebuddy-code"),
        (MemberSource::Npm, "@qoder-ai/qodercli"),
        (MemberSource::Npm, "opencode-ai"),
        (MemberSource::Formula, "opencode"),
        // Not in appendix B: opencode's own install, from its install
        // script read as text on 2026-10-01 (recipes::OPENCODE).
        (MemberSource::Standalone, "opencode"),
        (MemberSource::Npm, "@charmland/crush"),
        (MemberSource::Npm, "@ampcode/cli"),
        (MemberSource::Npm, "@sourcegraph/amp"),
        (MemberSource::Npm, "@kilocode/cli"),
        (MemberSource::Npm, "@github/copilot"),
        (MemberSource::Cask, "copilot-cli"),
        (MemberSource::Npm, "@augmentcode/auggie"),
        (MemberSource::Npm, "droid"),
        (MemberSource::Npm, "@factory/cli"),
        (MemberSource::Cask, "cursor-cli"),
        (MemberSource::Formula, "aider"),
        (MemberSource::Pypi, "aider-chat"),
        (MemberSource::Formula, "block-goose-cli"),
        (MemberSource::Pypi, "mistral-vibe"),
        (MemberSource::Npm, "openclaw"),
        (MemberSource::Formula, "ollama"),
        (MemberSource::Cask, "ollama-app"),
        (MemberSource::Standalone, "agy"),
        (MemberSource::Cask, "grok-build"),
        (MemberSource::Standalone, "grok"),
    ];

    #[test]
    fn test_table_parses_and_says_when_it_was_verified() {
        assert_eq!(verified_on(), "2026-10-01");
        assert_eq!(families().len(), 23);
    }

    #[test]
    fn test_every_family_has_a_unique_kebab_case_id_names_a_member_and_a_command() {
        let mut ids = HashSet::new();
        for f in families() {
            assert!(ids.insert(f.id.as_str()), "duplicate id {}", f.id);
            assert!(
                !f.id.is_empty()
                    && f.id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "id {:?} is not kebab-case",
                f.id
            );
            assert!(
                !f.name_en.trim().is_empty() && !f.name_zh.trim().is_empty(),
                "{}",
                f.id
            );
            assert!(!f.members.is_empty(), "{} has no member", f.id);
            assert!(!f.commands.is_empty(), "{} has no command", f.id);
            let commands: HashSet<_> = f.commands.iter().collect();
            assert_eq!(
                commands.len(),
                f.commands.len(),
                "{} repeats a command",
                f.id
            );
            for c in &f.commands {
                assert!(
                    !c.is_empty() && !c.contains('/') && !c.contains(char::is_whitespace),
                    "{}: command {c:?} is not a bare name",
                    f.id
                );
            }
        }
    }

    #[test]
    fn test_no_member_is_listed_twice_or_in_two_families() {
        let mut seen: HashMap<(MemberSource, String), &str> = HashMap::new();
        for f in families() {
            for m in &f.members {
                let k = (m.source, normalise(m.source, &m.name));
                if let Some(other) = seen.insert(k.clone(), &f.id) {
                    panic!("{k:?} is in both {other} and {}", f.id);
                }
            }
        }
    }

    #[test]
    fn test_every_member_is_one_appendix_b_verified() {
        let verified: HashSet<(MemberSource, &str)> = APPENDIX_B.iter().copied().collect();
        let mut listed = HashSet::new();
        for f in families() {
            for m in &f.members {
                assert!(
                    verified.contains(&(m.source, m.name.as_str())),
                    "{}: {:?} {} is not in appendix B",
                    f.id,
                    m.source,
                    m.name
                );
                listed.insert((m.source, m.name.as_str()));
            }
        }
        // And nothing verified was left out of the table by mistake.
        assert_eq!(listed, verified);
    }

    #[test]
    fn test_data_paths_are_the_verified_ones_and_under_home() {
        // Every family's, as the vendors' own docs or source name them
        // (module doc): a family added without its folders, or a folder
        // changed without its evidence in the module doc, fails here.
        let expected: &[(&str, &[&str])] = &[
            ("claude-code", &["~/.claude", "~/.claude.json"]),
            ("codex", &["~/.codex"]),
            ("gemini-cli", &["~/.gemini"]),
            ("qwen-code", &["~/.qwen"]),
            ("kimi-code", &["~/.kimi-code", "~/.kimi"]),
            ("iflow-cli", &["~/.iflow"]),
            ("codebuddy-code", &["~/.codebuddy"]),
            ("qoder-cli", &["~/.qoder"]),
            (
                "opencode",
                &["~/.local/share/opencode", "~/.config/opencode"],
            ),
            ("crush", &["~/.local/share/crush", "~/.config/crush"]),
            ("amp", &["~/.config/amp"]),
            ("kilo", &["~/.local/share/kilo", "~/.config/kilo"]),
            ("copilot-cli", &["~/.copilot"]),
            ("auggie", &["~/.augment"]),
            ("droid", &["~/.factory"]),
            ("cursor-cli", &["~/.cursor/cli-config.json"]),
            (
                "aider",
                &[
                    "~/.aider",
                    "~/.aider.conf.yml",
                    "~/.aider.model.settings.yml",
                    "~/.aider.model.metadata.json",
                ],
            ),
            ("goose", &["~/.local/share/goose", "~/.config/goose"]),
            ("mistral-vibe", &["~/.vibe"]),
            ("openclaw", &["~/.openclaw", "~/.clawdbot"]),
            // Ollama's models folder is kept_data's, not the table's.
            ("ollama", &[]),
            ("antigravity-cli", &["~/.gemini/antigravity-cli"]),
            ("grok-build", &["~/.grok"]),
        ];
        let listed: Vec<(&str, Vec<&str>)> = families()
            .iter()
            .map(|f| {
                (
                    f.id.as_str(),
                    f.data_paths.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        let expected: Vec<(&str, Vec<&str>)> = expected
            .iter()
            .map(|(id, paths)| (*id, paths.to_vec()))
            .collect();
        assert_eq!(listed, expected);
        for f in families() {
            let mut seen = HashSet::new();
            for p in &f.data_paths {
                assert!(p.starts_with("~/") && !p.contains(".."), "{}: {p}", f.id);
                assert!(!p.ends_with('/'), "{}: {p} ends with a slash", f.id);
                assert!(seen.insert(p.as_str()), "{} repeats {p}", f.id);
            }
        }
    }

    #[test]
    fn test_no_data_path_is_a_folder_many_programs_share() {
        // A tool's own folder or file, never the shared folder it sits in:
        // `~/.cursor` is the Cursor editor's too, so Cursor CLI names only
        // its own file there, and the XDG folders hold every tool's.
        const SHARED: &[&str] = &[
            "~/.config",
            "~/.local",
            "~/.local/share",
            "~/.local/state",
            "~/.local/bin",
            "~/.cache",
            "~/.cursor",
            "~/.agents",
            "~/Library",
        ];
        for f in families() {
            for p in &f.data_paths {
                assert!(!SHARED.contains(&p.as_str()), "{}: {p}", f.id);
                assert!(!p.starts_with("~/Library/"), "{}: {p}", f.id);
            }
        }
    }

    #[test]
    fn test_the_table_carries_no_version_numbers() {
        // Versions change weekly; the table is ids, names and commands.
        let raw: serde_json::Value = serde_json::from_str(TABLE_JSON).unwrap();
        fn walk(v: &serde_json::Value, path: &str) {
            match v {
                serde_json::Value::Object(map) => {
                    for (k, child) in map {
                        assert_ne!(k, "version", "{path}.{k}");
                        walk(child, &format!("{path}.{k}"));
                    }
                }
                serde_json::Value::Array(items) => {
                    for (i, child) in items.iter().enumerate() {
                        walk(child, &format!("{path}[{i}]"));
                    }
                }
                _ => {}
            }
        }
        walk(&raw, "");
        assert!(!TABLE_JSON.contains("@latest"));
    }

    #[test]
    fn test_matches_npm_names_only_on_npm() {
        assert_eq!(
            id_for("npm", ArtifactKind::Package, "@openai/codex"),
            Some("codex")
        );
        assert_eq!(id_for("npm", ArtifactKind::Package, "droid"), Some("droid"));
        assert_eq!(
            id_for("npm", ArtifactKind::Package, "@factory/cli"),
            Some("droid")
        );
        assert_eq!(
            id_for("npm", ArtifactKind::Package, "opencode-ai"),
            Some("opencode")
        );
        // An unscoped look-alike is not the vendor's.
        assert_eq!(id_for("npm", ArtifactKind::Package, "codex"), None);
        assert_eq!(id_for("npm", ArtifactKind::Package, "claude-code"), None);
        // The same name from another source is not an npm package.
        assert_eq!(id_for("brew", ArtifactKind::Formula, "@openai/codex"), None);
        assert_eq!(id_for("pipx", ArtifactKind::Tool, "openclaw"), None);
    }

    #[test]
    fn test_tells_a_formula_from_a_cask_of_the_same_name() {
        assert_eq!(id_for("brew", ArtifactKind::Cask, "codex"), Some("codex"));
        assert_eq!(id_for("brew", ArtifactKind::Formula, "codex"), None);
        assert_eq!(
            id_for("brew", ArtifactKind::Formula, "gemini-cli"),
            Some("gemini-cli")
        );
        assert_eq!(id_for("brew", ArtifactKind::Cask, "gemini-cli"), None);
        assert_eq!(
            id_for("brew", ArtifactKind::Formula, "ollama"),
            Some("ollama")
        );
        assert_eq!(
            id_for("brew", ArtifactKind::Cask, "ollama-app"),
            Some("ollama")
        );
        assert_eq!(id_for("brew", ArtifactKind::Cask, "ollama"), None);
        assert_eq!(
            id_for("brew", ArtifactKind::Cask, "grok-build"),
            Some("grok-build")
        );
        // Homebrew's `goose` is a database migration tool; Block's is
        // `block-goose-cli`. A tap's cask of the same token is someone
        // else's, and so is CodexBar.
        assert_eq!(id_for("brew", ArtifactKind::Formula, "goose"), None);
        assert_eq!(
            id_for("brew", ArtifactKind::Formula, "block-goose-cli"),
            Some("goose")
        );
        assert_eq!(id_for("brew", ArtifactKind::Cask, "codexbar"), None);
        assert_eq!(
            id_for("brew", ArtifactKind::Cask, "someone/tap/codex"),
            None
        );
        assert_eq!(id_for("brew", ArtifactKind::Formula, "grok"), None);
    }

    #[test]
    fn test_matches_pypi_names_on_pipx_uv_and_pip_as_pep_503_normalises_them() {
        assert_eq!(
            id_for("pipx", ArtifactKind::Tool, "aider-chat"),
            Some("aider")
        );
        assert_eq!(
            id_for("uv", ArtifactKind::Tool, "Aider_Chat"),
            Some("aider")
        );
        assert_eq!(
            id_for("pip", ArtifactKind::Package, "aider.chat"),
            Some("aider")
        );
        assert_eq!(
            id_for("uv", ArtifactKind::Tool, "mistral-vibe"),
            Some("mistral-vibe")
        );
        assert_eq!(
            id_for("pipx", ArtifactKind::Tool, "kimi-cli"),
            Some("kimi-code")
        );
        assert_eq!(id_for("pipx", ArtifactKind::Tool, "aider"), None);
        // A kind the source does not list is not looked up.
        assert_eq!(id_for("pipx", ArtifactKind::Package, "aider-chat"), None);
    }

    #[test]
    fn test_matches_a_recipe_only_on_its_own_standalone_source() {
        assert_eq!(
            id_for("standalone-claude", ArtifactKind::Binary, "claude"),
            Some("claude-code")
        );
        assert_eq!(
            id_for("standalone-agy", ArtifactKind::Binary, "agy"),
            Some("antigravity-cli")
        );
        assert_eq!(
            id_for("standalone-grok", ArtifactKind::Binary, "grok"),
            Some("grok-build")
        );
        assert_eq!(
            id_for("standalone-rustup", ArtifactKind::Binary, "rustup"),
            None
        );
        assert_eq!(
            id_for("standalone-agy", ArtifactKind::Binary, "claude"),
            None
        );
        // Codex's own install joins npm's `@openai/codex` and the cask
        // `codex` in one family; a `codex` on another recipe's source does
        // not.
        assert_eq!(
            id_for("standalone-codex", ArtifactKind::Binary, "codex"),
            Some("codex")
        );
        assert_eq!(
            id_for("standalone-claude", ArtifactKind::Binary, "codex"),
            None
        );
        // opencode's own install joins npm's `opencode-ai` and the formula
        // `opencode`.
        assert_eq!(
            id_for("standalone-opencode", ArtifactKind::Binary, "opencode"),
            Some("opencode")
        );
        // Cargo's binaries and Ollama's models belong to no family.
        assert_eq!(id_for("cargo", ArtifactKind::Binary, "claude"), None);
        assert_eq!(id_for("ollama", ArtifactKind::Model, "ollama"), None);
        assert_eq!(id_for("ollama", ArtifactKind::Model, "qwen3:8b"), None);
    }

    #[test]
    fn test_assign_sets_and_clears_by_each_artifacts_instance() {
        let instances = vec![
            instance("npm", "npm:/opt/homebrew"),
            instance("brew", "brew:/opt/homebrew"),
            instance("ollama", "ollama:http://127.0.0.1:11434"),
            instance("standalone-claude", "standalone-claude"),
        ];
        let mut stale = artifact("npm:/opt/homebrew", ArtifactKind::Package, "typescript");
        stale.facts.family = Some("codex".to_string());
        let mut artifacts = vec![
            artifact("npm:/opt/homebrew", ArtifactKind::Package, "@openai/codex"),
            stale,
            artifact("brew:/opt/homebrew", ArtifactKind::Formula, "ollama"),
            artifact(
                "ollama:http://127.0.0.1:11434",
                ArtifactKind::Model,
                "llama3.2:3b",
            ),
            artifact("standalone-claude", ArtifactKind::Binary, "claude"),
            // An instance this round does not know: nothing to go by.
            artifact("npm:/usr/local", ArtifactKind::Package, "@openai/codex"),
        ];
        assign(&instances, &mut artifacts);
        let families: Vec<_> = artifacts
            .iter()
            .map(|a| a.facts.family.as_deref())
            .collect();
        assert_eq!(
            families,
            [
                Some("codex"),
                None,
                Some("ollama"),
                None,
                Some("claude-code"),
                None
            ]
        );
    }

    /// The author's recorded `npm ls -g --json`, which lists `@openai/codex`
    /// among five other global packages.
    #[test]
    fn test_assigns_codex_and_nothing_else_on_the_recorded_npm_inventory() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        let id = "npm:/opt/homebrew";
        let mut artifacts = crate::adapters::npm::parse_ls_global(&json, id).expect("parse");
        assert!(artifacts.len() > 1);
        assign(&[instance("npm", id)], &mut artifacts);
        let tagged: Vec<_> = artifacts
            .iter()
            .filter_map(|a| Some((a.key.name.as_str(), a.facts.family.as_deref()?)))
            .collect();
        assert_eq!(tagged, [("@openai/codex", "codex")]);
    }

    /// The recorded `brew info --installed --json=v2`: the formula `ollama`
    /// is Ollama; the casks `codexbar` and a tap's `claudebar` -- menu-bar
    /// apps about Codex and Claude -- are not those tools.
    #[test]
    fn test_assigns_ollama_and_nothing_else_on_the_recorded_homebrew_inventory() {
        let json =
            std::fs::read_to_string("../../adapters/fixtures/brew/7.0.3/info-installed.json")
                .expect("read adapters/fixtures/brew/7.0.3/info-installed.json");
        let id = "brew:/opt/homebrew";
        let mut artifacts =
            crate::adapters::brew::parse::parse_info_installed(&json, id).expect("parse");
        assert!(artifacts.iter().any(|a| a.key.name == "codexbar"));
        assign(&[instance("brew", id)], &mut artifacts);
        let tagged: Vec<_> = artifacts
            .iter()
            .filter_map(|a| Some((a.key.name.as_str(), a.facts.family.as_deref()?)))
            .collect();
        assert_eq!(tagged, [("ollama", "ollama")]);
    }
}
