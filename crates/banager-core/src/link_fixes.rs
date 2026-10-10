//! What would put a missing program back (`NoAnswer::link_fixes`): for a
//! source whose launcher could not start for want of a program on `PATH`
//! (`NoAnswer::missing_program`, `node` for npm's `#!/usr/bin/env node`),
//! the Homebrew formulae that provide it and are installed but not linked.
//!
//! The case it exists for (finding (1) of the 2026-10-07 run): `node@22`
//! is keg-only, so Homebrew puts none of its commands where Terminal looks
//! unless someone runs `brew link --force node@22`; the author had. Then
//! npm updated itself (`npm install -g npm@latest`), which replaced the
//! `bin/npm` link `node@22` had put there with npm's own; the next `brew
//! upgrade node@22` unlinked the old version and, linking the new one, met
//! that file, which Homebrew had not linked: it took back what it had
//! linked and failed ("The `brew link` step did not complete
//! successfully"). Homebrew does link a keg again that was linked before an
//! upgrade (upgrade.rb:643 in Homebrew 7.0.8); the file in the way stopped
//! it. `/opt/homebrew/bin/node` was gone, and with it every npm command.
//! `brew link --formula --force node@22` puts it back once nothing is in
//! the way -- the same command, read the same way (`adapters/brew/links.rs`),
//! that links a keg-only formula back after its update (y1-keg); npm's own
//! update is no longer offered where npm is a formula's, so it does not
//! happen again that way (`UpdateBlocked::UpdatesWithFormula`).
//!
//! A formula is offered when it is, by the snapshot alone -- nothing is
//! read from the disk and nothing runs here:
//! - a formula of a Homebrew source Banager can act on (writable and
//!   answering: the gate `Session::issue_plan` keeps);
//! - keg-only, not because of macOS, and not linked (`CommandInputs::keg_only`,
//!   `keg_only_by_macos`, `link_recorded`, from `brew info --installed
//!   --json=v2`'s `keg_only`, `keg_only_reason` and `linked_keg`): `brew
//!   link` refuses a formula macOS provides at Homebrew's default prefix,
//!   and the link's plan refuses it too (y1-keg's rule); "linked" is
//!   Homebrew's record of a `brew link`, as everywhere (`link_recorded`);
//! - named for the program: `node`, or `node@<version>` -- Homebrew's own
//!   naming for a formula that installs `node`. A formula that provides it
//!   under another name is not offered: nothing Banager reads says so.
//!
//! Newest first: by the version in the name (`node@22` before `node@20`),
//! the unversioned formula before every versioned one, then by the version
//! the row shows. Worked out once a refresh round has every source's rows
//! (`Session::refresh`), over all of them.

use crate::model::{ArtifactKind, InstalledArtifact, LinkFix, ManagerInstance, NoAnswerKind};
use std::cmp::Ordering;
use std::collections::HashSet;

/// Sets `link_fixes` of every instance's `NoAnswer` from `artifacts`:
/// the formulae that would put its missing program back, newest first; none
/// for any other reason it did not answer.
pub fn fill(instances: &mut [ManagerInstance], artifacts: &[InstalledArtifact]) {
    let homebrews: HashSet<String> = instances
        .iter()
        .filter(|instance| {
            instance.adapter_id == "brew" && instance.writable() && instance.available()
        })
        .map(|instance| instance.id.clone())
        .collect();
    for instance in instances.iter_mut() {
        let Some(why) = instance.status.no_answer.as_mut() else {
            continue;
        };
        why.link_fixes = match (&why.missing_program, why.kind) {
            (Some(program), NoAnswerKind::CouldNotStart) => {
                fixes_for(program, &homebrews, artifacts)
            }
            _ => Vec::new(),
        };
    }
}

/// The formulae of `homebrews` that would put `program` back, newest first.
fn fixes_for(
    program: &str,
    homebrews: &HashSet<String>,
    artifacts: &[InstalledArtifact],
) -> Vec<LinkFix> {
    let mut found: Vec<(&InstalledArtifact, Option<&str>)> = artifacts
        .iter()
        .filter(|artifact| {
            artifact.key.kind == ArtifactKind::Formula
                && homebrews.contains(&artifact.key.instance_id)
                && artifact.facts.command_inputs.keg_only
                && !artifact.facts.command_inputs.keg_only_by_macos
                && !artifact.facts.command_inputs.link_recorded
        })
        .filter_map(|artifact| {
            named_for(&artifact.key.name, program).map(|version| (artifact, version))
        })
        .collect();
    found.sort_by(|(a, a_named), (b, b_named)| {
        by_named_version(*b_named, *a_named)
            .then_with(|| compare_versions(&b.version, &a.version))
            .then_with(|| a.key.name.cmp(&b.key.name))
    });
    found
        .into_iter()
        .map(|(artifact, _)| LinkFix {
            key: artifact.key.clone(),
            version: artifact.version.clone(),
        })
        .collect()
}

