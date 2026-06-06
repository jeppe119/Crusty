# Implementation Plan: Extract `crusty-core` Crate

> **Status:** Approved, ready to execute.
> **Goal:** No-regret first step toward a future Tauri GUI branch (+ MPRIS + Waybar tray).
> **Invariant:** The existing TUI compiles and passes tests after **every** phase. No big-bang.

---

## Overview

Split the single `crusty` crate into a Cargo workspace with a framework-agnostic
`crusty-core` (audio engine, queue, downloads, config/state, metadata) and
`crusty-tui` (existing ratatui app consuming core). The riskiest piece — wrapping
the `!Send` rodio player in a thread-backed `Send + Sync` engine — is built and
tested in isolation *before* the TUI is rewired, so the TUI compiles and passes
tests after every phase.

## Why this work (strategic context)

- The chosen direction is a future **Tauri v2 + Svelte 5 + SvelteKit + Tailwind**
  GUI branch for robust close-to-tray (native window hide/show beats TUI
  daemon-fork + PTY reconnect).
- MPRIS2 (`mpris-server`/zbus) + Waybar tray (`ksni` for TUI / Tauri tray for GUI)
  is process-agnostic and identical effort either way — it does NOT drive the decision.
- Extracting `crusty-core` lets the existing TUI and the future Tauri backend consume
  the SAME engine, preventing divergence. This is the no-regret first move under any route.

## Verified facts that shape the plan

- `main.rs`: `#[tokio::main]`, builds `MusicPlayerApp::new()?`, `app.run().await`.
- `MusicPlayerApp` (app.rs:47) owns `player: AudioPlayer`, `queue: Queue`,
  `downloads: DownloadManager`, `persistence`, channels, UI sub-structs.
- Run loop (app.rs:274–529): ~20 FPS render, polls `search_rx`/`feed_rx`/
  `downloads.poll_completion()`, auto-advance is `if player.is_finished() &&
  get_state()==Playing`, `event::poll(16ms)`, shutdown reads `get_volume()`/
  `get_time_pos()`, saves state, restores terminal.
- `AudioPlayer` (audio.rs): `player: Option<Player>` (None on headless), wall-clock
  position (`start_time`/`pause_time`/`total_paused_duration`), two-step `seek()`+
  `apply_seek()` with native `try_seek` then reload-fallback, `std::mem::forget(device_sink)`,
  `Drop` calls `stop()`. **Synchronous** `&mut self` API used by TUI.
- `Queue`/`Track` (queue.rs): pure logic, already has 20+ unit tests; `Track` is
  `serde` and is the persisted shape.
- `DownloadManager` (services/download.rs **not** player/): `Arc<Mutex<DownloadState>>`,
  `tokio::spawn` + `spawn_blocking(yt-dlp)`, tokio `mpsc`. `cookie_config` is a plain
  `Option<(bool, String)>` — no custom type, no youtube-module coupling.
- `config.rs`: `APP_NAME = "youtube-music-player"` (⚠ drives `~/.config` path),
  `config_dir`, `is_allowed_youtube_url`, `format_time`, `clean_title`. Everything `pub(crate)`.
- `PersistenceService` (services/persistence.rs): `write_atomic`,
  `PlaybackState{video_id,position_secs,title,duration,volume}`, **depends on
  `crate::ui::state::QueueState`** — the one UI↔core coupling.
- `QueueState` (ui/state.rs:161): clean serde DTO `{ tracks: Vec<Track>,
  current_track: Option<Track> }` — just mislocated. Easy to relocate.
- `feed.rs`/`playlist.rs`/`youtube/*`: YouTube/network-specific. **Out of scope** — stay in TUI.

---

## Pre-Coding Decisions (resolve before Phase D)

