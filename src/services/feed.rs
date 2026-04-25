//! YouTube Music feed scraping service.
#![allow(dead_code)]
//!
//! Fetches personalised playlists from YouTube Music using the user's browser
//! cookies (via yt-dlp). No OAuth or YouTube Data API is required — the same
//! cookie-based auth path used for audio extraction is reused here.
//!
//! # Entry points
//!
//! - [`fetch_home`] — YouTube Music home page (Mixes, Recommended, Listen Again)
//! - [`fetch_library`] — user's saved/imported playlists
//! - [`fetch_liked`] — the "Liked Music" auto-playlist
//!
//! All three return [`FeedSection`]s containing [`FeedPlaylist`] entries.

use std::process::Command;

use crate::ui::state::{FeedPlaylist, FeedSection, PlaylistType};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur while fetching a YouTube Music feed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FeedError {
    /// No browser cookie config was provided — user needs to log in first.
    #[error("No browser account selected. Please log in via the account picker.")]
    NoCookies,

    /// yt-dlp is not installed or not on PATH.
    #[error("yt-dlp not found. Please install yt-dlp and ensure it is on your PATH.")]
    YtDlpMissing,

    /// yt-dlp reported that authentication is required (cookies expired/missing).
    #[error("YouTube authentication expired. Please re-select your browser account.")]
    AuthExpired,

    /// yt-dlp exited with a non-zero status for a reason other than auth.
    #[error("yt-dlp failed: {0}")]
    YtDlpFailed(String),

    /// The stdout from yt-dlp could not be decoded as UTF-8.
    #[error("yt-dlp output was not valid UTF-8: {0}")]
    InvalidUtf8(String),
}

impl FeedError {
    /// A short, user-facing message suitable for display in the TUI status bar.
    pub(crate) fn user_message(&self) -> String {
        self.to_string()
    }
}

// ---------------------------------------------------------------------------
// Public fetch functions
// ---------------------------------------------------------------------------

/// Fetch the YouTube Music home feed.
///
/// Returns sections for Mixes, Recommended playlists, and Listen Again entries
/// found on `https://music.youtube.com`.
pub(crate) fn fetch_home(
    cookie_config: Option<(bool, String)>,
) -> Result<Vec<FeedSection>, FeedError> {
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;
    let stdout = run_yt_dlp(&["https://music.youtube.com"], &cookie_config)?;
    let entries = parse_entries(&stdout);
    Ok(group_into_sections(entries))
}

/// Fetch the user's saved/imported playlists from their YouTube Music library.
///
/// Returns a single [`FeedSection`] with `kind = PlaylistType::LibrarySaved`.
pub(crate) fn fetch_library(
    cookie_config: Option<(bool, String)>,
) -> Result<FeedSection, FeedError> {
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;
    let stdout = run_yt_dlp(
        &["https://music.youtube.com/library/playlists"],
        &cookie_config,
    )?;
    let entries = parse_entries(&stdout)
        .into_iter()
        .filter(|p| {
            matches!(
                p.playlist_type,
                PlaylistType::LibrarySaved | PlaylistType::Unknown
            )
        })
        .collect();

    Ok(FeedSection {
        title: "Library".to_string(),
        kind: PlaylistType::LibrarySaved,
        items: entries,
    })
}

