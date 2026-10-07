//! Why a source did not answer (`model::NoAnswer`): what the command Banager
//! asked it with did -- timed out, could not start, or ran and ended with an
//! error -- and, when its launcher's `#!/usr/bin/env <program>` found no
//! `<program>` on `PATH`, which program that was.
//!
//! Read off the command's own result (`CommandOutput`, or the
//! `RunnerError` that means it never started), never off text Banager
//! wrote. The program `env` names is read from stderr as the command wrote
//! it, before a login was masked out of it (`StderrCause::Read`), as an
//! operation's failure cause is (`CommandOutput::failure_cause`): a proxy
//! user name `node` would mask the very word.
//!
//! Every adapter whose `detect` turns a failed command into
//! `Unavailable::NotResponding` hands that command's result to [`of`].

use super::{CommandOutput, RunnerError};
use crate::model::{NoAnswer, NoAnswerKind};
use regex::Regex;
use std::sync::LazyLock;

/// `env`'s "not found", whole line: macOS's `env: node: …` and GNU's
/// `/usr/bin/env: 'node': …` (`‘’` in a UTF-8 locale), the name a bare
/// word a `PATH` lookup takes.
static ENV_NOT_FOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:/usr/bin/)?env: ['\x{2018}]?([A-Za-z0-9_][A-Za-z0-9._+@-]*)['\x{2019}]?: No such file or directory$",
    )
    .expect("a fixed pattern compiles")
});

/// The program `env` said it could not find, by the lines of `text` from
/// the last up: macOS's `env: node: No such file or directory`, and GNU
/// env's `/usr/bin/env: 'node': No such file or directory` (either quote).
/// A bare name only -- what a launcher's `#!/usr/bin/env <name>` looks up
/// on `PATH` -- never a path; and only "No such file or directory", not
/// "Permission denied" (found, but not runnable). `None` when no line says
/// so.
pub fn missing_program(text: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        ENV_NOT_FOUND
            .captures(line.trim_end())
            .map(|found| found[1].to_string())
    })
}

/// Why the command a source was asked with gave no answer, or `None` when
/// it answered (exit 0, whatever it said), was stopped by Banager's own
/// Cancel, or failed in a way that is Banager's to say (`NoMock`, a
/// parser's stdout too large to read):
///
/// - `TimedOut`: the runner stopped it when its time ran out.
/// - `CouldNotStart`: its program is not there or would not start
///   (`NotFound`, `Spawn`), or it exited 126 (found, not runnable) or 127
///   (not found) -- with, for 127, the program `env` names
///   (`CommandOutput::missing_program`).
/// - `ExitedWithError`: any other non-zero exit, or a signal.
///
/// `link_fixes` is always empty here: what would put a missing program back
/// is worked out over the whole snapshot (`link_fixes::fill`).
pub fn of(result: &Result<CommandOutput, RunnerError>) -> Option<NoAnswer> {
    let (kind, missing_program) = match result {
        Err(RunnerError::NotFound(_) | RunnerError::Spawn(_)) => {
            (NoAnswerKind::CouldNotStart, None)
        }
        Err(RunnerError::NoMock(_) | RunnerError::OutputTooLarge { .. }) => return None,
        Ok(output) if output.cancelled => return None,
        Ok(output) if output.timed_out => (NoAnswerKind::TimedOut, None),
        Ok(output) => match output.exit_code {
            Some(0) => return None,
            Some(126) => (NoAnswerKind::CouldNotStart, None),
            Some(127) => (NoAnswerKind::CouldNotStart, output.missing_program()),
            _ => (NoAnswerKind::ExitedWithError, None),
        },
    };
    let diagnostic = result.as_ref().ok().and_then(|output| {
        let summary = super::failure_summary(&output.stderr);
        // Keep the tail, on a UTF-8 boundary, after redaction and line selection.
        let mut start = summary.len().saturating_sub(4096);
        while !summary.is_char_boundary(start) {
            start += 1;
        }
        let tail = summary[start..].trim();
        (!tail.is_empty()).then(|| tail.to_string())
    });
    Some(NoAnswer {
        diagnostic,
        cause: result.as_ref().ok().and_then(CommandOutput::failure_cause),
        kind,
        missing_program,
        link_fixes: Vec::new(),
    })
}

