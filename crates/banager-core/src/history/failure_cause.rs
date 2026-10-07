//! Why a tool failed, in one of a few words a person knows, read off the
//! last lines it wrote to stderr (`Outcome::Failed`'s `summary`) -- the
//! same reading as `failureCause` in src/lib/failureCause.ts, which the
//! window does for an operation it watched. The history (`super`) keeps
//! only the word, never the lines, so it is read here, as the operation
//! finishes, and the lines are dropped.
//!
//! Two copies of one rule: `failure_cause_cases.json` beside this file is
//! read by the tests on both sides (this module's and
//! src/lib/history.test.ts), so a phrase added to one and not the other
//! fails one of them.
//!
//! An operation's failure is read further (`operation_failure_cause`, r6
//! y3-batch): when none of those causes is named, for the ones only a
//! change can meet -- a file or an app in the way, something missing, a
//! Mac the version does not support, a step that ran too long. Its shared
//! cases are `operation_cause_cases.json` (src/lib/failureCause.test.ts
//! reads them too).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// The causes, by the names `FailureCause` has in src/lib/failureCause.ts:
/// camelCase on the wire (`rename_all`), so the window's type is the same
/// union of strings and its words (`FAILURE_CAUSE_KEYS`) apply as they are.
///
/// The first seven are read off any failure, a lookup's included
/// (`failure_cause`); the next six only off an operation's
/// (`operation_failure_cause`); the last two are never read off words but
/// kept by the history for a failure of Banager's own (`record_for`).
/// An older Banager reading a history record with one of the later ones
/// drops that record and keeps the rest (`load` reads them one by one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureCause {
    Network,
    DiskFull,
    Permission,
    Busy,
    HomebrewUpdating,
    NeedsPassword,
    PasswordNotAccepted,
    /// Something already where it installs: an app or a file of the same
    /// name, a link Homebrew would make, another package it conflicts
    /// with.
    Conflict,
    /// Something it needs is not there: a package its source no longer
    /// has, a program it runs (`env: node: No such file or directory`), a
    /// file a step looked for.
    NotFound,
    /// A Homebrew cask's app is not where it was installed: Homebrew
    /// backs the old app up before an upgrade and refuses with "It seems
    /// the App source '/Applications/…' is not there" when it has been
    /// moved to the Trash or deleted (`move_back`,
    /// cask/artifact/moved.rb in Homebrew 7.0.8).
    AppMissing,
    /// The version does not run on this Mac: its macOS, or its chip.
    Unsupported,
    /// A step of the tool's ran past the tool's own time limit -- not a
    /// download, which is `Network`, and not Banager's own deadline, which
    /// ends an operation `Unconfirmed`.
    TimedOut,
    /// Homebrew installed the new version of a formula but could not link
    /// it into its prefix (`FormulaInstaller#link`, formula_installer.rb
    /// in Homebrew 7.0.8): an upgrade unlinks the old version first, so
    /// the formula's commands may be gone from Terminal. Only "The `brew
    /// link` step did not complete successfully" reaches stderr; which
    /// file was in the way goes to stdout (review of r6 y3-batch,
    /// finding 2).
    NotLinked,
    /// Banager stopped because what it was about to change was no longer
    /// what the confirmation showed (`Fault::PathChanged`,
    /// `FormulaChanged`, `HomebrewSettingsChanged`).
    Changed,
    /// Banager itself failed (`Fault::Panicked`, `Fault::Internal`).
    Internal,
}

struct Patterns {
    no_way_to_ask: Vec<Regex>,
    asked_in_a_window: Vec<Regex>,
    password_required: Regex,
    by_cause: Vec<(FailureCause, Vec<Regex>)>,
    /// `operation_failure_cause`'s, read only where `by_cause` and the
    /// password lines named nothing.
    by_operation_cause: Vec<(FailureCause, Vec<Regex>)>,
}

fn compile(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .map(|p| Regex::new(p).expect("a fixed pattern compiles"))
        .collect()
}

