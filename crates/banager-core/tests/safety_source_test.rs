//! Promises of `docs/what-we-run.md` that no example can pin, because
//! they are about code someone may add later: each is checked here over
//! the source of every production file of `banager-core` and the Tauri
//! shell (the part before its `#[cfg(test)] mod`, and no file that is
//! compiled for tests only). A new call of a kind a promise rules out
//! fails a test here and names the file and line, so the promise is kept
//! or the change is argued for and the list below changed with it.
//!
//! - Promise 1: a walk that must never look inside a protected place takes
//!   every step through `protected::resolve` and `dirfd`, never a path
//!   lookup of its own (`std::fs::metadata`, `canonicalize`, `exists`,
//!   `Path::is_dir`...), which would follow a link into `~/Documents` or
//!   onto `/Volumes`.
//! - Promise 3: nothing starts a process but `RealRunner`, which runs a
//!   confirmed plan or a read-only refresh command, and the Open Ollama
//!   button's `open -a Ollama`.
//! - Promise 4: nothing writes, creates, renames or deletes a file but
//!   `settings::save` and the history store (Banager's own two files), and
//!   `RealTrasher` moves a previewed path to the Trash. The third file,
//!   `.window-state.json`, is the Tauri plugin's, not code here.
//! - Promise 6: every file that is opened is opened without waiting
//!   (`O_NONBLOCK`) or as a folder (`O_DIRECTORY`), so a named pipe
//!   cannot stall a refresh; the few plain reads left are listed with why.

use std::path::{Path, PathBuf};

