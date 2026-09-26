//! Where a tool's own installer put it, and whether what is there is that
//! route's: a launcher at the installer's fixed path, resolved and
//! fingerprinted (never a `claude` found through `PATH`, which with
//! Homebrew's `bin` ahead of `~/.local/bin` would be Homebrew's copy and
//! make the native install invisible -- spec §3.3, D3); and, separately,
//! which copy runs when the user types the tool's name (spec §七).

use super::recipe::RouteKind;
use crate::model::InstanceNote;
use crate::runner::HostEnv;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

/// A recipe path (`~/.local/bin/claude`) under `home`. `HostEnv.home` is
/// the process's `HOME` as `HostEnv::discover` read it, not canonicalised:
/// the Unknown page (scan/mod.rs) compares an instance's raw `exe_path`
/// with the raw directory entries it reads, so both must come from the
/// same spelling. The path-list uninstall's function (`removal.rs`), for
/// recipes whose every path is `~/`. A path not starting with `~/` is a
/// programming error in a recipe constant;
/// `recipes::tests::test_a_paths_recipe_names_only_home_paths` catches it
/// before this can.
pub fn expand(home: &Path, spec: &str) -> PathBuf {
    let rest = spec
        .strip_prefix("~/")
        .unwrap_or_else(|| panic!("recipe path {spec:?} must start with ~/"));
    home.join(rest)
}

/// A recipe route path under `home` (`~/.local/bin/claude`) or under the
/// Cargo home (`$CARGO_HOME/bin/rustup`, or the bare `$CARGO_HOME`,
/// rustup's root). `HostEnv.home` is the `HOME` the login shell exported
/// and `cargo_home` is `cargo::cargo_home_of`'s answer (`CARGO_HOME`
/// when set, by the `home` crate's rule), neither canonicalised: the
/// Unknown page (scan/mod.rs) compares an instance's raw `exe_path` with
/// the raw directory entries it reads, so both must come from the same
/// spelling. `None` only for a `$CARGO_HOME` path when there is no
/// usable Cargo home (a relative `CARGO_HOME`, which names a directory
/// relative to cargo's own cwd): `detect` then lists nothing. `expand`
/// (above) is the `~/`-only function the path-list uninstall uses; a
/// `Paths` recipe may name only `~/` paths
/// (`recipes::tests::test_a_paths_recipe_names_only_home_paths`). Any
/// other shape is a programming error in a recipe constant;
/// `recipes::tests::test_every_recipe_path_is_under_home_or_the_cargo_home`
/// catches it before this can. Read by `StandaloneAdapter::detect`,
/// `seated_detected_for` and `execute`.
pub fn expand_route(home: &Path, cargo_home: Option<&Path>, spec: &str) -> Option<PathBuf> {
    if let Some(rest) = spec.strip_prefix("~/") {
        return Some(home.join(rest));
    }
    if spec == "$CARGO_HOME" {
        return cargo_home.map(Path::to_path_buf);
    }
    if let Some(rest) = spec.strip_prefix("$CARGO_HOME/") {
        return cargo_home.map(|cargo_home| cargo_home.join(rest));
    }
    panic!("recipe path {spec:?} must start with ~/ or $CARGO_HOME")
}

/// What is at a recipe's launcher path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Probe {
    /// Nothing this adapter lists: no launcher; a launcher that is a
    /// package manager's copy, another route's shape, or a link that
    /// resolves outside the root; a dangling link pointing somewhere else;
    /// or one `probe` cannot resolve (a symlink loop, a permission error,
    /// a second dangling link along the way). "Not installed", never "not
    /// responding".
    Absent,
    /// The launcher is this route's; `real` is the canonical binary it
    /// resolves to (the artifact's `path`).
    Present { real: PathBuf },
    /// A dangling launcher whose own text points into the root: the
    /// program files are gone (removed by hand or by another tool, or by
    /// an uninstall that stopped partway), the link is left. Listed with no
    /// version and `InstanceNote::LauncherOnly` so the state is visible;
    /// the path-list uninstall (`removal::plan_removal` asks this same
    /// question) lists the program directory as already gone and moves the
    /// link. Because a launcher must be one link straight into its root
    /// (`probe_strict`), a stopped uninstall leaves this state, never an
    /// `Absent` that would read as finished.
    LauncherOnly,
}

/// Path components that mean "a package manager put this here". Checked
/// before the fingerprint, so a `~/.local/bin/claude` that some tool
/// linked into Homebrew's Caskroom is brew's row (or the Unknown page's),
/// never listed twice (spec §3.3 step 2, D3).
const PACKAGE_MANAGER_MARKERS: [&str; 4] = ["Cellar", "Caskroom", "node_modules", "corepack"];

fn has_component(path: &Path, names: &[&str]) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => names.iter().any(|candidate| name == *candidate),
        _ => false,
    })
}

