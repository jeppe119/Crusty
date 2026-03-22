//! Playback orchestration methods for MusicPlayerApp.
//!
//! Handles play/pause/seek/volume and the centralized cache-or-download logic.

use crate::config::{format_time, is_allowed_youtube_url, LOOKAHEAD_DOWNLOAD_COUNT};
use crate::player::audio::PlayerState;
use crate::player::queue::Track;
use crate::services::persistence::MAX_HISTORY_SIZE;

use super::app::MusicPlayerApp;

impl MusicPlayerApp {
    pub(super) async fn play_next(&mut self) {
        // CRITICAL: Clear pending state FIRST so navigation always works
        self.pending_play_track = None;
        self.currently_downloading = None;

        if let Some(track) = self.queue.next() {
            self.queue.limit_history(MAX_HISTORY_SIZE);
            self.play_track_from_cache_or_download(&track);
        } else {
            self.status_message = "Queue is empty!".to_string();
        }
    }

    pub(super) async fn play_previous(&mut self) {
        // CRITICAL: Clear pending state FIRST so navigation always works
        self.pending_play_track = None;
        self.currently_downloading = None;

        if let Some(track) = self.queue.previous() {
            self.queue.limit_history(MAX_HISTORY_SIZE);
            self.play_track_from_cache_or_download(&track);
        } else {
            self.status_message = "No previous track!".to_string();
        }
    }

    // ==========================================
    // CENTRALIZED PLAY-FROM-CACHE-OR-DOWNLOAD
    // ==========================================
    // Single method that handles the "check cache -> play or download" logic.
    // All play methods (play_next, play_previous, play_selected_queue_track,
    // play_current_or_first) delegate to this to avoid duplication.
    pub(super) fn play_track_from_cache_or_download(&mut self, track: &Track) {
        let cached_file = self.downloads.get_cached_file(&track.video_id);

        if let Some(local_file) = cached_file {
            if std::path::Path::new(&local_file).exists() {
                self.player
                    .play_with_duration(&local_file, &track.title, track.duration as f64);
                self.status_message.clear();
                let next = self.queue.get_queue_slice(0, LOOKAHEAD_DOWNLOAD_COUNT);
                self.downloads
                    .ensure_next_tracks_ready(&next, self.cookie_config());
            } else {
                self.downloads.remove_from_cache(&track.video_id);
                self.pending_play_track = Some(track.clone());
                let cookie = self.cookie_config();
                if self.downloads.spawn_download(track, cookie) {
                    self.currently_downloading = Some(track.title.clone());
                }
            }
        } else if is_allowed_youtube_url(&track.url) {
            self.pending_play_track = Some(track.clone());
            let cookie = self.cookie_config();
            if self.downloads.spawn_download(track, cookie) {
                self.currently_downloading = Some(track.title.clone());
            }
        } else {
            self.status_message = "Cannot play non-YouTube URL".to_string();
        }
    }

    pub(super) fn spawn_download_with_limit(&self, track: &Track) -> bool {
        self.downloads.spawn_download(track, self.cookie_config())
    }

    pub(super) fn trigger_smart_downloads(&self) {
        let next_tracks = self.queue.get_queue_slice(0, LOOKAHEAD_DOWNLOAD_COUNT);
        self.downloads
            .ensure_next_tracks_ready(&next_tracks, self.cookie_config());
    }

    pub(super) async fn toggle_pause_or_start(&mut self) {
        // SMART SPACE BAR:
        // 1. If in expanded queue -> play SELECTED track
        // 2. If nothing playing and queue has tracks -> START FIRST TRACK
        // 3. If player Stopped but track exists -> RELOAD and play current track
        // 4. If something playing -> toggle pause/resume

        if self.ui.queue_expanded && !self.queue.is_empty() {
            // In expanded queue - play the SELECTED track!
            self.play_selected_queue_track().await;
        } else if self.queue.get_current().is_none() && !self.queue.is_empty() {
            // Nothing playing but queue has tracks - START PLAYING!
            self.play_current_or_first().await;
        } else if self.player.get_state() == PlayerState::Stopped
            && self.queue.get_current().is_some()
        {
            // Player stopped but track exists in queue - RELOAD IT!
            self.play_current_or_first().await;
        } else {
            // Normal pause/resume
            self.player.toggle_pause();
        }
    }