| # | Decision | Recommendation | Rationale |
|---|----------|----------------|-----------|
| 1 | Command channel transport | **`crossbeam-channel`** | `Sender: Send + Sync` (std `mpsc::Sender` is not `Sync`); `recv_timeout` gives the engine loop a clean "wait for command OR tick" without tokio. tokio `mpsc` would force runtime semantics onto a std thread. |
| 2 | Snapshot transport | **`tokio::sync::watch`** | `Sender::send` is sync (no runtime needed from the std engine thread); `Receiver` is `Clone + Send + Sync`, TUI reads via sync `borrow()`, future Tauri/MPRIS await `changed()`. Beats a `Mutex<Snapshot>` poll. |
| 3 | Discrete events (TrackEnded, LoadError) | **`tokio::sync::broadcast`**, small capacity (e.g. 16) | Multi-consumer for future MPRIS + Tauri + TUI. `Sender: Send + Sync` (kept in handle); each consumer owns its `Receiver`. |
| 4 | "Track finished" representation | **Event, not snapshot field** | Discrete one-shot; a `watch` field would re-trigger every frame. Engine emits `TrackEnded` once, using the existing `empty() && state==Playing && pos>=2s` guard internally. |
| 5 | Domain command enum location | **Keep `AppCommand` (key→action) in TUI**; add `AudioCommand` in core | `AppCommand` is input-mapping (crossterm keys). Core only needs `AudioCommand`. No churn to `ui/input.rs`. |
| 6 | `QueueState` DTO | **Move serde DTO to core** as `PersistedQueue`; keep UI selection/scroll in `ui::state` | Resolves the only persistence↔UI coupling cleanly. |
| 7 | Visibility on moved items | Promote `pub(crate)` → `pub` in core; TUI re-imports | Cross-crate access. |
| 8 | `APP_NAME` / state filenames / `Track` serde shape | **DO NOT CHANGE** (`"youtube-music-player"`, `history.json`/`queue.json`/`download_cache.json`/`playback_state.json`) | Backward-compat: must keep reading existing `~/.config` files. |
| 9 | Engine thread vs tokio | **Plain `std::thread`**, independent of the runtime | rodio is thread-affine; thread owns the stream. tokio sync senders work from std threads. |
| 10 | `Track` metadata (album/artwork for MPRIS) | **Defer**; do not change serde shape now | `Track` lives in core after Phase A, trivially extendable later with `#[serde(default)]` fields. |

---

## Target Workspace Layout

```
crusty/                      (workspace root: Cargo.toml [workspace])
├── crates/
│   ├── crusty-core/         lib
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs            (APP_NAME, paths, url/time helpers)
│   │       ├── model/              (Track, PlayerState, PersistedQueue, PlaybackState)
│   │       ├── queue.rs
│   │       ├── download.rs          (DownloadManager)
│   │       ├── persistence.rs       (PersistenceService, write_atomic)
│   │       └── engine/              (AudioEngine, AudioCommand, PlayerSnapshot, AudioEvent)
│   └── crusty-tui/          bin (the existing app)
│       └── src/ … (ui/, youtube/, services/feed.rs, services/playlist.rs, main.rs)
└── (crates/crusty-gui/      later: Tauri — OUT OF SCOPE)
```

---

## crusty-core Public API Sketch

