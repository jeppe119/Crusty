//! Async action methods for MusicPlayerApp.
//!
//! Handles search, playlist loading, queue additions, account management,
//! and the YouTube Music feed browser.

use std::time::Duration;

use crate::config::{clean_title, is_allowed_youtube_url, FEED_CACHE_TTL_SECS, MAX_TRACK_DURATION_SECS};
use crate::player::queue::Track;
use crate::services::cache_store::CacheStore;
use crate::ui::state::{AppMode, FeedSection};
use crate::youtube::extractor::YouTubeExtractor;

use super::app::MusicPlayerApp;

impl MusicPlayerApp {
    // -----------------------------------------------------------------------
    // Shared helpers
    // -----------------------------------------------------------------------

    /// Add `tracks` to the queue, applying the music-only duration filter.
    ///
    /// Returns `(added, filtered)` counts. Does **not** trigger downloads —
    /// callers are responsible for calling `trigger_smart_downloads()`.
    fn add_filtered_tracks(&mut self, tracks: &[Track]) -> (usize, usize) {
        let mut added = 0;
        let mut filtered = 0;
        for track in tracks {
            if !self.ui.music_only_mode || track.duration <= MAX_TRACK_DURATION_SECS {
                self.queue.add(track.clone());
                added += 1;
            } else {
                filtered += 1;
            }
        }
        (added, filtered)
    }

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

                let (added_count, filtered_count) = self.add_filtered_tracks(&tracks);

                // Store loaded playlist for display
                self.playlist.loaded_tracks = tracks;

                // Trigger smart downloads
                self.trigger_smart_downloads();

                if filtered_count > 0 {
                    self.status_message = format!(
                        "Added {} tracks to queue ({} filtered — press 'Shift+F' to allow all)",
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

                    let (added_count, filtered_count) = self.add_filtered_tracks(&tracks);

                    // Trigger smart downloads
                    self.trigger_smart_downloads();

                    if filtered_count > 0 {
                        self.status_message = format!(
                            "Added {} from '{}' ({} filtered — press 'Shift+F' to allow all)",
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

    // -----------------------------------------------------------------------
    // Feed browser actions
    // -----------------------------------------------------------------------

    /// Open the feed browser.
    ///
    /// - If the in-memory feed is already populated, just switches mode (instant).
    /// - If the disk cache is fresh (< 30 min), loads it synchronously and
    ///   switches mode without spawning yt-dlp.
    /// - Otherwise triggers a background fetch via `refresh_feed(force: false)`.
    pub(super) async fn open_feed_browser(&mut self) {
        self.mode = AppMode::FeedBrowser;
        self.feed.selected_section = 0;
        self.feed.selected_item = 0;

        // Already have in-memory sections — nothing to do.
        if !self.feed.sections.is_empty() {
            return;
        }

        // Try the disk cache before spawning yt-dlp.
        if self.try_load_feed_cache() {
            return;
        }

        // Cache miss — fetch in background.
        if !self.feed.is_loading {
            self.refresh_feed(false).await;
        }
    }

    /// Try to load the feed from the on-disk `CacheStore`.
    ///
    /// Returns `true` and populates `feed.sections` if a fresh cache entry
    /// exists. Returns `false` on miss (expired, missing, corrupt, schema
    /// mismatch) without modifying state.
    fn try_load_feed_cache(&mut self) -> bool {
        let store = Self::feed_cache_store(self.persistence.config_dir().to_owned());
        match store.load() {
            Ok(Some(mut sections)) => {
                // Re-validate URLs loaded from disk — defence-in-depth matching
                // the pattern used for queue restoration in load_queue_async.
                for section in &mut sections {
                    section
                        .items
                        .retain(|p| crate::config::is_allowed_youtube_url(&p.url));
                }
                self.feed.sections = sections;
                self.feed.last_fetch = Some(std::time::Instant::now());
                self.feed.last_error = None;
                true
            }
            _ => false,
        }
    }

    /// Spawn an async parallel yt-dlp fetch for the full YouTube Music feed
    /// (home + library + liked). Results are delivered via `feed_tx` and
    /// drained in the `run()` loop, which also persists the result to disk.
    ///
    /// `force = true` is used when the user explicitly presses `r` — it
    /// bypasses the TTL check and always re-fetches.
    pub(super) async fn refresh_feed(&mut self, force: bool) {
        // Prevent stacking multiple concurrent yt-dlp fan-outs.
        if self.feed.is_loading {
            self.status_message = "Feed refresh already in progress…".to_string();
            return;
        }

        if self.cookie_config().is_none() {
            self.feed.last_error = Some(
                "No browser account selected. Press 'q' then 'l' to log in.".to_string(),
            );
            return;
        }

        // On a forced refresh, clear the disk cache so the next open_feed_browser
        // doesn't serve stale data.
        if force {
            let store = Self::feed_cache_store(self.persistence.config_dir().to_owned());
            store.invalidate();
            self.feed.sections.clear();
        }

        self.feed.is_loading = true;
        self.feed.last_error = None;
        self.status_message = "Fetching YouTube Music feed (liked songs + playlists)…".to_string();

        let cookie = self.cookie_config();
        let config_dir = self.persistence.config_dir().to_owned();
        let tx = self.feed_tx.clone();

        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                crate::services::feed::fetch_all_parallel(cookie)
                    .map_err(|e| e.user_message())
            })
            .await
            .unwrap_or_else(|e| Err(format!("Task error: {e}")));

            // Persist a successful result to disk before sending to the UI.
            if let Ok(ref sections) = result {
                let store = MusicPlayerApp::feed_cache_store(config_dir);
                let _ = store.save(sections);
            }

            let _ = tx.send(result);
        });
    }

