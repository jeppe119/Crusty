# Phase 4 Security Review Backlog

## MEDIUM

1. ~~**In-memory history grows unbounded**~~ FIXED — Added `queue.limit_history(MAX_HISTORY_SIZE)` after `load_history` in constructor.

2. ~~**load_history does not strip/validate url and local_file fields**~~ FIXED — `load_history` and `load_queue` now strip `local_file = None` on all deserialized tracks. Added tests `load_history_strips_local_file` and `load_queue_strips_local_file`.

## LOW

3. ~~**TOCTOU race between metadata size check and read_to_string**~~ FIXED — Both load methods now open file once and read from handle.

4. **anyhow error chains may expose absolute config path in stderr** — Informational for desktop app. No change needed.

5. ~~**Written JSON files world-readable under typical umask**~~ FIXED — Added `fs::set_permissions(path, 0o600)` after writing on unix.
