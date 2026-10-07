//! What `brew link --force <formula>` would do in the prefix's `bin`
//! folder, read for its preview (`OpKind::Link`): the formula's own
//! commands, which it puts where Terminal looks
//! (`Warning::LinkPutsCommands`), and the files already there under their
//! names, which stand in its way (`Warning::LinkConflicts`).
//!
//! Homebrew links each of a keg's files to the same place under the prefix,
//! and stops at the first one it finds there already, unless that is a link
//! to this very file (`Keg#make_relative_symlink`, keg.rb:822-861 in
//! Homebrew 7.0.8) -- a link into another installed version of the same
//! formula is in the way too: it raises `ConflictError` and takes back the links it
//! made (`Keg#link`'s `rescue LinkError`). A link that leads nowhere is not
//! in the way -- Homebrew replaces it. So the link changes nothing while one
//! of these is there, and the preview says so instead of offering it.
//!
//! The case it was written for: npm updated through itself (`npm install
//! -g npm`) keeps its own `npm` and `npx` in `<prefix>/bin`, leading into
//! `<prefix>/lib/node_modules/npm`, where `node@22`'s link would go.
//!
//! Only `bin` is read: its names, then where each of the formula's names
//! there leads. Homebrew links the keg's other folders too (`lib`, `share`,
//! `include`), and a file in the way there is said by Homebrew itself when
//! it refuses. Nothing is read in or through a protected place
//! (`protected::look`); nothing is written and nothing runs.

use crate::protected::{look, Protected};
use std::path::{Path, PathBuf};

/// What `brew link --force <name>` would do in `<prefix>/bin`, read for its
/// preview: the formula's commands, and the files there that would stop it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LinkPreview {
    /// The names in `<prefix>/opt/<name>/bin`, sorted, but for a hidden one
    /// (`.DS_Store`, which Homebrew skips): the commands the link puts where
    /// Terminal looks (`Warning::LinkPutsCommands`).
    pub commands: Vec<String>,
    /// The files in `<prefix>/bin` under those names that would stop the
    /// link, in the same order: each one that leads somewhere other than
    /// into the version being linked (`<prefix>/opt/<name>`'s real path,
    /// `Cellar/<name>/<version>`) (`Warning::LinkConflicts`).
    pub in_the_way: Vec<PathBuf>,
}

/// The preview of `brew link --force <name>` (`LinkPreview`). Empty when
/// the keg's `bin` cannot be listed (no such folder, or in a protected
/// place): Homebrew's own refusal then says what it found.
pub(crate) fn link_preview(prefix: &Path, name: &str) -> LinkPreview {
    let protected = Protected::of_this_process();
    let Some(short) = name.rsplit('/').next().filter(|short| plain(short)) else {
        return LinkPreview::default();
    };
    // The version being linked: what `opt/<name>` leads to,
    // `Cellar/<name>/<version>`. Not the whole `Cellar/<name>`: a link
    // left over into another installed version is in the way too.
    let Ok(keg) = look::real_path(&prefix.join("opt").join(short), &protected) else {
        return LinkPreview::default();
    };
    let Ok(listing) = look::list(&prefix.join("opt").join(short).join("bin"), &protected) else {
        return LinkPreview::default();
    };
    let Ok(names) = listing.names() else {
        return LinkPreview::default();
    };
    let mut commands: Vec<String> = names
        .into_iter()
        .filter_map(|command| command.to_str().map(str::to_string))
        .filter(|command| !command.starts_with('.'))
        .collect();
    commands.sort();
    let in_the_way = commands
        .iter()
        .map(|command| prefix.join("bin").join(command))
        .filter(|there| match look::target(there, &protected) {
            Ok((real, _)) => !real.starts_with(&keg),
            // Nothing there, or a link that leads nowhere: Homebrew puts
            // its own in its place. Anything else (a protected place, a
            // folder that cannot be searched) is not known to be in the way.
            Err(_) => false,
        })
        .collect();
    LinkPreview {
        commands,
        in_the_way,
    }
}

