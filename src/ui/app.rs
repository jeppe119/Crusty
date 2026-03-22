// Main TUI application using ratatui
// Handles the terminal interface, user input, and display

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::io;
use tokio::sync::mpsc;

use crate::config::{clean_title, format_time, is_allowed_youtube_url, MAX_TRACK_DURATION_SECS};
use crate::player::audio::{AudioPlayer, PlayerState};
use crate::player::queue::{Queue, Track};
use crate::services::download::DownloadManager;
use crate::services::persistence::PersistenceService;
use crate::ui::state::{
    AppMode, MixPlaylist, PlaylistState, QueueState, SearchState, UiState, ViewMode,
};
use crate::youtube::browser_auth::{BrowserAccount, BrowserAuth};
use crate::youtube::extractor::{self, VideoInfo, YouTubeExtractor};

pub struct MusicPlayerApp {
    // Core modules
    pub(crate) player: AudioPlayer,
    pub(crate) queue: Queue,
    pub(crate) browser_auth: BrowserAuth,
    pub(crate) available_accounts: Vec<BrowserAccount>,
    persistence: PersistenceService,
    pub(crate) downloads: DownloadManager,

    // UI state (sub-structs)
    pub(crate) ui: UiState,
    pub(crate) search: SearchState,
    pub(crate) playlist: PlaylistState,
    pub(crate) mode: AppMode,
    pub(crate) current_view: ViewMode,
    previous_view: ViewMode,
    should_quit: bool,
    pub(crate) status_message: String,
    queue_loaded: bool,

    // Async channels
    search_rx: mpsc::UnboundedReceiver<Vec<VideoInfo>>,
    search_tx: mpsc::UnboundedSender<Vec<VideoInfo>>,

    // Playback state
    pending_play_track: Option<Track>,
    currently_downloading: Option<String>,
}

