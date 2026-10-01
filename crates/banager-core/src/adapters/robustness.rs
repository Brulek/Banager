//! Robustness tests for every parser of a tool's output: no input a tool
//! might print -- a newer version's shape, a truncated pipe, a localised
//! error, garbage -- may panic, hang or yield an absurd value.
//!
//! Each parser is fed its recorded fixtures (read-only, from
//! `adapters/fixtures/`) truncated at many offsets, with random byte
//! flips, with lines shuffled and duplicated, with invalid UTF-8 (decoded
//! lossily, as the runner decodes a tool's output), with CRLF line ends,
//! with numbers out of range, unexpected types and unknown keys in its
//! JSON, plus inputs of its own: empty, a 10 MB line, 100,000 nested
//! brackets, localised error messages. Every call runs under
//! `catch_unwind` and against a time bound (`CALL_LIMIT` for one that
//! returns; a watchdog ends the test process on one still running after
//! `HANG_LIMIT`, so an endless loop fails the suite rather than hanging
//! it), and what comes back must be a
//! typed error or a sane value: no artifact, update or search hit with an
//! empty name, no name or version with a control character (a newline
//! among them).
//!
//! No crate is added for this: the PRNG is a seeded splitmix64, so every
//! run tries the same inputs and a failure names one it can repeat.

use crate::adapters::AdapterError;
use crate::model::{InstalledArtifact, SearchHit, UpdateCandidate};
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// Seeded splitmix64.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; `n` must not be 0.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// How long one call may take. Generous for a debug build parsing 10 MB;
/// a quadratic parser over a large input blows through it by orders of
/// magnitude.
const CALL_LIMIT: Duration = Duration::from_secs(3);

/// How long one call may take before the suite stops waiting for it. A
/// call over `CALL_LIMIT` that returns is reported with the rest; one
/// that never returns would hang `cargo test`, so a watchdog ends the
/// test process instead, naming the parser and the input it was on.
const HANG_LIMIT: Duration = Duration::from_secs(60);

const TEN_MB: usize = 10 * 1024 * 1024;
const ONE_MB: usize = 1024 * 1024;

/// What tools print instead of what was asked for, in several languages.
const LOCALIZED_ERRORS: [&str; 6] = [
    "Error: Permission denied @ rb_sysopen - /opt/homebrew/var/homebrew/locks/update\n",
    "错误：无法连接到服务器。\n请稍后再试。\n",
    "Fehler: Zeitüberschreitung beim Verbinden mit registry.npmjs.org\n",
    "エラー: ネットワークに接続できません\n",
    "ERROR: Could not find a version that satisfies the requirement\n",
    "npm ERR! code ECONNRESET\nnpm ERR! errno ECONNRESET\n",
];

/// One recorded fixture, read-only.
fn fixture(path: &str) -> String {
    let full = format!("../../adapters/fixtures/{path}");
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("read {full}: {e}"))
}

/// `bytes` as the runner hands a tool's output to a parser: invalid UTF-8
/// replaced, never refused.
fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The inputs every parser gets whatever its fixtures: empty, blank,
/// JSON's small values, a 10 MB line after a fixture (and 1 MB ones), deep
/// nesting, invalid UTF-8 and localised errors.
fn generic_inputs(sample: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![
        ("empty".into(), String::new()),
        ("newline".into(), "\n".into()),
        ("blank".into(), "  \t \r\n \n".into()),
        ("null".into(), "null".into()),
        ("empty array".into(), "[]".into()),
        ("empty object".into(), "{}".into()),
        ("true".into(), "true".into()),
        ("zero".into(), "0".into()),
        ("NUL bytes".into(), "\0\0\0\n\0".into()),
        ("1 MB line".into(), "a".repeat(ONE_MB)),
        ("1 MB of spaces".into(), " ".repeat(ONE_MB)),
        (
            "fixture then a 10 MB line".into(),
            format!("{sample}{}\n", "x".repeat(TEN_MB)),
        ),
        (
            "1 MB name line".into(),
            format!("{} v1.0 (/x)\n", "n".repeat(ONE_MB)),
        ),
        ("100k [".into(), "[".repeat(100_000)),
        ("100k {\"a\":".into(), "{\"a\":".repeat(100_000)),
        (
            "100k nested TOML arrays".into(),
            format!("version = {}", "[".repeat(100_000)),
        ),
        (
            "100k nested TOML tables".into(),
            format!("version = {}", "{a = ".repeat(100_000)),
        ),
        (
            "invalid UTF-8".into(),
            lossy(b"\xff\xfe\xfd garbage \xc3\x28\n"),
        ),
        ("a huge number".into(), "1".repeat(100_000)),
    ];
    for (i, error) in LOCALIZED_ERRORS.iter().enumerate() {
        out.push((format!("localized error {i}"), error.to_string()));
    }
    out
}

