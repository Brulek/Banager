//! canager-core: pure Rust library with the Homebrew adapter and operation
//! engine. This crate must never depend on `tauri` — see
//! `docs/superpowers/specs/2026-09-17-canager-design.md` section 3.

pub mod adapters;
pub mod events;
pub mod model;
pub mod ops;
pub mod runner;
pub mod session;
pub mod settings;

pub use events::*;
pub use model::*;