impl MusicPlayerApp {
    pub fn new() -> Result<Self> {
        let (search_tx, search_rx) = mpsc::unbounded_channel();

        // Initialize browser auth (fallible — may fail if $HOME is unset or config dir is inaccessible)
        let browser_auth = BrowserAuth::new()
            .map_err(|e| anyhow::anyhow!("Failed to initialize browser auth: {}", e))?;

        // Check if user has already selected an account
        let is_authenticated = browser_auth.is_authenticated();

        let status_message = if is_authenticated {
            if let Some(account) = browser_auth.load_selected_account() {
                format!(
                    "Welcome back! Logged in as {} - Press '/' to search",
                    account.display_name
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let initial_mode = if is_authenticated {
            AppMode::Normal
        } else {
            AppMode::LoginPrompt
        };

        // Load persisted history
        let persistence = PersistenceService::new()?;
        let mut queue = Queue::new();
        if let Ok(history) = persistence.load_history() {
            for track in history {
                queue.add_to_history(track);
            }
        }

        // Don't load queue at startup - it blocks with large queues
        // Will load asynchronously after UI starts
        // if let Ok(queue_state) = Self::load_queue() {
        //     queue.restore_queue(queue_state.tracks, queue_state.current_track);
        // }

        Ok(MusicPlayerApp {
            player: AudioPlayer::new(),
            queue,
            browser_auth,
            available_accounts: Vec::new(),
            persistence,
            ui: UiState::default(),
            search: SearchState::default(),
            playlist: PlaylistState::default(),
            mode: initial_mode,
            current_view: ViewMode::Home,
            previous_view: ViewMode::Home,
            should_quit: false,
            status_message,
            queue_loaded: false,
            downloads: DownloadManager::new(),
            search_rx,
            search_tx,
            pending_play_track: None,
            currently_downloading: None,
        })
    }

    fn cookie_config(&self) -> Option<(bool, String)> {
        self.browser_auth
            .load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account))
    }

    fn save_history(&self) -> Result<()> {
        self.persistence.save_history(self.queue.get_history())
    }

    fn save_queue(&self) -> Result<()> {
        let state = QueueState {
            tracks: self.queue.get_queue_list(),
            current_track: self.queue.get_current().cloned(),
        };
        self.persistence.save_queue(&state)
    }

    // Async load queue in background
    async fn load_queue_async(&mut self) {
        if self.queue_loaded {
            return; // Already loaded
        }

        let result = tokio::task::spawn_blocking(|| -> Result<QueueState, String> {
            let svc = crate::services::persistence::PersistenceService::new()
                .map_err(|e| e.to_string())?;
            svc.load_queue().map_err(|e| e.to_string())
        })
        .await;

        match result {
            Ok(Ok(queue_state)) => {
                // Validate URLs loaded from disk before restoring
                let valid_tracks: Vec<Track> = queue_state
                    .tracks
                    .into_iter()
                    .filter(|t| is_allowed_youtube_url(&t.url))
                    .collect();
                let valid_current = queue_state
                    .current_track
                    .filter(|t| is_allowed_youtube_url(&t.url));
                let track_count = valid_tracks.len();
                self.queue.restore_queue(valid_tracks, valid_current);
                self.queue_loaded = true;

                // LIGHTWEIGHT RESTORATION STRATEGY:
                // Download ONLY current + next 5 tracks on startup
                // The rest will download as user navigates (via ensure_next_track_ready + sliding window)
                let mut downloads_started = 0;
                let has_current = self.queue.get_current().is_some();

                if let Some(current_track) = self.queue.get_current() {
                    // Current track exists - download it with HIGHEST priority (plays on Space)
                    if self.spawn_download_with_limit(current_track) {
                        downloads_started += 1;
                    }
                }

                // Download next 5 tracks only (reduced from 20 for FAST startup)
                // Less concurrent downloads = faster completion of priority tracks
                // ensure_next_track_ready() will handle the rest as user plays
                let next_tracks = self.queue.get_queue_slice(0, 5);
                for (idx, track) in next_tracks.iter().enumerate() {
                    if idx == 0 && !has_current {
                        // CRITICAL: If no current track, first queue track is HIGHEST priority
                        // This ensures instant playback when user presses Space for first time
                        if self.spawn_download_with_limit(track) {
                            downloads_started += 1;
                        }
                    } else {
                        // Buffer downloads (reduced from 20 to 5 for lightweight startup)
                        if self.spawn_download_with_limit(track) {
                            downloads_started += 1;
                        }
                    }
                }

                if downloads_started > 0 {
                    let priority_info = if has_current {
                        "current track ready soon"
                    } else if !next_tracks.is_empty() {
                        "first track ready soon"
                    } else {
                        "downloading"
                    };
                    self.status_message = format!(
                        "Restored {} tracks - {} ({} downloading)",
                        track_count, priority_info, downloads_started
                    );
                } else {
                    self.status_message =
                        format!("Restored {} tracks from previous session", track_count);
                }
            }
            Ok(Err(e)) => {
                eprintln!("Failed to load queue: {}", e);
                self.queue_loaded = true; // Mark as loaded anyway to avoid retrying
            }
            Err(e) => {
                eprintln!("Task error loading queue: {}", e);
                self.queue_loaded = true;
            }
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Clean up old pre-downloaded files on startup
        DownloadManager::cleanup_old_downloads();

        // Trigger async queue load on first iteration
        let mut queue_load_triggered = false;

        // Frame rate limiting: render at ~20 FPS (50ms per frame)
        // This prevents excessive CPU usage and gives async tasks more time
        let frame_duration = std::time::Duration::from_millis(50);
        let mut last_render = std::time::Instant::now();

        loop {
            // Load queue asynchronously on first iteration (non-blocking)
            if !queue_load_triggered {
                queue_load_triggered = true;
                self.load_queue_async().await;
            }

            // Only render if enough time has passed (frame rate limiting)
            let now = std::time::Instant::now();
            let should_render = now.duration_since(last_render) >= frame_duration;

            if should_render {
                terminal.draw(|f| self.draw_ui(f))?;
                last_render = now;
            }

            // Time-based animation updates (prevents mouse movement from speeding up animations)
            // Update animations at much slower rate for subtle, non-distracting effect
            let animation_interval = std::time::Duration::from_millis(150); // ~6.6 FPS for subtle animations
            if now.duration_since(self.ui.last_animation_update) >= animation_interval {
                self.ui.animation_frame = self.ui.animation_frame.wrapping_add(1);

                // Scroll title text slowly for readability
                self.ui.title_scroll_offset = self.ui.title_scroll_offset.wrapping_add(1);

                self.ui.last_animation_update = now;
            }

            // Check for search results
            if let Ok(results) = self.search_rx.try_recv() {
                self.search.results = results;
                self.ui.selected_result = 0;
                self.search.is_searching = false;
                self.status_message = format!("Found {} results", self.search.results.len());
            }

            // Check for completed downloads
            if let Some((video_id, result)) = self.downloads.poll_completion() {
                match result {
                    Ok(temp_file_path) => {
                        // Download succeeded! Play it if it's the pending track
                        if let Some(track) = &self.pending_play_track {
                            if track.video_id == video_id {
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                self.player.play_with_duration(
                                    &temp_file_path,
                                    &track.title,
                                    track.duration as f64,
                                );
                                self.status_message.clear();
                                self.pending_play_track = None;
                                self.currently_downloading = None;

                                // Ensure next track is downloading for instant skip
                                let next = self.queue.get_queue_slice(0, 10);
                                self.downloads
                                    .ensure_next_tracks_ready(&next, self.cookie_config());

                                // Clean up later
                                let temp_path = temp_file_path.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                                    let _ = std::fs::remove_file(&temp_path);
                                });
                            }
                        }

                        // RETRY PENDING: Check if there's a pending track waiting
                        if let Some(pending_track) = &self.pending_play_track {
                            if !self.downloads.is_cached(&pending_track.video_id)
                                && self.currently_downloading.is_none()
                            {
                                let cookie = self.cookie_config();
                                if self.downloads.spawn_download(pending_track, cookie) {
                                    self.currently_downloading = Some(pending_track.title.clone());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Download failed
                        if let Some(track) = &self.pending_play_track {
                            if track.video_id == video_id {
                                self.status_message = format!("❌ Download failed: {}", e);
                                self.pending_play_track = None;
                                self.currently_downloading = None;
                            }
                        }

                        // RETRY PENDING
                        if let Some(pending_track) = &self.pending_play_track {
                            if !self.downloads.is_cached(&pending_track.video_id)
                                && self.currently_downloading.is_none()
                            {
                                let cookie = self.cookie_config();
                                if self.downloads.spawn_download(pending_track, cookie) {
                                    self.currently_downloading = Some(pending_track.title.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Auto-advance to next track when current finishes
            // IMPORTANT: Only auto-advance when state is Playing (not Loading, Stopped, or Paused)
            // This prevents race condition where sink is empty during track loading
            if self.player.is_finished() && self.player.get_state() == PlayerState::Playing {
                if !self.queue.is_empty() {
                    self.status_message = "Track finished, playing next...".to_string();
                    self.play_next().await;
                } else {
                    self.player.stop();
                    self.status_message = "Playback finished - queue is empty".to_string();
                }
            }

            // Poll for events with shorter timeout to keep UI responsive
            // but yield to tokio runtime frequently for background tasks
            if event::poll(std::time::Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_input(key).await;
                }
            } else {
                // No events - yield to tokio runtime to process background tasks
                // This is CRITICAL for download performance when window is unfocused
                tokio::task::yield_now().await;
            }

            if self.should_quit {
                break;
            }
        }

        // Abort all background download tasks before saving
        self.downloads.abort_all();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Save history and queue before quitting
        if let Err(e) = self.save_history() {
            eprintln!("Failed to save history: {}", e);
        }
        // Save queue on exit (this is OK since we're exiting anyway)
        if let Err(e) = self.save_queue() {
            eprintln!("Failed to save queue: {}", e);
        }

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn draw_ui(&self, frame: &mut Frame) {
        use super::views;

        // Show login screen if not authenticated
        if matches!(self.mode, AppMode::LoginPrompt) {
            views::login::draw_login_screen(self, frame);
            return;
        }

        // Show account picker
        if matches!(self.mode, AppMode::AccountPicker) {
            views::login::draw_account_picker(self, frame);
            return;
        }

        // Show help screen
        if matches!(self.mode, AppMode::Help) {
            views::help::draw_help_screen(self, frame);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Main area
                Constraint::Length(6), // Bottom bar (compact - Playlists | Unified Player)
            ])
            .split(frame.area());

        // Header
        let title = if self.search.is_searching {
            "Searching... please wait".to_string()
        } else if !self.status_message.is_empty() {
            self.status_message.clone()
        } else {
            match self.mode {
                AppMode::Searching => format!("🔍 SEARCH MODE: {}_", self.search.query),
                AppMode::LoadingPlaylist => format!(
                    "📋 PASTE PLAYLIST URL: {}_  (Press Enter to load, Esc to cancel)",
                    self.playlist.url
                ),
                AppMode::Normal => {
                    let account_info =
                        if let Some(account) = self.browser_auth.load_selected_account() {
                            format!(" | Account: {}", account.display_name)
                        } else {
                            String::new()
                        };
                    format!("Controls: [/]Search [l]LoadPlaylist [Enter]Add [n]Next [p]Prev [Space]Play/Pause [j/k]Navigate [Shift+↑/↓]Volume [?]Help [q]Quit{}", account_info)
                }
                AppMode::LoginPrompt => "Login Required".to_string(),
                AppMode::AccountPicker => "Select YouTube Account".to_string(),
                AppMode::Help => "Help - Press '?', 'Esc', or 'q' to close".to_string(),
            }
        };
        let header = Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .title("YouTube Music Player"),
        );
        frame.render_widget(header, chunks[0]);

        // Main area layout depends on queue expansion, my mix expansion, history expansion, or view mode
        if self.ui.queue_expanded {
            // Queue expanded: Queue takes full main area
            views::queue::draw_queue_expanded(self, frame, chunks[1]);
        } else if self.ui.my_mix_expanded {
            // My Mix expanded: My Mix takes full main area
            views::playlist::draw_my_mix_expanded(self, frame, chunks[1]);
        } else if self.ui.history_expanded {
            // History expanded: History takes full main area
            views::history::draw_history_expanded(self, frame, chunks[1]);
        } else if self.ui.playlist_loading_expanded {
            // Playlist loading expanded: Show URL input interface
            views::playlist::draw_playlist_loading_expanded(self, frame, chunks[1]);
        } else if self.current_view == ViewMode::Search || matches!(self.mode, AppMode::Searching) {
            // Search view: Search Results (left) | History (right)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            views::search::draw_search_results(self, frame, main_chunks[0]);
            views::history::draw_history(self, frame, main_chunks[1]);
        } else {
            // Home view (default): Queue (left) | History (right)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            views::queue::draw_queue_compact(self, frame, main_chunks[0]);
            views::history::draw_history(self, frame, main_chunks[1]);
        }

        // Bottom bar: Player (50%) | Cache (15%) | Playlists (35%)
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50), // Player
                Constraint::Percentage(15), // Cache/Downloads
                Constraint::Percentage(35), // Playlists (more space!)
            ])
            .split(chunks[2]);

        views::player_bar::draw_player_compact(self, frame, bottom_chunks[0]);
        views::cache_stats::draw_cache_stats(self, frame, bottom_chunks[1]);
        views::playlist::draw_my_mix(self, frame, bottom_chunks[2]);
    }

    async fn handle_input(&mut self, key: KeyEvent) {
        use crate::ui::input::{key_to_command, AppCommand, InputContext};

        // Clear status message on any key press (except when searching)
        if !matches!(self.mode, AppMode::Searching) {
            self.status_message.clear();
        }

        let ctx = InputContext {
            mode: &self.mode,
            history_expanded: self.ui.history_expanded,
            search_query_len: self.search.query.len(),
            playlist_url_len: self.playlist.url.len(),
        };
        let cmd = key_to_command(key, &ctx);

        let Some(cmd) = cmd else { return };

        match cmd {
            AppCommand::Quit => self.should_quit = true,
            AppCommand::ShowHelp => self.mode = AppMode::Help,
            AppCommand::DismissHelp => self.mode = AppMode::Normal,
            AppCommand::StartSearch => self.mode = AppMode::Searching,
            AppCommand::StartLogin => self.start_login().await,
            AppCommand::StartLoadPlaylist => {
                self.mode = AppMode::LoadingPlaylist;
                self.ui.playlist_loading_expanded = true;
                self.playlist.url.clear();
                self.status_message = "Enter playlist URL (YouTube or YouTube Music)".to_string();
            }

            // Playback
            AppCommand::TogglePause => self.toggle_pause_or_start().await,
            AppCommand::NextTrack => {
                self.status_message = "Playing next track...".to_string();
                self.play_next().await;
            }
            AppCommand::PreviousTrack => {
                self.status_message = "Playing previous track...".to_string();
                self.play_previous().await;
            }
            AppCommand::VolumeUp { big_step } => self.volume_up(big_step),
            AppCommand::VolumeDown { big_step } => self.volume_down(big_step),
            AppCommand::SeekForward => self.seek_forward(),
            AppCommand::SeekBackward => self.seek_backward(),

            // Navigation
            AppCommand::NavigateDown => {
                if self.ui.queue_expanded {
                    self.next_queue_item();
                } else if self.ui.my_mix_expanded {
                    self.next_mix_item();
                } else if self.ui.history_expanded {
                    self.next_history_item();
                } else if self.current_view == ViewMode::Home {
                    self.next_mix_item();
                } else {
                    self.next_search_result();
                }
            }
            AppCommand::NavigateUp => {
                if self.ui.queue_expanded {
                    self.prev_queue_item();
                } else if self.ui.my_mix_expanded {
                    self.prev_mix_item();
                } else if self.ui.history_expanded {
                    self.prev_history_item();
                } else if self.current_view == ViewMode::Home {
                    self.prev_mix_item();
                } else {
                    self.prev_search_result();
                }
            }
            AppCommand::Select => {
                if self.current_view == ViewMode::Home {
                    self.add_selected_mix_to_queue().await;
                } else {
                    self.add_selected_to_queue();
                }
            }
            AppCommand::GoHome => {
                self.previous_view = self.current_view;
                self.current_view = ViewMode::Home;
                self.status_message = "Returned to Home (My Mix)".to_string();
            }
            AppCommand::EscapeBack => {
                std::mem::swap(&mut self.current_view, &mut self.previous_view);
                self.status_message = "Returned to previous view".to_string();
            }

            // Queue / History / Mix
            AppCommand::ToggleQueueExpand => {
                self.ui.queue_expanded = !self.ui.queue_expanded;
                self.status_message = if self.ui.queue_expanded {
                    "Queue expanded - use j/k to navigate, d to delete".to_string()
                } else {
                    "Queue collapsed".to_string()
                };
            }
            AppCommand::ToggleHistoryExpand => {
                self.ui.history_expanded = !self.ui.history_expanded;
                self.status_message = if self.ui.history_expanded {
                    "History expanded - use j/k to navigate, Shift+C to clear".to_string()
                } else {
                    "History collapsed".to_string()
                };
            }
            AppCommand::ToggleMixExpand => {
                self.ui.my_mix_expanded = !self.ui.my_mix_expanded;
                self.status_message = if self.ui.my_mix_expanded {
                    "My Mix expanded - use j/k to navigate, Shift+m to refresh".to_string()
                } else {
                    "My Mix collapsed".to_string()
                };
            }
            AppCommand::RefreshMix => {
                if self.ui.my_mix_expanded {
                    self.status_message = "Refreshing My Mix...".to_string();
                    self.refresh_my_mix().await;
                }
            }
            AppCommand::Delete => {
                if self.ui.history_expanded {
                    self.delete_selected_history_item();
                } else {
                    self.delete_selected_queue_item();
                }
            }
            AppCommand::ClearHistory => self.clear_history(),

            // Account picker
            AppCommand::NextAccount => self.next_account(),
            AppCommand::PreviousAccount => self.prev_account(),
            AppCommand::SelectAccount => self.select_account().await,
            AppCommand::CancelAccountPicker => self.mode = AppMode::LoginPrompt,

            // Search input
            AppCommand::SearchChar(c) => self.search.query.push(c),
            AppCommand::SearchBackspace => {
                self.search.query.pop();
            }
            AppCommand::SearchSubmit => {
                let query = self.search.query.clone();
                self.perform_search(&query).await;
                self.mode = AppMode::Normal;
                self.previous_view = self.current_view;
                self.current_view = ViewMode::Search;
                self.search.query.clear();
            }
            AppCommand::SearchCancel => {
                self.mode = AppMode::Normal;
                self.search.query.clear();
                std::mem::swap(&mut self.current_view, &mut self.previous_view);
            }

            // Playlist URL input
            AppCommand::PlaylistChar(c) => self.playlist.url.push(c),
            AppCommand::PlaylistBackspace => {
                self.playlist.url.pop();
            }
            AppCommand::PlaylistSubmit => {
                let url = self.playlist.url.clone();
                if !url.is_empty() {
                    self.load_playlist_from_url(&url).await;
                }
                self.mode = AppMode::Normal;
                self.playlist.url.clear();
                self.ui.playlist_loading_expanded = false;
            }
            AppCommand::PlaylistCancel => {
                self.mode = AppMode::Normal;
                self.playlist.url.clear();
                self.ui.playlist_loading_expanded = false;
                self.status_message = "Cancelled playlist loading".to_string();
            }
        }
    }

    async fn perform_search(&mut self, query: &str) {
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
                Err(e) => {
                    eprintln!("Search failed: {}", e);
                    // Send empty results to unblock UI
                    let _ = tx.send(Vec::new());
                }
            }
        });
    }

    async fn play_next(&mut self) {
        // CRITICAL: Clear pending state FIRST so navigation always works
        self.pending_play_track = None;
        self.currently_downloading = None;

        if let Some(track) = self.queue.next() {
            self.queue.limit_history(100);
            self.play_track_from_cache_or_download(&track);
        } else {
            self.status_message = "Queue is empty!".to_string();
        }
    }

    async fn play_previous(&mut self) {
        // CRITICAL: Clear pending state FIRST so navigation always works
        self.pending_play_track = None;
        self.currently_downloading = None;

        if let Some(track) = self.queue.previous() {
            self.queue.limit_history(100);
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
    fn play_track_from_cache_or_download(&mut self, track: &Track) {
        let cached_file = self.downloads.get_cached_file(&track.video_id);

        if let Some(local_file) = cached_file {
            if std::path::Path::new(&local_file).exists() {
                self.player
                    .play_with_duration(&local_file, &track.title, track.duration as f64);
                self.status_message.clear();
                let next = self.queue.get_queue_slice(0, 10);
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
        } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
            self.pending_play_track = Some(track.clone());
            let cookie = self.cookie_config();
            if self.downloads.spawn_download(track, cookie) {
                self.currently_downloading = Some(track.title.clone());
            }
        } else {
            self.status_message = "Cannot play non-YouTube URL".to_string();
        }
    }

    fn spawn_download_with_limit(&self, track: &Track) -> bool {
        self.downloads.spawn_download(track, self.cookie_config())
    }

    fn trigger_smart_downloads(&self) {
        let next_tracks = self.queue.get_queue_slice(0, 10);
        self.downloads
            .ensure_next_tracks_ready(&next_tracks, self.cookie_config());
    }

    async fn toggle_pause_or_start(&mut self) {
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

    async fn play_selected_queue_track(&mut self) {
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

    async fn play_current_or_first(&mut self) {
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

    fn volume_up(&mut self, has_shift: bool) {
        let current = self.player.get_volume();
        let increment = if has_shift { 5 } else { 1 };
        if current < 100 {
            self.player.set_volume((current + increment).min(100));
        }
    }

    fn volume_down(&mut self, has_shift: bool) {
        let current = self.player.get_volume();
        let decrement = if has_shift { 5 } else { 1 };
        if current > 0 {
            self.player.set_volume(current.saturating_sub(decrement));
        }
    }

    fn seek_forward(&mut self) {
        // Seek forward 10 seconds
        self.player.seek_relative(10.0);
        self.player.apply_seek();
        self.status_message = format!("Seeked +10s ({})", format_time(self.player.get_time_pos()));
    }

    fn seek_backward(&mut self) {
        // Seek backward 10 seconds
        self.player.seek_relative(-10.0);
        self.player.apply_seek();
        self.status_message = format!("Seeked -10s ({})", format_time(self.player.get_time_pos()));
    }

    fn next_search_result(&mut self) {
        if !self.search.results.is_empty() {
            self.ui.selected_result = (self.ui.selected_result + 1) % self.search.results.len();
        }
    }

    fn prev_search_result(&mut self) {
        if !self.search.results.is_empty() {
            if self.ui.selected_result == 0 {
                self.ui.selected_result = self.search.results.len() - 1;
            } else {
                self.ui.selected_result -= 1;
            }
        }
    }

    fn next_queue_item(&mut self) {
        let queue_len = self.queue.len();
        if queue_len > 0 {
            self.ui.selected_queue_item = (self.ui.selected_queue_item + 1) % queue_len;
            // HOVER DOWNLOAD: Start downloading this track immediately!
            self.trigger_hover_download(self.ui.selected_queue_item);
        }
    }

    fn prev_queue_item(&mut self) {
        let queue_len = self.queue.len();
        if queue_len > 0 {
            if self.ui.selected_queue_item == 0 {
                self.ui.selected_queue_item = queue_len - 1;
            } else {
                self.ui.selected_queue_item -= 1;
            }
            // HOVER DOWNLOAD: Start downloading this track immediately!
            self.trigger_hover_download(self.ui.selected_queue_item);
        }
    }

    fn trigger_hover_download(&self, index: usize) {
        let queue_slice = self.queue.get_queue_slice(index + 15, 1);
        self.downloads
            .trigger_hover_download(&queue_slice, self.cookie_config());
    }

    fn delete_selected_queue_item(&mut self) {
        if self.ui.queue_expanded && !self.queue.is_empty() {
            if let Some(removed_track) = self.queue.remove_at(self.ui.selected_queue_item) {
                let clean_title = clean_title(&removed_track.title);
                self.status_message = format!("Removed '{}' from queue", clean_title);

                // Adjust selection if needed
                let queue_len = self.queue.len();
                if queue_len == 0 {
                    self.ui.selected_queue_item = 0;
                } else if self.ui.selected_queue_item >= queue_len {
                    self.ui.selected_queue_item = queue_len - 1;
                }

                // Don't save on every action - only on exit
                // self.save_queue_async();
            }
        } else if !self.ui.queue_expanded {
            self.status_message = "Press 't' to expand queue first".to_string();
        }
    }

    fn next_mix_item(&mut self) {
        if !self.playlist.my_mix_playlists.is_empty() {
            self.ui.selected_mix_item =
                (self.ui.selected_mix_item + 1) % self.playlist.my_mix_playlists.len();
        }
    }

    fn prev_mix_item(&mut self) {
        if !self.playlist.my_mix_playlists.is_empty() {
            if self.ui.selected_mix_item == 0 {
                self.ui.selected_mix_item = self.playlist.my_mix_playlists.len() - 1;
            } else {
                self.ui.selected_mix_item -= 1;
            }
        }
    }

    fn next_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len > 0 {
            self.ui.selected_history_item = (self.ui.selected_history_item + 1) % history_len;
        }
    }

    fn prev_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len > 0 {
            if self.ui.selected_history_item == 0 {
                self.ui.selected_history_item = history_len - 1;
            } else {
                self.ui.selected_history_item -= 1;
            }
        }
    }

    fn clear_history(&mut self) {
        let count = self.queue.get_history().len();
        self.queue.clear_history();
        self.ui.selected_history_item = 0;
        self.status_message = format!("Cleared {} tracks from history", count);

        // Save to disk
        if let Err(e) = self.save_history() {
            self.status_message = format!("History cleared but failed to save: {}", e);
        }
    }

    fn delete_selected_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len == 0 {
            return;
        }

        if let Some(removed) = self.queue.remove_history_at(self.ui.selected_history_item) {
            let title = clean_title(&removed.title);
            self.status_message = format!("Removed '{}' from history", title);

            // Adjust selection
            let new_len = self.queue.get_history().len();
            if new_len == 0 {
                self.ui.selected_history_item = 0;
            } else if self.ui.selected_history_item >= new_len {
                self.ui.selected_history_item = new_len - 1;
            }

            // Save to disk
            if let Err(e) = self.save_history() {
                eprintln!("Failed to save history after delete: {}", e);
            }
        }
    }

    async fn load_playlist_from_url(&mut self, url: &str) {
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
            Self::fetch_playlist_tracks_blocking(&playlist_url, cookie_config)
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

                // Add tracks to queue (filter out tracks > 5 minutes)
                let mut added_count = 0;
                let mut filtered_count = 0;
                for track in &tracks {
                    if track.duration <= MAX_TRACK_DURATION_SECS {
                        self.queue.add(track.clone());
                        added_count += 1;
                    } else {
                        filtered_count += 1;
                    }
                }

                // Store loaded playlist for display (moved after iteration to avoid clone)
                self.playlist.loaded_tracks = tracks;

                // Trigger smart downloads - downloads next 15 + previous 5
                self.trigger_smart_downloads();

                if filtered_count > 0 {
                    self.status_message = format!(
                        "Added {} tracks to queue ({} long tracks filtered out)",
                        added_count, filtered_count
                    );
                } else {
                    self.status_message = format!("Added {} tracks to queue", added_count);
                }

                // Don't save on every action - only on exit
                // self.save_queue_async();
            }
            Ok(Err(e)) => {
                self.status_message = format!("Failed to fetch playlist: {}", e);
            }
            Err(e) => {
                self.status_message = format!("Task error: {}", e);
            }
        }
    }

    async fn add_selected_mix_to_queue(&mut self) {
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
                Self::fetch_playlist_tracks_blocking(&playlist_url, cookie_config)
            })
            .await;

            match fetch_result {
                Ok(Ok(tracks)) => {
                    if tracks.is_empty() {
                        self.status_message = format!("No tracks found in '{}'", mix.title);
                        return;
                    }

                    // Add tracks to queue (filter out tracks > 5 minutes = 300 seconds)
                    let mut added_count = 0;
                    let mut filtered_count = 0;
                    for track in tracks {
                        if track.duration <= MAX_TRACK_DURATION_SECS {
                            self.queue.add(track);
                            added_count += 1;
                        } else {
                            filtered_count += 1;
                        }
                    }

                    // Trigger smart downloads - downloads next 15 + previous 5
                    self.trigger_smart_downloads();

                    if filtered_count > 0 {
                        self.status_message = format!(
                            "Added {} tracks from '{}' ({} long tracks filtered out)",
                            added_count, mix.title, filtered_count
                        );
                    } else {
                        self.status_message =
                            format!("Added {} tracks from '{}' to queue", added_count, mix.title);
                    }

                    // Save queue to disk
                    if let Err(e) = self.save_queue() {
                        eprintln!("Failed to save queue: {}", e);
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

    fn fetch_playlist_tracks_blocking(
        playlist_url: &str,
        cookie_config: Option<(bool, String)>,
    ) -> Result<Vec<Track>, String> {
        use std::process::Command;

        // Validate URL before passing to yt-dlp
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

        // Add cookies from browser if available
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

        let stdout =
            String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;

        let mut tracks = Vec::new();

        // Parse each line of JSON output
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

    async fn refresh_my_mix(&mut self) {
        self.status_message = "Refreshing My Mix playlists...".to_string();
        self.fetch_my_mix().await;
    }

    async fn fetch_my_mix(&mut self) {
        // Fetch My Mix playlists using yt-dlp
        let cookie_config = self
            .browser_auth
            .load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account));

        let fetch_result =
            tokio::task::spawn_blocking(move || Self::fetch_my_mix_blocking(cookie_config)).await;

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

    fn fetch_my_mix_blocking(
        cookie_config: Option<(bool, String)>,
    ) -> Result<Vec<MixPlaylist>, String> {
        use std::process::Command;

        let mut cmd = Command::new("yt-dlp");
        cmd.arg("--flat-playlist")
            .arg("--dump-json")
            .arg("--no-warnings")
            .arg("--skip-download")
            .arg("--socket-timeout")
            .arg("30")
            .arg("--retries")
            .arg("2");

        // Add cookies from browser if available
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

        let stdout =
            String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;

        let mut playlists = Vec::new();

        // Parse each line of JSON output
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                // Check if this is a playlist entry
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

                        // Filter for My Mix playlists (auto-generated mixes)
                        // These typically have IDs starting with "RDCLAK", "RDAMPL", or contain "Mix" in title
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

    fn add_selected_to_queue(&mut self) {
        if let Some(video) = self.search.results.get(self.ui.selected_result) {
            // Filter out tracks > 5 minutes (300 seconds) - this is a music player!
            if video.duration > MAX_TRACK_DURATION_SECS {
                let clean_title = clean_title(&video.title);
                let mins = video.duration / 60;
                self.status_message = format!(
                    "'{}' is too long ({}min) - music only (<5min)",
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
                eprintln!("Failed to save queue: {}", e);
            }
        }
    }

    async fn start_login(&mut self) {
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

    fn next_account(&mut self) {
        if !self.available_accounts.is_empty() {
            self.ui.selected_account_idx =
                (self.ui.selected_account_idx + 1) % self.available_accounts.len();
        }
    }

    fn prev_account(&mut self) {
        if !self.available_accounts.is_empty() {
            if self.ui.selected_account_idx == 0 {
                self.ui.selected_account_idx = self.available_accounts.len() - 1;
            } else {
                self.ui.selected_account_idx -= 1;
            }
        }
    }

    async fn select_account(&mut self) {
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
