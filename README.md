<div align="center">
  <img src="Crusty.png" alt="Crusty" width="250" />
  <p><i>Because scrolling through YouTube on your phone is too mainstream ¯\_(ツ)_/¯</i></p>
</div>

---

## What the hell is this?

A terminal-based YouTube music player that nobody asked for, but you're getting anyway. Built in Rust because I wanted to learn it and terminals are cooler than GUIs.

**TL;DR:** It's like Spotify, but worse! And in your terminal! And it uses YouTube! 🎉

---

## Why does this exist?

- ✅ I wanted to learn Rust (didn't really learn it, but it compiles!)
- ✅ I had too much free time
- ✅ Claude helped me... or maybe helped me fuck it up (hard to tell)
- ✅ Spite is a powerful motivator
- ❌ Nobody asked for this

---

## Features (that actually work)

- ✅ Search YouTube for songs/videos
- ✅ Playlist support (YouTube & YouTube Music playlists)
- ✅ Smart caching (downloads to temp, auto-deletes after 1 hour)
- ✅ Music-only filter (auto-filters tracks >5min to keep it lightweight)
- ✅ Background pre-downloading (buffer next tracks while you listen)
- ✅ Seeking support (←/→ arrow keys skip 10 seconds)
- ✅ Play/Pause/Skip controls (groundbreaking, I know)
- ✅ Volume control (revolutionary)
- ✅ Queue management with history
- ✅ Progress bar that actually shows progress
- ✅ Keyboard shortcuts (because mouse is for normies)
- ✅ Beautiful terminal UI (beauty is subjective)
- ✅ Auto-advance to next track (so you don't have to)
- ✅ Download progress indicator (see what's buffering)

---

## Tech Stack (for the nerds)

| Component | Library |
|-----------|---------|
| **TUI** | `ratatui` + `crossterm` |
| **Async** | `tokio` |
| **YouTube** | `yt-dlp` (subprocess) |
| **Audio** | `rodio` (pure Rust) |
| **HTTP** | `reqwest` |
| **JSON** | `serde` + `serde_json` |

---

## Installation (good luck)

### Dependencies you need:

```bash
# yt-dlp (the magic sauce)
yay -S yt-dlp

# Rust (if you somehow don't have it)
# Arch/Manjaro:
sudo pacman -S rustup
rustup default stable

# Or the official way (other distros):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build this bad boy:

```bash
# Clone the repo
git clone <your-repo-url>
cd Crusty

# Build it (grab a coffee, this takes a while)
cargo build --release

# Run it
./target/release/youtube-music-player-rust

# Or just run it in dev mode (slower but good for debugging)
cargo run
```

---

## How to use it

### Keyboard shortcuts (because GUI is overrated):

| Key | What it does |
|-----|-------------|
| `Space` | Play/Pause (like a normal media player) |
| `n` | Next track (skip that garbage song) |
| `p` | Previous track (oh wait, that song was good) |
| `j/k` | Navigate up/down in lists |
| `↑` / `Shift+↑` | Volume up (+1 or +5) |
| `↓` / `Shift+↓` | Volume down (-1 or -5) |
| `→` | Seek forward 10s (skip the boring intro) |
| `←` | Seek backward 10s (wait, what did they say?) |
| `/` | Search YouTube |
| `l` | Load playlist URL |
| `t` | Toggle queue view |
| `Enter` | Add to queue / Play selected track |
| `q` | Quit (escape the terminal) |

---

## Project Structure (if you're curious)

```
Crusty/
├── Cargo.toml              # Rust dependencies (a.k.a. the shopping list)
├── README.md               # You are here!
├── Crusty.png              # Our adorable mascot
├── Crusty2.png             # Logo because branding matters
└── src/
    ├── main.rs             # Where the magic starts
    │
    ├── player/             # Audio stuff
    │   ├── mod.rs
    │   ├── audio.rs        # Makes noise come out of speakers
    │   └── queue.rs        # Manages what plays next
    │
    ├── youtube/            # YouTube shenanigans
    │   ├── mod.rs
    │   └── extractor.rs    # Talks to yt-dlp
    │
    └── ui/                 # Pretty terminal things
        ├── mod.rs
        └── app.rs          # The TUI magic
```

---

## What you'll learn (if you don't rage quit first)

### 1. **Ownership & Borrowing** (a.k.a. the borrow checker's reign of terror)
```rust
// The compiler WILL yell at you
let queue = Queue::new();
let track = queue.next(); // queue owns this now

// Borrowing (politely asking to look at something)
fn display_track(track: &Track) { ... } // just looking, not taking
```

### 2. **Error Handling** (because shit happens)
```rust
// Everything that can fail returns Result
pub async fn search(&self, query: &str) -> Result<Vec<VideoInfo>, Box<dyn Error>> {
    // Either Ok(yay) or Err(fuck)
}
```

### 3. **Pattern Matching** (switch statements on steroids)
```rust
match key_code {
    KeyCode::Char(' ') => self.toggle_pause(),
    KeyCode::Char('n') => self.play_next(),
    KeyCode::Char('q') => self.quit(),
    _ => {} // shrug emoji in code form
}
```

### 4. **Async/Await** (concurrent stuff without the headache)
```rust
async fn perform_search(&mut self, query: &str) {
    let results = self.extractor.search(query, 15).await?;
    // Do things while other things happen!
}
```

---

## Known Issues (a.k.a. "features")

- Sometimes YouTube changes their API and everything breaks :shipit:
- Audio might not work on some weird audio formats (not my problem)
- UI might look wonky on tiny terminal windows (get a bigger monitor)
- No playlist support yet (PRs welcome!)
- Error messages could be more helpful (working on it)

---

## Recent Improvements

### Bug Fixes:
- ✅ Fixed rapid queue clearing bug (state check prevents false positives)
- ✅ Fixed SPACE spam creating duplicate downloads (deduplication tracker)
- ✅ Seeking now works (←/→ arrows skip 10 seconds)
- ✅ Download priority fixed (current + next tracks download first)

### New Features:
- ✅ Music-only filter (auto-filters tracks >5min to avoid podcasts)
- ✅ Playlist loading indicator (no more UI freeze)
- ✅ Rolling download buffer (builds cache as you play)
- ✅ Smart download management (max 30 concurrent, proper prioritization)

---

## Contributing

This is a learning project, so feel free to:
- Submit PRs (I'll probably merge them)
- Open issues (I'll probably fix them... eventually)
- Fork it and make it your own
- Judge my code (constructively, please)
- Suggest features (no promises though)

Just remember: I built this to learn Rust, not to build the next Spotify. Expectations should be calibrated accordingly ¯\\\_(ツ)_/¯

---

## Learning Resources (for fellow Rust noobs)

- [The Rust Book](https://doc.rust-lang.org/book/) - Your new bible
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) - Learn by suffering
- [Ratatui Docs](https://docs.rs/ratatui/) - TUI wizardry
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) - Async black magic
- [Stack Overflow](https://stackoverflow.com/) - For when nothing works

---

## Stats (because numbers are fun)

- **Lines of code:** ~1500+ (mostly comments explaining Rust stuff)
- **Times I wanted to give up:** Lost count
- **Times it actually worked first try:** 0
- **Bugs fixed with Claude's help:** Too many

---

## License

MIT License - Do whatever you want with this. Make it better. Make it worse. Make it yours.

---

## Credits

Built with ☕ 🎵 and claude my guy

Special thanks to:
- The Rust community for being helpful
- yt-dlp for doing the heavy lifting
- My patience (RIP)
- You, for actually reading this far

---

<div align="center">
  <p><i>"It ain't much, but it's honest work"</i> - Some Farmer, probably</p>
  <p>Made by <a href="https://github.com/jeppe119">jeppe119</a> | ¯\_(ツ)_/¯</p>
</div>
