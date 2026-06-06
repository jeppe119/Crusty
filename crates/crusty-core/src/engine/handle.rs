//! The `Send + Sync` [`AudioEngine`] handle and the private engine-thread loop.
//!
//! The handle holds only:
//! - a `crossbeam_channel::Sender<AudioCommand>` (`Send + Sync`),
//! - a `tokio::sync::watch::Receiver<PlayerSnapshot>` (`Clone + Send + Sync`),
//! - a `tokio::sync::broadcast::Sender<AudioEvent>` (`Send + Sync`),
//! - an `Option<JoinHandle<()>>`.
//!
//! The `broadcast::Receiver` is intentionally **not** stored (it is not `Sync`);
//! consumers obtain their own via [`AudioEngine::subscribe_events`].

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::{RecvTimeoutError, Sender};
use rodio::{Decoder, Player};
use tokio::sync::{broadcast, watch};

use crate::PlayerState;

use super::command::AudioCommand;
use super::snapshot::{AudioEvent, PlayerSnapshot};

/// How long the engine thread waits for a command before ticking (refreshing
/// the snapshot + checking for a finished track).
const TICK: Duration = Duration::from_millis(150);

/// Capacity of the broadcast event channel. Events are rare, so this is ample.
const EVENT_CAPACITY: usize = 16;

/// The natural-finish guard (seconds). A track is only considered "ended" once
/// it has actually been playing for at least this long, preventing rapid
/// auto-advance during loading/buffering.
const FINISH_GUARD_SECS: f64 = 2.0;

// ============================================================================
// Pure helpers (no thread, no device) — deterministically unit-tested.
// ============================================================================

/// Compute the wall-clock playback position in seconds.
///
/// Mirrors the original `AudioPlayer::get_time_pos`: while paused, elapsed is
/// measured up to `pause_time`; otherwise up to `now`. The accumulated paused
/// duration is then subtracted. Saturating arithmetic guarantees a non-negative
/// result and never panics.
fn compute_position(
    start: Option<Instant>,
    pause_time: Option<Instant>,
    total_paused: Duration,
    now: Instant,
) -> f64 {
    let Some(start) = start else {
        return 0.0;
    };
    let elapsed = match pause_time {
        Some(pt) => pt.saturating_duration_since(start),
        None => now.saturating_duration_since(start),
    };
    elapsed.saturating_sub(total_paused).as_secs_f64()
}

/// Clamp an absolute seek target into the valid range.
///
/// Non-finite targets (`NaN`/`±inf`) and negative targets clamp to `0.0`. When
/// `duration > 0.0`, targets beyond the duration clamp to `duration`; when
/// duration is unknown (`<= 0.0`) no upper bound is applied.
fn clamp_seek(target: f64, duration: f64) -> f64 {
    if !target.is_finite() || target < 0.0 {
        0.0
    } else if duration > 0.0 && target > duration {
        duration
    } else {
        target
    }
}

/// Compute the `start_time` baseline for a seek to `target` seconds, accounting
/// for accumulated paused time.
///
/// Uses checked subtraction so it never panics: near system boot (monotonic
/// clock close to zero) or for very large targets the subtraction would
/// underflow `Instant`, in which case we fall back to "now" (position ~0).
fn seek_baseline(now: Instant, target: Duration, total_paused: Duration) -> Instant {
    now.checked_sub(target)
        .and_then(|t| t.checked_sub(total_paused))
        .unwrap_or(now)
}

/// Open and decode an audio file, returning a rodio decoder or an error string.
fn decode_from_file(path: &Path) -> Result<Decoder<std::fs::File>, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open audio file: {e}"))?;
    Decoder::new(file).map_err(|e| {
        format!("Audio decode failed: {e}. File may be corrupted or invalid format.")
    })
}

// ============================================================================
// Engine internal state (lives only on the engine thread).
// ============================================================================

/// Internal, thread-affine engine state. Owns the `!Send` rodio `Player`.
struct Engine {
    /// `None` in headless mode (no audio device).
    player: Option<Player>,
    state: PlayerState,
    volume: u32,
    duration: f64,
    current_title: String,
    current_artist: String,
    start_time: Option<Instant>,
    pause_time: Option<Instant>,
    total_paused_duration: Duration,
    current_file_path: Option<String>,
    /// Guards against emitting `TrackEnded` more than once per track.
    track_ended_emitted: bool,
    snapshot_tx: watch::Sender<PlayerSnapshot>,
    events_tx: broadcast::Sender<AudioEvent>,
}

impl Engine {
    /// Current wall-clock playback position in seconds.
    fn position(&self) -> f64 {
        compute_position(
            self.start_time,
            self.pause_time,
            self.total_paused_duration,
            Instant::now(),
        )
    }