/// Every mutation of one fixture: `budget` scales the counts down for a
/// large fixture so the whole suite stays fast in a debug build.
fn mutations(name: &str, base: &str, rng: &mut Rng, budget: usize) -> Vec<(String, String)> {
    let bytes = base.as_bytes();
    let mut out: Vec<(String, String)> = vec![(format!("{name} as recorded"), base.to_string())];

    // Truncated at evenly spaced and random offsets, mid-character too.
    if !bytes.is_empty() {
        for i in 0..budget {
            let at = bytes.len() * i / budget;
            out.push((format!("{name} cut at {at}"), lossy(&bytes[..at])));
        }
        for _ in 0..budget / 2 {
            let at = rng.below(bytes.len());
            out.push((format!("{name} cut at {at}"), lossy(&bytes[..at])));
        }
    }

    // Random byte flips.
    if !bytes.is_empty() {
        for round in 0..budget {
            let mut flipped = bytes.to_vec();
            for _ in 0..1 + rng.below(8) {
                let at = rng.below(flipped.len());
                flipped[at] ^= 1 << rng.below(8);
            }
            out.push((format!("{name} flipped #{round}"), lossy(&flipped)));
        }
        for round in 0..budget / 2 {
            let mut overwritten = bytes.to_vec();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(overwritten.len());
                const PUNCTUATION: &[u8] = b"\n\r\0:->[]{}\"' v()/";
                overwritten[at] = PUNCTUATION[rng.below(PUNCTUATION.len())];
            }
            out.push((format!("{name} overwritten #{round}"), lossy(&overwritten)));
        }
    }

    // Lines shuffled, duplicated, dropped; CRLF; invalid UTF-8 spliced in.
    let lines: Vec<&str> = base.split('\n').collect();
    for round in 0..budget / 4 + 1 {
        let mut shuffled = lines.clone();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, rng.below(i + 1));
        }
        out.push((format!("{name} shuffled #{round}"), shuffled.join("\n")));
        let mut doubled = Vec::new();
        for line in &lines {
            doubled.push(*line);
            if rng.below(3) == 0 {
                doubled.push(*line);
            }
        }
        out.push((format!("{name} duplicated #{round}"), doubled.join("\n")));
        let kept: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|_| rng.below(4) != 0)
            .collect();
        out.push((format!("{name} lines dropped #{round}"), kept.join("\n")));
    }
    out.push((format!("{name} CRLF"), base.replace('\n', "\r\n")));
    out.push((format!("{name} CR only"), base.replace('\n', "\r")));
    out.push((format!("{name} tabs"), base.replace(' ', "\t")));
    if !bytes.is_empty() {
        for round in 0..budget / 4 + 1 {
            let mut spliced = bytes.to_vec();
            let at = rng.below(spliced.len());
            spliced.splice(at..at, b"\xff\xc3\x28\xe2\x82".iter().copied());
            out.push((format!("{name} invalid UTF-8 #{round}"), lossy(&spliced)));
        }
    }

    // Numbers out of range, in place of every run of digits.
    for huge in ["99999999999999999999999", "-1", "1e999", "1.5", "0"] {
        out.push((
            format!("{name} numbers {huge}"),
            replace_digit_runs(base, huge),
        ));
    }

    // JSON: unexpected types for known keys, unknown keys everywhere.
    if let Ok(value) = serde_json::from_str::<Value>(base) {
        for round in 0..budget {
            let mut changed = value.clone();
            let nodes = count_nodes(&changed);
            let target = rng.below(nodes);
            let replacement = odd_value(rng);
            replace_node(&mut changed, &mut { target }, &replacement);
            out.push((format!("{name} retyped #{round}"), changed.to_string()));
        }
        let mut extra = value.clone();
        add_unknown_keys(&mut extra);
        out.push((format!("{name} unknown keys"), extra.to_string()));
        for (label, replacement) in [
            ("every string empty", Value::String(String::new())),
            ("every string a newline", Value::String("a\nb".into())),
            ("every string a control", Value::String("\u{1b}[31m".into())),
            ("every string blank", Value::String("  ".into())),
        ] {
            let mut all = value.clone();
            replace_strings(&mut all, &replacement);
            out.push((format!("{name} {label}"), all.to_string()));
            let mut keys = value.clone();
            replace_keys(&mut keys, replacement.as_str().unwrap_or_default());
            out.push((format!("{name} {label} (keys)"), keys.to_string()));
        }
    }
    out
}

