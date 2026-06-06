//! Commands sent from any consumer (TUI, GUI, MPRIS) into the audio engine
//! thread. These are fire-and-forget messages delivered over a
//! `crossbeam_channel` whose `Sender` is `Send + Sync`.

use std::path::PathBuf;

/// A control message for the audio engine thread.
///
/// All variants are non-blocking from the caller's perspective: the typed
/// helper methods on [`crate::AudioEngine`] enqueue these and return
/// immediately. The engine thread applies them in order.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Load and start playing the file at `path`.
    ///
    /// `duration_secs` is the known track length (from metadata); when `> 0.0`
    /// it is used for the progress bar, otherwise duration is treated as `0.0`.
    Play {
        /// Path to the decoded/cached audio file on disk.
        path: PathBuf,
        /// Display title for the now-playing snapshot.
        title: String,
        /// Artist/uploader for the now-playing snapshot (may be empty).
        artist: String,
        /// Known duration in seconds (`<= 0.0` means "unknown").
        duration_secs: f64,
    },
    /// Pause playback (no-op unless currently playing).
    Pause,
    /// Resume playback (no-op unless currently paused).
    Resume,
    /// Toggle between play and pause (no-op when stopped).
    TogglePause,
    /// Stop playback and clear the queued audio.
    Stop,
    /// Seek to an absolute position in seconds (clamped to `[0, duration]`).
    SeekTo(f64),
    /// Seek relative to the current position in seconds (may be negative).
    SeekRelative(f64),
    /// Set the volume on a `0..=100` scale.
    SetVolume(u32),
    /// Stop the engine thread and release the audio device.
    Shutdown,
}