    /// Port of `play_with_duration`: stop, decode (panic-guarded), append, and
    /// reset the wall-clock. On failure emit `LoadError` and return to Stopped.
    fn load_and_play(&mut self, path: PathBuf, title: String, artist: String, duration_secs: f64) {
        if self.player.is_none() {
            // Headless: nothing to play. Snapshot stays Stopped.
            return;
        }

        self.state = PlayerState::Loading;
        if let Some(p) = &self.player {
            p.stop();
        }

        let path_str = path.to_string_lossy().into_owned();

        // Decode inside catch_unwind — some decoders can panic on bad input.
        let decode_result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode_from_file(&path)));

        let outcome: Result<(), String> = match decode_result {
            Ok(Ok(decoder)) => {
                // SAFETY of unwrap: guarded by the early `is_none` return above.
                let player = self.player.as_ref().expect("player present");
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    player.append(decoder);
                })) {
                    Ok(()) => Ok(()),
                    Err(_) => Err("Audio append failed (decoder panicked)".to_string()),
                }
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err("Audio decode panicked".to_string()),
        };

        match outcome {
            Ok(()) => {
                self.state = PlayerState::Playing;
                self.current_title = title;
                self.current_artist = artist;
                self.duration = if duration_secs > 0.0 { duration_secs } else { 0.0 };
                self.start_time = Some(Instant::now());
                self.pause_time = None;
                self.total_paused_duration = Duration::ZERO;
                self.current_file_path = Some(path_str);
                self.track_ended_emitted = false;
            }
            Err(msg) => {
                self.state = PlayerState::Stopped;
                self.start_time = None;
                let _ = self.events_tx.send(AudioEvent::LoadError(msg));
            }
        }
    }

    /// Port of `seek` + `apply_seek`: native `try_seek`, then reload-and-seek
    /// forward as a fallback (covers backward-seek `SeekError`). Never panics.
    fn seek_to(&mut self, secs: f64) {
        let target = clamp_seek(secs, self.duration);
        let target_dur = Duration::from_secs_f64(target);

        let native_ok = self
            .player
            .as_ref()
            .is_some_and(|p| p.try_seek(target_dur).is_ok());

        if native_ok {
            self.start_time = Some(seek_baseline(
                Instant::now(),
                target_dur,
                self.total_paused_duration,
            ));
            return;
        }

        // Fallback: reload the file and seek forward from zero.
        if let Some(file_path) = self.current_file_path.clone() {
            let title = self.current_title.clone();
            let artist = self.current_artist.clone();
            let duration = self.duration;
            self.load_and_play(PathBuf::from(&file_path), title, artist, duration);
            if let Some(p) = &self.player {
                let _ = p.try_seek(target_dur);
            }
            self.start_time = Some(seek_baseline(
                Instant::now(),
                target_dur,
                self.total_paused_duration,
            ));
        }
        // Otherwise (headless / nothing loaded): no-op, no panic.
    }

    /// Seek relative to the current position, routed through [`Self::seek_to`].
    fn seek_relative(&mut self, secs: f64) {
        let target = self.position() + secs;
        self.seek_to(target);
    }

    /// Pause playback. No-op unless currently playing.
    fn pause(&mut self) {
        if self.state != PlayerState::Playing {
            return;
        }
        if let Some(p) = &self.player {
            p.pause();
        }
        self.pause_time = Some(Instant::now());
        self.state = PlayerState::Paused;
    }

    /// Resume playback. No-op unless currently paused.
    fn resume(&mut self) {
        if self.state != PlayerState::Paused {
            return;
        }
        if let Some(p) = &self.player {
            p.play();
        }
        if let Some(pt) = self.pause_time {
            self.total_paused_duration += Instant::now().saturating_duration_since(pt);
            self.pause_time = None;
        }
        self.state = PlayerState::Playing;
    }

    /// Toggle play/pause. No-op when stopped or loading.
    fn toggle(&mut self) {
        match self.state {
            PlayerState::Playing => self.pause(),
            PlayerState::Paused => self.resume(),
            _ => {}
        }
    }

    /// Stop playback and reset timing.
    fn stop(&mut self) {
        if let Some(p) = &self.player {
            p.stop();
        }
        self.start_time = None;
        self.pause_time = None;
        self.total_paused_duration = Duration::ZERO;
        self.state = PlayerState::Stopped;
    }

    /// Set volume on a `0..=100` scale.
    fn set_volume(&mut self, v: u32) {
        self.volume = v;
        if let Some(p) = &self.player {
            p.set_volume(v as f32 / 100.0);
        }
    }

    /// Port of `is_finished` + emit-once: when the sink drains during real
    /// playback (state Playing, started, past the 2s guard), emit `TrackEnded`
    /// exactly once and flip to Stopped.
    fn check_finished(&mut self) {
        if self.track_ended_emitted {
            return;
        }
        let finished = self.player.as_ref().is_some_and(|p| p.empty())
            && self.state == PlayerState::Playing
            && self.start_time.is_some()
            && self.position() >= FINISH_GUARD_SECS;

        if finished {
            let _ = self.events_tx.send(AudioEvent::TrackEnded);
            self.state = PlayerState::Stopped;
            self.track_ended_emitted = true;
        }
    }

    /// Build and publish the current snapshot (position computed live).
    fn publish_snapshot(&self) {
        let snapshot = PlayerSnapshot {
            state: self.state,
            position_secs: self.position(),
            duration_secs: self.duration,
            volume: self.volume,
            title: self.current_title.clone(),
            artist: self.current_artist.clone(),
        };
        self.snapshot_tx.send_replace(snapshot);
    }

    /// The engine thread main loop.
    fn run(mut self, cmd_rx: crossbeam_channel::Receiver<AudioCommand>) {
        loop {
            match cmd_rx.recv_timeout(TICK) {
                Ok(cmd) => match cmd {
                    AudioCommand::Play {
                        path,
                        title,
                        artist,
                        duration_secs,
                    } => self.load_and_play(path, title, artist, duration_secs),
                    AudioCommand::Pause => self.pause(),
                    AudioCommand::Resume => self.resume(),
                    AudioCommand::TogglePause => self.toggle(),
                    AudioCommand::Stop => self.stop(),
                    AudioCommand::SeekTo(s) => self.seek_to(s),
                    AudioCommand::SeekRelative(s) => self.seek_relative(s),
                    AudioCommand::SetVolume(v) => self.set_volume(v),
                    AudioCommand::Shutdown => break,
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }

            self.check_finished();
            self.publish_snapshot();
        }
        // Dropping `self.player` here stops audio output.
    }
}

