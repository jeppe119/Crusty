# Phase 5-6 Combined Review Backlog

## HIGH

1. **TOCTOU in spawn_download — three separate mutex locks** — Safe today (single-threaded caller). Deferred: merging into single DownloadState struct is a larger refactor. (`download.rs:73-120`)

2. ~~**MAX_CONCURRENT_DOWNLOADS = 30 is too high**~~ FIXED — Reduced to 5. (`config.rs:20`)

3. ~~**Weak URL guard in play_track_from_cache_or_download**~~ FIXED — Replaced `.contains("youtube.com")` with `is_allowed_youtube_url()`. (`app.rs:771`)

4. **app.rs exceeds 800-line limit** — 1326 lines. Playlist functions already extracted. Remaining playback methods could be extracted further in a future session.

5. ~~**pub field exposure on DownloadManager**~~ FIXED — Added `active_count()` and `cached_count()` accessors, made fields private. Updated cache_stats.rs to use accessors. (`download.rs:16,18`)

## MEDIUM

6. **eprintln leaks to terminal behind TUI** — Deferred: informational for desktop app. (`download.rs:302`, `app.rs:713,1254`)

7. **background_tasks grows unbounded between prune cycles** — Deferred: low impact, JoinHandle is small. (`download.rs:173-181`)

8. **load_queue_async blocks main task** — Deferred: existing pattern works for typical queue sizes. (`app.rs:146-231`)

## LOW

9. **DownloadManager::new() missing Default impl** — Deferred: cosmetic. (`download.rs:26`)

10. **_use_from_browser boolean always ignored** — Deferred: API cleanup. (`download.rs:290`)

11. **Magic numbers for cleanup timers** — Deferred: cosmetic. (`app.rs:312`, `download.rs:228`)