/// The workspace's root: two folders above this crate.
fn root() -> PathBuf {
    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

/// Files compiled only for tests (each declared under `#[cfg(test)]` or
/// the `test-support` feature in its parent; checked below).
const TEST_ONLY: [(&str, &str, &str); 5] = [
    (
        "crates/banager-core/src/adapters/robustness.rs",
        "crates/banager-core/src/adapters/mod.rs",
        "mod robustness;",
    ),
    (
        "crates/banager-core/src/trash/mock.rs",
        "crates/banager-core/src/trash/mod.rs",
        "pub mod mock;",
    ),
    (
        "crates/banager-core/src/icon/mock.rs",
        "crates/banager-core/src/icon/mod.rs",
        "pub mod mock;",
    ),
    (
        "crates/banager-core/src/session/test_support.rs",
        "crates/banager-core/src/session/mod.rs",
        "mod test_support;",
    ),
    (
        "crates/banager-core/src/commands/round_tests.rs",
        "crates/banager-core/src/commands.rs",
        "mod round_tests;",
    ),
];

/// One production line: its file (relative to the root), its number and
/// its text.
struct Line {
    file: String,
    number: usize,
    text: String,
}

/// Every `.rs` file under `dir`, recursively, sorted.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The production lines of every file: up to the first top-level
/// `#[cfg(test)]` that is followed by a `mod` line, comments left out.
fn production_lines() -> Vec<Line> {
    let root = root();
    let mut files = Vec::new();
    rust_files(&root.join("crates/banager-core/src"), &mut files);
    rust_files(&root.join("src-tauri/src"), &mut files);
    assert!(files.len() > 50, "found {} files", files.len());
    let mut lines = Vec::new();
    for path in files {
        let file = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if TEST_ONLY.iter().any(|(only, _, _)| *only == file) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let all: Vec<&str> = text.lines().collect();
        for (index, line) in all.iter().enumerate() {
            // A test module with a body ends the file's production part.
            if *line == "#[cfg(test)]"
                && all
                    .get(index + 1)
                    .is_some_and(|next| next.contains("mod ") && next.trim_end().ends_with('{'))
            {
                break;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            lines.push(Line {
                file: file.clone(),
                number: index + 1,
                text: line.to_string(),
            });
        }
    }
    lines
}

/// The lines among `lines` holding any of `patterns`, as `file:line: text`.
fn holding<'a>(lines: impl Iterator<Item = &'a Line>, patterns: &[&str]) -> Vec<String> {
    lines
        .filter(|line| patterns.iter().any(|pattern| line.text.contains(pattern)))
        .map(|line| format!("{}:{}: {}", line.file, line.number, line.text.trim()))
        .collect()
}

#[test]
fn test_the_test_only_files_are_compiled_for_tests_only() {
    // Leaving them out of the checks below is right only while each is
    // declared under `cfg(test)` (or the test-support feature).
    for (file, parent, declaration) in TEST_ONLY {
        assert!(root().join(file).is_file(), "{file}");
        let text = std::fs::read_to_string(root().join(parent)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|line| line.trim() == declaration)
            .unwrap_or_else(|| panic!("{parent} declares {declaration}"));
        assert!(
            lines[at - 1].trim().starts_with("#[cfg(") && lines[at - 1].contains("test"),
            "{parent}: {declaration} is not gated on tests: {:?}",
            lines[at - 1]
        );
    }
}

/// The calls that look a path up by its name, following any link on the
/// way: what a protected walk must never make. A bare name counts too
/// (`read_dir(` after `use std::fs::read_dir`), so each is matched without
/// its `fs::` or leading dot where that adds no false match. The kind
/// questions (`is_dir()` and the like) are `asks_a_path`'s.
const PATH_LOOKUPS: [&str; 17] = [
    "metadata(",
    "canonicalize(",
    "read_dir(",
    "read_link(",
    "fs::File",
    "File::open(",
    "fs::read",
    "fs::OpenOptions",
    "::exists(",
    ".exists()",
    ".try_exists(",
    "realpath",
    // Called as a function (`Path::is_dir(&p)`), or imported under
    // another name (`use std::fs::read_dir as list`): a walk takes
    // nothing from `std::fs`.
    "::is_dir(",
    "::is_file(",
    "::is_symlink(",
    "use std::fs",
    "std::fs::{",
];

/// What `is_dir()`, `is_file()` and `is_symlink()` may be asked of in a
/// walk: the answer of a step `resolve` or `dirfd` already took, by the
/// names the walks give it. `meta.is_dir()` on a `Stat` asks nothing of
/// the disk, where `path.is_dir()` stats `path` and follows every link in
/// it; the two can only be told apart by what the call is made on.
const STAT_NAMES: [&str; 4] = ["meta", "stat", "lstat", "target"];

/// Whether `text` asks `is_dir()`, `is_file()` or `is_symlink()` of
/// anything but a `STAT_NAMES` value or `Dir::stat_at`'s answer
/// (`held.stat_at(name).ok()?.is_symlink()`), such as
/// `Path::new("/Volumes").is_dir()` or `folder.join(name).is_file()`.
fn asks_a_path(text: &str) -> bool {
    [".is_dir(", ".is_file(", ".is_symlink("]
        .iter()
        .any(|call| {
            text.match_indices(call).any(|(at, _)| {
                let before = &text[..at];
                let name_starts = before
                    .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(0, |at| at + 1);
                let receiver = &before[name_starts..];
                let answered = STAT_NAMES.contains(&receiver)
                    || (receiver.is_empty()
                        && before.ends_with("ok()?")
                        && before.contains(".stat_at("));
                !answered
            })
        })
}

/// The lines among `lines` that look a path up: one of `PATH_LOOKUPS`,
/// or a kind question asked of a path (`asks_a_path`).
fn path_lookups<'a>(lines: impl Iterator<Item = &'a Line>) -> Vec<String> {
    lines
        .filter(|line| {
            PATH_LOOKUPS.iter().any(|lookup| line.text.contains(lookup)) || asks_a_path(&line.text)
        })
        .map(|line| format!("{}:{}: {}", line.file, line.number, line.text.trim()))
        .collect()
}

