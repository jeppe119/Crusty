// Main TUI application using ratatui
// Handles the terminal interface, user input, and display

use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph, Gauge},
    Frame, Terminal,
    style::{Color, Modifier, Style},
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use crate::player::audio::{AudioPlayer, PlayerState};
use crate::player::queue::{Queue, Track};
use crate::youtube::extractor::{YouTubeExtractor, VideoInfo};
use crate::youtube::browser_auth::{BrowserAuth, BrowserAccount};

enum AppMode {
    Normal,
    Searching,
    LoginPrompt,   // Show login screen
    AccountPicker, // Show list of browser accounts
    Help,          // Show help screen with keybinds
    LoadingPlaylist, // Loading playlist from URL
}

#[derive(Debug, Clone, PartialEq)]
enum ViewMode {
    Home,      // Showing My Mix | History
    Search,    // Showing Search Results | History
}

#[derive(Debug, Clone)]
struct MixPlaylist {
    id: String,
    title: String,
    track_count: usize,
    url: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct QueueState {
    tracks: Vec<Track>,
    current_track: Option<Track>,
}

pub struct MusicPlayerApp {
    player: AudioPlayer,
    queue: Queue,
    extractor: YouTubeExtractor,
    browser_auth: BrowserAuth,
    available_accounts: Vec<BrowserAccount>,
    selected_account_idx: usize,
    search_results: Vec<VideoInfo>,
    selected_result: usize,
    selected_queue_item: usize,
    search_query: String,
    playlist_url: String,
    mode: AppMode,
    should_quit: bool,
    is_searching: bool,
    search_rx: mpsc::UnboundedReceiver<Vec<VideoInfo>>,
    search_tx: mpsc::UnboundedSender<Vec<VideoInfo>>,
    status_message: String,
    // Track pre-downloaded files by video_id
    downloaded_files: Arc<Mutex<HashMap<String, String>>>,
    // Track failed downloads by video_id
    failed_downloads: Arc<Mutex<HashMap<String, String>>>,
    // Queue view expansion toggle
    queue_expanded: bool,
    // View tracking
    current_view: ViewMode,
    previous_view: ViewMode,
    // My Mix
    my_mix_playlists: Vec<MixPlaylist>,
    my_mix_expanded: bool,
    selected_mix_item: usize,
    // Playlist loading expansion
    playlist_loading_expanded: bool,
    // Loaded playlists
    loaded_playlist_tracks: Vec<Track>,
    loaded_playlist_name: String,
    // History
    history_expanded: bool,
    selected_history_item: usize,
    // Queue loading state
    queue_loaded: bool,
    // Download notification channel
    download_rx: mpsc::UnboundedReceiver<(String, Result<String, String>)>, // (video_id, result)
    download_tx: mpsc::UnboundedSender<(String, Result<String, String>)>,
    // Track being downloaded to play next
    pending_play_track: Option<Track>,
    // Track currently being downloaded (for progress display)
    currently_downloading: Option<String>, // track title
    // Active download count (for rate limiting)
    active_downloads: Arc<Mutex<usize>>,
    // Track which video_ids are currently downloading (prevents duplicate downloads on SPACE spam)
    downloading_videos: Arc<Mutex<std::collections::HashSet<String>>>,
    // Animation frame counter for download indicator
    animation_frame: u8,
    // Title scroll position for rotating long titles
    title_scroll_offset: usize,
    // Track background download tasks for proper cleanup
    background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    // Time-based animation tracking (prevents mouse movement from speeding up animations)
    last_animation_update: std::time::Instant,
}

impl MusicPlayerApp {
    pub fn new() -> Self {
        let (search_tx, search_rx) = mpsc::unbounded_channel();
        let (download_tx, download_rx) = mpsc::unbounded_channel();

        // Initialize browser auth
        let browser_auth = BrowserAuth::new().expect("Failed to initialize browser auth");

        // Check if user has already selected an account
        let is_authenticated = browser_auth.is_authenticated();

        let status_message = if is_authenticated {
            if let Some(account) = browser_auth.load_selected_account() {
                format!("Welcome back! Logged in as {} - Press '/' to search", account.display_name)
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
        let mut queue = Queue::new();
        if let Ok(history) = Self::load_history() {
            for track in history {
                queue.add_to_history(track);
            }
        }

        // Don't load queue at startup - it blocks with large queues
        // Will load asynchronously after UI starts
        // if let Ok(queue_state) = Self::load_queue() {
        //     queue.restore_queue(queue_state.tracks, queue_state.current_track);
        // }

        MusicPlayerApp {
            player: AudioPlayer::new(),
            queue,
            extractor: YouTubeExtractor::new(),
            browser_auth,
            available_accounts: Vec::new(),
            selected_account_idx: 0,
            search_results: Vec::new(),
            selected_result: 0,
            selected_queue_item: 0,
            search_query: String::new(),
            playlist_url: String::new(),
            mode: initial_mode,
            should_quit: false,
            is_searching: false,
            search_rx,
            search_tx,
            status_message,
            downloaded_files: Arc::new(Mutex::new(HashMap::new())),
            failed_downloads: Arc::new(Mutex::new(HashMap::new())),
            queue_expanded: false,
            current_view: ViewMode::Home,
            previous_view: ViewMode::Home,
            my_mix_playlists: Vec::new(),
            my_mix_expanded: false,
            selected_mix_item: 0,
            playlist_loading_expanded: false,
            loaded_playlist_tracks: Vec::new(),
            loaded_playlist_name: String::new(),
            history_expanded: false,
            selected_history_item: 0,
            queue_loaded: false,
            download_rx,
            download_tx,
            pending_play_track: None,
            currently_downloading: None,
            active_downloads: Arc::new(Mutex::new(0)),
            downloading_videos: Arc::new(Mutex::new(std::collections::HashSet::new())),
            animation_frame: 0,
            title_scroll_offset: 0,
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            last_animation_update: std::time::Instant::now(),
        }
    }

    // Get animated download indicator (Pac-Man style)
    fn get_download_animation(&self) -> &'static str {
        // Animate every 8 frames (slower animation)
        let frame = (self.animation_frame / 8) % 4;
        match frame {
            0 => "ᗧ··· ",  // Pac-Man open
            1 => "·ᗧ·· ",  // Moving right
            2 => "··ᗧ· ",  // Moving right
            3 => "···ᗧ ",  // Moving right
            _ => "ᗧ··· ",
        }
    }

    // Get bouncing playback visualization
    fn get_playback_visualization(&self, progress_ratio: u16) -> String {
        // Bouncing bars animation when music is playing
        let frame = (self.animation_frame / 4) % 8;  // Faster bounce

        // Create bouncing bar heights
        let bars = match frame {
            0 => "▁▂▃▄▅▆▇█",
            1 => "▂▃▄▅▆▇█▇",
            2 => "▃▄▅▆▇█▇▆",
            3 => "▄▅▆▇█▇▆▅",
            4 => "▅▆▇█▇▆▅▄",
            5 => "▆▇█▇▆▅▄▃",
            6 => "▇█▇▆▅▄▃▂",
            7 => "█▇▆▅▄▃▂▁",
            _ => "▄▄▄▄▄▄▄▄",
        };

        let time_pos = self.player.get_time_pos();
        let duration = self.player.get_duration();

        if duration > 0.0 {
            format!("{} {}/{}", bars, Self::format_time(time_pos), Self::format_time(duration))
        } else {
            format!("{} Playing...", bars)
        }
    }

    fn load_history() -> Result<Vec<Track>, Box<dyn std::error::Error>> {
        use std::fs;

        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("youtube-music-player");

        let history_file = config_dir.join("history.json");

        if !history_file.exists() {
            return Ok(Vec::new());
        }

        let contents = fs::read_to_string(history_file)?;
        let history: Vec<Track> = serde_json::from_str(&contents)?;

        Ok(history)
    }

    fn save_history(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;

        // Limit history to 100 most recent tracks before saving
        self.queue.limit_history(100);

        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("youtube-music-player");

        fs::create_dir_all(&config_dir)?;

        let history_file = config_dir.join("history.json");
        let history = self.queue.get_history();
        let json = serde_json::to_string_pretty(history)?;

        fs::write(history_file, json)?;

        Ok(())
    }

    fn load_queue() -> Result<QueueState, Box<dyn std::error::Error>> {
        use std::fs;

        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("youtube-music-player");

        let queue_file = config_dir.join("queue.json");

        if !queue_file.exists() {
            return Ok(QueueState {
                tracks: Vec::new(),
                current_track: None,
            });
        }

        let contents = fs::read_to_string(queue_file)?;
        let queue_state: QueueState = serde_json::from_str(&contents)?;

        Ok(queue_state)
    }

    fn save_queue(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;

        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("youtube-music-player");

        fs::create_dir_all(&config_dir)?;

        let queue_file = config_dir.join("queue.json");

        // Create queue state from current queue
        let queue_state = QueueState {
            tracks: self.queue.get_queue_list(),
            current_track: self.queue.get_current().cloned(),
        };

        let json = serde_json::to_string_pretty(&queue_state)?;

        fs::write(queue_file, json)?;

        Ok(())
    }

    // Async load queue in background
    async fn load_queue_async(&mut self) {
        if self.queue_loaded {
            return; // Already loaded
        }

        let result = tokio::task::spawn_blocking(|| -> Result<QueueState, String> {
            use std::fs;

            let config_dir = dirs::config_dir()
                .ok_or("Could not find config directory")?
                .join("youtube-music-player");

            let queue_file = config_dir.join("queue.json");

            if !queue_file.exists() {
                return Ok(QueueState {
                    tracks: Vec::new(),
                    current_track: None,
                });
            }

            let contents = fs::read_to_string(queue_file)
                .map_err(|e| format!("Failed to read queue file: {}", e))?;
            let queue_state: QueueState = serde_json::from_str(&contents)
                .map_err(|e| format!("Failed to parse queue file: {}", e))?;

            Ok(queue_state)
        }).await;

        match result {
            Ok(Ok(queue_state)) => {
                let track_count = queue_state.tracks.len();
                self.queue.restore_queue(queue_state.tracks, queue_state.current_track);
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
                    self.status_message = format!("Restored {} tracks - {} ({} downloading)", track_count, priority_info, downloads_started);
                } else {
                    self.status_message = format!("Restored {} tracks from previous session", track_count);
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

    // Async version that doesn't block the UI
    fn save_queue_async(&self) {
        // Clone the data we need
        let queue_state = QueueState {
            tracks: self.queue.get_queue_list(),
            current_track: self.queue.get_current().cloned(),
        };

        // Spawn blocking task to save in background
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                use std::fs;

                let config_dir = dirs::config_dir()
                    .ok_or("Could not find config directory")?
                    .join("youtube-music-player");

                fs::create_dir_all(&config_dir)
                    .map_err(|e| format!("Failed to create config dir: {}", e))?;

                let queue_file = config_dir.join("queue.json");
                let json = serde_json::to_string_pretty(&queue_state)
                    .map_err(|e| format!("Failed to serialize queue: {}", e))?;
                fs::write(queue_file, json)
                    .map_err(|e| format!("Failed to write queue file: {}", e))?;

                Ok(())
            }).await;

            match result {
                Ok(Ok(())) => {}, // Success
                Ok(Err(e)) => eprintln!("Failed to save queue: {}", e),
                Err(e) => eprintln!("Task error while saving queue: {}", e),
            }
        });
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Clean up old pre-downloaded files on startup
        Self::cleanup_old_downloads();

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
            if now.duration_since(self.last_animation_update) >= animation_interval {
                self.animation_frame = self.animation_frame.wrapping_add(1);

                // Scroll title text slowly for readability
                self.title_scroll_offset = self.title_scroll_offset.wrapping_add(1);

                self.last_animation_update = now;
            }

            // Check for search results
            if let Ok(results) = self.search_rx.try_recv() {
                self.search_results = results;
                self.selected_result = 0;
                self.is_searching = false;
                self.status_message = format!("Found {} results", self.search_results.len());
            }

            // Check for completed downloads
            if let Ok((video_id, result)) = self.download_rx.try_recv() {
                match result {
                    Ok(temp_file_path) => {
                        // Download succeeded! Play it if it's the pending track
                        if let Some(track) = &self.pending_play_track {
                            if track.video_id == video_id {
                                // Use async sleep instead of blocking thread sleep
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                self.player.play_with_duration(&temp_file_path, &track.title, track.duration as f64);
                                self.status_message = "".to_string();  // Clear status - player shows track
                                self.pending_play_track = None;
                                self.currently_downloading = None;

                                // PROACTIVE: Ensure next track is downloading for instant skip
                                self.ensure_next_track_ready();

                                // Clean up later
                                let temp_path = temp_file_path.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                                    let _ = std::fs::remove_file(&temp_path);
                                });
                            }
                        }

                        // RETRY PENDING: Check if there's a pending track waiting (was rate-limited)
                        if let Some(pending_track) = &self.pending_play_track {
                            let cached = self.downloaded_files.lock().ok()
                                .and_then(|files| files.contains_key(&pending_track.video_id).then(|| ()));

                            if cached.is_none() && self.currently_downloading.is_none() {
                                // Pending track not in cache and not downloading - retry now!
                                if self.spawn_download_with_limit(pending_track) {
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

                        // RETRY PENDING: Even on failure, check if there's a pending track waiting
                        if let Some(pending_track) = &self.pending_play_track {
                            let cached = self.downloaded_files.lock().ok()
                                .and_then(|files| files.contains_key(&pending_track.video_id).then(|| ()));

                            if cached.is_none() && self.currently_downloading.is_none() {
                                // Pending track not in cache and not downloading - retry now!
                                if self.spawn_download_with_limit(pending_track) {
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

        // CRITICAL: Abort all background download tasks before saving!
        eprintln!("Cleaning up background downloads...");
        if let Ok(mut tasks) = self.background_tasks.lock() {
            for handle in tasks.drain(..) {
                handle.abort();
            }
        }
        // Give tasks a moment to abort gracefully
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
        // Show login screen if not authenticated
        if matches!(self.mode, AppMode::LoginPrompt) {
            self.draw_login_screen(frame);
            return;
        }

        // Show account picker
        if matches!(self.mode, AppMode::AccountPicker) {
            self.draw_account_picker(frame);
            return;
        }

        // Show help screen
        if matches!(self.mode, AppMode::Help) {
            self.draw_help_screen(frame);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),      // Header
                Constraint::Min(10),        // Main area
                Constraint::Length(6),      // Bottom bar (compact - Playlists | Unified Player)
            ])
            .split(frame.size());

        // Header
        let title = if self.is_searching {
            "Searching... please wait".to_string()
        } else if !self.status_message.is_empty() {
            self.status_message.clone()
        } else {
            match self.mode {
                AppMode::Searching => format!("🔍 SEARCH MODE: {}_", self.search_query),
                AppMode::LoadingPlaylist => format!("📋 PASTE PLAYLIST URL: {}_  (Press Enter to load, Esc to cancel)", self.playlist_url),
                AppMode::Normal => {
                    let account_info = if let Some(account) = self.browser_auth.load_selected_account() {
                        format!(" | Account: {}", account.display_name)
                    } else {
                        String::new()
                    };
                    format!("Controls: [/]Search [l]LoadPlaylist [Enter]Add [n]Next [p]Prev [Space]Play/Pause [j/k]Navigate [Shift+↑/↓]Volume [?]Help [q]Quit{}", account_info)
                },
                AppMode::LoginPrompt => "Login Required".to_string(),
                AppMode::AccountPicker => "Select YouTube Account".to_string(),
                AppMode::Help => "Help - Press '?', 'Esc', or 'q' to close".to_string(),
            }
        };
        let header = Paragraph::new(title)
            .block(Block::default().borders(Borders::ALL).title("YouTube Music Player"));
        frame.render_widget(header, chunks[0]);

        // Main area layout depends on queue expansion, my mix expansion, history expansion, or view mode
        if self.queue_expanded {
            // Queue expanded: Queue takes full main area
            self.draw_queue_expanded(frame, chunks[1]);
        } else if self.my_mix_expanded {
            // My Mix expanded: My Mix takes full main area
            self.draw_my_mix_expanded(frame, chunks[1]);
        } else if self.history_expanded {
            // History expanded: History takes full main area
            self.draw_history_expanded(frame, chunks[1]);
        } else if self.playlist_loading_expanded {
            // Playlist loading expanded: Show URL input interface
            self.draw_playlist_loading_expanded(frame, chunks[1]);
        } else if self.current_view == ViewMode::Search || matches!(self.mode, AppMode::Searching) {
            // Search view: Search Results (left) | History (right)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            self.draw_search_results(frame, main_chunks[0]);
            self.draw_history(frame, main_chunks[1]);
        } else {
            // Home view (default): Queue (left) | History (right)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            self.draw_queue_compact(frame, main_chunks[0]);
            self.draw_history(frame, main_chunks[1]);
        }

        // Bottom bar: Player (50%) | Cache (15%) | Playlists (35%)
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),  // Player
                Constraint::Percentage(15),  // Cache/Downloads
                Constraint::Percentage(35),  // Playlists (more space!)
            ])
            .split(chunks[2]);

        self.draw_player_compact(frame, bottom_chunks[0]);
        self.draw_cache_stats(frame, bottom_chunks[1]);
        self.draw_my_mix(frame, bottom_chunks[2]);
    }

    fn draw_search_results(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let results: Vec<ListItem> = self
            .search_results
            .iter()
            .enumerate()
            .map(|(i, video)| {
                let content = video.title.clone();
                let style = if i == self.selected_result {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let results_list = List::new(results)
            .block(Block::default().borders(Borders::ALL).title("Search Results"));
        frame.render_widget(results_list, area);
    }

    fn draw_queue_compact(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Show queue items vertically
        let queue_len = self.queue.len();

        if queue_len == 0 {
            let queue_widget = Paragraph::new("Queue is empty - Add tracks by pressing Enter on search results")
                .block(Block::default().borders(Borders::ALL).title("Queue (0 tracks) - Press 't' to expand for management"));
            frame.render_widget(queue_widget, area);
        } else {
            // Calculate how many items fit in the visible area (fill the whole box!)
            let visible_height = area.height.saturating_sub(2) as usize; // Subtract borders
            let max_items = visible_height.min(queue_len);

            let queue_slice = self.queue.get_queue_slice(0, max_items);

            let items: Vec<ListItem> = queue_slice
                .iter()
                .enumerate()
                .map(|(i, track)| {
                    let content = format!("{}. {}", i + 1, &track.title);
                    ListItem::new(content).style(Style::default().fg(Color::White))
                })
                .collect();

            let queue_list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("Queue ({} tracks) - Press 't' to expand for management", queue_len)));
            frame.render_widget(queue_list_widget, area);
        }
    }

    fn draw_queue_expanded(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let total_tracks = self.queue.len();

        // Calculate visible window (show items around selected item)
        let visible_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let half_window = visible_height / 2;

        let (start_idx, end_idx) = if total_tracks <= visible_height {
            // Show all if fits on screen
            (0, total_tracks)
        } else {
            // Calculate scrolling window
            let start = self.selected_queue_item.saturating_sub(half_window);
            let end = (start + visible_height).min(total_tracks);

            // Adjust if we're at the end
            if end == total_tracks && total_tracks > visible_height {
                (total_tracks - visible_height, total_tracks)
            } else {
                (start, end)
            }
        };

        // Only get visible slice of tracks - huge performance improvement!
        let visible_count = end_idx - start_idx;
        let queue_slice = self.queue.get_queue_slice(start_idx, visible_count);

        let queue_items: Vec<ListItem> = queue_slice
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let actual_idx = start_idx + i;
                let content = format!("{}. {}", actual_idx + 1, &track.title);
                let style = if actual_idx == self.selected_queue_item {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let scroll_indicator = if total_tracks > visible_height {
            format!(" (Showing {}-{} of {})", start_idx + 1, end_idx, total_tracks)
        } else {
            String::new()
        };

        let queue_list = List::new(queue_items)
            .block(Block::default().borders(Borders::ALL).title(format!("Queue (Expanded) - {} tracks{} | [j/k] Navigate | [d] Delete | [t] Collapse", total_tracks, scroll_indicator)));
        frame.render_widget(queue_list, area);
    }

    fn draw_history(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let queue_history = self.queue.get_history();
        let total_history = queue_history.len();

        // Calculate how many items fit in the visible area (fill the whole box like queue!)
        let visible_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let max_items = visible_height.min(total_history);

        let history_items: Vec<ListItem> = queue_history
            .iter()
            .rev()  // Show most recent first
            .take(max_items)  // Fill the box!
            .map(|track| {
                let content = track.title.clone();
                ListItem::new(content).style(Style::default().fg(Color::DarkGray))
            })
            .collect();

        let history_list = List::new(history_items)
            .block(Block::default().borders(Borders::ALL).title(format!("History ({} played) - Press [Shift+H] to expand", total_history)));
        frame.render_widget(history_list, area);
    }

    fn draw_history_expanded(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let queue_history = self.queue.get_history();
        let total_history = queue_history.len();

        // Calculate visible window
        let visible_height = area.height.saturating_sub(2) as usize;
        let half_window = visible_height / 2;

        let (start_idx, end_idx) = if total_history <= visible_height {
            (0, total_history)
        } else {
            let start = self.selected_history_item.saturating_sub(half_window);
            let end = (start + visible_height).min(total_history);
            if end == total_history && total_history > visible_height {
                (total_history - visible_height, total_history)
            } else {
                (start, end)
            }
        };

        // Only render visible window
        let history_items: Vec<ListItem> = queue_history
            .iter()
            .rev()
            .enumerate()
            .skip(start_idx)
            .take(end_idx - start_idx)
            .map(|(i, track)| {
                let content = format!("{}. {}", i + 1, &track.title);
                let style = if i == self.selected_history_item {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let scroll_indicator = if total_history > visible_height {
            format!(" (Showing {}-{} of {})", start_idx + 1, end_idx, total_history)
        } else {
            String::new()
        };

        let history_list = List::new(history_items)
            .block(Block::default().borders(Borders::ALL).title(format!("History (Expanded) - {} played{} | [j/k] Navigate | [Shift+C] Clear | [Shift+H] Collapse", total_history, scroll_indicator)));
        frame.render_widget(history_list, area);
    }

    fn draw_my_mix(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mix_items: Vec<ListItem> = if !self.loaded_playlist_tracks.is_empty() {
            // Show first 50 tracks from loaded playlist
            self.loaded_playlist_tracks
                .iter()
                .take(50)
                .enumerate()
                .map(|(i, track)| {
                    let duration = Self::format_time(track.duration as f64);
                    let content = format!("{}. {} [{}]", i + 1, &track.title, duration);
                    ListItem::new(content).style(Style::default().fg(Color::White))
                })
                .collect()
        } else if self.my_mix_playlists.is_empty() {
            vec![
                ListItem::new("Press 'l' to load a playlist URL").style(Style::default().fg(Color::Yellow)),
                ListItem::new(""),
                ListItem::new("YouTube Music playlists supported!"),
            ]
        } else {
            self.my_mix_playlists
                .iter()
                .enumerate()
                .map(|(i, mix)| {
                    let content = if mix.track_count > 0 {
                        format!("{} ({} tracks)", mix.title, mix.track_count)
                    } else {
                        mix.title.clone()
                    };
                    let style = if i == self.selected_mix_item {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(content).style(style)
                })
                .collect()
        };

        let title = if !self.loaded_playlist_name.is_empty() {
            format!("{} - Press [l] to load another", self.loaded_playlist_name)
        } else {
            "Playlists - Press [l] to load playlist URL".to_string()
        };

        let mix_list = List::new(mix_items)
            .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(mix_list, area);
    }

    fn draw_my_mix_expanded(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mix_items: Vec<ListItem> = self.my_mix_playlists
            .iter()
            .enumerate()
            .map(|(i, mix)| {
                let content = format!("{}. {} ({} tracks)", i + 1, mix.title, mix.track_count);
                let style = if i == self.selected_mix_item {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let mix_list = List::new(mix_items)
            .block(Block::default().borders(Borders::ALL).title("My Mix (Expanded) - [j/k] Navigate | [Enter] Add to queue | [m] Collapse | [Shift+m] Refresh"));
        frame.render_widget(mix_list, area);
    }

    fn draw_playlist_loading_expanded(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Expanded playlist loading interface - shows input prominently
        let loading_text = vec![
            "📋 Load Playlist from URL",
            "",
            "Paste your YouTube or YouTube Music playlist URL below:",
            "",
            &format!("URL: {}_", self.playlist_url),
            "",
            "",
            "Instructions:",
            "  • Paste a YouTube Music playlist URL",
            "  • Paste a YouTube playlist URL",
            "  • Press Enter to load",
            "  • Press Esc to cancel",
            "",
            "",
            "Example URLs:",
            "  https://music.youtube.com/playlist?list=...",
            "  https://www.youtube.com/playlist?list=...",
        ].join("\n");

        let loading_widget = Paragraph::new(loading_text)
            .block(Block::default().borders(Borders::ALL).title("Load Playlist (Expanded) - Press [Esc] to cancel"))
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Left);

        frame.render_widget(loading_widget, area);
    }

    fn draw_player_compact(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Single Player box with 3 lines of content inside
        let current_track = self.queue.get_current();

        // Line 1: Now Playing title (rotating if too long)
        let now_playing = if let Some(track) = current_track {
            let clean_title = Self::clean_title(&track.title);
            let full_text = format!("{} - {}", clean_title, track.uploader);

            // Scroll text if too long (more than 80 chars)
            if full_text.len() > 80 {
                let scroll_pos = self.title_scroll_offset % full_text.len();
                let rotated = format!("{}   {}", &full_text[scroll_pos..], &full_text[..scroll_pos]);
                format!("Now Playing: {}", &rotated[..80.min(rotated.len())])
            } else {
                format!("Now Playing: {}", full_text)
            }
        } else {
            "No track playing".to_string()
        };

        // Line 2: Progress bar with bouncing visualization
        let time_pos = self.player.get_time_pos();
        let player_duration = self.player.get_duration();

        // ALWAYS prefer player duration (from actual audio) over track.duration (often 0 from flat-playlist)
        // Also use track.duration as last resort if available and > 0
        let duration = if player_duration > 0.0 {
            player_duration
        } else if let Some(track) = current_track {
            if track.duration > 0 {
                track.duration as f64
            } else {
                0.0
            }
        } else {
            0.0
        };

        let progress_ratio = if duration > 0.0 {
            (time_pos / duration * 100.0).min(100.0) as u16
        } else {
            0
        };

        // Build progress bar string with bouncing bars
        let progress_visual = if self.player.get_state() == PlayerState::Playing {
            // Always show bouncing bars when playing
            let frame = (self.animation_frame / 4) % 8;
            let bars = match frame {
                0 => "▁▂▃▄▅▆▇█",
                1 => "▂▃▄▅▆▇█▇",
                2 => "▃▄▅▆▇█▇▆",
                3 => "▄▅▆▇█▇▆▅",
                4 => "▅▆▇█▇▆▅▄",
                5 => "▆▇█▇▆▅▄▃",
                6 => "▇█▇▆▅▄▃▂",
                7 => "█▇▆▅▄▃▂▁",
                _ => "▄▄▄▄▄▄▄▄",
            };

            if duration > 0.0 {
                // Show elapsed/total when duration is known
                format!("{} {}/{}", bars, Self::format_time(time_pos), Self::format_time(duration))
            } else {
                // Just show elapsed time when duration isn't available yet
                format!("{} {}", bars, Self::format_time(time_pos))
            }
        } else if duration > 0.0 {
            format!("{}/{}", Self::format_time(time_pos), Self::format_time(duration))
        } else {
            "Not playing".to_string()
        };

        // Just bouncy bars + timer, no progress bar
        let progress_bar = if self.player.get_state() == PlayerState::Playing {
            // Bouncing bars animation
            let frame = (self.animation_frame / 4) % 8;
            let bars = match frame {
                0 => "▁▂▃▄▅▆▇█▁▂▃▄▅▆▇█▁▂▃▄▅▆▇█",
                1 => "▂▃▄▅▆▇█▇▂▃▄▅▆▇█▇▂▃▄▅▆▇█▇",
                2 => "▃▄▅▆▇█▇▆▃▄▅▆▇█▇▆▃▄▅▆▇█▇▆",
                3 => "▄▅▆▇█▇▆▅▄▅▆▇█▇▆▅▄▅▆▇█▇▆▅",
                4 => "▅▆▇█▇▆▅▄▅▆▇█▇▆▅▄▅▆▇█▇▆▅▄",
                5 => "▆▇█▇▆▅▄▃▆▇█▇▆▅▄▃▆▇█▇▆▅▄▃",
                6 => "▇█▇▆▅▄▃▂▇█▇▆▅▄▃▂▇█▇▆▅▄▃▂",
                7 => "█▇▆▅▄▃▂▁█▇▆▅▄▃▂▁█▇▆▅▄▃▂▁",
                _ => "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄",
            };

            // Just bars + timer
            if duration > 0.0 {
                format!("{} {}/{}", bars, Self::format_time(time_pos), Self::format_time(duration))
            } else {
                format!("{} {}", bars, Self::format_time(time_pos))
            }
        } else {
            // When paused, just show time
            if duration > 0.0 {
                format!("{}/{}", Self::format_time(time_pos), Self::format_time(duration))
            } else {
                "Not playing".to_string()
            }
        };

        // Line 3: Status info
        let state_str = match self.player.get_state() {
            PlayerState::Playing => "▶ Playing",
            PlayerState::Paused => "⏸ Paused",
            PlayerState::Stopped => "⏹ Stopped",
            PlayerState::Loading => "... Loading",
        };

        let volume = self.player.get_volume();
        let status_line = format!("{} | Vol: {}% | Queue: {} tracks", state_str, volume, self.queue.size());

        // Combine all 3 lines inside single Player box
        let player_content = format!("{}\n{}\n{}", now_playing, progress_bar, status_line);

        let player_widget = Paragraph::new(player_content)
            .block(Block::default().borders(Borders::ALL).title("Player"))
            .style(Style::default().fg(Color::Cyan));

        frame.render_widget(player_widget, area);
    }

    fn draw_cache_stats(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Cache/Download stats box
        let active_count = self.active_downloads.lock().ok().map(|c| *c).unwrap_or(0);
        let cached_count = self.downloaded_files.lock().ok().map(|f| f.len()).unwrap_or(0);

        let cache_info = if active_count > 0 {
            format!("{}\n⬇ {}\n💾 {}",
                self.get_download_animation(),
                active_count,
                cached_count)
        } else {
            format!("💾\n{}\ncached", cached_count)
        };

        let cache_widget = Paragraph::new(cache_info)
            .block(Block::default().borders(Borders::ALL).title("Cache"))
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);

        frame.render_widget(cache_widget, area);
    }

    fn draw_player_unified(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Unified Player: Combines player info, playback progress, and cache stats
        let current_track = self.queue.get_current();
        let now_playing = if let Some(track) = current_track {
            let clean_title = Self::clean_title(&track.title);
            format!("Now Playing: {} - {}", clean_title, track.uploader)
        } else {
            "No track playing".to_string()
        };

        let state_str = match self.player.get_state() {
            PlayerState::Playing => "▶ Playing",
            PlayerState::Paused => "⏸ Paused",
            PlayerState::Stopped => "⏹ Stopped",
            PlayerState::Loading => "... Loading",
        };

        let volume = self.player.get_volume();
        let time_pos = self.player.get_time_pos();
        let duration = self.player.get_duration();

        // Get download stats
        let active_count = self.active_downloads.lock().ok().map(|c| *c).unwrap_or(0);
        let cached_count = self.downloaded_files.lock().ok().map(|f| f.len()).unwrap_or(0);

        // Build unified info
        let download_status = if active_count > 0 {
            format!("{} ⬇ {} | 💾 {}", self.get_download_animation(), active_count, cached_count)
        } else {
            format!("💾 {} cached", cached_count)
        };

        // Playback visualization
        let progress_ratio = if duration > 0.0 {
            (time_pos / duration * 100.0).min(100.0) as u16
        } else {
            0
        };

        let playback_visual = if self.player.get_state() == PlayerState::Playing && duration > 0.0 {
            format!("{} {}/{}",
                self.get_playback_visualization(progress_ratio).split_whitespace().next().unwrap_or("▄▄▄▄▄▄▄▄"),
                Self::format_time(time_pos),
                Self::format_time(duration))
        } else if duration > 0.0 {
            format!("{}/{}", Self::format_time(time_pos), Self::format_time(duration))
        } else {
            "00:00".to_string()
        };

        let unified_info = format!(
            "{}\n{} | Vol: {}% | {} | Queue: {} | {}",
            now_playing,
            state_str,
            volume,
            playback_visual,
            self.queue.size(),
            download_status
        );

        let player_widget = Paragraph::new(unified_info)
            .block(Block::default().borders(Borders::ALL).title("Player"))
            .style(Style::default().fg(Color::Cyan));

        frame.render_widget(player_widget, area);
    }

    fn draw_player_bar(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Split the player bar into info section and progress bar (smaller!)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),  // Info section
                Constraint::Length(2),  // Progress bar (reduced from 3)
            ])
            .split(area);

        let current_track = self.queue.get_current();
        let now_playing = if let Some(track) = current_track {
            let clean_title = Self::clean_title(&track.title);
            format!("Now Playing: {} - {}", clean_title, track.uploader)
        } else {
            "No track playing".to_string()
        };

        let state_str = match self.player.get_state() {
            PlayerState::Playing => "▶ Playing",
            PlayerState::Paused => "⏸ Paused",
            PlayerState::Stopped => "⏹ Stopped",
            PlayerState::Loading => "... Loading",
        };

        let volume = self.player.get_volume();
        let time_pos = self.player.get_time_pos();
        let duration = self.player.get_duration();
        let time_str = if duration > 0.0 {
            format!("{} / {}", Self::format_time(time_pos), Self::format_time(duration))
        } else {
            Self::format_time(time_pos)
        };

        let player_info = format!(
            "{}\nState: {} | Volume: {}% | Time: {} | Queue: {} tracks remaining",
            now_playing,
            state_str,
            volume,
            time_str,
            self.queue.size()
        );

        let player_widget = Paragraph::new(player_info)
            .block(Block::default().borders(Borders::ALL).title("Player"));
        frame.render_widget(player_widget, chunks[0]);

        // Progress bar
        let progress_ratio = if duration > 0.0 {
            (time_pos / duration * 100.0).min(100.0) as u16
        } else {
            0
        };

        let progress_label = if duration > 0.0 {
            // Show playback progress
            let active_count = self.active_downloads.lock().ok().map(|c| *c).unwrap_or(0);
            let cached_count = self.downloaded_files.lock().ok().map(|f| f.len()).unwrap_or(0);
            if active_count > 0 {
                format!("{} / {} | {} {} downloading, {} cached",
                    Self::format_time(time_pos), Self::format_time(duration),
                    self.get_download_animation(), active_count, cached_count)
            } else if cached_count > 0 {
                format!("{} / {} | {} cached",
                    Self::format_time(time_pos), Self::format_time(duration), cached_count)
            } else {
                format!("{} / {}", Self::format_time(time_pos), Self::format_time(duration))
            }
        } else if let Some(downloading) = &self.currently_downloading {
            let active_count = self.active_downloads.lock().ok().map(|c| *c).unwrap_or(0);
            let cached_count = self.downloaded_files.lock().ok().map(|f| f.len()).unwrap_or(0);
            if active_count > 1 {
                format!("{} Downloading {} tracks ({} cached)", self.get_download_animation(), active_count, cached_count)
            } else {
                format!("{} Downloading: {} ({} cached)", self.get_download_animation(), downloading, cached_count)
            }
        } else {
            // No track loaded - but show download activity if present
            let active_count = self.active_downloads.lock().ok().map(|c| *c).unwrap_or(0);
            let cached_count = self.downloaded_files.lock().ok().map(|f| f.len()).unwrap_or(0);

            if active_count > 0 {
                format!("{} Downloading {} tracks | {} cached", self.get_download_animation(), active_count, cached_count)
            } else if cached_count > 0 {
                format!("No track loaded | {} cached", cached_count)
            } else {
                "No track loaded".to_string()
            }
        };

        // Split progress bar: Playback (left) | Download Stats (right)
        let progress_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[1]);

        // Left: Playback progress with bouncing visualization
        let playback_visual = if self.player.get_state() == PlayerState::Playing {
            // Bouncing bars when playing
            self.get_playback_visualization(progress_ratio)
        } else {
            // Static bar when paused/stopped
            progress_label.clone()
        };

        let progress_bar = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Playback"))
            .gauge_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            )
            .percent(progress_ratio)
            .label(playback_visual);

        frame.render_widget(progress_bar, progress_chunks[0]);

        // Right: Download stats (compact)
        let active_count = self.active_downloads.lock().ok().map(|c| *c).unwrap_or(0);
        let cached_count = self.downloaded_files.lock().ok().map(|f| f.len()).unwrap_or(0);

        let download_info = if active_count > 0 {
            format!("{} ⬇ {}\n💾 {}", self.get_download_animation(), active_count, cached_count)
        } else {
            format!("💾 {} cached", cached_count)
        };

        let download_widget = Paragraph::new(download_info)
            .block(Block::default().borders(Borders::ALL).title("Cache"))
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);

        frame.render_widget(download_widget, progress_chunks[1]);
    }

    async fn handle_input(&mut self, key: KeyEvent) {
        // Clear status message on any key press (except when searching)
        if !matches!(self.mode, AppMode::Searching) {
            self.status_message.clear();
        }

        let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match self.mode {
            AppMode::LoginPrompt => {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => self.should_quit = true,
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        self.start_login().await;
                    }
                    _ => {}
                }
            }
            AppMode::AccountPicker => {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        self.mode = AppMode::LoginPrompt;
                    }
                    KeyCode::Char('j') | KeyCode::Down => self.next_account(),
                    KeyCode::Char('k') | KeyCode::Up => self.prev_account(),
                    KeyCode::Enter => {
                        self.select_account().await;
                    }
                    _ => {}
                }
            }
            AppMode::Searching => {
                match key.code {
                    KeyCode::Char(c) => {
                        self.search_query.push(c);
                    }
                    KeyCode::Backspace => {
                        self.search_query.pop();
                    }
                    KeyCode::Enter => {
                        let query = self.search_query.clone();
                        self.perform_search(&query).await;
                        self.mode = AppMode::Normal;
                        // Switch to Search view
                        self.previous_view = self.current_view.clone();
                        self.current_view = ViewMode::Search;
                        self.search_query.clear();
                    }
                    KeyCode::Esc => {
                        self.mode = AppMode::Normal;
                        self.search_query.clear();
                        // Return to previous view
                        let temp = self.current_view.clone();
                        self.current_view = self.previous_view.clone();
                        self.previous_view = temp;
                    }
                    _ => {}
                }
            }
            AppMode::LoadingPlaylist => {
                match key.code {
                    KeyCode::Char(c) => {
                        self.playlist_url.push(c);
                    }
                    KeyCode::Backspace => {
                        self.playlist_url.pop();
                    }
                    KeyCode::Enter => {
                        let url = self.playlist_url.clone();
                        if !url.is_empty() {
                            self.load_playlist_from_url(&url).await;
                        }
                        self.mode = AppMode::Normal;
                        self.playlist_url.clear();
                        self.playlist_loading_expanded = false;
                    }
                    KeyCode::Esc => {
                        self.mode = AppMode::Normal;
                        self.playlist_url.clear();
                        self.playlist_loading_expanded = false;
                        self.status_message = "Cancelled playlist loading".to_string();
                    }
                    _ => {}
                }
            }
            AppMode::Help => {
                match key.code {
                    KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = AppMode::Normal;
                    }
                    _ => {}
                }
            }
            AppMode::Normal => {
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('?') => self.mode = AppMode::Help,
                    KeyCode::Char('/') => self.mode = AppMode::Searching,
                    KeyCode::Char('l') => {
                        // Expand playlist loading view
                        self.mode = AppMode::LoadingPlaylist;
                        self.playlist_loading_expanded = true;
                        self.playlist_url.clear();
                        self.status_message = "Enter playlist URL (YouTube or YouTube Music)".to_string();
                    }
                    KeyCode::Char(' ') => self.toggle_pause_or_start().await,
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        if has_shift {
                            // Shift+H: Toggle history expansion
                            self.history_expanded = !self.history_expanded;
                            self.status_message = if self.history_expanded {
                                "History expanded - use j/k to navigate, Shift+C to clear".to_string()
                            } else {
                                "History collapsed".to_string()
                            };
                        } else {
                            // h: Go to home view
                            self.previous_view = self.current_view.clone();
                            self.current_view = ViewMode::Home;
                            self.status_message = "Returned to Home (My Mix)".to_string();
                        }
                    }
                    KeyCode::Char('m') => {
                        if has_shift {
                            // Shift+m: Refresh My Mix (only when expanded)
                            if self.my_mix_expanded {
                                self.status_message = "Refreshing My Mix...".to_string();
                                self.refresh_my_mix().await;
                            }
                        } else {
                            // m: Toggle My Mix expansion
                            self.my_mix_expanded = !self.my_mix_expanded;
                            self.status_message = if self.my_mix_expanded {
                                "My Mix expanded - use j/k to navigate, Shift+m to refresh".to_string()
                            } else {
                                "My Mix collapsed".to_string()
                            };
                        }
                    }
                    KeyCode::Char('M') => {
                        // Capital M (Shift+m): Refresh My Mix
                        if self.my_mix_expanded {
                            self.status_message = "Refreshing My Mix...".to_string();
                            self.refresh_my_mix().await;
                        }
                    }
                    KeyCode::Esc => {
                        // Return to previous view
                        let temp = self.current_view.clone();
                        self.current_view = self.previous_view.clone();
                        self.previous_view = temp;
                        self.status_message = format!("Returned to previous view");
                    }
                    KeyCode::Char('n') => {
                        self.status_message = "Playing next track...".to_string();
                        self.play_next().await;
                    }
                    KeyCode::Char('p') => {
                        self.status_message = "Playing previous track...".to_string();
                        self.play_previous().await;
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        self.queue_expanded = !self.queue_expanded;
                        self.status_message = if self.queue_expanded {
                            "Queue expanded - use j/k to navigate, d to delete".to_string()
                        } else {
                            "Queue collapsed".to_string()
                        };
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        self.delete_selected_queue_item();
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        if has_shift {
                            // Shift+C: Clear history (only when expanded)
                            if self.history_expanded {
                                self.clear_history();
                            }
                        }
                    }
                    KeyCode::Up => self.volume_up(has_shift),
                    KeyCode::Down => self.volume_down(has_shift),
                    KeyCode::Right => self.seek_forward(),
                    KeyCode::Left => self.seek_backward(),
                    KeyCode::Char('j') => {
                        if self.queue_expanded {
                            self.next_queue_item();
                        } else if self.my_mix_expanded {
                            self.next_mix_item();
                        } else if self.history_expanded {
                            self.next_history_item();
                        } else if self.current_view == ViewMode::Home {
                            self.next_mix_item();
                        } else {
                            self.next_search_result();
                        }
                    }
                    KeyCode::Char('k') => {
                        if self.queue_expanded {
                            self.prev_queue_item();
                        } else if self.my_mix_expanded {
                            self.prev_mix_item();
                        } else if self.history_expanded {
                            self.prev_history_item();
                        } else if self.current_view == ViewMode::Home {
                            self.prev_mix_item();
                        } else {
                            self.prev_search_result();
                        }
                    }
                    KeyCode::Enter => {
                        if self.current_view == ViewMode::Home {
                            self.add_selected_mix_to_queue().await;
                        } else {
                            self.add_selected_to_queue();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    async fn perform_search(&mut self, query: &str) {
        // Mark as searching
        self.is_searching = true;

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
        // NOTE: No trigger_smart_downloads() here - sliding window handles it!
        // CRITICAL: Clear pending state FIRST so navigation always works
        self.pending_play_track = None;
        self.currently_downloading = None;

        // Pure next - always advances to next track (use SPACE to start first track!)
        if let Some(track) = self.queue.next() {
            // Limit history to 100 most recent tracks to prevent memory issues
            self.queue.limit_history(100);

            // Check if already in cache
            let cached_file = self.downloaded_files.lock().ok()
                .and_then(|files| files.get(&track.video_id).cloned());

            if let Some(local_file) = cached_file {
                // Verify file actually exists before playing (5-min cleanup might have deleted it)
                if std::path::Path::new(&local_file).exists() {
                    // ✅ INSTANT PLAYBACK - Already in cache!
                    self.player.play_with_duration(&local_file, &track.title, track.duration as f64);
                    self.status_message = "".to_string();  // Clear status - player shows track

                    // PROACTIVE: Ensure next track is downloading for instant skip
                    self.ensure_next_track_ready();
                } else {
                    // File was deleted - remove from cache and re-download
                    self.downloaded_files.lock().ok().map(|mut files| files.remove(&track.video_id));
                    self.pending_play_track = Some(track.clone());
                    if self.spawn_download_with_limit(&track) {
                        self.currently_downloading = Some(track.title.clone());
                    }
                }
            } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
                // Not in cache - spawn download (rate-limited!)
                // Always set pending track (even if rate limited - will retry when slot opens)
                self.pending_play_track = Some(track.clone());

                // Only set downloading state if spawn succeeds
                if self.spawn_download_with_limit(&track) {
                    self.currently_downloading = Some(track.title.clone());
                }
                // If rate limited, silent - will auto-retry when download slot opens
            } else {
                // Direct URL (not YouTube)
                self.player.play_with_duration(&track.url, &track.title, track.duration as f64);
                self.status_message = "".to_string();  // Clear status - player shows track
            }
        } else {
            self.status_message = "Queue is empty!".to_string();
        }
    }

    async fn play_previous(&mut self) {
        // CRITICAL: Clear pending state FIRST so navigation always works
        self.pending_play_track = None;
        self.currently_downloading = None;

        if let Some(track) = self.queue.previous() {
            // Limit history to 100 most recent tracks to prevent memory issues
            self.queue.limit_history(100);

            // Check if already in cache
            let cached_file = self.downloaded_files.lock().ok()
                .and_then(|files| files.get(&track.video_id).cloned());

            if let Some(local_file) = cached_file {
                // Verify file actually exists before playing (5-min cleanup might have deleted it)
                if std::path::Path::new(&local_file).exists() {
                    // ✅ INSTANT PLAYBACK - Already in cache!
                    self.player.play_with_duration(&local_file, &track.title, track.duration as f64);
                    self.status_message = "".to_string();  // Clear status - player shows track

                    // PROACTIVE: Ensure next track is downloading for instant skip
                    self.ensure_next_track_ready();
                } else {
                    // File was deleted - remove from cache and re-download
                    self.downloaded_files.lock().ok().map(|mut files| files.remove(&track.video_id));
                    self.pending_play_track = Some(track.clone());
                    if self.spawn_download_with_limit(&track) {
                        self.currently_downloading = Some(track.title.clone());
                    }
                }
            } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
                // Not in cache - spawn download (rate-limited!)
                // Always set pending track (even if rate limited - will retry when slot opens)
                self.pending_play_track = Some(track.clone());

                // Only set downloading state if spawn succeeds
                if self.spawn_download_with_limit(&track) {
                    self.currently_downloading = Some(track.title.clone());
                }
                // If rate limited, silent - will auto-retry when download slot opens
            } else {
                // Direct URL (not YouTube)
                self.player.play_with_duration(&track.url, &track.title, track.duration as f64);
                self.status_message = "".to_string();  // Clear status - player shows track
            }
        } else {
            self.status_message = "No previous track!".to_string();
        }
    }

