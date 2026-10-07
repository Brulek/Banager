//! Whether a keg-only formula is linked into its Homebrew prefix, and
//! whether each of its commands' places there is free for it, read for an
//! update's preview, again right before the update runs and once more right
//! after it (y1-keg, r6).
//!
//! Homebrew leaves a keg-only formula -- `node@22`, `openssl@3` -- out of
//! `<prefix>/bin` on purpose; a person who wants its commands in Terminal
//! links it by hand, with `brew link --force <name>` (which records the
//! link as `<prefix>/var/homebrew/linked/<name>`) or with links of their
//! own. An update unlinks the version it replaces first
//! (`Homebrew::Install.install_formula`, `install.rb:633-641` in Homebrew
//! 7.0.8: every link that leads into the old keg goes, whoever made it),
//! and links the new one again only where Homebrew recorded the link
//! (`Upgrade.create_formula_installer`, `upgrade.rb:635-643`) -- and then
//! only where nothing else is in the way (`Keg#link`, `keg.rb:498-560`):
//! one file there that is not Homebrew's link for it, and Homebrew links
//! nothing, unlinks what it linked so far, and the update fails with "The
//! `brew link` step did not complete successfully"
//! (`FormulaInstaller#link`, `formula_installer.rb:1281-1347`). Either way
//! the commands are gone from Terminal: on 2026-10-07 `npm` had updated
//! itself into `<prefix>/bin/npm` first, and `node` vanished with node@22's
//! update.
//!
//! What is read, all of it through `protected::look` -- names and links,
//! never a file's contents, and nothing in or through a protected place:
//! where `<prefix>/opt/<name>` leads (the keg Homebrew counts as the
//! formula's), the names in that keg's `bin` and `sbin`, what is at the
//! same name in `<prefix>/bin` and `<prefix>/sbin` and where it leads, and
//! whether `<prefix>/var/homebrew/linked/<name>` is there.

use crate::protected::{self, look, Protected};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// What is at one command's place in the prefix (`<prefix>/bin/node`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// A link that leads into this formula's folder in the Cellar -- any
    /// of its versions, straight there or through `opt/<name>`: `brew link`
    /// put it there, or a person did.
    Linked,
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
    /// `<prefix>/var/homebrew/linked/<name>` is there: Homebrew's own
    /// record of a `brew link`, which its update links again
    /// (`Keg#linked?`, `keg.rb:274-278`).
    pub(crate) recorded: bool,
    /// Each command in the keg's `bin` and `sbin`, by name.
    pub(crate) commands: Vec<CommandLink>,
}

impl KegLinks {
    /// Linked into the prefix: by `brew link` (its record), or by hand (a
    /// command's place leads into the formula).
    pub(crate) fn linked(&self) -> bool {
        self.recorded || self.commands.iter().any(|c| c.place == Place::Linked)
    }

    /// Linked as `brew link` leaves it: recorded, and every command's
    /// place leading into the formula.
    pub(crate) fn fully_linked(&self) -> bool {
        self.recorded && self.commands.iter().all(|c| c.place == Place::Linked)
    }

    /// The names of the commands whose place leads into the formula.
    pub(crate) fn linked_names(&self) -> Vec<String> {
        self.names_where(Place::Linked)
    }

    /// The places another file holds (`Place::Taken`), as paths.
    pub(crate) fn taken_paths(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| c.place == Place::Taken)
            .map(|c| c.path.display().to_string())
            .collect()
    }

    fn names_where(&self, place: Place) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| c.place == place)
            .map(|c| c.name.clone())
            .collect()
    }
}