#[test]
fn test_a_kind_question_is_told_apart_by_what_it_is_asked_of() {
    for answered in [
        "if !meta.is_dir() && meta.nlink() > 1 {",
        "Resolution::Found(_, stat) if stat.is_symlink() => link = place,",
        "} else if lstat.is_file() {",
        "if target.is_dir() || (target.mode() & 0o111) == 0 {",
        "if !opened.same_as(&stat) || !held.stat_at(name).ok()?.is_symlink() {",
    ] {
        assert!(!asks_a_path(answered), "{answered}");
    }
    for lookup in [
        "if Path::new(\"/Volumes\").is_dir() {",
        "folder.join(name).is_file()",
        "if path.is_symlink() {",
        "if xstat.is_dir() {",
        "meta.is_dir() && root.is_dir()",
        "std::fs::read_dir(&p)",
        "read_dir(&p)",
        "File::open(p)",
        "std::fs::exists(p)",
        "Path::is_dir(&p)",
        "use std::fs::{read_dir as list};",
    ] {
        assert!(
            asks_a_path(lookup) || PATH_LOOKUPS.iter().any(|call| lookup.contains(call)),
            "{lookup}"
        );
    }
}

#[test]
fn test_no_protected_walk_looks_a_path_up_by_its_name() {
    // The walks of promise 1, file by file, and Claude Code's PATH note
    // (`shadow_note`) within its file, whose other functions follow the
    // tools' fixed paths as documented ("Where the program comes from").
    // Had one of them called `std::fs::metadata(path)`, the kernel would
    // follow a link in `path` into `~/Documents`, and the check before
    // each step would not have been asked.
    let walks = [
        "crates/banager-core/src/protected.rs",
        "crates/banager-core/src/protected/round.rs",
        "crates/banager-core/src/commands.rs",
        "crates/banager-core/src/size.rs",
        "crates/banager-core/src/kept_data.rs",
        "crates/banager-core/src/scan/mod.rs",
        "crates/banager-core/src/runner/path_env.rs",
        "crates/banager-core/src/session/kept.rs",
        "crates/banager-core/src/session/scan.rs",
        "crates/banager-core/src/session/sizes.rs",
    ];
    let lines = production_lines();
    for walk in walks {
        assert!(
            lines.iter().any(|line| line.file == walk),
            "{walk} is not among the production files"
        );
    }
    let found = path_lookups(
        lines
            .iter()
            .filter(|line| walks.contains(&line.file.as_str())),
    );
    assert!(
        found.is_empty(),
        "a protected walk looks a path up: {found:#?}"
    );

    // `shadow_note`, from its signature to the next top-level item.
    let route: Vec<&Line> = lines
        .iter()
        .filter(|line| line.file == "crates/banager-core/src/adapters/standalone/route.rs")
        .collect();
    let start = route
        .iter()
        .position(|line| line.text.starts_with("pub fn shadow_note("))
        .expect("route.rs has shadow_note");
    let end = route[start + 1..]
        .iter()
        .position(|line| line.text == "}")
        .map(|at| start + 1 + at)
        .expect("shadow_note ends");
    let found = path_lookups(route[start..=end].iter().copied());
    assert!(found.is_empty(), "shadow_note looks a path up: {found:#?}");
}

#[test]
fn test_only_the_runner_and_the_open_ollama_button_start_a_process() {
    let found = holding(
        production_lines().iter(),
        &[
            "Command::new",
            "process::Command",
            "posix_spawn",
            "libc::fork",
            "libc::exec",
            "libc::system",
            "NSTask",
            "launchApplication",
            "openApplication",
            "openURL",
        ],
    );
    let allowed = |entry: &String| {
        entry.starts_with("crates/banager-core/src/runner/real.rs:")
            || (entry.starts_with("src-tauri/src/ipc.rs:")
                && entry.contains("std::process::Command::new(program)"))
    };
    let others: Vec<&String> = found.iter().filter(|entry| !allowed(entry)).collect();
    assert!(
        others.is_empty(),
        "a process started elsewhere: {others:#?}"
    );
    // And the runner does start them: the check above is not vacuous.
    assert!(found
        .iter()
        .any(|entry| entry.starts_with("crates/banager-core/src/runner/real.rs:")));
}

