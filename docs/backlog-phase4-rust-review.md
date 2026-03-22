# Phase 4 Rust Review Backlog

## HIGH

1. **Redundant PersistenceService construction in spawn_blocking** — `load_queue_async` creates a new PersistenceService inside `spawn_blocking` instead of reusing the existing one. Add `Clone` or `from_dir()` constructor. (`app.rs:164-168`)

2. **History trimming split across two locations** — In-memory `limit_history(100)` only runs in `play_next`/`play_previous`, not on delete-save paths. The `PersistenceService::save_history` slice trim prevents oversized files but in-memory queue can grow unbounded. (`app.rs:796,809`, `persistence.rs:61`)

## MEDIUM

3. **Inconsistent error reporting in delete_selected_history_item** — Uses `eprintln!` (invisible in TUI) instead of `self.status_message` like `clear_history` does. (`app.rs:1427` vs `app.rs:1403`)

4. **TOCTOU race on file existence check** — `path.exists()` then `fs::metadata()` are two syscalls. Idiomatic: open file, match on NotFound, check len from open handle. (`persistence.rs:37-39, 79-81`)

5. **PersistenceService missing Clone/from_dir** — Blocks clean fix for HIGH #1. Add `#[derive(Clone)]` or `pub(crate) fn from_dir(config_dir: PathBuf)`. (`persistence.rs:22-24`)

## LOW

6. **search_history #[allow(dead_code)] needs TODO comment** — Add `// TODO(phase-6): remove allow` for clarity. (`persistence.rs:120`)

7. **pub fn visibility wider than needed** — Methods are `pub` but struct is `pub(crate)`. Could be `pub(crate)` consistently. (`persistence.rs:27,34,57,76,105`)
