//! Refuse a cask uninstall that would unlink a command, manual page or
//! shell completion that is no longer the cask's: `Symlinked#unlink`
//! removes whatever link is at a recorded link's place, unless it leads
//! into a formula's keg (`cask/artifact/symlinked.rb`, Homebrew 7). Reads
//! the installed record and protected path metadata only.

use super::cask_receipt::{app_targets, Recorded};
use crate::commands;
use crate::protected::{self, Protected, Resolution};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// The first recorded link of `token`'s whose place holds a link that is
/// not the cask's, or that Banager cannot place or follow; `None` when
/// every one is the cask's or is left alone. A link is the cask's when its
/// own text names a place inside the cask's Caskroom folder, before
/// anything is followed (`commands::names_staged`, Homebrew's own
/// `target_links_to_source?`: Flutter's commands, through the suite it
/// moved to `<prefix>/share/flutter`), or when it leads into the cask's
/// Caskroom folder, an app it recorded, or the absolute source its stanza
/// names. Left alone, and so no conflict: a
/// place with no link (nothing there, or a file Homebrew does not touch),
/// a link to nothing (removing it stops nothing that works), and a link
/// into `<prefix>/Cellar`, which Homebrew skips (`conflicting_formula`).
/// An app recorded by name alone is looked for in `applications`,
/// Homebrew's default `appdir` (`/Applications`, which the adapter passes:
/// `BrewAdapter::applications`), and in `~/Applications`.
pub(super) fn conflict(
    prefix: &Path,
    token: &str,
    recorded: &Recorded,
    home: &Path,
    applications: &Path,
) -> Option<PathBuf> {
    let protected = Protected::new(home);
    let root = commands::caskroom_folder(prefix, token);
    let mut roots = vec![root.clone()];
    for app in app_targets(recorded) {
        let app = PathBuf::from(app);
        if app.is_absolute() {
            roots.push(app);
        } else {
            roots.push(applications.join(&app));
            roots.push(home.join("Applications").join(app));
        }
    }
    let real = |place: &Path| match protected::resolve(place, &protected, true) {
        Resolution::Found(real, _) => Some(real),
        _ => None,
    };
    let owners: Vec<PathBuf> = roots.iter().filter_map(|place| real(place)).collect();
    let staged: Vec<PathBuf> = std::iter::once(root.clone()).chain(real(&root)).collect();
    let cellar = real(&prefix.join("Cellar"));
    for artifact in &recorded.artifacts {
        let Some(stanzas) = artifact.as_object() else {
            continue;
        };
        for (stanza, args) in stanzas {
            let (link, source) = match placed(prefix, home, stanza, args) {
                Placed::NotLinked => continue,
                Placed::Unknown => return Some(root),
                Placed::At(link, source) => (link, source),
            };
            let at = match protected::resolve(&link, &protected, false) {
                Resolution::Missing => continue,
                Resolution::Found(_, stat) if !stat.is_symlink() => continue,
                Resolution::Found(at, _) => at,
                _ => return Some(link),
            };
            // Its own text, from the folder it is in: one that names a
            // place in the Caskroom folder is the cask's, whatever that
            // place leads to.
            let names_staged = at.parent().is_some_and(|folder| {
                protected::look::link_text(&at, &protected)
                    .ok()
                    .and_then(|text| commands::link_names(folder, &text))
                    .is_some_and(|named| commands::names_staged(&named, &staged))
            });
            if names_staged {
                continue;
            }
            let target = match protected::resolve(&link, &protected, true) {
                Resolution::Found(target, _) => target,
                Resolution::Missing => continue,
                _ => return Some(link),
            };
            let kept_by_homebrew = cellar
                .as_ref()
                .is_some_and(|cellar| protected::starts_with_folded(&target, cellar));
            let owned = owners
                .iter()
                .cloned()
                .chain(source.as_deref().and_then(real))
                .any(|place| protected::starts_with_folded(&target, &place));
            if !kept_by_homebrew && !owned {
                return Some(link);
            }
        }
    }
    None
}

/// Where one recorded stanza's link is, as `resolve_target` of its
/// Homebrew class puts it with `Cask::Config`'s default folders
/// (`cask/config.rb`; `cask/artifact/{relocated,manpage,bashcompletion,
/// zshcompletion,fishcompletion,pwshcompletion,command_wrapper}.rb`).
enum Placed {
    /// A stanza that links nothing.
    NotLinked,
    /// A linking stanza whose arguments Banager cannot place.
    Unknown,
    /// The link's place, and its source when the record spells it from `/`.
    At(PathBuf, Option<PathBuf>),
}

