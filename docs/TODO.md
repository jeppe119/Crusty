# TODO List - YouTube Music Player

## Completed ✅

### My Mix Implementation
- [x] Implement real My Mix fetching using yt-dlp
- [x] Implement fetching tracks from selected mix playlist
- [x] Test My Mix with real YouTube Music account
- [x] Handle edge cases (no mixes found, network errors, etc.)

### History Management
- [x] Add "Clear History" functionality (Shift+H to expand, Shift+C to clear)
- [x] Limit history size to prevent memory issues (limited to 100 tracks)
- [x] History persistence across app restarts

### Queue Pre-downloading
- [x] Implement smart 3-track buffer (prevents memory/CPU overload)
- [x] Implement cleanup of old pre-downloaded files (1 hour threshold)
- [x] Handle download failures gracefully (retry logic + failed tracking)

### UI/UX Improvements
- [x] Add visual progress bar for currently playing track
- [x] Add help screen (press `?` to show all keybinds)
- [x] Make queue vertical layout (10 tracks visible in compact, scrollable when expanded)
- [x] Add My Mix expansion (press 'm' to expand, Shift+M to refresh)
- [x] Add History expansion (Shift+H to expand/collapse)
- [x] Add view management (Home/Search views with 'h' and Esc navigation)
- [x] Clean video title tags (removes "(Official Video)", "(Lyric Video)", etc.)

### Playlist Management
- [x] Import playlists from YouTube URLs (press 'l' to load)
- [x] Display first 50 tracks from loaded playlist

## High Priority

### History Management (Remaining)
- [ ] Add history search/filter capability
- [ ] Cherry-pick delete individual history items

## Medium Priority

### UI/UX Improvements (Remaining)
- [ ] Show download status indicators (⬇ for downloading, ✓ for ready)
- [ ] Add album art/thumbnails display (if feasible in terminal)
- [ ] Improve status messages with colors and icons

### Playback Features
- [ ] Implement seeking (forward/backward with arrow keys)
- [ ] Add repeat mode (none, one, all)
- [ ] Add shuffle mode for queue
- [ ] Implement crossfade between tracks
- [ ] Add equalizer presets

### Playlist Management (Remaining)
- [ ] Save custom playlists to disk
- [ ] Load/create/edit custom playlists
- [ ] Export queue as playlist
- [ ] Add optional ytmusicapi integration for better My Mix support

## Low Priority

### Performance
- [ ] Optimize My Mix refresh to be non-blocking
- [ ] Cache search results to reduce API calls
- [ ] Implement lazy loading for large histories
- [ ] Reduce memory usage for downloaded files

### Quality of Life
- [ ] Add config file for user preferences
  - Default volume level
  - Default audio quality
  - Keybind customization
- [ ] Add lyrics display (if available)
- [ ] Add mini-player mode (compact view)
- [ ] Support for multiple audio formats (FLAC, AAC, etc.)

### Error Handling
- [ ] Better error messages for yt-dlp failures
- [ ] Retry logic for failed downloads
- [ ] Graceful handling of expired YouTube URLs
- [ ] Network connectivity checks

## Testing Needed

### Core Functionality
- [ ] Test history persistence across app restarts
- [ ] Test My Mix loading with different browser profiles
- [ ] Test queue pre-downloading with large queues
- [ ] Test view switching in all scenarios
- [ ] Test all new keybinds (h, m, Shift+m, ESC)

### Edge Cases
- [ ] Empty history on first launch
- [ ] No My Mix playlists found
- [ ] Network disconnection during playback
- [ ] Very long track titles (UI overflow)
- [ ] Special characters in track names
- [ ] Age-restricted content handling

### Performance Testing
- [ ] Large queue (100+ tracks) handling
- [ ] Long session (hours of playback)
- [ ] Memory usage over time
- [ ] CPU usage during background downloads

## Bug Fixes
- [ ] Fix any UI rendering issues in different terminal sizes
- [ ] Ensure proper cleanup of temp files on crash
- [ ] Handle terminal resize events
- [ ] Fix any race conditions in async code

## Documentation
- [ ] Update README with new features
- [ ] Add screenshots/demos
- [ ] Document keybinds in README
- [ ] Add installation guide for yt-dlp
- [ ] Create user guide for My Mix feature

## Future Ideas
- [ ] Integration with Last.fm for scrobbling
- [ ] Support for Spotify playlists (via yt-dlp)
- [ ] Radio mode (auto-play similar tracks)
- [ ] Collaborative queue (multiple users)
- [ ] Remote control via web interface
- [ ] Discord Rich Presence integration
- [ ] Podcasts support
- [ ] Offline mode (downloaded tracks only)
