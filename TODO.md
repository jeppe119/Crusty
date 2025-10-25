# TODO List - YouTube Music Player

## High Priority

### My Mix Implementation
- [ ] Implement real My Mix fetching using yt-dlp
  - Use `yt-dlp --cookies-from-browser firefox:<profile> --flat-playlist --dump-json "https://music.youtube.com"`
  - Parse YouTube Music auto-generated playlists
  - Extract: playlist ID, title, track count, URL
- [ ] Implement fetching tracks from selected mix playlist
  - Get individual video IDs from playlist
  - Add all tracks to queue with background downloading
- [ ] Test My Mix with real YouTube Music account
- [ ] Handle edge cases (no mixes found, network errors, etc.)

### History Management
- [ ] Add "Clear History" functionality (keybind or menu)
- [ ] Limit history size to prevent memory issues (e.g., keep last 100 tracks)
- [ ] Add history search/filter capability

### Queue Pre-downloading
- [ ] Implement cleanup of old pre-downloaded files
- [ ] Add download progress indicator for background downloads
- [ ] Handle download failures gracefully

## Medium Priority

### UI/UX Improvements
- [ ] Add visual progress bar for currently playing track
- [ ] Show download status indicators (⬇ for downloading, ✓ for ready)
- [ ] Add album art/thumbnails display (if feasible in terminal)
- [ ] Improve status messages with colors and icons
- [ ] Add help screen (press `?` to show all keybinds)

### Playback Features
- [ ] Implement seeking (forward/backward with arrow keys)
- [ ] Add repeat mode (none, one, all)
- [ ] Add shuffle mode for queue
- [ ] Implement crossfade between tracks
- [ ] Add equalizer presets

### Playlist Management
- [ ] Save custom playlists to disk
- [ ] Load/create/edit custom playlists
- [ ] Import playlists from YouTube URLs
- [ ] Export queue as playlist

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
