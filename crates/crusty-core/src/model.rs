//! Shared metadata and state types used across the player, queue, persistence,
//! and (later) the audio engine, MPRIS, and GUI layers.

/// The playback state of the audio engine.
///
/// Modelled as an enum rather than multiple booleans so the player can only ever
/// be in exactly one state at a time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerState {
    /// No audio loaded or playback has been stopped.
    Stopped,
    /// Audio is currently playing.
    Playing,
    /// Audio is loaded but temporarily paused.
    Paused,
    /// Audio is being loaded/decoded (transitional state).
    Loading,
}

/// A single music track with its metadata.
///
/// The serde representation is the persisted on-disk shape (history/queue JSON).
/// **Do not rename or remove fields** — add future fields with `#[serde(default)]`
/// to preserve backward compatibility with existing `~/.config` files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Track {
    /// Unique YouTube video identifier (e.g. `dQw4w9WgXcQ`).
    pub video_id: String,
    /// Display title of the track.
    pub title: String,
    /// Track length in seconds.
    pub duration: u64,
    /// Channel/uploader name (used as the "artist" in displays and MPRIS).
    pub uploader: String,
    /// Source URL (YouTube watch URL).
    pub url: String,
    /// Path to a pre-downloaded local file, if any.
    pub local_file: Option<String>,
}

impl Track {
    /// Creates a new [`Track`]. `local_file` starts as `None` (not yet downloaded).
    pub fn new(
        video_id: String,
        title: String,
        duration: u64,
        uploader: String,
        url: String,
    ) -> Self {
        Track {
            video_id,
            title,
            duration,
            uploader,
            url,
            local_file: None,
        }
    }
}

/// Serializable snapshot of the playback queue for persistence.
///
/// This is the on-disk shape of `queue.json`. **Do not rename fields.**
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedQueue {
    pub tracks: Vec<Track>,
    pub current_track: Option<Track>,
}
