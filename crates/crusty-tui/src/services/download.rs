//! Download manager for background audio file fetching and caching.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::config::{is_allowed_youtube_url, MAX_CONCURRENT_DOWNLOADS, TEMP_FILE_MAX_AGE_SECS};
use crate::player::queue::Track;

/// Result of a completed download: (video_id, Ok(file_path) | Err(error_message)).
pub(crate) type DownloadResult = (String, Result<String, String>);

/// Unified state for all download tracking, guarded by a single mutex.
struct DownloadState {
    downloaded_files: HashMap<String, String>,
    failed_downloads: HashMap<String, String>,
    active_count: usize,
    downloading_videos: HashSet<String>,
}

/// Manages background audio downloads with rate limiting and caching.
pub(crate) struct DownloadManager {
    state: Arc<Mutex<DownloadState>>,
    background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    download_tx: mpsc::UnboundedSender<DownloadResult>,
    download_rx: mpsc::UnboundedReceiver<DownloadResult>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        let (download_tx, download_rx) = mpsc::unbounded_channel();
        Self {
            state: Arc::new(Mutex::new(DownloadState {
                downloaded_files: HashMap::new(),
                failed_downloads: HashMap::new(),
                active_count: 0,
                downloading_videos: HashSet::new(),
            })),
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            download_tx,
            download_rx,
        }
    }

    /// Create a new DownloadManager with a pre-warmed cache of previously downloaded files.
    pub fn with_cache(cache: HashMap<String, String>) -> Self {
        let (download_tx, download_rx) = mpsc::unbounded_channel();
        Self {
            state: Arc::new(Mutex::new(DownloadState {
                downloaded_files: cache,
                failed_downloads: HashMap::new(),
                active_count: 0,
                downloading_videos: HashSet::new(),
            })),
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            download_tx,
            download_rx,
        }
    }

    /// Returns a snapshot of the current download cache for persistence.
    pub fn get_cache_snapshot(&self) -> HashMap<String, String> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .downloaded_files
            .clone()
    }

    /// Poll for a completed download without blocking.
    /// Also prunes finished background tasks to prevent unbounded growth.
    pub fn poll_completion(&mut self) -> Option<DownloadResult> {
        // Prune finished tasks on each poll cycle
        {
            let mut tasks = self
                .background_tasks
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            tasks.retain(|h| !h.is_finished());
        }
        self.download_rx.try_recv().ok()
    }

    /// Returns the number of currently active downloads.
    pub fn active_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active_count
    }

    /// Returns the number of cached (downloaded) files.
    pub fn cached_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .downloaded_files
            .len()
    }

    /// Returns true if the video is already cached.
    pub fn is_cached(&self, video_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .downloaded_files
            .contains_key(video_id)
    }

    /// Returns the cached file path for a video, if available.
    pub fn get_cached_file(&self, video_id: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .downloaded_files
            .get(video_id)
            .cloned()
    }

    /// Removes a video from the download cache (e.g., when file was deleted).
    pub fn remove_from_cache(&self, video_id: &str) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .downloaded_files
            .remove(video_id);
    }

    /// Spawn a background download for a track, respecting rate limits.
    /// `cookie_config` is optional browser cookie info: (use_from_browser, cookie_arg).
    /// Returns true if a download was actually spawned.
    pub fn spawn_download(&self, track: &Track, cookie_config: Option<(bool, String)>) -> bool {
        // Single lock acquisition for all precondition checks + slot claim
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

            if state.active_count >= MAX_CONCURRENT_DOWNLOADS {
                return false;
            }
            if state.downloaded_files.contains_key(&track.video_id) {
                return false;
            }
            if state.failed_downloads.contains_key(&track.video_id) {
                return false;
            }
            if state.downloading_videos.contains(&track.video_id) {
                return false;
            }

            // Atomically claim the download slot
            state.downloading_videos.insert(track.video_id.clone());
            state.active_count += 1;
        }

        let video_id = track.video_id.clone();
        let youtube_url = track.url.clone();
        let state = self.state.clone();
        let download_tx = self.download_tx.clone();

        let handle = tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                fetch_audio_url_blocking(&youtube_url, cookie_config)
            })
            .await;

            // Single lock for all post-download bookkeeping
            {
                let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                match &result {
                    Ok(Ok(file_path)) => {
                        st.downloaded_files
                            .insert(video_id.clone(), file_path.clone());
                    }
                    Ok(Err(e)) => {
                        st.failed_downloads.insert(video_id.clone(), e.clone());
                    }
                    Err(_) => {
                        st.failed_downloads.insert(
                            video_id.clone(),
                            "Download task failed unexpectedly".to_string(),
                        );
                    }
                }
                st.active_count = st.active_count.saturating_sub(1);
                st.downloading_videos.remove(&video_id);
            }

            // Send result outside the lock
            let send_result = match result {
                Ok(Ok(file_path)) => Ok(file_path),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Download task failed unexpectedly".to_string()),
            };
            let _ = download_tx.send((video_id, send_result));
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
            let max_age = Duration::from_secs(TEMP_FILE_MAX_AGE_SECS);

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

    if let Some((use_from_browser, cookie_arg)) = cookie_config {
        if use_from_browser {
            cmd.arg("--cookies-from-browser").arg(cookie_arg);
        }
    }

    cmd.arg(youtube_url);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        let snippet: String = error.chars().take(200).collect();
        return Err(format!("yt-dlp download failed: {}", snippet));
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