```rust
// ---- model ----
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState { Stopped, Playing, Paused, Loading }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Track { pub video_id, pub title, pub duration: u64,
                   pub uploader, pub url, pub local_file: Option<String> }
// (serde shape FROZEN; future album/artwork via #[serde(default)])

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedQueue { pub tracks: Vec<Track>, pub current_track: Option<Track> }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlaybackState { pub video_id: String, pub position_secs: f64,
                           pub title: String, pub duration: f64, pub volume: u32 }

// ---- config ---- (pub fns; APP_NAME unchanged)
pub fn config_dir() -> anyhow::Result<PathBuf>;
pub fn is_allowed_youtube_url(url: &str) -> bool;
pub fn format_time(seconds: f64) -> String;
pub const MAX_CONCURRENT_DOWNLOADS: usize; /* …other consts… */

// ---- queue ---- (unchanged API, now pub)
pub struct Queue { /* … */ }
impl Queue { pub fn next(&mut self)->Option<Track>; /* … existing surface … */ }

// ---- download ---- (unchanged API, now pub)
pub struct DownloadManager { /* … */ }
impl DownloadManager {
    pub fn new() -> Self;
    pub fn with_cache(cache: HashMap<String,String>) -> Self;
    pub fn spawn_download(&self, track: &Track, cookie: Option<(bool,String)>) -> bool;
    pub fn poll_completion(&mut self) -> Option<(String, Result<String,String>)>;
    /* get_cached_file, is_cached, ensure_next_tracks_ready, abort_all,
       cleanup_old_downloads, get_cache_snapshot … unchanged */
}

// ---- persistence ---- (now pub; consumes PersistedQueue, not ui::state)
pub struct PersistenceService { /* … */ }
impl PersistenceService {
    pub fn new() -> anyhow::Result<Self>;
    pub fn from_dir(dir: PathBuf) -> Self;          // test seam
    pub fn load_history(&self) -> anyhow::Result<Vec<Track>>;
    pub fn save_queue(&self, q: &PersistedQueue) -> anyhow::Result<()>;
    pub fn load_queue(&self) -> anyhow::Result<PersistedQueue>;
    pub fn save_playback_state(&self, s: &PlaybackState) -> anyhow::Result<()>;
    pub fn load_playback_state(&self) -> Option<PlaybackState>;
    /* save/load download_cache, clear_playback_state … unchanged */
}

// ================= ENGINE (Phase D) =================
pub enum AudioCommand {
    Play { path: PathBuf, title: String, duration_secs: f64 },
    Pause, Resume, TogglePause, Stop,
    SeekTo(f64), SeekRelative(f64),
    SetVolume(u32),
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct PlayerSnapshot {
    pub state: PlayerState,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume: u32,
    pub title: String,
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    TrackEnded,                 // emitted ONCE on natural finish (2s guard + state==Playing)
    LoadError(String),          // decode/open failure
    DeviceUnavailable,          // no output device
}

/// Send + Sync handle. Owns nothing thread-affine.
pub struct AudioEngine {
    cmd_tx: crossbeam_channel::Sender<AudioCommand>,         // Send + Sync
    snapshot_rx: tokio::sync::watch::Receiver<PlayerSnapshot>, // Clone + Send + Sync
    events_tx: tokio::sync::broadcast::Sender<AudioEvent>,   // Send + Sync (for subscribe)
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AudioEngine {
    /// Spawns the std::thread that owns the rodio OutputStream/Player.
    pub fn new() -> Self;

    // fire-and-forget commands (non-blocking; safe to call inside async fns)
    pub fn play(&self, path: impl Into<PathBuf>, title: impl Into<String>, duration_secs: f64);
    pub fn pause(&self);  pub fn resume(&self);  pub fn toggle_pause(&self);
    pub fn stop(&self);
    pub fn seek_to(&self, secs: f64);  pub fn seek_relative(&self, secs: f64);
    pub fn set_volume(&self, v: u32);

    // state out
    pub fn snapshot(&self) -> PlayerSnapshot;                    // self.snapshot_rx.borrow().clone()
    pub fn subscribe(&self) -> watch::Receiver<PlayerSnapshot>;  // async consumers (Tauri/MPRIS)

    // events out
    pub fn subscribe_events(&self) -> broadcast::Receiver<AudioEvent>; // each consumer owns one

    // lifecycle
    pub fn shutdown(mut self);  // send Shutdown, join thread
}
impl Drop for AudioEngine { /* best-effort Shutdown + join if not already taken */ }
```

**Why `AudioEngine` is `Send + Sync`:** all fields are (`crossbeam::Sender`,
`watch::Receiver`, `broadcast::Sender`, `JoinHandle`). The `broadcast::Receiver`
(not `Sync`) is **not** stored in the handle — the TUI owns its own via
`subscribe_events()`. The rodio `Player`/`OutputStream` live only inside the engine
thread's stack.

**Engine thread loop (port of audio.rs, no public API):**

```
open default sink on this thread (player: Option<Player>, leak device_sink as today)
loop {
  match cmd_rx.recv_timeout(TICK /*~150ms*/) {
    Ok(Play{..})  => decode + append (port play_with_duration), reset wall-clock
    Ok(SeekTo/SeekRelative) => try_seek; on SeekError::NotSupported → reload+seek-forward fallback
    Ok(SetVolume/Pause/Resume/TogglePause/Stop) => port existing methods
    Ok(Shutdown)  => break (drop Player → stops audio)
    Err(Timeout)  => {}            // just refresh + check-finished below
  }
  recompute position (wall-clock arithmetic) ;
  if natural_finish_detected_once() { events_tx.send(TrackEnded) }   // empty()&&Playing&&pos>=2s
  snapshot_tx.send_replace(build_snapshot());   // coalesced; cheap
}
```

---

## Implementation Phases

Each phase ends with **`cargo build` + `cargo test` green across the workspace** and a mergeable PR.

### Phase 0 — Workspace scaffolding (no logic moved)

1. **Create workspace root** (`Cargo.toml` `[workspace] members=["crates/*"]`).
2. **Move existing crate → `crates/crusty-tui`** (git-mv `src/` and `Cargo.toml`; bin name stays `crusty`).
3. **Create empty `crates/crusty-core`** (`lib.rs` with nothing but a crate doc). `crusty-tui` adds `crusty-core = { path = "../crusty-core" }` (unused yet).

