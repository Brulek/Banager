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

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// The causes, by the names `FailureCause` has in src/lib/failureCause.ts:
/// camelCase on the wire (`rename_all`), so the window's type is the same
/// union of strings and its words (`FAILURE_CAUSE_KEYS`) apply as they are.
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
}

struct Patterns {
    no_way_to_ask: Vec<Regex>,
    asked_in_a_window: Vec<Regex>,
    password_required: Regex,
    by_cause: Vec<(FailureCause, Vec<Regex>)>,
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
    })
}

/// The cause `text` names, or `None`: sudo's password lines first, on any
/// line, then each line from the last up, the first phrase that matches
/// deciding -- `failureCause` in src/lib/failureCause.ts says why.
pub fn failure_cause(text: &str) -> Option<FailureCause> {
    let p = patterns();
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.trim().is_empty())
        .collect();
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
    for line in lines.iter().rev() {
        for (cause, patterns) in &p.by_cause {
            if patterns.iter().any(|re| re.is_match(line)) {
                return Some(*cause);
            }
        }
    }
    None
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
            ])
            .unwrap(),
            r#"["network","diskFull","permission","busy","homebrewUpdating","needsPassword","passwordNotAccepted"]"#
        );
    }
}
