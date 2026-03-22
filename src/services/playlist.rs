//! Playlist fetching service — yt-dlp based playlist and My Mix extraction.

use std::process::Command;

use crate::config::is_allowed_youtube_url;
use crate::player::queue::Track;
use crate::ui::state::MixPlaylist;
use crate::youtube::extractor;

/// Fetch tracks from a playlist URL using yt-dlp.
pub(crate) fn fetch_playlist_tracks(
    playlist_url: &str,
    cookie_config: Option<(bool, String)>,
) -> Result<Vec<Track>, String> {
    if !is_allowed_youtube_url(playlist_url) {
        return Err("Invalid URL: must be a YouTube or YouTube Music URL".to_string());
    }

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--flat-playlist")
        .arg("--dump-json")
        .arg("--no-warnings")
        .arg("--socket-timeout")
        .arg("30")
        .arg("--retries")
        .arg("2");

    if let Some((_use_from_browser, cookie_arg)) = cookie_config {
        cmd.arg("--cookies-from-browser").arg(cookie_arg);
    }

    cmd.arg(playlist_url);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        eprintln!("yt-dlp playlist error: {}", error);
        return Err("yt-dlp failed — check logs for details".to_string());
    }

    let stdout = String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    let mut tracks = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            let video_id = json["id"].as_str().unwrap_or("").to_string();
            if !extractor::is_valid_video_id(&video_id) {
                continue;
            }

            let title = json["title"].as_str().unwrap_or("Unknown").to_string();
            let duration = json["duration"].as_u64().unwrap_or(0);
            let uploader = json["uploader"]
                .as_str()
                .or_else(|| json["channel"].as_str())
                .unwrap_or("Unknown")
                .to_string();
            let url = format!("https://www.youtube.com/watch?v={}", video_id);

            tracks.push(Track::new(video_id, title, duration, uploader, url));
        }
    }

    Ok(tracks)
}

/// Fetch My Mix playlists from YouTube Music home page.
pub(crate) fn fetch_my_mix(
    cookie_config: Option<(bool, String)>,
) -> Result<Vec<MixPlaylist>, String> {
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--flat-playlist")
        .arg("--dump-json")
        .arg("--no-warnings")
        .arg("--skip-download")
        .arg("--socket-timeout")
        .arg("30")
        .arg("--retries")
        .arg("2");

    if let Some((_use_from_browser, cookie_arg)) = cookie_config {
        cmd.arg("--cookies-from-browser").arg(cookie_arg);
    }

    cmd.arg("https://music.youtube.com");

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        eprintln!("yt-dlp my mix error: {}", error);
        return Err("yt-dlp failed — check logs for details".to_string());
    }

    let stdout = String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    let mut playlists = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(entry_type) = json["_type"].as_str() {
                if entry_type == "playlist" || entry_type == "url" {
                    let playlist_id = json["id"]
                        .as_str()
                        .or_else(|| json["playlist_id"].as_str())
                        .unwrap_or("")
                        .to_string();

                    let title = json["title"]
                        .as_str()
                        .or_else(|| json["playlist_title"].as_str())
                        .unwrap_or("Untitled Mix")
                        .to_string();

                    let track_count = json["playlist_count"]
                        .as_u64()
                        .or_else(|| json["n_entries"].as_u64())
                        .unwrap_or(0) as usize;

                    let url = json["url"]
                        .as_str()
                        .or_else(|| json["webpage_url"].as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            format!("https://music.youtube.com/playlist?list={}", playlist_id)
                        });

                    if playlist_id.starts_with("RDCLAK")
                        || playlist_id.starts_with("RDAMPL")
                        || title.contains("Mix")
                        || title.contains("mix")
                    {
                        playlists.push(MixPlaylist {
                            title,
                            track_count,
                            url,
                        });
                    }
                }
            }
        }
    }

    Ok(playlists)
}