    /// Build the `CacheStore` for the feed, rooted at `config_dir`.
    fn feed_cache_store(config_dir: std::path::PathBuf) -> CacheStore<Vec<FeedSection>> {
        CacheStore::new(
            config_dir.join("feed_cache.json"),
            Duration::from_secs(FEED_CACHE_TTL_SECS),
            FeedSection::CACHE_SCHEMA_VERSION,
        )
    }

    // Feed navigation helpers

    pub(super) fn feed_navigate_down(&mut self) {
        use crate::ui::state::FeedFocus;
        match self.feed.focus {
            FeedFocus::Tracks => {
                let max = self.feed.expanded_tracks.len().saturating_sub(1);
                if self.feed.selected_track < max {
                    self.feed.selected_track += 1;
                }
            }
            FeedFocus::Playlists => {
                // j navigates items within the selected section.
                let Some(section) = self.feed.sections.get(self.feed.selected_section) else {
                    return;
                };
                let max = section.items.len().saturating_sub(1);
                if self.feed.selected_item < max {
                    self.feed.selected_item += 1;
                } else {
                    // At the bottom of the section — move to the next section.
                    self.feed_next_section_tab();
                }
            }
        }
    }

    pub(super) fn feed_navigate_up(&mut self) {
        use crate::ui::state::FeedFocus;
        match self.feed.focus {
            FeedFocus::Tracks => {
                self.feed.selected_track = self.feed.selected_track.saturating_sub(1);
            }
            FeedFocus::Playlists => {
                // k navigates items within the selected section.
                if self.feed.selected_item > 0 {
                    self.feed.selected_item -= 1;
                } else {
                    // At the top of the section — move to the previous section.
                    self.feed_prev_section_tab();
                }
            }
        }
    }

    pub(super) fn feed_next_section(&mut self) {
        use crate::ui::state::FeedFocus;
        // l / → : collapse track view → playlist view (go left),
        // or when already in playlist view, do nothing (Enter expands).
        if self.feed.focus == FeedFocus::Tracks {
            self.feed.focus = FeedFocus::Playlists;
            self.feed.expanded_tracks.clear();
        }
        // In Playlists focus, l/→ is a no-op — use Enter to expand into tracks.
    }

    pub(super) fn feed_prev_section(&mut self) {
        use crate::ui::state::FeedFocus;
        // h / ← : collapse track view → playlist view (go left).
        if self.feed.focus == FeedFocus::Tracks {
            self.feed.focus = FeedFocus::Playlists;
            self.feed.expanded_tracks.clear();
        }
        // In Playlists focus, h/← is a no-op — use j/k to move between sections.
    }

