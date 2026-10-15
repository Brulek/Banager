//! Whether a keg-only formula is linked into its Homebrew prefix, and
//! whether each of its commands' places there is free for it, read for an
//! update's preview, again right before the update runs and once more right
//! after it (y1-keg, r6); and for the preview of the link a source's notice
//! offers and once it has run (`OpKind::Link`, y2-npmwhy): the one reading
//! of a formula's links both use, so that "linked" and "in the way" mean
//! the same to both, and both run the same `brew link --formula --force`.
//!
//! Homebrew leaves a keg-only formula -- `node@22`, `openssl@3` -- out of
//! `<prefix>/bin` on purpose, except a versioned one installed on request
//! with no other version of it there, which it links itself
//! (`FormulaInstaller#auto_link_versioned_keg_only?`,
//! `formula_installer.rb:1923-1934`). Otherwise a person who wants its
//! commands in Terminal links it: with `brew link --force <name>`, or with
//! links of their own. What Homebrew 7.0.8's update does then depends on
//! one thing, `<prefix>/var/homebrew/linked/<name>`, the record a `brew
//! link` leaves (`Keg#linked?`, `keg.rb:274-278`):
//!
//! - With the record, the update first unlinks the version it replaces
//!   (`Upgrade.outdated_kegs`, `upgrade.rb:268-272`; `install.rb:632-641`):
//!   every link whose one-level target -- the link's text joined to its
//!   folder, nothing followed (`Utils::Path.resolved_path`,
//!   `utils/path.rb:84-85`) -- is that version's own file goes
//!   (`Keg#unlink`, `keg.rb:361-391`), and nothing else does. Then it links
//!   the new version (`upgrade.rb:640-643`), and stops at any place that is
//!   there and is not its own link to the new version or a cask's link
//!   (`Keg#make_relative_symlink`, `keg.rb:823-861`): it links nothing,
//!   and the update fails with "The `brew link` step did not complete
//!   successfully" (`FormulaInstaller#link`, `formula_installer.rb:1281-1347`)
//!   -- on 2026-10-07 `npm` had updated itself into `<prefix>/bin/npm`
//!   first, and `node` vanished with node@22's update.
//! - Without it, the update unlinks nothing and links nothing; only
//!   `opt/<name>` moves to the new version. A link of a person's own
//!   through `opt/<name>` follows it there; one straight into the version
//!   replaced keeps leading to it until that version is cleaned up
//!   (`brew cleanup` keeps only a recorded one, `formula.rb:3767-3793`),
//!   and then to nothing.
//!
//! What is read, all of it through `protected::look` -- names and links,
//! never a file's contents, and nothing in or through a protected place:
//! where `<prefix>/opt/<name>` leads (the keg Homebrew counts as the
//! formula's), the names in that keg's `bin` and `sbin`, what is at the
//! same name in `<prefix>/bin` and `<prefix>/sbin`, its link's text and
//! where it leads, and `<prefix>/var/homebrew/linked/<name>`, its text and
//! whether it leads to a folder.

use crate::protected::{self, look, Protected};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// What is at one command's place in the prefix (`<prefix>/bin/node`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// A link whose one-level target is this command's file in the keg
    /// `opt/<name>` leads to: the link `brew link` makes, whoever made it.
    /// Homebrew's update removes it where the formula's link is recorded
    /// (`keg.rb:376-377`), and leaves it, leading to the version replaced,
    /// where it is not.
    Linked,
    /// A link that leads into the formula some other way -- through
    /// `opt/<name>` or another linked folder, or into another of its
    /// versions. Homebrew's update leaves it as it is, so it keeps working
    /// (through `opt/<name>`, with the new version); but `brew link` of the
    /// formula, Homebrew's own after a recorded formula's update among
    /// them, stops at it (`keg.rb:850-851`).
    SurvivesUpdate,
    /// Nothing, or a link that leads nowhere, which `brew link` replaces
    /// (`Keg#make_relative_symlink`, `keg.rb:850-856`): free for the
    /// formula's own link.
    Free,
    /// A file, or a link that leads somewhere else -- another formula's,
    /// a copy npm put there of itself -- or something that cannot be looked
    /// at: `brew link` without `--overwrite` stops at it
    /// (`Keg::ConflictError`) and links none of the formula.
    Taken,
}

