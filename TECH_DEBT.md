# Tech Debt

Non-blocking items logged during the `crusty-core` extraction (see
`CRUSTY_CORE_EXTRACTION_PLAN.md`). CRITICAL/HIGH findings were fixed in-phase;
the items below are MEDIUM/LOW and deferred.

## From the Phase F `rust-reviewer` pass (engine + TUI rewire)

- **[MEDIUM] Typed errors instead of `String`** — `crusty-core` engine surfaces
  decode/open failures as `String` (`AudioEvent::LoadError(String)`,
  `decode_from_file -> Result<_, String>`). For a foundational library, a
  `thiserror` enum would let non-TUI consumers (GUI/MPRIS) branch on cause.
  Acceptable for now since these are user-facing messages.
  `crates/crusty-core/src/engine/{handle.rs,snapshot.rs}`.

- **[LOW] Store `PathBuf`, not `String`, for the current file** — the backward-seek
  reload path round-trips `path.to_string_lossy()` → `PathBuf::from`, which would
  corrupt non-UTF-8 paths. Temp download files are ASCII, so not hit in practice.
  `crates/crusty-core/src/engine/handle.rs` (`current_file_path`).

- **[LOW] Seek-while-paused position drift** — recomputing `start_time` from
  `Instant::now()` during a seek while paused ignores the active `pause_time`,
  causing sub-tick drift that self-corrects on resume. Cosmetic.
  `crates/crusty-core/src/engine/handle.rs` (`seek_to`).

- **[LOW] Device-sink leak is per-engine, not per-process** — `std::mem::forget`
  on the device sink is correct/bounded for the app (one engine per process), but
  the unit tests construct ~8 engines, each leaking a sink on machines with a real
  audio device (harmless in headless CI). Consider documenting the "one engine per
  process" caller contract more prominently or guarding tests.
  `crates/crusty-core/src/engine/handle.rs`.

- **[LOW] Headless play gives no feedback** — in headless mode (`player == None`),
  `Play` is silently ignored and `DeviceUnavailable` only fires once at startup.
  Pressing Play later produces no signal. Minor UX.
  `crates/crusty-core/src/engine/handle.rs` (`load_and_play`).

- **[LOW] Optimistic snapshot mutation in the TUI** — `volume_up/down` write
  `self.snapshot.volume` directly (a cache that is wholesale-replaced each frame)
  for responsive repeat-presses; `position_secs` is not optimistically updated, so
  the "+10s" seek status text can differ from the bar for one frame. Works as
  intended; mild smell. `crates/crusty-tui/src/ui/playback.rs`.

## Pre-existing clippy lints (carried over, not introduced by the refactor)

- **[LOW] `% 4 == 0` → `is_multiple_of`** — clippy suggests `.is_multiple_of(4)`.
  Pre-existing animation-frame code; left as-is to keep the refactor scope clean.
  `crates/crusty-tui/src/ui/app.rs` (animation tick).
