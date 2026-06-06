//! Outbound state from the audio engine: a coalesced [`PlayerSnapshot`]
//! (published via `tokio::sync::watch`) and discrete one-shot [`AudioEvent`]s
//! (published via `tokio::sync::broadcast`).

use crate::PlayerState;

/// A point-in-time view of the engine's playback state.
///
/// Published on every engine tick (~150ms) and after every command via a
/// `watch` channel. Consumers read the latest value with `borrow()` (sync, no
/// runtime needed) or await `changed()` for push-style updates.
#[derive(Debug, Clone)]
pub struct PlayerSnapshot {
    /// Current playback state.
    pub state: PlayerState,
    /// Current playback position in seconds (wall-clock derived).
    pub position_secs: f64,
    /// Total track duration in seconds (`0.0` when unknown).
    pub duration_secs: f64,
    /// Current volume on a `0..=100` scale.
    pub volume: u32,
    /// Title of the currently loaded track (empty when none).
    pub title: String,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            state: PlayerState::Stopped,
            position_secs: 0.0,
            duration_secs: 0.0,
            volume: 100,
            title: String::new(),
        }
    }
}

/// A discrete, one-shot event from the engine.
///
/// Unlike [`PlayerSnapshot`] (which re-publishes continuously), these fire
/// exactly once per occurrence and are delivered to every active subscriber.
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// The current track reached its natural end (emitted once per track).
    TrackEnded,
    /// A track failed to open/decode/append; carries a human-readable message.
    LoadError(String),
    /// No audio output device is available (headless mode).
    DeviceUnavailable,
}
