//! Pure, side-effect-free mappings between the engine's [`PlayerSnapshot`] and
//! MPRIS2 types. Kept separate from the D-Bus server so they can be unit-tested
//! without a session bus.

use mpris_server::{Metadata, PlaybackStatus, Time, TrackId};

use crate::{PlayerSnapshot, PlayerState};

/// Map the engine [`PlayerState`] to an MPRIS [`PlaybackStatus`].
///
/// `Loading` reports as `Stopped` (no audio is audible yet), matching how the
/// engine treats the transitional state for finish-detection.
#[must_use]
pub fn playback_status(state: PlayerState) -> PlaybackStatus {
    match state {
        PlayerState::Playing => PlaybackStatus::Playing,
        PlayerState::Paused => PlaybackStatus::Paused,
        PlayerState::Stopped | PlayerState::Loading => PlaybackStatus::Stopped,
    }
}

/// Convert a position/duration in seconds to an MPRIS [`Time`] (microseconds).
///
/// Non-finite and negative inputs clamp to zero; the result is saturated into
/// `i64` microseconds so it can never panic or wrap.
#[must_use]
pub fn secs_to_time(secs: f64) -> Time {
    if !secs.is_finite() || secs <= 0.0 {
        return Time::ZERO;
    }
    let micros = secs * 1_000_000.0;
    // Saturate rather than wrap on the f64 -> i64 cast.
    let micros = if micros >= i64::MAX as f64 {
        i64::MAX
    } else {
        micros as i64
    };
    Time::from_micros(micros)
}

/// Convert an MPRIS [`Time`] (microseconds) back to f64 seconds (never negative).
#[must_use]
pub fn time_to_secs(time: Time) -> f64 {
    let secs = time.as_micros() as f64 / 1_000_000.0;
    if secs < 0.0 {
        0.0
    } else {
        secs
    }
}

/// Engine volume (`0..=100`) to MPRIS volume (`0.0..=1.0`).
#[must_use]
pub fn volume_to_mpris(v: u32) -> f64 {
    (v as f64 / 100.0).clamp(0.0, 1.0)
}

/// MPRIS volume (`0.0..=1.0`, possibly out of range) to engine volume (`0..=100`).
#[must_use]
pub fn volume_from_mpris(v: f64) -> u32 {
    if !v.is_finite() {
        return 0;
    }
    (v.clamp(0.0, 1.0) * 100.0).round() as u32
}

/// Build the MPRIS object path for a synthesized track number.
///
/// The path uses only digits and the fixed prefix, so it is always a valid
/// D-Bus object path — unlike YouTube video IDs, which contain `-`/`_` that are
/// illegal in path *elements*.
#[must_use]
pub fn trackid_for(track_no: u64) -> TrackId {
    let path = format!("/org/mpris/MediaPlayer2/crusty/track/{track_no}");
    TrackId::try_from(path).unwrap_or(TrackId::NO_TRACK)
}

/// Decide the next synthesized track number.
///
/// The number is bumped whenever the title changes, giving MPRIS clients a
/// distinct `mpris:trackid` per track (required for `SetPosition` and for
/// clients that detect track changes via the id).
#[must_use]
pub fn next_track_no(prev_title: &str, new_title: &str, prev_no: u64) -> u64 {
    if prev_title == new_title {
        prev_no
    } else {
        prev_no.wrapping_add(1)
    }
}