/// Whether `name` is one plain path component: not empty, no `/`, not `.`
/// or `..` (as `kegs::plain`).
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A prefix laid out as the author's was on 2026-10-07: `node@22`'s
    /// keg, its `opt` link, no `bin/node`, and npm's own newer copy's
    /// `npm` in `bin`.
    fn prefix(tag: &str) -> PathBuf {
        let root = crate::testing::unique_temp_path(&format!("brew-link-{tag}"));
        let keg = root.join("Cellar/node@22/22.23.3_1/bin");
        std::fs::create_dir_all(&keg).unwrap();
        for command in ["node", "npm", "corepack"] {
            std::fs::write(keg.join(command), b"").unwrap();
        }
        std::fs::create_dir_all(root.join("opt")).unwrap();
        symlink("../Cellar/node@22/22.23.3_1", root.join("opt/node@22")).unwrap();
        std::fs::create_dir_all(root.join("lib/node_modules/npm/bin")).unwrap();
        std::fs::write(root.join("lib/node_modules/npm/bin/npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        symlink(
            "../lib/node_modules/npm/bin/npm-cli.js",
            root.join("bin/npm"),
        )
        .unwrap();
        root
    }

    #[test]
    fn test_npms_own_copy_is_in_the_way_of_linking_node() {
        let root = prefix("npm-in-the-way");
        let found = link_preview(&root, "node@22");
        let expected = LinkPreview {
            commands: vec![
                "corepack".to_string(),
                "node".to_string(),
                "npm".to_string(),
            ],
            in_the_way: vec![root.join("bin/npm")],
        };
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, expected);
    }

    #[test]
    fn test_its_own_links_and_links_to_nothing_are_not_in_the_way() {
        let root = prefix("own-links");
        std::fs::remove_file(root.join("bin/npm")).unwrap();
        // The keg's own `node`, linked before; a `corepack` leading nowhere.
        symlink(
            "../Cellar/node@22/22.23.3_1/bin/node",
            root.join("bin/node"),
        )
        .unwrap();
        symlink(
            "../lib/node_modules/corepack/gone.js",
            root.join("bin/corepack"),
        )
        .unwrap();
        let found = link_preview(&root, "node@22").in_the_way;
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, Vec::<PathBuf>::new());
    }

    #[test]
    fn test_a_link_into_another_installed_version_is_in_the_way() {
        // Homebrew skips only a link to this very file (`src ==
        // resolved_path(dst)`, keg.rb:824 in Homebrew 7.0.8): one left
        // over from another version still installed raises
        // `ConflictError`, although it leads into `Cellar/node@22`.
        let root = prefix("old-version");
        let old = root.join("Cellar/node@22/22.23.2_2/bin");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("node"), b"").unwrap();
        symlink(
            "../Cellar/node@22/22.23.2_2/bin/node",
            root.join("bin/node"),
        )
        .unwrap();
        std::fs::remove_file(root.join("bin/npm")).unwrap();
        let found = link_preview(&root, "node@22").in_the_way;
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, vec![root.join("bin/node")]);
    }

    #[test]
    fn test_a_plain_file_is_in_the_way_and_a_formula_with_no_keg_says_nothing() {
        let root = prefix("plain-file");
        std::fs::write(root.join("bin/corepack"), b"#!/bin/sh\n").unwrap();
        // A `.DS_Store` in the keg's `bin` is no command: Homebrew skips it.
        std::fs::write(root.join("Cellar/node@22/22.23.3_1/bin/.DS_Store"), b"").unwrap();
        let found = link_preview(&root, "node@22");
        let none = link_preview(&root, "node@20");
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            found.in_the_way,
            vec![root.join("bin/corepack"), root.join("bin/npm")]
        );
        assert_eq!(found.commands, ["corepack", "node", "npm"]);
        assert_eq!(none, LinkPreview::default());
    }
}