    // ==========================================
    // CENTRALIZED DOWNLOAD SYSTEM
    // ==========================================
    // This is the ONLY function that should spawn downloads.
    // All other code paths must call this function.
    //
    // Features:
    // - Global rate limiting (max 30 concurrent downloads for fast bulk downloading)
    // - Populates cache only (not tied to specific playback)
    // - Tracks active downloads with atomic counter
    // - Handles success/failure and updates appropriate maps
    fn spawn_download_with_limit(&self, track: &Track) -> bool {
        // RATE LIMIT: Allow up to 30 concurrent downloads for fast bulk downloading
        // Increased from 15 to improve performance, especially when window is unfocused
        let active_count = {
            let count = self.active_downloads.lock().ok();
            count.map(|c| *c).unwrap_or(0)
        };

        if active_count >= 30 {
            // Already at max downloads, skip
            return false;
        }

        let video_id = &track.video_id;

        // Skip if already downloaded
        if let Ok(files) = self.downloaded_files.lock() {
            if files.contains_key(video_id) {
                return false;  // Already in cache
            }
        }

        // Skip if download already failed (will retry when playing)
        if let Ok(failed) = self.failed_downloads.lock() {
            if failed.contains_key(video_id) {
                return false;  // Known failure
            }
        }

        // CRITICAL: Skip if already downloading (prevents SPACE spam duplicates!)
        if let Ok(mut downloading) = self.downloading_videos.lock() {
            if downloading.contains(video_id) {
                return false;  // Already downloading this video
            }
            // Mark as downloading
            downloading.insert(video_id.clone());
        }

        // Increment active download counter
        if let Ok(mut count) = self.active_downloads.lock() {
            *count += 1;
        }

        let video_id = track.video_id.clone();
        let youtube_url = track.url.clone();
        let cookie_config = self.browser_auth.load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account));
        let downloaded_files = self.downloaded_files.clone();
        let failed_downloads = self.failed_downloads.clone();
        let active_downloads = self.active_downloads.clone();
        let downloading_videos = self.downloading_videos.clone();
        let download_tx = self.download_tx.clone();