#[test]
fn test_nothing_writes_a_file_but_banagers_own_two_and_the_move_to_the_trash() {
    let found = holding(
        production_lines().iter(),
        &[
            "fs::write",
            "File::create",
            "create_dir",
            "fs::rename",
            "remove_file",
            "remove_dir",
            "fs::copy",
            "set_permissions(",
            "hard_link",
            "fs::symlink(",
            ".write(true)",
            ".create(true)",
            ".create_new(true)",
            ".append(true)",
            ".truncate(true)",
            "O_CREAT",
            "O_WRONLY",
            "O_RDWR",
            "O_TRUNC",
            "O_APPEND",
            "libc::unlink",
            "libc::rmdir",
            "libc::mkdir",
            "libc::rename",
            "libc::symlink",
            "libc::link(",
            "libc::chmod",
            "libc::truncate",
            "trashItemAtURL",
            "removeItemAt",
            "moveItemAt",
            "copyItemAt",
            "writeToFile",
            "writeToURL",
            "createDirectoryAt",
            "createFileAtPath",
        ],
    );
    // Each allowed write, by file: what it writes.
    let allowed: [(&str, &[&str]); 3] = [
        // settings.json: the folder made if missing, a temp file beside
        // it renamed into place.
        (
            "crates/banager-core/src/settings.rs:",
            &[
                "create_dir_all(",
                "fs::write(&tmp_path",
                "fs::rename(&tmp_path",
            ],
        ),
        // history.json, the same way, the temp file removed when the
        // rename failed.
        (
            "crates/banager-core/src/history/mod.rs:",
            &[
                "create_dir_all(",
                "fs::write(&tmp_path",
                "fs::rename(&tmp_path",
                "remove_file(&tmp_path",
            ],
        ),
        // A previewed path moved to the Trash.
        (
            "crates/banager-core/src/trash/real.rs:",
            &["trashItemAtURL"],
        ),
    ];
    let others: Vec<&String> = found
        .iter()
        .filter(|entry| {
            !allowed.iter().any(|(file, calls)| {
                entry.starts_with(file) && calls.iter().any(|call| entry.contains(call))
            })
        })
        .collect();
    assert!(
        others.is_empty(),
        "a write outside Banager's own files: {others:#?}"
    );
    for (file, calls) in allowed {
        for call in calls {
            assert!(
                found
                    .iter()
                    .any(|entry| entry.starts_with(file) && entry.contains(call)),
                "{file} no longer has {call}: drop it from the list"
            );
        }
    }
}

/// The statement `lines[index]` is in: from the line after the last one
/// before it in its file that ends a statement or opens or closes a
/// block, to the first one from it that holds a `;`. When it passes a
/// `flags` it built just before, that `let flags` statement too.
fn statement(lines: &[Line], index: usize) -> String {
    let file = &lines[index].file;
    let ends = |line: &Line| {
        let text = line.text.trim_end();
        text.ends_with(';') || text.ends_with('{') || text.ends_with('}')
    };
    let mut first = index;
    while first > 0 && lines[first - 1].file == *file && !ends(&lines[first - 1]) {
        first -= 1;
    }
    let mut last = index;
    while !lines[last].text.contains(';') && last + 1 < lines.len() && lines[last + 1].file == *file
    {
        last += 1;
    }
    let mut text: Vec<&str> = lines[first..=last]
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    if text.iter().any(|line| line.contains(", flags)")) {
        let built = (0..first)
            .rev()
            .take_while(|at| lines[*at].file == *file)
            .find(|at| lines[*at].text.trim_start().starts_with("let flags ="));
        if let Some(at) = built {
            let end = (at..first)
                .find(|end| lines[*end].text.contains(';'))
                .unwrap_or(at);
            text.extend(lines[at..=end].iter().map(|line| line.text.as_str()));
        }
    }
    text.join("\n")
}

