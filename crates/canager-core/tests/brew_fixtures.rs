use canager_core::adapters::brew::parse::{
    parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version,
};
use canager_core::model::{ArtifactKind, UpdateBlocked};

// Substitute this if `brew --version` on your machine differs from the one
// recorded in Task 8.
const FIXTURE_DIR: &str = "../../adapters/fixtures/brew/7.0.3";

fn read_fixture(name: &str) -> String {
    let path = format!("{}/{}", FIXTURE_DIR, name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path, e))
}

#[test]
fn test_parse_info_installed_snapshot() {
    let json = read_fixture("info-installed.json");
    let result = parse_info_installed(&json, "brew:/opt/homebrew").expect("parse");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_outdated_snapshot() {
    let json = read_fixture("outdated.json");
    let result = parse_outdated(&json, "brew:/opt/homebrew").expect("parse");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_search_snapshot() {
    let text = read_fixture("search-jq.txt");
    let result = parse_search(&text, "brew");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_search_desc_snapshot() {
    let text = read_fixture("search-desc-jq.txt");
    let result = parse_search(&text, "brew");
    insta::assert_json_snapshot!(result);
}

/// `brew uses --installed jq` on both recording machines printed nothing:
/// jq is a leaf there, with no installed formula depending on it. That is a
/// real recording of a real case -- the one where `plan` attaches no
/// dependency warning to an uninstall -- so the empty file stays, named for
/// what produced it. The case where the warning *is* attached needs a
/// formula that actually has dependents, which is `uses-pcre2.txt` below.
#[test]
fn test_parse_uses_with_no_dependents_snapshot() {
    let text = read_fixture("uses-jq.txt");
    let result = parse_uses(&text);
    insta::assert_json_snapshot!(result);
}

/// The dependency-warning path: real `brew uses --installed pcre2` output,
/// four installed formulae deep. Until this fixture existed, the only
/// recorded `brew uses` output in the repo was the empty one above, so the
/// half of `plan` that turns dependents into a `Warning` had no recorded
/// output behind it at all.
#[test]
fn test_parse_uses_with_dependents_snapshot() {
    let text = read_fixture("uses-pcre2.txt");
    let result = parse_uses(&text);
    assert!(
        !result.is_empty(),
        "this fixture exists to carry a non-empty `brew uses --installed`; \
         an empty recording here would silently re-open the gap it closed"
    );
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_version_reads_the_recorded_version() {
    let text = read_fixture("version.txt");
    assert_eq!(parse_version(&text), Some("7.0.3".to_string()));
}

// Recorded later, against Homebrew 7.0.6. See that directory's README:
// `outdated.json` there is verbatim, `outdated-pinned.json` is the same
// recording with two entries' pin fields edited, because nothing on the
// recording Mac was pinned and pinning one would have changed its Homebrew.
const FIXTURE_DIR_7_0_6: &str = "../../adapters/fixtures/brew/7.0.6";

fn read_fixture_7_0_6(name: &str) -> String {
    let path = format!("{}/{}", FIXTURE_DIR_7_0_6, name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path, e))
}

/// The README's claim, checked: put the four edited values back and the
/// edited file is the recording, value for value. If someone re-records one
/// file and not the other, or edits more than the pin fields, this fails
/// instead of the fixture quietly becoming hand-written.
#[test]
fn outdated_pinned_fixture_differs_from_the_recording_only_in_the_pin_fields() {
    let recorded: serde_json::Value =
        serde_json::from_str(&read_fixture_7_0_6("outdated.json")).expect("recorded json");
    let mut edited: serde_json::Value =
        serde_json::from_str(&read_fixture_7_0_6("outdated-pinned.json")).expect("edited json");

    let mut restored = 0;
    for partition in ["formulae", "casks"] {
        for entry in edited[partition].as_array_mut().expect("partition") {
            let name = entry["name"].as_str().expect("name").to_string();
            if (partition, name.as_str()) == ("formulae", "glib")
                || (partition, name.as_str()) == ("casks", "onyx")
            {
                assert_eq!(entry["pinned"], serde_json::json!(true), "{name}");
                assert!(entry["pinned_version"].is_string(), "{name}");
                entry["pinned"] = serde_json::json!(false);
                entry["pinned_version"] = serde_json::Value::Null;
                restored += 1;
            }
        }
    }
    assert_eq!(restored, 2, "both edited entries must still be in the file");
    assert_eq!(edited, recorded);
}

/// Every entry in the verbatim recording is unpinned, so nothing is blocked.
#[test]
fn test_parse_outdated_7_0_6_recording_blocks_nothing() {
    let json = read_fixture_7_0_6("outdated.json");
    let result = parse_outdated(&json, "brew:/opt/homebrew").expect("parse");
    assert_eq!(result.len(), 12);
    assert!(result.iter().all(|c| c.blocked.is_none()), "{result:#?}");
}

/// A pinned formula and a pinned cask come back marked, and only they do:
/// Homebrew would refuse `brew upgrade` of exactly these two.
#[test]
fn test_parse_outdated_marks_exactly_the_pinned_entries() {
    let json = read_fixture_7_0_6("outdated-pinned.json");
    let result = parse_outdated(&json, "brew:/opt/homebrew").expect("parse");
    let blocked: Vec<_> = result
        .iter()
        .filter(|c| c.blocked.is_some())
        .map(|c| (c.key.kind, c.key.name.as_str(), c.blocked))
        .collect();
    assert_eq!(
        blocked,
        vec![
            (ArtifactKind::Formula, "glib", Some(UpdateBlocked::Pinned)),
            (ArtifactKind::Cask, "onyx", Some(UpdateBlocked::Pinned)),
        ]
    );
    assert_eq!(result.len(), 12, "a pinned entry is still listed");
}
