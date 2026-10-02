//! `resolve` for one round of judging which copy a command runs
//! (`commands::judge`): the same answer for every path, but each folder
//! and name on the way looked up once in the round instead of once per
//! path. A round asks where a few thousand paths lead that share a few
//! dozen folders (`<prefix>/bin`, `Cellar`, `~/.local/bin`, ...); walking
//! each from `/` reopened those folders every time.
//!
//! What it keeps, for one round only (a `Round` is made per judgement
//! and dropped with it, never shared between rounds or threads):
//!
//! - the folders it opened more than once, open (`Held`, at most
//!   `MAX_HELD`), each reached as `resolve` reaches a folder: from the
//!   folder before it, held open, with `O_NOFOLLOW` and checked to be the
//!   folder looked at. A path below one starts there, as `resolve`'s walk
//!   from `/` would have reached it through the same folders;
//! - what each name it looked at is (`fstatat` without following), and
//!   each link's text, by the path it was looked at as -- exact case, as a
//!   disk that tells case apart may hold both spellings.
//!
//! Every name is checked against the protected places before it is first
//! looked at, as in `resolve`; a cached answer is only ever for a name
//! that was checked. Where several names are left, none of them `..`,
//! none looked at yet and none in a protected place, the kernel is asked
//! for the last of them in one lookup that follows no link anywhere
//! (`Dir::stat_beneath`): a file or folder at the end is what `resolve`
//! would find through those folders; anything else -- a link at the end
//! or on the way, an unreadable folder -- is taken one step at a time,
//! as `resolve` takes it.
//!
//! Always follows the last link (`resolve`'s `follow_last`), as judging
//! does.

use super::{names, Protected, Resolution, MAX_LINKS};
use crate::dirfd::{Dir, Stat};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The most folders a round keeps open between paths: enough for every
/// `PATH` folder, bin folder and the folders above them on a busy Mac,
/// few beside the 256 descriptors an app opened from Finder may have.
/// Past it, a folder is open only while a path is walked through it, as
/// in `resolve`.
const MAX_HELD: usize = 64;

/// One folder open for search, at `path` -- every name on the way a
/// folder looked at, no link -- and the folder it was opened from.
struct Held {
    path: PathBuf,
    dir: Dir,
    parent: Option<Rc<Held>>,
}

/// What asking for the rest of a path in one lookup came to.
enum Beneath {
    Answer(Resolution),
    /// Not asked: it could not be asked in one (a `..`, a protected
    /// place, a name already looked at, one name left).
    NotAsked,
    /// Asked, and the kernel could not say: one step at a time says.
    Unanswered,
}

/// What one name in a folder was when it was looked at.
#[derive(Clone, Copy)]
enum Entry {
    Is(Stat),
    Missing,
    Refused,
    Protected,
}

pub(crate) struct Round {
    protected: Protected,
    root: Option<Rc<Held>>,
    /// The folders kept open, by path.
    held: HashMap<PathBuf, Rc<Held>>,
    /// Every folder opened so far: one opened again is kept open.
    opened: HashSet<PathBuf>,
    /// What each name looked at is, by the path it was looked at as.
    entries: HashMap<PathBuf, Entry>,
    /// Each link's text, by its path; `None` when it could not be read.
    links: HashMap<PathBuf, Option<PathBuf>>,
    /// Whether the rest of a path may be asked for in one lookup: always,
    /// but for a test that takes every step one at a time, as on a
    /// kernel without the flag.
    in_one: bool,
}

impl Round {
    pub(crate) fn new(protected: Protected) -> Round {
        Round {
            protected,
            root: None,
            held: HashMap::new(),
            opened: HashSet::new(),
            entries: HashMap::new(),
            links: HashMap::new(),
            in_one: true,
        }
    }

    /// A round that takes every step one at a time.
    #[cfg(test)]
    fn one_step_at_a_time(protected: Protected) -> Round {
        Round {
            in_one: false,
            ..Round::new(protected)
        }
    }