/// Fetch the user's "Liked Music" auto-playlist.
///
/// Returns a single [`FeedSection`] containing one entry for the liked-songs
/// playlist (`list=LM`).
pub(crate) fn fetch_liked(
    cookie_config: Option<(bool, String)>,
) -> Result<FeedSection, FeedError> {
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;
    let stdout = run_yt_dlp(
        &["https://music.youtube.com/playlist?list=LM"],
        &cookie_config,
    )?;

    // The liked-music playlist returns individual tracks, not a playlist entry.
    // We synthesise a single FeedPlaylist entry representing the whole list.
    let track_count = stdout.lines().filter(|l| !l.trim().is_empty()).count();

    let entry = FeedPlaylist {
        id: "LM".to_string(),
        title: "Liked Music".to_string(),
        url: "https://music.youtube.com/playlist?list=LM".to_string(),
        playlist_type: PlaylistType::LibraryLiked,
        track_count_estimate: track_count,
        thumbnail_url: None,
        description: Some("Your liked songs".to_string()),
    };

    Ok(FeedSection {
        title: "Liked Music".to_string(),
        kind: PlaylistType::LibraryLiked,
        items: vec![entry],
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run yt-dlp with `--flat-playlist --dump-json` against `urls`, injecting
/// the browser cookie argument. Returns stdout as a `String`.
///
/// Extracted as a separate function so tests can verify argument construction
/// without invoking the real binary.
fn run_yt_dlp(urls: &[&str], cookie_config: &(bool, String)) -> Result<String, FeedError> {
    let (_use_from_browser, cookie_arg) = cookie_config;

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--flat-playlist")
        .arg("--dump-json")
        .arg("--no-warnings")
        .arg("--skip-download")
        .arg("--socket-timeout")
        .arg("30")
        .arg("--retries")
        .arg("2")
        .arg("--cookies-from-browser")
        .arg(cookie_arg);

    for url in urls {
        cmd.arg(url);
    }

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            FeedError::YtDlpMissing
        } else {
            FeedError::YtDlpFailed(e.to_string())
        }
    })?;

    // Check stderr for auth-related failures before checking exit status,
    // because yt-dlp sometimes exits 0 even when auth fails.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_auth_error(&stderr) {
        return Err(FeedError::AuthExpired);
    }

    if !output.status.success() {
        return Err(FeedError::YtDlpFailed(stderr.trim().to_string()));
    }

    String::from_utf8(output.stdout).map_err(|e| FeedError::InvalidUtf8(e.to_string()))
}

/// Returns `true` if the yt-dlp stderr output indicates an authentication
/// failure (expired cookies, not logged in, etc.).
fn is_auth_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("sign in")
        || lower.contains("login required")
        || lower.contains("needs login")
        || lower.contains("not authenticated")
        || lower.contains("please log in")
}

/// Parse newline-delimited JSON output from yt-dlp into a flat list of
/// [`FeedPlaylist`] entries. Lines that are empty or cannot be parsed are
/// silently skipped.
pub(crate) fn parse_entries(stdout: &str) -> Vec<FeedPlaylist> {
    let mut entries = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Only process playlist-type entries
        let entry_type = json["_type"].as_str().unwrap_or("");
        if entry_type != "playlist" && entry_type != "url" {
            continue;
        }

        let id = json["id"]
            .as_str()
            .or_else(|| json["playlist_id"].as_str())
            .unwrap_or("")
            .to_string();

        if id.is_empty() {
            continue;
        }

        let title = json["title"]
            .as_str()
            .or_else(|| json["playlist_title"].as_str())
            .unwrap_or("Untitled")
            .to_string();

        let playlist_type = classify(&id, &title);

        let track_count_estimate = json["playlist_count"]
            .as_u64()
            .or_else(|| json["n_entries"].as_u64())
            .unwrap_or(0) as usize;

        let thumbnail_url = json["thumbnails"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|t| t["url"].as_str())
            .or_else(|| json["thumbnail"].as_str())
            .map(|s| s.to_string());

        let description = json["description"]
            .as_str()
            .or_else(|| json["uploader"].as_str())
            .or_else(|| json["channel"].as_str())
            .map(|s| s.to_string());

        // Canonicalise URL to music.youtube.com for Music playlists
        let url = json["url"]
            .as_str()
            .or_else(|| json["webpage_url"].as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                format!("https://music.youtube.com/playlist?list={}", id)
            });

        entries.push(FeedPlaylist {
            id,
            title,
            url,
            playlist_type,
            track_count_estimate,
            thumbnail_url,
            description,
        });
    }

    entries
}

/// Classify a playlist by its ID prefix and title into a [`PlaylistType`].
///
/// ID-based rules take priority over title-based rules so that a playlist
/// with a `PL*` ID is always `LibrarySaved` regardless of its title.
pub(crate) fn classify(id: &str, title: &str) -> PlaylistType {
    if id.starts_with("RDCLAK") || id.starts_with("RDAMPL") {
        PlaylistType::Mix
    } else if id == "LM" {
        PlaylistType::LibraryLiked
    } else if id.starts_with("OLAK5uy_") {
        PlaylistType::Recommended
    } else if id.starts_with("PL") || id.starts_with("VL") {
        // ID-based library check before title check so "Listen Again" in the
        // title of a PL* playlist doesn't override the library classification.
        PlaylistType::LibrarySaved
    } else if title.to_lowercase().contains("listen again") {
        PlaylistType::ListenAgain
    } else {
        PlaylistType::Unknown
    }
}