- **TUI change:** none functional; only paths.
- **Test gate:** `cargo test` (all existing tests) passes from the new layout. Risk: **Low**.

### Phase A — Move pure types + config

1. **Move `Track`** + its `new()` → `crusty-core::model::track` (`pub`). Keep serde derives + field names.
2. **Move `PlayerState` enum** (only the enum) → `crusty-core::model` (`pub`). `AudioPlayer` stays in TUI and imports it from core.
3. **Move `config.rs`** helpers/consts → `crusty-core::config` (`pub`), `APP_NAME` unchanged.
4. **TUI change:** replace `crate::player::queue::Track` etc. with `crusty_core::Track`; add `pub use crusty_core::{Track, PlayerState};` shim in `player/queue` and `player/audio` to minimize import churn. Move `config.rs` unit tests with it.

- **Test gate:** `cargo test -p crusty-core` (config tests) + full TUI build/tests. Risk: **Low**.

### Phase B — Move Queue

1. **Move `queue.rs`** (struct + impl + its 20+ tests) → `crusty-core::queue` (`pub`). Depends only on `Track` + std.
2. **TUI change:** import `crusty_core::Queue`; delete `src/player/queue.rs`, keep a re-export if convenient.

- **Test gate:** queue unit tests now run under `cargo test -p crusty-core`. Risk: **Low**.

### Phase C — Move DownloadManager + persistence + state DTOs

1. **Move `QueueState` serde DTO** from `ui/state.rs` → `crusty-core::model::PersistedQueue`. Leave UI-only fields (`UiState` selections) in `ui/state.rs`.
2. **Move `PlaybackState`** → `crusty-core::model`.
3. **Move `PersistenceService` + `write_atomic` + size/history consts** → `crusty-core::persistence`. Re-target its `QueueState` use to `PersistedQueue`. Keep filenames + atomic-write logic byte-identical.
4. **Move `DownloadManager`** (`services/download.rs` incl. `fetch_audio_url_blocking`) → `crusty-core::download`. `cookie_config: Option<(bool,String)>` stays plain. core gains `tokio` (full or `rt-multi-thread`+`process`+`sync`) + `tempfile` + `dirs` deps.
5. **TUI change:** import these from core. Where the TUI persists the queue, convert its `Queue` ↔ `PersistedQueue` (small `to_persisted()/restore_from()` helpers). `feed.rs`/`playlist.rs` stay in TUI and keep calling the plain cookie tuple.

- **Test gate:** persistence round-trip tests using `from_dir(temp)` (history/queue/playback_state/download_cache write→read), DownloadManager unit tests (`with_cache`, `is_cached`, `get_cache_snapshot`, rate-limit slot logic). Target **80%+** on moved logic. Risk: **Medium** (the `QueueState` decoupling + config-path compat).

### Phase D — Build `AudioEngine` in core (the hard one) — *not wired to TUI*

1. **Add deps to core:** `crossbeam-channel`, `tokio` (`sync`), keep `rodio 0.22`.
2. **Create `engine/`**: `AudioCommand`, `PlayerSnapshot`, `AudioEvent`, `AudioEngine`, and the private engine-thread loop. Port all of `audio.rs`: decode/append, pause/resume/toggle/stop, volume, **wall-clock position arithmetic**, **seek** (native `try_seek` → reload-and-seek-forward fallback, explicitly matching `SeekError::NotSupported`), and the **finish detection** (`empty() && state==Playing && pos>=2.0`) → emit `TrackEnded` exactly once then set internal `Stopped`.
3. **Headless path:** `player: None` → ignore `Play` (or emit `DeviceUnavailable`), still publish `Stopped` snapshots (mirrors current `Option<Player>`).
4. **Lifecycle:** `new()` spawns thread (opens sink *on that thread*); `shutdown()` sends `Shutdown` + joins; `Drop` is a best-effort fallback.
5. **TUI change:** none — core just gains an (as-yet unused) module. TUI keeps its own `AudioPlayer`.

- **Test gate (integration, gating):**
  - command→snapshot round-trips: `set_volume(50)` then poll `subscribe()` until `volume==50`; `play`→state `Playing`/`Loading`; `pause`→`Paused`.
  - seek clamping (negative→0, beyond duration→duration) via snapshot.
  - `TrackEnded` emitted once (drive with a short fixture/sine WAV or a stub decode path; assert single event).
  - backward-seek path exercises the `NotSupported` fallback without panic.
  - `Send + Sync` asserted at compile time: `fn _assert<T: Send + Sync>(){} _assert::<AudioEngine>();`.
  - headless CI: must not require an audio device (tests target the `None`/command-bookkeeping paths). Risk: **High** — see risk table.

