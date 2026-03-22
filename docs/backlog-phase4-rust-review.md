# Phase 4 Rust Review Backlog

## HIGH

1. ~~**Redundant PersistenceService construction in spawn_blocking**~~ FIXED — Added `from_dir()` + `config_dir()` accessor. `load_queue_async` now clones config_dir and uses `from_dir`.

2. ~~**History trimming split across two locations**~~ FIXED — Added `limit_history` after `load_history` in constructor. In-memory history is now capped immediately after loading.

## MEDIUM

3. ~~**Inconsistent error reporting in delete_selected_history_item**~~ FIXED — Changed from `eprintln!` to `self.status_message`.

4. ~~**TOCTOU race on file existence check**~~ FIXED — Both `load_history` and `load_queue` now open file once and check len from handle.

5. ~~**PersistenceService missing Clone/from_dir**~~ FIXED — Added `pub(crate) fn from_dir(config_dir: PathBuf)` and `pub(crate) fn config_dir(&self) -> &Path`.

## LOW

6. **search_history #[allow(dead_code)]** — Kept, will be wired in future UI feature.

7. ~~**pub fn visibility wider than needed**~~ FIXED — All methods changed to `pub(crate)`.