/// How the formula `name` (a tap's `user/tap/name` is `name` in the
/// prefix) stands in `prefix`, or `None` when that cannot be told: no
/// `opt/<name>`, one that does not lead into `Cellar/<name>`, its record
/// or a command folder of its keg that cannot be looked at, or any of it
/// in or through a protected place.
pub(crate) fn read_links(prefix: &Path, name: &str) -> Option<KegLinks> {
    let protected = Protected::of_this_process();
    let short = name.rsplit('/').next().filter(|short| plain(short))?;
    // The formula's folder in the Cellar, and the keg `opt/<name>` leads
    // to inside it: both folders, every link followed.
    let folder = |path: PathBuf| {
        look::target(&path, &protected)
            .ok()
            .filter(|(_, stat)| stat.is_dir())
            .map(|(path, _)| path)
    };
    let rack = folder(prefix.join("Cellar").join(short))?;
    let keg = folder(prefix.join("opt").join(short))?;
    if !protected::starts_with_folded(&keg, &rack) {
        return None;
    }
    let recorded = match look::lstat(&prefix.join("var/homebrew/linked").join(short), &protected) {
        Ok(stat) => stat.is_symlink(),
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(_) => return None,
    };
    let mut commands = Vec::new();
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
        for entry in listing.names().ok()? {
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
            let place = place_of(&path, &rack, &protected);
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
/// whose folder in the Cellar is `rack` (every link followed).
fn place_of(path: &Path, rack: &Path, protected: &Protected) -> Place {
    match look::lstat(path, protected) {
        Err(error) if error.kind() == ErrorKind::NotFound => Place::Free,
        Err(_) => Place::Taken,
        Ok(stat) if !stat.is_symlink() => Place::Taken,
        Ok(_) => match look::target(path, protected) {
            Ok((to, _)) if protected::starts_with_folded(&to, rack) => Place::Linked,
            Ok(_) => Place::Taken,
            Err(error) if error.kind() == ErrorKind::NotFound => Place::Free,
            Err(_) => Place::Taken,
        },
    }
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
    fn a_keg_only_formula_linked_with_brew_link_is_linked_and_recorded() {
        let prefix = node_22_prefix("links-brew-link");
        brew_link(&prefix, "22.23.3");
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(links.recorded);
        assert!(links.linked());
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
        assert!(links.taken_paths().is_empty());
        assert_eq!(links.commands[1].path, prefix.join("bin/node"));
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn a_keg_only_formula_left_as_homebrew_installed_it_is_not_linked() {
        let prefix = node_22_prefix("links-not-linked");
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(!links.recorded);
        assert!(!links.linked());
        assert!(!links.fully_linked());
        assert!(links.commands.iter().all(|c| c.place == Place::Free));
        assert!(links.linked_names().is_empty());
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn links_a_person_made_through_opt_count_as_linked_without_a_record() {
        // `ln -s /opt/homebrew/opt/node@22/bin/node /opt/homebrew/bin/node`:
        // no record, so Homebrew's update unlinks it and links nothing back.
        let prefix = node_22_prefix("links-by-hand");
        symlink(prefix.join("opt/node@22/bin/node"), prefix.join("bin/node")).unwrap();
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(!links.recorded);
        assert!(links.linked());
        assert!(!links.fully_linked());
        assert_eq!(links.linked_names(), ["node"]);
        assert_eq!(
            places(&links),
            [
                ("corepack", Place::Free),
                ("node", Place::Linked),
                ("npm", Place::Free),
                ("npx", Place::Free),
            ]
        );
        // A link into an older version still leads into the formula.
        let _ = std::fs::remove_file(prefix.join("bin/node"));
        std::fs::create_dir_all(prefix.join("Cellar/node@22/22.23.2_2/bin")).unwrap();
        std::fs::write(prefix.join("Cellar/node@22/22.23.2_2/bin/node"), b"").unwrap();
        symlink(
            "../Cellar/node@22/22.23.2_2/bin/node",
            prefix.join("bin/node"),
        )
        .unwrap();
        assert_eq!(
            read_links(&prefix, "node@22").unwrap().linked_names(),
            ["node"]
        );
        std::fs::remove_dir_all(&prefix).unwrap();
    }

    #[test]
    fn npm_updating_itself_takes_npm_and_npx_from_a_linked_node() {
        // The author's Mac on 2026-10-07, between npm's update and node@22's.
        let prefix = node_22_prefix("links-npm-took");
        brew_link(&prefix, "22.23.3");
        npm_updates_itself(&prefix);
        let links = read_links(&prefix, "node@22").expect("read");
        assert!(links.linked());
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
            links.taken_paths(),
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
        assert!(!links.linked());
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
}