/// [`of`], for a command whose answer Banager read as `answer` (a
/// `--version`'s version): no reason when it read one, whatever the command
/// did -- an answer is an answer.
pub fn unless_answered<T>(
    answer: &Option<T>,
    result: &Result<CommandOutput, RunnerError>,
) -> Option<NoAnswer> {
    if answer.is_some() {
        None
    } else {
        of(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::StderrCause;
    use std::path::PathBuf;

    fn ran(exit_code: Option<i32>, stderr: &str) -> CommandOutput {
        CommandOutput {
            exit_code,
            stdout: String::new(),
            stderr: stderr.to_string(),
            timed_out: false,
            cancelled: false,
            stderr_cause: StderrCause::InStderr,
        }
    }

    fn said(kind: NoAnswerKind, missing_program: Option<&str>) -> Option<NoAnswer> {
        Some(NoAnswer {
            diagnostic: None,
            cause: None,
            kind,
            missing_program: missing_program.map(str::to_string),
            link_fixes: Vec::new(),
        })
    }

    #[test]
    fn test_detect_diagnostic_is_bounded_redacted_and_keeps_cause_read_before_masking() {
        let raw = format!(
            "{}\nsudo: a password is required\nhttps://person:secret@proxy.test",
            "界".repeat(5000)
        );
        let redactor = crate::runner::redact::Redactor::for_settings([(
            "HTTPS_PROXY",
            "http://person:pass@proxy.test",
        )]);
        let output = CommandOutput {
            stderr_cause: StderrCause::Read {
                cause: Some(crate::history::FailureCause::NeedsPassword),
                missing_program: None,
            },
            ..ran(Some(1), &redactor.redact(&raw))
        };
        let why = of(&Ok(output)).unwrap();
        let diagnostic = why.diagnostic.as_deref().unwrap();
        assert!(diagnostic.len() <= 4096);
        assert!(!diagnostic.contains("secret"));
        assert!(!diagnostic.contains("person"));
        assert!(!diagnostic.contains("password"));
        assert!(diagnostic.contains("****word"));
        assert_eq!(why.cause, Some(crate::history::FailureCause::NeedsPassword));
        let wire = serde_json::to_value(&why).unwrap();
        assert_eq!(wire["cause"], "needsPassword");
        assert_eq!(serde_json::from_value::<NoAnswer>(wire).unwrap(), why);
    }

    const ENV_NODE: &str = "env: node: No such file or directory\n";

    #[test]
    fn test_each_way_a_source_can_fail_to_answer_is_told_apart() {
        use NoAnswerKind::*;
        let timed_out = CommandOutput {
            timed_out: true,
            ..ran(None, "")
        };
        let cancelled = CommandOutput {
            cancelled: true,
            ..ran(None, "")
        };
        let cases: Vec<(&str, Result<CommandOutput, RunnerError>, Option<NoAnswer>)> = vec![
            // npm's launcher, `#!/usr/bin/env node`, with no node on PATH:
            // what the author's npm did on 2026-10-07.
            (
                "env found no node",
                Ok(ran(Some(127), ENV_NODE)),
                said(CouldNotStart, Some("node")),
            ),
            (
                "127 without env's words",
                Ok(ran(Some(127), "sh: foo: command not found\n")),
                said(CouldNotStart, None),
            ),
            (
                "126: found, not runnable",
                Ok(ran(Some(126), "env: node: Permission denied\n")),
                said(CouldNotStart, None),
            ),
            (
                "the program is not there",
                Err(RunnerError::NotFound(PathBuf::from(
                    "/opt/homebrew/bin/npm",
                ))),
                said(CouldNotStart, None),
            ),
            (
                "macOS would not start it",
                Err(RunnerError::Spawn(std::io::Error::from(
                    std::io::ErrorKind::PermissionDenied,
                ))),
                said(CouldNotStart, None),
            ),
            ("it ran out of time", Ok(timed_out), said(TimedOut, None)),
            (
                "it ran and failed",
                Ok(ran(Some(1), "npm error code EJSONPARSE\n")),
                said(ExitedWithError, None),
            ),
            (
                "env's words, but not 127",
                Ok(ran(Some(1), ENV_NODE)),
                said(ExitedWithError, None),
            ),
            (
                "a signal it did not get from Banager",
                Ok(ran(None, "")),
                said(ExitedWithError, None),
            ),
            ("it answered", Ok(ran(Some(0), "")), None),
            ("Banager's own Cancel", Ok(cancelled), None),
            (
                "too much to parse is Banager's to say",
                Err(RunnerError::OutputTooLarge { limit: 1 }),
                None,
            ),
            (
                "a test's missing canned answer",
                Err(RunnerError::NoMock(vec!["npm".to_string()])),
                None,
            ),
        ];
        for (case, result, expected) in cases {
            let actual = of(&result).map(|mut why| {
                why.diagnostic = None;
                why.cause = None;
                why
            });
            assert_eq!(actual, expected, "{case}");
        }
    }

    #[test]
    fn test_the_missing_program_is_read_from_stderr_as_the_command_wrote_it() {
        // A proxy user name `node` masks the word in what the window is
        // shown; the runner read it first.
        let masked = CommandOutput {
            stderr_cause: StderrCause::Read {
                cause: None,
                missing_program: Some("node".to_string()),
            },
            ..ran(Some(127), "env: ****: No such file or directory\n")
        };
        assert_eq!(
            of(&Ok(masked)).map(|mut why| {
                why.diagnostic = None;
                why
            }),
            said(NoAnswerKind::CouldNotStart, Some("node"))
        );
        let read_none = CommandOutput {
            stderr_cause: StderrCause::Read {
                cause: None,
                missing_program: None,
            },
            ..ran(Some(127), ENV_NODE)
        };
        assert_eq!(
            of(&Ok(read_none)).map(|mut why| {
                why.diagnostic = None;
                why
            }),
            said(NoAnswerKind::CouldNotStart, None),
            "what the runner read stands"
        );
    }

    #[test]
    fn test_only_envs_not_found_line_names_a_program() {
        for (text, expected) in [
            ("env: node: No such file or directory", Some("node")),
            ("env: node: No such file or directory\r\n", Some("node")),
            (
                "env: python3.13: No such file or directory",
                Some("python3.13"),
            ),
            (
                "/usr/bin/env: 'node': No such file or directory",
                Some("node"),
            ),
            (
                "/usr/bin/env: \u{2018}node\u{2019}: No such file or directory",
                Some("node"),
            ),
            (
                "warning: old npm\nenv: node: No such file or directory\n",
                Some("node"),
            ),
            ("env: node: Permission denied", None),
            (
                "env: /opt/homebrew/bin/node: No such file or directory",
                None,
            ),
            ("sh: node: command not found", None),
            ("npm error: env: node: No such file or directory", None),
            ("", None),
        ] {
            assert_eq!(missing_program(text).as_deref(), expected, "{text:?}");
        }
    }

    #[test]
    fn test_the_last_not_found_line_names_the_program() {
        // A launcher that tries `python3`, warns, then falls back to
        // `node`: the last line `env` wrote is what ended it.
        let stderr = "env: python3: No such file or directory\nwarning: trying node\nenv: node: No such file or directory\n";
        assert_eq!(missing_program(stderr).as_deref(), Some("node"));
        assert_eq!(
            of(&Ok(ran(Some(127), stderr))),
            said(NoAnswerKind::CouldNotStart, Some("node"))
        );
    }

    #[test]
    fn test_blanks_at_the_end_of_envs_line_do_not_hide_the_program() {
        // Spaces or a tab before the newline, or a carriage return with
        // no newline after it, which `str::lines` leaves on the line.
        for text in [
            "env: node: No such file or directory \t\n",
            "env: node: No such file or directory\r",
        ] {
            assert_eq!(missing_program(text).as_deref(), Some("node"), "{text:?}");
        }
    }

    #[test]
    fn test_exit_126_names_no_program_whatever_stderr_says() {
        // 126 is found but not runnable: whatever `env` line is in its
        // stderr, no program is missing.
        assert_eq!(
            of(&Ok(ran(Some(126), ENV_NODE))),
            said(NoAnswerKind::CouldNotStart, None)
        );
    }

    #[test]
    fn test_a_version_read_is_an_answer_whatever_the_command_did() {
        // Today's callers read a version off an exit 0 alone; one read
        // off anything else is an answer all the same.
        let failed = Ok(ran(Some(127), ENV_NODE));
        assert_eq!(unless_answered(&Some("11.6.2".to_string()), &failed), None);
        assert_eq!(
            unless_answered(&None::<String>, &failed),
            said(NoAnswerKind::CouldNotStart, Some("node"))
        );
    }
}
