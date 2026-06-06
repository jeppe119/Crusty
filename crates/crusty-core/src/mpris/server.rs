//! The MPRIS2 D-Bus bridge: implements `org.mpris.MediaPlayer2` +
//! `org.mpris.MediaPlayer2.Player` on top of an [`EngineController`], and emits
//! `PropertiesChanged`/`Seeked` as engine state evolves.
//!
//! Serves Waybar's `mpris` module, Quickshell's `Quickshell.Services.Mpris`, and
//! `playerctl` from one bus name: `org.mpris.MediaPlayer2.crusty`.

use mpris_server::zbus::{self, fdo};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, Property, RootInterface, Server, Signal,
    Time, TrackId, Volume,
};
use tokio::sync::mpsc;

use crate::EngineController;

use super::mapping;

/// Actions that the engine cannot satisfy alone — queue navigation and
/// "start playing when stopped" live in the *host* (the TUI's queue logic, or
/// the GUI's). The bridge forwards these so MPRIS `Next`/`Previous`/`Play`
/// behave identically to the host's own keybindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisAction {
    /// Toggle pause, or start the current/first track when stopped.
    PlayPause,
    /// Explicit play (same host handling as `PlayPause`).
    Play,
    /// Stop playback.
    Stop,
    /// Advance to the next queued track.
    Next,
    /// Go back to the previous track.
    Previous,
    /// Quit the application.
    Quit,
}

/// The D-Bus interface implementation. Holds a cloneable engine controller for
/// direct state/volume/seek control, and an action sender for host-authoritative
/// commands (next/previous/play-when-stopped/quit).
pub struct CrustyMpris {
    engine: EngineController,
    actions: mpsc::UnboundedSender<MprisAction>,
    /// Whether this host can `raise` (bring a window to front). The TUI cannot;
    /// a GUI sets this true and handles `raise` via the action channel.
    can_raise: bool,
}

impl CrustyMpris {
    /// Create the bridge implementation.
    pub fn new(
        engine: EngineController,
        actions: mpsc::UnboundedSender<MprisAction>,
        can_raise: bool,
    ) -> Self {
        Self {
            engine,
            actions,
            can_raise,
        }
    }

    fn act(&self, action: MprisAction) {
        let _ = self.actions.send(action);
    }
}

impl RootInterface for CrustyMpris {
    async fn identity(&self) -> fdo::Result<String> {
        Ok("Crusty".into())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("crusty".into())
    }

    async fn raise(&self) -> fdo::Result<()> {
        if self.can_raise {
            // Reuse the action channel; the GUI host interprets this. (No
            // dedicated Raise action yet — hosts may map Quit/None as needed.)
        }
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        self.act(MprisAction::Quit);
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(self.can_raise)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> zbus::Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}

impl mpris_server::PlayerInterface for CrustyMpris {
    // ── Control methods ─────────────────────────────────────────────────────
    async fn next(&self) -> fdo::Result<()> {
        self.act(MprisAction::Next);
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        self.act(MprisAction::Previous);
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        self.engine.pause();
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        self.act(MprisAction::PlayPause);
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        self.act(MprisAction::Stop);
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        self.act(MprisAction::Play);
        Ok(())
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        self.engine.seek_relative(mapping::time_to_secs(offset));
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        // Accept any trackid: the engine has a single current track, and stale
        // positions are harmless (clamped). Seek absolute.
        self.engine.seek_to(mapping::time_to_secs(position));
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    // ── Property getters ────────────────────────────────────────────────────
    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(mapping::playback_status(self.engine.snapshot().state))
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> zbus::Result<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> zbus::Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_shuffle(&self, _shuffle: bool) -> zbus::Result<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        // track_no 0 is acceptable for a one-off query; the diff loop keeps the
        // authoritative per-track id in PropertiesChanged emissions.
        Ok(mapping::build_metadata(&self.engine.snapshot(), 0))
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(mapping::volume_to_mpris(self.engine.snapshot().volume))
    }

    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        self.engine.set_volume(mapping::volume_from_mpris(volume));
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        Ok(mapping::secs_to_time(self.engine.snapshot().position_secs))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        // Best-effort: the engine falls back to reload-and-seek-forward when a
        // decoder rejects a backward seek (it never panics).
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

/// Run the MPRIS server until the engine snapshot channel closes.
///
/// Registers `org.mpris.MediaPlayer2.crusty` on the session bus, then watches the
/// engine snapshot and emits `PropertiesChanged` (PlaybackStatus / Metadata /
/// Volume) only when the relevant fields change — avoiding signal spam at the
/// engine's ~150ms tick. Returns `Err` if the session bus is unavailable; the
/// caller is expected to log-and-ignore so the app still runs headless.
pub async fn serve_mpris(
    engine: EngineController,
    actions: mpsc::UnboundedSender<MprisAction>,
    can_raise: bool,
) -> zbus::Result<()> {
    let imp = CrustyMpris::new(engine.clone(), actions, can_raise);
    let server = Server::new("crusty", imp).await?;

    let mut rx = engine.subscribe();

    // Track the last-emitted values to diff against.
    let mut last = engine.snapshot();
    let mut track_no: u64 = 0;

    loop {
        // Wait for the next snapshot change; exit cleanly when the engine drops.
        if rx.changed().await.is_err() {
            break;
        }
        let snap = rx.borrow_and_update().clone();

        let mut changed: Vec<Property> = Vec::new();

        if mapping::playback_status(snap.state) != mapping::playback_status(last.state) {
            changed.push(Property::PlaybackStatus(mapping::playback_status(snap.state)));
        }

        if snap.volume != last.volume {
            changed.push(Property::Volume(mapping::volume_to_mpris(snap.volume)));
        }

        // Title change ⇒ new track ⇒ bump trackid + emit Metadata.
        let new_track_no = mapping::next_track_no(&last.title, &snap.title, track_no);
        if new_track_no != track_no
            || snap.title != last.title
            || (snap.duration_secs - last.duration_secs).abs() > f64::EPSILON
            || snap.artist != last.artist
        {
            track_no = new_track_no;
            changed.push(Property::Metadata(mapping::build_metadata(&snap, track_no)));
        }

        if !changed.is_empty() {
            // Ignore emit errors (bus hiccup shouldn't kill the loop).
            let _ = server.properties_changed(changed).await;
        }

        last = snap;
    }

    Ok(())
}

/// Emit a `Seeked` signal at the given absolute position. Hosts call this after
/// applying an MPRIS-initiated seek so position-interpolating clients (e.g.
/// Quickshell) resync immediately.
pub async fn emit_seeked<T>(server: &Server<T>, position_secs: f64) -> zbus::Result<()>
where
    T: RootInterface + mpris_server::PlayerInterface + 'static,
{
    server
        .emit(Signal::Seeked {
            position: mapping::secs_to_time(position_secs),
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crusty_mpris_is_send_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<CrustyMpris>();
    }

    #[test]
    fn mpris_action_is_copy() {
        let a = MprisAction::Next;
        let b = a; // Copy
        assert_eq!(a, b);
    }
}