    pub(super) fn feed_next_section_tab(&mut self) {
        // Move to the next section in the left column (j navigates down).
        if !self.feed.sections.is_empty() {
            let max = self.feed.sections.len() - 1;
            if self.feed.selected_section < max {
                self.feed.selected_section += 1;
                self.feed.selected_item = 0;
            }
        }
    }

    pub(super) fn feed_prev_section_tab(&mut self) {
        // Move to the previous section in the left column (k navigates up).
        if self.feed.selected_section > 0 {
            self.feed.selected_section -= 1;
            self.feed.selected_item = 0;
        }
    }

    /// Returns a clone of the currently highlighted `FeedPlaylist`, if any.
    pub(super) fn feed_selected_item(&self) -> Option<crate::ui::state::FeedPlaylist> {
        self.feed
            .sections
            .get(self.feed.selected_section)?
            .items
            .get(self.feed.selected_item)
            .cloned()
    }

    /// Returns a reference to the currently highlighted `FeedPlaylist`, if any.
    /// Used by the render path to avoid cloning.
    pub(crate) fn feed_selected_item_ref(&self) -> Option<&crate::ui::state::FeedPlaylist> {
        self.feed
            .sections
            .get(self.feed.selected_section)?
            .items
            .get(self.feed.selected_item)
    }

    /// Expand the selected playlist to show individual tracks.
    /// Switches focus to the Tracks pane once loaded.
    pub(super) async fn feed_expand_playlist(&mut self) {
        use crate::ui::state::FeedFocus;

        // If already in track view, Enter on a track = play that track
        if self.feed.focus == FeedFocus::Tracks {
            self.feed_play_selected_track().await;
            return;
        }

        let Some(item) = self.feed_selected_item() else {
            return;
        };

        self.feed.tracks_loading = true;
        self.feed.expanded_tracks.clear();
        self.feed.selected_track = 0;
        self.status_message = format!("Loading tracks from '{}'…", item.title);

        tokio::task::yield_now().await;

        let cookie = self.cookie_config();
        let url = item.url.clone();

        let result = tokio::task::spawn_blocking(move || {
            crate::services::feed::fetch_tracks_for_playlist(cookie, &url)
                .map_err(|e| e.user_message())
        })
        .await
        .unwrap_or_else(|e| Err(format!("Task error: {e}")));

        self.feed.tracks_loading = false;

        match result {
            Ok(tracks) if tracks.is_empty() => {
                self.status_message = format!("No tracks found in '{}'", item.title);
            }
            Ok(tracks) => {
                let count = tracks.len();
                self.feed.expanded_tracks = tracks;
                self.feed.focus = FeedFocus::Tracks;
                self.status_message = format!(
                    "{} tracks — j/k navigate, Enter play, a add, h/l back",
                    count
                );
            }
            Err(e) => {
                self.status_message = e;
            }
        }
    }

    /// Play the currently highlighted track in the expanded track list.
    async fn feed_play_selected_track(&mut self) {
        let Some(track) = self.feed.expanded_tracks.get(self.feed.selected_track).cloned() else {
            return;
        };

        let queue_track = crate::player::queue::Track::new(
            track.video_id.clone(),
            track.title.clone(),
            track.duration,
            track.uploader.clone(),
            track.url.clone(),
        );

        self.queue = crate::player::queue::Queue::new();
        self.queue.add(queue_track);
        self.trigger_smart_downloads();
        self.play_current_or_first().await;

        // Mark parent playlist as imported
        if let Some(item) = self.feed_selected_item() {
            self.feed.imported_ids.insert(item.id.clone());
        }

        self.feed.focus = crate::ui::state::FeedFocus::Playlists;
        self.mode = AppMode::Normal;
        self.status_message = format!("▶ Playing '{}'", track.title);
    }

    /// Add the currently highlighted track (in track view) to the queue.
    pub(super) fn feed_add_selected_track(&mut self) {
        let Some(track) = self.feed.expanded_tracks.get(self.feed.selected_track).cloned() else {
            return;
        };

        let queue_track = crate::player::queue::Track::new(
            track.video_id.clone(),
            track.title.clone(),
            track.duration,
            track.uploader.clone(),
            track.url.clone(),
        );

        let was_empty = self.queue.is_empty();
        self.spawn_download_with_limit(&queue_track);
        self.queue.add(queue_track);

        // Mark parent playlist as (partially) imported
        if let Some(item) = self.feed_selected_item() {
            self.feed.imported_ids.insert(item.id.clone());
        }

        self.status_message = format!(
            "✓ Added '{}' — {} in queue{}",
            track.title,
            self.queue.len(),
            if was_empty { " — press Space to play" } else { "" }
        );

        if let Err(e) = self.save_queue() {
            self.status_message = format!("Added, but queue not saved: {e}");
        }
    }

