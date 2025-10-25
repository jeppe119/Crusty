# YouTube Terminal Music Player (Rust)

A terminal-based music player with TUI that streams from YouTube, built in Rust.

**This is a Rust rewrite of a Python project** - designed as a learning project for Rust beginners!

---

## 🎯 Project Goals

- Learn Rust fundamentals (ownership, borrowing, async/await)
- Build a fully functional TUI music player
- Stream and play audio from YouTube
- Implement queue management and playback controls

---

## 📊 Python → Rust Translation

| Component | Python | Rust |
|-----------|--------|------|
| **TUI Framework** | `textual` | `ratatui` + `crossterm` |
| **Async Runtime** | `asyncio` | `tokio` |
| **YouTube Data** | `yt-dlp` (library) | `yt-dlp` (subprocess) or `rustube` |
| **Audio Playback** | `python-mpv` | `rodio` (pure Rust audio) |
| **HTTP Client** | `requests` | `reqwest` |
| **JSON** | stdlib | `serde` + `serde_json` |
| **Error Handling** | exceptions | `Result<T, E>` + `anyhow`/`thiserror` |

---

## 🚀 Features

- ✅ Search YouTube for songs/videos
- ✅ Stream audio directly from YouTube
- ✅ Play/Pause/Skip controls
- ✅ Volume control
- ✅ Queue management with history
- ✅ Progress bar with time display
- ✅ Keyboard shortcuts
- ✅ Beautiful terminal UI

---

## 🏗️ Project Structure

```
youtube-music-player-rust/
├── Cargo.toml              # Rust dependencies and project config
├── README.md               # This file!
└── src/
    ├── main.rs             # Entry point - starts the app
    │
    ├── player/             # Audio playback & queue management
    │   ├── mod.rs          # Module declaration
    │   ├── audio.rs        # Audio playback engine (rodio)
    │   └── queue.rs        # Queue management (VecDeque)
    │
    ├── youtube/            # YouTube integration
    │   ├── mod.rs          # Module declaration
    │   └── extractor.rs    # Search & extract audio streams
    │
    └── ui/                 # Terminal user interface
        ├── mod.rs          # Module declaration
        └── app.rs          # Main TUI application (ratatui)
```

---

## 📁 File Descriptions

### `src/main.rs`
- **Purpose**: Application entry point
- **What it does**:
  - Sets up the async runtime with `tokio`
  - Initializes and runs the `MusicPlayerApp`
- **Key concepts**:
  - `#[tokio::main]` macro for async main
  - Module declarations (`mod player`, `mod youtube`, `mod ui`)

### `src/player/audio.rs`
- **Purpose**: Audio playback engine
- **What it does**:
  - Plays audio streams from URLs
  - Controls playback (play/pause/stop/seek)
  - Manages volume
  - Tracks playback position and duration
- **Key Rust concepts**:
  - `enum PlayerState` for state management
  - `Arc<Mutex<>>` for thread-safe shared state
  - Using the `rodio` crate for pure Rust audio playback

### `src/player/queue.rs`
- **Purpose**: Queue and playback order management
- **What it does**:
  - Maintains a queue of tracks to play
  - Supports next/previous navigation
  - Tracks playback history
  - Allows adding/removing tracks
- **Key Rust concepts**:
  - `struct Track` for track metadata
  - `VecDeque<Track>` for efficient queue operations
  - `Option<T>` for nullable values
  - Methods returning `Option<Track>` for safe access

### `src/youtube/extractor.rs`
- **Purpose**: YouTube search and audio extraction
- **What it does**:
  - Searches YouTube using yt-dlp
  - Extracts audio stream URLs
  - Fetches video metadata (title, duration, uploader)
- **Key Rust concepts**:
  - `async fn` for asynchronous operations
  - `Result<T, E>` for error handling
  - `serde` for JSON serialization/deserialization
  - Using `std::process::Command` to call yt-dlp

### `src/ui/app.rs`
- **Purpose**: Main TUI application
- **What it does**:
  - Renders the terminal interface
  - Handles keyboard input
  - Updates display based on player state
  - Manages search results and queue display
- **Key Rust concepts**:
  - `ratatui` widgets (Table, List, Paragraph, etc.)
  - Event loop with `crossterm`
  - Pattern matching on keyboard events
  - Lifetime management for UI rendering

---

## 🔧 Dependencies Explained

### Core Dependencies

**`tokio`** - Async runtime
- Allows async/await in Rust
- Handles concurrent operations (UI updates, audio playback, network requests)

**`ratatui` + `crossterm`** - TUI framework
- `ratatui`: High-level TUI library (widgets, layouts, rendering)
- `crossterm`: Low-level terminal control (raw mode, events, colors)

**`rodio`** - Audio playback
- Pure Rust audio library
- Supports various audio formats
- Provides `Sink` for playback control

**`serde` + `serde_json`** - Serialization
- Parse JSON responses from yt-dlp
- Serialize/deserialize video metadata

