//! Banager's engine: everything an operation does, with no window attached.
//!
//! Banager is a desktop app for people who do not write code, for looking
//! after the things they installed from a terminal — Homebrew, npm, pipx,
//! uv, pip, cargo, Ollama, and tools that come with their own installer
//! (Claude Code, Antigravity CLI, Grok Build, rustup). This crate is the
//! part that does the work: the [`model`] every source is described in,
//! the [`adapters`] that speak each source's command line, the [`runner`]
//! that spawns those commands and streams their output back line by line,
//! and the [`ops`] engine that turns a user's request into a plan, executes
//! it under locks and cancellation, and reports what actually happened.
//! Two read-only paths sit beside them: [`scan`], which programs in the
//! usual bin directories none of those sources put there, and [`icon`], the
//! icon Finder shows for the app a Homebrew cask installed, which macOS
//! draws for that cask's row. [`trash`] is the one place it changes a file
//! itself: macOS's own move-to-Trash, for a confirmed uninstall of a tool
//! that has no uninstall command.
//!
//! It must never depend on `tauri` (see
//! `docs/superpowers/specs/2026-09-17-banager-design.md` section 3). The
//! Tauri shell in `src-tauri/` is a thin IPC layer over this crate, and
//! keeping the dependency one-way is what lets the whole engine be tested
//! headlessly, which is most of this repository's test suite.
//!
//! # What it assumes: Unix, and in practice macOS
//!
//! This crate is written against POSIX and does not compile on Windows.
//! It puts every child in its own process group and stops that group with
//! `killpg` so a `brew` invocation's grandchildren stop with it
//! (`runner::real`), refuses to run a package manager as root via
//! `geteuid` (`adapters::brew`, `runner::path_env`), and asks `access(2)`
//! whether npm's global prefix is writable (`adapters::npm`). None of
//! those have a drop-in Windows equivalent — they need a different design,
//! not a shim.
//!
//! Banager v0.1 ships for macOS only: that is the platform it is built,
//! signed and tested on. Linux is Unix, so this crate compiles there and
//! the Linuxbrew path is already among Homebrew's candidates, but nothing
//! in the release is verified on it. **Windows and Linux are roadmap, not
//! bugs** — a build failure on Windows is this crate saying "not yet", and
//! the `compile_error!` below says so in one sentence rather than leaving
//! a contributor to work it out from a screenful of `libc` errors.

#[cfg(not(unix))]
compile_error!(
    "banager-core does not support this platform yet. It is written against POSIX \
     (process groups and killpg, geteuid, access(2)), which Windows has no drop-in \
     equivalent for. Banager v0.1 targets macOS; Windows support is on the roadmap, \
     so this is a feature that has not been written, not a build you need to fix. \
     See the crate documentation in crates/banager-core/src/lib.rs."
);

pub mod adapters;
/// The daily check's decision -- whether a tick starts a refresh round --
/// and the record of who asked for each round. Pure: the shell runs the
/// task and the round.
pub mod auto_check;
/// Which copy of each command runs when the user types its name in
/// Terminal, judged against `PATH` after each refresh's inventory --
/// read-only, behind a budget, like `scan`.
pub mod commands;
/// What the window's 「拷贝诊断信息」 needs and cannot read itself: macOS's
/// version, the chip, the `PATH` folders and each source's program, with
/// the home folder as `~` -- read-only, no command runs.
pub mod diagnostics;
pub mod events;
/// Which AI coding tool an installed artifact is a copy of, from a table
/// bundled into the binary (`data/ai-tools.json`).
pub mod families;
pub mod history;
pub mod http;
/// The icon Finder shows for a cask's app, drawn by macOS for the window
/// and remembered in memory -- read-only, behind a seam like `trash`.
pub mod icon;
/// What an uninstall leaves behind -- a tool's settings and data, Ollama's
/// models -- named in its preview, measured read-only, never deleted.
pub mod kept_data;
pub mod model;
/// The notification when a run of operations finishes, Settings'
/// 「操作完成时通知」: whether the page's report of a finished run posts
/// one, and which runs were reported. Pure: the shell reads the window's
/// focus, words it and posts.
pub mod notify_operations;
/// The update notification's decision -- whether the page's report of the
/// updates it offers posts one -- and the record of what was told. Pure:
/// the shell reads the window's focus and posts.
pub mod notify_updates;
pub mod ops;
/// The places no read-only walk ever enters -- the folders macOS asks the
/// user about first, and every other disk -- shared by `commands` and
/// `size`.
pub mod protected;
pub mod runner;
pub mod scan;
pub mod session;
pub mod settings;
/// How much disk each installed tool takes, measured read-only after each
/// refresh, outside the snapshot.
pub mod size;
/// Fixture constructors shared by this crate's tests, its `tests/`
/// integration tests and the Tauri shell's tests. See the module doc for
/// why it is public rather than `#[cfg(test)]`.
pub mod testing;
/// Moving an item to the Trash -- the one change Banager makes to a file
/// in its own process besides its settings, behind a seam like `runner`
/// and `http`.
pub mod trash;

pub use events::*;
pub use model::*;
