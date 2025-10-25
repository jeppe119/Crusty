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

use crate::player::audio::{AudioPlayer, PlayerState};
use crate::player::queue::{Queue, Track};
use crate::youtube::extractor::{YouTubeExtractor, VideoInfo};
use crate::youtube::auth::YouTubeAuth;

enum AppMode {
    Normal,
    Searching,
    LoginPrompt,  // Show login screen
}

pub struct MusicPlayerApp {
    player: AudioPlayer,
    queue: Queue,
    extractor: YouTubeExtractor,
    auth: Option<YouTubeAuth>,
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
}

impl MusicPlayerApp {
    pub fn new() -> Self {
        let (search_tx, search_rx) = mpsc::unbounded_channel();

        // Try to initialize YouTube auth
        let auth = match YouTubeAuth::new() {
            Ok(auth) => {
                eprintln!("YouTube auth initialized");
                Some(auth)
            }
            Err(e) => {
                eprintln!("Failed to initialize YouTube auth: {}", e);
                None
            }
        };

        // Check if user needs to login
        let is_authenticated = auth.as_ref()
            .map(|a| a.is_authenticated())
            .unwrap_or(false);

        let initial_mode = if is_authenticated {
            AppMode::Normal
        } else {
            AppMode::LoginPrompt
        };

        MusicPlayerApp {
            player: AudioPlayer::new(),
            queue: Queue::new(),
            extractor: YouTubeExtractor::new(),
            auth,
            search_results: Vec::new(),
            selected_result: 0,
            selected_queue_item: 0,
            search_query: String::new(),
            mode: initial_mode,
            should_quit: false,
            is_searching: false,
            search_rx,
            search_tx,
            status_message: String::new(),
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
                    "Controls: [/]Search [Enter]Add to queue [n]Next [p]Prev [Space]Play/Pause [j/k]Navigate [↑/↓]Volume [q]Quit".to_string()
                },
                AppMode::LoginPrompt => "Login Required".to_string(),
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
            self.status_message = format!("Loading: {}...", track.title);

            // If the URL is a YouTube URL (not a direct stream), fetch the audio URL first
            if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
                // Get cookies path if auth is available
                let cookies_path = self.auth.as_ref().and_then(|auth| auth.get_cookies_path());

                // Fetch audio URL in a blocking task
                let youtube_url = track.url.clone();
                let fetch_result = tokio::task::spawn_blocking(move || {
                    Self::fetch_audio_url_blocking(&youtube_url, cookies_path)
                }).await;

                match fetch_result {
                    Ok(Ok(audio_url)) => {
                        // Now play the audio in a blocking task
                        let url_clone = audio_url.clone();
                        let title_clone = track.title.clone();

                        // Call play in a blocking context
                        self.player.play(&url_clone, &title_clone);
                        self.status_message = format!("Now playing: {}", track.title);
                    }
                    Ok(Err(e)) => {
                        self.status_message = format!("Error: {}", e);
                        eprintln!("Failed to get audio URL: {}", e);
                    }
                    Err(e) => {
                        self.status_message = format!("Task error: {}", e);
                        eprintln!("Task join error: {}", e);
                    }
                }
            } else {
                // Already have direct URL
                self.player.play(&track.url, &track.title);
                self.status_message = format!("Now playing: {}", track.title);
            }
        } else {
            self.status_message = "Queue is empty!".to_string();
        }
    }

    async fn play_previous(&mut self) {
        if let Some(track) = self.queue.previous() {
            self.status_message = format!("Loading: {}...", track.title);

            // Same logic as play_next
            if track.url.contains("youtube.com") || track.url.contains("youtu.be") {
                // Get cookies path if auth is available
                let cookies_path = self.auth.as_ref().and_then(|auth| auth.get_cookies_path());

                let youtube_url = track.url.clone();
                let fetch_result = tokio::task::spawn_blocking(move || {
                    Self::fetch_audio_url_blocking(&youtube_url, cookies_path)
                }).await;

                match fetch_result {
                    Ok(Ok(audio_url)) => {
                        self.player.play(&audio_url, &track.title);
                        self.status_message = format!("Now playing: {}", track.title);
                    }
                    Ok(Err(e)) => {
                        self.status_message = format!("Error: {}", e);
                        eprintln!("Failed to get audio URL: {}", e);
                    }
                    Err(e) => {
                        self.status_message = format!("Task error: {}", e);
                    }
                }
            } else {
                self.player.play(&track.url, &track.title);
                self.status_message = format!("Now playing: {}", track.title);
            }
        } else {
            self.status_message = "No previous track!".to_string();
        }
    }

    // Helper to fetch audio URL in blocking context
    fn fetch_audio_url_blocking(youtube_url: &str, cookies_path: Option<std::path::PathBuf>) -> Result<String, String> {
        use std::process::Command;

        eprintln!("Fetching audio URL for: {}", youtube_url);

        let mut cmd = Command::new("yt-dlp");
        cmd.arg("--get-url")
            .arg("-f")
            .arg("bestaudio/best");  // Fallback to best if bestaudio not available

        // Add cookies if available
        if let Some(cookies) = cookies_path {
            eprintln!("Using cookies from: {:?}", cookies);
            cmd.arg("--cookies").arg(cookies);
        }

        cmd.arg("--no-check-certificate")  // Skip certificate validation
            .arg(youtube_url);

        let output = cmd.output()
            .map_err(|e| format!("Failed to run yt-dlp: {}. Is yt-dlp installed?", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            eprintln!("yt-dlp error: {}", error);
            return Err(format!("yt-dlp failed: {}", error));
        }

        let url = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 from yt-dlp: {}", e))?
            .trim()
            .to_string();

        if url.is_empty() {
            return Err("yt-dlp returned empty URL".to_string());
        }

        eprintln!("Got audio URL (first 100 chars): {}", &url[..url.len().min(100)]);

        Ok(url)
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
            self.queue.add(track);

            // Show feedback
            self.status_message = format!("Added '{}' to queue! ({} total)", video.title, self.queue.get_queue_list().len());

            // Don't auto-play here - let user press 'n' to start
            // Auto-play can crash if yt-dlp takes too long
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
        self.status_message = "Starting login...".to_string();

        let auth = match &self.auth {
            Some(a) => a,
            None => {
                self.status_message = "Auth not initialized!".to_string();
                return;
            }
        };

        // Start OAuth flow
        match auth.start_oauth_flow() {
            Ok((auth_url, _csrf_token, pkce_verifier)) => {
                self.status_message = "Opening browser... Complete login there".to_string();

                // Open browser
                if let Err(e) = open::that(&auth_url) {
                    self.status_message = format!("Failed to open browser: {}. Visit: {}", e, auth_url);
                    eprintln!("Manual login URL: {}", auth_url);
                    return;
                }

                // TODO: Start callback server and wait for authorization code
                // For now, show a message
                self.status_message = "Login flow started! Check your browser...".to_string();

                // We'll implement the full callback server next
                // For now, just show that we got here
                eprintln!("OAuth URL opened. Callback server TODO");
            }
            Err(e) => {
                self.status_message = format!("Login failed: {}", e);
            }
        }
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
            "Welcome! You need to login with your Google account",
            "to access YouTube Music and play songs.",
            "",
            "Press 'L' to login with Google",
            "Press 'Q' to quit",
            "",
            if !self.status_message.is_empty() {
                &self.status_message
            } else {
                "Waiting for login..."
            },
        ].join("\n");

        let login_widget = Paragraph::new(login_text)
            .block(Block::default().borders(Borders::ALL).title("Login Required"))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);

        frame.render_widget(login_widget, chunks[1]);
    }
}
