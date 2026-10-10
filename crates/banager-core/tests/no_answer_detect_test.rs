//! Every package manager whose `detect` turns a failed `--version` into
//! `Unavailable::NotResponding` says why (`InstanceStatus::no_answer`,
//! `runner::no_answer::of`): it ran out of time, it could not start, or it
//! ran and ended with an error. npm's own two commands are tested in
//! adapters/npm.rs, Homebrew's in adapters/brew/mod.rs.
//!
//! Each source is found on a `PATH` of one temporary folder holding a file
//! of its name, and asked through a `MockRunner`: nothing on this Mac runs.

use banager_core::adapters::cargo::CargoAdapter;
use banager_core::adapters::pip::PipAdapter;
use banager_core::adapters::pipx::PipxAdapter;
use banager_core::adapters::uv::UvAdapter;
use banager_core::http::MockHttpClient;
use banager_core::model::{ManagerInstance, NoAnswerKind, Unavailable};
use banager_core::runner::{CommandOutput, HostEnv, MockRunner};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn folder_with(tag: &str, program: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "banager-no-answer-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("a folder for the program");
    let exe = bin.join(program);
    std::fs::write(&exe, b"#!/bin/sh\n").expect("the program");
    (dir, exe)
}

fn env_of(dir: &Path) -> HostEnv {
    HostEnv {
        path_dirs: vec![dir.join("bin")],
        home: dir.to_path_buf(),
        euid: 501,
        cargo_home: Some(dir.to_path_buf()),
        rustup_home: None,
        zdotdir: None,
        ollama_host: None,
    }
}

fn said(exit_code: Option<i32>, stderr: &str, timed_out: bool) -> CommandOutput {
    CommandOutput {
        exit_code,
        stdout: String::new(),
        stderr: stderr.to_string(),
        timed_out,
        cancelled: false,
        stderr_cause: Default::default(),
    }
}

fn reason(instances: &[ManagerInstance]) -> (Option<Unavailable>, Option<NoAnswerKind>) {
    assert_eq!(instances.len(), 1, "{instances:?}");
    let status = &instances[0].status;
    (
        status.unavailable,
        status.no_answer.as_ref().map(|why| why.kind),
    )
}

const NOT_RESPONDING: Option<Unavailable> = Some(Unavailable::NotResponding);

#[tokio::test]
async fn test_pipx_that_could_not_start_says_so() {
    // Homebrew's pipx starts with its Python's path; that Python gone, the
    // shell's "not found" is 127.
    let (dir, exe) = folder_with("pipx", "pipx");
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![exe.to_str().unwrap(), "--version"],
        said(
            Some(127),
            "bad interpreter: No such file or directory\n",
            false,
        ),
    );
    let instances = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()))
        .detect(&env_of(&dir))
        .await;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        reason(&instances),
        (NOT_RESPONDING, Some(NoAnswerKind::CouldNotStart))
    );
}

#[tokio::test]
async fn test_uv_that_ran_and_failed_says_so() {
    let (dir, exe) = folder_with("uv", "uv");
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![exe.to_str().unwrap(), "--version"],
        said(Some(2), "error: failed to read config\n", false),
    );
    let instances = UvAdapter::new(runner).detect(&env_of(&dir)).await;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        reason(&instances),
        (NOT_RESPONDING, Some(NoAnswerKind::ExitedWithError))
    );
}

#[tokio::test]
async fn test_cargo_that_ran_out_of_time_says_so() {
    let (dir, exe) = folder_with("cargo", "cargo");
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![exe.to_str().unwrap(), "--version"],
        said(None, "", true),
    );
    let instances = CargoAdapter::new(runner, Arc::new(MockHttpClient::new()))
        .detect(&env_of(&dir))
        .await;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        reason(&instances),
        (NOT_RESPONDING, Some(NoAnswerKind::TimedOut))
    );
}

#[tokio::test]
async fn test_a_broken_pip_says_why_and_a_python_with_no_pip_needs_no_reason() {
    let (dir, exe) = folder_with("pip", "python3.13");
    let python = exe.to_str().unwrap();
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![python, "-m", "pip", "--version"],
        said(
            Some(1),
            "Traceback (most recent call last):\nImportError: cannot import name 'x'\n",
            false,
        ),
    );
    let instances = PipAdapter::new(runner).detect(&env_of(&dir)).await;
    assert_eq!(
        reason(&instances),
        (NOT_RESPONDING, Some(NoAnswerKind::ExitedWithError))
    );

    // `NoPip` is its own reason, and nothing more is said of it.
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![python, "-m", "pip", "--version"],
        said(Some(1), &format!("{python}: No module named pip\n"), false),
    );
    let instances = PipAdapter::new(runner).detect(&env_of(&dir)).await;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(reason(&instances), (Some(Unavailable::NoPip), None));
}
