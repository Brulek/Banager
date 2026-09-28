//! What a `brew uninstall --cask` does beyond deleting what Homebrew
//! installed for the cask, read from what Homebrew recorded when it
//! installed it -- never from `brew info`, which loads the cask's current
//! definition, while the uninstall uses the recorded one.
//!
//! How Homebrew 7.0.6-70 finds the uninstall artifacts it runs
//! (`Cask::Installer#load_installed_caskfile!`, `cask/installer.rb:987-1045`):
//! it loads the caskfile it saved at install,
//! `Caskroom/<token>/.metadata/<version>/<timestamp>/Casks/<token>.<ext>`
//! -- the entry with the greatest name among every version's, the first of
//! `json`, `internal.json` and `rb` there (`Caskroom.cask_installed_caskfile`,
//! `cask/caskroom.rb:14`, `:47-62`).
//!
//! - A `.json` one is read with its own `artifacts` when it has them, else
//!   with the receipt's `uninstall_artifacts`, else -- a receipt that lists
//!   none -- with the cask's *current* definition
//!   (`FromAPILoader#load_from_json`, `cask/cask_loader.rb:468-480`;
//!   `resolve_installed_artifacts`, `:854-869`).
//! - A `.rb` one is Ruby. Homebrew 7 saves one only for a cask with
//!   `uninstall_preflight`/`uninstall_postflight` blocks
//!   (`save_caskfile`, `cask/installer.rb:594-607`), and the receipt written
//!   by the same install lists the artifacts it declares.
//! - An `.internal.json` one is a legacy full definition, which Canager
//!   does not read.
//!
//! The receipt, `Caskroom/<token>/.metadata/INSTALL_RECEIPT.json`, is
//! written at install from the same definition: `uninstall_artifacts` is
//! `Cask#artifacts_list(uninstall_only: true)` (`cask/tab.rb:31-45`,
//! `cask/cask.rb:709-732`), one `{ "<stanza>": [<its arguments>] }` object
//! per artifact that has an uninstall phase, plus the `zap` stanza, which
//! runs only with `--zap`; `uninstall_flight_blocks` says whether the cask
//! had those Ruby blocks.
//!
//! `read_recorded` finds and reads those files; `classify` says, from what
//! they list, whether the uninstall is plain and which kinds of extra step
//! it takes, each named as the record names it (`CaskStep`). Both only
//! read: nothing is written, nothing is run.

use crate::model::CaskStep;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The uninstall artifacts `brew uninstall --cask` will run, as Homebrew
/// recorded them at install.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Recorded {
    /// One `{ "<stanza>": [<arguments>] }` object per artifact.
    pub(crate) artifacts: Vec<Value>,
    /// The receipt's `uninstall_flight_blocks`: the cask has Ruby that runs
    /// before or after its uninstall.
    pub(crate) flight_blocks: bool,
}

/// The receipt's name in a cask's `.metadata` folder (`AbstractTab::FILENAME`).
const RECEIPT: &str = "INSTALL_RECEIPT.json";

/// `token`'s recorded uninstall under the Homebrew at `prefix`, or `None`
/// when Canager cannot tell what Homebrew will run: no Caskroom folder for
/// the token (or one that is a link), no saved caskfile, a legacy
/// `.internal.json` one, a file that does not parse as the object it
/// should be, or no list of artifacts short of the cask's current
/// definition. `token` is the cask's token or full name
/// (`gautham-v/tap/claudebar`); the Caskroom folder is its last part, as
/// `Caskroom.token_from_full_token` takes it (`cask/caskroom.rb`).
pub(crate) fn read_recorded(prefix: &Path, token: &str) -> Option<Recorded> {
    let token = caskroom_token(token)?;
    let caskroom = prefix.join("Caskroom").join(token);
    // Homebrew skips a Caskroom folder that is a link (`cask/caskroom.rb:51`).
    if !std::fs::symlink_metadata(&caskroom).ok()?.is_dir() {
        return None;
    }
    let metadata = caskroom.join(".metadata");
    let caskfile = saved_caskfile(&metadata, token)?;
    let receipt = read_regular_file(&metadata.join(RECEIPT))
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let receipt = receipt.as_ref().and_then(Value::as_object);
    let flight_blocks = receipt
        .and_then(|r| r.get("uninstall_flight_blocks"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // The receipt's list, when it lists anything: Homebrew takes an empty
    // one as none (`artifacts.presence`) and falls back to the cask's
    // current definition, which is not a record.
    let from_receipt = || {
        receipt
            .and_then(|r| r.get("uninstall_artifacts"))
            .and_then(Value::as_array)
            .filter(|list| !list.is_empty())
            .cloned()
    };
    let name = caskfile.file_name()?.to_str()?;
    let artifacts = if name.ends_with(".internal.json") {
        return None;
    } else if name.ends_with(".json") {
        let saved: Value = serde_json::from_slice(&read_regular_file(&caskfile)?).ok()?;
        match saved.as_object()?.get("artifacts") {
            Some(Value::Array(list)) => list.clone(),
            None | Some(Value::Null) => from_receipt()?,
            Some(_) => return None,
        }
    } else {
        from_receipt()?
    };
    Some(Recorded {
        artifacts,
        flight_blocks,
    })
}

/// The folder name under `Caskroom` for a token or full name, as
/// `Caskroom.token_from_full_token` splits it (`name.split("/", 3)`: the
/// third part when there is one, else the whole name); `None` for a name
/// that would not be one folder there.
fn caskroom_token(name: &str) -> Option<&str> {
    let token = name.splitn(3, '/').nth(2).unwrap_or(name);
    let plain = !token.is_empty() && token != "." && token != ".." && !token.contains('/');
    plain.then_some(token)
}

/// `Caskroom.cask_installed_caskfile` for one token: among the entries
/// two levels under `.metadata` (hidden names skipped, as Ruby's `Dir.glob`
/// skips them), the one with the greatest name -- the first of those, in
/// sorted order, when two share it -- and in it the first of
/// `Casks/<token>.json`, `.internal.json` and `.rb` that exists.
fn saved_caskfile(metadata: &Path, token: &str) -> Option<PathBuf> {
    let mut newest: Option<(Vec<u8>, PathBuf)> = None;
    for version in sorted_entries(metadata) {
        if !version.is_dir() {
            continue;
        }
        for entry in sorted_entries(&version) {
            let name = entry.file_name()?.as_encoded_bytes().to_vec();
            if newest.as_ref().is_none_or(|(best, _)| name > *best) {
                newest = Some((name, entry));
            }
        }
    }
    let (_, timestamped) = newest?;
    ["json", "internal.json", "rb"]
        .iter()
        .map(|ext| timestamped.join("Casks").join(format!("{token}.{ext}")))
        .find(|path| path.exists())
}

/// The entries of `dir` whose names do not start with `.`, sorted by name;
/// none when it cannot be listed.
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok())
        .filter(|entry| !entry.file_name().as_encoded_bytes().starts_with(b"."))
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    entries
}

