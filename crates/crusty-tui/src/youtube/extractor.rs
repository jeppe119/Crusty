// YouTube audio stream extractor
// Uses rustube or yt-dlp subprocess to extract audio streams and metadata

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub duration: u64,
    pub uploader: String,
    pub thumbnail: Option<String>,
    pub url: String,
}

/// Returns true if a YouTube video ID contains only safe characters (alphanumeric, dash, underscore).
#[must_use]
pub fn is_valid_video_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 16
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

pub struct YouTubeExtractor {
    // No state needed - all operations are stateless subprocess calls
}

impl YouTubeExtractor {
    pub fn new() -> Self {
        YouTubeExtractor {}
    }

    pub async fn search(&self, query: &str, max_results: usize) -> Result<Vec<VideoInfo>, String> {
        // Run yt-dlp search in a blocking task to avoid blocking async runtime
        // Sanitize query: strip leading dashes to prevent yt-dlp flag injection
        let sanitized = query.trim().trim_start_matches('-').to_string();
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        let results = tokio::task::spawn_blocking(move || {
            let output = Command::new("yt-dlp")
                .arg("--dump-json")
                .arg("--skip-download")
                .arg("--no-playlist")
                .arg("--default-search")
                .arg("ytsearch")
                .arg("--socket-timeout")
                .arg("30")
                .arg("--retries")
                .arg("2")
                .arg(format!("ytsearch{}:{}", max_results, sanitized))
                .output()
                .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                eprintln!("yt-dlp search error: {}", error);
                return Err("yt-dlp search failed — check logs for details".to_string());
            }

            let stdout =
                String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;
            let mut results = Vec::new();

            for line in stdout.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                let json: serde_json::Value =
                    serde_json::from_str(line).map_err(|e| format!("JSON parse error: {}", e))?;

                let video_id = json["id"].as_str().unwrap_or("").to_string();

                // Skip entries with invalid or empty video IDs
                if !is_valid_video_id(&video_id) {
                    continue;
                }

                // Don't fetch audio URL here - it's slow and URLs expire
                // We'll fetch it on-demand when user actually plays the track
                let placeholder_url = format!("https://www.youtube.com/watch?v={}", video_id);

                results.push(VideoInfo {
                    id: video_id,
                    title: json["title"].as_str().unwrap_or("Unknown").to_string(),
                    duration: json["duration"].as_u64().unwrap_or(0),
                    uploader: json["uploader"].as_str().unwrap_or("Unknown").to_string(),
                    thumbnail: json["thumbnail"].as_str().map(|s| s.to_string()),
                    url: placeholder_url,
                });
            }

            Ok::<Vec<VideoInfo>, String>(results)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

        Ok(results)
    }
}