/// The phrases of src/lib/failureCause.ts, in its order and with its
/// flags: `(?i)` where it has `/i`.
fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        no_way_to_ask: compile(&[
            r"(?i)\bsudo: a terminal is required to read the password\b",
            r"(?i)\bsudo: no tty present and no askpass program specified\b",
            r"(?i)\bsudo: no askpass program specified\b",
        ]),
        asked_in_a_window: compile(&[
            r"(?i)\bsudo: no password was provided\b",
            r"(?i)\bsudo: \d+ incorrect password attempts?\b",
        ]),
        password_required: Regex::new(r"(?i)\bsudo: a password is required\b")
            .expect("a fixed pattern compiles"),
        by_cause: vec![
            (
                FailureCause::HomebrewUpdating,
                compile(&[
                    r"(?i)\bbrew update\b.*(?:time[sd]? out|still running|超时)",
                    r"(?i)\bHomebrew\b.*\bstill updating\b",
                ]),
            ),
            (
                FailureCause::DiskFull,
                compile(&[
                    r"(?i)No space left on device",
                    r"\bENOSPC\b",
                    r"(?i)\bDisk quota exceeded\b",
                    r"(?i)\bdisk is full\b",
                ]),
            ),
            (
                FailureCause::Permission,
                compile(&[
                    r"(?i)Permission denied",
                    r"(?i)Operation not permitted",
                    r"\bEACCES\b",
                    r"\bEPERM\b",
                    r"(?i)\bare not writable by your user\b",
                    r"(?i)\bis not writable\b",
                ]),
            ),
            (
                FailureCause::Busy,
                compile(&[
                    r"(?i)\balready locked\b",
                    r"(?i)\bhas already locked\b",
                    r"(?i)\bis locked by another process\b",
                    r"(?i)\banother active Homebrew\b",
                    r"(?i)\banother `?brew [a-z]+`? process is already (?:running|in progress)\b",
                    r"(?i)\bprocess is already (?:running|in progress)\b",
                    r"(?i)\bwaiting for file lock\b",
                ]),
            ),
            (
                FailureCause::Network,
                compile(&[
                    r"(?i)\bcould(?: not|n't) resolve host\b",
                    r"(?i)\btimed out\b",
                    r"(?i)\bTimeout was reached\b",
                    r"(?i)\bconnection (?:refused|reset|aborted)\b",
                    r"(?i)\bremote end closed connection without response\b",
                    r"(?i)\bnetwork is unreachable\b",
                    r"(?i)\bno route to host\b",
                    r"(?i)\bTemporary failure in name resolution\b",
                    r"(?i)\bnodename nor servname provided\b",
                    r"(?i)\bno such host\b",
                    r"(?i)\bi/o timeout\b",
                    r"(?i)\bdns error\b",
                    r"(?i)\berror sending request\b",
                    r"(?i)\bFailed to establish a new connection\b",
                    r"(?i)\bMax retries exceeded\b",
                    r"(?i)\bFailed to download\b",
                    r"(?i)\bfailed to fetch\b",
                    r"\bcurl: \((?:6|7|28|35|52|56)\)",
                    r"\b(?:ENOTFOUND|EAI_AGAIN|ECONNREFUSED|ECONNRESET|ETIMEDOUT|ENETUNREACH)\b",
                    r"(?i)\bnpm (?:ERR!|error) network\b",
                    r"\bSSL(?:Error|_ERROR\w*|\b)",
                    r"(?i)\bssl certificate\b",
                    r"(?i)\bcertificate verify failed\b",
                ]),
            ),
        ],
        // `OPERATION_PATTERNS` in src/lib/failureCause.ts, in its order.
        by_operation_cause: vec![
            (
                FailureCause::NotLinked,
                compile(&[
                    r"(?i)\bThe `brew link` step did not complete successfully\b",
                    r"(?i)\bAn unexpected error occurred during the `brew link` step\b",
                ]),
            ),
            (
                FailureCause::AppMissing,
                compile(&[
                    r"(?i)\bIt seems the App source '[^']*/Applications/[^']*' is not there\b",
                ]),
            ),
            (
                FailureCause::Unsupported,
                compile(&[
                    r"(?i)\bdoes not run on macOS versions\b",
                    r"(?i)\bis not available on macOS\b",
                    r"(?i)\bdepends on hardware architecture being one of\b",
                    r"(?i)\beither does not compile or function as expected on macOS\b",
                    r"\bEBADPLATFORM\b",
                    r"(?i)\bis not a supported wheel on this platform\b",
                ]),
            ),
            (
                FailureCause::Conflict,
                compile(&[
                    r"(?i)\bIt seems there is already an? [A-Za-z ]+ at\b",
                    r"(?i)\bconflicts with '",
                    r"(?i)\bconflicting formulae\b",
                    r"(?i)\bCould not symlink\b",
                    r"(?i)\balready exists\. You may want to remove it\b",
                    r"\bEEXIST\b",
                    r"(?i)\bbinary `[^`]+` already exists in destination\b",
                    r"(?i)\bExecutable already exists\b",
                ]),
            ),
            (
                FailureCause::NotFound,
                compile(&[
                    r"(?i)\bIt seems the [A-Za-z ]+ source '[^']*' is not there\b",
                    r"(?i)\bNo available (?:formula|cask)\b",
                    r"(?i)\bNo (?:cask|formula|formulae) with (?:this|the) name\b",
                    r"(?i)\b(?:Cask|Formula) '[^']+' is not installed\b",
                    r"(?i)\bNo such keg\b",
                    r"\bE404\b",
                    r"(?i)\b404 Not Found\b",
                    r"(?i)\bis not in this registry\b",
                    r"(?i)\bcommand not found\b",
                    r"(?i)\bNo such file or directory\b",
                    r"\bENOENT\b",
                    r"(?i)\bNo matching distribution found\b",
                    r"(?i)\bcould not find `[^`]+` in registry\b",
                    r"(?i)\bwas not found in the package registry\b",
                    r"(?i)\bfile does not exist\b",
                ]),
            ),
            (
                FailureCause::Permission,
                compile(&[
                    r"(?i)\bFailed to quarantine\b",
                    r"(?i)\bFailed to release .* from quarantine\b",
                    r"(?i)\bCannot remove undeletable\b",
                ]),
            ),
            (
                FailureCause::TimedOut,
                compile(&[
                    r"\bTimeout::Error\b",
                    r"(?i)\bexecution expired\b",
                    r"(?i)\bdid not finish within\b",
                ]),
            ),
        ],
    })
}

