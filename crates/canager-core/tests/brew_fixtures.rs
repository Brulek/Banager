use canager_core::adapters::brew::parse::{
    parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version,
};

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

#[test]
fn test_parse_uses_snapshot() {
    let text = read_fixture("uses-jq.txt");
    let result = parse_uses(&text);
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_version_reads_the_recorded_version() {
    let text = read_fixture("version.txt");
    assert_eq!(parse_version(&text), Some("7.0.3".to_string()));
}