/// Build MPRIS [`Metadata`] from a snapshot and a synthesized track number.
///
/// Omits `mpris:artUrl` and `xesam:album` (Crusty has neither). Always sets a
/// valid `mpris:trackid`. `xesam:artist` is omitted when the artist is empty.
#[must_use]
pub fn build_metadata(snap: &PlayerSnapshot, track_no: u64) -> Metadata {
    let mut builder = Metadata::builder()
        .trackid(trackid_for(track_no))
        .title(snap.title.clone())
        .length(secs_to_time(snap.duration_secs));

    if !snap.artist.is_empty() {
        builder = builder.artist([snap.artist.clone()]);
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_status_maps_all_states() {
        assert_eq!(playback_status(PlayerState::Playing), PlaybackStatus::Playing);
        assert_eq!(playback_status(PlayerState::Paused), PlaybackStatus::Paused);
        assert_eq!(playback_status(PlayerState::Stopped), PlaybackStatus::Stopped);
        // Loading reports as Stopped (nothing audible yet).
        assert_eq!(playback_status(PlayerState::Loading), PlaybackStatus::Stopped);
    }

    #[test]
    fn secs_to_time_basic() {
        assert_eq!(secs_to_time(0.0).as_micros(), 0);
        assert_eq!(secs_to_time(1.0).as_micros(), 1_000_000);
        assert_eq!(secs_to_time(2.5).as_micros(), 2_500_000);
    }

    #[test]
    fn secs_to_time_clamps_invalid() {
        assert_eq!(secs_to_time(-5.0).as_micros(), 0);
        assert_eq!(secs_to_time(f64::NAN).as_micros(), 0);
        assert_eq!(secs_to_time(f64::INFINITY).as_micros(), 0);
        // Absurdly large values saturate instead of wrapping.
        assert_eq!(secs_to_time(1e30).as_micros(), i64::MAX);
    }

    #[test]
    fn time_to_secs_roundtrip_and_clamp() {
        assert!((time_to_secs(Time::from_micros(2_500_000)) - 2.5).abs() < 1e-9);
        assert_eq!(time_to_secs(Time::from_micros(-1_000_000)), 0.0);
    }

    #[test]
    fn volume_conversions() {
        assert!((volume_to_mpris(50) - 0.5).abs() < 1e-9);
        assert!((volume_to_mpris(0) - 0.0).abs() < 1e-9);
        assert!((volume_to_mpris(100) - 1.0).abs() < 1e-9);
        assert_eq!(volume_from_mpris(0.5), 50);
        assert_eq!(volume_from_mpris(0.0), 0);
        assert_eq!(volume_from_mpris(1.0), 100);
        // Out-of-range / non-finite clamp.
        assert_eq!(volume_from_mpris(2.0), 100);
        assert_eq!(volume_from_mpris(-1.0), 0);
        assert_eq!(volume_from_mpris(f64::NAN), 0);
    }

    #[test]
    fn trackid_is_valid_object_path() {
        let id = trackid_for(7);
        // The crate stores it as a zbus ObjectPath; if it parsed, it's valid.
        // Round-trip the inner path to confirm it isn't the NO_TRACK fallback.
        let inner = id.into_inner();
        assert_eq!(inner.as_str(), "/org/mpris/MediaPlayer2/crusty/track/7");
    }

    #[test]
    fn next_track_no_increments_only_on_title_change() {
        assert_eq!(next_track_no("a", "a", 3), 3);
        assert_eq!(next_track_no("a", "b", 3), 4);
        assert_eq!(next_track_no("", "first", 0), 1);
    }

    #[test]
    fn build_metadata_populates_fields() {
        let snap = PlayerSnapshot {
            title: "Song".into(),
            artist: "Artist".into(),
            duration_secs: 180.0,
            ..Default::default()
        };
        let meta = build_metadata(&snap, 2);
        assert_eq!(meta.title().as_deref(), Some("Song"));
        assert_eq!(meta.artist(), Some(vec!["Artist".to_string()]));
        assert_eq!(meta.length(), Some(secs_to_time(180.0)));
        assert_eq!(
            meta.trackid().map(|t| t.into_inner().as_str().to_string()),
            Some("/org/mpris/MediaPlayer2/crusty/track/2".to_string())
        );
        // No album / artUrl.
        assert!(meta.album().is_none());
        assert!(meta.art_url().is_none());
    }

    #[test]
    fn build_metadata_omits_empty_artist() {
        let snap = PlayerSnapshot {
            title: "Song".into(),
            artist: String::new(),
            ..Default::default()
        };
        let meta = build_metadata(&snap, 1);
        assert!(meta.artist().is_none());
    }
}