    // -----------------------------------------------------------------------
    // Feed play / import actions
    // -----------------------------------------------------------------------

    /// Add the selected feed playlist to the existing queue without clearing it.
    ///
    /// Stays in the feed browser so the user can add more playlists.
    /// Marks the playlist with a ✓ in the feed view.
    pub(super) async fn feed_add_to_playlist(&mut self) {
        let Some(item) = self.feed_selected_item() else {
            return;
        };

        // Already imported this session — give feedback but don't re-fetch
        if self.feed.imported_ids.contains(&item.id) {
            self.status_message = format!("'{}' is already in the queue", item.title);
            return;
        }

        self.status_message = format!("📥 Importing '{}'…", item.title);
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let tracks = match self.feed_load_tracks(&item.url, &item.title).await {
            Ok(t) => t,
            Err(msg) => {
                self.status_message = msg;
                return;
            }
        };

        let was_empty = self.queue.is_empty();
        let (added, filtered) = self.add_filtered_tracks(&tracks);

        if added == 0 {
            self.status_message = format!("No playable tracks found in '{}'", item.title);
            return;
        }

        // Mark imported — shows ✓ in the feed view
        self.feed.imported_ids.insert(item.id.clone());

        // Kick off background downloads for the new tracks
        self.trigger_smart_downloads();

        if filtered > 0 {
            self.status_message = format!(
                "✓ Added {} tracks from '{}' ({} filtered) — {} in queue",
                added,
                item.title,
                filtered,
                self.queue.len()
            );
        } else {
            self.status_message = format!(
                "✓ Added {} tracks from '{}' — {} in queue{}",
                added,
                item.title,
                self.queue.len(),
                if was_empty { " — press Space to play" } else { "" }
            );
        }

        // Persist
        if let Err(e) = self.save_queue() {
            self.status_message = format!("Imported, but queue not saved: {e}");
        }
    }

