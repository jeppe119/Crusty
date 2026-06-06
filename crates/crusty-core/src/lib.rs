//! `crusty-core` — framework-agnostic core for the Crusty music player.
//!
//! Houses the shared, UI-independent logic consumed by both the terminal UI
//! (`crusty-tui`) and the future desktop GUI (`crusty-gui`):
//!
//! - audio engine (thread-backed, `Send + Sync` handle wrapping the `!Send` rodio player)
//! - playback queue
//! - download manager
//! - config + persisted state
//! - shared metadata types
//!
//! This crate must never depend on `ratatui`/`crossterm` (TUI) or `tauri` (GUI).
//!
//! Modules are populated incrementally per `CRUSTY_CORE_EXTRACTION_PLAN.md`.