    /// The places this round never looks into.
    #[cfg(test)]
    pub(crate) fn protected(&self) -> &Protected {
        &self.protected
    }

    fn root(&mut self) -> Option<Rc<Held>> {
        if self.root.is_none() {
            self.root = Some(Rc::new(Held {
                path: PathBuf::from("/"),
                dir: Dir::root().ok()?,
                parent: None,
            }));
        }
        self.root.clone()
    }

    /// What `resolve(path, protected, true)` answers, from what this
    /// round has already looked at where it can.
    pub(crate) fn resolve(&mut self, path: &Path) -> Resolution {
        if !path.is_absolute() {
            return Resolution::Refused;
        }
        // A name already looked at, through folders only, that is no
        // link: where a path to a file ends, asked for again to see
        // whether it can be run. Answered as `resolve` spells it (`/a//b`
        // is `/a/b`).
        if let Some((known, Entry::Is(stat))) = self.entries.get_key_value(path) {
            if !stat.is_symlink() {
                return Resolution::Found(known.clone(), *stat);
            }
        }
        // The deepest folder held open that the path names on its way:
        // `resolve` would reach it from `/` through the same folders.
        let start = path
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .find_map(|prefix| self.held.get(prefix).cloned());
        let Some(mut here) = start.or_else(|| self.root()) else {
            return Resolution::Refused;
        };
        let Ok(rest) = path.strip_prefix(&here.path) else {
            return Resolution::Refused;
        };
        let mut pending: VecDeque<OsString> = names(rest).collect();
        let mut found: Option<(PathBuf, Stat)> = None;
        let mut links = 0;
        // Whether the names left may still be asked for in one lookup:
        // not again once the kernel was asked and could not say, until a
        // link's text gives new names.
        let mut in_one = self.in_one;
        while let Some(name) = pending.pop_front() {
            #[cfg(test)]
            super::STEP.with(|step| {
                if let Some(step) = step.borrow_mut().as_mut() {
                    step(&here.path);
                }
            });
            if name == ".." {
                if let Some(parent) = &here.parent {
                    here = parent.clone();
                }
                found = None;
                continue;
            }
            let candidate = here.path.join(&name);
            if in_one {
                match self.beneath(&here, &name, &candidate, &pending) {
                    Beneath::Answer(answer) => return answer,
                    Beneath::NotAsked => {}
                    Beneath::Unanswered => in_one = false,
                }
            }
            let stat = match self.look_at(&here, &name, &candidate) {
                Entry::Is(stat) => stat,
                Entry::Missing => return Resolution::Missing,
                Entry::Refused => return Resolution::Refused,
                Entry::Protected => {
                    let mut at = candidate;
                    for name in pending {
                        if name == ".." {
                            at.pop();
                        } else {
                            at.push(name);
                        }
                    }
                    return Resolution::Protected(at);
                }
            };
            if stat.is_symlink() {
                links += 1;
                if links > MAX_LINKS {
                    return Resolution::Refused;
                }
                let Some(target) = self.link_text(&here, &name, &candidate) else {
                    return Resolution::Refused;
                };
                if target.is_absolute() {
                    let Some(root) = self.root() else {
                        return Resolution::Refused;
                    };
                    here = root;
                }
                let spliced: Vec<OsString> = names(&target).collect();
                for name in spliced.into_iter().rev() {
                    pending.push_front(name);
                }
                found = None;
                in_one = self.in_one;
                continue;
            }
            if !pending.is_empty() {
                // A folder on the way. Not a folder, or not one this may
                // search, is where the kernel's own lookup stops too.
                if !stat.is_dir() {
                    return Resolution::Refused;
                }
                match self.enter(&here, &name, &candidate, &stat) {
                    Some(next) => here = next,
                    None => return Resolution::Refused,
                }
            }
            found = Some((candidate, stat));
        }
        match found {
            Some((at, stat)) => Resolution::Found(at, stat),
            // Ended on `..`, on a link to a folder already open, or is
            // the root: the folder reached, open.
            None => match here.dir.stat() {
                Ok(stat) => Resolution::Found(here.path.clone(), stat),
                Err(_) => Resolution::Refused,
            },
        }
    }

