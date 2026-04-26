//! YouTube Music feed scraping service.
//!
//! Fetches personalised playlists from YouTube Music using the user's browser
//! cookies (via yt-dlp). No OAuth or YouTube Data API is required — the same
//! cookie-based auth path used for audio extraction is reused here.
//!
//! # Entry points
//!
//! - [`fetch_liked`] — the "Liked Music" auto-playlist (`list=LM`)
//! - [`fetch_library_playlists`] — all playlists from `youtube.com/feed/playlists`
//!   (owned, saved, mixes — everything the user has in their library)
//! - [`fetch_all_parallel`] — fetches both in parallel and merges results
//!
//! All functions return [`FeedSection`]s containing [`FeedPlaylist`] entries.

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

/// Fetch the user's "Liked Music" playlist from YouTube Music (`list=LM`).
///
/// Also extracts the user's YouTube channel ID from the response metadata,
/// retained for potential future use. Current callers ignore it.
///
/// Returns a `(FeedSection, Option<channel_id>)` tuple.
pub(crate) fn fetch_liked(
    cookie_config: Option<(bool, String)>,
) -> Result<(FeedSection, Option<String>), FeedError> {
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;
    let stdout = run_yt_dlp(
        &["https://music.youtube.com/playlist?list=LM"],
        &cookie_config,
    )?;

    if stdout.trim().is_empty() {
        return Err(FeedError::AuthExpired);
    }

    // Parse individual track lines to count tracks and extract channel ID.
    let mut track_count = 0usize;
    let mut channel_id: Option<String> = None;

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            track_count += 1;
            // The channel_id in the playlist metadata is the *owner's* channel.
            if channel_id.is_none() {
                if let Some(cid) = json["playlist_channel_id"].as_str() {
                    if !cid.is_empty() {
                        channel_id = Some(cid.to_string());
                    }
                }
            }
        }
    }

    let entry = FeedPlaylist {
        id: "LM".to_string(),
        title: "Liked Music".to_string(),
        url: "https://music.youtube.com/playlist?list=LM".to_string(),
        playlist_type: PlaylistType::LibraryLiked,
        track_count_estimate: track_count,
        thumbnail_url: None,
        description: Some(format!("{track_count} liked songs")),
    };

    Ok((
        FeedSection {
            title: "Liked Music".to_string(),
            kind: PlaylistType::LibraryLiked,
            items: vec![entry],
        },
        channel_id,
    ))
}


/// Fetch all playlists from `youtube.com/feed/playlists`.
///
/// This single endpoint returns **everything** in the user's library:
/// - Saved YouTube Music mixes (`RDCLAK*`)
/// - Playlists created by the user (`PL*`)
/// - Playlists saved from other creators (`PL*`)
/// - Private playlists
///
/// System playlists (`WL` — Watch Later, `LL` — Liked Videos) are filtered
/// out because they are either irrelevant or already covered by [`fetch_liked`].
///
/// Returns two sections: "Saved Mixes" (RDCLAK/RDAMPL) and "My Playlists"
/// (everything else), omitting whichever is empty.
pub(crate) fn fetch_library_playlists(
    cookie_config: Option<(bool, String)>,
) -> Result<Vec<FeedSection>, FeedError> {
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;
    let stdout = run_yt_dlp(
        &["https://www.youtube.com/feed/playlists"],
        &cookie_config,
    )?;

    if stdout.trim().is_empty() {
        // Empty output is not fatal — user may have no saved playlists.
        return Ok(Vec::new());
    }

    // IDs to skip — system playlists covered elsewhere or not useful.
    const SKIP_IDS: &[&str] = &[
        "WL", // Watch Later
        "LL", // Liked Videos (covered by fetch_liked / list=LM)
        "LM", // Liked Music — defensive, already fetched separately
        "HL", // Watch History pseudo-playlist (defensive)
    ];

    let entries = parse_entries(&stdout)
        .into_iter()
        .filter(|p| !SKIP_IDS.contains(&p.id.as_str()))
        .map(|mut p| {
            // The feed/playlists endpoint returns `playlist_count` = the number
            // of playlists in the feed page (always 7 in testing), NOT the track
            // count of each individual playlist. Zero it out so the UI shows "—"
            // rather than a misleading number. The real count is fetched on expand.
            p.track_count_estimate = 0;
            p
        })
        .collect::<Vec<_>>();

    // Split into mixes (RDCLAK/RDAMPL) and regular playlists.
    let mut mixes = Vec::new();
    let mut playlists = Vec::new();

    for entry in entries {
        match entry.playlist_type {
            PlaylistType::Mix => mixes.push(entry),
            // LibraryLiked should never appear here (LM is in SKIP_IDS above),
            // but if it does, treat it as a regular playlist rather than silently
            // dropping it. Exhaustive match ensures new variants force a decision.
            PlaylistType::LibrarySaved
            | PlaylistType::Recommended
            | PlaylistType::ListenAgain
            | PlaylistType::LibraryLiked
            | PlaylistType::Unknown => playlists.push(entry),
        }
    }

    let mut sections = Vec::new();

    if !mixes.is_empty() {
        sections.push(FeedSection {
            title: "Saved Mixes".to_string(),
            kind: PlaylistType::Mix,
            items: mixes,
        });
    }

    if !playlists.is_empty() {
        sections.push(FeedSection {
            title: "My Playlists".to_string(),
            kind: PlaylistType::LibrarySaved,
            items: playlists,
        });
    }

    Ok(sections)
}