    pub(super) async fn play_selected_queue_track(&mut self) {
        // CRITICAL: Clear pending state FIRST
        self.pending_play_track = None;
        self.currently_downloading = None;

        // Play the track at selected_queue_item index
        let queue_list = self.queue.get_queue_list();

        if self.ui.selected_queue_item >= queue_list.len() {
            self.status_message = "Invalid selection".to_string();
            return;
        }

        let track = queue_list[self.ui.selected_queue_item].clone();

        // Remove all tracks before and including selected from queue
        // This makes the selected track the "current" one
        for _ in 0..=self.ui.selected_queue_item {
            self.queue.remove_at(0);
        }

        // Set as current track
        self.queue
            .restore_queue(self.queue.get_queue_list(), Some(track.clone()));

        // Now play it
        self.play_track_from_cache_or_download(&track);

        // Collapse queue after selection
        self.ui.queue_expanded = false;
        self.ui.selected_queue_item = 0;
    }

    pub(super) async fn play_current_or_first(&mut self) {
        // CRITICAL: Clear pending state FIRST
        self.pending_play_track = None;
        self.currently_downloading = None;

        // Play whatever is current, or start first track if nothing current
        let track = if let Some(current) = self.queue.get_current().cloned() {
            // Already have a current track, just play it
            current
        } else if let Some(first_track) = self.queue.start_or_next() {
            // No current track, get first from queue
            first_track
        } else {
            self.status_message = "Queue is empty!".to_string();
            return;
        };

        // Play the track using centralized cache-or-download logic
        self.play_track_from_cache_or_download(&track);
    }

    pub(super) fn volume_up(&mut self, has_shift: bool) {
        let current = self.player.get_volume();
        let increment = if has_shift { 5 } else { 1 };
        if current < 100 {
            self.player.set_volume((current + increment).min(100));
        }
    }

    pub(super) fn volume_down(&mut self, has_shift: bool) {
        let current = self.player.get_volume();
        let decrement = if has_shift { 5 } else { 1 };
        if current > 0 {
            self.player.set_volume(current.saturating_sub(decrement));
        }
    }

    pub(super) fn seek_forward(&mut self) {
        self.player.seek_relative(10.0);
        self.player.apply_seek();
        self.status_message = format!("Seeked +10s ({})", format_time(self.player.get_time_pos()));
    }

    pub(super) fn seek_backward(&mut self) {
        self.player.seek_relative(-10.0);
        self.player.apply_seek();
        self.status_message = format!("Seeked -10s ({})", format_time(self.player.get_time_pos()));
    }

    /// Resume playback from saved state if a matching cached file exists.
    pub(super) fn try_resume_playback(&mut self) {
        let Some(saved) = self.persistence.load_playback_state() else {
            return;
        };

        // Always restore volume, even if track can't be resumed
        self.player.set_volume(saved.volume);

        // If no track was playing, just restore volume
        if saved.video_id.is_empty() {
            return;
        }

        // Check if the track is still in the download cache
        let Some(file_path) = self.downloads.get_cached_file(&saved.video_id) else {
            return;
        };

        // Verify the file still exists on disk
        if !std::path::Path::new(&file_path).exists() {
            return;
        }

        // Play the track from cache
        self.player
            .play_with_duration(&file_path, &saved.title, saved.duration);

        // Seek to the saved position
        if saved.position_secs > 1.0 {
            self.player.seek(saved.position_secs);
            self.player.apply_seek();
        }

        self.status_message = format!(
            "Resumed '{}' at {} (vol {}%)",
            saved.title,
            format_time(saved.position_secs),
            saved.volume
        );

        // Clear the saved state so it doesn't re-trigger
        self.persistence.clear_playback_state();
    }
}