/// A file's bytes, or `None` unless `path` leads, links followed, to a
/// regular file Canager can read. Nothing but a regular file is opened --
/// a named pipe would wait for a writer -- and the open file is checked
/// again, as `brew_env::read_brew_env_file` does.
fn read_regular_file(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// What a recorded cask uninstall does beyond deleting what Homebrew
/// installed, for the sentence the confirmation says under the tool and
/// the lines it lists under 「请注意」.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Classified {
    /// Nothing but what Homebrew put down and linked for the cask, apps
    /// quit, folders left empty removed, and paths' owners or permissions
    /// changed or processes ended: its settings and data stay.
    Plain,
    /// Extra steps, one entry per kind in `CaskStep`'s order, each with
    /// what the record names for it (nothing for `RunsOwnSteps`).
    Steps(Vec<(CaskStep, Vec<String>)>),
    /// A record Canager does not read: an artifact, a directive or a value
    /// of a shape it does not know. Says nothing it cannot back.
    Unknown,
}

/// Stanzas whose uninstall removes only what Homebrew itself put down or
/// linked for the cask, with the artifact classes each is in Homebrew
/// 7.0.6-70 (`cask/dsl.rb:39-72`): the moved kinds (`cask/artifact/moved.rb`,
/// deleted from their target -- `keyboard_layout` also clears macOS's
/// keyboard layout cache, `qlplugin` reloads Quick Look), the linked kinds
/// (`symlinked.rb`, a link removed only when it is one) and the completion
/// files Homebrew generated. `preflight_steps` and `postflight_steps` run at
/// uninstall only to remove the links they made and marked so
/// (`Runner#run_uninstall_step`, `install_steps.rb:1349-1363`). `zap` runs
/// only with `--zap`, which Canager never passes. `artifact`, a moved kind
/// whose target the cask chooses, is `classify`'s own case.
const PLAIN_STANZAS: [&str; 28] = [
    "app",
    "app_image",
    "audio_unit_plugin",
    "colorpicker",
    "dictionary",
    "font",
    "input_method",
    "internet_plugin",
    "keyboard_layout",
    "mdimporter",
    "prefpane",
    "qlplugin",
    "screen_saver",
    "service",
    "suite",
    "vst3_plugin",
    "vst_plugin",
    "binary",
    "command_wrapper",
    "manpage",
    "bash_completion",
    "fish_completion",
    "pwsh_completion",
    "zsh_completion",
    "generate_completions_from_executable",
    "preflight_steps",
    "postflight_steps",
    "zap",
];

/// Uninstall steps that leave the cask plain: they change who owns a path
/// or its permissions so it can be removed, or end a process.
const PLAIN_STEP_TYPES: [&str; 3] = ["set_ownership", "set_permissions", "terminate_process"];

/// The kinds of extra step `recorded` takes, or `Plain` when it takes none,
/// or `Unknown`. `home` is the home folder, to shorten a path in it to `~`
/// and to tell whether an `artifact` lands in it; unknown, every absolute
/// `artifact` target counts as possibly in it.
///
/// Plain: every artifact is one of `PLAIN_STANZAS`, or an `artifact` placed
/// outside the home folder, or `uninstall` with nothing but `quit`,
/// `signal` (apps quit) and `rmdir` (a folder removed only when nothing but
/// empty folders and `.DS_Store` files is left in it,
/// `abstract_uninstall.rb:695-750`), or uninstall steps of the types in
/// `PLAIN_STEP_TYPES`; and there are no Ruby flight blocks. Anything else
/// is a kind of extra step, and then the apps quit are said too.
pub(crate) fn classify(recorded: &Recorded, home: Option<&Path>) -> Classified {
    let mut steps = Steps::default();
    if recorded.flight_blocks {
        steps.flag(CaskStep::RunsOwnSteps);
    }
    for artifact in &recorded.artifacts {
        let Some((stanza, args)) = single_entry(artifact) else {
            return Classified::Unknown;
        };
        let known = match stanza {
            s if PLAIN_STANZAS.contains(&s) => true,
            "artifact" => match artifact_target(args) {
                Some(target) => {
                    if in_home(target, home) {
                        steps.add(CaskStep::Deletes, [target.to_string()], home);
                    }
                    true
                }
                None => false,
            },
            "uninstall" => uninstall_directives(args, &mut steps, home),
            "uninstall_preflight_steps" | "uninstall_postflight_steps" => {
                uninstall_steps(args, &mut steps, home)
            }
            // A Ruby block the receipt marks by name only
            // (`artifacts_list`: `{ summarize => nil }`).
            "uninstall_preflight" | "uninstall_postflight" => {
                steps.flag(CaskStep::RunsOwnSteps);
                true
            }
            _ => false,
        };
        if !known {
            return Classified::Unknown;
        }
    }
    steps.finish()
}

/// The kinds found so far, each with its names in the order first met and
/// each name once.
#[derive(Default)]
struct Steps {
    kinds: BTreeMap<CaskStep, Vec<String>>,
}

impl Steps {
    /// `step` with each of `items`, shortened to `~` in the home folder. A
    /// kind given no names is not a step: an empty `delete: []` deletes
    /// nothing.
    fn add(
        &mut self,
        step: CaskStep,
        items: impl IntoIterator<Item = String>,
        home: Option<&Path>,
    ) {
        for item in items {
            let shown = shown(&item, home);
            let names = self.kinds.entry(step).or_default();
            if !names.contains(&shown) {
                names.push(shown);
            }
        }
    }

    /// `step` without names.
    fn flag(&mut self, step: CaskStep) {
        self.kinds.entry(step).or_default();
    }

    /// `Plain` when only apps are quit, else every kind, in order.
    fn finish(self) -> Classified {
        if self.kinds.keys().all(|step| *step == CaskStep::QuitsApps) {
            return Classified::Plain;
        }
        Classified::Steps(self.kinds.into_iter().collect())
    }
}

