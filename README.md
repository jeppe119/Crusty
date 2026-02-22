<div align="center">
  <img src="Crusty.png" alt="Crusty" width="250" />
  <p><i>A terminal-based YouTube music player written in Rust</i></p>
</div>

---

## About

A terminal YouTube music player built in Rust as a learning project. It uses `yt-dlp` for extraction and `rodio` for playback, all wrapped in a `ratatui` TUI.

---

## Screenshots

<div align="center">
  <img src="screenshots_login.png" alt="Login prompt" width="700" />
  <p><i>Browser account selection</i></p>
</div>

<div align="center">
  <img src="screenshots_home.png" alt="Home screen" width="700" />
  <p><i>Home screen after login</i></p>
</div>

<div align="center">
  <img src="screenshots_playlist.png" alt="Playlist loaded" width="700" />
  <p><i>Playlist loaded with tracks queued</i></p>
</div>

<div align="center">
  <img src="screenshots_playing.png" alt="Music playing" width="700" />
  <p><i>Music playing with history</i></p>
</div>

---

## Features

- Search YouTube for songs and videos
- Playlist support (YouTube & YouTube Music)
- Smart caching (downloads to temp, auto-deletes after 1 hour)
- Music-only filter (auto-filters tracks >5min)
- Background pre-downloading of next tracks
- Seeking support (arrow keys skip 10 seconds)
- Play/Pause/Skip/Volume controls
- Queue management with history
- Progress bar and download indicator
- Keyboard-driven interface

---

## Tech Stack

| Component | Library |
|-----------|---------|
| **TUI** | `ratatui` + `crossterm` |
| **Async** | `tokio` |
| **YouTube** | `yt-dlp` (subprocess) |
| **Audio** | `rodio` (pure Rust) |
| **HTTP** | `reqwest` |
| **JSON** | `serde` + `serde_json` |

---

## Installation

### Dependencies

```bash
# yt-dlp
yay -S yt-dlp

# Rust (Arch/Manjaro)
sudo pacman -S rustup
rustup default stable

# Or the official way (other distros):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build

```bash
git clone https://github.com/jeppe119/Crusty.git
cd Crusty

cargo build --release
./target/release/crusty

# Or run in dev mode
cargo run
```

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Play/Pause |
| `n` | Next track |
| `p` | Previous track |
| `j/k` | Navigate up/down in lists |
| `Up` / `Shift+Up` | Volume up (+1 or +5) |
| `Down` / `Shift+Down` | Volume down (-1 or -5) |
| `Right` | Seek forward 10s |
| `Left` | Seek backward 10s |
| `/` | Search YouTube |
| `l` | Load playlist URL |
| `t` | Toggle queue view |
| `Enter` | Add to queue / Play selected |
| `q` | Quit |

---

## Project Structure

```
Crusty/
├── Cargo.toml
├── README.md
├── Crusty.png
├── Crusty2.png
└── src/
    ├── main.rs
    │
    ├── player/
    │   ├── mod.rs
    │   ├── audio.rs        # Audio playback
    │   └── queue.rs        # Queue management
    │
    ├── youtube/
    │   ├── mod.rs
    │   └── extractor.rs    # yt-dlp interface
    │
    └── ui/
        ├── mod.rs
        └── app.rs          # TUI rendering
```

---

## Rust Concepts Used

### Ownership & Borrowing
```rust
let queue = Queue::new();
let track = queue.next();

fn display_track(track: &Track) { ... }
```

### Error Handling
```rust
pub async fn search(&self, query: &str) -> Result<Vec<VideoInfo>, Box<dyn Error>> {
    // ...
}
```

### Pattern Matching
```rust
match key_code {
    KeyCode::Char(' ') => self.toggle_pause(),
    KeyCode::Char('n') => self.play_next(),
    KeyCode::Char('q') => self.quit(),
    _ => {}
}
```

### Async/Await
```rust
async fn perform_search(&mut self, query: &str) {
    let results = self.extractor.search(query, 15).await?;
}
```

---

## Known Issues

- YouTube API changes can break extraction
- Some audio formats may not be supported
- UI may not render well in very small terminals

---

## Recent Changes

### Bug Fixes
- Fixed rapid queue clearing bug (state check prevents false positives)
- Fixed duplicate downloads from repeated input
- Seeking now works with arrow keys
- Download priority fixed (current + next tracks download first)

### New Features
- Music-only filter (auto-filters tracks >5min)
- Playlist loading indicator
- Rolling download buffer
- Smart download management (max 30 concurrent, proper prioritization)

---

## Contributing

This is a learning project. PRs, issues, and forks are all welcome.

---

## Resources

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Ratatui Docs](https://docs.rs/ratatui/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)

---

## License

MIT

---

<div align="center">
  <p>Made by <a href="https://github.com/jeppe119">jeppe119</a></p>
</div>
