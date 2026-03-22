# Phase 4 Security Review Backlog

## MEDIUM

1. **In-memory history grows unbounded** — `limit_history` not called before delete-save path. Session loading 10K entries holds them all in memory until next play event. Call `limit_history` after `load_history` in constructor. (`app.rs:146`, `app.rs:1427`)

2. **load_history does not strip/validate url and local_file fields** — Deserialized history tracks have expired CDN URLs and potentially tampered `local_file` paths. Queue load validates with `is_allowed_youtube_url` but history load does not. Strip `url`/`local_file` from deserialized history tracks. (`persistence.rs:47-48`, `queue.rs:71`)

## LOW

3. **TOCTOU race between metadata size check and read_to_string** — Open file once, check length from handle, then read. (`persistence.rs:41-46, 86-89`)

4. **anyhow error chains may expose absolute config path in stderr** — Context strings chain OS errors with full paths. Informational for desktop app. (`app.rs:236, 420, 424, 1428`)

5. **Written JSON files world-readable under typical umask** — `fs::write` uses process umask. Set `0o600` permissions on history.json and queue.json. (`persistence.rs:69, 110`)