/// The one stanza an artifact object holds, with its arguments.
fn single_entry(artifact: &Value) -> Option<(&str, &Value)> {
    let object = artifact.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.iter().next().map(|(key, args)| (key.as_str(), args))
}

/// The `target:` of an `artifact` stanza (`["<source>", {"target": …}]`),
/// which Homebrew requires of it.
fn artifact_target(args: &Value) -> Option<&str> {
    args.as_array()?
        .iter()
        .find_map(|arg| arg.as_object()?.get("target")?.as_str())
}

/// Whether `target` is in the home folder: `~` or under it, or, the home
/// folder unknown, any absolute path.
fn in_home(target: &str, home: Option<&Path>) -> bool {
    if target == "~" || target.starts_with("~/") {
        return true;
    }
    match home {
        Some(home) => Path::new(target).starts_with(home),
        None => Path::new(target).is_absolute(),
    }
}

/// `item` with the home folder shortened to `~` (`scan::display_path`).
fn shown(item: &str, home: Option<&Path>) -> String {
    match home {
        Some(home) if item.starts_with('/') => crate::scan::display_path(Path::new(item), home)
            .to_string_lossy()
            .into_owned(),
        _ => item.to_string(),
    }
}

/// The directives an `uninstall` stanza may hold, in the order Homebrew
/// runs them (`ORDERED_DIRECTIVES`, `abstract_uninstall.rb:23-35`), then
/// its one setting (`METADATA_KEYS`, `:37-39`). Read in this order, so the
/// names under each kind come in the order Homebrew meets them, whatever
/// order the record's object keeps its keys in.
const DIRECTIVES: [&str; 12] = [
    "early_script",
    "launchctl",
    "quit",
    "signal",
    "login_item",
    "kext",
    "script",
    "pkgutil",
    "delete",
    "trash",
    "rmdir",
    "on_upgrade",
];

/// The `uninstall` stanza's directives (`[{ "<directive>": <value>, … }]`),
/// each by what Homebrew does with it; false for a directive Homebrew
/// does not take (it refuses the stanza, `assert_valid_keys`) or a value of
/// a shape it does not.
fn uninstall_directives(args: &Value, steps: &mut Steps, home: Option<&Path>) -> bool {
    let Some(args) = args.as_array() else {
        return false;
    };
    for directives in args {
        let Some(directives) = directives.as_object() else {
            return false;
        };
        if directives
            .keys()
            .any(|key| !DIRECTIVES.contains(&key.as_str()))
        {
            return false;
        }
        for directive in DIRECTIVES {
            let Some(value) = directives.get(directive) else {
                continue;
            };
            let taken = match directive {
                "early_script" | "script" => scripts(value)
                    .map(|programs| steps.add(CaskStep::RunsScript, programs, home))
                    .is_some(),
                "launchctl" => add_strings(steps, CaskStep::RemovesServices, value, home),
                "quit" => add_strings(steps, CaskStep::QuitsApps, value, home),
                "signal" => signalled(value)
                    .map(|ids| steps.add(CaskStep::QuitsApps, ids, home))
                    .is_some(),
                "login_item" => login_items(value)
                    .map(|items| steps.add(CaskStep::RemovesLoginItems, items, home))
                    .is_some(),
                "kext" => add_strings(steps, CaskStep::RemovesKexts, value, home),
                "pkgutil" => add_strings(steps, CaskStep::RemovesPackages, value, home),
                "delete" => add_strings(steps, CaskStep::Deletes, value, home),
                "trash" => add_strings(steps, CaskStep::Trashes, value, home),
                // Removes a folder only when nothing but empty folders and
                // `.DS_Store` files is left in it: no line.
                "rmdir" => strings(value).is_some(),
                // When to signal during an upgrade: not a step.
                _ => true,
            };
            if !taken {
                return false;
            }
        }
    }
    true
}

/// A directive's strings under `step`; false when the value is not one
/// string or a list of them.
fn add_strings(steps: &mut Steps, step: CaskStep, value: &Value, home: Option<&Path>) -> bool {
    strings(value)
        .map(|items| steps.add(step, items, home))
        .is_some()
}

/// One string, or a list of strings, as Homebrew's `Array(value)` takes a
/// directive's value.
fn strings(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(one) => Some(vec![one.clone()]),
        Value::Array(list) => list
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => None,
    }
}

/// The programs `early_script:`/`script:` run: a string is the program,
/// an object names it as `executable` (`read_script_arguments`,
/// `abstract_artifact.rb:154-189`); Homebrew refuses one without.
fn scripts(value: &Value) -> Option<Vec<String>> {
    let one = |value: &Value| match value {
        Value::String(program) => Some(program.clone()),
        Value::Object(script) => script.get("executable")?.as_str().map(str::to_string),
        _ => None,
    };
    match value {
        Value::Array(list) => list.iter().map(one).collect(),
        other => one(other).map(|program| vec![program]),
    }
}

