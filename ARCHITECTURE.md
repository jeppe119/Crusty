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

## MPRIS2 control surface (`crusty_core::mpris`, `mpris` feature)

Crusty registers `org.mpris.MediaPlayer2.crusty` on the session bus, giving
**Waybar** (`mpris` module), **Quickshell** (`Quickshell.Services.Mpris`), and
**`playerctl -p crusty`** display + control — with no GUI required (it runs in the
TUI process today, and the GUI will reuse it).

- **`EngineController`** (`Clone + Send + Sync`): the bridge's view of the engine
  (command sender + snapshot receiver) — `AudioEngine` itself isn't `Clone`
  (it owns the thread `JoinHandle`). Get one via `AudioEngine::controller()`.
- **`mpris::mapping`** — pure, unit-tested snapshot↔MPRIS conversions
  (`PlaybackStatus`, microsecond `Time`, volume, `build_metadata`, synthesized
  `mpris:trackid` as `/org/mpris/MediaPlayer2/crusty/track/{n}` — digits only, so
  always a valid object path despite YouTube IDs containing `-`/`_`).
- **`mpris::server`** — `CrustyMpris` implements `RootInterface` + `PlayerInterface`
  using the regular `Server<T>` (the handle is `Send + Sync`, so no `LocalServer`/
  `spawn_local`). `serve_mpris()` diffs the snapshot stream and emits
  `PropertiesChanged` **only on change** (no spam at the 150ms tick). Per the spec,
  `Position` is not in `PropertiesChanged`; emit `Seeked` after MPRIS-initiated seeks.
- **`MprisAction`** — queue navigation (`Next`/`Previous`), `PlayPause` (start-when-
  stopped), `Stop`, `Quit` live in the *host*, not the engine, so the bridge forwards
  them over an `mpsc` channel. The host maps each to the **same** method its
  keybindings use ⇒ external control == keyboard.
- **Feature flag:** `mpris` (default-on in `crusty-tui`). `--no-default-features`
  builds without D-Bus; `serve_mpris` errors (no session bus) are logged + swallowed.

## Theming — matugen (`themes/matugen/`)

Material You dynamic colour from the wallpaper, via [matugen](https://github.com/InioX/matugen).
One `matugen image …` run themes both front-ends:
- `crusty-gui.css` (template) → `~/.config/crusty/theme.css` → GUI CSS custom
  properties (`--md-sys-color-*`), consumed by Tailwind v4 `@theme`, hot-reloaded
  via `@tauri-apps/plugin-fs` `watch()`.
- `crusty-quickshell.json` (template) → `~/.cache/matugen/crusty-colors.json`,
  loaded by the shipped `quickshell/Colors.qml` singleton (`FileView` +
  `JsonAdapter`, hot-reloads natively, ships Material-dark defaults).
See `themes/matugen/README.md`.

## Future integration points (not yet built)
- **Tauri GUI (`crates/crusty-gui`):** put `AudioEngine` in Tauri managed `State`;
  `#[tauri::command]`s call the engine; drive `app.emit` from `subscribe()`/
  `subscribe_events()`; reuse the Phase-1 MPRIS bridge (`serve_mpris(..., can_raise=true)`).
  Close-to-tray = `WindowEvent::CloseRequested` → `prevent_close()` + `window.hide()`;
  restore via a tray **right-click menu** item (Linux libappindicator does not fire
  tray left-click). Tray appears in Waybar's `tray` (SNI) module. Consumes the
  matugen CSS above.
- **`crates/crusty-yt` (prerequisite for the GUI):** the YouTube search/feed/
  playlist/auth logic currently lives in `crusty-tui` (`youtube/*`, `services/feed.rs`,
  `services/playlist.rs`). The GUI can't search/play without it, so it must be
  extracted into a shared crate (depends on `crusty-core` for `Track`; no
  `ratatui`/`tauri`) before the GUI search UI is built.

See `CRUSTY_CORE_EXTRACTION_PLAN.md` for the phased history of this refactor.