#[test]
fn test_every_file_opened_is_opened_without_waiting_or_as_a_folder() {
    let lines = production_lines();
    // Each open call, and the flags in its own statement: a second, plain
    // open beside a guarded one is not covered by the other's flags.
    let mut unguarded = Vec::new();
    let mut opens = 0;
    for (index, line) in lines.iter().enumerate() {
        let open = ["OpenOptions::new()", "libc::open(", "libc::openat("]
            .iter()
            .any(|call| line.text.contains(call));
        if !open {
            continue;
        }
        opens += 1;
        let flagged = ["O_NONBLOCK", "O_DIRECTORY", "SEARCH"]
            .iter()
            .any(|flag| statement(&lines, index).contains(flag));
        if !flagged {
            unguarded.push(format!(
                "{}:{}: {}",
                line.file,
                line.number,
                line.text.trim()
            ));
        }
    }
    assert!(
        unguarded.is_empty(),
        "an open that would wait on a named pipe: {unguarded:#?}"
    );
    assert!(opens >= 6, "the opens were found: {opens}");

    // The plain reads that remain, each by its text, with why it cannot
    // wait: Banager's own settings.json and history.json, in its own
    // Application Support folder, read as it starts, never in a refresh.
    let plain = holding(
        lines.iter(),
        &["File::open(", "fs::read(", "fs::read_to_string("],
    );
    let allowed = [
        (
            "crates/banager-core/src/settings.rs:",
            "match std::fs::read(path) {",
        ),
        (
            "crates/banager-core/src/history/mod.rs:",
            "let Ok(bytes) = std::fs::read(path) else {",
        ),
    ];
    let others: Vec<&String> = plain
        .iter()
        .filter(|entry| {
            !allowed
                .iter()
                .any(|(file, read)| entry.starts_with(file) && entry.ends_with(read))
        })
        .collect();
    assert!(
        others.is_empty(),
        "a read that can wait on a named pipe: {others:#?}"
    );
    for (file, read) in allowed {
        let count = plain
            .iter()
            .filter(|entry| entry.starts_with(file) && entry.ends_with(read))
            .count();
        assert_eq!(count, 1, "{file} reads `{read}` once: drop or fix it here");
    }
}

#[test]
fn test_an_opens_flags_are_read_from_its_own_statement() {
    let line = |number: usize, text: &str| Line {
        file: "f.rs".to_string(),
        number,
        text: text.to_string(),
    };
    // A plain open just after a guarded one.
    let lines = [
        line(1, "fn f() {"),
        line(2, "    let a = std::fs::OpenOptions::new()"),
        line(3, "        .custom_flags(libc::O_NONBLOCK)"),
        line(4, "        .open(p)?;"),
        line(
            5,
            "    let b = std::fs::OpenOptions::new().read(true).open(q)?;",
        ),
        line(6, "}"),
    ];
    assert!(statement(&lines, 1).contains("O_NONBLOCK"));
    assert!(!statement(&lines, 4).contains("O_NONBLOCK"));
    // Flags built before the call that passes them.
    let lines = [
        line(1, "    let flags = if list {"),
        line(2, "        libc::O_RDONLY | libc::O_DIRECTORY"),
        line(3, "    } else {"),
        line(4, "        SEARCH"),
        line(5, "    } | libc::O_NOFOLLOW;"),
        line(
            6,
            "    let fd = unsafe { libc::openat(dir, c.as_ptr(), flags) };",
        ),
    ];
    assert!(statement(&lines, 5).contains("O_DIRECTORY"));
}
