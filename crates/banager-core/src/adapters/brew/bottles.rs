//! Whether Homebrew has a bottle -- a ready-built copy -- of a formula for
//! this Mac, read for the update's preview (r18 R46-2). Homebrew builds a
//! formula from source when none of its bottles fits the Mac
//! (`FormulaInstaller#pour_bottle?`, `formula_installer.rb:258-259`), and
//! it no longer builds bottles for Intel Macs, nor for Apple silicon on
//! macOS 14 or older (`docs/Support-Tiers.md:143-159` in Homebrew 7.0.9):
//! there an update compiles, for minutes or hours. The bottles are the
//! keys of `bottle.stable.files` in the `brew info --installed --json=v2`
//! reply the inventory already reads (`Formula#bottle_hash`,
//! `formula.rb:3242-3269`), for the version the catalogue has now, which is
//! the one an update installs.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The macOS releases Homebrew names its bottles after, with their
/// versions (`MacOSVersion::RELEASES`, `macos_version.rb:21-35`).
const RELEASES: [(&str, (u32, u32)); 12] = [
    ("golden_gate", (27, 0)),
    ("tahoe", (26, 0)),
    ("sequoia", (15, 0)),
    ("sonoma", (14, 0)),
    ("ventura", (13, 0)),
    ("monterey", (12, 0)),
    ("big_sur", (11, 0)),
    ("catalina", (10, 15)),
    ("mojave", (10, 14)),
    ("high_sierra", (10, 13)),
    ("sierra", (10, 12)),
    ("el_capitan", (10, 11)),
];

/// The bottle tag Homebrew looks for on this Mac
/// (`Utils::Bottles.tag`, `extend/os/mac/utils/bottles.rb:9-13`): its
/// processor as the Homebrew runs it, and the macOS version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MacTag {
    pub(crate) arm: bool,
    pub(crate) macos: (u32, u32),
}

/// This Mac's tag for the Homebrew at `prefix`, or `None` when its macOS
/// version cannot be read or is newer than every release named above (a
/// bottle named after it would not be known here). A Homebrew in
/// `/opt/homebrew` runs as Apple silicon and one in `/usr/local` as Intel
/// (Homebrew's own default prefixes, `install.sh`); one elsewhere as
/// Banager itself does. The version is the kernel's
/// (`diagnostics::read_os`, `sysctlbyname`): nothing runs.
/// Unused in this crate's unit tests, which read nothing of the Mac
/// running them (`BrewAdapter::mac_tag_fn`).
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn this_mac(prefix: &Path) -> Option<MacTag> {
    let arm = if prefix == Path::new("/opt/homebrew") {
        true
    } else if prefix == Path::new("/usr/local") {
        false
    } else {
        cfg!(target_arch = "aarch64")
    };
    let version = crate::diagnostics::read_os().macos_version?;
    let macos = parse_version(&version)?;
    known(macos).then_some(MacTag { arm, macos })
}

/// Whether macOS `version` is no newer than the newest release named in
/// `RELEASES`.
fn known(version: (u32, u32)) -> bool {
    RELEASES.iter().any(|(_, release)| version.0 <= release.0)
}

/// `"26.0.1"` as `(26, 0)`, `"10.15.7"` as `(10, 15)`.
fn parse_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |minor| minor.parse().ok())?;
    Some((major, minor))
}

/// The processor and macOS release a bottle tag names: `arm64_tahoe` is
/// Apple silicon on Tahoe, `sonoma` Intel on Sonoma; `None` for `all`, a
/// Linux tag, or a release not named in `RELEASES`.
fn tag_of(tag: &str) -> Option<(bool, (u32, u32))> {
    let (arm, release) = match tag.strip_prefix("arm64_") {
        Some(release) => (true, release),
        None => (false, tag),
    };
    RELEASES
        .iter()
        .find(|(name, _)| *name == release)
        .map(|(_, version)| (arm, *version))
}

/// Whether an update compiles on `mac`: none of `tags`, a formula's
/// bottles, fits it. One fits when it is `all`, or for the same processor
/// and a macOS no newer than the Mac's (Homebrew pours a bottle built on
/// an older macOS, `find_older_compatible_tag`,
/// `extend/os/mac/utils/bottles.rb:43-58`).
pub(crate) fn builds_from_source(tags: &[String], mac: MacTag) -> bool {
    !tags.iter().any(|tag| {
        tag == "all"
            || tag_of(tag).is_some_and(|(arm, version)| arm == mac.arm && version <= mac.macos)
    })
}

