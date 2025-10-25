// Main TUI application using ratatui
// Handles the terminal interface, user input, and display

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
    style::{Color, Modifier, Style},
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
    mode: AppMode,
    should_quit: bool,
    is_searching: bool,
    search_rx: mpsc::UnboundedReceiver<Vec<VideoInfo>>,
    search_tx: mpsc::UnboundedSender<Vec<VideoInfo>>,
    status_message: String,
    // Track pre-downloaded files by video_id
    downloaded_files: Arc<Mutex<HashMap<String, String>>>,
}

impl MusicPlayerApp {
    pub fn new() -> Self {
        let (search_tx, search_rx) = mpsc::unbounded_channel();

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

        MusicPlayerApp {
            player: AudioPlayer::new(),
            queue: Queue::new(),
            extractor: YouTubeExtractor::new(),
            browser_auth,
            available_accounts: Vec::new(),
            selected_account_idx: 0,
            search_results: Vec::new(),
            selected_result: 0,
            selected_queue_item: 0,
            search_query: String::new(),
            mode: initial_mode,
            should_quit: false,
            is_searching: false,
            search_rx,
            search_tx,
            status_message,
            downloaded_files: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            terminal.draw(|f| self.draw_ui(f))?;

            // Check for search results
            if let Ok(results) = self.search_rx.try_recv() {
                self.search_results = results;
                self.selected_result = 0;
                self.is_searching = false;
                self.status_message = format!("Found {} results", self.search_results.len());
            }

            // Auto-advance to next track when current finishes
            if self.player.is_finished() && self.player.get_state() == PlayerState::Playing {
                if !self.queue.is_empty() {
                    self.status_message = "Track finished, playing next...".to_string();
                    self.play_next().await;
                } else {
                    self.player.stop();
                    self.status_message = "Playback finished - queue is empty".to_string();
                }
            }

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_input(key.code).await;
                }
            }

