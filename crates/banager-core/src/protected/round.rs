//! `resolve` for one round of judging which copy a command runs
//! (`commands::judge`): the same answer for every path, from what the
//! round has already looked at where it can, instead of a walk from `/`
//! for each path. A round asks where a few thousand paths lead that share
//! a few dozen folders (`<prefix>/bin`, `Cellar`, `~/.local/bin`, ...);
//! walking each from `/` reopened those folders every time.
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
//!   disk that tells case apart may hold both spellings: each name
//!   `lstat`ed and each link read at most once per spelling.
//!
//! Every name is checked against the protected places before it is first
//! looked at, as in `resolve`; a cached answer is only ever for a name
//! that was checked. Where several names are left, none of them `..` and
//! none in a protected place, and the first is no folder held open, the
//! kernel is asked for the last of them in one lookup that follows no
//! link anywhere (`Dir::stat_beneath`), looking each folder on the way up
//! again: a file or folder at the end is what `resolve` would find
//! through those folders; anything else -- a link at the end or on the
//! way, an unreadable folder -- is taken one step at a time, as `resolve`
//! takes it.
//!
//! A folder opened before follows its folder if another program renames
//! it: before each time one is used again, it is asked where it now is
//! (`still_outside`), and one now inside a protected place is not used --
//! the round lets go of everything it kept and walks from `/`.
//!
//! Always follows the last link (`resolve`'s `follow_last`), as judging
//! does.

use super::{names, Protected, Resolution, MAX_LINKS};
use crate::dirfd::{Dir, Stat};
use std::cell::RefCell;
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
    /// Where the kernel last said it is, when that was outside every
    /// protected place.
    was_at: RefCell<Option<PathBuf>>,
}