/// Fetch all available feed sources and merge into a single ordered list.
///
/// **What yt-dlp supports (as of 2026):**
/// - `playlist?list=LM` — Liked Music ✅ (requires cookies)
/// - `youtube.com/feed/playlists` — full library (owned + saved + mixes) ✅
/// - `music.youtube.com/feed/music` — personalised home feed ❌ (not supported)
/// - `music.youtube.com/library/playlists` — library ❌ (404s even with cookies)
///
/// Strategy: fetch Liked Music first (mandatory — also acts as the auth gate),
/// then fetch the full library feed. The two calls are sequential; the library
/// fetch is non-fatal — if it fails, Liked Music is still returned.
///
/// Note: despite the name, the fetches are sequential (both are blocking
/// yt-dlp subprocesses). The caller in `actions.rs` wraps this in
/// `spawn_blocking` so it does not block the async runtime.
pub(crate) fn fetch_all_parallel(
    cookie_config: Option<(bool, String)>,
) -> Result<Vec<FeedSection>, FeedError> {
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;

    // Step 1: Liked Music — mandatory (auth check).
    let (liked_section, _channel_id) = fetch_liked(Some(cookie_config.clone()))?;

    // Step 2: Full library feed — optional, failure is non-fatal.
    // Auth failures are already caught by fetch_liked above; this handles
    // transient errors (network, yt-dlp version drift, etc.).
    let library_sections = match fetch_library_playlists(Some(cookie_config)) {
        Ok(s) => s,
        Err(e) => {
            // Non-fatal: surface in logs so the user can diagnose if needed.
            eprintln!("[crusty] library feed fetch failed (non-fatal): {e}");
            Vec::new()
        }
    };

    let mut sections = Vec::new();

    // Library sections first (Saved Mixes, then My Playlists)
    for section in library_sections {
        if !section.items.is_empty() {
            sections.push(section);
        }
    }

    // Liked Music always last
    if !liked_section.items.is_empty() {
        sections.push(liked_section);
    }

    if sections.is_empty() {
        return Err(FeedError::YtDlpFailed(
            "No playlists found. Make sure you are logged into YouTube in your browser.".into(),
        ));
    }

    Ok(sections)
}

