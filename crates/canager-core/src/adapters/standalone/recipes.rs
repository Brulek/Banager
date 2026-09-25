//! The tools, as data. One `pub static` per tool, `RECIPES` listing them
//! in registration order; `StandaloneAdapter::new` builds one adapter per
//! entry (`all()`). Each tool also has a meta TOML
//! (`adapters/meta/standalone-<id>.toml`) and a recorded fixture directory
//! (`adapters/fixtures/standalone-<id>/`), and the tests below hold every
//! constant to the invariants the code relies on.

use super::recipe::{
    Expect, KeepSpec, Latest, Recipe, RemoveSpec, Route, RouteKind, Uninstall, UpgradeCmd,
    VersionCmd, VersionParse,
};
use crate::model::{CancelPolicy, KeptWhat, RemovedWhat};

/// Claude Code, the native install (`curl -fsSL https://claude.ai/install.sh
/// | bash`, run by the user; Canager never runs it).
///
/// Every value here is from `.superpowers/phase4/claude.md` (VERIFIED on
/// this Mac or in Anthropic's own documentation, 2026-09-24, unless
/// noted) and from the recording in
/// `adapters/fixtures/standalone-claude/<version>/`:
/// - the launcher `~/.local/bin/claude` is a symbolic link into
///   `~/.local/share/claude/versions/<version>`, one full executable per
///   installed version, kept after upgrades (§2a);
/// - `claude --version` prints `<version> (Claude Code)` (§1). Anthropic
///   documents that Claude Code checks for updates on startup and that
///   `DISABLE_AUTOUPDATER` stops only that background check (§5, doc
///   text); whether `--version` alone reaches the updater was not
///   observed, so the variable is set on every version read regardless,
///   while the upgrade plan adds no override (spec §3.4); the runner
///   inherits ambient environment, and manual updates still work with
///   `DISABLE_AUTOUPDATER=1` (§5);
/// - the newest published version is the channel pointer
///   `downloads.claude.ai/claude-code-releases/<latest|stable>`, a bare
///   version each; install.sh itself reads the `latest` one (§4). The
///   `stable` pointer is behind `latest` (2.1.273 vs 2.1.281 on
///   2026-09-24), which is why `check_updates` compares rather than tests
///   inequality;
/// - it updates itself in the background when its updater is on (§5);
/// - `claude update` (alias `upgrade`, no options) is the documented
///   updater (§6). The install script stages its download under
///   `~/.claude/downloads`, checks it against the release manifest's
///   checksum, and only then runs the new binary's own `install`, which
///   sets up the launcher (install.sh, read directly, §4 and §6);
///   `claude update` itself is compiled and its steps were not read
///   (§6, §8), so Canager assumes nothing about interruption:
///   `KillThenReconcile`, no claim in the preview, and stopped upgrades
///   remain `Unconfirmed` even if the version changes. After exit 0, a
///   readable version gates success. 1800 s is spec §4.1's upgrade
///   budget; the binary is about 220 MB.
///
/// There is no `claude uninstall` subcommand (§2a, `claude --help`). The
/// removal list is Anthropic's own "Uninstall Claude Code → Native"
/// instructions at code.claude.com/docs/en/setup (§7, VERIFIED: exactly
/// `rm -f ~/.local/bin/claude` and `rm -rf ~/.local/share/claude`), plus
/// `~/.claude/downloads`, the staging directory install.sh names as
/// `DOWNLOAD_DIR` for the native route's downloads (§2a, VERIFIED from
/// install.sh; optional -- it may not be there). The kept paths are the
/// same page's separate, explicitly optional step ("Removing configuration
/// files will delete all your settings…"; the VS Code extension, the
/// JetBrains plugin and the desktop app write to `~/.claude/` too, §7):
/// `~/.claude` and `~/.claude.json`, which Canager keeps (spec Q4) -- of
/// `~/.claude` it moves only `downloads`, the cache above. Order: program
/// files, cache, the launcher last (spec §6.2).
pub static CLAUDE: Recipe = Recipe {
    id: "claude",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-claude.toml"),
    route: Route {
        kind: RouteKind::SymlinkIntoRoot,
        launcher: "~/.local/bin/claude",
        root: "~/.local/share/claude",
    },
    version: VersionCmd {
        args: &["--version"],
        env: &[("DISABLE_AUTOUPDATER", "1")],
        parse: VersionParse::FirstToken,
    },
    latest: Latest::ClaudeChannel {
        base: "https://downloads.claude.ai/claude-code-releases",
    },
    self_updates: true,
    upgrade: UpgradeCmd {
        args: &["update"],
        timeout_secs: 1800,
        cancel: CancelPolicy::KillThenReconcile,
    },
    uninstall: Some(Uninstall::Paths {
        remove: &[
            RemoveSpec {
                path: "~/.local/share/claude",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: false,
            },
            RemoveSpec {
                path: "~/.claude/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Cache,
                optional: true,
            },
            RemoveSpec {
                path: "~/.local/bin/claude",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            },
        ],
        keep: &[
            KeepSpec {
                path: "~/.claude",
                what: KeptWhat::SettingsAndHistory,
            },
            KeepSpec {
                path: "~/.claude.json",
                what: KeptWhat::Settings,
            },
        ],
    }),
};