/// Group a flat list of [`FeedPlaylist`] entries into labelled [`FeedSection`]s.
fn group_into_sections(entries: Vec<FeedPlaylist>) -> Vec<FeedSection> {
    let mut mixes: Vec<FeedPlaylist> = Vec::new();
    let mut recommended: Vec<FeedPlaylist> = Vec::new();
    let mut listen_again: Vec<FeedPlaylist> = Vec::new();
    let mut other: Vec<FeedPlaylist> = Vec::new();

    for entry in entries {
        match entry.playlist_type {
            PlaylistType::Mix => mixes.push(entry),
            PlaylistType::Recommended => recommended.push(entry),
            PlaylistType::ListenAgain => listen_again.push(entry),
            _ => other.push(entry),
        }
    }

    let mut sections = Vec::new();

    if !mixes.is_empty() {
        sections.push(FeedSection {
            title: "My Mixes".to_string(),
            kind: PlaylistType::Mix,
            items: mixes,
        });
    }
    if !recommended.is_empty() {
        sections.push(FeedSection {
            title: "Recommended".to_string(),
            kind: PlaylistType::Recommended,
            items: recommended,
        });
    }
    if !listen_again.is_empty() {
        sections.push(FeedSection {
            title: "Listen Again".to_string(),
            kind: PlaylistType::ListenAgain,
            items: listen_again,
        });
    }
    if !other.is_empty() {
        sections.push(FeedSection {
            title: "Other".to_string(),
            kind: PlaylistType::Unknown,
            items: other,
        });
    }

    sections
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- classify --

    #[test]
    fn classify_rdclak_is_mix() {
        assert_eq!(classify("RDCLAK5uy_abc123", "My Mix"), PlaylistType::Mix);
    }

    #[test]
    fn classify_rdampl_is_mix() {
        assert_eq!(classify("RDAMPL_xyz", "Radio"), PlaylistType::Mix);
    }

    #[test]
    fn classify_lm_is_liked() {
        assert_eq!(classify("LM", "Liked Music"), PlaylistType::LibraryLiked);
    }

    #[test]
    fn classify_olak5uy_is_recommended() {
        assert_eq!(
            classify("OLAK5uy_abc", "Album Playlist"),
            PlaylistType::Recommended
        );
    }

    #[test]
    fn classify_listen_again_title() {
        assert_eq!(
            classify("PLsomething", "Listen Again"),
            PlaylistType::LibrarySaved // id wins for PL prefix
        );
        assert_eq!(
            classify("RDsomething", "Listen Again"),
            PlaylistType::ListenAgain // non-PL, non-RDCLAK/RDAMPL → title check
        );
    }

    #[test]
    fn classify_pl_prefix_is_library_saved() {
        assert_eq!(classify("PLabc123", "My Playlist"), PlaylistType::LibrarySaved);
    }

    #[test]
    fn classify_vl_prefix_is_library_saved() {
        assert_eq!(classify("VLabc123", "Playlist"), PlaylistType::LibrarySaved);
    }

    #[test]
    fn classify_unknown_falls_through() {
        assert_eq!(classify("XYZabc", "Random"), PlaylistType::Unknown);
    }

    // -- parse_entries --

    #[test]
    fn parse_entries_empty_input() {
        assert!(parse_entries("").is_empty());
    }

    #[test]
    fn parse_entries_skips_blank_lines() {
        let input = "\n\n\n";
        assert!(parse_entries(input).is_empty());
    }

    #[test]
    fn parse_entries_skips_invalid_json() {
        let input = "not json\n{also not json\n";
        assert!(parse_entries(input).is_empty());
    }

    #[test]
    fn parse_entries_skips_non_playlist_type() {
        let input = r#"{"_type":"video","id":"abc123","title":"A Song"}"#;
        assert!(parse_entries(input).is_empty());
    }

    #[test]
    fn parse_entries_parses_mix_playlist() {
        let input = r#"{"_type":"playlist","id":"RDCLAK5uy_test","title":"My Mix 1","playlist_count":50,"thumbnail":"https://img.example.com/thumb.jpg","uploader":"YouTube Music"}"#;
        let entries = parse_entries(input);

        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.id, "RDCLAK5uy_test");
        assert_eq!(e.title, "My Mix 1");
        assert_eq!(e.playlist_type, PlaylistType::Mix);
        assert_eq!(e.track_count_estimate, 50);
        assert_eq!(
            e.thumbnail_url,
            Some("https://img.example.com/thumb.jpg".to_string())
        );
        assert_eq!(e.description, Some("YouTube Music".to_string()));
    }

    #[test]
    fn parse_entries_parses_recommended_playlist() {
        let input = r#"{"_type":"playlist","id":"OLAK5uy_abc","title":"Chill Vibes","n_entries":30}"#;
        let entries = parse_entries(input);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].playlist_type, PlaylistType::Recommended);
        assert_eq!(entries[0].track_count_estimate, 30);
    }

    #[test]
    fn parse_entries_handles_missing_optional_fields() {
        let input = r#"{"_type":"playlist","id":"PLtest123","title":"My Playlist"}"#;
        let entries = parse_entries(input);

        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.track_count_estimate, 0);
        assert!(e.thumbnail_url.is_none());
        assert!(e.description.is_none());
    }

    #[test]
    fn parse_entries_skips_entries_with_empty_id() {
        let input = r#"{"_type":"playlist","id":"","title":"No ID"}"#;
        assert!(parse_entries(input).is_empty());
    }

    #[test]
    fn parse_entries_multiple_lines() {
        let input = concat!(
            r#"{"_type":"playlist","id":"RDCLAK5uy_a","title":"Mix A"}"#,
            "\n",
            r#"{"_type":"playlist","id":"OLAK5uy_b","title":"Rec B"}"#,
            "\n",
            r#"{"_type":"playlist","id":"PLc","title":"Saved C"}"#,
        );
        let entries = parse_entries(input);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].playlist_type, PlaylistType::Mix);
        assert_eq!(entries[1].playlist_type, PlaylistType::Recommended);
        assert_eq!(entries[2].playlist_type, PlaylistType::LibrarySaved);
    }

    // -- is_auth_error --

    #[test]
    fn auth_error_detected_sign_in() {
        assert!(is_auth_error("ERROR: Sign in to confirm your age"));
    }

    #[test]
    fn auth_error_detected_login_required() {
        assert!(is_auth_error("ERROR: Login required"));
    }

    #[test]
    fn auth_error_detected_needs_login() {
        assert!(is_auth_error("This video needs login to watch"));
    }

    #[test]
    fn auth_error_not_triggered_on_normal_output() {
        assert!(!is_auth_error("[youtube] Extracting URL"));
        assert!(!is_auth_error(""));
    }

    // -- group_into_sections --

    #[test]
    fn group_into_sections_empty() {
        assert!(group_into_sections(vec![]).is_empty());
    }

    #[test]
    fn group_into_sections_creates_correct_sections() {
        let entries = vec![
            FeedPlaylist {
                id: "RDCLAK5uy_a".into(),
                title: "Mix A".into(),
                url: "https://music.youtube.com/playlist?list=RDCLAK5uy_a".into(),
                playlist_type: PlaylistType::Mix,
                track_count_estimate: 10,
                thumbnail_url: None,
                description: None,
            },
            FeedPlaylist {
                id: "OLAK5uy_b".into(),
                title: "Rec B".into(),
                url: "https://music.youtube.com/playlist?list=OLAK5uy_b".into(),
                playlist_type: PlaylistType::Recommended,
                track_count_estimate: 20,
                thumbnail_url: None,
                description: None,
            },
        ];

        let sections = group_into_sections(entries);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "My Mixes");
        assert_eq!(sections[0].items.len(), 1);
        assert_eq!(sections[1].title, "Recommended");
        assert_eq!(sections[1].items.len(), 1);
    }

    #[test]
    fn group_into_sections_omits_empty_sections() {
        let entries = vec![FeedPlaylist {
            id: "RDCLAK5uy_a".into(),
            title: "Mix A".into(),
            url: "https://music.youtube.com/playlist?list=RDCLAK5uy_a".into(),
            playlist_type: PlaylistType::Mix,
            track_count_estimate: 5,
            thumbnail_url: None,
            description: None,
        }];

        let sections = group_into_sections(entries);
        // Only "My Mixes" — no Recommended, Listen Again, or Other sections
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "My Mixes");
    }
}