### Phase E — Rewire `crusty-tui` onto `AudioEngine`

1. **`MusicPlayerApp` fields:** replace `player: AudioPlayer` with `engine: AudioEngine` and `engine_events: broadcast::Receiver<AudioEvent>` (from `engine.subscribe_events()` in `new()`). Optionally cache `snapshot: PlayerSnapshot` refreshed once per frame.
2. **`ui/playback.rs`:**
   - `toggle_pause()` → `engine.toggle_pause()`.
   - `seek_forward/backward` → `engine.seek_relative(±10.0)` (drop the `seek()+apply_seek()` two-step; status text uses `engine.snapshot().position_secs`).
   - `volume_up/down` → read `snapshot().volume`, compute, `engine.set_volume(..)`.
   - `play_track_from_cache_or_download` → `engine.play(local_file, title, duration)` (now non-blocking — decode moves off the UI thread, a bonus latency win).
   - `try_resume_playback` → `engine.set_volume`, `engine.play(..)`, then `engine.seek_to(pos)`.
3. **`ui/app.rs` run loop:**
   - Per frame: `let snap = self.engine.snapshot();` replaces `get_state/get_time_pos/get_duration/get_volume`.
   - **Auto-advance:** replace `is_finished() && state==Playing` with draining events:
     `while let Ok(ev) = self.engine_events.try_recv() { match ev { TrackEnded => {clear_playback_state(); if !queue.is_empty(){play_next().await} else {engine.stop()}}, LoadError(e)=>status, DeviceUnavailable=>status } }`. Handle `TryRecvError::Lagged` by falling back to a `snapshot.state==Stopped` check.
   - **Shutdown:** read `snap.position_secs`/`snap.volume` **before** stopping; save `PlaybackState`; save history/queue/cache; `self.engine.shutdown()` (join); then restore terminal. Preserve `downloads.abort_all()` ordering.
4. **`ui/views/player_bar.rs`** and any view reading player state → read from the cached `snapshot`.
5. **Delete `src/player/audio.rs`** from TUI.

- **Test gate:** full workspace `cargo test`; manual/e2e smoke of play, pause/resume (space), seek ±10s, volume ±, natural auto-advance, resume-on-restart, headless start (no device). Risk: **High** (behavioral parity).

### Phase F — Cleanup

1. Remove transitional re-export shims; tighten visibility (`pub(crate)` where cross-crate not needed).
2. Doc-comment the core public API; add a short `ARCHITECTURE.md` describing the engine contract (for the future Tauri/MPRIS authors).
3. Run **rust-reviewer**; fix CRITICAL/HIGH, log MEDIUM/LOW to `TECH_DEBT.md`.
4. Confirm an existing `~/.config/youtube-music-player/*.json` still loads (compat check).

- **Test gate:** `cargo test` + `cargo clippy -- -D warnings`. Risk: **Low**.

---

## Risks & Mitigations

- **`!Send` rodio across reuse (the core reason for this work).**
  - *Mitigation:* `OutputStream`/`Player` created and used **only** inside the engine `std::thread`; handle exposes only `Send + Sync` channel ends. Compile-time `_assert::<AudioEngine>()` for `Send + Sync`.
- **rodio 0.22 backward-seek `SeekError::NotSupported`.**
  - *Mitigation:* keep the proven two-stage strategy inside the engine: native `try_seek` first; on error, reload file + seek-forward-from-zero (port of `apply_seek`). Match the error explicitly; never `unwrap`.
- **Position accuracy: `watch` snapshot vs 20 FPS poll.**
  - *Mitigation:* engine ticks ~150ms via `recv_timeout`, recomputes wall-clock position, `send_replace`. Progress bar is second-resolution → imperceptible. TUI reads `borrow()` each frame (cheap clone; `title` String clone at 20 FPS is negligible — optionally gate on `has_changed()`).
- **Shutdown position staleness.** Saved position may be ≤150ms stale.
  - *Mitigation:* acceptable for resume; if exact value desired later, add a `Command::FlushSnapshot` ack. Not needed now.
- **`TrackEnded` double-fire / missed (broadcast lag).**
  - *Mitigation:* engine emits once then flips internal `Stopped`; TUI treats `Lagged` by re-checking `snapshot.state==Stopped` as a fallback advance trigger. Capacity 16 (events are rare).