fn replace_digit_runs(text: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_run {
                out.push_str(with);
            }
            in_run = true;
        } else {
            in_run = false;
            out.push(c);
        }
    }
    out
}

fn count_nodes(value: &Value) -> usize {
    1 + match value {
        Value::Array(items) => items.iter().map(count_nodes).sum(),
        Value::Object(map) => map.values().map(count_nodes).sum(),
        _ => 0,
    }
}

/// Replaces the `target`th node, in pre-order, with `with`.
fn replace_node(value: &mut Value, target: &mut usize, with: &Value) -> bool {
    if *target == 0 {
        *value = with.clone();
        return true;
    }
    *target -= 1;
    match value {
        Value::Array(items) => items.iter_mut().any(|v| replace_node(v, target, with)),
        Value::Object(map) => map.values_mut().any(|v| replace_node(v, target, with)),
        _ => false,
    }
}

fn odd_value(rng: &mut Rng) -> Value {
    match rng.below(9) {
        0 => Value::Null,
        1 => Value::Bool(true),
        2 => serde_json::json!(-1),
        3 => serde_json::json!(1.5e300),
        4 => Value::String(String::new()),
        5 => Value::String("\n".into()),
        6 => Value::Array(vec![Value::Null, serde_json::json!({})]),
        7 => serde_json::json!({"unexpected": [1, "two", null]}),
        _ => serde_json::json!(u64::MAX),
    }
}

fn add_unknown_keys(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(add_unknown_keys),
        Value::Object(map) => {
            map.values_mut().for_each(add_unknown_keys);
            map.insert(
                "zz_unknown_key_from_a_newer_version".into(),
                serde_json::json!({"nested": [1, {"deeper": null}], "flag": true}),
            );
        }
        _ => {}
    }
}

fn replace_strings(value: &mut Value, with: &Value) {
    match value {
        Value::String(_) => *value = with.clone(),
        Value::Array(items) => items.iter_mut().for_each(|v| replace_strings(v, with)),
        Value::Object(map) => map.values_mut().for_each(|v| replace_strings(v, with)),
        _ => {}
    }
}

/// Every object key renamed to `with` plus its index, so a map keyed by
/// name (npm's, pipx's, cargo's) gets names of that shape.
fn replace_keys(value: &mut Value, with: &str) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|v| replace_keys(v, with)),
        Value::Object(map) => {
            let old = std::mem::take(map);
            for (i, (_, mut v)) in old.into_iter().enumerate() {
                replace_keys(&mut v, with);
                let key = if i == 0 {
                    with.to_string()
                } else {
                    format!("{with}{i}")
                };
                map.insert(key, v);
            }
        }
        _ => {}
    }
}

/// Every input one parser is fed: its fixtures' mutations, then the
/// generic inputs.
fn inputs_for(seed: u64, fixtures: &[&str]) -> Vec<(String, String)> {
    let bases: Vec<(String, String)> = fixtures
        .iter()
        .map(|path| (path.to_string(), fixture(path)))
        .collect();
    inputs_from(seed, &bases)
}

/// `inputs_for` over bodies given inline, as `(name, text)`: for an
/// answer no fixture records, such as a registry's JSON.
fn inputs_from(seed: u64, bases: &[(String, String)]) -> Vec<(String, String)> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    let mut sample = String::new();
    for (name, base) in bases {
        let budget = if base.len() > 100_000 { 6 } else { 40 };
        out.extend(mutations(name, base, &mut rng, budget));
        if sample.is_empty() {
            sample = base.chars().take(4096).collect();
        }
    }
    out.extend(generic_inputs(&sample));
    out
}

/// Ends the test process when one call runs past `HANG_LIMIT`: the
/// call is on this thread and cannot be stopped from another, and the
/// process ending fails `cargo test` where waiting would hang it.
struct Watchdog {
    state: std::sync::Arc<std::sync::Mutex<Option<(String, Instant)>>>,
}

impl Watchdog {
    fn start(parser: &'static str) -> Watchdog {
        let state: std::sync::Arc<std::sync::Mutex<Option<(String, Instant)>>> = Default::default();
        let watched = std::sync::Arc::downgrade(&state);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(200));
            let Some(state) = watched.upgrade() else {
                return;
            };
            let current = state.lock().map(|s| s.clone()).unwrap_or_default();
            if let Some((label, since)) = current {
                if since.elapsed() > HANG_LIMIT {
                    eprintln!("{parser} <- {label}\n    still running after {HANG_LIMIT:?}");
                    std::process::exit(101);
                }
            }
        });
        Watchdog { state }
    }

    /// The call about to start.
    fn on(&self, label: &str) {
        if let Ok(mut state) = self.state.lock() {
            *state = Some((label.to_string(), Instant::now()));
        }
    }
}

