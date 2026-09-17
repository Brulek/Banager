//! canager-core: pure Rust library with the Homebrew adapter and operation
//! engine. This crate must never depend on `tauri` — see
//! `docs/superpowers/specs/2026-09-17-canager-design.md` section 3.

pub mod model;
pub mod events;

pub use model::*;
pub use events::*;