/// The bundle ids `signal:` signals: its value flattened into
/// `[signal, bundle id]` pairs, as `AbstractUninstall#initialize` reads it;
/// `None` when that does not pair up.
fn signalled(value: &Value) -> Option<Vec<String>> {
    fn flatten(value: &Value, out: &mut Vec<String>) -> Option<()> {
        match value {
            Value::String(one) => out.push(one.clone()),
            Value::Array(list) => {
                for item in list {
                    flatten(item, out)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
    let mut flat = Vec::new();
    flatten(value, &mut flat)?;
    if flat.len() % 2 != 0 {
        return None;
    }
    Some(flat.into_iter().skip(1).step_by(2).collect())
}

/// The login items `login_item:` deletes: a name, or `{ "path": … }`
/// (`uninstall_login_item`, `abstract_uninstall.rb:526-553`).
fn login_items(value: &Value) -> Option<Vec<String>> {
    let one = |value: &Value| match value {
        Value::String(name) => Some(name.clone()),
        Value::Object(item) => item.get("path")?.as_str().map(str::to_string),
        _ => None,
    };
    match value {
        Value::Array(list) => list.iter().map(one).collect(),
        other => one(other).map(|item| vec![item]),
    }
}

/// `uninstall_preflight_steps`/`uninstall_postflight_steps`
/// (`[{ "steps": [ { "type": …, … } ] }]`), which run with every step type
/// (`install_steps.rb` `Runner#run_install_step`). A step Canager names goes
/// under its kind; one it does not, under `RunsOwnSteps`. A `remove` step
/// deletes for good (`install_steps.rb:1049-1068`): each path it names
/// outright goes under `Deletes`, and one it does not, under
/// `DeletesUnnamed` (`removed_path`).
fn uninstall_steps(args: &Value, steps: &mut Steps, home: Option<&Path>) -> bool {
    let Some(args) = args.as_array() else {
        return false;
    };
    for arg in args {
        let Some(list) = arg
            .as_object()
            .and_then(|arg| arg.get("steps"))
            .and_then(Value::as_array)
        else {
            return false;
        };
        for step in list {
            let Some(step) = step.as_object() else {
                return false;
            };
            let Some(kind) = step.get("type").and_then(Value::as_str) else {
                return false;
            };
            match kind {
                t if PLAIN_STEP_TYPES.contains(&t) => {}
                "run" => match run_program(step) {
                    Some(program) => steps.add(CaskStep::RunsScript, [program], home),
                    None => steps.flag(CaskStep::RunsOwnSteps),
                },
                // `security find-certificate -a -c <name> -Z`, then
                // `delete-certificate -Z` for each certificate it finds
                // (`install_steps.rb:1179-1210`): every one whose name
                // contains `name` (security(1): `-c` matches "every
                // certificate ... whose common name includes" it). With
                // `matching_certificate`, only the one whose hash is that
                // file's, which the line for the name would overstate.
                "delete_keychain_certificate" => match untemplated(step.get("name")) {
                    Some(name) if !step.contains_key("matching_certificate") => {
                        steps.add(CaskStep::DeletesCertificates, [name], home)
                    }
                    _ => steps.flag(CaskStep::RunsOwnSteps),
                },
                "remove" => {
                    // `step_paths(step, "paths")`: a list of path specs,
                    // each with its `path` (`install_steps.rb:1446-1449`).
                    let Some(specs) = step.get("paths").and_then(Value::as_array) else {
                        return false;
                    };
                    for spec in specs {
                        let Some(spec) = spec.as_object() else {
                            return false;
                        };
                        let Some(path) = spec.get("path").and_then(Value::as_str) else {
                            return false;
                        };
                        let base = match spec.get("base") {
                            None | Some(Value::Null) => None,
                            Some(Value::String(base)) => Some(base.as_str()),
                            Some(_) => return false,
                        };
                        match removed_path(path, base) {
                            Some(path) => steps.add(CaskStep::Deletes, [path], home),
                            None => steps.flag(CaskStep::DeletesUnnamed),
                        }
                    }
                }
                _ => steps.flag(CaskStep::RunsOwnSteps),
            }
        }
    }
    true
}

/// The path a `remove` step's path spec names, when the record says it
/// outright, as `Runner#resolve_path` resolves it
/// (`install_steps.rb:1540-1547`): with no base or an absolute one, a path
/// from `/` or `~` as recorded -- Homebrew expands the `~` to the home
/// folder -- and with the home folder as its base, `~/<path>`. `None` for
/// a path with a `{{…}}` template Homebrew fills in at run time, a relative
/// one it resolves against its working folder, and one it resolves against
/// any other base: a folder it knows only when it runs the step, such as
/// the cask's staged folder or, for `search_path`, each folder it looks
/// for commands in (`expand_path_glob`, `:1473-1497`).
fn removed_path(path: &str, base: Option<&str>) -> Option<String> {
    if path.contains("{{") {
        return None;
    }
    match base {
        None | Some("") | Some("absolute") => {
            (path.starts_with('/') || path.starts_with('~')).then(|| path.to_string())
        }
        // `Dir.home/path`, which an absolute `path` replaces.
        Some("home") if path.starts_with('/') => Some(path.to_string()),
        Some("home") => Some(format!("~/{path}")),
        Some(_) => None,
    }
}

/// The program a `run` step starts, when the record names it outright: a
/// path with no base or an absolute one, or one under the home folder
/// (`Runner#resolve_command`, `install_steps.rb:1551-1555`) -- never one
/// with a `{{…}}` template Homebrew fills in at run time.
fn run_program(step: &Map<String, Value>) -> Option<String> {
    let command = step.get("command")?.as_object()?;
    let path = untemplated(command.get("path"))?;
    match command.get("base").and_then(Value::as_str) {
        None | Some("") | Some("absolute") => Some(path),
        Some("home") => Some(format!("~/{path}")),
        Some(_) => None,
    }
}

/// A string value with no `{{…}}` template in it.
fn untemplated(value: Option<&Value>) -> Option<String> {
    value?
        .as_str()
        .filter(|text| !text.contains("{{"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CaskStep::*;

    /// The fixtures' home folder: the constructed receipts put `/$HOME` there
    /// (`adapters/fixtures/brew/7.0.6/README.md`).
    const HOME: &str = "/Users/someone";

    /// `adapters/fixtures/brew/7.0.6/receipts/<name>.json`.
    macro_rules! receipt {
        ($name:literal) => {
            (
                $name,
                include_str!(concat!(
                    "../../../../../adapters/fixtures/brew/7.0.6/receipts/",
                    $name,
                    ".json"
                )),
            )
        };
    }

    /// The five receipts recorded on this Mac, unedited.
    const RECORDED: [(&str, &str); 5] = [
        receipt!("claudebar"),
        receipt!("codexbar"),
        receipt!("libreoffice"),
        receipt!("onyx"),
        receipt!("package-manager-manager"),
    ];

    /// A receipt's record, read the way `read_recorded` reads one beside a
    /// saved caskfile that has no `artifacts` of its own.
    fn recorded(json: &str) -> Recorded {
        let receipt: Value = serde_json::from_str(json).expect("the fixture parses");
        Recorded {
            artifacts: receipt["uninstall_artifacts"]
                .as_array()
                .expect("uninstall_artifacts")
                .clone(),
            flight_blocks: receipt["uninstall_flight_blocks"]
                .as_bool()
                .unwrap_or(false),
        }
    }

    fn classified(json: &str) -> Classified {
        classify(&recorded(json), Some(Path::new(HOME)))
    }

    fn steps(kinds: &[(CaskStep, &[&str])]) -> Classified {
        Classified::Steps(
            kinds
                .iter()
                .map(|(step, items)| (*step, items.iter().map(|s| s.to_string()).collect()))
                .collect(),
        )
    }

    #[test]
    fn every_receipt_recorded_on_this_mac_is_plain() {
        // `quit`, `app`, `binary` (with and without a `target`),
        // `command_wrapper` and `zap`: nothing but what Homebrew put down,
        // an app quit, and a stanza that runs only with `--zap`.
        for (name, json) in RECORDED {
            assert_eq!(classified(json), Classified::Plain, "{name}");
        }
    }

    #[test]
    fn what_only_removes_what_homebrew_put_down_is_plain() {
        for (name, json) in [
            // `rmdir` removes a folder only when nothing but empty folders
            // and `.DS_Store` files is left in it.
            receipt!("malus"),
            // `signal` only ends the app's own process.
            receipt!("dbeaver-community"),
            // An `artifact` outside the home folder.
            receipt!("graalvm-jdk"),
            // Seven `font` stanzas, and no app at all.
            receipt!("font-fira-code"),
        ] {
            assert_eq!(classified(json), Classified::Plain, "{name}");
        }
    }

    #[test]
    fn each_kind_of_extra_step_is_named_with_what_the_receipt_names_for_it() {
        let cases: [((&str, &str), Classified); 13] = [
            (
                receipt!("microsoft-word"),
                steps(&[
                    (
                        RemovesPackages,
                        &[
                            "com.microsoft.package.Microsoft_Word.app",
                            "com.microsoft.pkg.licensing",
                        ],
                    ),
                    (RemovesServices, &["com.microsoft.office.licensingV2.helper"]),
                    (QuitsApps, &["com.microsoft.autoupdate2"]),
                ]),
            ),
            (
                receipt!("duckietv"),
                steps(&[
                    (
                        Deletes,
                        &[
                            "/Applications/duckieTV.app",
                            "~/Library/Application Support/DuckieTV-Standalone",
                        ],
                    ),
                    (RemovesPackages, &["tv.duckie.base.pkg"]),
                ]),
            ),
            (receipt!("nvs"), steps(&[(Trashes, &["~/.nvs"])])),
            (
                receipt!("gpt4all"),
                steps(&[
                    (Deletes, &["~/Library/Application Support/nomic.ai/GPT4All"]),
                    (
                        RunsScript,
                        &["/Applications/gpt4all/maintenancetool.app/Contents/MacOS/maintenancetool"],
                    ),
                ]),
            ),
            // A relative program is named as recorded (Homebrew runs it from
            // the cask's staged folder); `rmdir` gets no line.
            (
                receipt!("adobe-air"),
                steps(&[(
                    RunsScript,
                    &["Adobe AIR Installer.app/Contents/MacOS/Adobe AIR Installer"],
                )]),
            ),
            (
                receipt!("adobe-creative-cloud"),
                steps(&[
                    (
                        Deletes,
                        &[
                            "/Applications/Adobe Creative Cloud/*Adobe Creative Cloud",
                            "/Applications/Adobe Creative Cloud/.Uninstall*",
                            "/Applications/Adobe Creative Cloud/Icon?",
                            "/Applications/Utilities/Adobe Application Manager",
                            "/Applications/Utilities/Adobe Creative Cloud*",
                            "/Applications/Utilities/Adobe Installers/.Uninstall*",
                            "/Applications/Utilities/Adobe Installers/Uninstall Adobe Creative Cloud",
                            "/Applications/Utilities/Adobe Sync",
                            "/Library/Internet Plug-Ins/AdobeAAMDetect.plugin",
                            "/Library/LaunchDaemons/com.adobe.agsservice.plist",
                        ],
                    ),
                    // `early_script` before `script`, as Homebrew runs them.
                    (RunsScript, &["/usr/bin/pluginkit", "/usr/bin/pkill"]),
                    (
                        RemovesServices,
                        &[
                            "Adobe_Genuine_Software_Integrity_Service",
                            "com.adobe.acc.installer",
                            "com.adobe.acc.installer.v2",
                            "com.adobe.AdobeCreativeCloud",
                            "com.adobe.AdobeDesktopService",
                            "com.adobe.ccxprocess",
                            "com.adobe.CCXProcess.*",
                        ],
                    ),
                    // `quit`'s, then `signal`'s: the bundle id of its pair.
                    (
                        QuitsApps,
                        &["com.adobe.acc.AdobeCreativeCloud", "com.adobe.accmac"],
                    ),
                ]),
            ),
            (
                receipt!("gutenprint"),
                steps(&[
                    (
                        Deletes,
                        &[
                            "/usr/libexec/cups/backend/gutenprint*",
                            "/usr/libexec/cups/driver/gutenprint*",
                            "/usr/libexec/cups/filter/rastertogutenprint*",
                        ],
                    ),
                    (RemovesPackages, &["org.gutenprint.printer-driver"]),
                    // `script` as a list of hashes.
                    (
                        RunsScript,
                        &["/opt/homebrew/Caskroom/gutenprint/5.3.3/uninstall-gutenprint.command"],
                    ),
                ]),
            ),
            (
                receipt!("airscroll"),
                steps(&[(RemovesLoginItems, &["AirScroll"])]),
            ),
            (
                receipt!("airparrot"),
                steps(&[
                    (
                        RemovesKexts,
                        &[
                            "/Library/Extensions/AirParrotDriver.kext",
                            "/Library/Extensions/APExtFramebuffer.kext",
                            "/System/Library/Extensions/AirParrotDriver.kext",
                            "/System/Library/Extensions/APExtFramebuffer.kext",
                            "com.squirrels.driver.AirParrotSpeakers",
                        ],
                    ),
                    (QuitsApps, &["com.squirrels.AirParrot-3"]),
                ]),
            ),
            (
                receipt!("charles"),
                steps(&[
                    (
                        Deletes,
                        &["/Library/PrivilegedHelperTools/com.xk72.Charles.ProxyHelper"],
                    ),
                    (RemovesServices, &["com.xk72.Charles.ProxyHelper"]),
                    (DeletesCertificates, &["Charles"]),
                    (QuitsApps, &["com.xk72.Charles"]),
                ]),
            ),
            // A `run` step whose program is named outright; the install-time
            // `postflight_steps` (`set_ownership`) and the two completions
            // stay plain.
            (
                receipt!("openzfs"),
                steps(&[
                    (RemovesPackages, &["org.openzfsonosx.zfs"]),
                    (RunsScript, &["/usr/local/zfs/bin/zpool"]),
                    (
                        RemovesServices,
                        &[
                            "org.openzfsonosx.InvariantDisks",
                            "org.openzfsonosx.zconfigd",
                            "org.openzfsonosx.zed",
                            "org.openzfsonosx.zpool-import",
                            "org.openzfsonosx.zpool-import-all",
                        ],
                    ),
                ]),
            ),
            // A `move` step Canager does not name.
            (
                receipt!("miniconda"),
                steps(&[
                    (Deletes, &["/opt/homebrew/Caskroom/miniconda/base"]),
                    (RunsOwnSteps, &[]),
                ]),
            ),
            // `terminate_process` stays plain and unnamed.
            (
                receipt!("appvolume"),
                steps(&[
                    (
                        Deletes,
                        &[
                            "/Library/Audio/Plug-Ins/HAL/AppVolumeAudioDevice.driver",
                            "~/Library/LaunchAgents/io.appvolume.daemon.plist",
                        ],
                    ),
                    (
                        RemovesPackages,
                        &[
                            "io.appvolume.app",
                            "io.appvolume.daemon",
                            "io.appvolume.driver",
                            "io.appvolume.ui",
                        ],
                    ),
                    (RemovesServices, &["io.appvolume.daemon"]),
                    (QuitsApps, &["io.appvolume"]),
                ]),
            ),
        ];
        for ((name, json), expected) in cases {
            assert_eq!(classified(json), expected, "{name}");
        }
    }

    #[test]
    fn an_artifact_placed_in_the_home_folder_is_deleted_again_and_said() {
        // `~` as the cask wrote it.
        let (_, twelite) = receipt!("twelite-stage");
        assert_eq!(classified(twelite), steps(&[(Deletes, &["~/MWSTAGE"])]));
        // `/$HOME`, which Homebrew fills in with the home folder's path:
        // said with `~`.
        let (_, touchosc) = receipt!("touchosc-editor");
        assert_eq!(
            classified(touchosc),
            steps(&[(
                Deletes,
                &["~/Library/Application Support/TouchOSCEditor/layouts"]
            )])
        );
        // Someone else's home folder is not this one's.
        assert_eq!(
            classify(&recorded(touchosc), Some(Path::new("/Users/other"))),
            Classified::Plain
        );
        // With the home folder unknown, any absolute target may be in it.
        assert_eq!(
            classify(&recorded(touchosc), None),
            steps(&[(
                Deletes,
                &["/Users/someone/Library/Application Support/TouchOSCEditor/layouts"]
            )])
        );
    }

    #[test]
    fn ruby_around_the_uninstall_runs_steps_canager_cannot_name() {
        let (_, json) = receipt!("uninstall-flight-block");
        assert_eq!(classified(json), steps(&[(RunsOwnSteps, &[])]));
        // The receipt's flag alone says so too.
        let flagged = Recorded {
            artifacts: vec![serde_json::json!({ "app": ["Some.app"] })],
            flight_blocks: true,
        };
        assert_eq!(
            classify(&flagged, Some(Path::new(HOME))),
            steps(&[(RunsOwnSteps, &[])])
        );
    }

    #[test]
    fn a_record_canager_does_not_read_is_unknown_not_plain() {
        let (_, json) = receipt!("unknown-stanza");
        assert_eq!(classified(json), Classified::Unknown);
        let unknown = |artifacts: Value| {
            classify(
                &Recorded {
                    artifacts: artifacts.as_array().unwrap().clone(),
                    flight_blocks: false,
                },
                Some(Path::new(HOME)),
            )
        };
        for artifacts in [
            // A directive Homebrew does not take.
            serde_json::json!([{ "uninstall": [{ "erase": "~/x" }] }]),
            // A `signal` that does not pair up.
            serde_json::json!([{ "uninstall": [{ "signal": ["TERM"] }] }]),
            // A `script` that names no program: Homebrew refuses it.
            serde_json::json!([{ "uninstall": [{ "script": { "args": ["-q"] } }] }]),
            // A `delete` that is not a path or a list of paths.
            serde_json::json!([{ "uninstall": [{ "delete": 1 }] }]),
            // An entry with two stanzas, and one that is not an object.
            serde_json::json!([{ "app": ["A.app"], "font": ["a.ttf"] }]),
            serde_json::json!(["app"]),
            // An `artifact` without its target.
            serde_json::json!([{ "artifact": ["x"] }]),
            // Uninstall steps without their list, or a step without a type.
            serde_json::json!([{ "uninstall_postflight_steps": [{}] }]),
            serde_json::json!([{ "uninstall_postflight_steps": [{ "steps": [{ "name": "x" }] }] }]),
        ] {
            assert_eq!(
                unknown(artifacts.clone()),
                Classified::Unknown,
                "{artifacts}"
            );
        }
        // An empty directive does nothing, so says nothing.
        assert_eq!(
            unknown(serde_json::json!([{ "uninstall": [{ "delete": [], "quit": "a.b" }] }])),
            Classified::Plain
        );
    }

    #[test]
    fn uninstall_steps_name_their_program_or_certificate_only_when_the_record_does() {
        let classify_steps = |steps: Value| {
            classify(
                &Recorded {
                    artifacts: vec![serde_json::json!({
                        "uninstall_preflight_steps": [{ "steps": steps }]
                    })],
                    flight_blocks: false,
                },
                Some(Path::new(HOME)),
            )
        };
        // The three that stay plain.
        assert_eq!(
            classify_steps(serde_json::json!([
                { "type": "set_ownership", "paths": [{ "path": "/Library/X" }] },
                { "type": "set_permissions", "paths": [{ "path": "/Library/X" }], "permissions": "0755" },
                { "type": "terminate_process", "name": "coreaudiod" }
            ])),
            Classified::Plain
        );
        // A program under the home folder is named with `~`; one Homebrew
        // resolves against a base it fills in at run time, or with a
        // template, is not named.
        assert_eq!(
            classify_steps(serde_json::json!([
                { "type": "run", "command": { "base": "home", "path": "bin/tool" } },
                { "type": "run", "command": { "base": "staged_path", "path": "uninstall.sh" } },
                { "type": "run", "command": { "path": "{{appdir}}/X.app/uninstall" } },
                { "type": "delete_keychain_certificate", "name": "{{name}} CA" }
            ])),
            steps(&[(RunsScript, &["~/bin/tool"]), (RunsOwnSteps, &[])])
        );
    }

    #[test]
    fn a_certificate_step_names_the_text_every_deleted_certificates_name_contains() {
        // Each step deletes every certificate `security find-certificate -a
        // -c <name>` lists: here, every one whose name contains
        // `127.0.0.1` too.
        let (_, autofirma) = receipt!("autofirma");
        assert_eq!(
            classified(autofirma),
            steps(&[
                (Deletes, &["/Applications/AutoFirma.app"]),
                (RemovesPackages, &["es.gob.afirma"]),
                (DeletesCertificates, &["AutoFirma ROOT", "127.0.0.1"]),
                (QuitsApps, &["es.gob.afirma"]),
            ])
        );
        // One that deletes only the certificate matching a file is not
        // that line.
        let (_, betwixt) = receipt!("betwixt");
        assert_eq!(classified(betwixt), steps(&[(RunsOwnSteps, &[])]));
    }

    #[test]
    fn a_remove_step_deletes_for_good_and_names_each_path_the_record_spells_out() {
        let classify_remove = |paths: Value| {
            classify(
                &Recorded {
                    artifacts: vec![serde_json::json!({
                        "uninstall_postflight_steps": [{ "steps": [
                            { "type": "remove", "paths": paths, "recursive": true }
                        ] }]
                    })],
                    flight_blocks: false,
                },
                Some(Path::new(HOME)),
            )
        };
        // No base or an absolute one: the path from `/` or `~` as recorded,
        // the home folder shortened to `~`, a glob as it stands. The home
        // folder as the base: `~/<path>`.
        assert_eq!(
            classify_remove(serde_json::json!([
                { "path": "/usr/local/bin/gpg" },
                { "path": "/Users/someone/Library/Application Support/Foo" },
                { "path": "~/.foo", "base": "absolute" },
                { "path": "/usr/local/bin/arm-*" },
                { "path": ".config/foo", "base": "home" }
            ])),
            steps(&[(
                Deletes,
                &[
                    "/usr/local/bin/gpg",
                    "~/Library/Application Support/Foo",
                    "~/.foo",
                    "/usr/local/bin/arm-*",
                    "~/.config/foo",
                ]
            )])
        );
        // What Homebrew resolves only when it runs the step: still deleted
        // for good, named by nothing.
        for unnamed in [
            serde_json::json!({ "path": "charm", "base": "search_path" }),
            serde_json::json!({ "path": "uninstall.sh", "base": "staged_path" }),
            serde_json::json!({ "path": "relative/file" }),
            serde_json::json!({ "path": "{{appdir}}/Foo.app" }),
        ] {
            assert_eq!(
                classify_remove(serde_json::json!([unnamed.clone()])),
                steps(&[(DeletesUnnamed, &[])]),
                "{unnamed}"
            );
        }
        // Both kinds from one step, each once.
        assert_eq!(
            classify_remove(serde_json::json!([
                { "path": "/usr/local/bin/x" },
                { "path": "x", "base": "staged_path" },
                { "path": "/usr/local/bin/x" },
                { "path": "y", "base": "staged_path" }
            ])),
            steps(&[(Deletes, &["/usr/local/bin/x"]), (DeletesUnnamed, &[])])
        );
        // No path at all deletes nothing.
        assert_eq!(classify_remove(serde_json::json!([])), Classified::Plain);
        // A spec Homebrew would not take.
        for bad in [
            serde_json::json!({ "path": "/x" }),
            serde_json::json!(["/x"]),
            serde_json::json!([{ "base": "home" }]),
            serde_json::json!([{ "path": "/x", "base": 1 }]),
        ] {
            assert_eq!(classify_remove(bad.clone()), Classified::Unknown, "{bad}");
        }
    }

    #[test]
    fn a_remove_step_from_the_catalogue_is_a_permanent_deletion() {
        // pycharm-edu's only extra step removes `charm` from each folder
        // Homebrew looks for commands in: no path to name, still for good.
        let (_, pycharm) = receipt!("pycharm-edu");
        assert_eq!(classified(pycharm), steps(&[(DeletesUnnamed, &[])]));
        // playdate-simulator's step names its links outright; they go with
        // what its `delete:` names, in the order Homebrew meets them.
        let (_, playdate) = receipt!("playdate-simulator");
        assert_eq!(
            classified(playdate),
            steps(&[
                (Deletes, &["/usr/local/bin/arm-*", "/usr/local/playdate"]),
                (Trashes, &["~/Developer/PlaydateSDK"]),
                (RemovesPackages, &["date.play.sdk"]),
            ])
        );
    }

    #[test]
    fn a_path_in_the_home_folder_is_shown_with_a_tilde() {
        let home = Some(Path::new(HOME));
        assert_eq!(shown("/Users/someone/Library/X", home), "~/Library/X");
        assert_eq!(shown("/Users/someone", home), "~");
        assert_eq!(shown("~/Library/X", home), "~/Library/X");
        // Another folder that only starts with the same letters.
        assert_eq!(shown("/Users/someones/X", home), "/Users/someones/X");
        assert_eq!(shown("com.example.app", home), "com.example.app");
        assert_eq!(shown("/Users/someone/X", None), "/Users/someone/X");
    }

    /// A Homebrew prefix of a test's own, removed when the test ends.
    struct Prefix(PathBuf);

    impl Prefix {
        fn new(label: &str) -> Prefix {
            let dir = std::env::temp_dir().join(format!(
                "canager-cask-receipt-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).expect("create the prefix");
            Prefix(dir)
        }

        fn metadata(&self, token: &str) -> PathBuf {
            self.0.join("Caskroom").join(token).join(".metadata")
        }

        fn receipt(&self, token: &str, json: &str) {
            let metadata = self.metadata(token);
            std::fs::create_dir_all(&metadata).unwrap();
            std::fs::write(metadata.join(RECEIPT), json).unwrap();
        }

        /// `.metadata/<version>/<timestamp>/Casks/<file>`, as Homebrew saves it.
        fn caskfile(
            &self,
            token: &str,
            version: &str,
            timestamp: &str,
            file: &str,
            contents: &str,
        ) {
            let casks = self
                .metadata(token)
                .join(version)
                .join(timestamp)
                .join("Casks");
            std::fs::create_dir_all(&casks).unwrap();
            std::fs::write(casks.join(file), contents).unwrap();
        }
    }

    impl Drop for Prefix {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reads_the_receipt_beside_the_caskfile_homebrew_7_saves() {
        // Homebrew 7 saves `{}` (or only `url_specs`) as the caskfile and
        // keeps the list in the receipt, as the five casks on this Mac show.
        let prefix = Prefix::new("receipt");
        let (_, json) = receipt!("microsoft-word");
        prefix.receipt("microsoft-word", json);
        prefix.caskfile(
            "microsoft-word",
            "16.113.26092012",
            "20260927132900.000",
            "microsoft-word.json",
            "{}",
        );
        assert_eq!(
            read_recorded(&prefix.0, "microsoft-word"),
            Some(recorded(json))
        );
        // A full name finds the same folder: its last part.
        assert_eq!(
            read_recorded(&prefix.0, "homebrew/cask/microsoft-word"),
            Some(recorded(json))
        );
    }

    #[test]
    fn a_caskfile_with_its_own_artifacts_is_what_homebrew_runs() {
        let prefix = Prefix::new("own-artifacts");
        let (_, json) = receipt!("microsoft-word");
        prefix.receipt("word", json);
        prefix.caskfile(
            "word",
            "1.0",
            "20260101000000.000",
            "word.json",
            r#"{"artifacts": [{"app": ["Word.app"]}]}"#,
        );
        let read = read_recorded(&prefix.0, "word").expect("recorded");
        assert_eq!(
            read.artifacts,
            vec![serde_json::json!({"app": ["Word.app"]})]
        );
        assert_eq!(classify(&read, Some(Path::new(HOME))), Classified::Plain);
        // An empty list is a list: nothing to run (`save_caskfile` writes
        // one for a cask with no uninstall artifacts).
        prefix.caskfile(
            "word",
            "1.0",
            "20260101000000.000",
            "word.json",
            r#"{"artifacts": []}"#,
        );
        let read = read_recorded(&prefix.0, "word").expect("recorded");
        assert!(read.artifacts.is_empty());
        assert_eq!(classify(&read, Some(Path::new(HOME))), Classified::Plain);
    }

    #[test]
    fn the_newest_saved_caskfile_is_the_one_read() {
        // The greatest timestamp across every version folder, as
        // `Caskroom.cask_installed_caskfile` picks it.
        let prefix = Prefix::new("newest");
        prefix.receipt("app", r#"{"uninstall_artifacts": [{"app": ["Old.app"]}]}"#);
        prefix.caskfile(
            "app",
            "2.0",
            "20250101000000.000",
            "app.json",
            r#"{"artifacts": [{"uninstall": [{"delete": "~/older"}]}]}"#,
        );
        prefix.caskfile(
            "app",
            "1.0",
            "20260101000000.000",
            "app.json",
            r#"{"artifacts": [{"uninstall": [{"trash": "~/newer"}]}]}"#,
        );
        let read = read_recorded(&prefix.0, "app").expect("recorded");
        assert_eq!(
            classify(&read, Some(Path::new(HOME))),
            steps(&[(Trashes, &["~/newer"])])
        );
    }

    #[test]
    fn a_ruby_caskfile_is_read_through_its_receipt() {
        // Homebrew 7 saves Ruby only for a cask with uninstall flight
        // blocks; the receipt of the same install lists its artifacts.
        let prefix = Prefix::new("ruby");
        let (_, json) = receipt!("uninstall-flight-block");
        prefix.receipt("uninstall-flight-block", json);
        prefix.caskfile(
            "uninstall-flight-block",
            "1.0",
            "20250101000000.000",
            "uninstall-flight-block.rb",
            "cask \"uninstall-flight-block\" do\nend\n",
        );
        let read = read_recorded(&prefix.0, "uninstall-flight-block").expect("recorded");
        assert!(read.flight_blocks);
        assert_eq!(
            classify(&read, Some(Path::new(HOME))),
            steps(&[(RunsOwnSteps, &[])])
        );
    }

    #[test]
    fn nothing_is_said_of_what_homebrew_would_take_from_elsewhere() {
        let (_, json) = receipt!("onyx");
        // The receipt lists nothing: Homebrew would load the cask's current
        // definition instead.
        let prefix = Prefix::new("empty-receipt");
        prefix.receipt("onyx", r#"{"uninstall_artifacts": []}"#);
        prefix.caskfile("onyx", "5.0.2", "20260811074234.796", "onyx.json", "{}");
        assert_eq!(read_recorded(&prefix.0, "onyx"), None);
        // No receipt at all.
        let prefix = Prefix::new("no-receipt");
        prefix.caskfile("onyx", "5.0.2", "20260811074234.796", "onyx.json", "{}");
        assert_eq!(read_recorded(&prefix.0, "onyx"), None);
        // A receipt that does not parse.
        let prefix = Prefix::new("bad-receipt");
        prefix.receipt("onyx", "{ not json");
        prefix.caskfile("onyx", "5.0.2", "20260811074234.796", "onyx.json", "{}");
        assert_eq!(read_recorded(&prefix.0, "onyx"), None);
        // A legacy `.internal.json` definition.
        let prefix = Prefix::new("internal-json");
        prefix.receipt("onyx", json);
        prefix.caskfile(
            "onyx",
            "5.0.2",
            "20260811074234.796",
            "onyx.internal.json",
            "{}",
        );
        assert_eq!(read_recorded(&prefix.0, "onyx"), None);
        // No saved caskfile.
        let prefix = Prefix::new("no-caskfile");
        prefix.receipt("onyx", json);
        assert_eq!(read_recorded(&prefix.0, "onyx"), None);
        // No Caskroom folder for the token.
        assert_eq!(read_recorded(&prefix.0, "codexbar"), None);
        // A name that is not one folder under `Caskroom`.
        assert_eq!(read_recorded(&prefix.0, "a/b"), None);
        assert_eq!(read_recorded(&prefix.0, ".."), None);
    }

    #[test]
    fn a_caskroom_folder_that_is_a_link_is_not_read() {
        let prefix = Prefix::new("linked");
        let (_, json) = receipt!("onyx");
        prefix.receipt("real", json);
        prefix.caskfile("real", "5.0.2", "20260811074234.796", "onyx.json", "{}");
        std::os::unix::fs::symlink(
            prefix.0.join("Caskroom/real"),
            prefix.0.join("Caskroom/onyx"),
        )
        .unwrap();
        assert_eq!(read_recorded(&prefix.0, "onyx"), None);
    }
}