/// What a check found wrong with one input.
#[derive(Debug)]
struct Problem {
    parser: &'static str,
    input: String,
    what: String,
}

/// Runs `parse` over every input, collecting each panic, overlong call and
/// absurd result. `sane` says what is absurd about a result.
fn run<T>(
    parser: &'static str,
    inputs: &[(String, String)],
    parse: impl Fn(&str) -> T,
    sane: impl Fn(&T) -> Result<(), String>,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    let watchdog = Watchdog::start(parser);
    for (label, input) in inputs {
        watchdog.on(label);
        let started = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| parse(input)));
        let took = started.elapsed();
        let what = match result {
            Err(panic) => Some(format!(
                "panicked: {}",
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default()
            )),
            Ok(_) if took > CALL_LIMIT => Some(format!("took {took:?}")),
            Ok(value) => sane(&value).err(),
        };
        if let Some(what) = what {
            problems.push(Problem {
                parser,
                input: format!("{label}: {:?}", input.chars().take(160).collect::<String>()),
                what,
            });
        }
    }
    problems
}

fn assert_none(problems: Vec<Problem>) {
    if !problems.is_empty() {
        let shown: Vec<String> = problems
            .iter()
            .take(40)
            .map(|p| format!("{} <- {}\n    {}", p.parser, p.input, p.what))
            .collect();
        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        for p in &problems {
            let kind: String = p.what.split(':').next().unwrap_or_default().to_string();
            *kinds.entry(format!("{} / {kind}", p.parser)).or_default() += 1;
        }
        let kinds: Vec<String> = kinds.iter().map(|(k, n)| format!("{n:>5}  {k}")).collect();
        panic!(
            "{} problem(s):\n{}\nfirst {}:\n{}",
            problems.len(),
            kinds.join("\n"),
            shown.len(),
            shown.join("\n")
        );
    }
}

// --- What a sane result is ------------------------------------------------

/// The start of `s`, for a message: never a 10 MB one.
fn short(s: &str) -> String {
    s.chars().take(80).collect()
}

fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

fn name_ok(what: &str, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{what} is empty: {:?}", short(name)));
    }
    if has_control(name) {
        return Err(format!("{what} has a control character: {:?}", short(name)));
    }
    Ok(())
}

fn version_ok(what: &str, version: &str) -> Result<(), String> {
    if has_control(version) {
        return Err(format!(
            "{what} has a control character: {:?}",
            short(version)
        ));
    }
    Ok(())
}

fn artifacts_ok(artifacts: &[InstalledArtifact]) -> Result<(), String> {
    for a in artifacts {
        name_ok("name", &a.key.name)?;
        name_ok("display name", &a.display_name)?;
        version_ok("version", &a.version)?;
        for command in &a.facts.command_inputs.provided {
            name_ok("command", &command.name)?;
        }
    }
    Ok(())
}

fn candidates_ok(candidates: &[UpdateCandidate]) -> Result<(), String> {
    for c in candidates {
        name_ok("name", &c.key.name)?;
        version_ok("current", &c.current)?;
        name_ok("target", &c.target)?;
    }
    Ok(())
}

fn hits_ok(hits: &[SearchHit]) -> Result<(), String> {
    for h in hits {
        name_ok("hit", &h.name)?;
    }
    Ok(())
}

fn version_token_ok(version: &Option<String>) -> Result<(), String> {
    match version {
        Some(v) if v.trim().is_empty() || v.chars().any(|c| c.is_control()) => {
            Err(format!("bad version: {:?}", short(v)))
        }
        _ => Ok(()),
    }
}

fn result_ok<T>(
    result: &Result<T, AdapterError>,
    ok: impl Fn(&T) -> Result<(), String>,
) -> Result<(), String> {
    match result {
        Ok(value) => ok(value),
        Err(_) => Ok(()),
    }
}

/// A reason that ends up on a row: short, whatever the body was.
fn reason_ok(reason: &str) -> Result<(), String> {
    if reason.chars().count() > 400 {
        return Err(format!("a reason of {} characters", reason.chars().count()));
    }
    Ok(())
}

fn string_result_ok(result: &Result<String, String>) -> Result<(), String> {
    match result {
        Ok(v) => name_ok("version", v),
        Err(reason) => reason_ok(reason),
    }
}

