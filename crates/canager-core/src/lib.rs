//! canager-core: pure Rust library with the Homebrew adapter and operation
//! engine. This crate must never depend on `tauri` — see
//! `docs/superpowers/specs/2026-09-17-canager-design.md` section 3.

pub mod adapters;
pub mod events;
pub mod http;
pub mod model;
pub mod ops;
pub mod runner;
pub mod session;
pub mod settings;
/// Fixture constructors shared by this crate's tests, its `tests/`
/// integration tests and the Tauri shell's tests. See the module doc for
/// why it is public rather than `#[cfg(test)]`.
pub mod testing;

pub use events::*;
pub use model::*;
