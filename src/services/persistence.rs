//! Persistence service for saving and loading history and queue state.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config;
use crate::player::queue::Track;
use crate::ui::state::QueueState;

/// Maximum file size in bytes (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of entries allowed in a persisted collection.
const MAX_ENTRY_COUNT: usize = 10_000;

/// Maximum number of history entries to persist.
const MAX_HISTORY_SIZE: usize = 100;

/// Handles reading and writing history/queue state to disk.
pub(crate) struct PersistenceService {
    config_dir: PathBuf,
}

impl PersistenceService {
    pub fn new() -> Result<Self> {
        let config_dir = config::config_dir()?;
        Ok(Self { config_dir })
    }

    // -- History --------------------------------------------------------

    pub fn load_history(&self) -> Result<Vec<Track>> {
        let path = self.config_dir.join("history.json");

        if !path.exists() {
            return Ok(Vec::new());
        }

        let metadata = fs::metadata(&path).context("Failed to stat history file")?;
        if metadata.len() > MAX_FILE_SIZE {
            anyhow::bail!("History file too large ({} bytes)", metadata.len());
        }

        let contents = fs::read_to_string(&path).context("Failed to read history file")?;
        let history: Vec<Track> =
            serde_json::from_str(&contents).context("Failed to parse history file")?;

        if history.len() > MAX_ENTRY_COUNT {
            anyhow::bail!("History file contains too many entries ({})", history.len());
        }

        Ok(history)
    }

    pub fn save_history(&self, history: &[Track]) -> Result<()> {
        fs::create_dir_all(&self.config_dir).context("Failed to create config directory")?;

        // Limit to most recent entries before serializing
        let to_save = if history.len() > MAX_HISTORY_SIZE {
            &history[history.len() - MAX_HISTORY_SIZE..]
        } else {
            history
        };

        let path = self.config_dir.join("history.json");
        let json = serde_json::to_string_pretty(to_save).context("Failed to serialize history")?;
        fs::write(path, json).context("Failed to write history file")?;

        Ok(())
    }

    // -- Queue ----------------------------------------------------------

    pub fn load_queue(&self) -> Result<QueueState> {
        let path = self.config_dir.join("queue.json");

        if !path.exists() {
            return Ok(QueueState {
                tracks: Vec::new(),
                current_track: None,
            });
        }

        let metadata = fs::metadata(&path).context("Failed to stat queue file")?;
        if metadata.len() > MAX_FILE_SIZE {
            anyhow::bail!("Queue file too large ({} bytes)", metadata.len());
        }

        let contents = fs::read_to_string(&path).context("Failed to read queue file")?;
        let queue_state: QueueState =
            serde_json::from_str(&contents).context("Failed to parse queue file")?;

        if queue_state.tracks.len() > MAX_ENTRY_COUNT {
            anyhow::bail!(
                "Queue file contains too many entries ({})",
                queue_state.tracks.len()
            );
        }

        Ok(queue_state)
    }

    pub fn save_queue(&self, state: &QueueState) -> Result<()> {
        fs::create_dir_all(&self.config_dir).context("Failed to create config directory")?;

        let path = self.config_dir.join("queue.json");
        let json = serde_json::to_string_pretty(state).context("Failed to serialize queue")?;
        fs::write(path, json).context("Failed to write queue file")?;

        Ok(())
    }
}

// -- History search/filter ----------------------------------------------

/// Case-insensitive substring search across title and uploader fields.
/// Will be wired to UI in the input handler extraction (Phase 6).
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
    use std::io::Write as _;
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
        PersistenceService {
            config_dir: dir.to_path_buf(),
        }
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
        // Should keep the most recent (50..150)
        assert_eq!(loaded[0].video_id, "50");
        assert_eq!(loaded[99].video_id, "149");
    }

    #[test]
    fn load_history_rejects_oversized_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.json");

        // Write a file larger than 10 MB
        let mut f = fs::File::create(&path).unwrap();
        let big_content = "x".repeat(11 * 1024 * 1024);
        f.write_all(big_content.as_bytes()).unwrap();

        let svc = service_in(tmp.path());
        let result = svc.load_history();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
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

        let mut f = fs::File::create(&path).unwrap();
        let big_content = "x".repeat(11 * 1024 * 1024);
        f.write_all(big_content.as_bytes()).unwrap();

        let svc = service_in(tmp.path());
        let result = svc.load_queue();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
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