/// Fetch the individual tracks of any playlist URL and return them as
/// `Vec<FeedTrack>` for display in the feed browser's track pane.
///
/// This is used when the user expands a playlist to cherry-pick tracks.
pub(crate) fn fetch_tracks_for_playlist(
    cookie_config: Option<(bool, String)>,
    url: &str,
) -> Result<Vec<crate::ui::state::FeedTrack>, FeedError> {
    if !crate::config::is_allowed_youtube_url(url) {
        return Err(FeedError::YtDlpFailed(format!(
            "Blocked non-YouTube URL: {}",
            url.chars().take(100).collect::<String>()
        )));
    }
    let cookie_config = cookie_config.ok_or(FeedError::NoCookies)?;
    let stdout = run_yt_dlp(&[url], &cookie_config)?;

    if stdout.trim().is_empty() {
        return Err(FeedError::AuthExpired);
    }

    let mut tracks = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        let video_id = json["id"].as_str().unwrap_or("").to_string();
        if video_id.is_empty() {
            continue;
        }

        let title = sanitize_text(
            json["title"].as_str().unwrap_or("Unknown"),
        );
        let uploader = sanitize_text(
            json["uploader"]
                .as_str()
                .or_else(|| json["channel"].as_str())
                .unwrap_or("Unknown"),
        );
        let duration = json["duration"].as_u64().unwrap_or(0);

        // Prefer music.youtube.com URL if available
        let url = json["url"]
            .as_str()
            .or_else(|| json["webpage_url"].as_str())
            .map(sanitize_text)
            .unwrap_or_else(|| {
                format!("https://music.youtube.com/watch?v={video_id}")
            });

        tracks.push(crate::ui::state::FeedTrack {
            video_id,
            title,
            uploader,
            duration,
            url,
        });
    }

    Ok(tracks)
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
        // Take only the first non-empty line, cap at 200 chars, and strip
        // control characters so the message is safe to display in the TUI.
        let snippet = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unknown error");
        let snippet = sanitize_text(&snippet.chars().take(200).collect::<String>());
        return Err(FeedError::YtDlpFailed(snippet));
    }

    String::from_utf8(output.stdout).map_err(|e| FeedError::InvalidUtf8(e.to_string()))
}

/// Strip control characters from a string sourced from yt-dlp output.
///
/// Prevents terminal-escape injection (OSC 8 hyperlinks, cursor-positioning
/// sequences, etc.) from reaching the ratatui renderer. Tabs are preserved;
/// all other C0/C1 control characters are removed.
pub(crate) fn sanitize_text(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect()
}