fn placed(prefix: &Path, home: &Path, stanza: &str, args: &Value) -> Placed {
    let folder = match stanza {
        "binary" | "command_wrapper" => "bin",
        "manpage" => "share/man",
        "bash_completion" => "etc/bash_completion.d",
        "zsh_completion" => "share/zsh/site-functions",
        "fish_completion" => "share/fish/vendor_completions.d",
        "pwsh_completion" => "share/pwsh/completions",
        _ => return Placed::NotLinked,
    };
    let Some(args) = args.as_array() else {
        return Placed::Unknown;
    };
    // The source -- for `command_wrapper`, the command's name, the wrapper
    // being Homebrew's own file in the Caskroom.
    let Some(source) = args.first().and_then(Value::as_str) else {
        return Placed::Unknown;
    };
    let given = match args.iter().skip(1).find_map(|arg| arg.get("target")) {
        None => None,
        Some(target) => match target.as_str() {
            // An empty one is none (`@target_string.presence`).
            Some("") => None,
            Some(target) => Some(target),
            None => return Placed::Unknown,
        },
    };
    let Some(base) = Path::new(source).file_name().and_then(|name| name.to_str()) else {
        return Placed::Unknown;
    };
    let target = given.unwrap_or(base);
    let stem = Path::new(target)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(target);
    let name = match stanza {
        // The section from the source's name; any target is ignored.
        "manpage" => match man_section(source) {
            Some(section) => format!("man{section}/{base}"),
            None => return Placed::Unknown,
        },
        "bash_completion" => stem.to_string(),
        "zsh_completion" if target.starts_with('_') => target.to_string(),
        "zsh_completion" => format!("_{stem}"),
        "fish_completion" if target.ends_with(".fish") => target.to_string(),
        "fish_completion" => format!("{stem}.fish"),
        "pwsh_completion" if target.starts_with('_') && target.ends_with(".ps1") => {
            target.to_string()
        }
        "pwsh_completion" => format!("_{stem}.ps1"),
        _ => target.to_string(),
    };
    // Homebrew expands a target that starts at `~`; anything else that
    // climbs out or is still a placeholder is not guessed at.
    let link = match name.strip_prefix("~/") {
        Some(rest) if home.is_absolute() => home.join(rest),
        Some(_) => return Placed::Unknown,
        None => prefix.join(folder).join(&name),
    };
    let rest = Path::new(name.strip_prefix("~/").unwrap_or(&name));
    if name == "~"
        || rest
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        || name.contains(['$', '{'])
    {
        return Placed::Unknown;
    }
    let source = Path::new(source);
    Placed::At(link, source.is_absolute().then(|| source.to_path_buf()))
}

