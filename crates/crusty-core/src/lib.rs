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

pub mod config;
pub mod download;
pub mod engine;
pub mod model;
#[cfg(feature = "mpris")]
pub mod mpris;
pub mod persistence;
pub mod queue;

pub use download::{DownloadManager, DownloadResult};
pub use engine::{AudioCommand, AudioEngine, AudioEvent, EngineController, PlayerSnapshot};
pub use model::{PersistedQueue, PlayerState, Track};
pub use persistence::{PersistenceService, PlaybackState};
pub use queue::Queue;