/// One command of the formula: its name, its place in the prefix, and
/// what is there now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandLink {
    /// `node`, as typed in Terminal.
    pub(crate) name: String,
    /// `<prefix>/bin/node` (or `<prefix>/sbin/…`).
    pub(crate) path: PathBuf,
    pub(crate) place: Place,
}

/// How one formula stands in its prefix, as `read_links` found it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KegLinks {
    /// `<prefix>/var/homebrew/linked/<name>` is a link to a folder whose
    /// one-level target is the keg `opt/<name>` leads to: Homebrew's own
    /// record of a `brew link` (`Keg#linked?`, `keg.rb:274-278`), the one
    /// thing that makes its update unlink the formula and link it again
    /// (`upgrade.rb:268-272`, `640-643`).
    pub(crate) recorded: bool,
    /// Each command in the keg's `bin` and `sbin`, by name.
    pub(crate) commands: Vec<CommandLink>,
}

impl KegLinks {
    /// Linked as `brew link` leaves it: recorded, and every command's
    /// place holding Homebrew's link to it.
    pub(crate) fn fully_linked(&self) -> bool {
        self.recorded && self.commands.iter().all(|c| c.place == Place::Linked)
    }

    /// The names of the commands whose place holds Homebrew's link
    /// (`Place::Linked`): what a recorded formula's update unlinks and
    /// links again.
    pub(crate) fn linked_names(&self) -> Vec<String> {
        self.names_where(|place| place == Place::Linked)
    }

    /// The names of all of the formula's commands, in its keg's `bin` and
    /// `sbin`, sorted, each once: what `brew link` puts where Terminal
    /// looks (`Warning::LinkPutsCommands`, the link a source's notice
    /// offers, y2-npmwhy).
    pub(crate) fn command_names(&self) -> Vec<String> {
        let mut names = self.names_where(|_| true);
        names.dedup();
        names
    }

    /// The names of the commands typing which runs the formula: whose
    /// place leads into it, Homebrew's link or not.
    pub(crate) fn in_terminal_names(&self) -> Vec<String> {
        self.names_where(|place| matches!(place, Place::Linked | Place::SurvivesUpdate))
    }

    /// The places a `brew link` of the formula stops at, as paths: another
    /// file there (`Place::Taken`), or a link into the formula that is not
    /// Homebrew's (`Place::SurvivesUpdate`) -- both are there, and neither
    /// is its own link nor goes with the update's unlink (`keg.rb:850-851`).
    pub(crate) fn held_paths(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| matches!(c.place, Place::Taken | Place::SurvivesUpdate))
            .map(|c| c.path.display().to_string())
            .collect()
    }

    /// The places a `brew link` of the formula would take back with its
    /// own if it stopped (`Warning::LinkRollbackRisk`), as paths: those
    /// already holding Homebrew's link (`Place::Linked`), where its link is
    /// not recorded. `Keg#link` skips such a link, but on any error, also
    /// outside `bin` and `sbin`, its `rescue` calls `Keg#unlink`
    /// (`keg.rb:590-598`), which removes every link to the keg's files,
    /// whoever made it (`keg.rb:361-391`). None where the link is recorded:
    /// `brew link` then says "Already linked" and changes nothing
    /// (`cmd/link.rb`).
    pub(crate) fn rollback_paths(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| !self.recorded && c.place == Place::Linked)
            .map(|c| c.path.display().to_string())
            .collect()
    }

    fn names_where(&self, wanted: impl Fn(Place) -> bool) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| wanted(c.place))
            .map(|c| c.name.clone())
            .collect()
    }
}

