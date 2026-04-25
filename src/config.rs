//! Shared constants, paths, and utility functions for the Crusty music player.

use std::path::PathBuf;

use anyhow::Context;

/// Application name used for config directory and display.
pub(crate) const APP_NAME: &str = "youtube-music-player";

/// Maximum track duration in seconds (5 minutes). Tracks longer than this are filtered out.
pub(crate) const MAX_TRACK_DURATION_SECS: u64 = 300;

/// Maximum length for user search queries.
pub(crate) const MAX_SEARCH_QUERY_LEN: usize = 500;

/// Maximum length for user-entered playlist URLs.
pub(crate) const MAX_PLAYLIST_URL_LEN: usize = 2048;

/// Maximum concurrent downloads (kept low to avoid resource exhaustion).
pub(crate) const MAX_CONCURRENT_DOWNLOADS: usize = 5;

/// Maximum age (in seconds) of temp audio files before cleanup sweeps remove them.
pub(crate) const TEMP_FILE_MAX_AGE_SECS: u64 = 3600;

/// Number of tracks to pre-download ahead of the current position.
pub(crate) const LOOKAHEAD_DOWNLOAD_COUNT: usize = 10;

/// Number of tracks to pre-download on startup/queue restore.
pub(crate) const STARTUP_DOWNLOAD_COUNT: usize = 5;

/// How long (seconds) the on-disk feed cache is considered fresh.
/// After this TTL the next `open_feed_browser` triggers a background re-fetch.
pub(crate) const FEED_CACHE_TTL_SECS: u64 = 30 * 60; // 30 minutes

/// Returns the application config directory path (e.g., `~/.config/youtube-music-player`).
pub(crate) fn config_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join(APP_NAME);
    Ok(dir)
}

/// Returns true if the URL is an allowed YouTube domain.
#[must_use]
pub(crate) fn is_allowed_youtube_url(url: &str) -> bool {
    url.starts_with("https://www.youtube.com/")
        || url.starts_with("https://music.youtube.com/")
        || url.starts_with("https://youtu.be/")
        || url.starts_with("https://youtube.com/")
}

/// Formats a duration in seconds as `MM:SS` or `HH:MM:SS`.
#[must_use]
pub(crate) fn format_time(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours:02}:{mins:02}:{secs:02}")
    } else {
        format!("{mins:02}:{secs:02}")
    }
}

/// Returns the title as-is (placeholder for future title cleaning logic).
#[must_use]
pub(crate) fn clean_title(title: &str) -> &str {
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_youtube_url_valid() {
        assert!(is_allowed_youtube_url(
            "https://www.youtube.com/watch?v=abc"
        ));
        assert!(is_allowed_youtube_url(
            "https://music.youtube.com/watch?v=abc"
        ));
        assert!(is_allowed_youtube_url("https://youtu.be/abc"));
        assert!(is_allowed_youtube_url("https://youtube.com/watch?v=abc"));
    }

    #[test]
    fn test_is_allowed_youtube_url_invalid() {
        assert!(!is_allowed_youtube_url("https://evil.com/youtube.com"));
        assert!(!is_allowed_youtube_url("http://www.youtube.com/watch"));
        assert!(!is_allowed_youtube_url("https://vimeo.com/video"));
        assert!(!is_allowed_youtube_url(""));
    }

    #[test]
    fn test_format_time_minutes_seconds() {
        assert_eq!(format_time(0.0), "00:00");
        assert_eq!(format_time(59.0), "00:59");
        assert_eq!(format_time(60.0), "01:00");
        assert_eq!(format_time(90.0), "01:30");
        assert_eq!(format_time(212.0), "03:32");
    }

    #[test]
    fn test_format_time_hours() {
        assert_eq!(format_time(3600.0), "01:00:00");
        assert_eq!(format_time(3661.0), "01:01:01");
    }

    #[test]
    fn test_clean_title_passthrough() {
        assert_eq!(clean_title("Hello World"), "Hello World");
        assert_eq!(clean_title(""), "");
    }

    #[test]
    fn test_config_dir_returns_path() {
        let dir = config_dir();
        assert!(dir.is_ok());
        let path = dir.unwrap();
        assert!(path.ends_with(APP_NAME));
    }
}