/// A man page's section, as `Manpage.from_args` reads it from the
/// source's name (`/\.([1-8]|n|l)(?:\.gz)?$/`).
fn man_section(source: &str) -> Option<char> {
    let name = source.strip_suffix(".gz").unwrap_or(source);
    let mut last = name.chars().rev();
    let section = last.next()?;
    (last.next()? == '.' && matches!(section, '1'..='8' | 'n' | 'l')).then_some(section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A Homebrew prefix and a home folder of a test's own, removed when
    /// dropped. The apps are in the home folder's `Applications`, where
    /// `--appdir=~/Applications` puts apps, and the folder that stands for
    /// `/Applications` is one of its own too (`applications`), so nothing
    /// looks at this Mac's own `/Applications`.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(label: &str) -> Tmp {
            let dir = std::env::temp_dir().join(format!(
                "banager-cask-links-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(dir.join("prefix/Caskroom")).unwrap();
            std::fs::create_dir_all(dir.join("home/Applications")).unwrap();
            std::fs::create_dir_all(dir.join("Applications")).unwrap();
            Tmp(std::fs::canonicalize(&dir).unwrap())
        }

        fn prefix(&self) -> PathBuf {
            self.0.join("prefix")
        }

        fn home(&self) -> PathBuf {
            self.0.join("home")
        }

        /// What stands for `/Applications`: empty.
        fn applications(&self) -> PathBuf {
            self.0.join("Applications")
        }

        /// A file at `path`, its folders made.
        fn file(&self, path: &Path) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }

        /// A link at `at` to `to`, replacing whatever link is there.
        fn link(&self, to: &Path, at: &Path) {
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            let _ = std::fs::remove_file(at);
            symlink(to, at).unwrap();
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn recorded(artifacts: serde_json::Value) -> Recorded {
        Recorded {
            artifacts: artifacts.as_array().unwrap().clone(),
            ..Default::default()
        }
    }

    /// Ghostty's and Docker Desktop's recorded links as Homebrew 7 writes
    /// them (`$APPDIR` expanded at install, targets as the cask spells
    /// them; `formulae.brew.sh/api/cask/{ghostty,docker-desktop,wezterm}`),
    /// with the app under `~/Applications`: man pages by section, and each
    /// shell's completion renamed as Homebrew renames it.
    #[test]
    fn test_an_ordinary_casks_own_man_pages_and_completions_are_its_own() {
        let tmp = Tmp::new("own");
        let (prefix, home, apps) = (tmp.prefix(), tmp.home(), tmp.applications());
        let app = home.join("Applications/Ghostty.app/Contents/Resources");
        let a = |rest: &str| app.join(rest).to_str().unwrap().to_string();
        let record = recorded(serde_json::json!([
            { "app": ["Ghostty.app"] },
            { "manpage": [a("man/man5/ghostty.5")] },
            { "manpage": [a("man/man1/ghostty.1.gz")] },
            { "zsh_completion": [a("zsh/site-functions/_ghostty")] },
            { "bash_completion": [a("bash-completion/completions/ghostty.bash")] },
            { "fish_completion": [a("fish/vendor_completions.d/ghostty.fish")] },
            { "bash_completion": [a("etc/docker-compose.bash-completion")] },
            { "zsh_completion": [a("etc/docker-compose.zsh-completion")] },
            { "fish_completion": [a("etc/docker.fish-completion")] },
            { "zsh_completion": [a("shell-completion/zsh"), { "target": "_wezterm" }] },
            { "pwsh_completion": [a("pwsh/ghostty")] },
        ]));
        let links = [
            ("man/man5/ghostty.5", "share/man/man5/ghostty.5"),
            ("man/man1/ghostty.1.gz", "share/man/man1/ghostty.1.gz"),
            (
                "zsh/site-functions/_ghostty",
                "share/zsh/site-functions/_ghostty",
            ),
            (
                "bash-completion/completions/ghostty.bash",
                "etc/bash_completion.d/ghostty",
            ),
            (
                "fish/vendor_completions.d/ghostty.fish",
                "share/fish/vendor_completions.d/ghostty.fish",
            ),
            (
                "etc/docker-compose.bash-completion",
                "etc/bash_completion.d/docker-compose",
            ),
            (
                "etc/docker-compose.zsh-completion",
                "share/zsh/site-functions/_docker-compose",
            ),
            (
                "etc/docker.fish-completion",
                "share/fish/vendor_completions.d/docker.fish",
            ),
            ("shell-completion/zsh", "share/zsh/site-functions/_wezterm"),
            ("pwsh/ghostty", "share/pwsh/completions/_ghostty.ps1"),
        ];
        for (source, link) in links {
            tmp.file(&app.join(source));
            tmp.link(&app.join(source), &prefix.join(link));
        }
        assert_eq!(conflict(&prefix, "ghostty", &record, &home, &apps), None);

        // Each kind's link, once it leads into another source's package,
        // is not the cask's: npm links man pages under the same prefix.
        let npm = prefix.join("lib/node_modules/other/man/ghostty.1");
        tmp.file(&npm);
        for (_, link) in links {
            let link = prefix.join(link);
            let own = std::fs::read_link(&link).unwrap();
            tmp.link(&npm, &link);
            assert_eq!(
                conflict(&prefix, "ghostty", &record, &home, &apps),
                Some(link.clone())
            );
            tmp.link(&own, &link);
        }
    }

    /// Flutter's record as Homebrew 7 writes it (homebrew/cask 3.47.6:
    /// `suite "flutter", target: "#{HOMEBREW_PREFIX}/share/flutter"`,
    /// `binary "flutter/bin/dart"`, `binary "flutter/bin/flutter"`, from
    /// the Homebrew API), and its disk as Homebrew leaves it: the suite
    /// moved to `<prefix>/share/flutter`, a link to it left where it was
    /// staged (`Moved#post_move`: `FileUtils.ln_sf target, source`), and
    /// each command linked to its staged path, literally
    /// (`Symlinked#create_filesystem_link`).
    struct Flutter {
        tmp: Tmp,
        record: Recorded,
        dart: PathBuf,
        staged: PathBuf,
    }

    impl Flutter {
        fn new(label: &str) -> Flutter {
            let tmp = Tmp::new(label);
            let prefix = tmp.prefix();
            let suite = prefix.join("share/flutter");
            for command in ["dart", "flutter"] {
                tmp.file(&suite.join("bin").join(command));
            }
            let staged = prefix.join("Caskroom/flutter/3.47.6/flutter");
            tmp.link(&suite, &staged);
            for command in ["dart", "flutter"] {
                tmp.link(
                    &staged.join("bin").join(command),
                    &prefix.join("bin").join(command),
                );
            }
            let record = recorded(serde_json::json!([
                { "suite": ["flutter", { "target": suite.to_str().unwrap() }] },
                { "binary": ["flutter/bin/dart"] },
                { "binary": ["flutter/bin/flutter"] },
                { "zap": [{ "trash": "~/.flutter" }] },
            ]));
            Flutter {
                dart: prefix.join("bin/dart"),
                staged,
                record,
                tmp,
            }
        }

        fn conflict(&self) -> Option<PathBuf> {
            let tmp = &self.tmp;
            conflict(
                &tmp.prefix(),
                "flutter",
                &self.record,
                &tmp.home(),
                &tmp.applications(),
            )
        }
    }

    /// A link whose own text names a place in the cask's Caskroom folder
    /// is the cask's, whatever that place leads to: Homebrew's own test
    /// (`target_links_to_source?`, `target.readlink == source`). Flutter's
    /// `bin/dart` leads, through the staged `flutter` link, to
    /// `<prefix>/share/flutter/bin/dart`, no folder of the cask's.
    #[test]
    fn test_flutters_commands_through_its_moved_suite_are_its_own() {
        let flutter = Flutter::new("flutter");
        assert_eq!(flutter.conflict(), None);
        // Spelled from the folder the link is in, as another tool might.
        flutter.tmp.link(
            Path::new("../Caskroom/flutter/3.47.6/flutter/bin/dart"),
            &flutter.dart,
        );
        assert_eq!(flutter.conflict(), None);
    }

    /// Only the link's own text is taken for the cask's: a link that names
    /// anything else is followed and judged as before.
    #[test]
    fn test_a_flutter_command_taken_over_or_named_elsewhere_still_refuses() {
        let flutter = Flutter::new("flutter-taken");
        let prefix = flutter.tmp.prefix();
        let npm = prefix.join("lib/node_modules/dart/bin/dart.js");
        flutter.tmp.file(&npm);
        // npm's link (`npm install -g --force`).
        flutter.tmp.link(&npm, &flutter.dart);
        assert_eq!(flutter.conflict(), Some(flutter.dart.clone()));
        // Another cask's staged file, its token beginning with this one's.
        let other = prefix.join("Caskroom/flutter@beta/3.48.0/flutter/bin/dart");
        flutter.tmp.file(&other);
        flutter.tmp.link(&other, &flutter.dart);
        assert_eq!(flutter.conflict(), Some(flutter.dart.clone()));
        // By name it stays in the Caskroom folder; on the disk the `..`
        // climbs out of `share/flutter`, where the staged link leads, into
        // npm's package. Its text is not taken at its word.
        let climbs = flutter
            .staged
            .join("../../lib/node_modules/dart/bin/dart.js");
        flutter.tmp.link(&climbs, &flutter.dart);
        assert_eq!(std::fs::canonicalize(&flutter.dart).unwrap(), npm);
        assert_eq!(flutter.conflict(), Some(flutter.dart.clone()));
    }

    /// What Homebrew leaves or removes without anything that worked
    /// stopping: a link to nothing (the app dragged to the Trash, its
    /// `code` link left behind), a formula's link into the Cellar, which
    /// `Symlinked#unlink` skips (`conflicting_formula`), and a target
    /// under the home folder, which Homebrew expands (`Relocated`).
    #[test]
    fn test_dead_links_formula_links_and_home_targets_are_not_conflicts() {
        let tmp = Tmp::new("left");
        let (prefix, home, apps) = (tmp.prefix(), tmp.home(), tmp.applications());
        let app = home.join("Applications/Alacritty.app/Contents");
        let source = app.join("MacOS/alacritty");
        let terminfo = app.join("Resources/61/alacritty");
        let record = recorded(serde_json::json!([
            { "app": ["Alacritty.app"] },
            { "binary": [source.to_str().unwrap()] },
            { "binary": [terminfo.to_str().unwrap(), { "target": "~/.terminfo/61/alacritty" }] },
        ]));
        // The app is gone: both links lead nowhere.
        tmp.link(&source, &prefix.join("bin/alacritty"));
        tmp.link(&terminfo, &home.join(".terminfo/61/alacritty"));
        assert_eq!(conflict(&prefix, "alacritty", &record, &home, &apps), None);
        // The app is back: the home folder's link is the cask's.
        tmp.file(&source);
        tmp.file(&terminfo);
        assert_eq!(conflict(&prefix, "alacritty", &record, &home, &apps), None);
        // A formula's link: Homebrew leaves it where it is.
        let keg = prefix.join("Cellar/alacritty/0.16.1/bin/alacritty");
        tmp.file(&keg);
        tmp.link(&keg, &prefix.join("bin/alacritty"));
        assert_eq!(conflict(&prefix, "alacritty", &record, &home, &apps), None);
        // npm's, at the home folder's target: not the cask's.
        let npm = prefix.join("lib/node_modules/terminfo/alacritty");
        tmp.file(&npm);
        tmp.link(&npm, &home.join(".terminfo/61/alacritty"));
        assert_eq!(
            conflict(&prefix, "alacritty", &record, &home, &apps),
            Some(home.join(".terminfo/61/alacritty"))
        );
    }
}
