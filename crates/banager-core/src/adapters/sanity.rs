//! What a parser keeps of a tool's answer, whatever the tool printed.
//!
//! A newer tool version, a truncated pipe or a tool printing colour codes
//! into a pipe can hand a parser a name that is empty or a version with a
//! newline or an escape character in it. Shown on a row, such a value is
//! garbage; used as a key, it names no package a command can act on. So
//! every parser passes what it read through here before returning it:
//! an entry without a usable name is dropped, as a line a text parser
//! cannot read is skipped; an installed version that is not text is
//! unknown (`""`, what a formula with no installed keg already has); and
//! an update whose versions are not text is dropped, since there is no
//! saying what it updates to. On every recorded fixture this changes
//! nothing (`adapters/robustness.rs` holds the inputs where it does).

use crate::model::{InstalledArtifact, SearchHit, UpdateCandidate};

/// A character a row must not show: a control character (Unicode Cc --
/// newline, `\r`, escape, NUL), or one that is invisible or reorders
/// the text around it: the format characters (Cf -- bidi overrides and
/// isolates such as U+202E, zero-width spaces and joiners, the BOM, the
/// soft hyphen, tag characters) and the line and paragraph separators
/// (U+2028, U+2029). With one of these a name can look like another, or
/// like nothing. The Cf list is Unicode 16's, written out because `std`
/// has no general-category lookup.
pub(crate) fn is_unshowable(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}'
                | '\u{600}'..='\u{605}'
                | '\u{61c}'
                | '\u{6dd}'
                | '\u{70f}'
                | '\u{890}'..='\u{891}'
                | '\u{8e2}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{206f}'
                | '\u{feff}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{110bd}'
                | '\u{110cd}'
                | '\u{13430}'..='\u{1343f}'
                | '\u{1bca0}'..='\u{1bca3}'
                | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0001}'
                | '\u{e0020}'..='\u{e007f}'
        )
}

/// A name a row can show and a command can be given: something besides
/// white space, and no character `is_unshowable` -- no newline, no `\r`
/// a CRLF left, no escape sequence, no NUL, no bidi override or
/// zero-width character.
pub(crate) fn is_name(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(is_unshowable)
}

/// A version as text: no character `is_unshowable`. Empty is allowed: it
/// is an unknown version.
pub(crate) fn is_version(s: &str) -> bool {
    !s.chars().any(is_unshowable)
}

/// A version read off a `--version` line: a non-empty token with no
/// control character, else `None` -- the tool did not say a version.
pub(crate) fn version_token(token: Option<String>) -> Option<String> {
    token.filter(|t| is_name(t))
}

/// The installed artifacts worth listing: each with a usable name; a
/// display name that is not one falls back to the name, a version that
/// is not text becomes unknown, and a command with no usable name is
/// left out.
pub(crate) fn artifacts(mut list: Vec<InstalledArtifact>) -> Vec<InstalledArtifact> {
    list.retain(|a| is_name(&a.key.name));
    for a in &mut list {
        if !is_name(&a.display_name) {
            a.display_name = a.key.name.clone();
        }
        if !is_version(&a.version) {
            a.version.clear();
        }
        a.facts
            .command_inputs
            .provided
            .retain(|command| is_name(&command.name));
    }
    list
}

/// The updates worth offering: a usable name, a current version that is
/// text, and a target that is a usable version.
pub(crate) fn candidates(mut list: Vec<UpdateCandidate>) -> Vec<UpdateCandidate> {
    list.retain(|c| is_name(&c.key.name) && is_version(&c.current) && is_name(&c.target));
    list
}

/// The search results worth showing: each with a usable name.
pub(crate) fn hits(mut list: Vec<SearchHit>) -> Vec<SearchHit> {
    list.retain(|h| is_name(&h.name));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_name_is_visible_text_on_one_line() {
        for ok in [
            "jq",
            "gautham-v/tap/claudebar",
            "qwen3.8:27b-mlx",
            "black@3.12",
            "名字",
        ] {
            assert!(is_name(ok), "{ok:?}");
        }
        for bad in [
            "",
            "  ",
            "\t",
            "a\nb",
            "jq\r",
            "\u{1b}[31mjq",
            "a\0b",
            "\u{7f}",
        ] {
            assert!(!is_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn regression_a_name_or_version_holds_no_invisible_or_reordering_character() {
        for bad in [
            "jq\u{202e}gnp.exe",
            "\u{2066}jq\u{2069}",
            "j\u{200b}q",
            "\u{200d}",
            "\u{feff}jq",
            "jq\u{2028}",
            "jq\u{2029}x",
            "j\u{ad}q",
            "jq\u{e0041}",
        ] {
            assert!(!is_name(bad), "{bad:?}");
            assert!(!is_version(bad), "{bad:?}");
        }
        // Not format characters: letters with marks, CJK, a plain space.
        for ok in ["café", "名字", "naïve", "a b"] {
            assert!(is_name(ok), "{ok:?}");
        }
    }

    #[test]
    fn test_a_version_may_be_unknown_but_never_holds_a_control_character() {
        assert!(is_version(""));
        assert!(is_version("1.2.3-beta+4"));
        assert!(!is_version("1.2\n3"));
        assert!(!is_version("1.2.3\r"));
        assert_eq!(version_token(Some("1.2".into())), Some("1.2".into()));
        assert_eq!(version_token(Some("1.2\0".into())), None);
        assert_eq!(version_token(Some(String::new())), None);
        assert_eq!(version_token(None), None);
    }
}
