//! Canager's engine: everything an operation does, with no window attached.
//!
//! Canager is a desktop app for people who do not write code, for looking
//! after the things they installed from a terminal — Homebrew, npm, pipx,
//! uv, pip, cargo, Ollama. This crate is the part that does the work: the
//! [`model`] every source is described in, the [`adapters`] that speak each
//! package manager's command line, the [`runner`] that spawns those
//! commands and streams their output back line by line, and the [`ops`]
//! engine that turns a user's request into a plan, executes it under locks
//! and cancellation, and reports what actually happened. [`scan`] is the
//! one read-only path beside them: which programs in the usual bin
//! directories none of those sources put there.
//!
//! It must never depend on `tauri` (see
//! `docs/superpowers/specs/2026-09-17-canager-design.md` section 3). The
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
//! Canager v0.1 ships for macOS only: that is the platform it is built,
//! signed and tested on. Linux is Unix, so this crate compiles there and
//! the Linuxbrew path is already among Homebrew's candidates, but nothing
//! in the release is verified on it. **Windows and Linux are roadmap, not
//! bugs** — a build failure on Windows is this crate saying "not yet", and
//! the `compile_error!` below says so in one sentence rather than leaving
//! a contributor to work it out from a screenful of `libc` errors.

#[cfg(not(unix))]
compile_error!(
    "canager-core does not support this platform yet. It is written against POSIX \
     (process groups and killpg, geteuid, access(2)), which Windows has no drop-in \
     equivalent for. Canager v0.1 targets macOS; Windows support is on the roadmap, \
     so this is a feature that has not been written, not a build you need to fix. \
     See the crate documentation in crates/canager-core/src/lib.rs."
);

pub mod adapters;
pub mod events;
pub mod http;
pub mod model;
pub mod ops;
pub mod runner;
pub mod scan;
pub mod session;
pub mod settings;
/// Fixture constructors shared by this crate's tests, its `tests/`
/// integration tests and the Tauri shell's tests. See the module doc for
/// why it is public rather than `#[cfg(test)]`.
pub mod testing;

pub use events::*;
pub use model::*;