// --- One test per tool ----------------------------------------------------

const INSTANCE: &str = "test:/instance";

#[test]
fn brew_parsers_survive_any_input() {
    use crate::adapters::brew::parse;
    let info = inputs_for(1, &["brew/7.0.3/info-installed.json"]);
    let mut problems = run(
        "brew parse_info_installed",
        &info,
        |s| parse::parse_info_installed(s, INSTANCE),
        |r| result_ok(r, |a| artifacts_ok(a)),
    );
    let outdated = inputs_for(
        2,
        &[
            "brew/7.0.3/outdated.json",
            "brew/7.0.6/outdated.json",
            "brew/7.0.6/outdated-pinned.json",
        ],
    );
    problems.extend(run(
        "brew parse_outdated",
        &outdated,
        |s| parse::parse_outdated(s, INSTANCE),
        |r| result_ok(r, |c| candidates_ok(c)),
    ));
    let search = inputs_for(
        3,
        &["brew/7.0.3/search-jq.txt", "brew/7.0.3/search-desc-jq.txt"],
    );
    problems.extend(run(
        "brew parse_search",
        &search,
        |s| parse::parse_search(s, "brew"),
        |h| hits_ok(h),
    ));
    let uses = inputs_for(4, &["brew/7.0.3/uses-pcre2.txt", "brew/7.0.3/uses-jq.txt"]);
    problems.extend(run("brew parse_uses", &uses, parse::parse_uses, |names| {
        names.iter().try_for_each(|n| name_ok("formula", n))
    }));
    let version = inputs_for(5, &["brew/7.0.3/version.txt"]);
    problems.extend(run(
        "brew parse_version",
        &version,
        parse::parse_version,
        version_token_ok,
    ));
    assert_none(problems);
}

