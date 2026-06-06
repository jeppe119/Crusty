//! Persistence service for saving and loading history and queue state.

use std::collections::HashMap;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config;
use crate::player::queue::Track;
use crate::ui::state::QueueState;

/// Maximum file size in bytes (10 MB).
pub(crate) const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of entries allowed in a persisted collection.
const MAX_ENTRY_COUNT: usize = 10_000;

/// Maximum number of history entries to persist.
pub(crate) const MAX_HISTORY_SIZE: usize = 100;

// ---------------------------------------------------------------------------
// Atomic write helper
// ---------------------------------------------------------------------------

/// Atomically write `bytes` to `path` with 0o600 permissions on Unix.
///
/// Writes to a temporary file in the same directory, then renames it into
/// place. On POSIX systems `rename(2)` is atomic within the same filesystem,
/// so readers see either the old file or the new one — never a torn write.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("path has no parent directory")?;
    fs::create_dir_all(dir).context("Failed to create config directory")?;

    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).context("Failed to create temp file for atomic write")?;

    tmp.write_all(bytes)
        .context("Failed to write bytes to temp file")?;

    // Best-effort fsync — ensures data reaches disk before the rename.
    let _ = tmp.as_file().sync_all();

    // Set 0o600 on the temp file *before* the rename so the final path
    // never briefly has world-readable permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o600));
    }

    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("Failed to atomically replace {}: {}", path.display(), e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// PersistenceService
// ---------------------------------------------------------------------------

/// Handles reading and writing history/queue state to disk.
pub(crate) struct PersistenceService {
    config_dir: PathBuf,
}

impl PersistenceService {
    pub(crate) fn new() -> Result<Self> {
        let config_dir = config::config_dir()?;
        Ok(Self { config_dir })
    }

    pub(crate) fn from_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub(crate) fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    // -- History --------------------------------------------------------

    pub(crate) fn load_history(&self) -> Result<Vec<Track>> {
        let path = self.config_dir.join("history.json");

        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("Failed to open history file"),
        };

        let metadata = file.metadata().context("Failed to stat history file")?;
        if metadata.len() > MAX_FILE_SIZE {
            anyhow::bail!("History file too large ({} bytes)", metadata.len());
        }

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .context("Failed to read history file")?;
        let mut history: Vec<Track> =
            serde_json::from_str(&contents).context("Failed to parse history file")?;

        if history.len() > MAX_ENTRY_COUNT {
            anyhow::bail!("History file contains too many entries ({})", history.len());
        }

        // Strip fields that should not be restored from disk
        for track in &mut history {
            track.local_file = None;
        }

        Ok(history)
    }

    pub(crate) fn save_history(&self, history: &[Track]) -> Result<()> {
        // Limit to most recent entries before serializing
        let to_save = if history.len() > MAX_HISTORY_SIZE {
            &history[history.len() - MAX_HISTORY_SIZE..]
        } else {
            history
        };

        let path = self.config_dir.join("history.json");
        let json = serde_json::to_string_pretty(to_save).context("Failed to serialize history")?;
        write_atomic(&path, json.as_bytes()).context("Failed to write history file")
    }

    // -- Queue ----------------------------------------------------------

    pub(crate) fn load_queue(&self) -> Result<QueueState> {
        let path = self.config_dir.join("queue.json");

        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(QueueState {
                    tracks: Vec::new(),
                    current_track: None,
                });
            }
            Err(e) => return Err(e).context("Failed to open queue file"),
        };

        let metadata = file.metadata().context("Failed to stat queue file")?;
        if metadata.len() > MAX_FILE_SIZE {
            anyhow::bail!("Queue file too large ({} bytes)", metadata.len());
        }

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .context("Failed to read queue file")?;
        let mut queue_state: QueueState =
            serde_json::from_str(&contents).context("Failed to parse queue file")?;

        if queue_state.tracks.len() > MAX_ENTRY_COUNT {
            anyhow::bail!(
                "Queue file contains too many entries ({})",
                queue_state.tracks.len()
            );
        }

        // Strip local_file paths that should not survive a restart
        for track in &mut queue_state.tracks {
            track.local_file = None;
        }
        if let Some(ref mut t) = queue_state.current_track {
            t.local_file = None;
        }

        Ok(queue_state)
    }

    pub(crate) fn save_queue(&self, state: &QueueState) -> Result<()> {
        let path = self.config_dir.join("queue.json");
        let json = serde_json::to_string_pretty(state).context("Failed to serialize queue")?;
        write_atomic(&path, json.as_bytes()).context("Failed to write queue file")
    }

    // -- Download cache -------------------------------------------------

    /// Load the download cache (video_id → file_path), filtering out entries
    /// whose files no longer exist on disk.
    pub(crate) fn load_download_cache(&self) -> HashMap<String, String> {
        let path = self.config_dir.join("download_cache.json");

        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return HashMap::new(),
        };

        let metadata = match file.metadata() {
            Ok(m) if m.len() <= MAX_FILE_SIZE => m,
            _ => return HashMap::new(),
        };
        let _ = metadata;

        let mut contents = String::new();
        if file.read_to_string(&mut contents).is_err() {
            return HashMap::new();
        }

        let cache: HashMap<String, String> = match serde_json::from_str(&contents) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // Only keep entries where the file still exists
        cache
            .into_iter()
            .filter(|(_, path)| std::path::Path::new(path).exists())
            .collect()
    }

    /// Save the download cache to disk.
    pub(crate) fn save_download_cache(&self, cache: &HashMap<String, String>) -> Result<()> {
        let path = self.config_dir.join("download_cache.json");
        let json = serde_json::to_string(cache).context("Failed to serialize download cache")?;
        write_atomic(&path, json.as_bytes()).context("Failed to write download cache")
    }

    // -- Playback state (resume position) -----------------------------------

    /// Save the current playback position so it can be resumed on restart.
    pub(crate) fn save_playback_state(&self, state: &PlaybackState) -> Result<()> {
        let path = self.config_dir.join("playback_state.json");
        let json =
            serde_json::to_string(state).context("Failed to serialize playback state")?;
        write_atomic(&path, json.as_bytes()).context("Failed to write playback state")
    }

    /// Load the saved playback state, if any.
    pub(crate) fn load_playback_state(&self) -> Option<PlaybackState> {
        let path = self.config_dir.join("playback_state.json");
        let data = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Remove the saved playback state (e.g. when track finishes naturally).
    pub(crate) fn clear_playback_state(&self) {
        let path = self.config_dir.join("playback_state.json");
        let _ = fs::remove_file(path);
    }
}