- **Download channel ownership.** `poll_completion(&mut self)` keeps the `mpsc::Receiver` inside `DownloadManager`.
  - *Mitigation:* move `DownloadManager` whole (rx included) into core; TUI keeps `&mut downloads`. No channel split. The `pending_play_track`/retry orchestration stays in TUI (it's UI policy).
- **Config / state-file compatibility.**
  - *Mitigation:* freeze `APP_NAME`, all four JSON filenames, atomic-write logic, and `Track` serde field names. Phase F compat check on a real config dir.
- **`tokio` runtime ↔ std engine thread.**
  - *Mitigation:* engine thread never enters the runtime; `watch::send`/`broadcast::send` are sync and runtime-free from a std thread. Validated assumption — verify in Phase D integration test (engine usable from a `#[test]` without `#[tokio::test]`).
- **Drop ordering (engine join vs runtime teardown).**
  - *Mitigation:* explicit `engine.shutdown()` in the run-loop teardown before `main` returns; `Drop` is a fallback only.
- **`DownloadManager` carries `tokio::spawn`/`spawn_blocking` into core.**
  - *Mitigation:* accept for "no-regret" extraction (core depends on tokio anyway via the rest). Note a *future* `Downloader` trait so a Tauri backend could inject its own runtime/strategy — **not now**.

---

## Out of Scope (designed-for, not built)

- **No Tauri** crate/commands. (Engine `subscribe()`/`subscribe_events()`/command methods are exactly what Tauri `#[command]`s will call; `AudioEngine` goes in Tauri `State`.)
- **No MPRIS** D-Bus handler. (Will own a `subscribe_events()` receiver + a `subscribe()` watch; maps `AudioEvent`/`PlayerSnapshot` → MPRIS signals. Add `Track.album/artwork` then via `#[serde(default)]`.)
- **No Waybar/system tray.**
- **No `crusty-gui` crate.**
- **YouTube extraction, feed, playlist** (`youtube/*`, `services/feed.rs`, `services/playlist.rs`) **stay in `crusty-tui`** for this extraction — they're network/UX policy, not the shared engine. (Could become `crusty-yt` later.)
- No new `Track` metadata fields, no equalizer/gapless/visualization.

---

## Later Phases (post-extraction, recorded for context)

- **GUI branch:** scaffold `crusty-gui` Tauri v2 + Svelte 5 + SvelteKit (`adapter-static`, `fallback: index.html`, `ssr=false`) + Tailwind v4. `AudioEngine` in Tauri managed `State`; `#[tauri::command]`s call engine methods; `app.emit` driven by `subscribe()`/`subscribe_events()`.
- **Close-to-tray (GUI):** `WindowEvent::CloseRequested` → `api.prevent_close()` + `window.hide()`; `RunEvent::ExitRequested` → `api.prevent_exit()`. ⚠ Linux tray left-click does NOT fire (libappindicator) — use a right-click context-menu "Show"/"Quit" + always set a menu (icon may be invisible without one).
- **MPRIS2:** `mpris-server` 0.9 (`tokio` feature), `LocalServer`+`LocalPlayerInterface`. Bus name `org.mpris.MediaPlayer2.crusty`. Omit `mpris:artUrl`/`xesam:album`; required `mpris:trackid` changes per track. Emit `Seeked` + `PropertiesChanged`. Waybar `mpris` module + `playerctl -p crusty …` for controls.
- **TUI tray (if kept):** `ksni` 0.3.x (SNI; works under Waybar `tray` module).

---

## Success Criteria

- [ ] `cargo build` + `cargo test` green at the end of **every** phase (0–F).
- [ ] `crusty-core` compiles with **no `ratatui`/`crossterm` dependency**.
- [ ] `AudioEngine` passes a compile-time `Send + Sync` assertion.
- [ ] Engine integration tests cover command→snapshot round-trips, seek clamping + backward-seek fallback, single `TrackEnded`, headless `None` path — **80%+** coverage on core logic.
- [ ] TUI behavioral parity: play/pause/resume, seek ±10s, volume ±(shift), natural auto-advance, resume-on-restart, headless start — all unchanged for the user.
- [ ] Existing `~/.config/youtube-music-player/*.json` files load without migration.
- [ ] `src/player/audio.rs` deleted from the TUI; the TUI consumes `crusty_core::AudioEngine` exclusively.
