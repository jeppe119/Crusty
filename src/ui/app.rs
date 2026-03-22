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

use crate::config::{
    is_allowed_youtube_url, LOOKAHEAD_DOWNLOAD_COUNT, PLAYED_FILE_CLEANUP_DELAY_SECS,
    STARTUP_DOWNLOAD_COUNT,
};
use crate::player::audio::{AudioPlayer, PlayerState};
use crate::player::queue::{Queue, Track};
use crate::services::download::DownloadManager;
use crate::services::persistence::PersistenceService;
use crate::ui::state::{AppMode, PlaylistState, QueueState, SearchState, UiState, ViewMode};
use crate::youtube::browser_auth::{BrowserAccount, BrowserAuth};
use crate::youtube::extractor::VideoInfo;

pub struct MusicPlayerApp {
    // Core modules
    pub(crate) player: AudioPlayer,
    pub(crate) queue: Queue,
    pub(crate) browser_auth: BrowserAuth,
    pub(crate) available_accounts: Vec<BrowserAccount>,
    pub(super) persistence: PersistenceService,
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
    pub(super) search_tx: mpsc::UnboundedSender<Vec<VideoInfo>>,

    // Playback state
    pub(super) pending_play_track: Option<Track>,
    pub(super) currently_downloading: Option<String>,
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
            queue.limit_history(crate::services::persistence::MAX_HISTORY_SIZE);
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

    pub(super) fn cookie_config(&self) -> Option<(bool, String)> {
        self.browser_auth
            .load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account))
    }

    pub(super) fn save_history(&self) -> Result<()> {
        self.persistence.save_history(self.queue.get_history())
    }

    pub(super) fn save_queue(&self) -> Result<()> {
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

        let config_dir = self.persistence.config_dir().to_owned();
        let result = tokio::task::spawn_blocking(move || -> Result<QueueState, String> {
            let svc = PersistenceService::from_dir(config_dir);
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

                // Download a small batch on startup for fast initial playback.
                // ensure_next_track_ready() handles the rest as user plays.
                let next_tracks = self.queue.get_queue_slice(0, STARTUP_DOWNLOAD_COUNT);
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
                self.status_message = format!("Failed to load queue: {}", e);
                self.queue_loaded = true; // Mark as loaded anyway to avoid retrying
            }
            Err(e) => {
                self.status_message = format!("Task error loading queue: {}", e);
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
                                let next = self.queue.get_queue_slice(0, LOOKAHEAD_DOWNLOAD_COUNT);
                                self.downloads
                                    .ensure_next_tracks_ready(&next, self.cookie_config());

                                // Clean up the temp file after a delay
                                let temp_path = temp_file_path.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(
                                        PLAYED_FILE_CLEANUP_DELAY_SECS,
                                    ))
                                    .await;
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
            AppCommand::ToggleMusicOnlyMode => {
                self.ui.music_only_mode = !self.ui.music_only_mode;
                self.status_message = if self.ui.music_only_mode {
                    "Music mode ON (tracks >5min filtered)".to_string()
                } else {
                    "All content mode (no duration filter)".to_string()
                };
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

    // Playback methods: see ui/playback.rs
    // Navigation methods: see ui/navigation.rs
    // Action methods (search, playlist, login): see ui/actions.rs
}
