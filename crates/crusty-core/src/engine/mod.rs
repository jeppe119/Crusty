//! Thread-backed audio engine.
//!
//! The public [`AudioEngine`] handle is `Send + Sync` and owns only channel
//! ends plus a thread join handle. The `!Send` rodio `Player` and the audio
//! device sink live exclusively on the engine's own `std::thread` and are never
//! exposed across the handle. This makes the engine safe to share between the
//! TUI, a future Tauri GUI, and an MPRIS handler.
//!
//! See `CRUSTY_CORE_EXTRACTION_PLAN.md` (Phase D) for the design rationale.

mod command;
mod handle;
mod snapshot;

pub use command::AudioCommand;
pub use handle::AudioEngine;
pub use snapshot::{AudioEvent, PlayerSnapshot};
