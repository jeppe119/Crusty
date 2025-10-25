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

pub struct YouTubeExtractor {
    // No state needed - all operations are stateless subprocess calls
}

impl YouTubeExtractor {
    pub fn new() -> Self {
        YouTubeExtractor {}
    }

    pub async fn get_audio_url(&self, video_url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new("yt-dlp")
            .arg("--get-url")
            .arg("-f")
            .arg("bestaudio")
            .arg(video_url)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp failed: {}", error).into());
        }

        let url = String::from_utf8(output.stdout)?
            .trim()
            .to_string();

        Ok(url)
    }

    pub async fn get_video_info(&self, video_url: &str) -> Result<VideoInfo, Box<dyn std::error::Error>> {
        let output = Command::new("yt-dlp")
            .arg("-j")
            .arg("--no-playlist")
            .arg(video_url)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp failed: {}", error).into());
        }

        let json_str = String::from_utf8(output.stdout)?;
        let json: serde_json::Value = serde_json::from_str(&json_str)?;

        let audio_url_output = Command::new("yt-dlp")
            .arg("--get-url")
            .arg("-f")
            .arg("bestaudio")
            .arg(video_url)
            .output()?;

        let audio_url = String::from_utf8(audio_url_output.stdout)?
            .trim()
            .to_string();

        Ok(VideoInfo {
            id: json["id"].as_str().unwrap_or("").to_string(),
            title: json["title"].as_str().unwrap_or("Unknown").to_string(),
            duration: json["duration"].as_u64().unwrap_or(0),
            uploader: json["uploader"].as_str().unwrap_or("Unknown").to_string(),
            thumbnail: json["thumbnail"].as_str().map(|s| s.to_string()),
            url: audio_url,
        })
    }

    pub async fn search(&self, query: &str, max_results: usize) -> Result<Vec<VideoInfo>, String> {
        // Run yt-dlp search in a blocking task to avoid blocking async runtime
        let query = query.to_string();
        let results = tokio::task::spawn_blocking(move || {
            let output = Command::new("yt-dlp")
                .arg("--dump-json")
                .arg("--skip-download")
                .arg("--no-playlist")
                .arg("--default-search")
                .arg("ytsearch")
                .arg(format!("ytsearch{}:{}", max_results, query))
                .output()
                .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(format!("yt-dlp search failed: {}", error));
            }

            let stdout = String::from_utf8(output.stdout)
                .map_err(|e| format!("Invalid UTF-8: {}", e))?;
            let mut results = Vec::new();

            for line in stdout.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                let json: serde_json::Value = serde_json::from_str(line)
                    .map_err(|e| format!("JSON parse error: {}", e))?;

                let video_id = json["id"].as_str().unwrap_or("").to_string();

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
        }).await.map_err(|e| format!("Task join error: {}", e))??;

        Ok(results)
    }
}