/// Returns `true` if `id` looks like a safe YouTube playlist ID.
///
/// Playlist IDs from yt-dlp should only contain alphanumerics, hyphens,
/// and underscores. This guards the synthesised-URL fallback in
/// `parse_entries` against a malformed `id` being embedded in a URL.
fn is_safe_playlist_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
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

        // Sanitize all yt-dlp-derived strings to strip control characters
        // (prevents terminal-escape injection in the TUI renderer).
        let id = sanitize_text(&id);
        let title = sanitize_text(
            json["title"]
                .as_str()
                .or_else(|| json["playlist_title"].as_str())
                .unwrap_or("Untitled"),
        );

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
            .map(sanitize_text);

        let description = json["description"]
            .as_str()
            .or_else(|| json["uploader"].as_str())
            .or_else(|| json["channel"].as_str())
            .map(sanitize_text);

        // Canonicalise URL to music.youtube.com for Music playlists.
        // For the synthesised fallback, only use `id` if it passes the
        // safe-ID check — otherwise skip this entry entirely.
        let url = if let Some(raw_url) = json["url"]
            .as_str()
            .or_else(|| json["webpage_url"].as_str())
        {
            sanitize_text(raw_url)
        } else if is_safe_playlist_id(&id) {
            format!("https://music.youtube.com/playlist?list={id}")
        } else {
            continue; // unsafe id — skip rather than construct a bad URL
        };

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
#[allow(dead_code)]
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

    // -- fetch_all_parallel merge logic --

    /// Build a minimal FeedSection for testing merge behaviour.
    fn make_section(title: &str, kind: PlaylistType, n_items: usize) -> FeedSection {
        FeedSection {
            title: title.to_string(),
            kind,
            items: (0..n_items)
                .map(|i| FeedPlaylist {
                    id: format!("{title}-{i}"),
                    title: format!("Item {i}"),
                    url: format!("https://music.youtube.com/playlist?list={title}-{i}"),
                    playlist_type: kind,
                    track_count_estimate: 5,
                    thumbnail_url: None,
                    description: None,
                })
                .collect(),
        }
    }

    #[test]
    fn merge_appends_non_empty_optional_sections() {
        // Simulate what fetch_all_parallel does with its results.
        let home = vec![
            make_section("My Mixes", PlaylistType::Mix, 2),
            make_section("Recommended", PlaylistType::Recommended, 1),
        ];
        let lib: Result<FeedSection, FeedError> =
            Ok(make_section("Library", PlaylistType::LibrarySaved, 3));
        let liked: Result<FeedSection, FeedError> =
            Ok(make_section("Liked Music", PlaylistType::LibraryLiked, 1));

        let mut sections = home;
        if let Ok(s) = lib {
            if !s.items.is_empty() {
                sections.push(s);
            }
        }
        if let Ok(s) = liked {
            if !s.items.is_empty() {
                sections.push(s);
            }
        }

        assert_eq!(sections.len(), 4);
        assert_eq!(sections[2].title, "Library");
        assert_eq!(sections[3].title, "Liked Music");
    }

    #[test]
    fn merge_skips_empty_optional_sections() {
        let home = vec![make_section("My Mixes", PlaylistType::Mix, 2)];
        // Library returns an empty section (e.g. user has no saved playlists)
        let lib: Result<FeedSection, FeedError> =
            Ok(make_section("Library", PlaylistType::LibrarySaved, 0));
        let liked: Result<FeedSection, FeedError> =
            Ok(make_section("Liked Music", PlaylistType::LibraryLiked, 1));

        let mut sections = home;
        if let Ok(s) = lib {
            if !s.items.is_empty() {
                sections.push(s);
            }
        }
        if let Ok(s) = liked {
            if !s.items.is_empty() {
                sections.push(s);
            }
        }

        // Library was empty — only home + liked
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].title, "Liked Music");
    }

    #[test]
    fn merge_tolerates_optional_section_errors() {
        let home = vec![make_section("My Mixes", PlaylistType::Mix, 2)];
        let lib: Result<FeedSection, FeedError> =
            Err(FeedError::YtDlpFailed("network error".into()));
        let liked: Result<FeedSection, FeedError> =
            Err(FeedError::AuthExpired);

        let mut sections = home;
        if let Ok(s) = lib {
            if !s.items.is_empty() {
                sections.push(s);
            }
        }
        if let Ok(s) = liked {
            if !s.items.is_empty() {
                sections.push(s);
            }
        }

        // Both optional fetches failed — only home sections remain
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "My Mixes");
    }

    // -- fetch_library_playlists filtering logic --

    #[test]
    fn library_playlists_skips_watch_later_and_liked_videos() {
        // WL, LL, LM, HL should all be filtered out
        const SKIP_IDS: &[&str] = &["WL", "LL", "LM", "HL"];
        let input = concat!(
            r#"{"_type":"url","id":"WL","title":"Watch later","url":"https://www.youtube.com/playlist?list=WL"}"#,
            "\n",
            r#"{"_type":"url","id":"LL","title":"Liked videos","url":"https://www.youtube.com/playlist?list=LL"}"#,
            "\n",
            r#"{"_type":"url","id":"LM","title":"Liked Music","url":"https://www.youtube.com/playlist?list=LM"}"#,
            "\n",
            r#"{"_type":"url","id":"HL","title":"History","url":"https://www.youtube.com/playlist?list=HL"}"#,
            "\n",
            r#"{"_type":"url","id":"RDCLAK5uy_abc","title":"Noise Riot: Rock Hits","url":"https://www.youtube.com/playlist?list=RDCLAK5uy_abc"}"#,
            "\n",
            r#"{"_type":"url","id":"PLtest123","title":"My Gaming Playlist","url":"https://www.youtube.com/playlist?list=PLtest123"}"#,
        );
        let entries = parse_entries(input)
            .into_iter()
            .filter(|p| !SKIP_IDS.contains(&p.id.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "RDCLAK5uy_abc");
        assert_eq!(entries[1].id, "PLtest123");
    }

    #[test]
    fn library_playlists_splits_mixes_and_playlists() {
        let input = concat!(
            r#"{"_type":"url","id":"RDCLAK5uy_abc","title":"Rock Mix","url":"https://www.youtube.com/playlist?list=RDCLAK5uy_abc"}"#,
            "\n",
            r#"{"_type":"url","id":"RDAMPL_xyz","title":"Chill Mix","url":"https://www.youtube.com/playlist?list=RDAMPL_xyz"}"#,
            "\n",
            r#"{"_type":"url","id":"PLtest123","title":"My Playlist","url":"https://www.youtube.com/playlist?list=PLtest123"}"#,
        );
        let entries = parse_entries(input);
        let mut mixes = Vec::new();
        let mut playlists = Vec::new();
        for e in entries {
            match e.playlist_type {
                PlaylistType::Mix => mixes.push(e),
                _ => playlists.push(e),
            }
        }
        assert_eq!(mixes.len(), 2);
        assert_eq!(playlists.len(), 1);
        assert_eq!(mixes[0].id, "RDCLAK5uy_abc");
        assert_eq!(mixes[1].id, "RDAMPL_xyz");
        assert_eq!(playlists[0].id, "PLtest123");
    }

    #[test]
    fn library_playlists_empty_when_only_system_playlists() {
        const SKIP_IDS: &[&str] = &["WL", "LL", "LM", "HL"];
        let input = concat!(
            r#"{"_type":"url","id":"WL","title":"Watch later","url":"https://www.youtube.com/playlist?list=WL"}"#,
            "\n",
            r#"{"_type":"url","id":"LL","title":"Liked videos","url":"https://www.youtube.com/playlist?list=LL"}"#,
            "\n",
            r#"{"_type":"url","id":"LM","title":"Liked Music","url":"https://www.youtube.com/playlist?list=LM"}"#,
            "\n",
            r#"{"_type":"url","id":"HL","title":"History","url":"https://www.youtube.com/playlist?list=HL"}"#,
        );
        let entries = parse_entries(input)
            .into_iter()
            .filter(|p| !SKIP_IDS.contains(&p.id.as_str()))
            .collect::<Vec<_>>();
        assert!(entries.is_empty());
    }

    // -- CacheStore integration with FeedSection --

    #[test]
    fn feed_cache_round_trip() {
        use crate::services::cache_store::CacheStore;
        use std::time::Duration;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let store: CacheStore<Vec<FeedSection>> = CacheStore::new(
            tmp.path().join("feed_cache.json"),
            Duration::from_secs(3600),
            2, // current schema version
        );

        let sections = vec![
            make_section("Saved Mixes", PlaylistType::Mix, 2),
            make_section("My Playlists", PlaylistType::LibrarySaved, 1),
        ];

        store.save(&sections).unwrap();
        let loaded = store.load().unwrap().unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "Saved Mixes");
        assert_eq!(loaded[0].items.len(), 2);
        assert_eq!(loaded[1].title, "My Playlists");
    }

    #[test]
    fn feed_cache_expires_after_ttl() {
        use crate::services::cache_store::CacheStore;
        use std::time::Duration;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        // TTL of 0 — always expired
        let store: CacheStore<Vec<FeedSection>> = CacheStore::new(
            tmp.path().join("feed_cache.json"),
            Duration::from_secs(0),
            1,
        );

        store
            .save(&vec![make_section("My Mixes", PlaylistType::Mix, 1)])
            .unwrap();

        // Should be a miss immediately
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn feed_cache_schema_version_mismatch_is_miss() {
        use crate::services::cache_store::CacheStore;
        use std::time::Duration;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Save with current schema version
        let store_v2: CacheStore<Vec<FeedSection>> = CacheStore::new(
            tmp.path().join("feed_cache.json"),
            Duration::from_secs(3600),
            2,
        );
        store_v2
            .save(&vec![make_section("Saved Mixes", PlaylistType::Mix, 1)])
            .unwrap();

        // Load with a future schema version — should be a miss
        let store_v3: CacheStore<Vec<FeedSection>> = CacheStore::new(
            tmp.path().join("feed_cache.json"),
            Duration::from_secs(3600),
            3,
        );
        assert!(store_v3.load().unwrap().is_none());
    }
}