// ============================================================================
// Public handle.
// ============================================================================

/// A `Send + Sync` handle to the thread-backed audio engine.
///
/// Construct with [`AudioEngine::new`] (spawns the engine thread which opens the
/// audio device on that thread). Send commands via the typed helpers or
/// [`AudioEngine::send`]; read state via [`AudioEngine::snapshot`] /
/// [`AudioEngine::subscribe`]; observe one-shot events via
/// [`AudioEngine::subscribe_events`].
pub struct AudioEngine {
    cmd_tx: Sender<AudioCommand>,
    snapshot_rx: watch::Receiver<PlayerSnapshot>,
    events_tx: broadcast::Sender<AudioEvent>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AudioEngine {
    /// Spawn the engine thread (which owns the rodio device + `Player`).
    ///
    /// In headless environments the device open fails; the engine then runs in
    /// headless mode, emitting a single `DeviceUnavailable` event and otherwise
    /// publishing `Stopped` snapshots.
    #[must_use]
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<AudioCommand>();
        let (snapshot_tx, snapshot_rx) = watch::channel(PlayerSnapshot::default());
        let (events_tx, _events_rx) = broadcast::channel::<AudioEvent>(EVENT_CAPACITY);

        let events_tx_thread = events_tx.clone();

        let thread = std::thread::Builder::new()
            .name("crusty-audio-engine".to_string())
            .spawn(move || {
                // Open the device ON this thread (rodio is thread-affine).
                let player = match rodio::DeviceSinkBuilder::open_default_sink() {
                    Ok(device_sink) => {
                        let (player, source) = Player::new();
                        device_sink.mixer().add(source);
                        // The device sink MUST outlive the Player. Leaking it
                        // once, here on the engine thread, is the established
                        // pattern (one engine per process).
                        std::mem::forget(device_sink);
                        player.set_volume(1.0);
                        Some(player)
                    }
                    Err(_) => {
                        let _ = events_tx_thread.send(AudioEvent::DeviceUnavailable);
                        None
                    }
                };

                let engine = Engine {
                    player,
                    state: PlayerState::Stopped,
                    volume: 100,
                    duration: 0.0,
                    current_title: String::new(),
                    current_artist: String::new(),
                    start_time: None,
                    pause_time: None,
                    total_paused_duration: Duration::ZERO,
                    current_file_path: None,
                    track_ended_emitted: false,
                    snapshot_tx,
                    events_tx: events_tx_thread,
                };
                engine.run(cmd_rx);
            })
            .expect("failed to spawn audio engine thread");

        Self {
            cmd_tx,
            snapshot_rx,
            events_tx,
            thread: Some(thread),
        }
    }