#[test]
fn cask_receipts_survive_any_record() {
    use crate::adapters::brew::cask_receipt::{classify, Recorded};
    let names = [
        "claudebar",
        "libreoffice",
        "onyx",
        "uninstall-flight-block",
        "unknown-stanza",
        "wireshark-chmodbpf",
        "adobe-creative-cloud",
    ];
    let paths: Vec<String> = names
        .iter()
        .map(|n| format!("brew/7.0.6/receipts/{n}.json"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let inputs = inputs_for(6, &paths);
    let problems = run(
        "cask_receipt classify",
        &inputs,
        |s| {
            let value: Value = serde_json::from_str(s).ok()?;
            let recorded = Recorded {
                artifacts: value
                    .get("uninstall_artifacts")
                    .or_else(|| value.get("artifacts"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                flight_blocks: value
                    .get("uninstall_flight_blocks")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            Some(classify(
                &recorded,
                Some(std::path::Path::new("/Users/someone")),
            ))
        },
        |_| Ok(()),
    );
    assert_none(problems);
}

#[test]
fn brew_env_files_survive_any_bytes() {
    use crate::adapters::brew::brew_env::after_brew_env;
    let mut inputs = generic_inputs("HOMEBREW_NO_AUTOREMOVE=1\n");
    let mut rng = Rng::new(7);
    let base = "HOMEBREW_NO_AUTOREMOVE=1\nHOMEBREW_NO_INSTALL_CLEANUP+=x\nexport A=b\n";
    inputs.extend(mutations("brew.env", base, &mut rng, 40));
    inputs.push((
        "100k appends".into(),
        "HOMEBREW_NO_INSTALL_CLEANUP+=xx\n".repeat(100_000),
    ));
    let problems = run(
        "brew_env after_brew_env",
        &inputs,
        |s| {
            let bytes = s.as_bytes().to_vec();
            after_brew_env(
                &[],
                std::path::Path::new("/opt/homebrew"),
                &|name| (name == "HOME").then(|| "/Users/someone".into()),
                &|_| Some(bytes.clone()),
            )
        },
        |_| Ok(()),
    );
    assert_none(problems);
}

#[test]
fn npm_parsers_survive_any_input() {
    use crate::adapters::npm;
    let ls = inputs_for(10, &["npm/12.0.2/ls-global.json"]);
    let mut problems = run(
        "npm parse_ls_global",
        &ls,
        |s| npm::parse_ls_global(s, INSTANCE),
        |r| result_ok(r, |a| artifacts_ok(a)),
    );
    let outdated = inputs_for(11, &["npm/12.0.2/outdated-global.json"]);
    problems.extend(run(
        "npm parse_outdated_global",
        &outdated,
        |s| npm::parse_outdated_global(s, INSTANCE),
        |r| result_ok(r, |c| candidates_ok(c)),
    ));
    let search = inputs_for(12, &["npm/12.0.2/search-jq.json"]);
    problems.extend(run(
        "npm parse_search",
        &search,
        |s| npm::parse_search(s, "npm"),
        |r| result_ok(r, |h| hits_ok(h)),
    ));
    assert_none(problems);
}

#[test]
fn pip_parsers_survive_any_input() {
    use crate::adapters::pip;
    let list = inputs_for(
        20,
        &["pip/26.2.1/list.json", "pip/26.2.1/list-not-required.json"],
    );
    let mut problems = run("pip parse_pip_list", &list, pip::parse_pip_list, |r| {
        result_ok(r, |packages| {
            packages.iter().try_for_each(|p| {
                name_ok("name", &p.name)?;
                version_ok("version", &p.version)
            })
        })
    });
    let outdated = inputs_for(21, &["pip/26.2.1/list-outdated.json"]);
    problems.extend(run(
        "pip parse_pip_outdated",
        &outdated,
        |s| pip::parse_pip_outdated(s, INSTANCE),
        |r| result_ok(r, |c| candidates_ok(c)),
    ));
    let version = inputs_for(22, &["pip/26.2.1/version.txt"]);
    problems.extend(run(
        "second_token (pip, cargo, uv)",
        &version,
        crate::adapters::second_token,
        version_token_ok,
    ));
    assert_none(problems);
}

#[test]
fn pipx_parsers_survive_any_input() {
    use crate::adapters::pipx;
    let version = inputs_for(30, &["pipx/1.17.3/version.txt"]);
    let mut problems = run(
        "pipx parse_version",
        &version,
        pipx::parse_version,
        version_token_ok,
    );
    let list = inputs_for(31, &["pipx/1.17.3/list.json"]);
    problems.extend(run(
        "pipx parse_list",
        &list,
        |s| pipx::parse_list(s, INSTANCE),
        |r| result_ok(r, |a| artifacts_ok(a)),
    ));
    let outdated = inputs_for(
        32,
        &[
            "pipx/1.17.3/list-outdated.txt",
            "pipx/1.17.3/list-outdated-pinned.txt",
        ],
    );
    problems.extend(run(
        "pipx parse_outdated",
        &outdated,
        |s| pipx::parse_outdated(s, INSTANCE),
        |c| candidates_ok(c),
    ));
    // PyPI's answer about one package: no fixture records it, so the
    // shape `latest_pypi_version` reads, inline.
    let pypi = inputs_from(
        33,
        &[(
            "PyPI package body".into(),
            r#"{"info":{"name":"cowsay","version":"6.1","summary":"The famous cowsay for GNU/Linux is now available for python","requires_python":">=3.8","yanked":false},"last_serial":1,"releases":{"5.0":[],"6.1":[]},"urls":[],"vulnerabilities":[]}"#.into(),
        )],
    );
    problems.extend(run(
        "pipx parse_pypi_body",
        &pypi,
        pipx::parse_pypi_body,
        string_result_ok,
    ));
    assert_none(problems);
}

#[test]
fn cargo_parsers_survive_any_input() {
    use crate::adapters::cargo;
    let mut crates2 = inputs_for(40, &["cargo/1.98.1/crates2.json"]);
    let many: Vec<String> = (0..60_000)
        .map(|i| format!("\"c{i} 1.0.{i} (registry+https://x)\":{{\"bins\":[\"c{i}\"]}}"))
        .collect();
    crates2.push((
        "60k crates".into(),
        format!("{{\"installs\":{{{}}}}}", many.join(",")),
    ));
    let home = std::path::Path::new("/Users/someone/.cargo");
    let mut problems = run(
        "cargo parse_crates2",
        &crates2,
        |s| cargo::parse_crates2(s, INSTANCE, home),
        |r| {
            result_ok(r, |artifacts| {
                artifacts_ok(artifacts)?;
                artifacts.iter().try_for_each(|a| match &a.path {
                    Some(path) if !path.starts_with(home.join("bin")) => {
                        Err(format!("a program outside the cargo bin folder: {path:?}"))
                    }
                    _ => Ok(()),
                })
            })
        },
    );
    problems.extend(run(
        "cargo parse_crates2_bins",
        &crates2,
        cargo::parse_crates2_bins,
        |r| {
            result_ok(r, |crates| {
                crates
                    .iter()
                    .try_for_each(|(name, _)| name_ok("crate", name))
            })
        },
    ));
    // crates.io's answer about one crate: no fixture records it, so the
    // shape `latest_stable_version` reads, inline.
    let crates_io = inputs_from(
        42,
        &[(
            "crates.io crate body".into(),
            r#"{"crate":{"id":"hexyl","name":"hexyl","max_version":"0.18.0","max_stable_version":"0.18.0","newest_version":"0.18.0"},"versions":[{"num":"0.18.0","yanked":false}]}"#.into(),
        )],
    );
    problems.extend(run(
        "cargo parse_crates_io_body",
        &crates_io,
        cargo::parse_crates_io_body,
        string_result_ok,
    ));
    let version = inputs_for(41, &["cargo/1.98.1/version.txt", "uv/0.12.17/version.txt"]);
    problems.extend(run(
        "second_token (cargo, uv)",
        &version,
        crate::adapters::second_token,
        version_token_ok,
    ));
    assert_none(problems);
}

#[test]
fn uv_parsers_survive_any_input() {
    use crate::adapters::uv;
    let mut show_paths = inputs_for(50, &["uv/0.12.17/tool-list-show-paths.txt"]);
    let many: String = (0..100_000)
        .map(|i| format!("t{i} v1.{i} (/u/t{i})\n- b{i} (/u/bin/b{i})\n"))
        .collect();
    show_paths.push(("100k tools".into(), many));
    let mut problems = run(
        "uv parse_tool_list_show_paths",
        &show_paths,
        |s| uv::parse_tool_list_show_paths(s, INSTANCE),
        |a| artifacts_ok(a),
    );
    let outdated = inputs_for(51, &["uv/0.12.17/tool-list-outdated.txt"]);
    problems.extend(run(
        "uv parse_tool_list_outdated",
        &outdated,
        |s| uv::parse_tool_list_outdated(s, INSTANCE),
        |c| candidates_ok(c),
    ));
    assert_none(problems);
}

#[test]
fn ollama_parsers_survive_any_input() {
    use crate::adapters::ollama::parse;
    let version = inputs_for(60, &["ollama/0.34.1/version.txt"]);
    let mut problems = run(
        "ollama parse_version",
        &version,
        parse::parse_version,
        version_token_ok,
    );
    let tags = inputs_for(61, &["ollama/0.34.1/api-tags.json"]);
    problems.extend(run(
        "ollama parse_tags",
        &tags,
        |s| parse::parse_tags(s, INSTANCE),
        |r| result_ok(r, |a| artifacts_ok(a)),
    ));
    let manifests = inputs_for(62, &["ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json"]);
    problems.extend(run(
        "ollama layer_digests",
        &manifests,
        parse::layer_digests,
        |_| Ok(()),
    ));
    problems.extend(run(
        "ollama config_digest",
        &manifests,
        parse::config_digest,
        |_| Ok(()),
    ));
    let mut names: Vec<(String, String)> = tags.clone();
    names.push(("colons and slashes".into(), ":/:/::".into()));
    problems.extend(run(
        "ollama split_model_reference",
        &names,
        parse::split_model_reference,
        |_| Ok(()),
    ));
    assert_none(problems);
}

#[test]
fn standalone_parsers_survive_any_input() {
    use crate::adapters::standalone::latest;
    use crate::adapters::standalone::recipe::VersionParse;
    let versions = inputs_for(
        70,
        &[
            "standalone-claude/2.1.282/version.txt",
            "standalone-agy/1.2.11/version.txt",
            "standalone-grok/1.0.41/version.txt",
            "standalone-rustup/1.29.1/version.txt",
        ],
    );
    let mut problems = run(
        "standalone parse_version (first token)",
        &versions,
        |s| latest::parse_version(s, VersionParse::FirstToken),
        version_token_ok,
    );
    problems.extend(run(
        "standalone parse_version (second token)",
        &versions,
        |s| latest::parse_version(s, VersionParse::SecondToken),
        version_token_ok,
    ));
    problems.extend(run(
        "standalone compare_dotted",
        &versions,
        |s| latest::compare_dotted(s, "1.2.3"),
        |_| Ok(()),
    ));
    let channel = inputs_for(
        71,
        &[
            "standalone-claude/2.1.282/stable.txt",
            "standalone-claude/2.1.282/latest.txt",
        ],
    );
    problems.extend(run(
        "standalone parse_channel_body",
        &channel,
        latest::parse_channel_body,
        string_result_ok,
    ));
    let release = inputs_for(72, &["standalone-rustup/1.29.1/release-stable.toml"]);
    problems.extend(run(
        "standalone parse_release_stable_toml",
        &release,
        latest::parse_release_stable_toml,
        string_result_ok,
    ));
    let manifest = inputs_for(73, &["standalone-agy/1.2.11/manifest-darwin_arm64.json"]);
    problems.extend(run(
        "standalone parse_json_field",
        &manifest,
        |s| latest::parse_json_field(s, "version"),
        string_result_ok,
    ));
    let check = inputs_for(74, &["standalone-grok/1.0.41/update-check.json"]);
    problems.extend(run(
        "standalone parse_update_check",
        &check,
        |s| latest::parse_update_check(s, "latestVersion", "updateAvailable", Some("error")),
        |r| match r {
            Ok(check) => name_ok("latest", &check.latest),
            Err(reason) => reason_ok(reason),
        },
    ));
    let settings = inputs_for(75, &["standalone-agy/1.2.11/update_status.json"]);
    problems.extend(run(
        "standalone claude_channel_from_json",
        &settings,
        latest::claude_channel_from_json,
        |_| Ok(()),
    ));
    let startup = inputs_for(76, &["standalone-rustup/1.29.1/toolchains.txt"]);
    let patterns = crate::adapters::standalone::rustup::leftover_patterns(
        std::path::Path::new("/Users/someone"),
        std::path::Path::new("/Users/someone/.cargo"),
    );
    problems.extend(run(
        "rustup classify_leftover",
        &startup,
        |s| crate::adapters::standalone::rustup::classify_leftover(s, &patterns),
        |_| Ok(()),
    ));
    problems.extend(run(
        "rustup remove_first_exact_line",
        &startup,
        |s| {
            let mut contents = s.to_string();
            crate::adapters::standalone::rustup::remove_first_exact_line(
                &mut contents,
                ". \"$HOME/.cargo/env\"",
            )
        },
        |_| Ok(()),
    ));
    assert_none(problems);
}

#[test]
fn adapter_metadata_survives_any_toml() {
    let meta = inputs_for(80, &[]);
    let mut all = meta;
    let mut rng = Rng::new(81);
    let base = std::fs::read_to_string("../../adapters/meta/brew.toml").expect("read brew.toml");
    all.extend(mutations("brew.toml", &base, &mut rng, 40));
    let problems = run(
        "AdapterMeta::from_toml",
        &all,
        crate::adapters::AdapterMeta::from_toml,
        |_| Ok(()),
    );
    assert_none(problems);
}

/// What the parsers hand on is looked up by name: the families table
/// (`families::family_for`) and the Other Programs scan's name rules
/// (`scan::Glob`, `scan::display_path`). Neither parses a tool's output
/// itself -- the table is compiled in, the scan reads folders -- but both
/// take whatever name a parser let through.
#[test]
fn name_lookups_survive_any_name() {
    use crate::model::{ArtifactKey, ArtifactKind, RemovedWhat};
    let names = inputs_for(
        90,
        &["npm/12.0.2/search-jq.json", "brew/7.0.3/search-jq.txt"],
    );
    let glob = crate::scan::Glob {
        dir: "~/.local/bin",
        prefix: "agy.",
        suffix: ".old",
        what: RemovedWhat::Backups,
    };
    let home = std::path::Path::new("/Users/someone");
    let problems = run(
        "families and scan name rules",
        &names,
        |name| {
            let mut found = 0;
            for (adapter, kind) in [
                ("npm", ArtifactKind::Package),
                ("brew", ArtifactKind::Formula),
                ("brew", ArtifactKind::Cask),
                ("pipx", ArtifactKind::Tool),
                ("pip", ArtifactKind::Package),
                ("standalone-claude", ArtifactKind::Binary),
            ] {
                let key = ArtifactKey {
                    instance_id: INSTANCE.into(),
                    kind,
                    name: name.to_string(),
                };
                found += usize::from(crate::families::family_for(adapter, &key).is_some());
            }
            let _ = glob.matches_name(name);
            let _ = crate::scan::display_path(&home.join(name), home);
            found
        },
        |_| Ok(()),
    );
    assert_none(problems);
}

#[test]
fn the_harness_is_deterministic_and_covers_its_mutations() {
    let a = inputs_for(99, &["npm/12.0.2/outdated-global.json"]);
    let b = inputs_for(99, &["npm/12.0.2/outdated-global.json"]);
    assert_eq!(a, b, "the same seed tries the same inputs");
    for kind in [
        "cut at",
        "flipped",
        "shuffled",
        "duplicated",
        "CRLF",
        "invalid UTF-8",
        "retyped",
        "unknown keys",
        "numbers 1e999",
        "10 MB line",
        "100k [",
        "localized error",
    ] {
        assert!(
            a.iter().any(|(label, _)| label.contains(kind)),
            "no input of kind {kind:?}"
        );
    }
}