#[derive(Deserialize)]
struct Root {
    #[serde(default)]
    formulae: Vec<Formula>,
}

#[derive(Deserialize)]
struct Formula {
    name: String,
    /// `{}` for a formula with no bottle at all, `{"stable": {"files":
    /// {...}}}` otherwise; absent from a brew that does not write it, whose
    /// formulae are not judged.
    #[serde(default)]
    bottle: Option<Bottle>,
}

#[derive(Deserialize)]
struct Bottle {
    #[serde(default)]
    stable: Option<Stable>,
}

#[derive(Deserialize)]
struct Stable {
    #[serde(default)]
    files: HashMap<String, serde_json::Value>,
}

/// The formulae of a `brew info --installed --json=v2` reply that no
/// bottle fits `mac` (`builds_from_source`), by name in the Cellar. A
/// reply that does not parse, and a formula whose entry has no `bottle`,
/// give none: nothing is claimed of what was not read.
pub(crate) fn formulae_built_from_source(json: &str, mac: MacTag) -> HashSet<String> {
    let Ok(root) = serde_json::from_str::<Root>(json) else {
        return HashSet::new();
    };
    root.formulae
        .into_iter()
        .filter_map(|formula| {
            let bottle = formula.bottle?;
            let tags: Vec<String> = bottle
                .stable
                .map(|stable| stable.files.into_keys().collect())
                .unwrap_or_default();
            builds_from_source(&tags, mac).then(|| {
                let name = formula.name;
                name.rsplit('/').next().unwrap_or(&name).to_string()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTEL_TAHOE: MacTag = MacTag {
        arm: false,
        macos: (26, 0),
    };
    const ARM_SONOMA: MacTag = MacTag {
        arm: true,
        macos: (14, 6),
    };
    const ARM_TAHOE: MacTag = MacTag {
        arm: true,
        macos: (26, 1),
    };

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_bottle_fits_for_the_same_processor_and_an_older_or_the_same_macos() {
        let newest = tags(&[
            "arm64_tahoe",
            "arm64_sequoia",
            "x86_64_linux",
            "arm64_linux",
        ]);
        assert!(!builds_from_source(&newest, ARM_TAHOE));
        // Homebrew 7 builds no Intel bottle, nor one for Apple silicon on
        // macOS 14 or older.
        assert!(builds_from_source(&newest, INTEL_TAHOE));
        assert!(builds_from_source(&newest, ARM_SONOMA));
        // A bottle built on an older macOS is poured on a newer one.
        assert!(!builds_from_source(&tags(&["sonoma"]), INTEL_TAHOE));
        assert!(!builds_from_source(&tags(&["arm64_ventura"]), ARM_SONOMA));
        assert!(!builds_from_source(&tags(&["all"]), INTEL_TAHOE));
        // No bottle at all, or only one for a release this build does not
        // know.
        assert!(builds_from_source(&[], ARM_TAHOE));
        assert!(builds_from_source(&tags(&["arm64_someday"]), ARM_TAHOE));
    }

    #[test]
    fn reads_which_installed_formulae_have_no_bottle_for_this_mac() {
        let json = r#"{"formulae":[
            {"name":"node","bottle":{"stable":{"rebuild":0,"files":{"arm64_tahoe":{"cellar":":any"},"arm64_sequoia":{}}}}},
            {"name":"jq","bottle":{"stable":{"files":{"all":{}}}}},
            {"name":"someone/tap/speedtest","full_name":"someone/tap/speedtest","bottle":{}},
            {"name":"old"}
        ],"casks":[]}"#;
        let built = |mac| {
            let mut names: Vec<String> =
                formulae_built_from_source(json, mac).into_iter().collect();
            names.sort();
            names
        };
        assert_eq!(built(ARM_TAHOE), ["speedtest"]);
        assert_eq!(built(ARM_SONOMA), ["node", "speedtest"]);
        assert!(formulae_built_from_source("not json", ARM_TAHOE).is_empty());
    }

    #[test]
    fn reads_macos_versions_and_judges_none_newer_than_it_knows() {
        assert_eq!(parse_version("26.0.1"), Some((26, 0)));
        assert_eq!(parse_version("10.15.7"), Some((10, 15)));
        assert_eq!(parse_version("27"), Some((27, 0)));
        assert_eq!(parse_version(""), None);
        assert!(known((27, 0)));
        assert!(known((11, 7)));
        assert!(!known((28, 0)));
    }
}
