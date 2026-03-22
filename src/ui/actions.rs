//! Async action methods for MusicPlayerApp.
//!
//! Handles search, playlist loading, queue additions, and account management.

use crate::config::{clean_title, is_allowed_youtube_url, MAX_TRACK_DURATION_SECS};
use crate::player::queue::Track;
use crate::ui::state::AppMode;
use crate::youtube::extractor::YouTubeExtractor;

use super::app::MusicPlayerApp;

impl MusicPlayerApp {
    pub(super) async fn perform_search(&mut self, query: &str) {
        // Mark as searching
        self.search.is_searching = true;

        // Spawn background task for search
        let extractor = YouTubeExtractor::new();
        let query = query.to_string();
        let tx = self.search_tx.clone();

        tokio::spawn(async move {
            match extractor.search(&query, 15).await {
                Ok(results) => {
                    let _ = tx.send(results);
                }
                Err(_e) => {
                    // Send empty results to unblock UI (shows "Found 0 results")
                    let _ = tx.send(Vec::new());
                }
            }
        });
    }

    pub(super) async fn load_playlist_from_url(&mut self, url: &str) {
        // Validate URL is a known YouTube domain before passing to yt-dlp
        if !is_allowed_youtube_url(url) {
            self.status_message = "Invalid URL: must be a YouTube or YouTube Music URL".to_string();
            return;
        }

        self.status_message = "Loading playlist... (this may take a moment)".to_string();

        // Yield to allow UI to render the loading message before blocking fetch
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let cookie_config = self
            .browser_auth
            .load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account));

        let playlist_url = url.to_string();
        let fetch_result = tokio::task::spawn_blocking(move || {
            crate::services::playlist::fetch_playlist_tracks(&playlist_url, cookie_config)
        })
        .await;

        match fetch_result {
            Ok(Ok(tracks)) => {
                if tracks.is_empty() {
                    self.status_message = "No tracks found in playlist".to_string();
                    return;
                }

                let track_count = tracks.len();

                self.playlist.loaded_name = format!("Loaded Playlist ({} tracks)", track_count);

                // Add tracks to queue (filter long tracks in music-only mode)
                let mut added_count = 0;
                let mut filtered_count = 0;
                for track in &tracks {
                    if !self.ui.music_only_mode || track.duration <= MAX_TRACK_DURATION_SECS {
                        self.queue.add(track.clone());
                        added_count += 1;
                    } else {
                        filtered_count += 1;
                    }
                }

                // Store loaded playlist for display (moved after iteration to avoid clone)
                self.playlist.loaded_tracks = tracks;

                // Trigger smart downloads
                self.trigger_smart_downloads();

                if filtered_count > 0 {
                    self.status_message = format!(
                        "Added {} tracks to queue ({} filtered — press 'f' to allow all)",
                        added_count, filtered_count
                    );
                } else {
                    self.status_message = format!("Added {} tracks to queue", added_count);
                }
            }
            Ok(Err(e)) => {
                self.status_message = format!("Failed to fetch playlist: {}", e);
            }
            Err(e) => {
                self.status_message = format!("Task error: {}", e);
            }
        }
    }

    pub(super) async fn add_selected_mix_to_queue(&mut self) {
        if let Some(mix) = self
            .playlist
            .my_mix_playlists
            .get(self.ui.selected_mix_item)
            .cloned()
        {
            self.status_message = format!(
                "⏳ Fetching tracks from '{}'... (this may take a moment)",
                mix.title
            );

            // Yield to allow UI to render the loading message before blocking fetch
            tokio::task::yield_now().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let cookie_config = self
                .browser_auth
                .load_selected_account()
                .map(|account| self.browser_auth.get_cookie_arg(&account));

            let playlist_url = mix.url.clone();
            let fetch_result = tokio::task::spawn_blocking(move || {
                crate::services::playlist::fetch_playlist_tracks(&playlist_url, cookie_config)
            })
            .await;

            match fetch_result {
                Ok(Ok(tracks)) => {
                    if tracks.is_empty() {
                        self.status_message = format!("No tracks found in '{}'", mix.title);
                        return;
                    }

                    // Add tracks to queue (filter long tracks in music-only mode)
                    let mut added_count = 0;
                    let mut filtered_count = 0;
                    for track in tracks {
                        if !self.ui.music_only_mode || track.duration <= MAX_TRACK_DURATION_SECS {
                            self.queue.add(track);
                            added_count += 1;
                        } else {
                            filtered_count += 1;
                        }
                    }

                    // Trigger smart downloads
                    self.trigger_smart_downloads();

                    if filtered_count > 0 {
                        self.status_message = format!(
                            "Added {} from '{}' ({} filtered — press 'f' to allow all)",
                            added_count, mix.title, filtered_count
                        );
                    } else {
                        self.status_message =
                            format!("Added {} tracks from '{}' to queue", added_count, mix.title);
                    }

                    // Save queue to disk
                    if let Err(e) = self.save_queue() {
                        self.status_message =
                            format!("Added tracks but failed to save queue: {}", e);
                    }
                }
                Ok(Err(e)) => {
                    self.status_message = format!("Failed to fetch tracks: {}", e);
                }
                Err(e) => {
                    self.status_message = format!("Task error: {}", e);
                }
            }
        }
    }

    pub(super) async fn refresh_my_mix(&mut self) {
        self.status_message = "Refreshing My Mix playlists...".to_string();
        self.fetch_my_mix().await;
    }

    pub(super) async fn fetch_my_mix(&mut self) {
        // Fetch My Mix playlists using yt-dlp
        let cookie_config = self
            .browser_auth
            .load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account));

        let fetch_result = tokio::task::spawn_blocking(move || {
            crate::services::playlist::fetch_my_mix(cookie_config)
        })
        .await;

        match fetch_result {
            Ok(Ok(playlists)) => {
                if playlists.is_empty() {
                    self.status_message = "No My Mix playlists found".to_string();
                } else {
                    self.playlist.my_mix_playlists = playlists;
                    self.status_message = format!(
                        "Loaded {} My Mix playlists",
                        self.playlist.my_mix_playlists.len()
                    );
                }
            }
            Ok(Err(e)) => {
                self.status_message = format!("Failed to fetch My Mix: {}", e);
                // Keep existing playlists if any
            }
            Err(e) => {
                self.status_message = format!("Task error: {}", e);
            }
        }
    }

    pub(super) fn add_selected_to_queue(&mut self) {
        if let Some(video) = self.search.results.get(self.ui.selected_result) {
            // In music-only mode, filter out tracks > 5 minutes
            if self.ui.music_only_mode && video.duration > MAX_TRACK_DURATION_SECS {
                let clean_title = clean_title(&video.title);
                let mins = video.duration / 60;
                self.status_message = format!(
                    "'{}' is too long ({}min) — press 'f' to allow all content",
                    clean_title, mins
                );
                return;
            }

            let track = Track::new(
                video.id.clone(),
                video.title.clone(),
                video.duration,
                video.uploader.clone(),
                video.url.clone(),
            );

            let was_empty = self.queue.is_empty();

            // Start background download through centralized rate-limited system
            self.spawn_download_with_limit(&track);

            self.queue.add(track);

            // Show feedback
            let clean_title = clean_title(&video.title);
            self.status_message = format!(
                "Added '{}' to queue! Downloading in background... ({} total)",
                clean_title,
                self.queue.len()
            );

            if was_empty {
                self.status_message =
                    format!("Added '{}' to queue! Press 'n' to play", clean_title);
            }

            // Save queue to disk
            if let Err(e) = self.save_queue() {
                self.status_message = format!("Track added but failed to save queue: {}", e);
            }
        }
    }

    pub(super) async fn start_login(&mut self) {
        self.status_message = "Detecting YouTube accounts from browsers...".to_string();

        // Detect available accounts from Chrome/Firefox/Zen
        self.available_accounts = self.browser_auth.detect_accounts();

        if self.available_accounts.is_empty() {
            self.status_message =
                "No browser accounts found. Please login to YouTube in Chrome or Firefox first."
                    .to_string();
        } else {
            self.status_message = format!(
                "Found {} account(s). Select one:",
                self.available_accounts.len()
            );
            self.ui.selected_account_idx = 0;
            self.mode = AppMode::AccountPicker;
        }
    }

    pub(super) async fn select_account(&mut self) {
        if let Some(account) = self.available_accounts.get(self.ui.selected_account_idx) {
            match self.browser_auth.save_selected_account(account) {
                Ok(_) => {
                    self.status_message = format!(
                        "✓ Logged in as {} - Press '/' to search for music!",
                        account.display_name
                    );
                    self.mode = AppMode::Normal;
                }
                Err(e) => {
                    self.status_message = format!("Failed to save account: {}", e);
                }
            }
        }
    }
}