**`reqwest`** - HTTP client
- Make async HTTP requests
- Fetch video data and streams

**`anyhow` + `thiserror`** - Error handling
- `anyhow`: Ergonomic error handling for applications
- `thiserror`: Derive macros for custom error types

---

## 🎓 Rust Concepts You'll Learn

### 1. **Ownership & Borrowing**
```rust
// Owner transfers ownership
let queue = Queue::new();
let track = queue.next(); // queue owns tracks

// Borrowing (references)
fn display_track(track: &Track) { ... } // borrows, doesn't take ownership
```

### 2. **Error Handling**
```rust
// Result type for operations that can fail
pub async fn search(&self, query: &str) -> Result<Vec<VideoInfo>, Box<dyn Error>> {
    // Returns Ok(data) or Err(error)
}
```

### 3. **Pattern Matching**
```rust
match key_code {
    KeyCode::Char(' ') => self.toggle_pause(),
    KeyCode::Char('n') => self.play_next(),
    KeyCode::Char('q') => self.quit(),
    _ => {}
}
```

### 4. **Async/Await**
```rust
async fn perform_search(&mut self, query: &str) {
    let results = self.extractor.search(query, 15).await?;
    // Process results...
}
```

### 5. **Structs & Enums**
```rust
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

pub struct Track {
    pub title: String,
    pub duration: u64,
    // ...
}
```

---

## 📦 Installation & Prerequisites

### System Dependencies
```bash
# yt-dlp for YouTube data extraction
yay -S yt-dlp

# Optional: mpv (if we use libmpv instead of rodio)
# yay -S mpv
```

### Build & Run
```bash
# Build the project
cargo build

# Run in development mode
cargo run

# Build optimized release binary
cargo build --release

# Run release binary
./target/release/youtube-music-player-rust
```

---

## ⌨️ Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Play/Pause |
| `n` | Next track |
| `p` | Previous track |
| `↑` | Volume up |
| `↓` | Volume down |
| `→` | Seek forward 10s |
| `←` | Seek backward 10s |
| `q` | Quit |
| `Enter` | Add selected result to queue / Play selected queue item |

---

## 🗺️ Development Roadmap

### Phase 1: Foundation ✅
- [x] Project structure setup
- [x] Module skeletons with imports
- [x] Implement `Track` and `Queue` data structures
- [x] Basic audio playback with `rodio`

### Phase 2: YouTube Integration ✅
- [x] YouTube search via yt-dlp subprocess
- [x] Extract audio stream URLs
- [x] Parse video metadata

### Phase 3: TUI Interface ✅
- [x] Basic layout (search, results, queue, player info)
- [x] Keyboard input handling
- [x] Display updates

### Phase 4: Integration ✅
- [x] Connect all components
- [x] Implement playback controls
- [x] Queue management
- [x] Error handling

### Phase 5: Polish ✅
- [x] UI improvements
- [x] Better error messages
- [x] Playback progress tracking
- [x] Auto-advance to next track
- [x] Status messages for user feedback

---

## 🎉 Recent Improvements

### Fixed Issues:
- ✅ **Fixed crash when playing tracks** - Improved audio download and decode error handling
  - Added proper HTTP client with timeout and user-agent headers
  - Added validation for downloaded audio data
  - Better error messages when audio fails to decode

### New Features:
- ✅ **Auto-advance to next track** - Automatically plays next song when current finishes
- ✅ **Playback position tracking** - Shows current time and duration
- ✅ **Better status messages** - Real-time feedback for all user actions
- ✅ **Improved UI** - Shows queue size, playback state with icons, and progress
- ✅ **Pause tracking** - Accurately tracks time even when paused

### Technical Improvements:
- Better error handling in audio player with detailed error messages
- HTTP timeout protection (30 seconds)
- Audio data validation before decoding
- Time tracking with pause duration calculation
- Automatic track advancement when queue has items

---

## 🐛 Debugging Tips

### Check if dependencies are installed:
```bash
cargo check
```

### Run with debug output:
```bash
RUST_LOG=debug cargo run
```

### Fix common issues:
```bash
# If yt-dlp not found
which yt-dlp

# Update Rust
rustup update

# Clean build artifacts
cargo clean
```

---

## 📚 Learning Resources

- [The Rust Book](https://doc.rust-lang.org/book/) - Official Rust tutorial
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) - Learn by doing
- [Ratatui Documentation](https://docs.rs/ratatui/) - TUI framework docs
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) - Async Rust
- [Rodio Examples](https://github.com/RustAudio/rodio) - Audio playback

---

## 🤝 Contributing

This is a learning project! Feel free to:
- Try implementing the `TODO` sections
- Improve error handling
- Add new features
- Optimize performance
- Fix bugs

---

## 📝 License

MIT License - Feel free to use this code for learning!

---

## 🎉 Credits

Original Python version: 690 lines
Rust version: ~1200-1500 lines (estimated, more verbose but safer!)

Built with ☕ as a Rust learning project