impl Held {
    fn new(path: PathBuf, dir: Dir, parent: Option<Rc<Held>>) -> Held {
        #[cfg(test)]
        OPEN.with(|open| open.set(open.get() + 1));
        Held {
            path,
            dir,
            parent,
            was_at: RefCell::new(None),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// How many `Held` folders are open on this thread: what a test holds
    /// the descriptors a round keeps to.
    static OPEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Drop for Held {
    fn drop(&mut self) {
        OPEN.with(|open| open.set(open.get() - 1));
    }
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
    /// Whether checking the whole rest stands for checking each folder on
    /// its way (`Protected::one_check_covers_the_way`), asked once.
    one_check: bool,
}

impl Round {
    pub(crate) fn new(protected: Protected) -> Round {
        Round {
            one_check: protected.one_check_covers_the_way(),
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
            self.root = Some(Rc::new(Held::new(
                PathBuf::from("/"),
                Dir::root().ok()?,
                None,
            )));
        }
        self.root.clone()
    }

    /// What `resolve(path, protected, true)` answers, from what this
    /// round has already looked at where it can.
    pub(crate) fn resolve(&mut self, path: &Path) -> Resolution {
        match self.walk(path) {
            Some(found) => found,
            // A folder held open is now in a protected place, or cannot
            // say where it is (`still_outside`): nothing this round kept
            // is used again, and the path is walked from `/`, as
            // `resolve` would walk it.
            None => {
                self.forget();
                self.walk(path).unwrap_or(Resolution::Refused)
            }
        }
    }

    /// Everything this round kept, let go: the folders held open (`/`
    /// aside), and what each name and link was.
    fn forget(&mut self) {
        self.held.clear();
        self.opened.clear();
        self.entries.clear();
        self.links.clear();
    }

    /// `resolve`'s answer, or `None` when a folder this round opened
    /// before, about to be used again, has been moved into a protected
    /// place since (`still_outside`).
    fn walk(&mut self, path: &Path) -> Option<Resolution> {
        if !path.is_absolute() {
            return Some(Resolution::Refused);
        }
        // A name already looked at, through folders only, that is no
        // link: where a path to a file ends, asked for again to see
        // whether it can be run. Answered as `resolve` spells it (`/a//b`
        // is `/a/b`).
        if let Some((known, Entry::Is(stat))) = self.entries.get_key_value(path) {
            if !stat.is_symlink() {
                return Some(Resolution::Found(known.clone(), *stat));
            }
        }
        // The deepest folder held open that the path names on its way:
        // `resolve` would reach it from `/` through the same folders.
        let start = path
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .find_map(|prefix| self.held.get(prefix).cloned());
        if let Some(held) = &start {
            if !self.still_outside(held) {
                return None;
            }
        }
        let Some(mut here) = start.or_else(|| self.root()) else {
            return Some(Resolution::Refused);
        };
        let Ok(rest) = path.strip_prefix(&here.path) else {
            return Some(Resolution::Refused);
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
                if let Some(parent) = here.parent.clone() {
                    if !self.still_outside(&parent) {
                        return None;
                    }
                    here = parent;
                }
                found = None;
                continue;
            }
            let candidate = here.path.join(&name);
            if in_one {
                match self.beneath(&here, &name, &candidate, &pending) {
                    Beneath::Answer(answer) => return Some(answer),
                    Beneath::NotAsked => {}
                    Beneath::Unanswered => in_one = false,
                }
            }
            let stat = match self.look_at(&here, &name, &candidate) {
                Entry::Is(stat) => stat,
                Entry::Missing => return Some(Resolution::Missing),
                Entry::Refused => return Some(Resolution::Refused),
                Entry::Protected => {
                    let mut at = candidate;
                    for name in pending {
                        if name == ".." {
                            at.pop();
                        } else {
                            at.push(name);
                        }
                    }
                    return Some(Resolution::Protected(at));
                }
            };
            if stat.is_symlink() {
                links += 1;
                if links > MAX_LINKS {
                    return Some(Resolution::Refused);
                }
                let Some(target) = self.link_text(&here, &name, &candidate) else {
                    return Some(Resolution::Refused);
                };
                if target.is_absolute() {
                    let Some(root) = self.root() else {
                        return Some(Resolution::Refused);
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
                    return Some(Resolution::Refused);
                }
                here = match self.held.get(&candidate).cloned() {
                    Some(held) if self.still_outside(&held) => held,
                    Some(_) => return None,
                    None => match self.enter(&here, &name, &candidate, &stat) {
                        Some(next) => next,
                        None => return Some(Resolution::Refused),
                    },
                };
            }
            found = Some((candidate, stat));
        }
        Some(match found {
            Some((at, stat)) => Resolution::Found(at, stat),
            // Ended on `..`, on a link to a folder already open, or is
            // the root: the folder reached, open.
            None => match here.dir.stat() {
                Ok(stat) => Resolution::Found(here.path.clone(), stat),
                Err(_) => Resolution::Refused,
            },
        })
    }

    /// Whether `held`, a folder this round opened before and is about to
    /// use again, is still outside every protected place: asked of the
    /// folder itself (`Dir::path`, `F_GETPATH`), which looks nothing up
    /// and reads nothing in it. A folder another program renamed into
    /// `~/Documents` since it was opened is there now, whatever path this
    /// round knows it by. Asked each time one is used again -- a held one
    /// at the start of a path or stepped into, the one a `..` goes back
    /// to -- as `resolve`, which asks nothing, keeps its folders for one
    /// path only; `/` never moves.
    fn still_outside(&self, held: &Held) -> bool {
        if held.parent.is_none() {
            return true;
        }
        match held.dir.path() {
            Ok(at) => {
                let mut was_at = held.was_at.borrow_mut();
                // Where it was last time, and outside then: outside now.
                if was_at
                    .as_ref()
                    .is_some_and(|was| was.as_os_str() == at.as_os_str())
                {
                    return true;
                }
                let outside = !self.protected.contains(&at);
                *was_at = outside.then_some(at);
                outside
            }
            // No such call off a Mac, where no place asks permission.
            Err(e) => e.kind() == std::io::ErrorKind::Unsupported,
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
        // each name: with the last one outside every place, so is every
        // folder on its way (`Protected::one_check_covers_the_way`, which
        // holds for every home folder); else each folder is checked.
        let mut at = candidate.to_path_buf();
        at.extend(pending);
        if self.protected.contains(&at) {
            return Beneath::NotAsked;
        }
        if self.one_check {
            debug_assert!(
                at.ancestors()
                    .all(|folder| !self.protected.contains(folder)),
                "{at:?}"
            );
        } else if at
            .ancestors()
            .skip(1)
            .take(pending.len())
            .any(|folder| self.protected.contains(folder))
        {
            return Beneath::NotAsked;
        }
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

    /// The folder `name` in `here`, just looked at (`stat`), opened from
    /// `here` as `resolve` opens it -- and kept open when it is opened for
    /// the second time this round.
    fn enter(
        &mut self,
        here: &Rc<Held>,
        name: &OsStr,
        candidate: &Path,
        stat: &Stat,
    ) -> Option<Rc<Held>> {
        let (dir, _) = here.dir.open_dir_at(name, Some(stat), false).ok()?;
        let next = Rc::new(Held::new(candidate.to_path_buf(), dir, Some(here.clone())));
        let again = !self.opened.insert(candidate.to_path_buf());
        if again && self.held.len() < MAX_HELD {
            self.held.insert(candidate.to_path_buf(), next.clone());
        }
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{is_within, places, resolve, DATA_VOLUME, PROTECTED_IN_HOME, STEP};
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
            // Every place has an `s`: a long s is the same letter to the
            // disk (`protected::AS_ASCII`).
            let long_s = place.replace(['s', 'S'], "\u{17F}");
            asked.extend([
                tree.at(&format!("{long_s}/bin/tool")),
                tree.on_data_volume(&format!("{long_s}/bin/tool")),
                tree.link(&format!("bin/long-s{i}"), format!("../{long_s}/bin/tool")),
            ]);
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
    fn test_with_a_place_on_the_way_to_the_data_volume_each_folder_is_checked() {
        // No place is, for any home folder; were one, a path through it
        // spelled from the data volume would be outside every place while
        // a folder on its way is in one. The round then checks each.
        let tree = Tree::new("on-the-way");
        tree.file("bin/tool", 0o755);
        let protected = Protected {
            places: vec![PathBuf::from("/System/Volumes")],
        };
        assert!(!protected.one_check_covers_the_way());
        let path = tree.on_data_volume("bin/tool");
        assert!(!protected.contains(&path));
        let old = resolve(&path, &protected, true);
        assert!(matches!(old, Resolution::Protected(_)), "{old:?}");
        let mut round = Round::new(protected.clone());
        let (new, made) = calls::measure(|| round.resolve(&path));
        assert_same(&old, &new, &path);
        assert_eq!(made.stat_beneath, 0, "{made:?}");
        assert!(Protected::new(&tree.root).one_check_covers_the_way());
    }

    /// No call `calls` saw, but asking a folder where it is, looked at a
    /// name in a protected place or through one; and one did ask.
    fn assert_moved_folder_not_used(calls: &calls::Calls, protected: &Protected) {
        assert!(calls.get_path > 0, "{calls:?}");
        for (call, path) in &calls.paths {
            if *call == calls::Call::GetPath {
                continue;
            }
            for at in path.ancestors() {
                assert!(!protected.contains(at), "{call:?} looked at {at:?}");
            }
        }
    }

    #[test]
    fn test_a_held_folder_moved_into_a_protected_place_is_not_used_again() {
        // Held, then renamed into `Documents` mid-judgement: a descriptor
        // follows its folder, so using it would look inside `Documents`.
        // Asked where it is (`F_GETPATH`) before it is used again, it is
        // not; the round forgets what it kept and walks from `/`.
        let tree = Tree::new("moved-start");
        tree.file("a/sub/x", 0o644);
        tree.file("a/tool", 0o755);
        tree.dir("Documents");
        let protected = Protected::new(&tree.root);
        let mut round = Round::new(protected.clone());
        // `..` is taken one step at a time: `a` and `a/sub` opened twice.
        for _ in 0..2 {
            round.resolve(&tree.at("a/sub/../sub/missing"));
        }
        assert!(round.held.contains_key(&tree.at("a/sub")));
        fs::rename(tree.at("a"), tree.at("Documents/a")).unwrap();
        // Starts at the held `a/sub`, now `Documents/a/sub`.
        let path = tree.at("a/sub/../tool");
        let (found, made) = calls::measure(|| round.resolve(&path));
        assert_same(&resolve(&path, &protected, true), &found, &path);
        assert!(matches!(found, Resolution::Missing), "{found:?}");
        assert_moved_folder_not_used(&made, &protected);
        assert!(round.held.is_empty(), "nothing kept is used again");
    }

    #[test]
    fn test_a_folder_a_dotdot_goes_back_to_is_asked_where_it_is() {
        // `p/q` held; `q` moved out of `p`, then `p` into `Documents`: the
        // walk starts at `q` (outside, still), and `..` would go back to
        // `p`'s descriptor -- inside `Documents` now.
        let tree = Tree::new("moved-parent");
        tree.dir("p/q");
        tree.file("p/tool", 0o755);
        tree.dir("Documents");
        let protected = Protected::new(&tree.root);
        let mut round = Round::new(protected.clone());
        for _ in 0..2 {
            round.resolve(&tree.at("p/q/../q/missing"));
        }
        assert!(round.held.contains_key(&tree.at("p/q")));
        fs::rename(tree.at("p/q"), tree.at("elsewhere-q")).unwrap();
        fs::rename(tree.at("p"), tree.at("Documents/p")).unwrap();
        let path = tree.at("p/q/../tool");
        let (found, made) = calls::measure(|| round.resolve(&path));
        assert_same(&resolve(&path, &protected, true), &found, &path);
        assert_moved_folder_not_used(&made, &protected);
    }

    #[test]
    fn test_a_held_folder_stepped_into_again_is_asked_where_it_is() {
        // `x` and `x/c` held; the walk reaches `x/c` again through a link
        // in `x`, after `x/c` was moved into `Documents`.
        let tree = Tree::new("moved-entered");
        tree.file("x/c/f", 0o755);
        tree.link("x/to-c", "c");
        tree.dir("Documents");
        let protected = Protected::new(&tree.root);
        let mut round = Round::new(protected.clone());
        for _ in 0..2 {
            round.resolve(&tree.at("x/c/../c/missing"));
        }
        assert!(round.held.contains_key(&tree.at("x/c")));
        fs::rename(tree.at("x/c"), tree.at("Documents/c")).unwrap();
        let path = tree.at("x/to-c/f");
        let (found, made) = calls::measure(|| round.resolve(&path));
        assert_same(&resolve(&path, &protected, true), &found, &path);
        assert!(matches!(found, Resolution::Missing), "{found:?}");
        assert_moved_folder_not_used(&made, &protected);
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
        // And each folder used again asked where it now is (`brew/bin`,
        // then `brew`, which `..` goes back to): two `F_GETPATH`s.
        assert_eq!(
            (
                three.stat_at,
                three.read_link_at,
                three.stat_beneath,
                three.get_path,
                three.total()
            ),
            (1, 1, 1, 2, 5),
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

    #[test]
    fn test_a_round_holds_64_folders_at_most_and_answers_alike_past_them() {
        // 80 folders, each opened twice: once 64 are held (the tree's own
        // folders above them among them), the rest are opened again each
        // time, as `resolve` opens them. Never more open than `/`, the held
        // ones, and the folders of the one walk under way.
        let tree = Tree::new("cap");
        let count = 80;
        for i in 0..count {
            tree.file(&format!("d{i:02}/f"), 0o755);
        }
        let protected = Protected::new(&tree.root);
        // `..` is taken one step at a time, so `dNN` is opened each time.
        let paths: Vec<PathBuf> = (0..count)
            .map(|i| tree.at(&format!("d{i:02}/../d{i:02}/f")))
            .collect();
        let depth = tree.root.components().count() + 3;
        let open_before = OPEN.with(|open| open.get());
        let most = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = most.clone();
        STEP.with(|step| {
            *step.borrow_mut() = Some(Box::new(move |_: &Path| {
                seen.set(seen.get().max(OPEN.with(|open| open.get())));
            }))
        });
        let expected: Vec<Resolution> = paths
            .iter()
            .map(|path| resolve(path, &protected, true))
            .collect();
        let mut round = Round::new(protected.clone());
        for pass in 0..3 {
            let (_, made) = calls::measure(|| {
                for (path, expected) in paths.iter().zip(&expected) {
                    assert_same(expected, &round.resolve(path), path);
                    // At rest: `/` and the held ones.
                    assert_eq!(
                        OPEN.with(|open| open.get()) - open_before,
                        1 + round.held.len(),
                        "{path:?}"
                    );
                }
            });
            assert!(round.held.len() <= MAX_HELD);
            if pass == 2 {
                assert_eq!(round.held.len(), MAX_HELD);
                let held = (0..count)
                    .filter(|i| round.held.contains_key(&tree.at(&format!("d{i:02}"))))
                    .count();
                assert!(held > 0 && held < count, "{held} of the folders held");
                // The ones past the limit are opened again; the held not.
                assert_eq!(made.open_dir_at, count - held, "{made:?}");
            }
        }
        STEP.with(|step| *step.borrow_mut() = None);
        assert!(
            most.get() - open_before <= 1 + MAX_HELD + depth,
            "{} open at most",
            most.get() - open_before
        );
        drop(round);
        assert_eq!(OPEN.with(|open| open.get()), open_before, "all closed");
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

        /// `rel` with one of its names, picked at random, spelled another
        /// way a Mac's disk takes for the same name: in capitals, or with a
        /// long s, a Kelvin sign or an `st` ligature (`protected::AS_ASCII`).
        fn shout(&mut self, rel: &str) -> String {
            let mut names: Vec<String> = rel.split('/').map(str::to_string).collect();
            let at = self.below(names.len());
            names[at] = match self.below(4) {
                0 => names[at].replace(['s', 'S'], "\u{17F}"),
                1 => names[at].replace(['k', 'K'], "\u{212A}"),
                2 => names[at]
                    .replace("st", "\u{FB06}")
                    .replace("St", "\u{FB05}"),
                _ => names[at].to_ascii_uppercase(),
            };
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
                "Document\u{17F}",
                "Mobile Document\u{17F}",
                "Des\u{212A}top",
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
                    // `contains` itself, against the places as listed
                    // and spelled from `/` at every call (`is_within`).
                    assert_eq!(
                        protected.contains(path),
                        is_within(path, &places(std::slice::from_ref(&tree.root))),
                        "{path:?}"
                    );
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
