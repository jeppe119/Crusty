# Crusty Architecture

Crusty is a Cargo **workspace**. Framework-agnostic logic lives in `crusty-core`;
the terminal UI lives in `crusty-tui`. A future desktop GUI (`crusty-gui`, Tauri)
and MPRIS/tray integration are designed to consume the same core.

```
crusty/                          (workspace root)
├── crates/
│   ├── crusty-core/             framework-agnostic — NO ratatui/crossterm/tauri
│   │   └── src/
│   │       ├── config.rs        APP_NAME, paths, URL/time helpers, consts
│   │       ├── model.rs         Track, PlayerState, PersistedQueue
│   │       ├── queue.rs         Queue (playback queue + history)
│   │       ├── download.rs      DownloadManager (yt-dlp, rate-limited, cached)
│   │       ├── persistence.rs   PersistenceService, PlaybackState, write_atomic
│   │       └── engine/          AudioEngine (thread-backed, Send+Sync handle)
│   └── crusty-tui/              ratatui/crossterm UI → consumes crusty-core
│       └── src/ui, youtube, services/{feed,playlist,cache_store}, main.rs
└── (crates/crusty-gui/          FUTURE: Tauri backend → consumes crusty-core)
```

## The audio engine contract (`crusty_core::engine`)

rodio's `Player`/device sink are **`!Send`** (thread-affine). To share playback
control across a TUI loop, Tauri commands, and an MPRIS handler without wrapping
everything in `Arc<Mutex>`, the engine runs on a **dedicated `std::thread`** that
exclusively owns the `!Send` rodio objects. The public `AudioEngine` handle is
**`Send + Sync`** (enforced by a compile-time assertion) and communicates only
over channels:

| Direction | Mechanism | Type |
|-----------|-----------|------|
| Commands **in** | `crossbeam-channel` | `AudioCommand` |
| State **out** (polled/awaited) | `tokio::sync::watch` | `PlayerSnapshot` |
| One-shot events **out** (multi-consumer) | `tokio::sync::broadcast` | `AudioEvent` |

```rust
let engine = AudioEngine::new();          // spawns the engine thread (opens device on it)

engine.play(path, title, duration_secs);  // fire-and-forget; safe from async contexts
engine.toggle_pause();
engine.seek_relative(10.0);
engine.set_volume(80);

let snap = engine.snapshot();             // PlayerSnapshot { state, position_secs,
                                          //   duration_secs, volume, title }
let mut rx = engine.subscribe();          // watch::Receiver — await rx.changed()
let mut ev = engine.subscribe_events();   // broadcast::Receiver<AudioEvent>

engine.shutdown();                        // send Shutdown + join; Drop does this too
```

### `AudioEvent`
- `TrackEnded` — emitted **exactly once** per natural finish (sink drained while
  `Playing`, past a 2s guard). The engine flips itself to `Stopped`.
- `LoadError(String)` — decode/open failure.
- `DeviceUnavailable` — no audio output device; engine runs **headless**
  (ignores `Play`, keeps publishing `Stopped` snapshots).

### Design guarantees (validated by review + tests)
- **Send+Sync boundary:** the handle stores only channel ends + a `JoinHandle`;
  the `broadcast::Receiver` (not `Sync`) is never stored — consumers call
  `subscribe_events()`. The rodio `Player`/device sink never leave the thread.
- **No panics on seek:** native `try_seek` first, then reload-and-seek-forward
  fallback (covers rodio 0.22 backward-seek `SeekError`). Seek baselines use
  checked `Instant` arithmetic; non-finite/huge targets are clamped.
- **No deadlock/hang on shutdown:** the loop uses `recv_timeout(150ms)`;
  `shutdown()` takes the `JoinHandle` so `Drop` never double-joins.
- **Device sink leak is intentional and bounded:** `std::mem::forget` once on the
  engine thread (rodio requires it to outlive the `Player`). Contract: **one
  `AudioEngine` per process.**

## How the TUI consumes the engine
`MusicPlayerApp` owns the `AudioEngine`, a `broadcast::Receiver<AudioEvent>`, and a
cached `PlayerSnapshot` refreshed once per frame (~20 FPS). Views read the cached
snapshot; the main loop drains `AudioEvent`s to drive auto-advance (with a
`Lagged` → `Stopped`-snapshot fallback).

## Persistence / backward compatibility
Config dir: `~/.config/youtube-music-player/` (`APP_NAME` is **frozen**). Files:
`history.json`, `queue.json`, `download_cache.json`, `playback_state.json`,
`feed_cache.json`. Writes are atomic (temp file + rename, 0o600 on Unix). The
`Track` serde shape and all filenames are **frozen** — extend only with
`#[serde(default)]` fields. `PlaybackState.volume` already uses `#[serde(default)]`
(→ 100) for forward/backward compat.

## Future integration points (not yet built)
- **Tauri GUI:** put `AudioEngine` in Tauri managed `State`; `#[tauri::command]`s
  call the engine methods; drive `app.emit` from `subscribe()`/`subscribe_events()`.
  Close-to-tray = `WindowEvent::CloseRequested` → `prevent_close()` + `window.hide()`;
  restore via a tray **right-click menu** item (Linux libappindicator does not fire
  tray left-click). Tray appears in Waybar's `tray` (SNI) module.
- **MPRIS2 (`mpris-server`/zbus):** own a `subscribe_events()` receiver + a
  `subscribe()` watch; map `AudioEvent`/`PlayerSnapshot` → MPRIS signals
  (`PropertiesChanged`, `Seeked`). Bus name `org.mpris.MediaPlayer2.crusty`.
  Add `Track.album`/`artwork` via `#[serde(default)]` when needed. Works with the
  Waybar `mpris` module + `playerctl -p crusty`.

See `CRUSTY_CORE_EXTRACTION_PLAN.md` for the phased history of this refactor.