/// `text`'s lines that are not blank, without a trailing `\r`.
fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// The first of `by_cause` that a line of `lines` names, from the last
/// line up.
fn last_named(lines: &[&str], by_cause: &[(FailureCause, Vec<Regex>)]) -> Option<FailureCause> {
    for line in lines.iter().rev() {
        for (cause, patterns) in by_cause {
            if patterns.iter().any(|re| re.is_match(line)) {
                return Some(*cause);
            }
        }
    }
    None
}

/// Why an operation failed, by the words of the tool that failed it: the
/// causes `failure_cause` reads first, on all of `text`; then, only where
/// it names none, the ones only a change meets (`by_operation_cause`),
/// each line from the last up. Two passes, not one: a network failure a
/// tool retried and then gave up on in other words -- pip's "No matching
/// distribution found" after its retries -- stays the network.
/// `operationFailureCause` in src/lib/failureCause.ts is the same rule.
pub fn operation_failure_cause(text: &str) -> Option<FailureCause> {
    failure_cause(text).or_else(|| last_named(&lines_of(text), &patterns().by_operation_cause))
}

/// The cause `text` names, or `None`: sudo's password lines first, on any
/// line, then each line from the last up, the first phrase that matches
/// deciding -- `failureCause` in src/lib/failureCause.ts says why.
pub fn failure_cause(text: &str) -> Option<FailureCause> {
    let p = patterns();
    let lines = lines_of(text);
    let any = |patterns: &[Regex]| {
        lines
            .iter()
            .any(|line| patterns.iter().any(|re| re.is_match(line)))
    };
    if any(&p.no_way_to_ask) {
        return Some(FailureCause::NeedsPassword);
    }
    if any(&p.asked_in_a_window) {
        return Some(FailureCause::PasswordNotAccepted);
    }
    if lines.iter().any(|line| p.password_required.is_match(line)) {
        return Some(FailureCause::NeedsPassword);
    }
    last_named(&lines, &p.by_cause)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        text: String,
        cause: Option<FailureCause>,
    }

    #[test]
    fn test_failure_cause_reads_every_shared_case_as_the_window_does() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("failure_cause_cases.json")).expect("cases parse");
        assert!(cases.len() >= 30, "the shared cases are all there");
        for case in cases {
            assert_eq!(failure_cause(&case.text), case.cause, "{}", case.name);
        }
    }

    #[test]
    fn test_operation_failure_cause_reads_every_shared_case_as_the_window_does() {
        // r6 y3-batch, finding 3: an operation's failure is read for the
        // causes a lookup's is, and then for the ones only a change can
        // meet -- a conflict, something missing, a Mac it does not support,
        // a step that ran too long. `operation_cause_cases.json` is read by
        // src/lib/failureCause.test.ts too.
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("operation_cause_cases.json")).expect("cases parse");
        assert!(cases.len() >= 30, "the shared cases are all there");
        for case in cases {
            assert_eq!(
                operation_failure_cause(&case.text),
                case.cause,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn test_a_lookup_is_read_for_the_first_causes_only() {
        // `failure_cause`, which a lookup's words are read with too
        // (adapters/mod.rs), names none of the causes only a change meets.
        assert_eq!(
            failure_cause("Error: It seems the App source '/Applications/Foo.app' is not there."),
            None
        );
        assert_eq!(failure_cause("env: node: No such file or directory"), None);
        // Every shared case of the first causes reads the same through both.
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("failure_cause_cases.json")).expect("cases parse");
        for case in cases.into_iter().filter(|case| case.cause.is_some()) {
            assert_eq!(
                operation_failure_cause(&case.text),
                case.cause,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn test_failure_cause_wire_names_are_the_windows() {
        assert_eq!(
            serde_json::to_string(&[
                FailureCause::Network,
                FailureCause::DiskFull,
                FailureCause::Permission,
                FailureCause::Busy,
                FailureCause::HomebrewUpdating,
                FailureCause::NeedsPassword,
                FailureCause::PasswordNotAccepted,
                FailureCause::Conflict,
                FailureCause::NotFound,
                FailureCause::AppMissing,
                FailureCause::Unsupported,
                FailureCause::TimedOut,
                FailureCause::NotLinked,
                FailureCause::Changed,
                FailureCause::Internal,
            ])
            .unwrap(),
            r#"["network","diskFull","permission","busy","homebrewUpdating","needsPassword","passwordNotAccepted","conflict","notFound","appMissing","unsupported","timedOut","notLinked","changed","internal"]"#
        );
    }
}
