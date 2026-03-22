//! Download manager for background audio file fetching and caching.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::config::{is_allowed_youtube_url, MAX_CONCURRENT_DOWNLOADS};
use crate::player::queue::Track;

/// Result of a completed download: (video_id, Ok(file_path) | Err(error_message)).
pub(crate) type DownloadResult = (String, Result<String, String>);

/// Manages background audio downloads with rate limiting and caching.
pub(crate) struct DownloadManager {
    downloaded_files: Arc<Mutex<HashMap<String, String>>>,
    failed_downloads: Arc<Mutex<HashMap<String, String>>>,
    active_downloads: Arc<Mutex<usize>>,
    downloading_videos: Arc<Mutex<HashSet<String>>>,
    background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    download_tx: mpsc::UnboundedSender<DownloadResult>,
    download_rx: mpsc::UnboundedReceiver<DownloadResult>,
}

impl DownloadManager {
    pub fn new() -> Self {
        let (download_tx, download_rx) = mpsc::unbounded_channel();
        Self {
            downloaded_files: Arc::new(Mutex::new(HashMap::new())),
            failed_downloads: Arc::new(Mutex::new(HashMap::new())),
            active_downloads: Arc::new(Mutex::new(0)),
            downloading_videos: Arc::new(Mutex::new(HashSet::new())),
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            download_tx,
            download_rx,
        }
    }

    /// Poll for a completed download without blocking.
    pub fn poll_completion(&mut self) -> Option<DownloadResult> {
        self.download_rx.try_recv().ok()
    }

    /// Returns the number of currently active downloads.
    pub fn active_count(&self) -> usize {
        *self
            .active_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Returns the number of cached (downloaded) files.
    pub fn cached_count(&self) -> usize {
        self.downloaded_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// Returns true if the video is already cached.
    pub fn is_cached(&self, video_id: &str) -> bool {
        self.downloaded_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(video_id)
    }

    /// Returns the cached file path for a video, if available.
    pub fn get_cached_file(&self, video_id: &str) -> Option<String> {
        self.downloaded_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(video_id)
            .cloned()
    }

    /// Removes a video from the download cache (e.g., when file was deleted).
    pub fn remove_from_cache(&self, video_id: &str) {
        self.downloaded_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(video_id);
    }

    /// Spawn a background download for a track, respecting rate limits.
    /// `cookie_config` is optional browser cookie info: (use_from_browser, cookie_arg).
    /// Returns true if a download was actually spawned.
    pub fn spawn_download(&self, track: &Track, cookie_config: Option<(bool, String)>) -> bool {
        let active_count = *self
            .active_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if active_count >= MAX_CONCURRENT_DOWNLOADS {
            return false;
        }

        let video_id = &track.video_id;

        // Skip if already downloaded
        if self
            .downloaded_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(video_id)
        {
            return false;
        }

        // Skip if download already failed
        if self
            .failed_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(video_id)
        {
            return false;
        }

        // Skip if already downloading
        {
            let mut downloading = self
                .downloading_videos
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if downloading.contains(video_id) {
                return false;
            }
            downloading.insert(video_id.clone());
        }

        // Increment active download counter
        *self
            .active_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner()) += 1;

        let video_id = track.video_id.clone();
        let youtube_url = track.url.clone();
        let downloaded_files = self.downloaded_files.clone();
        let failed_downloads = self.failed_downloads.clone();
        let active_downloads = self.active_downloads.clone();
        let downloading_videos = self.downloading_videos.clone();
        let download_tx = self.download_tx.clone();

        let handle = tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                fetch_audio_url_blocking(&youtube_url, cookie_config)
            })
            .await;

            match result {
                Ok(Ok(file_path)) => {
                    downloaded_files
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(video_id.clone(), file_path.clone());
                    let _ = download_tx.send((video_id.clone(), Ok(file_path)));
                }
                Ok(Err(e)) => {
                    failed_downloads
                        .lock()
                        .unwrap_or_else(|e2| e2.into_inner())
                        .insert(video_id.clone(), e.clone());
                    let _ = download_tx.send((video_id.clone(), Err(e)));
                }
                Err(e) => {
                    let error_msg = "Download task failed unexpectedly".to_string();
                    eprintln!("Download task join error: {}", e);
                    failed_downloads
                        .lock()
                        .unwrap_or_else(|e2| e2.into_inner())
                        .insert(video_id.clone(), error_msg.clone());
                    let _ = download_tx.send((video_id.clone(), Err(error_msg)));
                }
            }