/// Saved playback position for resume-on-restart.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct PlaybackState {
    pub video_id: String,
    pub position_secs: f64,
    pub title: String,
    pub duration: f64,
    #[serde(default = "default_volume")]
    pub volume: u32,
}

fn default_volume() -> u32 {
    100
}

// -- History search/filter ----------------------------------------------

/// Case-insensitive substring search across title and uploader fields.
#[allow(dead_code)]
pub(crate) fn search_history<'a>(history: &'a [Track], query: &str) -> Vec<&'a Track> {
    if query.is_empty() {
        return history.iter().collect();
    }
    let query_lower = query.to_lowercase();
    history
        .iter()
        .filter(|t| {
            t.title.to_lowercase().contains(&query_lower)
                || t.uploader.to_lowercase().contains(&query_lower)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_track(id: &str, title: &str, uploader: &str) -> Track {
        Track::new(
            id.to_string(),
            title.to_string(),
            120,
            uploader.to_string(),
            format!("https://www.youtube.com/watch?v={id}"),
        )
    }

    fn service_in(dir: &std::path::Path) -> PersistenceService {
        PersistenceService::from_dir(dir.to_path_buf())
    }

    // -- write_atomic tests --

    #[test]
    fn write_atomic_creates_file_with_correct_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.json");

        write_atomic(&path, b"{\"key\":\"value\"}").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "{\"key\":\"value\"}");
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.json");

        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn write_atomic_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dir").join("test.json");

        write_atomic(&path, b"hello").unwrap();

        assert!(path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_sets_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("secret.json");

        write_atomic(&path, b"secret").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        // Only owner read/write (0o600); mask off file-type bits
        assert_eq!(mode & 0o777, 0o600);
    }

    // -- History round-trip tests --

    #[test]
    fn save_and_load_history_round_trip() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let tracks = vec![
            make_track("a", "Song A", "Artist A"),
            make_track("b", "Song B", "Artist B"),
        ];

        svc.save_history(&tracks).unwrap();
        let loaded = svc.load_history().unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].video_id, "a");
        assert_eq!(loaded[1].video_id, "b");
    }

    #[test]
    fn load_history_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let loaded = svc.load_history().unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_history_limits_to_max_size() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let tracks: Vec<Track> = (0..150)
            .map(|i| make_track(&i.to_string(), &format!("Song {i}"), "Artist"))
            .collect();

        svc.save_history(&tracks).unwrap();
        let loaded = svc.load_history().unwrap();

        assert_eq!(loaded.len(), MAX_HISTORY_SIZE);
        assert_eq!(loaded[0].video_id, "50");
        assert_eq!(loaded[99].video_id, "149");
    }

    #[test]
    fn load_history_rejects_oversized_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.json");

        // Write directly (bypassing write_atomic) to create an oversized file
        let big_content = "x".repeat(11 * 1024 * 1024);
        fs::write(&path, big_content).unwrap();

        let svc = service_in(tmp.path());
        let result = svc.load_history();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn load_history_strips_local_file() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let mut track = make_track("a", "Song A", "Artist A");
        track.local_file = Some("/tmp/evil.mp3".to_string());

        svc.save_history(&[track]).unwrap();
        let loaded = svc.load_history().unwrap();

        assert!(loaded[0].local_file.is_none());
    }

    // -- Queue round-trip tests --

    #[test]
    fn save_and_load_queue_round_trip() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let state = QueueState {
            tracks: vec![
                make_track("a", "Song A", "Artist A"),
                make_track("b", "Song B", "Artist B"),
            ],
            current_track: Some(make_track("c", "Current", "Artist C")),
        };

        svc.save_queue(&state).unwrap();
        let loaded = svc.load_queue().unwrap();

        assert_eq!(loaded.tracks.len(), 2);
        assert_eq!(loaded.tracks[0].video_id, "a");
        assert!(loaded.current_track.is_some());
        assert_eq!(loaded.current_track.unwrap().video_id, "c");
    }

    #[test]
    fn load_queue_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let loaded = svc.load_queue().unwrap();
        assert!(loaded.tracks.is_empty());
        assert!(loaded.current_track.is_none());
    }

    #[test]
    fn load_queue_rejects_oversized_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("queue.json");

        let big_content = "x".repeat(11 * 1024 * 1024);
        fs::write(&path, big_content).unwrap();

        let svc = service_in(tmp.path());
        let result = svc.load_queue();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn load_queue_strips_local_file() {
        let tmp = TempDir::new().unwrap();
        let svc = service_in(tmp.path());

        let mut track = make_track("a", "Song A", "Artist A");
        track.local_file = Some("/tmp/evil.mp3".to_string());

        let state = QueueState {
            tracks: vec![track],
            current_track: None,
        };

        svc.save_queue(&state).unwrap();
        let loaded = svc.load_queue().unwrap();

        assert!(loaded.tracks[0].local_file.is_none());
    }

    // -- search_history tests --

    #[test]
    fn search_history_matches_title() {
        let tracks = vec![
            make_track("a", "Never Gonna Give You Up", "Rick Astley"),
            make_track("b", "Bohemian Rhapsody", "Queen"),
            make_track("c", "Give Me Everything", "Pitbull"),
        ];

        let results = search_history(&tracks, "give");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].video_id, "a");
        assert_eq!(results[1].video_id, "c");
    }

    #[test]
    fn search_history_matches_uploader() {
        let tracks = vec![
            make_track("a", "Song A", "Rick Astley"),
            make_track("b", "Song B", "Queen"),
        ];

        let results = search_history(&tracks, "astley");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].video_id, "a");
    }

    #[test]
    fn search_history_case_insensitive() {
        let tracks = vec![make_track("a", "HELLO World", "Artist")];

        let results = search_history(&tracks, "hello");
        assert_eq!(results.len(), 1);

        let results = search_history(&tracks, "WORLD");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_history_empty_query_returns_all() {
        let tracks = vec![
            make_track("a", "Song A", "Artist A"),
            make_track("b", "Song B", "Artist B"),
        ];

        let results = search_history(&tracks, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_history_no_matches() {
        let tracks = vec![make_track("a", "Song A", "Artist A")];

        let results = search_history(&tracks, "zzzzz");
        assert!(results.is_empty());
    }
}