/// Whether the formula `name` (a tap's `user/tap/name` read by its last
/// part) is named for `program`: `Some(None)` for `program` itself,
/// `Some(Some(version))` for `program@version`, `None` for anything else
/// (`nodenv`, `node-build`).
fn named_for<'a>(name: &'a str, program: &str) -> Option<Option<&'a str>> {
    let short = name.rsplit('/').next().unwrap_or(name);
    if short == program {
        return Some(None);
    }
    let version = short.strip_prefix(program)?.strip_prefix('@')?;
    (!version.is_empty()).then_some(Some(version))
}

/// The version in two formulae's names, the unversioned one the newer.
fn by_named_version(a: Option<&str>, b: Option<&str>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => compare_versions(a, b),
    }
}

/// Two versions, part by part (split at `.`, `_`, `-` and `+`): parts that
/// are numbers compare as numbers (`3.14` after `3.9`), others as text, and
/// a version that runs out first is the older.
fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |version: &str| -> Vec<String> {
        version
            .split(['.', '_', '-', '+'])
            .map(str::to_string)
            .collect()
    };
    let (a, b) = (parts(a), parts(b));
    for (x, y) in a.iter().zip(b.iter()) {
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtifactKey, InstanceStatus, NoAnswer, ReadOnlyReason, Unavailable};
    use crate::testing::{installed_artifact, manager_instance};

    const BREW: &str = "brew:/opt/homebrew";

    fn npm_without(program: Option<&str>, kind: NoAnswerKind) -> ManagerInstance {
        ManagerInstance {
            status: InstanceStatus {
                unavailable: Some(Unavailable::NotResponding),
                notes: Vec::new(),
                no_answer: Some(NoAnswer {
                    diagnostic: None,
                    cause: None,
                    kind,
                    missing_program: program.map(str::to_string),
                    link_fixes: Vec::new(),
                }),
            },
            ..manager_instance("npm", "npm:/opt/homebrew")
        }
    }

    fn formula(name: &str, version: &str, keg_only: bool, linked: bool) -> InstalledArtifact {
        let mut artifact = installed_artifact(BREW, ArtifactKind::Formula, name);
        artifact.version = version.to_string();
        artifact.facts.command_inputs.keg_only = keg_only;
        artifact.facts.command_inputs.link_recorded = linked;
        artifact
    }

    fn fix(name: &str, version: &str) -> LinkFix {
        LinkFix {
            key: ArtifactKey {
                instance_id: BREW.to_string(),
                kind: ArtifactKind::Formula,
                name: name.to_string(),
            },
            version: version.to_string(),
        }
    }

    fn fixes_of(instance: &ManagerInstance) -> Vec<LinkFix> {
        instance
            .status
            .no_answer
            .as_ref()
            .map(|why| why.link_fixes.clone())
            .unwrap_or_default()
    }

    fn brew() -> ManagerInstance {
        manager_instance("brew", BREW)
    }

    #[test]
    fn test_the_unlinked_node_formulae_are_offered_newest_first() {
        // The author's Mac, with a node@20 beside it: both keg-only and
        // unlinked, so either would put `node` back.
        let mut instances = vec![
            brew(),
            npm_without(Some("node"), NoAnswerKind::CouldNotStart),
        ];
        let artifacts = vec![
            formula("node@20", "20.19.5", true, false),
            formula("jq", "1.8.1", false, true),
            formula("node@22", "22.23.3_1", true, false),
            formula("nodenv", "1.6.2", true, false),
            formula("node-build", "5.4.1", true, false),
        ];
        fill(&mut instances, &artifacts);
        assert_eq!(
            fixes_of(&instances[1]),
            vec![fix("node@22", "22.23.3_1"), fix("node@20", "20.19.5")]
        );
        assert_eq!(instances[0].status.no_answer, None, "Homebrew answered");
    }

    #[test]
    fn test_newest_is_by_the_version_in_the_name_then_the_rows() {
        let mut instances = vec![
            brew(),
            npm_without(Some("python3"), NoAnswerKind::CouldNotStart),
        ];
        let artifacts = vec![
            formula("python3@3.9", "3.9.25", true, false),
            formula("python3@3.14", "3.14.8", true, false),
            formula("python3", "3.14.0", true, false),
            formula("python3@3.13", "3.13.16", true, false),
        ];
        fill(&mut instances, &artifacts);
        let names: Vec<String> = fixes_of(&instances[1])
            .into_iter()
            .map(|fix| fix.key.name)
            .collect();
        assert_eq!(
            names,
            vec!["python3", "python3@3.14", "python3@3.13", "python3@3.9"]
        );
    }

    #[test]
    fn test_nothing_is_offered_that_would_not_put_it_back() {
        // Linked already (something else is wrong), not keg-only (Homebrew
        // links it itself), keg-only because of macOS (`brew link` refuses
        // it, y1-keg), a cask, another source's package of that name.
        let mut instances = vec![
            brew(),
            npm_without(Some("node"), NoAnswerKind::CouldNotStart),
        ];
        let mut cask = installed_artifact(BREW, ArtifactKind::Cask, "node@22");
        cask.facts.command_inputs.keg_only = true;
        let mut npm_package =
            installed_artifact("npm:/opt/homebrew", ArtifactKind::Formula, "node");
        npm_package.facts.command_inputs.keg_only = true;
        let mut by_macos = formula("node@20", "20.19.5", true, false);
        by_macos.facts.command_inputs.keg_only_by_macos = true;
        let artifacts = vec![
            formula("node@22", "22.23.3_1", true, true),
            formula("node", "26.0.0", false, false),
            by_macos,
            cask,
            npm_package,
        ];
        fill(&mut instances, &artifacts);
        assert_eq!(fixes_of(&instances[1]), Vec::new());
    }

    #[test]
    fn test_only_a_missing_program_has_fixes() {
        // Timed out, ran and failed, or could not start with no program
        // named: no fix, and one a previous round wrote is taken off.
        let artifacts = vec![formula("node@22", "22.23.3_1", true, false)];
        for (program, kind) in [
            (Some("node"), NoAnswerKind::TimedOut),
            (Some("node"), NoAnswerKind::ExitedWithError),
            (None, NoAnswerKind::CouldNotStart),
        ] {
            let mut npm = npm_without(program, kind);
            if let Some(why) = npm.status.no_answer.as_mut() {
                why.link_fixes = vec![fix("node@22", "22.23.3_1")];
            }
            let mut instances = vec![brew(), npm];
            fill(&mut instances, &artifacts);
            assert_eq!(fixes_of(&instances[1]), Vec::new(), "{kind:?} {program:?}");
        }
    }

    #[test]
    fn test_a_homebrew_banager_cannot_act_on_offers_nothing() {
        // The gate refuses a plan on it (`Session::issue_plan`), so no
        // button is offered for one.
        let artifacts = vec![formula("node@22", "22.23.3_1", true, false)];
        for homebrew in [
            ManagerInstance {
                status: InstanceStatus {
                    unavailable: Some(Unavailable::RefusesAsRoot),
                    ..InstanceStatus::default()
                },
                ..brew()
            },
            ManagerInstance {
                read_only_reason: Some(ReadOnlyReason::PrefixNotWritable),
                ..brew()
            },
        ] {
            let mut instances = vec![
                homebrew,
                npm_without(Some("node"), NoAnswerKind::CouldNotStart),
            ];
            fill(&mut instances, &artifacts);
            assert_eq!(fixes_of(&instances[1]), Vec::new());
        }
        // And with no Homebrew in the snapshot at all.
        let mut instances = vec![npm_without(Some("node"), NoAnswerKind::CouldNotStart)];
        fill(&mut instances, &artifacts);
        assert_eq!(fixes_of(&instances[0]), Vec::new());
    }

    #[test]
    fn test_a_formula_named_for_the_program_and_an_empty_version_is_not_offered() {
        // `node@` is neither `node` nor `node@<version>`.
        let mut instances = vec![
            brew(),
            npm_without(Some("node"), NoAnswerKind::CouldNotStart),
        ];
        fill(
            &mut instances,
            &[
                formula("node@", "22.23.3_1", true, false),
                formula("node@22", "22.23.3_1", true, false),
            ],
        );
        assert_eq!(fixes_of(&instances[1]), vec![fix("node@22", "22.23.3_1")]);
    }

    #[test]
    fn test_a_homebrew_revision_is_newer_than_the_version_it_revises() {
        // `22.23.3_1` is the formula's revision 1 of `22.23.3`: every part
        // the same, and one more. A version that runs out first is the older,
        // in a name (`node@22.1` after `node@22`) as in a row.
        assert_eq!(compare_versions("22.23.3_1", "22.23.3"), Ordering::Greater);
        assert_eq!(compare_versions("22.23.3", "22.23.3_1"), Ordering::Less);
        assert_eq!(compare_versions("22.23.3", "22.23.3"), Ordering::Equal);
        assert_eq!(
            by_named_version(Some("22.1"), Some("22")),
            Ordering::Greater
        );
    }
}