/// How the formula `name` (a tap's `user/tap/name` is `name` in the
/// prefix) stands in `prefix`, or `None` when that cannot be told: no
/// `opt/<name>`, one that does not lead into `Cellar/<name>`, its record
/// or a command folder of its keg that cannot be looked at, `bin` and
/// `sbin` together holding more names than one directory budget
/// (`look::ListingBudget`) -- never the commands of part of them -- or any
/// of it in or through a protected place.
pub(crate) fn read_links(prefix: &Path, name: &str) -> Option<KegLinks> {
    let protected = Protected::of_this_process();
    let short = name.rsplit('/').next().filter(|short| plain(short))?;
    // The formula's folder in the Cellar, and the keg `opt/<name>` leads
    // to inside it: both folders, every link followed.
    let folder = |path: &Path| {
        look::target(path, &protected)
            .ok()
            .filter(|(_, stat)| stat.is_dir())
            .map(|(path, _)| path)
    };
    let rack = folder(&prefix.join("Cellar").join(short))?;
    let opt = prefix.join("opt").join(short);
    let keg = folder(&opt)?;
    if !protected::starts_with_folded(&keg, &rack) {
        return None;
    }
    // The keg as Homebrew names it: `opt/<name>`'s one-level target
    // (`Keg.new(Utils::Path.resolved_path(formula.opt_prefix))`,
    // `upgrade.rb:635-636`), against which a link and the record are
    // compared, as Homebrew compares them.
    let homebrews_keg = one_level_target(&opt, &protected).ok()?;
    let record = prefix.join("var/homebrew/linked").join(short);
    let recorded = match look::lstat(&record, &protected) {
        Ok(stat) if stat.is_symlink() => {
            folder(&record).is_some()
                && one_level_target(&record, &protected).ok()? == homebrews_keg
        }
        Ok(_) => false,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(_) => return None,
    };
    let mut commands = Vec::new();
    let mut budget = look::ListingBudget::default();
    for folder in ["bin", "sbin"] {
        let listing = match look::list(&keg.join(folder), &protected) {
            Ok(listing) => listing,
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                continue
            }
            Err(_) => return None,
        };
        for entry in listing.names(&mut budget).ok()? {
            let Some(command) = entry.to_str().filter(|n| plain(n) && *n != ".DS_Store") else {
                continue;
            };
            let Ok(stat) = listing.lstat(&entry) else {
                continue;
            };
            // What `brew link` links of a keg's `bin`: its files, and its
            // links into the keg itself (npm's `bin/npm` leads to
            // `lib/node_modules/npm/bin/npm-cli.js`); never a folder
            // (`link_dir("bin") { :skip_dir }`). A link out of the keg is
            // another formula's (`keg.rb:923-926`) and is left out.
            let links = if stat.is_symlink() {
                matches!(
                    look::target(&keg.join(folder).join(command), &protected),
                    Ok((to, stat)) if !stat.is_dir() && protected::starts_with_folded(&to, &keg)
                )
            } else {
                stat.is_file()
            };
            if !links {
                continue;
            }
            let path = prefix.join(folder).join(command);
            let homebrews = homebrews_keg.join(folder).join(command);
            let place = place_of(&path, &homebrews, &rack, &protected);
            commands.push(CommandLink {
                name: command.to_string(),
                path,
                place,
            });
        }
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    Some(KegLinks { recorded, commands })
}

/// What is at `path`, a command's place in the prefix, for the formula
/// whose folder in the Cellar is `rack` (every link followed) and whose
/// file for that command, as Homebrew names it, is `homebrews`.
fn place_of(path: &Path, homebrews: &Path, rack: &Path, protected: &Protected) -> Place {
    match look::lstat(path, protected) {
        Err(error) if error.kind() == ErrorKind::NotFound => Place::Free,
        Err(_) => Place::Taken,
        Ok(stat) if !stat.is_symlink() => Place::Taken,
        Ok(_) => {
            if one_level_target(path, protected).is_ok_and(|to| to == homebrews) {
                return Place::Linked;
            }
            match look::target(path, protected) {
                Ok((to, _)) if protected::starts_with_folded(&to, rack) => Place::SurvivesUpdate,
                Ok(_) => Place::Taken,
                Err(error) if error.kind() == ErrorKind::NotFound => Place::Free,
                Err(_) => Place::Taken,
            }
        }
    }
}

/// The link `path`'s one-level target: its text joined to its folder,
/// nothing followed (`Utils::Path.resolved_path`, `utils/path.rb:84-85`).
fn one_level_target(path: &Path, protected: &Protected) -> std::io::Result<PathBuf> {
    let text = look::link_text(path, protected)?;
    Ok(lexical_join(path.parent().unwrap_or(Path::new("/")), &text))
}

