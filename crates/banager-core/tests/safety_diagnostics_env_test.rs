//! Promise 5 of `docs/what-we-run.md`: what Rust hands the window for
//! 「拷贝诊断信息」 (`diagnostics::current`) holds no environment
//! variable's value but `PATH`'s folders, and no absolute path into the
//! home folder. A test binary of its own, with one test, because it sets
//! this process's environment: every variable a secret could be in is
//! given a value of its own, the home folder a name of its own, and none
//! of them may show up in the facts.

use banager_core::diagnostics::current;
use banager_core::model::ManagerInstance;
use banager_core::testing::manager_instance;
use std::path::PathBuf;

/// Variables that can hold a secret, or say who and where someone is.
const VARIABLES: [&str; 22] = [
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "NO_PROXY",
    "HOMEBREW_GITHUB_API_TOKEN",
    "HOMEBREW_BOTTLE_DOMAIN",
    "GITHUB_TOKEN",
    "NPM_TOKEN",
    "NPM_CONFIG_REGISTRY",
    "PIP_INDEX_URL",
    "UV_INDEX_URL",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "AWS_SECRET_ACCESS_KEY",
    "OLLAMA_HOST",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "ZDOTDIR",
    "USER",
    "TMPDIR",
];

#[test]
fn test_the_diagnostic_facts_hold_no_variables_value_and_no_path_into_the_home_folder() {
    let home = "/Users/x3-diagnostics-account";
    let mut values = Vec::new();
    for (n, name) in VARIABLES.iter().enumerate() {
        let value = format!("x3-secret-{n}-value");
        std::env::set_var(name, &value);
        values.push(value);
    }
    std::env::set_var("HOME", home);
    // PATH's folders are the one value it shows: under the home, as `~`.
    std::env::set_var(
        "PATH",
        format!("{home}/.local/bin:/opt/homebrew/bin:{home}:/System/Volumes/Data{home}/bin"),
    );
    let instances: Vec<ManagerInstance> = ["claude", "agy"]
        .iter()
        .map(|tool| ManagerInstance {
            exe_path: PathBuf::from(format!("{home}/.local/bin/{tool}")),
            prefix: PathBuf::from(format!("{home}/.local")),
            ..manager_instance("standalone", &format!("standalone-{tool}"))
        })
        .collect();

    let facts = current(true, &instances);
    let json = serde_json::to_string(&facts).unwrap();
    for (name, value) in VARIABLES.iter().zip(&values) {
        assert!(
            !json.contains(value.as_str()),
            "{name}'s value is in {json}"
        );
    }
    assert!(
        !json.contains("x3-diagnostics-account"),
        "the home folder is named: {json}"
    );
    assert_eq!(
        facts.path_dirs,
        ["~/.local/bin", "/opt/homebrew/bin", "~", "~/bin"]
    );
    assert!(facts
        .sources
        .iter()
        .all(|source| source.exe_path.starts_with("~/.local/bin/")));
}