    /// Shared helper: fetch playlist tracks from a feed playlist URL.
    ///
    /// Returns `Ok(tracks)` on success, or `Err(user_message)` on failure.
    /// Handles cookie-missing, auth-expired, and yt-dlp errors uniformly.
    async fn feed_load_tracks(
        &self,
        url: &str,
        title: &str,
    ) -> Result<Vec<Track>, String> {
        if self.cookie_config().is_none() {
            return Err(
                "No browser account selected. Press 'q' then 'l' to log in.".to_string(),
            );
        }

        let cookie_config = self.cookie_config();
        let playlist_url = url.to_string();

        let result = tokio::task::spawn_blocking(move || {
            crate::services::playlist::fetch_playlist_tracks(&playlist_url, cookie_config)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Task error: {e}")));

        match result {
            Ok(tracks) if tracks.is_empty() => {
                Err(format!("No tracks found in '{title}'"))
            }
            Ok(tracks) => Ok(tracks),
            Err(e) => {
                // Surface auth-expired hint prominently
                let lower = e.to_lowercase();
                let msg = if lower.contains("sign in") || lower.contains("login") {
                    format!(
                        "YouTube auth expired for '{}' — re-select your browser account (press 'q' then 'l')",
                        title
                    )
                } else {
                    format!("Failed to load '{}': {}", title, e)
                };
                Err(msg)
            }
        }
    }

    pub(super) async fn start_login(&mut self) {
        self.status_message = "Detecting YouTube accounts from browsers...".to_string();

        // Detect available accounts from Chrome/Firefox/Zen
        let detected = self.browser_auth.detect_accounts();

        if detected.is_empty() {
            self.status_message =
                "No browser accounts found. Please login to YouTube in Chrome or Firefox first."
                    .to_string();
        } else {
            // Prepend a Log out sentinel so it's always index 0.
            self.available_accounts = Self::build_account_list(detected);
            self.status_message = format!(
                "Found {} account(s). Select one:",
                self.available_accounts.len().saturating_sub(1) // exclude logout entry
            );
            self.ui.selected_account_idx = 0;
            self.mode = AppMode::AccountPicker;
        }
    }

    /// Open the account picker from Normal mode (switch account or log out).
    /// Works even when already authenticated — no need to be on the login screen.
    pub(super) async fn switch_account(&mut self) {
        self.status_message = "Detecting YouTube accounts from browsers...".to_string();

        let detected = self.browser_auth.detect_accounts();

        if detected.is_empty() {
            self.status_message =
                "No browser accounts found. Please login to YouTube in Chrome or Firefox first."
                    .to_string();
            return;
        }

        self.available_accounts = Self::build_account_list(detected);
        self.ui.selected_account_idx = 0;
        self.status_message = "Select an account or choose Log out".to_string();
        self.mode = AppMode::AccountPicker;
    }

    /// Build the account list shown in the picker.
    /// Always prepends a `[Log out]` sentinel at index 0.
    fn build_account_list(
        detected: Vec<crate::youtube::browser_auth::BrowserAccount>,
    ) -> Vec<crate::youtube::browser_auth::BrowserAccount> {
        use crate::youtube::browser_auth::BrowserAccount;
        let mut list = vec![BrowserAccount {
            browser: "logout".to_string(),
            profile: String::new(),
            email: None,
            display_name: "[ Log out ]".to_string(),
        }];
        list.extend(detected);
        list
    }

    pub(super) async fn select_account(&mut self) {
        let Some(account) = self.available_accounts.get(self.ui.selected_account_idx).cloned()
        else {
            return;
        };

        // Logout sentinel
        if account.browser == "logout" {
            self.browser_auth.clear_selected_account();
            // Clear the feed cache so stale data from the old account isn't shown
            self.feed.sections.clear();
            self.feed.imported_ids.clear();
            self.feed.last_error = None;
            self.available_accounts.clear();
            self.mode = AppMode::LoginPrompt;
            self.status_message = "Logged out. Press 'l' to select a new account.".to_string();
            return;
        }

        match self.browser_auth.save_selected_account(&account) {
            Ok(_) => {
                self.status_message = format!(
                    "✓ Logged in as {} — Press 'f' for your feed or '/' to search",
                    account.display_name
                );
                // Clear any stale feed from a previous account
                self.feed.sections.clear();
                self.feed.imported_ids.clear();
                self.feed.last_error = None;
                self.mode = AppMode::Normal;
            }
            Err(e) => {
                self.status_message = format!("Failed to save account: {}", e);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests for feed navigation helpers and imported_ids tracking
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::ui::state::{FeedPlaylist, FeedSection, FeedState, PlaylistType};

    // Build a minimal FeedState with `n_sections` sections, each with
    // `items_per_section` items.
    fn make_feed(n_sections: usize, items_per_section: usize) -> FeedState {
        let sections = (0..n_sections)
            .map(|s| FeedSection {
                title: format!("Section {s}"),
                kind: PlaylistType::Mix,
                items: (0..items_per_section)
                    .map(|i| FeedPlaylist {
                        id: format!("id-{s}-{i}"),
                        title: format!("Playlist {s}-{i}"),
                        url: format!("https://music.youtube.com/playlist?list=id-{s}-{i}"),
                        playlist_type: PlaylistType::Mix,
                        track_count_estimate: 10,
                        thumbnail_url: None,
                        description: None,
                    })
                    .collect(),
            })
            .collect();

        FeedState {
            sections,
            ..Default::default()
        }
    }

    // Helper: simulate feed_navigate_down on a FeedState directly
    fn nav_down(feed: &mut FeedState) {
        let max = feed
            .sections
            .get(feed.selected_section)
            .map(|s| s.items.len().saturating_sub(1))
            .unwrap_or(0);
        if feed.selected_item < max {
            feed.selected_item += 1;
        }
    }

    fn nav_up(feed: &mut FeedState) {
        feed.selected_item = feed.selected_item.saturating_sub(1);
    }

    fn next_section(feed: &mut FeedState) {
        if !feed.sections.is_empty() {
            let max = feed.sections.len() - 1;
            if feed.selected_section < max {
                feed.selected_section += 1;
                feed.selected_item = 0;
            }
        }
    }

    fn prev_section(feed: &mut FeedState) {
        if feed.selected_section > 0 {
            feed.selected_section -= 1;
            feed.selected_item = 0;
        }
    }

    // -- navigate_down --

    #[test]
    fn navigate_down_increments_item() {
        let mut feed = make_feed(1, 3);
        nav_down(&mut feed);
        assert_eq!(feed.selected_item, 1);
    }

    #[test]
    fn navigate_down_clamps_at_last_item() {
        let mut feed = make_feed(1, 3);
        feed.selected_item = 2; // already at last
        nav_down(&mut feed);
        assert_eq!(feed.selected_item, 2); // unchanged
    }

    #[test]
    fn navigate_down_noop_on_empty_section() {
        let mut feed = make_feed(1, 0);
        nav_down(&mut feed);
        assert_eq!(feed.selected_item, 0);
    }

    // -- navigate_up --

    #[test]
    fn navigate_up_decrements_item() {
        let mut feed = make_feed(1, 3);
        feed.selected_item = 2;
        nav_up(&mut feed);
        assert_eq!(feed.selected_item, 1);
    }

    #[test]
    fn navigate_up_clamps_at_zero() {
        let mut feed = make_feed(1, 3);
        feed.selected_item = 0;
        nav_up(&mut feed);
        assert_eq!(feed.selected_item, 0);
    }

    // -- next_section / prev_section --

    #[test]
    fn next_section_advances_and_resets_item() {
        let mut feed = make_feed(3, 5);
        feed.selected_item = 3;
        next_section(&mut feed);
        assert_eq!(feed.selected_section, 1);
        assert_eq!(feed.selected_item, 0); // reset on section change
    }

    #[test]
    fn next_section_clamps_at_last_section() {
        let mut feed = make_feed(2, 3);
        feed.selected_section = 1; // already last
        next_section(&mut feed);
        assert_eq!(feed.selected_section, 1);
    }

    #[test]
    fn prev_section_goes_back_and_resets_item() {
        let mut feed = make_feed(3, 5);
        feed.selected_section = 2;
        feed.selected_item = 4;
        prev_section(&mut feed);
        assert_eq!(feed.selected_section, 1);
        assert_eq!(feed.selected_item, 0);
    }

    #[test]
    fn prev_section_clamps_at_zero() {
        let mut feed = make_feed(3, 3);
        feed.selected_section = 0;
        prev_section(&mut feed);
        assert_eq!(feed.selected_section, 0);
    }

    #[test]
    fn next_section_noop_on_empty_feed() {
        let mut feed = FeedState::default();
        next_section(&mut feed);
        assert_eq!(feed.selected_section, 0);
    }

    // -- imported_ids --

    #[test]
    fn imported_ids_tracks_inserted_ids() {
        let mut feed = make_feed(1, 3);
        assert!(!feed.imported_ids.contains("id-0-1"));
        feed.imported_ids.insert("id-0-1".to_string());
        assert!(feed.imported_ids.contains("id-0-1"));
        assert!(!feed.imported_ids.contains("id-0-0"));
    }

    #[test]
    fn imported_ids_insert_is_idempotent() {
        let mut feed = make_feed(1, 2);
        feed.imported_ids.insert("id-0-0".to_string());
        feed.imported_ids.insert("id-0-0".to_string());
        assert_eq!(feed.imported_ids.len(), 1);
    }

    // -- selected_item helper (via FeedState directly) --

    #[test]
    fn selected_item_returns_correct_playlist() {
        let feed = make_feed(2, 3);
        let item = feed
            .sections
            .get(feed.selected_section)
            .and_then(|s| s.items.get(feed.selected_item))
            .cloned();
        assert!(item.is_some());
        assert_eq!(item.unwrap().id, "id-0-0");
    }

    #[test]
    fn selected_item_returns_none_on_empty_feed() {
        let feed = FeedState::default();
        let item = feed
            .sections
            .get(feed.selected_section)
            .and_then(|s| s.items.get(feed.selected_item))
            .cloned();
        assert!(item.is_none());
    }
}