/// `folder` joined with a link's text `link` as Ruby's `Pathname#join`
/// does it: an absolute `link` starts over, and `..` takes off the name
/// before it, without following any link.
fn lexical_join(folder: &Path, link: &Path) -> PathBuf {
    let mut joined = PathBuf::new();
    for component in folder.join(link).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // At the root `..` is the root, as it is for `Pathname`.
                if joined.parent().is_some() {
                    joined.pop();
                }
            }
            other => joined.push(other),
        }
    }
    joined
}

/// Whether `name` is one plain path component: not empty, no `/`, not `.`
/// or `..`.
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::adapters::read_file::tests::temp_dir;
    use std::os::unix::fs::symlink;

    /// A Homebrew prefix under a fresh temporary folder, laid out as
    /// Homebrew 7.0.8 lays out `node@22` 22.23.3 (`make_keg`), with
    /// `opt/node@22` leading to it, and the prefix's `bin` and
    /// `lib/node_modules` with nothing of it yet.
    pub(crate) fn node_22_prefix(tag: &str) -> PathBuf {
        let prefix = temp_dir(tag);
        make_keg(&prefix, "22.23.3");
        std::fs::create_dir_all(prefix.join("opt")).unwrap();
        symlink("../Cellar/node@22/22.23.3", prefix.join("opt/node@22")).unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::create_dir_all(prefix.join("lib/node_modules")).unwrap();
        std::fs::create_dir_all(prefix.join("var/homebrew/linked")).unwrap();
        prefix
    }

    /// `node@22`'s keg `version` in `prefix`'s Cellar: its `bin` with
    /// `node` (a file) and `corepack`, `npm` and `npx` (links into the
    /// keg's own `lib/node_modules`).
    pub(crate) fn make_keg(prefix: &Path, version: &str) {
        let keg = prefix.join("Cellar/node@22").join(version);
        std::fs::create_dir_all(keg.join("bin")).unwrap();
        for module in ["npm/bin", "corepack/dist"] {
            std::fs::create_dir_all(keg.join("lib/node_modules").join(module)).unwrap();
        }
        std::fs::write(keg.join("bin/node"), b"").unwrap();
        std::fs::write(keg.join("lib/node_modules/npm/bin/npm-cli.js"), b"").unwrap();
        std::fs::write(keg.join("lib/node_modules/npm/bin/npx-cli.js"), b"").unwrap();
        std::fs::write(keg.join("lib/node_modules/corepack/dist/corepack.js"), b"").unwrap();
        symlink(
            "../lib/node_modules/npm/bin/npm-cli.js",
            keg.join("bin/npm"),
        )
        .unwrap();
        symlink(
            "../lib/node_modules/npm/bin/npx-cli.js",
            keg.join("bin/npx"),
        )
        .unwrap();
        symlink(
            "../lib/node_modules/corepack/dist/corepack.js",
            keg.join("bin/corepack"),
        )
        .unwrap();
    }

    /// What `brew link --force node@22` leaves: a relative link in the
    /// prefix's `bin` for each command, and the record.
    pub(crate) fn brew_link(prefix: &Path, version: &str) {
        for command in ["corepack", "node", "npm", "npx"] {
            symlink(
                format!("../Cellar/node@22/{version}/bin/{command}"),
                prefix.join("bin").join(command),
            )
            .unwrap();
        }
        symlink(
            format!("../../../Cellar/node@22/{version}"),
            prefix.join("var/homebrew/linked/node@22"),
        )
        .unwrap();
    }

    /// What `npm install -g npm` from that npm leaves, its prefix being
    /// Homebrew's: its own copy in `lib/node_modules/npm`, and `bin/npm`
    /// and `bin/npx` leading there instead.
    pub(crate) fn npm_updates_itself(prefix: &Path) {
        let own = prefix.join("lib/node_modules/npm/bin");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("npm-cli.js"), b"").unwrap();
        std::fs::write(own.join("npx-cli.js"), b"").unwrap();
        for (command, script) in [("npm", "npm-cli.js"), ("npx", "npx-cli.js")] {
            let _ = std::fs::remove_file(prefix.join("bin").join(command));
            symlink(
                format!("../lib/node_modules/npm/bin/{script}"),
                prefix.join("bin").join(command),
            )
            .unwrap();
        }
    }

    fn places(links: &KegLinks) -> Vec<(&str, Place)> {
        links
            .commands
            .iter()
            .map(|c| (c.name.as_str(), c.place))
            .collect()
    }

    #[test]
    fn bounded_links_share_the_limit_across_bin_and_sbin() {
        let prefix = node_22_prefix("bounded-links");
        let keg = prefix.join("Cellar/node@22/22.23.3");
        std::fs::create_dir_all(keg.join("sbin")).unwrap();
        for folder in ["bin", "sbin"] {
            for n in 0..2050 {
                std::fs::write(keg.join(folder).join(format!(".ignored-{n}")), "").unwrap();
            }
        }
        let (answer, calls) = crate::dirfd::calls::measure(|| read_links(&prefix, "node@22"));
        assert!(answer.is_none());
        assert_eq!(calls.entries, 4097);
        std::fs::remove_dir_all(prefix).unwrap();
    }

    #[test]
    fn a_keg_only_formula_linked_with_brew_link_is_linked_and_recorded() {
        let prefix = node_22_prefix("links-brew-link");
        brew_link(&prefix, "22.23.3");
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(links.recorded);
        assert!(links.fully_linked());
        assert_eq!(
            places(&links),
            [
                ("corepack", Place::Linked),
                ("node", Place::Linked),
                ("npm", Place::Linked),
                ("npx", Place::Linked),
            ]
        );
        assert_eq!(links.linked_names(), ["corepack", "node", "npm", "npx"]);
        assert!(links.held_paths().is_empty());
        assert_eq!(links.commands[1].path, prefix.join("bin/node"));
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn a_keg_only_formula_left_as_homebrew_installed_it_is_not_linked() {
        let prefix = node_22_prefix("links-not-linked");
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(!links.recorded);
        assert!(links.in_terminal_names().is_empty());
        assert!(!links.fully_linked());
        assert!(links.commands.iter().all(|c| c.place == Place::Free));
        assert!(links.linked_names().is_empty());
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn a_link_through_opt_survives_the_update_and_only_a_link_into_the_keg_is_homebrews() {
        // `ln -s ../opt/node@22/bin/node bin/node`: its one-level target is
        // `<prefix>/opt/node@22/bin/node`, not the keg's file, so Homebrew's
        // unlink leaves it (`keg.rb:376-377`) and it follows `opt` to the
        // new version -- but `brew link` stops at it (`keg.rb:850-851`).
        let prefix = node_22_prefix("links-by-hand");
        for through_opt in [
            PathBuf::from("../opt/node@22/bin/node"),
            prefix.join("opt/node@22/bin/node"),
        ] {
            let _ = std::fs::remove_file(prefix.join("bin/node"));
            symlink(&through_opt, prefix.join("bin/node")).unwrap();
            let links = read_links(&prefix, "node@22").expect("read");
            assert!(!links.recorded);
            assert!(!links.fully_linked());
            assert_eq!(
                places(&links),
                [
                    ("corepack", Place::Free),
                    ("node", Place::SurvivesUpdate),
                    ("npm", Place::Free),
                    ("npx", Place::Free),
                ],
                "{through_opt:?}"
            );
            assert!(links.linked_names().is_empty());
            assert_eq!(links.in_terminal_names(), ["node"]);
            assert_eq!(
                links.held_paths(),
                [prefix.join("bin/node").display().to_string()]
            );
        }
        // Straight into the keg `opt` leads to, relative or not: the link
        // `brew link` makes, which Homebrew's unlink removes.
        for into_keg in [
            PathBuf::from("../Cellar/node@22/22.23.3/bin/node"),
            prefix.join("Cellar/node@22/22.23.3/bin/node"),
        ] {
            let _ = std::fs::remove_file(prefix.join("bin/node"));
            symlink(&into_keg, prefix.join("bin/node")).unwrap();
            let links = read_links(&prefix, "node@22").expect("read");
            assert_eq!(links.linked_names(), ["node"], "{into_keg:?}");
            assert_eq!(links.in_terminal_names(), ["node"]);
            assert!(links.held_paths().is_empty());
        }
        // Into an older version: not the keg the update replaces, so it
        // stays as it is, leading to that older version.
        let _ = std::fs::remove_file(prefix.join("bin/node"));
        std::fs::create_dir_all(prefix.join("Cellar/node@22/22.23.2_2/bin")).unwrap();
        std::fs::write(prefix.join("Cellar/node@22/22.23.2_2/bin/node"), b"").unwrap();
        symlink(
            "../Cellar/node@22/22.23.2_2/bin/node",
            prefix.join("bin/node"),
        )
        .unwrap();
        let links = read_links(&prefix, "node@22").unwrap();
        assert!(links.linked_names().is_empty());
        assert_eq!(links.in_terminal_names(), ["node"]);
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn a_record_counts_only_where_it_leads_to_the_keg_opt_leads_to() {
        // `Keg#linked?` (`keg.rb:274-278`): a link, to a folder, whose
        // one-level target is the keg -- what the update relinks
        // (`upgrade.rb:640-643`).
        let prefix = node_22_prefix("links-record");
        brew_link(&prefix, "22.23.3");
        assert!(read_links(&prefix, "node@22").unwrap().recorded);
        let record = prefix.join("var/homebrew/linked/node@22");
        std::fs::remove_file(&record).unwrap();
        symlink("../../../Cellar/node@22/22.1.0", &record).unwrap();
        assert!(
            !read_links(&prefix, "node@22").unwrap().recorded,
            "to nothing"
        );
        std::fs::remove_file(&record).unwrap();
        std::fs::create_dir_all(prefix.join("Cellar/node@22/22.23.2_2")).unwrap();
        symlink("../../../Cellar/node@22/22.23.2_2", &record).unwrap();
        assert!(
            !read_links(&prefix, "node@22").unwrap().recorded,
            "to another version"
        );
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn joins_a_link_to_its_folder_as_homebrew_does_without_following_anything() {
        let at = Path::new("/opt/homebrew/bin");
        assert_eq!(
            lexical_join(at, Path::new("../opt/node@22/bin/node")),
            Path::new("/opt/homebrew/opt/node@22/bin/node")
        );
        assert_eq!(
            lexical_join(at, Path::new("/opt/homebrew/Cellar/./x/../y")),
            Path::new("/opt/homebrew/Cellar/y")
        );
        assert_eq!(
            lexical_join(at, Path::new("../../../../../z")),
            Path::new("/z")
        );
    }

    #[test]
    fn npm_updating_itself_takes_npm_and_npx_from_a_linked_node() {
        // The author's Mac on 2026-10-07, between npm's update and node@22's.
        let prefix = node_22_prefix("links-npm-took");
        brew_link(&prefix, "22.23.3");
        npm_updates_itself(&prefix);
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(links.recorded);
        assert!(!links.fully_linked());
        assert_eq!(
            places(&links),
            [
                ("corepack", Place::Linked),
                ("node", Place::Linked),
                ("npm", Place::Taken),
                ("npx", Place::Taken),
            ]
        );
        assert_eq!(
            links.held_paths(),
            [
                prefix.join("bin/npm").display().to_string(),
                prefix.join("bin/npx").display().to_string(),
            ]
        );
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn another_formulas_link_or_a_file_takes_a_place_and_a_link_to_nothing_does_not() {
        let prefix = node_22_prefix("links-other");
        // `node`, linked: its `bin/node`.
        std::fs::create_dir_all(prefix.join("Cellar/node/25.0.0/bin")).unwrap();
        std::fs::write(prefix.join("Cellar/node/25.0.0/bin/node"), b"").unwrap();
        symlink("../Cellar/node/25.0.0/bin/node", prefix.join("bin/node")).unwrap();
        // A file someone copied in.
        std::fs::write(prefix.join("bin/corepack"), b"").unwrap();
        // A link to a version since deleted.
        symlink("../Cellar/node@22/22.1.0/bin/npm", prefix.join("bin/npm")).unwrap();
        let links = read_links(&prefix, "node@22").expect("read");
        assert_eq!(
            places(&links),
            [
                ("corepack", Place::Taken),
                ("node", Place::Taken),
                ("npm", Place::Free),
                ("npx", Place::Free),
            ]
        );
        assert!(links.in_terminal_names().is_empty());
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn reads_a_kegs_sbin_and_leaves_out_folders_and_links_out_of_the_keg() {
        let prefix = node_22_prefix("links-sbin");
        let keg = prefix.join("Cellar/node@22/22.23.3");
        std::fs::create_dir_all(keg.join("sbin")).unwrap();
        std::fs::write(keg.join("sbin/nodeserv"), b"").unwrap();
        std::fs::create_dir_all(keg.join("bin/helpers")).unwrap();
        std::fs::create_dir_all(prefix.join("Cellar/other/1.0/bin")).unwrap();
        std::fs::write(prefix.join("Cellar/other/1.0/bin/tool"), b"").unwrap();
        symlink("../../../other/1.0/bin/tool", keg.join("bin/tool")).unwrap();
        let links = read_links(&prefix, "node@22").expect("read");
        let names: Vec<&str> = links.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["corepack", "node", "nodeserv", "npm", "npx"]);
        let sbin = links
            .commands
            .iter()
            .find(|c| c.name == "nodeserv")
            .unwrap();
        assert_eq!(sbin.path, prefix.join("sbin/nodeserv"));
        assert_eq!(sbin.place, Place::Free);
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn reads_nothing_without_opt_or_in_a_protected_place() {
        let prefix = node_22_prefix("links-none");
        assert_eq!(read_links(&prefix, "wget"), None);
        assert_eq!(read_links(&prefix, ".."), None);
        // A tap's formula is its last name in the prefix.
        assert!(read_links(&prefix, "someone/tap/node@22").is_some());
        // `opt` leading somewhere other than its own Cellar folder.
        std::fs::remove_file(prefix.join("opt/node@22")).unwrap();
        std::fs::create_dir_all(prefix.join("elsewhere/bin")).unwrap();
        symlink("../elsewhere", prefix.join("opt/node@22")).unwrap();
        assert_eq!(read_links(&prefix, "node@22"), None);
        std::fs::remove_dir_all(&prefix).unwrap();
        // A prefix kept in `~/Documents`: never looked into.
        let home = std::fs::canonicalize(node_22_prefix("links-kept")).unwrap();
        let kept = home.join("Documents/homebrew");
        std::fs::create_dir_all(&kept).unwrap();
        for part in ["Cellar", "opt", "bin", "var"] {
            std::fs::rename(home.join(part), kept.join(part)).unwrap();
        }
        assert!(read_links(&kept, "node@22").is_some());
        let as_if = crate::protected::as_if_home(&home);
        assert_eq!(read_links(&kept, "node@22"), None);
        drop(as_if);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn every_command_linked_into_the_keg_without_the_record_is_not_fully_linked() {
        // Each of the four linked straight into the keg by hand, as `brew
        // link` links them, but no `brew link` ran: no record, so Homebrew's
        // update unlinks and links nothing (`upgrade.rb:268-272`,
        // `640-643`). Linked, not linked as `brew link` leaves it.
        let prefix = node_22_prefix("links-by-hand-all");
        brew_link(&prefix, "22.23.3");
        std::fs::remove_file(prefix.join("var/homebrew/linked/node@22")).unwrap();
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(!links.recorded);
        assert_eq!(links.linked_names(), ["corepack", "node", "npm", "npx"]);
        assert!(!links.fully_linked());
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn a_command_in_both_bin_and_sbin_is_named_once() {
        // Two places, `bin/node` and `sbin/node`, one name typed in
        // Terminal: the link a source's notice offers names it once.
        let prefix = node_22_prefix("links-bin-and-sbin");
        let sbin = prefix.join("Cellar/node@22/22.23.3/sbin");
        std::fs::create_dir_all(&sbin).unwrap();
        std::fs::write(sbin.join("node"), b"").unwrap();
        let links = read_links(&prefix, "node@22").expect("read");
        let node: Vec<PathBuf> = links
            .commands
            .iter()
            .filter(|c| c.name == "node")
            .map(|c| c.path.clone())
            .collect();
        assert_eq!(node, [prefix.join("bin/node"), prefix.join("sbin/node")]);
        assert_eq!(links.command_names(), ["corepack", "node", "npm", "npx"]);
        std::fs::remove_dir_all(&prefix).unwrap();
    }
}