/// Resolve existing components before interpreting `..`; only genuinely
/// missing components may remain lexical. Errors (permissions, loops,
/// non-directories, or dangling intermediate symlinks) are not evidence of
/// missing program files. Private readers: `probe_strict`'s NotFound
/// branch, `placed` (and through it `one_hop`), and `leads_to_program`.
fn canonicalize_existing_prefix(path: &Path) -> std::io::Result<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        let Component::Normal(name) = component else {
            // The root, `.` and `..`, folded by `lexical_join` without
            // touching the disk. Safe only because `resolved` holds no
            // symlink -- what exists of it is canonical, the rest does not
            // exist -- so a `..` cannot skip a link's destination.
            resolved = lexical_join(&resolved, Path::new(component.as_os_str()));
            continue;
        };
        let next = resolved.join(name);
        match std::fs::canonicalize(&next) {
            Ok(real) => resolved = real,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // An existing symlink with a missing destination
                // cannot safely be treated as a missing directory.
                match std::fs::symlink_metadata(&next) {
                    Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {
                        resolved = next;
                    }
                    _ => return Err(error),
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(resolved)
}

/// Whether `launcher` is this route's install of the tool whose root is
/// `root`, and if so which binary it runs (spec §3.3 steps 1-3). Whatever
/// `probe_strict` cannot tell -- a symlink loop, a permission error, a
/// dangling link along the way -- reads as `Absent` here: "not installed",
/// never "not responding", for detection and every refresh.
pub fn probe(kind: RouteKind, launcher: &Path, root: &Path) -> Probe {
    probe_strict(kind, launcher, root).unwrap_or(Probe::Absent)
}

/// `probe`, keeping what it could not tell: `Ok(Absent)` only when the
/// disk says so -- no launcher at all (its `lstat` answers "no such
/// file"), or one that is not this route's -- and `Err` for any other
/// error on the way. Read by `probe`, and by
/// `StandaloneAdapter::reconcile_after_uninstall`, which must not call an
/// uninstall finished because a permission error hid the launcher.
///
/// The launcher is one link, from the installer's fixed path straight
/// into the root: its own text (`one_hop`) must name a place inside the
/// root, and so must where it finally resolves. A launcher that reaches
/// the root through another link outside it (`claude ->
/// ~/.local/bin/claude-current -> ~/.local/share/claude/versions/<v>`) is
/// not the installer's layout and is `Absent` -- the Unknown page lists it
/// -- because once the root has gone to the Trash that other link would
/// dangle and the launcher's own text would no longer say whose it is: a
/// stopped uninstall would read as a finished one. A link the tool keeps
/// inside its root (a `current`) moves with the root and is its business.
pub fn probe_strict(kind: RouteKind, launcher: &Path, root: &Path) -> std::io::Result<Probe> {
    // Step 1: `lstat`, not `stat` -- a dangling link is still a launcher.
    let meta = match std::fs::symlink_metadata(launcher) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Probe::Absent),
        Err(error) => return Err(error),
    };
    match std::fs::canonicalize(launcher) {
        Ok(real) => {
            // Step 2: shared exclusion.
            if has_component(&real, &PACKAGE_MANAGER_MARKERS) {
                return Ok(Probe::Absent);
            }
            // Step 3: the fingerprint.
            match kind {
                RouteKind::SymlinkIntoRoot => {
                    if !meta.file_type().is_symlink() {
                        return Ok(Probe::Absent);
                    }
                    let canonical_root = match std::fs::canonicalize(root) {
                        Ok(canonical_root) => canonical_root,
                        // The launcher runs something, and there is no
                        // root it could be in.
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(Probe::Absent)
                        }
                        Err(error) => return Err(error),
                    };
                    if one_hop(launcher)?.starts_with(&canonical_root)
                        && real.starts_with(&canonical_root)
                    {
                        Ok(Probe::Present { real })
                    } else {
                        Ok(Probe::Absent)
                    }
                }
                RouteKind::FlatFile => {
                    // The installer's own copy is a regular file; a link
                    // of that name points at somebody else's.
                    if meta.file_type().is_file() {
                        Ok(Probe::Present { real })
                    } else {
                        Ok(Probe::Absent)
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Only a link-shaped route has a launcher-only state (program
            // files gone, link left). A flat-file launcher *is* the
            // program, so a dangling link at its path is not this install
            // half-removed -- and rustup's root is the whole Cargo home,
            // under which any link text would land. Exhaustive, so a
            // future kind has to decide.
            match kind {
                RouteKind::SymlinkIntoRoot => {}
                RouteKind::FlatFile => return Ok(Probe::Absent),
            }
            if !meta.file_type().is_symlink() {
                return Ok(Probe::Absent);
            }
            let hop = one_hop(launcher)?;
            let root = canonicalize_existing_prefix(root)?;
            if !has_component(&hop, &PACKAGE_MANAGER_MARKERS) && hop.starts_with(&root) {
                Ok(Probe::LauncherOnly)
            } else {
                Ok(Probe::Absent)
            }
        }
        // A loop or a permission error: not a dangling native install --
        // and no proof that there is none.
        Err(error) => Err(error),
    }
}

/// Where `launcher`'s own text points, as a place on the disk: the text
/// (`readlink`) taken from the launcher's resolved directory -- so a
/// relative `../` means what it means under a `~/.local/bin` that is
/// itself a link -- with the destination's own directory resolved as far
/// as it exists and the destination itself not followed (`placed`): it may
/// be gone (the launcher-only state) or a link the tool keeps inside its
/// root. Read by `probe_strict`, in both of its launcher arms, and by
/// `leads_to_program`, for any link.
fn one_hop(launcher: &Path) -> std::io::Result<PathBuf> {
    let text = std::fs::read_link(launcher)?;
    let dir = std::fs::canonicalize(launcher.parent().unwrap_or(Path::new("/")))?;
    // `join` with an absolute text is that text.
    placed(&dir.join(text))
}

/// `path` as `one_hop` places a link's destination: its folder resolved as
/// far as it exists (`canonicalize_existing_prefix`), its last component
/// not followed. Read by `one_hop`, and by `leads_to_program` to place the
/// links a link may point at the same way, so the two compare.
fn placed(path: &Path) -> std::io::Result<PathBuf> {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => Ok(canonicalize_existing_prefix(parent)?.join(name)),
        // A path ending in `..`, or naming `/`: a directory, resolved like
        // any other -- never a program the route could run.
        _ => canonicalize_existing_prefix(path),
    }
}

/// Whether `link` is another link to the program a route's launcher runs
/// -- grok's `~/.grok/bin/agent`, or a fallback link its installer made in
/// `~/.local/bin` -- rather than a link to something else in the tool's
/// folder. `probe_strict`'s fingerprint (text and target inside the root)
/// cannot tell: grok's root, `~/.grok`, also holds the user's plugins,
/// skills and scripts, which its uninstall keeps. So the launcher's
/// one-link shape, against the program instead: the link's own text
/// (`one_hop`) must land inside `program` -- the folder the program's
/// files are in -- or name one of `via`, the tool's own links such a link
/// may point at instead (grok's installer's fallback text is unverified:
/// one hop, or two through `~/.grok/bin`); and where the link resolves, if
/// it does, must be inside `program` or be `runs`, the file the launcher
/// runs (`Probe::Present`'s `real`; `None` when the launcher runs nothing:
/// it dangles, or is not this route's). A dangling link -- `program`
/// already in the Trash, or removed by hand -- is judged by its text alone,
/// as the launcher is. Anything else -- not a link, not there, a loop or an
/// error on the way -- is `false`: not confirmed. Read by
/// `removal::check_item`, for `Expect::SymlinkToProgram`.
pub fn leads_to_program(link: &Path, program: &Path, via: &[PathBuf], runs: Option<&Path>) -> bool {
    let is_link = std::fs::symlink_metadata(link).is_ok_and(|meta| meta.file_type().is_symlink());
    if !is_link {
        return false;
    }
    let (Ok(hop), Ok(program)) = (one_hop(link), canonicalize_existing_prefix(program)) else {
        return false;
    };
    let names_one_of_via = via
        .iter()
        .any(|other| placed(other).is_ok_and(|other| other == hop));
    if !hop.starts_with(&program) && !names_one_of_via {
        return false;
    }
    match std::fs::canonicalize(link) {
        Ok(real) => real.starts_with(&program) || runs == Some(real.as_path()),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// `target` as seen from `dir`, with `.` and `..` folded away without
/// touching the file system: a relative link text (`../downloads/grok-…`)
/// becomes the absolute path it names; an absolute one is normalised as
/// it is. Climbing above the root stays at the root. Read by
/// `canonicalize_existing_prefix`, which folds with it only onto a path
/// that holds no symlink.
pub fn lexical_join(dir: &Path, target: &Path) -> PathBuf {
    let joined = if target.is_absolute() {
        target.to_path_buf()
    } else {
        dir.join(target)
    };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Whether typing `command` runs this copy, given that this instance's
/// binary is `real` (spec §七): `None` when the first executable `command`
/// on `PATH` is this very file; `NotOnPath` when no executable `command`
/// on `PATH` is, since typing the name then never runs this copy, whether
/// it finds nothing or another program with that name; otherwise -- this
/// file is on `PATH`, behind another executable with that name -- the
/// `ShadowedBy*` note classifying the first one by where it resolves. Only
/// by where, which does not say what program it is: Homebrew's formula
/// `grok`, a regular-expression tool, gets the same `ShadowedByHomebrew`
/// as the `grok` of Homebrew's cask `grok-build`, which is Grok Build, and
/// the `grok` of npm's package `grok-cli`, a third-party wrapper, gets
/// `ShadowedByNpm`. So the notices call the first one another program
/// with that name, never another copy. Payload-free on purpose
/// (`InstanceNote`'s rule, from the instance-level channel spec's §2.3,
/// restated in spec §七); the sentence names the command, which the user
/// knows, not the winner's path, which they would not.
pub fn shadow_note(command: &str, env: &HostEnv, real: &Path) -> Option<InstanceNote> {
    // Standalone-only lookup: changing the shared package-manager
    // discovery helper would broaden this step beyond its PATH notices.
    // Every executable `command` on PATH, in PATH's order, as its
    // canonical path; `None` for one that does not canonicalise.
    let mut found = env
        .path_dirs
        .iter()
        .map(|dir| dir.join(command))
        .filter(|path| {
            std::fs::metadata(path)
                .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
        .map(|path| std::fs::canonicalize(path).ok());
    let Some(first) = found.next() else {
        return Some(InstanceNote::NotOnPath);
    };
    if first.as_deref() == Some(real) {
        return None;
    }
    // The first one is not this file, or does not resolve. It shadows
    // this copy only if this copy is on PATH behind it: with no later
    // entry resolving to this file, no PATH entry is known to reach this
    // copy, so the note is that it is not on PATH.
    if !found.any(|later| later.as_deref() == Some(real)) {
        return Some(InstanceNote::NotOnPath);
    }
    let Some(first) = first else {
        return Some(InstanceNote::ShadowedByOther);
    };
    Some(if has_component(&first, &["Cellar", "Caskroom"]) {
        InstanceNote::ShadowedByHomebrew
    } else if has_component(&first, &["node_modules"]) {
        InstanceNote::ShadowedByNpm
    } else {
        InstanceNote::ShadowedByOther
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Named here as well as through `super::*`, so these tests do not lean
    // on the imports above them; an explicit import beside a glob is not a
    // warning.
    use std::path::{Path, PathBuf};

    #[test]
    fn test_expand_joins_a_tilde_path_onto_home() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            expand(home, "~/.local/bin/claude"),
            PathBuf::from("/Users/someone/.local/bin/claude")
        );
        assert_eq!(
            expand(home, "~/.local/share/claude"),
            PathBuf::from("/Users/someone/.local/share/claude")
        );
    }

    #[test]
    fn test_expand_keeps_the_spelling_of_home_it_was_given() {
        // `HostEnv.home` is whatever `HOME` says; nothing here
        // canonicalises it. The Unknown page's rule 0 compares an
        // instance's raw `exe_path` with the raw directory entry it found
        // (scan/mod.rs), so the launcher path must be built from the same
        // spelling the scan uses.
        let home = Path::new("/Volumes/Data/homes/someone");
        assert_eq!(
            expand(home, "~/.local/bin/claude"),
            PathBuf::from("/Volumes/Data/homes/someone/.local/bin/claude")
        );
    }

    #[test]
    #[should_panic(expected = "must start with ~/")]
    fn test_expand_refuses_a_path_that_is_not_under_home() {
        // Unreachable from the shipped recipes
        // (`recipes::tests::test_a_paths_recipe_names_only_home_paths`
        // holds every path the path-list uninstall expands to `~/`); a
        // panic here is a programming error surfacing at the first test
        // run, not a state of anyone's Mac.
        let _ = expand(Path::new("/Users/someone"), "/usr/local/bin/claude");
    }

    #[test]
    fn test_expand_route_joins_home_paths_always_and_cargo_home_paths_when_there_is_one() {
        // rustup's launcher and root (spec §3.5): `$CARGO_HOME/bin/rustup`
        // and the bare `$CARGO_HOME`. The Cargo home is whatever
        // `cargo::cargo_home_of` answered -- `CARGO_HOME` when set -- so
        // a Mac with it set finds rustup where rustup actually is; and
        // `None` (a relative CARGO_HOME, unsupported) means a recipe under
        // it has no path at all, while a `~/` path never depends on it.
        let home = Path::new("/Users/someone");
        let default = Path::new("/Users/someone/.cargo");
        assert_eq!(
            expand_route(home, Some(default), "~/.local/bin/claude"),
            Some(PathBuf::from("/Users/someone/.local/bin/claude"))
        );
        assert_eq!(
            expand_route(home, None, "~/.local/bin/claude"),
            Some(PathBuf::from("/Users/someone/.local/bin/claude"))
        );
        assert_eq!(
            expand_route(home, Some(default), "$CARGO_HOME/bin/rustup"),
            Some(PathBuf::from("/Users/someone/.cargo/bin/rustup"))
        );
        assert_eq!(
            expand_route(
                home,
                Some(Path::new("/Volumes/Data/cargo")),
                "$CARGO_HOME/bin/rustup"
            ),
            Some(PathBuf::from("/Volumes/Data/cargo/bin/rustup"))
        );
        assert_eq!(
            expand_route(home, Some(Path::new("/Volumes/Data/cargo")), "$CARGO_HOME"),
            Some(PathBuf::from("/Volumes/Data/cargo"))
        );
        assert_eq!(expand_route(home, None, "$CARGO_HOME/bin/rustup"), None);
        assert_eq!(expand_route(home, None, "$CARGO_HOME"), None);
    }

    #[test]
    #[should_panic(expected = "must start with ~/ or $CARGO_HOME")]
    fn test_expand_route_refuses_any_other_shape() {
        let home = Path::new("/Users/someone");
        let _ = expand_route(home, Some(&home.join(".cargo")), "/usr/local/bin/rustup");
    }

    use super::super::recipe::RouteKind;
    use super::super::testing::{claude_layout, grok_layout, rustup_layout, TempHome, Unreadable};
    use crate::model::InstanceNote;

    #[test]
    fn test_probe_finds_a_flat_file_launcher_and_its_real_path_is_itself() {
        // rustup: `$CARGO_HOME/bin/rustup` is an 11 MB Mach-O regular file
        // (spec §3.5, VERIFIED); its thirteen proxies are links *to* it,
        // but the launcher itself is no link.
        let home = TempHome::new("probe-flat-present");
        let layout = rustup_layout(&home.path().join(".cargo"));
        assert_eq!(
            probe(RouteKind::FlatFile, &layout.launcher, &layout.cargo_home),
            Probe::Present {
                real: layout.launcher.clone()
            }
        );
    }

    #[test]
    fn test_probe_rejects_a_link_or_a_directory_where_a_flat_file_is_expected() {
        // A `rustup` that is itself a symlink (Homebrew's keg-only formula
        // linked by hand, or anything else) is not the installer's copy;
        // nor is a directory of that name.
        let home = TempHome::new("probe-flat-link");
        let elsewhere = home.file("opt/homebrew/Cellar/rustup/1.29.1/bin/rustup");
        let launcher = home.link(".cargo/bin/rustup", &elsewhere);
        assert_eq!(
            probe(RouteKind::FlatFile, &launcher, &home.path().join(".cargo")),
            Probe::Absent
        );

        let home = TempHome::new("probe-flat-dir");
        let launcher = home.dir(".cargo/bin/rustup");
        assert_eq!(
            probe(RouteKind::FlatFile, &launcher, &home.path().join(".cargo")),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_is_absent_for_a_flat_file_launcher_that_is_not_there() {
        let home = TempHome::new("probe-flat-missing");
        assert_eq!(
            probe(
                RouteKind::FlatFile,
                &home.path().join(".cargo/bin/rustup"),
                &home.path().join(".cargo")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_is_absent_for_a_dangling_link_where_a_flat_file_is_expected() {
        // A flat-file launcher is the program itself, so a dangling link
        // at its path is not this install half-removed. rustup's root is
        // the whole Cargo home: without the flat-file rule this link's
        // text lands under it and the row would be a version-less rustup
        // whose plans run a dangling link (Ruling 8).
        let home = TempHome::new("probe-flat-dangling");
        let cargo_home = home.dir(".cargo");
        let launcher = home.link(".cargo/bin/rustup", Path::new("rustup.old"));
        assert_eq!(
            probe(RouteKind::FlatFile, &launcher, &cargo_home),
            Probe::Absent
        );
        // The same link under the link-shaped route is still B's
        // `LauncherOnly`: the rule belongs to the route, not to the link.
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &cargo_home),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_probe_finds_a_launcher_that_links_into_its_root() {
        let home = TempHome::new("probe-present");
        let layout = claude_layout(&home, "2.1.281");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
            Probe::Present { real: layout.real }
        );
    }

    #[test]
    fn test_probe_follows_a_two_hop_link_into_the_root() {
        // `realpath`, not one `readlink`: a launcher that links to a
        // `current` link inside the root still resolves into it.
        // Its first hop lands inside the root, as the one-hop rule requires.
        let home = TempHome::new("probe-two-hop");
        let real = home.file(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/share/claude/current", &real);
        let launcher = home.link(".local/bin/claude", &current);
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &launcher,
                &home.path().join(".local/share/claude")
            ),
            Probe::Present { real }
        );
    }

    #[test]
    fn test_probe_refuses_a_launcher_that_reaches_the_root_through_a_link_outside_it() {
        // Phase 4 step C: the launcher is one link straight into the root.
        // Through `~/.local/bin/claude-current` it resolves into the root
        // all the same, but once the root is in the Trash that second link
        // dangles, and the launcher's own text -- `claude-current`, outside
        // the root -- no longer says whose it is: a stopped uninstall would
        // read as a finished one. Not the installer's layout, so not this
        // route's instance (the Unknown page lists it), before and after.
        let home = TempHome::new("probe-hop-outside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/bin/claude-current", &real);
        let launcher = home.link(".local/bin/claude", &current);
        let root = home.path().join(".local/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Absent
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_counts_a_launcher_dangling_through_the_roots_own_link_as_launcher_only() {
        // The other side of the one-hop rule: a link the tool keeps inside
        // its root is its own business. With the version it points at gone
        // and the root's `current` link left dangling, the launcher's own
        // text still names a place inside the root: launcher-only, the row
        // an Uninstall finishes -- not `Absent`, which would have hidden a
        // root that still holds files.
        let home = TempHome::new("probe-dangling-through-current");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/share/claude/current", &real);
        let launcher = home.link(".local/bin/claude", &current);
        let root = home.path().join(".local/share/claude");
        std::fs::remove_dir_all(root.join("versions")).unwrap();
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_probe_strict_says_it_cannot_tell_where_probe_says_absent() {
        // A launcher whose folder cannot be read: `probe` answers `Absent`
        // (detection's "not installed"), `probe_strict` an error -- the
        // reading after an uninstall must not take a permission error for
        // "the launcher is gone". A launcher that really is gone is
        // `Ok(Absent)` for both.
        let home = TempHome::new("probe-strict-unreadable");
        let layout = claude_layout(&home, "2.1.281");
        let bin = home.path().join(".local/bin");
        {
            let Some(_locked) = Unreadable::new(&bin) else {
                return;
            };
            assert_eq!(
                probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
                Probe::Absent
            );
            let error = probe_strict(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root)
                .expect_err("a permission error is not an answer");
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        }
        std::fs::remove_file(&layout.launcher).unwrap();
        assert_eq!(
            probe_strict(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root).unwrap(),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_accepts_a_home_reached_through_a_symlink() {
        // `HostEnv.home` may be a symlink to the real home (a home on
        // another volume). The launcher and root the recipe expands under
        // it are then non-canonical spellings of the same files; both
        // sides are canonicalised before the fingerprint compares them, and
        // `real` comes back canonical.
        let home = TempHome::new("probe-linked-home");
        let real_home = home.dir("real-home");
        let real = home.file("real-home/.local/share/claude/versions/2.1.281");
        home.link("real-home/.local/bin/claude", &real);
        let linked_home = home.link("linked-home", &real_home);
        let launcher = linked_home.join(".local/bin/claude");
        let root = linked_home.join(".local/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Present { real }
        );
    }

    #[test]
    fn test_probe_leaves_a_homebrew_or_npm_copy_to_its_own_source() {
        // Shared exclusion before the fingerprint (spec §3.3 step 2): a
        // `~/.local/bin/claude` that resolves under Caskroom, Cellar,
        // node_modules or corepack is a row brew or npm already lists.
        for tail in [
            "opt/homebrew/Caskroom/claude-code/2.1.267/claude",
            "opt/homebrew/Cellar/something/1.0/bin/claude",
            "opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            "opt/homebrew/lib/node_modules/corepack/dist/claude.js",
        ] {
            let home = TempHome::new("probe-excluded");
            let target = home.file(tail);
            let launcher = home.link(".local/bin/claude", &target);
            assert_eq!(
                probe(
                    RouteKind::SymlinkIntoRoot,
                    &launcher,
                    &home.path().join(".local/share/claude")
                ),
                Probe::Absent,
                "{tail}"
            );
        }
    }

    #[test]
    fn test_probe_excludes_package_manager_markers_even_inside_the_native_root() {
        for marker in ["Cellar", "Caskroom", "node_modules", "corepack"] {
            let home = TempHome::new("probe-marker-inside-root");
            let root = home.dir(".local/share/claude");
            let target = home.executable(&format!(".local/share/claude/{marker}/claude"));
            let launcher = home.link(".local/bin/claude", &target);
            // Without the marker exclusion, the root fingerprint accepts
            // this real executable, so this test detects that deletion.
            assert_eq!(
                probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
                Probe::Absent
            );
            std::fs::remove_file(&target).unwrap();
            assert_eq!(
                probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
                Probe::Absent
            );
        }
    }

    #[test]
    fn test_probe_resolves_a_linked_bin_before_relative_dotdot() {
        let home = TempHome::new("probe-linked-bin");
        let other_bin = home.dir("other/bin");
        home.link(".local/bin", &other_bin);
        let launcher = home.link(
            ".local/bin/claude",
            Path::new("../share/claude/versions/missing"),
        );
        let native_root = home.path().join(".local/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &native_root),
            Probe::Absent
        );
        // The very same link belongs to this root, proving the parent
        // resolution changes its meaning instead of rejecting all links.
        let actual_root = home.path().join("other/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &actual_root),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_probe_does_not_call_a_symlink_loop_launcher_only() {
        let home = TempHome::new("probe-loop");
        let root = home.dir(".local/share/claude");
        let target = root.join("loop");
        home.link(".local/share/claude/loop", &target);
        let launcher = home.link(".local/bin/claude", &target);
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_rejects_a_plain_file_where_a_link_is_expected() {
        let home = TempHome::new("probe-plain-file");
        let launcher = home.file(".local/bin/claude");
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &launcher,
                &home.path().join(".local/share/claude")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_rejects_a_link_that_resolves_outside_the_root() {
        let home = TempHome::new("probe-outside-root");
        let elsewhere = home.file("elsewhere/claude");
        let launcher = home.link(".local/bin/claude", &elsewhere);
        home.dir(".local/share/claude");
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &launcher,
                &home.path().join(".local/share/claude")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_is_absent_when_there_is_no_launcher() {
        // "Not installed", not "not responding": no instance at all.
        let home = TempHome::new("probe-missing");
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &home.path().join(".local/bin/claude"),
                &home.path().join(".local/share/claude")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_reports_launcher_only_for_a_dangling_link_whose_text_points_into_the_root() {
        // The half-uninstalled state (spec §3.3 step 2, §十二 Q17): the
        // program directory is gone, the link is left. Absolute text, and
        // relative text as grok's installer writes it (`../…`).
        let home = TempHome::new("probe-dangling-absolute");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::LauncherOnly
        );

        let home = TempHome::new("probe-dangling-relative");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(
            ".local/bin/claude",
            Path::new("../share/claude/versions/2.1.281"),
        );
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_probe_is_absent_for_a_dangling_link_that_points_elsewhere() {
        // An old installer's leftover pointing somewhere else is the
        // Unknown page's broken link, not a Claude Code that stopped
        // answering (spec §十三 #33).
        let home = TempHome::new("probe-dangling-elsewhere");
        let launcher = home.link(
            ".local/bin/claude",
            &home
                .path()
                .join("Applications/Old.app/Contents/MacOS/claude"),
        );
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &launcher,
                &home.path().join(".local/share/claude")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_reports_launcher_only_under_a_home_reached_through_a_symlink() {
        // The dangling twin of
        // `test_probe_accepts_a_home_reached_through_a_symlink`: the
        // installer spelled the link text through the real home,
        // `HostEnv.home` is the symlink to it, and the program directory
        // is gone. Neither the target nor the root canonicalises whole, so
        // `probe` canonicalises the deepest ancestor of each that still
        // exists and the two spellings agree; without that the row would
        // vanish from the Installed page and the link would be listed on
        // the Unknown page as a broken link instead.
        let home = TempHome::new("probe-linked-home-dangling");
        let real_home = home.dir("real-home");
        home.dir("real-home/.local/share");
        home.link(
            "real-home/.local/bin/claude",
            &real_home.join(".local/share/claude/versions/2.1.281"),
        );
        let linked_home = home.link("linked-home", &real_home);
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &linked_home.join(".local/bin/claude"),
                &linked_home.join(".local/share/claude")
            ),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_leads_to_program_accepts_a_link_to_the_program_directly_or_through_the_tools_own_links()
    {
        // grok's `~/.grok/bin/agent` points straight at a download, as the
        // launcher does. The fallback links its installer may make in
        // `~/.local/bin` say one hop or two (unverified, grok.md §2):
        // straight at a download -- the current one, or an older one an
        // update left -- or at one of the two links in `~/.grok/bin`.
        let home = TempHome::new("program-link-accepts");
        let layout = grok_layout(&home, "1.0.41");
        let older = home.executable(".grok/downloads/grok-1.0.34-macos-aarch64");
        let program = layout.root.join("downloads");
        let via = [layout.launcher.clone(), layout.agent.clone()];
        let links = [
            layout.agent.clone(),
            home.link(
                ".local/bin/grok",
                Path::new("../../.grok/downloads/grok-1.0.41-macos-aarch64"),
            ),
            home.link(".local/bin/grok-older", &older),
            home.link(".local/bin/grok-two-hops", &layout.launcher),
            home.link(".local/bin/agent", &layout.agent),
        ];
        for link in &links {
            assert!(
                leads_to_program(link, &program, &via, Some(layout.real.as_path())),
                "{link:?}"
            );
        }
        // The same, with every path spelled through a home reached by a
        // symlink (`HostEnv.home` may be one, a home on another volume):
        // the link's text and the paths it is compared with are placed
        // alike, each folder resolved.
        let linked = home.link("linked-home", home.path());
        let spelled = |path: &Path| linked.join(path.strip_prefix(home.path()).unwrap());
        let via = [spelled(&layout.launcher), spelled(&layout.agent)];
        for link in &links {
            assert!(
                leads_to_program(
                    &spelled(link),
                    &spelled(&program),
                    &via,
                    Some(layout.real.as_path())
                ),
                "{link:?} through the linked home"
            );
        }
    }

    #[test]
    fn test_leads_to_program_refuses_a_link_into_the_rest_of_the_tools_folder_or_elsewhere() {
        // `~/.grok` is also the folder grok's uninstall keeps: a plugin's or
        // a skill's program, or a script of the user's in `~/.grok/bin`, is
        // not grok's, and neither is a link to one. Nor is a link whose own
        // text names neither the program folder nor one of the two links in
        // `~/.grok/bin`, even when it resolves into the program through
        // another link: once that other link is in the Trash it dangles,
        // with nothing left to say whose it was -- the launcher's one-link
        // rule. Nor a link to a "download" that is itself a link out of the
        // program folder, a regular file, or nothing at all.
        let home = TempHome::new("program-link-refuses");
        let layout = grok_layout(&home, "1.0.41");
        let program = layout.root.join("downloads");
        let via = [layout.launcher.clone(), layout.agent.clone()];
        let runs = Some(layout.real.as_path());
        let plugin = home.executable(".grok/plugins/p/bin/agent");
        let skill = home.executable(".grok/skills/s/bin/grok");
        let script = home.executable(".grok/bin/my-own-script");
        home.link(".local/bin/grok", &layout.real);
        let current = home.link(".grok/downloads/current", &plugin);
        let refused = [
            home.link(".local/bin/agent", &plugin),
            home.link(".local/bin/skill", &skill),
            home.link(".local/bin/script", &script),
            home.link(".local/bin/agent-beside-grok", Path::new("grok")),
            home.link(".local/bin/elsewhere", &home.executable("other/grok")),
            home.link(".local/bin/current", &current),
            home.file(".local/bin/plain"),
            home.path().join(".local/bin/missing"),
        ];
        for link in &refused {
            assert!(!leads_to_program(link, &program, &via, runs), "{link:?}");
        }

        // grok's own `agent` replaced by a link to the plugin: not grok's,
        // and neither is a fallback through it, although that one's text
        // names one of the two links -- it ends at the plugin.
        std::fs::remove_file(&layout.agent).unwrap();
        home.link(".grok/bin/agent", Path::new("../plugins/p/bin/agent"));
        let through_agent = home.link(".local/bin/agent-two-hops", &layout.agent);
        assert!(!leads_to_program(&layout.agent, &program, &via, runs));
        assert!(!leads_to_program(&through_agent, &program, &via, runs));
        // And through a loop, nothing is confirmed.
        std::fs::remove_file(&layout.agent).unwrap();
        home.link(".grok/bin/agent", Path::new("agent"));
        assert!(!leads_to_program(&through_agent, &program, &via, runs));
    }

    #[test]
    fn test_leads_to_program_reads_a_dangling_link_by_its_own_text() {
        // With the program folder in the Trash -- the uninstall moves
        // `~/.grok/downloads` before `~/.grok/bin/agent`, or it was removed
        // by hand -- the links dangle and their own text decides, as the
        // launcher's does: into the program folder, or at one of the two
        // links in `~/.grok/bin`, is grok's; into a plugin that has since
        // been removed is not. There is no file the launcher runs.
        let home = TempHome::new("program-link-dangling");
        let layout = grok_layout(&home, "1.0.41");
        let program = layout.root.join("downloads");
        let via = [layout.launcher.clone(), layout.agent.clone()];
        let accepted = [
            layout.agent.clone(),
            home.link(
                ".local/bin/grok",
                Path::new("../../.grok/downloads/grok-1.0.41-macos-aarch64"),
            ),
            home.link(".local/bin/grok-two-hops", &layout.launcher),
            home.link(".local/bin/agent", &layout.agent),
        ];
        let plugin_gone = home.link(
            ".local/bin/plugin",
            &layout.root.join("plugins/gone/bin/agent"),
        );
        std::fs::remove_dir_all(&program).unwrap();
        for link in &accepted {
            assert!(!link.exists(), "{link:?} dangles");
            assert!(leads_to_program(link, &program, &via, None), "{link:?}");
        }
        assert!(!leads_to_program(&plugin_gone, &program, &via, None));
    }

    #[test]
    fn test_leads_to_program_accepts_the_very_file_the_launcher_runs_wherever_it_is() {
        // Another name for the command is also a link to exactly the file
        // the launcher runs, in a layout whose launcher runs one outside the
        // program folder. Only that file, and only when there is one: a
        // launcher that is not this route's gives none.
        let home = TempHome::new("program-link-runs");
        let program = home.dir(".grok/downloads");
        let runs = home.executable(".grok/versions/1.0.41/grok");
        let other = home.executable(".grok/versions/1.0.40/grok");
        let launcher = home.link(".grok/bin/grok", Path::new("../versions/1.0.41/grok"));
        let via = [launcher.clone()];
        let fallback = home.link(".local/bin/grok", &launcher);
        assert!(leads_to_program(
            &fallback,
            &program,
            &via,
            Some(runs.as_path())
        ));
        assert!(!leads_to_program(&fallback, &program, &via, None));
        assert!(!leads_to_program(
            &fallback,
            &program,
            &via,
            Some(other.as_path())
        ));
    }

    #[test]
    fn test_lexical_join_normalises_dot_and_dotdot_without_touching_the_disk() {
        let dir = Path::new("/Users/someone/.local/bin");
        assert_eq!(
            lexical_join(dir, Path::new("../share/claude/versions/2.1.281")),
            PathBuf::from("/Users/someone/.local/share/claude/versions/2.1.281")
        );
        assert_eq!(
            lexical_join(dir, Path::new("./claude-real")),
            PathBuf::from("/Users/someone/.local/bin/claude-real")
        );
        assert_eq!(
            lexical_join(dir, Path::new("/Users/someone/.grok/downloads/grok-1.0.41")),
            PathBuf::from("/Users/someone/.grok/downloads/grok-1.0.41")
        );
        // Climbing past the root stays at the root.
        assert_eq!(
            lexical_join(Path::new("/a"), Path::new("../../../b")),
            PathBuf::from("/b")
        );
    }

    #[test]
    fn test_shadow_note_says_not_on_path_when_typing_the_name_finds_nothing() {
        let home = TempHome::new("shadow-not-on-path");
        let layout = claude_layout(&home, "2.1.281");
        let env = home.env(vec![home.dir("somewhere/else")]);
        assert_eq!(
            shadow_note("claude", &env, &layout.real),
            Some(InstanceNote::NotOnPath)
        );
    }

    #[test]
    fn test_shadow_note_says_not_on_path_when_a_namesake_is_on_path_and_this_copy_is_not() {
        // The launcher's directory is missing from PATH and no link to
        // this copy is on it, so typing the name never runs this copy,
        // whatever program with that name PATH finds instead. A
        // `ShadowedBy*` note would say that program comes earlier in PATH
        // than this copy, which is not in PATH at all; uninstalling that
        // program would leave the name finding nothing.
        for tail in [
            "opt/homebrew/Caskroom/claude-code/2.1.267/claude",
            "opt/homebrew/Cellar/x/1/bin/claude",
            "opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            "Applications/Some.app/Contents/MacOS/claude",
        ] {
            let home = TempHome::new("shadow-only-another-copy");
            let layout = claude_layout(&home, "2.1.281");
            let winner = home.executable(tail);
            let first = home.dir("first-on-path");
            home.link("first-on-path/claude", &winner);
            let env = home.env(vec![first]);
            assert_eq!(
                shadow_note("claude", &env, &layout.real),
                Some(InstanceNote::NotOnPath),
                "{tail}"
            );
        }
    }

    #[test]
    fn test_shadow_note_counts_a_link_to_this_copy_as_this_copy_being_on_path() {
        // `~/.local/bin` is missing from PATH, but `~/bin/claude` links to
        // the launcher and `~/bin` is on PATH after Homebrew's directory:
        // this copy is on PATH, behind Homebrew's, so Homebrew's shadows
        // it. What decides is whether an executable `claude` on PATH is
        // this file, not whether the launcher's own directory is listed.
        let home = TempHome::new("shadow-reached-through-a-link");
        let layout = claude_layout(&home, "2.1.281");
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        let brew_bin = home.dir("opt/homebrew/bin");
        home.link("opt/homebrew/bin/claude", &cask);
        let bin = home.dir("bin");
        home.link("bin/claude", &layout.launcher);
        let env = home.env(vec![brew_bin, bin]);
        assert_eq!(
            shadow_note("claude", &env, &layout.real),
            Some(InstanceNote::ShadowedByHomebrew)
        );
    }

    #[test]
    fn test_shadow_note_is_silent_when_path_finds_this_very_copy() {
        let home = TempHome::new("shadow-same");
        let layout = claude_layout(&home, "2.1.281");
        let env = home.env(vec![home.path().join(".local/bin")]);
        assert_eq!(shadow_note("claude", &env, &layout.real), None);
    }

    #[test]
    fn test_shadow_note_says_not_on_path_when_the_launcher_is_on_path_but_its_target_is_not_executable(
    ) {
        // Same PATH as the test above -- `~/.local/bin` is on it -- but the
        // file the launcher links to has no executable bit, so the scan
        // (regular file with an executable bit, through the link) keeps no
        // entry that is this copy: typing the name cannot run it. The
        // note is NotOnPath, as when the folder is missing from PATH,
        // which is why `sourceNotice.notOnPath` gives the missing folder
        // as the likely cause and not as the cause.
        let home = TempHome::new("shadow-target-not-executable");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::set_permissions(&layout.real, std::fs::Permissions::from_mode(0o644)).unwrap();
        let env = home.env(vec![home.path().join(".local/bin")]);
        assert_eq!(
            shadow_note("claude", &env, &layout.real),
            Some(InstanceNote::NotOnPath)
        );
    }

    #[test]
    fn test_shadow_note_is_silent_for_a_link_to_the_same_launcher() {
        // `~/bin/claude → ~/.local/bin/claude` earlier on PATH runs the
        // same file: canonical paths are compared, not the names PATH
        // found.
        let home = TempHome::new("shadow-link-to-launcher");
        let layout = claude_layout(&home, "2.1.281");
        let bin = home.dir("bin");
        home.link("bin/claude", &layout.launcher);
        let env = home.env(vec![bin, home.path().join(".local/bin")]);
        assert_eq!(shadow_note("claude", &env, &layout.real), None);
    }

    #[test]
    fn test_shadow_note_skips_an_earlier_non_executable_namesake() {
        let home = TempHome::new("shadow-non-executable");
        let layout = claude_layout(&home, "2.1.281");
        let namesake = home.file("earlier/claude");
        std::fs::set_permissions(&namesake, std::fs::Permissions::from_mode(0o644)).unwrap();
        let env = home.env(vec![
            home.path().join("earlier"),
            home.path().join(".local/bin"),
        ]);
        assert_eq!(shadow_note("claude", &env, &layout.real), None);
    }

    #[test]
    fn test_shadow_note_classifies_the_executable_that_wins_on_path() {
        for (tail, expected) in [
            (
                "opt/homebrew/Caskroom/claude-code/2.1.267/claude",
                InstanceNote::ShadowedByHomebrew,
            ),
            (
                "opt/homebrew/Cellar/x/1/bin/claude",
                InstanceNote::ShadowedByHomebrew,
            ),
            (
                "opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                InstanceNote::ShadowedByNpm,
            ),
            (
                "Applications/Some.app/Contents/MacOS/claude",
                InstanceNote::ShadowedByOther,
            ),
        ] {
            let home = TempHome::new("shadow-classify");
            let layout = claude_layout(&home, "2.1.281");
            let winner = home.executable(tail);
            let first = home.dir("first-on-path");
            home.link("first-on-path/claude", &winner);
            let env = home.env(vec![first, home.path().join(".local/bin")]);
            assert_eq!(
                shadow_note("claude", &env, &layout.real),
                Some(expected),
                "{tail}"
            );
        }
    }

    #[test]
    fn test_shadow_note_classifies_by_where_the_first_one_resolves_not_by_what_program_it_is() {
        // A name and a place, nothing more (step D's review): Homebrew's
        // formula `grok`, a regular-expression tool (`bin/grok` in its
        // keg), gets the very note that the `grok` of Homebrew's cask
        // `grok-build` -- Grok Build itself, its download staged in the
        // Caskroom -- gets, and the `grok` of npm's package `grok-cli`, a
        // third-party wrapper whose `grok` is its `index.js`, gets npm's.
        // Which is why the notices call what runs another program with
        // that name and never another copy, in both locales
        // (src/lib/sources.test.ts).
        for (tail, expected) in [
            (
                "opt/homebrew/Cellar/grok/0.9.2_2/bin/grok",
                InstanceNote::ShadowedByHomebrew,
            ),
            (
                "opt/homebrew/Caskroom/grok-build/1.0.41/grok-1.0.41-macos-aarch64",
                InstanceNote::ShadowedByHomebrew,
            ),
            (
                "opt/homebrew/lib/node_modules/grok-cli/index.js",
                InstanceNote::ShadowedByNpm,
            ),
        ] {
            let home = TempHome::new("shadow-namesake");
            let layout = grok_layout(&home, "1.0.41");
            let winner = home.executable(tail);
            let brew_bin = home.dir("opt/homebrew/bin");
            home.link("opt/homebrew/bin/grok", &winner);
            let env = home.env(vec![brew_bin, home.path().join(".grok/bin")]);
            assert_eq!(
                shadow_note("grok", &env, &layout.real),
                Some(expected),
                "{tail}"
            );
        }
    }
}