/// Every tool this adapter type registers, in registration order. The
/// refresh fans out alphabetically by adapter id regardless
/// (`refresh_round`), so this order is only the reading order.
pub static RECIPES: &[&Recipe] = &[&CLAUDE];

#[cfg(test)]
mod tests {
    use super::super::recipe::{Expect, Uninstall, SHARED_FOLDERS};
    use super::*;
    use crate::adapters::AdapterMeta;
    use crate::model::CancelPolicy;
    use crate::model::{KeptWhat, RemovedWhat};
    use std::path::Path;

    #[test]
    fn test_every_recipe_path_is_under_home() {
        // `route::expand` joins a `~/` path onto `HostEnv.home` and nothing
        // else: a recipe path that does not start that way is a programming
        // error this test turns into a red build, not a runtime surprise.
        // (`$CARGO_HOME/` joins with rustup, step E.)
        for recipe in RECIPES {
            for path in [recipe.route.launcher, recipe.route.root] {
                assert!(
                    path.starts_with("~/"),
                    "{}: recipe path {path:?} must start with ~/",
                    recipe.id
                );
                assert!(
                    !path.contains("/../") && !path.ends_with("/.."),
                    "{}: recipe path {path:?} must not climb",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_every_recipe_launcher_is_named_after_its_id() {
        // `id` is the command the user types and the launcher's file name
        // (spec §3.1); a `binary` field arrives with the first tool where
        // the two differ.
        for recipe in RECIPES {
            assert_eq!(
                Path::new(recipe.route.launcher)
                    .file_name()
                    .and_then(|n| n.to_str()),
                Some(recipe.id),
                "{}: launcher {:?} must be named after the id",
                recipe.id,
                recipe.route.launcher
            );
        }
    }

    #[test]
    fn test_every_recipe_meta_parses_and_names_the_standalone_id() {
        for recipe in RECIPES {
            let meta = AdapterMeta::from_toml(recipe.meta_toml)
                .unwrap_or_else(|e| panic!("{}: meta toml: {e}", recipe.id));
            assert_eq!(meta.id, format!("standalone-{}", recipe.id));
            assert!(!meta.id.contains(':'), "Session::build asserts no ':'");
            assert_eq!(meta.kind, "standalone");
            assert!(!meta.name.is_empty());
            assert!(meta.homepage.starts_with("https://"));
            assert!(
                !meta.verified_versions.is_empty(),
                "{}: a recorded fixture backs verified_versions",
                recipe.id
            );
        }
    }

    #[test]
    fn test_claude_is_the_native_route_read_with_its_autoupdater_off() {
        assert_eq!(CLAUDE.id, "claude");
        assert_eq!(CLAUDE.route.kind, RouteKind::SymlinkIntoRoot);
        assert_eq!(CLAUDE.route.launcher, "~/.local/bin/claude");
        assert_eq!(CLAUDE.route.root, "~/.local/share/claude");
        assert_eq!(CLAUDE.version.args, &["--version"]);
        // Spec §3.4: the version read must not start a background update
        // check; the upgrade plan (`StandaloneAdapter::plan`) must not
        // carry this.
        assert_eq!(CLAUDE.version.env, &[("DISABLE_AUTOUPDATER", "1")]);
        assert_eq!(CLAUDE.version.parse, VersionParse::FirstToken);
        assert!(CLAUDE.self_updates);
    }

    #[test]
    fn test_claude_updates_with_its_own_updater() {
        assert_eq!(CLAUDE.upgrade.args, &["update"]);
        assert_eq!(CLAUDE.upgrade.timeout_secs, 1800);
        assert_eq!(CLAUDE.upgrade.cancel, CancelPolicy::KillThenReconcile);
        assert_eq!(
            CLAUDE.latest,
            Latest::ClaudeChannel {
                base: "https://downloads.claude.ai/claude-code-releases"
            }
        );
    }

    #[test]
    fn test_recipes_lists_claude_once() {
        assert_eq!(RECIPES.len(), 1);
        assert!(std::ptr::eq(RECIPES[0], &CLAUDE));
    }

    #[test]
    fn test_every_recipe_latest_url_is_an_allowed_https_host() {
        // Spec §4.2: a recipe that brings a new host and the allowlist
        // entry are one reviewed change; `RealHttpClient::send` refuses
        // anything else before connecting, so a recipe whose host is not
        // on the list would be a permanent "could not check" row.
        use crate::http::real::host_allowed;
        for recipe in RECIPES {
            let urls: Vec<String> = match recipe.latest {
                Latest::ClaudeChannel { base } => vec![
                    format!(
                        "{base}/{}",
                        crate::adapters::standalone::latest::CHANNEL_LATEST
                    ),
                    format!(
                        "{base}/{}",
                        crate::adapters::standalone::latest::CHANNEL_STABLE
                    ),
                ],
                Latest::HttpTomlVersion { url } => vec![url.to_string()],
            };
            for url in urls {
                host_allowed(&url).unwrap_or_else(|e| panic!("{}: {url}: {e}", recipe.id));
            }
        }
    }

    /// The remove and keep paths of a recipe with a path list; empty for
    /// one without (or with another kind of uninstall, step E on).
    fn path_lists(recipe: &Recipe) -> (Vec<&'static str>, Vec<&'static str>) {
        if let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall {
            (
                remove.iter().map(|spec| spec.path).collect(),
                keep.iter().map(|spec| spec.path).collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        }
    }

    #[test]
    fn test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder() {
        // `route::expand` panics on a path that does not start with `~/`,
        // and the removal's check 1 refuses a path whose folder is the home
        // folder or one of `SHARED_FOLDERS` (ruling 5) -- a recipe listing
        // one would refuse every uninstall, and the never-list exists so no
        // recipe can quietly move `~/.local/bin` whole. Held here, on the
        // data as spelled, so the first test run says so rather than a
        // user's dialog.
        for recipe in RECIPES {
            let (remove, keep) = path_lists(recipe);
            for path in remove.iter().chain(keep.iter()) {
                let rest = path
                    .strip_prefix("~/")
                    .unwrap_or_else(|| panic!("{}: {path:?} must start with ~/", recipe.id));
                assert!(
                    !rest.is_empty()
                        && !rest.ends_with('/')
                        && !rest.contains("..")
                        && !rest.contains("/./"),
                    "{}: {path:?} must name one plain path",
                    recipe.id
                );
            }
            for path in &remove {
                let folder = Path::new(path.strip_prefix("~/").unwrap())
                    .parent()
                    .unwrap_or(Path::new(""));
                assert!(
                    folder != Path::new("")
                        && !SHARED_FOLDERS.iter().any(|shared| folder == Path::new(shared)),
                    "{}: {path:?} sits directly in the home folder or in a shared folder; check 1 would refuse it",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another() {
        // Spec §6.2: the launcher last, so a run that stops partway leaves
        // exactly the launcher-only state a second run finishes. Spec
        // §6.3's former check 7: no removed path is inside another removed
        // path (moving `a` and then `a/b` would fail on the second), and no
        // kept path is inside a removed one (it would go with it) -- both
        // properties of the constant, not of the Mac.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let last = remove
                .last()
                .unwrap_or_else(|| panic!("{}: an empty remove list", recipe.id));
            // The launcher itself. (Grok's list ends with `~/.grok/bin`, the
            // folder that holds its launcher: step D widens this with that
            // recipe, not before.)
            assert_eq!(
                last.path, recipe.route.launcher,
                "{}: the last path must be the launcher",
                recipe.id
            );
            assert_eq!(last.what, RemovedWhat::Launcher, "{}", recipe.id);
            assert!(
                !last.optional,
                "{}: the launcher is never optional",
                recipe.id
            );
            let removed: Vec<&str> = remove.iter().map(|spec| spec.path).collect();
            for a in &removed {
                for b in &removed {
                    assert!(
                        a == b || !b.starts_with(&format!("{a}/")),
                        "{}: {b:?} is inside {a:?}",
                        recipe.id
                    );
                }
                for kept in keep.iter().map(|spec| spec.path) {
                    assert!(
                        kept != *a && !kept.starts_with(&format!("{a}/")),
                        "{}: kept {kept:?} is inside removed {a:?}",
                        recipe.id
                    );
                }
            }
        }
    }

    #[test]
    fn test_claude_codes_uninstall_is_anthropics_two_paths_plus_the_download_cache() {
        // The list, exactly, in execution order: what the dialog shows
        // (spec §6.3's claude row, §6.6). The provenance is the constant's
        // doc comment and the fixture README.
        let Some(Uninstall::Paths { remove, keep }) = &CLAUDE.uninstall else {
            panic!("claude has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![
                (
                    "~/.local/share/claude",
                    Expect::Dir,
                    RemovedWhat::Program,
                    false
                ),
                ("~/.claude/downloads", Expect::Dir, RemovedWhat::Cache, true),
                (
                    "~/.local/bin/claude",
                    Expect::SymlinkIntoRoot,
                    RemovedWhat::Launcher,
                    false
                ),
            ]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.claude", KeptWhat::SettingsAndHistory),
                ("~/.claude.json", KeptWhat::Settings),
            ]
        );
    }
}