            // Decrement active download count and remove from in-flight tracker
            {
                let mut count = active_downloads.lock().unwrap_or_else(|e| e.into_inner());
                *count = count.saturating_sub(1);
            }
            downloading_videos
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&video_id);
        });

        // Track the background task, pruning finished ones
        {
            let mut tasks = self
                .background_tasks
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            tasks.retain(|h| !h.is_finished());
            tasks.push(handle);
        }

        true
    }

    /// Download tracks near the current queue position for instant playback.
    pub fn ensure_next_tracks_ready(
        &self,
        queue_slice: &[&Track],
        cookie_config: Option<(bool, String)>,
    ) {
        for track in queue_slice {
            self.spawn_download(track, cookie_config.clone());
        }
    }

    /// Download a single track at a lookahead offset for the sliding window strategy.
    pub fn trigger_hover_download(
        &self,
        queue_slice: &[&Track],
        cookie_config: Option<(bool, String)>,
    ) {
        if let Some(&track) = queue_slice.first() {
            self.spawn_download(track, cookie_config);
        }
    }

    /// Abort all background download tasks.
    pub fn abort_all(&self) {
        let mut tasks = self
            .background_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for handle in tasks.drain(..) {
            handle.abort();
        }
    }

    /// Clean up old pre-downloaded files from the temp directory.
    pub fn cleanup_old_downloads() {
        use std::env;
        use std::time::{Duration, SystemTime};

        let temp_dir = env::temp_dir();

        if let Ok(entries) = std::fs::read_dir(&temp_dir) {
            let now = SystemTime::now();
            let max_age = Duration::from_secs(3600);

            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    if file_name.starts_with("yt-music-audio-") {
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if let Ok(age) = now.duration_since(modified) {
                                    if age > max_age {
                                        let _ = std::fs::remove_file(entry.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Download audio to a temp file using yt-dlp.
fn fetch_audio_url_blocking(
    youtube_url: &str,
    cookie_config: Option<(bool, String)>,
) -> Result<String, String> {
    use std::env;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    if !is_allowed_youtube_url(youtube_url) {
        return Err("Invalid URL: must be a YouTube or YouTube Music URL".to_string());
    }

    let temp_dir = env::temp_dir();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let temp_file = temp_dir.join(format!(
        "yt-music-audio-{}-{}.%(ext)s",
        std::process::id(),
        timestamp
    ));

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-f")
        .arg("bestaudio/best")
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg("--audio-quality")
        .arg("192K")
        .arg("-o")
        .arg(&temp_file)
        .arg("--no-playlist")
        .arg("--no-mtime")
        .arg("--socket-timeout")
        .arg("30")
        .arg("--retries")
        .arg("2");

    if let Some((_use_from_browser, cookie_arg)) = cookie_config {
        cmd.arg("--cookies-from-browser").arg(cookie_arg);
    }

    cmd.arg(youtube_url);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        eprintln!("yt-dlp download error: {}", error);
        return Err("yt-dlp download failed — check logs for details".to_string());
    }

    // Find the downloaded file (yt-dlp replaces %(ext)s with actual extension)
    let temp_dir_path = env::temp_dir();
    let search_pattern = format!("yt-music-audio-{}-{}", std::process::id(), timestamp);

    let files: Vec<_> = std::fs::read_dir(&temp_dir_path)
        .map_err(|e| format!("Failed to read temp dir: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&search_pattern)
        })
        .collect();

    if files.is_empty() {
        return Err(format!(
            "yt-dlp completed but no audio file found (searched for {}.*)",
            search_pattern
        ));
    }

    let downloaded_file = files[0].path();

    // Canonicalize and verify the file is within the temp directory
    let canonical = downloaded_file
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize downloaded file path: {}", e))?;
    let temp_canonical = temp_dir_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize temp dir: {}", e))?;
    if !canonical.starts_with(&temp_canonical) {
        return Err("Downloaded file path is outside temp directory".to_string());
    }

    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("Failed to check downloaded file: {}", e))?;

    if metadata.len() == 0 {
        return Err("Downloaded file is empty".to_string());
    }

    if metadata.len() < 10000 {
        return Err(format!(
            "Downloaded file is too small ({} bytes), likely incomplete",
            metadata.len()
        ));
    }

    Ok(canonical.to_string_lossy().to_string())
}