    /// Generic escape hatch: enqueue any [`AudioCommand`] (non-blocking).
    pub fn send(&self, cmd: AudioCommand) {
        // If the engine thread has gone away the send errors; ignore it — the
        // handle is fire-and-forget by contract.
        let _ = self.cmd_tx.send(cmd);
    }

    /// Load and start playing the file at `path`.
    pub fn play(
        &self,
        path: impl Into<PathBuf>,
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

    /// Obtain a cheap, cloneable [`EngineController`] for spawned tasks (MPRIS,
    /// Tauri commands) that need to drive and observe the engine without owning
    /// its thread lifecycle.
    #[must_use]
    pub fn controller(&self) -> super::controller::EngineController {
        super::controller::EngineController::new(self.cmd_tx.clone(), self.snapshot_rx.clone())
    }

    /// Obtain a `broadcast::Receiver` for one-shot [`AudioEvent`]s. Each
    /// consumer owns its own receiver.
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<AudioEvent> {
        self.events_tx.subscribe()
    }

    /// Stop the engine thread and join it. Blocks until the thread exits.
    pub fn shutdown(mut self) {
        let _ = self.cmd_tx.send(AudioCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // `Drop` runs next but finds `thread == None`, so it does nothing.
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        // Best-effort cleanup if `shutdown` was not called explicitly.
        if let Some(thread) = self.thread.take() {
            let _ = self.cmd_tx.send(AudioCommand::Shutdown);
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- compile-time Send + Sync assertion ----

    #[test]
    fn audio_engine_is_send_sync() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<AudioEngine>();
    }

    // ---- pure helper: clamp_seek ----

    #[test]
    fn clamp_seek_clamps_negative_to_zero() {
        assert_eq!(clamp_seek(-5.0, 100.0), 0.0);
    }

    #[test]
    fn clamp_seek_clamps_beyond_duration() {
        assert_eq!(clamp_seek(150.0, 100.0), 100.0);
    }

    #[test]
    fn clamp_seek_passes_through_in_range() {
        assert_eq!(clamp_seek(42.5, 100.0), 42.5);
    }

    #[test]
    fn clamp_seek_no_upper_bound_when_duration_unknown() {
        assert_eq!(clamp_seek(9999.0, 0.0), 9999.0);
        assert_eq!(clamp_seek(-1.0, 0.0), 0.0);
    }

    #[test]
    fn clamp_seek_rejects_non_finite() {
        // All non-finite inputs clamp to 0.0 (safe, never panics downstream).
        assert_eq!(clamp_seek(f64::NAN, 100.0), 0.0);
        assert_eq!(clamp_seek(f64::INFINITY, 100.0), 0.0);
        assert_eq!(clamp_seek(f64::INFINITY, 0.0), 0.0);
        assert_eq!(clamp_seek(f64::NEG_INFINITY, 100.0), 0.0);
    }

    // ---- pure helper: seek_baseline (never panics on underflow) ----

    #[test]
    fn seek_baseline_normal_case() {
        let now = Instant::now();
        // A modest target well within the monotonic clock's range.
        let base = seek_baseline(now, Duration::from_secs(5), Duration::ZERO);
        let pos = compute_position(Some(base), None, Duration::ZERO, now);
        assert!((pos - 5.0).abs() < 0.01, "pos = {pos}");
    }

    #[test]
    fn seek_baseline_huge_target_does_not_panic() {
        let now = Instant::now();
        // A target far exceeding the monotonic clock's elapsed time would
        // underflow `Instant - Duration`; seek_baseline must clamp to `now`.
        let base = seek_baseline(now, Duration::from_secs(u32::MAX as u64), Duration::ZERO);
        // Falling back to `now` yields position ~0, never a panic.
        let pos = compute_position(Some(base), None, Duration::ZERO, now);
        assert!(pos >= 0.0);
    }

    // ---- pure helper: compute_position ----

    #[test]
    fn compute_position_zero_without_start() {
        assert_eq!(
            compute_position(None, None, Duration::ZERO, Instant::now()),
            0.0
        );
    }

    #[test]
    fn compute_position_elapsed_when_playing() {
        let now = Instant::now();
        let start = now - Duration::from_secs(10);
        let pos = compute_position(Some(start), None, Duration::ZERO, now);
        assert!((pos - 10.0).abs() < 0.01, "pos = {pos}");
    }

    #[test]
    fn compute_position_uses_pause_time_when_paused() {
        let now = Instant::now();
        let start = now - Duration::from_secs(30);
        let pause = now - Duration::from_secs(20); // paused at the 10s mark
        let pos = compute_position(Some(start), Some(pause), Duration::ZERO, now);
        assert!((pos - 10.0).abs() < 0.01, "pos = {pos}");
    }

    #[test]
    fn compute_position_subtracts_total_paused() {
        let now = Instant::now();
        let start = now - Duration::from_secs(30);
        let pos = compute_position(Some(start), None, Duration::from_secs(5), now);
        assert!((pos - 25.0).abs() < 0.01, "pos = {pos}");
    }

    #[test]
    fn compute_position_saturates_when_paused_exceeds_elapsed() {
        let now = Instant::now();
        let start = now - Duration::from_secs(5);
        let pos = compute_position(Some(start), None, Duration::from_secs(10), now);
        assert_eq!(pos, 0.0);
    }

    // ---- integration: real engine thread (headless-safe) ----

    /// Poll a watch receiver until `pred` holds or the deadline passes.
    fn wait_until<F>(rx: &watch::Receiver<PlayerSnapshot>, timeout: Duration, mut pred: F) -> bool
    where
        F: FnMut(&PlayerSnapshot) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if pred(&rx.borrow()) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn set_volume_roundtrips_to_snapshot() {
        let engine = AudioEngine::new();
        engine.set_volume(50);
        let rx = engine.subscribe();
        assert!(
            wait_until(&rx, Duration::from_secs(2), |s| s.volume == 50),
            "volume did not propagate to snapshot"
        );
        engine.shutdown();
    }

    #[test]
    fn pause_resume_toggle_are_noops_when_stopped() {
        let engine = AudioEngine::new();
        engine.pause();
        engine.resume();
        engine.toggle_pause();
        let rx = engine.subscribe();
        // State must remain Stopped throughout a generous window.
        let drifted = wait_until(&rx, Duration::from_millis(400), |s| {
            s.state != PlayerState::Stopped
        });
        assert!(!drifted, "state changed away from Stopped on a no-op");
        engine.shutdown();
    }

    #[test]
    fn seek_while_stopped_does_not_panic_and_clamps() {
        let engine = AudioEngine::new();
        engine.seek_relative(-50.0);
        engine.seek_to(-10.0);
        engine.seek_to(99_999.0);
        let rx = engine.subscribe();
        // Snapshot remains reachable and position never goes negative.
        assert!(wait_until(&rx, Duration::from_millis(400), |s| {
            s.position_secs >= 0.0
        }));
        engine.shutdown();
    }

    #[test]
    fn huge_seek_does_not_kill_engine_thread() {
        // Regression: `Instant - Duration` underflow on a large seek target with
        // unknown duration must NOT panic the engine thread. The engine must keep
        // processing commands + publishing snapshots afterwards.
        let engine = AudioEngine::new();
        engine.seek_to(1_000_000_000.0); // ~31 years; would underflow naive arithmetic
        engine.seek_to(f64::NAN); // also must not panic
        engine.set_volume(77);
        let rx = engine.subscribe();
        assert!(
            wait_until(&rx, Duration::from_secs(2), |s| s.volume == 77),
            "engine thread died after a huge/NaN seek (volume command never applied)"
        );
        engine.shutdown();
    }

    #[test]
    fn subscribe_events_is_tolerant_of_device_presence() {
        let engine = AudioEngine::new();
        let mut events = engine.subscribe_events();
        // Tolerant: accept DeviceUnavailable, any other event, or none.
        std::thread::sleep(Duration::from_millis(200));
        match events.try_recv() {
            Ok(AudioEvent::DeviceUnavailable) => {}
            Ok(_) => {}
            Err(_) => {}
        }
        // The engine must still publish snapshots regardless.
        let rx = engine.subscribe();
        assert!(wait_until(&rx, Duration::from_secs(1), |s| s.volume <= 100));
        engine.shutdown();
    }

    #[test]
    fn shutdown_joins_cleanly() {
        let engine = AudioEngine::new();
        engine.set_volume(30);
        engine.pause();
        engine.stop();
        // Must return promptly (no hang).
        engine.shutdown();
    }

    #[test]
    fn default_and_typed_helpers_enqueue_without_panic() {
        let engine = AudioEngine::default();
        engine.play("/nonexistent/file.mp3", "title", "artist", 100.0);
        engine.set_volume(80);
        engine.seek_to(5.0);
        engine.seek_relative(10.0);
        engine.pause();
        engine.resume();
        engine.toggle_pause();
        engine.stop();
        engine.send(AudioCommand::SetVolume(60));
        std::thread::sleep(Duration::from_millis(200));
        engine.shutdown();
    }
}