        let handle = tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                Self::fetch_audio_url_blocking(&youtube_url, cookie_config)
            }).await;

            match result {
                Ok(Ok(file_path)) => {
                    // Add to cache
                    if let Ok(mut files) = downloaded_files.lock() {
                        files.insert(video_id.clone(), file_path.clone());
                    }
                    // Notify main thread with ACTUAL file path (for auto-play if pending)
                    let _ = download_tx.send((video_id.clone(), Ok(file_path)));
                }
                Ok(Err(e)) => {
                    if let Ok(mut failed) = failed_downloads.lock() {
                        failed.insert(video_id.clone(), e.clone());
                    }
                    let _ = download_tx.send((video_id.clone(), Err(e)));
                }
                Err(e) => {
                    let error_msg = format!("Task error: {}", e);
                    if let Ok(mut failed) = failed_downloads.lock() {
                        failed.insert(video_id.clone(), error_msg.clone());
                    }
                    let _ = download_tx.send((video_id.clone(), Err(error_msg)));
                }
            }

            // IMPORTANT: Decrement active download count and remove from in-flight tracker
            if let Ok(mut count) = active_downloads.lock() {
                *count = count.saturating_sub(1);
            }
            // Remove from downloading tracker (allows retry if user presses SPACE again)
            if let Ok(mut downloading) = downloading_videos.lock() {
                downloading.remove(&video_id);
            }
        });

        // Track the background task for proper cleanup
        if let Ok(mut tasks) = self.background_tasks.lock() {
            tasks.push(handle);
        }

        true  // Download was spawned
    }

    // Trigger smart downloads: download tracks near current position
    // PROACTIVE BUFFER BUILDING - For instant playback as you navigate
    // ==========================================
    // Called when a track starts playing to keep building the download buffer
    // Downloads next 10 tracks to maintain smooth playback without CPU spikes
    fn ensure_next_track_ready(&self) {
        // Download next 10 tracks from queue position 0
        // This keeps a rolling buffer as user plays through the playlist
        // Distributed load: 10 tracks per song = smooth, no spike
        let next_tracks = self.queue.get_queue_slice(0, 10);
        for track in next_tracks.iter() {
            self.spawn_download_with_limit(track);
        }
    }

    fn trigger_smart_downloads(&self) {
        // LIGHTWEIGHT strategy: Download next 10 tracks from queue (reduced from 20)
        // Less concurrent downloads = faster priority track completion
        // With 30 concurrent downloads max, this still provides good buffering
        // ensure_next_track_ready() handles immediate next track
        // History tracks stay cached automatically (never deleted)

        // Download next 10 tracks from queue position 0 (reduced for FAST UX)
        let next_tracks = self.queue.get_queue_slice(0, 10);
        for track in next_tracks.iter() {
            self.spawn_download_with_limit(track);
        }

        // History is already cached - no need to re-download!
    }

    // Clean up old pre-downloaded files from temp directory
    fn cleanup_old_downloads() {
        use std::env;
        use std::time::{SystemTime, Duration};

        let temp_dir = env::temp_dir();

        // Try to read temp directory
        if let Ok(entries) = std::fs::read_dir(&temp_dir) {
            let now = SystemTime::now();
            let max_age = Duration::from_secs(3600); // 1 hour

            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    // Only target our audio files
                    if file_name.starts_with("yt-music-audio-") {
                        // Check file age
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if let Ok(age) = now.duration_since(modified) {
                                    // Delete files older than max_age
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

    // Helper to download audio to temp file using yt-dlp
    fn fetch_audio_url_blocking(youtube_url: &str, cookie_config: Option<(bool, String)>) -> Result<String, String> {
        use std::process::Command;
        use std::env;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Create unique temp file path for audio download
        let temp_dir = env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();
        let temp_file = temp_dir.join(format!("yt-music-audio-{}-{}.%(ext)s", std::process::id(), timestamp));

        let mut cmd = Command::new("yt-dlp");
        cmd.arg("-f")
            .arg("bestaudio/best")  // Get best audio
            .arg("-x")              // Extract audio only
            .arg("--audio-format")
            .arg("mp3")             // Convert to MP3 (universally supported)
            .arg("--audio-quality")
            .arg("192K")            // 192 kbps bitrate
            .arg("-o")
            .arg(&temp_file)        // Output to temp file (yt-dlp will replace %(ext)s)
            .arg("--no-playlist")   // Don't download playlists
            .arg("--no-mtime");     // Don't preserve modification time

        // Add cookies from browser if available
        if let Some((_use_from_browser, cookie_arg)) = cookie_config {
            cmd.arg("--cookies-from-browser").arg(cookie_arg);
        }

        cmd.arg(youtube_url);

        // Run yt-dlp and wait for completion
        let output = cmd.output()
            .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp download failed: {}", error));
        }

        // yt-dlp replaces %(ext)s with actual extension, so find the file
        let temp_dir_path = env::temp_dir();
        let search_pattern = format!("yt-music-audio-{}-{}", std::process::id(), timestamp);

        // Find the downloaded file
        let files: Vec<_> = std::fs::read_dir(&temp_dir_path)
            .map_err(|e| format!("Failed to read temp dir: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_name()
                    .to_string_lossy()
                    .starts_with(&search_pattern)
            })
            .collect();

        if files.is_empty() {
            return Err(format!("yt-dlp completed but no audio file found (searched for {}.*)", search_pattern));
        }

        let downloaded_file = files[0].path();

        // Verify file exists and has content
        let metadata = std::fs::metadata(&downloaded_file)
            .map_err(|e| format!("Failed to check downloaded file: {}", e))?;

        if metadata.len() == 0 {
            return Err("Downloaded file is empty".to_string());
        }

        if metadata.len() < 10000 {
            return Err(format!("Downloaded file is too small ({} bytes), likely incomplete", metadata.len()));
        }

        // Give extra time for file system to finish writing
        std::thread::sleep(std::time::Duration::from_millis(500));

        Ok(downloaded_file.to_string_lossy().to_string())
    }

    async fn toggle_pause_or_start(&mut self) {
        // SMART SPACE BAR:
        // 1. If in expanded queue -> play SELECTED track
        // 2. If nothing playing and queue has tracks -> START FIRST TRACK
        // 3. If player Stopped but track exists -> RELOAD and play current track
        // 4. If something playing -> toggle pause/resume

        if self.queue_expanded && !self.queue.is_empty() {
            // In expanded queue - play the SELECTED track!
            self.play_selected_queue_track().await;
        } else if self.queue.get_current().is_none() && !self.queue.is_empty() {
            // Nothing playing but queue has tracks - START PLAYING!
            self.play_current_or_first().await;
        } else if self.player.get_state() == PlayerState::Stopped && self.queue.get_current().is_some() {
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

        if self.selected_queue_item >= queue_list.len() {
            self.status_message = "Invalid selection".to_string();
            return;
        }

        let track = queue_list[self.selected_queue_item].clone();

        // Remove all tracks before and including selected from queue
        // This makes the selected track the "current" one
        for _ in 0..=self.selected_queue_item {
            self.queue.remove_at(0);
        }

        // Set as current track
        self.queue.restore_queue(self.queue.get_queue_list(), Some(track.clone()));

        // Now play it
        let cached_file = self.downloaded_files.lock().ok()
            .and_then(|files| files.get(&track.video_id).cloned());

        if let Some(local_file) = cached_file {
            // ✅ INSTANT PLAYBACK - Already in cache!
            self.player.play_with_duration(&local_file, &track.title, track.duration as f64);
            self.status_message = "".to_string();  // Clear status - player shows track

            // PROACTIVE: Ensure next track is downloading for instant skip
            self.ensure_next_track_ready();
        } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
            // Not in cache - spawn download (rate-limited!)
            // Always set pending track (even if rate limited - will retry when slot opens)
            self.pending_play_track = Some(track.clone());

            // Only set downloading state if spawn succeeds
            if self.spawn_download_with_limit(&track) {
                self.currently_downloading = Some(track.title.clone());
            }
        } else {
            // Direct URL (not YouTube)
            self.player.play_with_duration(&track.url, &track.title, track.duration as f64);
            self.status_message = "".to_string();  // Clear status - player shows track
        }

        // Collapse queue after selection
        self.queue_expanded = false;
        self.selected_queue_item = 0;
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

        // Now play the track (with smart pre-download logic)
        let cached_file = self.downloaded_files.lock().ok()
            .and_then(|files| files.get(&track.video_id).cloned());

        if let Some(local_file) = cached_file {
            // ✅ INSTANT PLAYBACK - Already in cache!
            self.player.play_with_duration(&local_file, &track.title, track.duration as f64);
            self.status_message = "".to_string();  // Clear status - player shows track

            // PROACTIVE: Ensure next track is downloading for instant skip
            self.ensure_next_track_ready();
        } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
            // Not in cache - spawn download (rate-limited!)
            // Always set pending track (even if rate limited - will retry when slot opens)
            self.pending_play_track = Some(track.clone());

            // Only set downloading state if spawn succeeds
            if self.spawn_download_with_limit(&track) {
                self.currently_downloading = Some(track.title.clone());
            }
            // If rate limited, silent - will auto-retry when download slot opens
        } else {
            // Direct URL (not YouTube)
            self.player.play_with_duration(&track.url, &track.title, track.duration as f64);
            self.status_message = "".to_string();  // Clear status - player shows track
        }
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
        self.status_message = format!("Seeked +10s ({})", Self::format_time(self.player.get_time_pos()));
    }

    fn seek_backward(&mut self) {
        // Seek backward 10 seconds
        self.player.seek_relative(-10.0);
        self.player.apply_seek();
        self.status_message = format!("Seeked -10s ({})", Self::format_time(self.player.get_time_pos()));
    }

    fn next_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.selected_result = (self.selected_result + 1) % self.search_results.len();
        }
    }

    fn prev_search_result(&mut self) {
        if !self.search_results.is_empty() {
            if self.selected_result == 0 {
                self.selected_result = self.search_results.len() - 1;
            } else {
                self.selected_result -= 1;
            }
        }
    }

    fn next_queue_item(&mut self) {
        let queue_len = self.queue.len();
        if queue_len > 0 {
            self.selected_queue_item = (self.selected_queue_item + 1) % queue_len;
            // HOVER DOWNLOAD: Start downloading this track immediately!
            self.trigger_hover_download(self.selected_queue_item);
        }
    }

    fn prev_queue_item(&mut self) {
        let queue_len = self.queue.len();
        if queue_len > 0 {
            if self.selected_queue_item == 0 {
                self.selected_queue_item = queue_len - 1;
            } else {
                self.selected_queue_item -= 1;
            }
            // HOVER DOWNLOAD: Start downloading this track immediately!
            self.trigger_hover_download(self.selected_queue_item);
        }
    }

    fn trigger_hover_download(&self, index: usize) {
        // SLIDING WINDOW STRATEGY:
        // Download ONLY 1 track at position +15 from hover
        // This maintains a 15-track lookahead buffer while only downloading 1 track per keypress
        // Super efficient - no CPU spikes! (Max 30 concurrent downloads total)
        let download_index = index + 15;

        // Get the track at position +15
        let queue_slice = self.queue.get_queue_slice(download_index, 1);
        if let Some(&track) = queue_slice.first() {
            // Use centralized download function (handles rate limiting, cache checks, etc.)
            self.spawn_download_with_limit(track);
        }
        // Going backward? Already cached from history! No download needed.
    }

    fn delete_selected_queue_item(&mut self) {
        if self.queue_expanded && !self.queue.is_empty() {
            if let Some(removed_track) = self.queue.remove_at(self.selected_queue_item) {
                let clean_title = Self::clean_title(&removed_track.title);
                self.status_message = format!("Removed '{}' from queue", clean_title);

                // Adjust selection if needed
                let queue_len = self.queue.len();
                if queue_len == 0 {
                    self.selected_queue_item = 0;
                } else if self.selected_queue_item >= queue_len {
                    self.selected_queue_item = queue_len - 1;
                }

                // Don't save on every action - only on exit
                // self.save_queue_async();
            }
        } else if !self.queue_expanded {
            self.status_message = "Press 't' to expand queue first".to_string();
        }
    }

    fn next_mix_item(&mut self) {
        if !self.my_mix_playlists.is_empty() {
            self.selected_mix_item = (self.selected_mix_item + 1) % self.my_mix_playlists.len();
        }
    }

    fn prev_mix_item(&mut self) {
        if !self.my_mix_playlists.is_empty() {
            if self.selected_mix_item == 0 {
                self.selected_mix_item = self.my_mix_playlists.len() - 1;
            } else {
                self.selected_mix_item -= 1;
            }
        }
    }

    fn next_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len > 0 {
            self.selected_history_item = (self.selected_history_item + 1) % history_len;
        }
    }

    fn prev_history_item(&mut self) {
        let history_len = self.queue.get_history().len();
        if history_len > 0 {
            if self.selected_history_item == 0 {
                self.selected_history_item = history_len - 1;
            } else {
                self.selected_history_item -= 1;
            }
        }
    }

    fn clear_history(&mut self) {
        let count = self.queue.get_history().len();
        self.queue.clear_history();
        self.selected_history_item = 0;
        self.status_message = format!("Cleared {} tracks from history", count);

        // Save to disk
        if let Err(e) = self.save_history() {
            self.status_message = format!("History cleared but failed to save: {}", e);
        }
    }

    async fn load_playlist_from_url(&mut self, url: &str) {
        self.status_message = format!("⏳ Loading playlist... (this may take a moment)");

        // Yield to allow UI to render the loading message before blocking fetch
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let cookie_config = self.browser_auth.load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account));

        let playlist_url = url.to_string();
        let fetch_result = tokio::task::spawn_blocking(move || {
            Self::fetch_playlist_tracks_blocking(&playlist_url, cookie_config)
        }).await;

        match fetch_result {
            Ok(Ok(tracks)) => {
                if tracks.is_empty() {
                    self.status_message = "No tracks found in playlist".to_string();
                    return;
                }

                let track_count = tracks.len();

                // Store loaded playlist for display
                self.loaded_playlist_tracks = tracks.clone();
                self.loaded_playlist_name = format!("Loaded Playlist ({} tracks)", track_count);

                // Add tracks to queue (filter out tracks > 5 minutes = 300 seconds)
                let mut added_count = 0;
                let mut filtered_count = 0;
                for track in tracks {
                    if track.duration <= 300 {
                        self.queue.add(track);
                        added_count += 1;
                    } else {
                        filtered_count += 1;
                    }
                }

                // Trigger smart downloads - downloads next 15 + previous 5
                self.trigger_smart_downloads();

                if filtered_count > 0 {
                    self.status_message = format!("Added {} tracks to queue ({} long tracks filtered out)", added_count, filtered_count);
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
        if let Some(mix) = self.my_mix_playlists.get(self.selected_mix_item).cloned() {
            self.status_message = format!("⏳ Fetching tracks from '{}'... (this may take a moment)", mix.title);

            // Yield to allow UI to render the loading message before blocking fetch
            tokio::task::yield_now().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let cookie_config = self.browser_auth.load_selected_account()
                .map(|account| self.browser_auth.get_cookie_arg(&account));

            let playlist_url = mix.url.clone();
            let fetch_result = tokio::task::spawn_blocking(move || {
                Self::fetch_playlist_tracks_blocking(&playlist_url, cookie_config)
            }).await;

            match fetch_result {
                Ok(Ok(tracks)) => {
                    if tracks.is_empty() {
                        self.status_message = format!("No tracks found in '{}'", mix.title);
                        return;
                    }

                    let track_count = tracks.len();

                    // Add tracks to queue (filter out tracks > 5 minutes = 300 seconds)
                    let mut added_count = 0;
                    let mut filtered_count = 0;
                    for track in tracks {
                        if track.duration <= 300 {
                            self.queue.add(track);
                            added_count += 1;
                        } else {
                            filtered_count += 1;
                        }
                    }

                    // Trigger smart downloads - downloads next 15 + previous 5
                    self.trigger_smart_downloads();

                    if filtered_count > 0 {
                        self.status_message = format!("Added {} tracks from '{}' ({} long tracks filtered out)", added_count, mix.title, filtered_count);
                    } else {
                        self.status_message = format!("Added {} tracks from '{}' to queue", added_count, mix.title);
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

    fn fetch_playlist_tracks_blocking(playlist_url: &str, cookie_config: Option<(bool, String)>) -> Result<Vec<Track>, String> {
        use std::process::Command;

        let mut cmd = Command::new("yt-dlp");
        cmd.arg("--flat-playlist")
            .arg("--dump-json")
            .arg("--no-warnings");

        // Add cookies from browser if available
        if let Some((_use_from_browser, cookie_arg)) = cookie_config {
            cmd.arg("--cookies-from-browser").arg(cookie_arg);
        }

        cmd.arg(playlist_url);

        let output = cmd.output()
            .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp failed: {}", error));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8: {}", e))?;

        let mut tracks = Vec::new();

        // Parse each line of JSON output
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                let video_id = json["id"].as_str().unwrap_or("").to_string();
                if video_id.is_empty() {
                    continue;
                }

                let title = json["title"].as_str().unwrap_or("Unknown").to_string();
                let duration = json["duration"].as_u64().unwrap_or(0);
                let uploader = json["uploader"].as_str()
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
        let cookie_config = self.browser_auth.load_selected_account()
            .map(|account| self.browser_auth.get_cookie_arg(&account));

        let fetch_result = tokio::task::spawn_blocking(move || {
            Self::fetch_my_mix_blocking(cookie_config)
        }).await;

        match fetch_result {
            Ok(Ok(playlists)) => {
                if playlists.is_empty() {
                    self.status_message = "No My Mix playlists found".to_string();
                } else {
                    self.my_mix_playlists = playlists;
                    self.status_message = format!("Loaded {} My Mix playlists", self.my_mix_playlists.len());
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

    fn fetch_my_mix_blocking(cookie_config: Option<(bool, String)>) -> Result<Vec<MixPlaylist>, String> {
        use std::process::Command;

        let mut cmd = Command::new("yt-dlp");
        cmd.arg("--flat-playlist")
            .arg("--dump-json")
            .arg("--no-warnings")
            .arg("--skip-download");

        // Add cookies from browser if available
        if let Some((_use_from_browser, cookie_arg)) = cookie_config {
            cmd.arg("--cookies-from-browser").arg(cookie_arg);
        }

        cmd.arg("https://music.youtube.com");

        let output = cmd.output()
            .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp failed: {}", error));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8: {}", e))?;

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
                        let playlist_id = json["id"].as_str()
                            .or_else(|| json["playlist_id"].as_str())
                            .unwrap_or("")
                            .to_string();

                        let title = json["title"].as_str()
                            .or_else(|| json["playlist_title"].as_str())
                            .unwrap_or("Untitled Mix")
                            .to_string();

                        let track_count = json["playlist_count"].as_u64()
                            .or_else(|| json["n_entries"].as_u64())
                            .unwrap_or(0) as usize;

                        let url = json["url"].as_str()
                            .or_else(|| json["webpage_url"].as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("https://music.youtube.com/playlist?list={}", playlist_id));

                        // Filter for My Mix playlists (auto-generated mixes)
                        // These typically have IDs starting with "RDCLAK", "RDAMPL", or contain "Mix" in title
                        if playlist_id.starts_with("RDCLAK")
                            || playlist_id.starts_with("RDAMPL")
                            || title.contains("Mix")
                            || title.contains("mix") {
                            playlists.push(MixPlaylist {
                                id: playlist_id,
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
        if let Some(video) = self.search_results.get(self.selected_result) {
            // Filter out tracks > 5 minutes (300 seconds) - this is a music player!
            if video.duration > 300 {
                let clean_title = Self::clean_title(&video.title);
                let mins = video.duration / 60;
                self.status_message = format!("'{}' is too long ({}min) - music only (<5min)", clean_title, mins);
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

            // Start background download for this track
            let video_id = track.video_id.clone();
            let youtube_url = track.url.clone();
            let cookie_config = self.browser_auth.load_selected_account()
                .map(|account| self.browser_auth.get_cookie_arg(&account));
            let downloaded_files = self.downloaded_files.clone();
            let failed_downloads = self.failed_downloads.clone();

            // Spawn background download
            let handle = tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    Self::fetch_audio_url_blocking(&youtube_url, cookie_config)
                }).await;

                match result {
                    Ok(Ok(file_path)) => {
                        // Store the downloaded file path
                        if let Ok(mut files) = downloaded_files.lock() {
                            files.insert(video_id, file_path);
                        }
                    }
                    Ok(Err(e)) => {
                        // Track failed download (yt-dlp error)
                        if let Ok(mut failed) = failed_downloads.lock() {
                            failed.insert(video_id, e);
                        }
                    }
                    Err(e) => {
                        // Track failed download (task join error)
                        let error_msg = format!("Task error: {}", e);
                        if let Ok(mut failed) = failed_downloads.lock() {
                            failed.insert(video_id, error_msg);
                        }
                    }
                }
            });

            // Track the background task for proper cleanup
            if let Ok(mut tasks) = self.background_tasks.lock() {
                tasks.push(handle);
            }

            self.queue.add(track);

            // Show feedback
            let clean_title = Self::clean_title(&video.title);
            self.status_message = format!("Added '{}' to queue! Downloading in background... ({} total)", clean_title, self.queue.len());

            if was_empty {
                self.status_message = format!("Added '{}' to queue! Press 'n' to play", clean_title);
            }

            // Save queue to disk
            if let Err(e) = self.save_queue() {
                eprintln!("Failed to save queue: {}", e);
            }
        }
    }

    fn format_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as u64;
        let secs = (seconds % 60.0) as u64;
        format!("{:02}:{:02}", mins, secs)
    }

    fn clean_title(title: &str) -> &str {
        // Just return the title as-is for now - fast path
        // We can add smarter cleaning later if needed
        title
    }

    async fn start_login(&mut self) {
        self.status_message = "Detecting YouTube accounts from browsers...".to_string();

        // Detect available accounts from Chrome/Firefox/Zen
        self.available_accounts = self.browser_auth.detect_accounts();

        if self.available_accounts.is_empty() {
            self.status_message = "No browser accounts found. Please login to YouTube in Chrome or Firefox first.".to_string();
        } else {
            self.status_message = format!("Found {} account(s). Select one:", self.available_accounts.len());
            self.selected_account_idx = 0;
            self.mode = AppMode::AccountPicker;
        }
    }

    fn next_account(&mut self) {
        if !self.available_accounts.is_empty() {
            self.selected_account_idx = (self.selected_account_idx + 1) % self.available_accounts.len();
        }
    }

    fn prev_account(&mut self) {
        if !self.available_accounts.is_empty() {
            if self.selected_account_idx == 0 {
                self.selected_account_idx = self.available_accounts.len() - 1;
            } else {
                self.selected_account_idx -= 1;
            }
        }
    }

    async fn select_account(&mut self) {
        if let Some(account) = self.available_accounts.get(self.selected_account_idx) {
            match self.browser_auth.save_selected_account(account) {
                Ok(_) => {
                    self.status_message = format!("✓ Logged in as {} - Press '/' to search for music!", account.display_name);
                    self.mode = AppMode::Normal;
                }
                Err(e) => {
                    self.status_message = format!("Failed to save account: {}", e);
                }
            }
        }
    }

    fn draw_account_picker(&self, frame: &mut Frame) {
        use ratatui::layout::{Alignment, Constraint, Direction, Layout};

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Min(10),
                Constraint::Percentage(20),
            ])
            .split(frame.size());

        // Header
        let header_text = vec![
            "Select YouTube Account",
            "",
            "Use j/k or ↑/↓ to navigate",
            "Press Enter to select",
            "Press Esc to go back",
            "",
        ].join("\n");

        let header = Paragraph::new(header_text)
            .block(Block::default().borders(Borders::ALL).title("Account Selection"))
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);

        frame.render_widget(header, chunks[0]);

        // Account list
        let account_items: Vec<ListItem> = self
            .available_accounts
            .iter()
            .enumerate()
            .map(|(i, account)| {
                let content = format!("{}", account.display_name);
                let style = if i == self.selected_account_idx {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let account_list = List::new(account_items)
            .block(Block::default().borders(Borders::ALL).title("Available Accounts"));

        frame.render_widget(account_list, chunks[1]);
    }

    fn draw_help_screen(&self, frame: &mut Frame) {
        use ratatui::layout::{Alignment, Constraint, Direction, Layout};

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Min(10),
                Constraint::Percentage(10),
            ])
            .split(frame.size());

        let help_text = vec![
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━ KEYBINDS ━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            "",
            "PLAYBACK:",
            "  Space     Toggle play/pause",
            "  n         Play next track",
            "  p         Play previous track",
            "  ↑/↓       Volume up/down (Shift for +/-5%)",
            "  ←/→       Seek backward/forward (not yet implemented)",
            "",
            "NAVIGATION:",
            "  j/k       Navigate lists (down/up)",
            "  /         Search for music",
            "  l         Load playlist from URL (YouTube/YouTube Music)",
            "  h         Go to Home (My Mix) view",
            "  Esc       Return to previous view",
            "",
            "QUEUE MANAGEMENT:",
            "  Enter     Add selected item to queue",
            "  t         Toggle queue expansion",
            "  d         Delete selected queue item (when queue expanded)",
            "",
            "MY MIX:",
            "  m         Toggle My Mix expansion",
            "  Shift+M   Refresh My Mix (when expanded)",
            "",
            "HISTORY:",
            "  Shift+H   Toggle history expansion",
            "  Shift+C   Clear history (when history expanded)",
            "",
            "OTHER:",
            "  ?         Show this help screen",
            "  q         Quit application",
            "",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            "",
            "Press '?', 'Esc', or 'q' to close this help screen",
        ].join("\n");

        let help_widget = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Left);

        frame.render_widget(help_widget, chunks[1]);
    }

    fn draw_login_screen(&self, frame: &mut Frame) {
        use ratatui::widgets::Paragraph;
        use ratatui::layout::{Alignment, Constraint, Direction, Layout};

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(10),
                Constraint::Percentage(40),
            ])
            .split(frame.size());

        let login_text = vec![
            "YouTube Music Player",
            "",
            "Welcome! To access YouTube Music, you'll select",
            "a YouTube account from your browser (Chrome/Firefox).",
            "",
            "Make sure you're logged into YouTube in your browser first.",
            "",
            "Press 'L' to select account",
            "Press 'Q' to quit",
            "",
            if !self.status_message.is_empty() {
                &self.status_message
            } else {
                ""
            },
        ].join("\n");

        let login_widget = Paragraph::new(login_text)
            .block(Block::default().borders(Borders::ALL).title("Login Required"))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);

        frame.render_widget(login_widget, chunks[1]);
    }
}
