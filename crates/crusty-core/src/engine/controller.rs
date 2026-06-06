//! A cheap, cloneable control handle to the audio engine.
//!
//! [`AudioEngine`](crate::AudioEngine) owns the engine thread's `JoinHandle` and
//! is therefore deliberately **not** `Clone` (joining must happen exactly once).
//! Spawned tasks that only need to *drive* and *observe* the engine — e.g. the
//! MPRIS bridge, or Tauri command handlers — take an [`EngineController`]
//! instead: it holds only the command sender and a snapshot receiver, is
//! `Clone + Send + Sync + 'static`, and never touches the thread lifecycle.

use crossbeam_channel::Sender;
use tokio::sync::watch;

use super::command::AudioCommand;
use super::snapshot::PlayerSnapshot;

/// A `Clone + Send + Sync` control/observation handle to the engine.
///
/// Obtain one with [`AudioEngine::controller`](crate::AudioEngine::controller).
/// Commands are fire-and-forget; if the engine thread has gone away the sends
/// are silently dropped (the handle is best-effort by contract).
#[derive(Clone)]
pub struct EngineController {
    cmd_tx: Sender<AudioCommand>,
    snapshot_rx: watch::Receiver<PlayerSnapshot>,
}

impl EngineController {
    /// Construct from the engine's channel ends. Crate-internal: produced by
    /// [`AudioEngine::controller`](crate::AudioEngine::controller).
    pub(super) fn new(
        cmd_tx: Sender<AudioCommand>,
        snapshot_rx: watch::Receiver<PlayerSnapshot>,
    ) -> Self {
        Self { cmd_tx, snapshot_rx }
    }

    /// Enqueue any [`AudioCommand`] (non-blocking; errors ignored).
    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Load and start playing the file at `path`.
    pub fn play(
        &self,
        path: impl Into<std::path::PathBuf>,
        title: impl Into<String>,
        artist: impl Into<String>,
        duration_secs: f64,
    ) {
        self.send(AudioCommand::Play {
            path: path.into(),
            title: title.into(),
            artist: artist.into(),
            duration_secs,
        });
    }

    /// Pause playback.
    pub fn pause(&self) {
        self.send(AudioCommand::Pause);
    }

    /// Resume playback.
    pub fn resume(&self) {
        self.send(AudioCommand::Resume);
    }

    /// Toggle between play and pause.
    pub fn toggle_pause(&self) {
        self.send(AudioCommand::TogglePause);
    }

    /// Stop playback.
    pub fn stop(&self) {
        self.send(AudioCommand::Stop);
    }

    /// Seek to an absolute position in seconds.
    pub fn seek_to(&self, secs: f64) {
        self.send(AudioCommand::SeekTo(secs));
    }

    /// Seek relative to the current position in seconds.
    pub fn seek_relative(&self, secs: f64) {
        self.send(AudioCommand::SeekRelative(secs));
    }

    /// Set the volume on a `0..=100` scale.
    pub fn set_volume(&self, v: u32) {
        self.send(AudioCommand::SetVolume(v));
    }

    /// Return the most recent [`PlayerSnapshot`] (cheap clone).
    #[must_use]
    pub fn snapshot(&self) -> PlayerSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    /// Obtain a `watch::Receiver` for push-style snapshot updates.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<PlayerSnapshot> {
        self.snapshot_rx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_controller_is_clone_send_sync_static() {
        fn _assert<T: Clone + Send + Sync + 'static>() {}
        _assert::<EngineController>();
    }
}