    /// The answer, in one lookup that follows no link, when `name` and
    /// every name `pending` holds after it are plain names, none of them
    /// in a protected place, and they end on a file or folder reached
    /// through folders only -- or are not there. Not asked when one step
    /// costs nothing: `name` is a folder held open, or was looked at and
    /// is no folder. Otherwise the walk takes `name` one step at a time,
    /// which answers the rest: a link at the end or on the way, a name
    /// that is not a folder, a folder this may not search, a kernel
    /// without the flag.
    fn beneath(
        &mut self,
        here: &Held,
        name: &OsStr,
        candidate: &Path,
        pending: &VecDeque<OsString>,
    ) -> Beneath {
        if pending.is_empty() || pending.iter().any(|name| name == "..") {
            return Beneath::NotAsked;
        }
        match self.entries.get(candidate) {
            None => {}
            Some(Entry::Is(stat)) if stat.is_dir() && !self.held.contains_key(candidate) => {}
            Some(_) => return Beneath::NotAsked,
        }
        // Checked before the lookup is made, as one step at a time checks
        // each name: the last one outside every place, so is every
        // folder on its way (`Protected::contains`).
        let mut at = candidate.to_path_buf();
        at.extend(pending);
        if self.protected.contains(&at) {
            return Beneath::NotAsked;
        }
        debug_assert!(
            at.ancestors()
                .all(|folder| !self.protected.contains(folder)),
            "{at:?}"
        );
        // Looked at before, through folders only: as the lookup would
        // answer, or a link, which it would not.
        match self.entries.get(&at) {
            Some(Entry::Is(stat)) if stat.is_symlink() => return Beneath::Unanswered,
            Some(Entry::Is(stat)) => return Beneath::Answer(Resolution::Found(at, *stat)),
            _ => {}
        }
        let mut all: Vec<OsString> = Vec::with_capacity(pending.len() + 1);
        all.push(name.to_os_string());
        all.extend(pending.iter().cloned());
        match here.dir.stat_beneath(&all) {
            // A link at the end: followed one step at a time, from where
            // its text is read.
            Ok(stat) if stat.is_symlink() => {
                self.entries.insert(at, Entry::Is(stat));
                Beneath::Unanswered
            }
            Ok(stat) => {
                self.entries.insert(at.clone(), Entry::Is(stat));
                Beneath::Answer(Resolution::Found(at, stat))
            }
            // Every name before the missing one was a folder, no link,
            // that this may search: one step at a time stops at that
            // name too.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Beneath::Answer(Resolution::Missing)
            }
            Err(_) => Beneath::Unanswered,
        }
    }

    /// What `name` in `here` is: as looked at before in this round, or
    /// checked against the protected places and then looked at.
    fn look_at(&mut self, here: &Held, name: &OsStr, candidate: &Path) -> Entry {
        if let Some(known) = self.entries.get(candidate) {
            return *known;
        }
        let entry = if self.protected.contains(candidate) {
            Entry::Protected
        } else {
            match here.dir.stat_at(name) {
                Ok(stat) => Entry::Is(stat),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Entry::Missing,
                Err(_) => Entry::Refused,
            }
        };
        self.entries.insert(candidate.to_path_buf(), entry);
        entry
    }

    /// The text of the link `name` in `here`, read once in this round.
    fn link_text(&mut self, here: &Held, name: &OsStr, candidate: &Path) -> Option<PathBuf> {
        if let Some(known) = self.links.get(candidate) {
            return known.clone();
        }
        let text = here.dir.read_link_at(name).ok();
        self.links.insert(candidate.to_path_buf(), text.clone());
        text
    }

    /// The folder `name` in `here`, just looked at (`stat`): the one held
    /// open, or opened from `here` as `resolve` opens it -- and kept open
    /// when it is opened for the second time this round.
    fn enter(
        &mut self,
        here: &Rc<Held>,
        name: &OsStr,
        candidate: &Path,
        stat: &Stat,
    ) -> Option<Rc<Held>> {
        if let Some(held) = self.held.get(candidate) {
            return Some(held.clone());
        }
        let (dir, _) = here.dir.open_dir_at(name, Some(stat), false).ok()?;
        let next = Rc::new(Held {
            path: candidate.to_path_buf(),
            dir,
            parent: Some(here.clone()),
        });
        let again = !self.opened.insert(candidate.to_path_buf());
        if again && self.held.len() < MAX_HELD {
            self.held.insert(candidate.to_path_buf(), next.clone());
        }
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{resolve, DATA_VOLUME, PROTECTED_IN_HOME, STEP};
    use super::*;
    use crate::dirfd::calls;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    /// A fresh folder, canonical (`/var` is a link on a Mac), removed with
    /// everything in it -- folders locked by the test unlocked first.
    struct Tree {
        root: PathBuf,
        locked: Vec<PathBuf>,
    }

    impl Tree {
        fn new(tag: &str) -> Tree {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let raw = std::env::temp_dir().join(format!(
                "banager-round-{tag}-{}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&raw).unwrap();
            Tree {
                root: fs::canonicalize(raw).unwrap(),
                locked: Vec::new(),
            }
        }

        fn at(&self, rel: &str) -> PathBuf {
            self.root.join(rel)
        }

        fn dir(&self, rel: &str) -> PathBuf {
            let path = self.at(rel);
            fs::create_dir_all(&path).unwrap();
            path
        }

        fn file(&self, rel: &str, mode: u32) -> PathBuf {
            let path = self.at(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"never run").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            path
        }

        fn link(&self, rel: &str, target: impl AsRef<Path>) -> PathBuf {
            let path = self.at(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(target, &path).unwrap();
            path
        }

        fn lock(&mut self, rel: &str, mode: u32) {
            let path = self.at(rel);
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            self.locked.push(path);
        }

        /// `rel` under the tree, spelled from the data volume.
        fn on_data_volume(&self, rel: &str) -> PathBuf {
            Path::new(DATA_VOLUME)
                .join(self.root.strip_prefix("/").unwrap())
                .join(rel)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            for path in self.locked.iter().rev() {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// `old` and `new` are the same answer: the same path, byte for byte,
    /// and the same `fstatat` fields.
    fn assert_same(old: &Resolution, new: &Resolution, asked: &Path) {
        let same = match (old, new) {
            (Resolution::Found(a, sa), Resolution::Found(b, sb)) => {
                a.as_os_str() == b.as_os_str() && sa == sb
            }
            (Resolution::Protected(a), Resolution::Protected(b)) => a.as_os_str() == b.as_os_str(),
            (Resolution::Missing, Resolution::Missing)
            | (Resolution::Refused, Resolution::Refused) => true,
            _ => false,
        };
        assert!(same, "{asked:?}: resolve {old:?}, round {new:?}");
    }

    #[test]
    fn test_a_round_answers_as_resolve_does_through_links_limits_and_dotdot() {
        let tree = Tree::new("links");
        tree.dir("bin/sub");
        tree.file("bin/tool", 0o755);
        for i in 0..34 {
            let target = if i == 0 {
                "bin".to_string()
            } else {
                format!("link{}", i - 1)
            };
            tree.link(&format!("link{i}"), target);
        }
        tree.link("cycle", "cycle");
        tree.link("private", "Documents/../bin/tool");
        tree.link("to-root", "/");
        tree.link("here", ".");
        let protected = Protected::new(&tree.root);
        let mut round = Round::new(protected.clone());
        for pass in 0..3 {
            for rel in [
                "bin/tool",
                "bin/sub/../tool",
                "bin/sub/..",
                "bin//tool",
                "bin/./tool",
                "bin/tool/",
                "link0/tool",
                "link30/tool",
                "link31/tool",
                "link32/tool",
                "link33/tool",
                "link31/../link0/tool",
                "cycle",
                "private",
                "missing/../bin/tool",
                "bin/tool/../tool",
                "Documents/../bin/tool",
                "dOcUmEnTs/tool",
                "bin/../../..",
                "here/bin/tool",
                "to-root",
            ] {
                let path = tree.at(rel);
                let old = resolve(&path, &protected, true);
                assert_same(&old, &round.resolve(&path), &path);
                assert_same(&old, &Round::new(protected.clone()).resolve(&path), &path);
            }
            // The kernel's limit holds however much was cached before.
            assert!(
                matches!(
                    round.resolve(&tree.at("link31/tool")),
                    Resolution::Found(..)
                ),
                "pass {pass}"
            );
            assert!(
                matches!(round.resolve(&tree.at("link32/tool")), Resolution::Refused),
                "pass {pass}"
            );
        }
        for path in ["/", "/.."] {
            let path = Path::new(path);
            assert_same(&resolve(path, &protected, true), &round.resolve(path), path);
        }
        assert!(matches!(
            round.resolve(Path::new("relative")),
            Resolution::Refused
        ));
    }

    #[test]
    fn test_a_round_never_looks_at_anything_in_a_protected_place() {
        let tree = Tree::new("protected");
        tree.dir("bin");
        let mut asked = Vec::new();
        for (i, place) in PROTECTED_IN_HOME.iter().enumerate() {
            // Something there, so that a look inside would find it.
            tree.file(&format!("{place}/bin/tool"), 0o755);
            let lower = place.to_ascii_lowercase();
            let upper = place.to_ascii_uppercase();
            asked.extend([
                tree.at(&format!("{place}/bin/tool")),
                tree.at(&format!("{lower}/bin/tool")),
                tree.on_data_volume(&format!("{upper}/bin/tool")),
                tree.link(&format!("bin/rel{i}"), format!("../{place}/bin/tool")),
                tree.link(
                    &format!("bin/abs{i}"),
                    tree.at(&format!("{lower}/bin/tool")),
                ),
                tree.link(
                    &format!("bin/data{i}"),
                    tree.on_data_volume(&format!("{place}/bin/tool")),
                ),
                tree.link(&format!("bin/chain{i}"), format!("rel{i}")),
                tree.link(&format!("bin/dir{i}"), format!("../{place}/bin"))
                    .join("tool"),
                tree.link(&format!("bin/back{i}"), format!("../{place}/../bin/rel{i}")),
            ]);
        }
        asked.extend([
            PathBuf::from("/Volumes/banager-no-such-disk/bin/tool"),
            PathBuf::from("/volumes/banager-no-such-disk"),
            Path::new(DATA_VOLUME).join("Volumes/banager-no-such-disk/bin/tool"),
            tree.link("bin/disk", "/Volumes/banager-no-such-disk/bin/tool"),
        ]);
        let protected = Protected::new(&tree.root);
        let mut round = Round::new(protected.clone());
        // The folders around them looked up and held first, as in a
        // judgement.
        for _ in 0..2 {
            round.resolve(&tree.at("bin/missing"));
        }
        let mut looked = 0;
        for _pass in 0..2 {
            for path in &asked {
                let (new, calls) = calls::measure(|| round.resolve(path));
                assert!(matches!(new, Resolution::Protected(_)), "{path:?}: {new:?}");
                assert_same(&resolve(path, &protected, true), &new, path);
                for (call, at) in &calls.paths {
                    for at in at.ancestors() {
                        assert!(
                            !protected.contains(at),
                            "{call:?} looked at {at:?} for {path:?}"
                        );
                    }
                }
                looked += calls.total();
            }
        }
        assert!(looked > 0, "the links were looked at");
    }

    #[test]
    fn test_a_folder_held_open_is_used_after_it_is_replaced_by_a_link() {
        let tree = Tree::new("swap");
        tree.dir("a/sub");
        tree.dir("Documents/sub");
        tree.file("a/tool", 0o755);
        tree.file("Documents/tool", 0o755);
        let inode = fs::metadata(tree.at("a/tool")).unwrap().ino();
        let mut round = Round::new(Protected::new(&tree.root));
        // Opened twice, one step at a time (a `..` is never asked for in
        // one lookup): `a` and `a/sub` are held, `tool` not looked at.
        for _ in 0..2 {
            round.resolve(&tree.at("a/sub/../sub/missing"));
        }
        fs::rename(tree.at("a"), tree.at("aside")).unwrap();
        symlink("Documents", tree.at("a")).unwrap();
        // Back up from the folder held open, never through the link.
        let found = round.resolve(&tree.at("a/sub/../tool"));
        assert!(
            matches!(&found, Resolution::Found(_, stat) if stat.ino() == inode),
            "{found:?}"
        );
        // A new round sees the link, and stops before the place.
        assert!(matches!(
            Round::new(Protected::new(&tree.root)).resolve(&tree.at("a/tool")),
            Resolution::Protected(_)
        ));
    }

    #[test]
    fn test_a_round_looks_each_shared_folder_up_once() {
        // Homebrew's layout: `bin/<name>` → `../Cellar/<name>/<version>/bin/<name>`.
        let tree = Tree::new("shared");
        let count = 40;
        for i in 0..count {
            tree.file(&format!("brew/Cellar/f{i}/1.0/bin/f{i}"), 0o755);
            tree.link(
                &format!("brew/bin/f{i}"),
                format!("../Cellar/f{i}/1.0/bin/f{i}"),
            );
        }
        // One more keg, linked once the others were looked up.
        tree.file("brew/Cellar/h/1.0/bin/h", 0o755);
        let protected = Protected::new(&tree.root);
        let paths: Vec<PathBuf> = (0..count)
            .map(|i| tree.at(&format!("brew/bin/f{i}")))
            .collect();
        let (old, before) = calls::measure(|| {
            paths
                .iter()
                .map(|path| resolve(path, &protected, true))
                .collect::<Vec<_>>()
        });
        let mut round = Round::new(protected.clone());
        let (new, after) = calls::measure(|| {
            paths
                .iter()
                .map(|path| round.resolve(path))
                .collect::<Vec<_>>()
        });
        for ((old, new), path) in old.iter().zip(&new).zip(&paths) {
            assert!(matches!(old, Resolution::Found(..)), "{old:?}");
            assert_same(old, new, path);
        }
        // `/` once for the round, not once per path.
        assert_eq!(before.root, count);
        assert_eq!(after.root, 1);
        // Once `brew/bin` is held: the link, its text, and the rest of the
        // way in one lookup from `brew`, the folder `brew/bin` was opened
        // from.
        tree.link("brew/bin/g", "../Cellar/h/1.0/bin/h");
        let (next, three) = calls::measure(|| round.resolve(&tree.at("brew/bin/g")));
        assert!(matches!(next, Resolution::Found(..)), "{next:?}");
        assert_eq!(
            (
                three.stat_at,
                three.read_link_at,
                three.stat_beneath,
                three.total()
            ),
            (1, 1, 1, 3),
            "{three:?}"
        );
        // A name already looked at through folders only is not asked for
        // again.
        let (again, none) = calls::measure(|| round.resolve(&tree.at("brew/Cellar/h/1.0/bin/h")));
        assert!(matches!(again, Resolution::Found(..)), "{again:?}");
        assert_eq!(none.total(), 0, "{none:?}");
        assert!(
            after.total() * 4 < before.total(),
            "before {before:?}, after {after:?}"
        );
    }

    /// Reproducible numbers, with no new dependency.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }

        fn pick<'a>(&mut self, items: &'a [String]) -> &'a str {
            &items[self.below(items.len())]
        }

        /// `rel` with one of its names, picked at random, in capitals.
        fn shout(&mut self, rel: &str) -> String {
            let mut names: Vec<String> = rel.split('/').map(str::to_string).collect();
            let at = self.below(names.len());
            names[at] = names[at].to_ascii_uppercase();
            names.join("/")
        }
    }

    /// A tree of folders, files and links of every kind `resolve` meets
    /// -- relative and absolute, chains and loops, through `..`, spelled
    /// from the data volume or in another case, into protected places,
    /// nowhere, through folders that cannot be searched or read -- and
    /// paths to ask about it.
    fn random_tree(seed: u64) -> (Tree, Vec<PathBuf>) {
        let mut rng = Rng(seed);
        let mut tree = Tree::new("random");
        let mut dirs: Vec<String> = [
            "a",
            "a/b",
            "a/b/c",
            "x",
            "x/y",
            "Library",
            "Library/Application Support",
            "locked",
            "locked/in",
            "searchonly",
            "readonly",
        ]
        .iter()
        .map(|d| d.to_string())
        .collect();
        for i in 0..rng.below(4) {
            let parent = rng.pick(&dirs).to_string();
            dirs.push(format!("{parent}/n{i}"));
        }
        for dir in &dirs {
            tree.dir(dir);
            tree.file(&format!("{dir}/f"), 0o755);
            tree.file(&format!("{dir}/g"), 0o644);
        }
        let protected_rel: Vec<String> = [
            "Documents/p",
            "Library/Mobile Documents/q",
            "Library/Containers/r",
        ]
        .iter()
        .map(|d| d.to_string())
        .collect();
        for dir in &protected_rel {
            tree.file(&format!("{dir}/f"), 0o755);
        }
        let mut places: Vec<String> = dirs.clone();
        places.extend(dirs.iter().map(|d| format!("{d}/f")));
        places.extend(dirs.iter().map(|d| format!("{d}/g")));
        let homes: Vec<String> = dirs
            .iter()
            .filter(|d| !d.starts_with("locked") && *d != "readonly")
            .cloned()
            .collect();
        let mut links: Vec<String> = Vec::new();
        for i in 0..24 + rng.below(16) {
            let home = rng.pick(&homes).to_string();
            let name = format!("{home}/l{i}");
            let up = "../".repeat(home.split('/').count());
            let target: PathBuf = match rng.below(14) {
                0 | 1 => format!("{up}{}", rng.pick(&places)).into(),
                2 => tree.at(rng.pick(&places)),
                3 => tree.on_data_volume(rng.pick(&places)),
                4 => {
                    let place = rng.pick(&places).to_string();
                    tree.at(&rng.shout(&place))
                }
                5 => format!("{up}{}/f", rng.pick(&protected_rel)).into(),
                6 => {
                    let place = rng.pick(&protected_rel).to_string();
                    tree.on_data_volume(&rng.shout(&place))
                }
                7 => "/Volumes/banager-no-such-disk/f".into(),
                8 => format!("{up}nowhere/f").into(),
                9 if !links.is_empty() => format!("{up}{}", rng.pick(&links)).into(),
                10 if !links.is_empty() => tree.at(rng.pick(&links)),
                11 => format!("l{i}").into(),
                12 => format!("{up}locked/in/f").into(),
                _ => format!("{up}a/b/../../{}", rng.pick(&places)).into(),
            };
            tree.link(&name, target);
            links.push(name);
        }
        tree.lock("locked", 0o000);
        tree.lock("searchonly", 0o111);
        tree.lock("readonly", 0o444);
        let mut asked: Vec<PathBuf> = Vec::new();
        for rel in places.iter().chain(&links) {
            asked.push(tree.at(rel));
            asked.push(tree.on_data_volume(rel));
            asked.push(tree.at(&rng.shout(rel)));
        }
        let mut words: Vec<String> = dirs
            .iter()
            .chain(&links)
            .map(|rel| rel.rsplit('/').next().unwrap().to_string())
            .collect();
        words.extend(
            [
                "..",
                ".",
                "",
                "f",
                "g",
                "nowhere",
                "Documents",
                "documents",
                "Library",
                "Mobile Documents",
                "in",
            ]
            .iter()
            .map(|w| w.to_string()),
        );
        for _ in 0..120 {
            let mut rel = rng.pick(&homes).to_string();
            for _ in 0..1 + rng.below(5) {
                rel.push('/');
                rel.push_str(rng.pick(&words));
            }
            asked.push(if rng.below(4) == 0 {
                tree.on_data_volume(&rel)
            } else {
                tree.at(&rel)
            });
        }
        (tree, asked)
    }

    #[test]
    fn test_a_round_answers_as_resolve_does_on_random_trees() {
        let mut kinds = HashSet::new();
        // Lookups made in one, and folders opened one step at a time.
        let mut calls_made = (0, 0);
        for seed in 1..=12u64 {
            let (tree, asked) = random_tree(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let protected = Protected::new(&tree.root);
            // One round for every path, as a judgement asks them, twice
            // over; one that takes every step one at a time, as on a
            // kernel without `AT_SYMLINK_NOFOLLOW_ANY`; and a fresh one
            // for each.
            let mut round = Round::new(protected.clone());
            let mut stepping = Round::one_step_at_a_time(protected.clone());
            for pass in 0..2 {
                for path in &asked {
                    let old = resolve(path, &protected, true);
                    let (new, made) = calls::measure(|| round.resolve(path));
                    assert_same(&old, &new, path);
                    calls_made.0 += made.stat_beneath;
                    calls_made.1 += made.open_dir_at;
                    let (stepped, made) = calls::measure(|| stepping.resolve(path));
                    assert_same(&old, &stepped, path);
                    assert_eq!(made.stat_beneath, 0);
                    if pass == 0 {
                        assert_same(&old, &Round::new(protected.clone()).resolve(path), path);
                    }
                    kinds.insert(match old {
                        Resolution::Found(_, stat) if stat.is_dir() => "folder",
                        Resolution::Found(..) => "file",
                        Resolution::Protected(_) => "protected",
                        Resolution::Missing => "missing",
                        Resolution::Refused => "refused",
                    });
                }
            }
            drop(tree);
        }
        assert_eq!(
            kinds,
            HashSet::from(["folder", "file", "protected", "missing", "refused"])
        );
        assert!(calls_made.0 > 0 && calls_made.1 > 0, "{calls_made:?}");
    }

    #[test]
    fn test_a_round_steps_from_a_folder_it_holds_rather_than_from_the_root() {
        // The test hook sees the folders a walk steps from.
        let tree = Tree::new("steps");
        tree.file("a/b/tool", 0o755);
        tree.link("a/link", "b/tool");
        let protected = Protected::new(&tree.root);
        let mut round = Round::new(protected);
        for _ in 0..2 {
            round.resolve(&tree.at("a/link"));
        }
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let record = seen.clone();
        STEP.with(|step| {
            *step.borrow_mut() = Some(Box::new(move |at: &Path| {
                record.borrow_mut().push(at.to_path_buf());
            }))
        });
        let found = round.resolve(&tree.at("a/link"));
        STEP.with(|step| *step.borrow_mut() = None);
        assert!(matches!(found, Resolution::Found(..)), "{found:?}");
        // From `a`, held: the link (its text known), then `b/tool` from
        // the same folder, answered as before.
        assert_eq!(*seen.borrow(), vec![tree.at("a"), tree.at("a")]);
    }
}