            if self.should_quit {
                break;
            }
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

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(5),
            ])
            .split(frame.size());

        // Header
        let title = if self.is_searching {
            "Searching... please wait".to_string()
        } else if !self.status_message.is_empty() {
            self.status_message.clone()
        } else {
            match self.mode {
                AppMode::Searching => format!("Search: {}_", self.search_query),
                AppMode::Normal => {
                    let account_info = if let Some(account) = self.browser_auth.load_selected_account() {
                        format!(" | Account: {}", account.display_name)
                    } else {
                        String::new()
                    };
                    format!("Controls: [/]Search [Enter]Add [n]Next [p]Prev [Space]Play/Pause [j/k]Navigate [↑/↓]Volume [q]Quit{}", account_info)
                },
                AppMode::LoginPrompt => "Login Required".to_string(),
                AppMode::AccountPicker => "Select YouTube Account".to_string(),
            }
        };
        let header = Paragraph::new(title)
            .block(Block::default().borders(Borders::ALL).title("YouTube Music Player"));
        frame.render_widget(header, chunks[0]);

        // Main area - split between search results and queue
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // Search results
        let results: Vec<ListItem> = self
            .search_results
            .iter()
            .enumerate()
            .map(|(i, video)| {
                let duration = Self::format_time(video.duration as f64);
                let content = format!("{} - {} [{}]", video.title, video.uploader, duration);
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
        frame.render_widget(results_list, main_chunks[0]);

        // Queue
        let queue_items: Vec<ListItem> = self
            .queue
            .get_queue_list()
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let duration = Self::format_time(track.duration as f64);
                let content = format!("{} - {} [{}]", track.title, track.uploader, duration);
                let style = if i == self.selected_queue_item {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let queue_list = List::new(queue_items)
            .block(Block::default().borders(Borders::ALL).title("Queue"));
        frame.render_widget(queue_list, main_chunks[1]);

        // Player info
        let current_track = self.queue.get_current();
        let now_playing = if let Some(track) = current_track {
            format!("Now Playing: {} - {}", track.title, track.uploader)
        } else {
            "No track playing".to_string()
        };

        let state_str = match self.player.get_state() {
            PlayerState::Playing => "▶ Playing",
            PlayerState::Paused => "⏸ Paused",
            PlayerState::Stopped => "⏹ Stopped",
        };

        let volume = self.player.get_volume();

        // Show playback progress
        let time_pos = self.player.get_time_pos();
        let duration = self.player.get_duration();
        let time_str = if duration > 0.0 {
            format!("{} / {}", Self::format_time(time_pos), Self::format_time(duration))
        } else {
            Self::format_time(time_pos)
        };

        let player_info = format!(
            "{}\nState: {} | Volume: {}% | Time: {}\nQueue: {} tracks remaining",
            now_playing,
            state_str,
            volume,
            time_str,
            self.queue.size()
        );

        let player_widget = Paragraph::new(player_info)
            .block(Block::default().borders(Borders::ALL).title("Player"));
        frame.render_widget(player_widget, chunks[2]);
    }

    async fn handle_input(&mut self, key: KeyCode) {
        // Clear status message on any key press (except when searching)
        if !matches!(self.mode, AppMode::Searching) {
            self.status_message.clear();
        }

        match self.mode {
            AppMode::LoginPrompt => {
                match key {
                    KeyCode::Char('q') | KeyCode::Char('Q') => self.should_quit = true,
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        self.start_login().await;
                    }
                    _ => {}
                }
            }
            AppMode::AccountPicker => {
                match key {
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
                match key {
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
                        self.search_query.clear();
                    }
                    KeyCode::Esc => {
                        self.mode = AppMode::Normal;
                        self.search_query.clear();
                    }
                    _ => {}
                }
            }
            AppMode::Normal => {
                match key {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('/') => self.mode = AppMode::Searching,
                    KeyCode::Char(' ') => self.toggle_pause(),
                    KeyCode::Char('n') => {
                        self.status_message = "Playing next track...".to_string();
                        self.play_next().await;
                    }
                    KeyCode::Char('p') => {
                        self.status_message = "Playing previous track...".to_string();
                        self.play_previous().await;
                    }
                    KeyCode::Up => self.volume_up(),
                    KeyCode::Down => self.volume_down(),
                    KeyCode::Right => self.seek_forward(),
                    KeyCode::Left => self.seek_backward(),
                    KeyCode::Char('j') => self.next_search_result(),
                    KeyCode::Char('k') => self.prev_search_result(),
                    KeyCode::Enter => self.add_selected_to_queue(),
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
        if let Some(track) = self.queue.next() {
            // Check if already downloaded
            let pre_downloaded = self.downloaded_files.lock().ok()
                .and_then(|files| files.get(&track.video_id).cloned());

            if let Some(local_file) = pre_downloaded {
                // Already downloaded! Play immediately (instant skip!)
                self.player.play(&local_file, &track.title);
                self.status_message = format!("▶ Now playing: {}", track.title);
            } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
                // Need to download it now
                self.status_message = format!("Downloading: {}...", track.title);

                let cookie_config = self.browser_auth.load_selected_account()
                    .map(|account| self.browser_auth.get_cookie_arg(&account));

                let youtube_url = track.url.clone();
                let fetch_result = tokio::task::spawn_blocking(move || {
                    Self::fetch_audio_url_blocking(&youtube_url, cookie_config)
                }).await;

                match fetch_result {
                    Ok(Ok(temp_file_path)) => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        self.player.play(&temp_file_path, &track.title);
                        self.status_message = format!("Now playing: {}", track.title);

                        // Clean up later
                        let temp_path = temp_file_path.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                            let _ = std::fs::remove_file(&temp_path);
                        });
                    }
                    Ok(Err(e)) => {
                        self.status_message = format!("Error: {}", e);
                    }
                    Err(e) => {
                        self.status_message = format!("Task error: {}", e);
                    }
                }
            } else {
                // Direct URL
                self.player.play(&track.url, &track.title);
                self.status_message = format!("Now playing: {}", track.title);
            }
        } else {
            self.status_message = "Queue is empty!".to_string();
        }
    }

    async fn play_previous(&mut self) {
        if let Some(track) = self.queue.previous() {
            // Check if already downloaded
            let pre_downloaded = self.downloaded_files.lock().ok()
                .and_then(|files| files.get(&track.video_id).cloned());

            if let Some(local_file) = pre_downloaded {
                // Already downloaded! Play immediately (instant skip!)
                self.player.play(&local_file, &track.title);
                self.status_message = format!("◀ Now playing: {}", track.title);
            } else if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
                // Need to download it now
                self.status_message = format!("Downloading: {}...", track.title);

                let cookie_config = self.browser_auth.load_selected_account()
                    .map(|account| self.browser_auth.get_cookie_arg(&account));

                let youtube_url = track.url.clone();
                let fetch_result = tokio::task::spawn_blocking(move || {
                    Self::fetch_audio_url_blocking(&youtube_url, cookie_config)
                }).await;

                match fetch_result {
                    Ok(Ok(temp_file_path)) => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        self.player.play(&temp_file_path, &track.title);
                        self.status_message = format!("◀ Now playing: {}", track.title);

                        // Clean up later
                        let temp_path = temp_file_path.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                            let _ = std::fs::remove_file(&temp_path);
                        });
                    }
                    Ok(Err(e)) => {
                        self.status_message = format!("Error: {}", e);
                    }
                    Err(e) => {
                        self.status_message = format!("Task error: {}", e);
                    }
                }
            } else {
                // Direct URL
                self.player.play(&track.url, &track.title);
                self.status_message = format!("◀ Now playing: {}", track.title);
            }
        } else {
            self.status_message = "No previous track!".to_string();
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

        cmd.arg("--no-check-certificate")  // Skip certificate validation
            .arg(youtube_url);

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

    fn toggle_pause(&mut self) {
        self.player.toggle_pause();
    }

    fn volume_up(&mut self) {
        let current = self.player.get_volume();
        if current < 100 {
            self.player.set_volume((current + 5).min(100));
        }
    }

    fn volume_down(&mut self) {
        let current = self.player.get_volume();
        if current > 0 {
            self.player.set_volume(current.saturating_sub(5));
        }
    }

    fn seek_forward(&mut self) {
        // TODO: Implement when player supports seeking
        // self.player.seek_relative(10.0);
    }

    fn seek_backward(&mut self) {
        // TODO: Implement when player supports seeking
        // self.player.seek_relative(-10.0);
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

    fn add_selected_to_queue(&mut self) {
        if let Some(video) = self.search_results.get(self.selected_result) {
            let track = Track::new(
                video.id.clone(),
                video.title.clone(),
                video.duration,
                video.uploader.clone(),
                video.url.clone(),
            );

            let was_empty = self.queue.get_queue_list().is_empty();

            // Start background download for this track
            let video_id = track.video_id.clone();
            let youtube_url = track.url.clone();
            let cookie_config = self.browser_auth.load_selected_account()
                .map(|account| self.browser_auth.get_cookie_arg(&account));
            let downloaded_files = self.downloaded_files.clone();

            // Spawn background download
            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    Self::fetch_audio_url_blocking(&youtube_url, cookie_config)
                }).await;

                if let Ok(Ok(file_path)) = result {
                    // Store the downloaded file path
                    if let Ok(mut files) = downloaded_files.lock() {
                        files.insert(video_id, file_path);
                    }
                }
            });

            self.queue.add(track);

            // Show feedback
            self.status_message = format!("Added '{}' to queue! Downloading in background... ({} total)", video.title, self.queue.get_queue_list().len());

            if was_empty {
                self.status_message = format!("Added '{}' to queue! Press 'n' to play", video.title);
            }
        }
    }

    fn format_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as u64;
        let secs = (seconds % 60.0) as u64;
        format!("{:02}:{:02}", mins, secs)
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
