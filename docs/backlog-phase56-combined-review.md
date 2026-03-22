# Phase 5-6 Combined Review Backlog

## HIGH

1. ~~**TOCTOU in spawn_download — three separate mutex locks**~~ FIXED — Unified 4 mutex fields into single `DownloadState` struct. Single lock acquisition for all precondition checks.

2. ~~**MAX_CONCURRENT_DOWNLOADS = 30 is too high**~~ FIXED — Reduced to 5. (`config.rs:20`)

3. ~~**Weak URL guard in play_track_from_cache_or_download**~~ FIXED — Replaced `.contains("youtube.com")` with `is_allowed_youtube_url()`. (`app.rs:771`)

4. ~~**app.rs exceeds 800-line limit**~~ FIXED — Extracted to `ui/playback.rs` (190 lines), `ui/navigation.rs` (161 lines), `ui/actions.rs` (305 lines). `app.rs` now 702 lines.

5. ~~**pub field exposure on DownloadManager**~~ FIXED — Added `active_count()` and `cached_count()` accessors, made fields private. Updated cache_stats.rs to use accessors. (`download.rs:16,18`)

## MEDIUM

6. ~~**eprintln leaks to terminal behind TUI**~~ FIXED — Replaced with `status_message` in TUI-active paths. Kept 2 calls in shutdown path (after TUI teardown, `eprintln!` is correct). Removed from `download.rs` entirely.

7. ~~**background_tasks grows unbounded between prune cycles**~~ FIXED — Added periodic pruning in `poll_completion()`.

8. **load_queue_async blocks main task** — Deferred: already uses `spawn_blocking` correctly. No action needed.

## LOW

9. ~~**DownloadManager::new() missing Default impl**~~ FIXED — Added `impl Default for DownloadManager`.

10. ~~**_use_from_browser boolean always ignored**~~ FIXED — Renamed to `use_from_browser`, now conditionally applies `--cookies-from-browser`.

11. ~~**Magic numbers for cleanup timers**~~ FIXED — Extracted to `PLAYED_FILE_CLEANUP_DELAY_SECS`, `TEMP_FILE_MAX_AGE_SECS`, `LOOKAHEAD_DOWNLOAD_COUNT`, `STARTUP_DOWNLOAD_COUNT` in `config.rs`. History limit uses `MAX_HISTORY_SIZE` constant.
