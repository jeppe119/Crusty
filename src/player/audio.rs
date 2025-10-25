// ==========================================
// AUDIO PLAYBACK ENGINE
// ==========================================
// This module manages playing audio streams using the rodio library.
// It handles:
// - Connecting to audio output devices (speakers/headphones)
// - Playing, pausing, stopping audio
// - Volume control
// - Tracking playback state and track information
//
// Key Concept: Rodio is a pure Rust audio playback library
// It provides a "Sink" abstraction for controlling audio playback

use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;
use std::time::{Duration, Instant};

// ==========================================
// PLAYER STATE ENUM
// ==========================================
// This enum represents the three possible states of the audio player.
//
// Why use an enum instead of multiple booleans?
// - Clearer: Can't be both Playing AND Stopped at the same time
// - Type-safe: Rust ensures you handle all cases
// - Memory efficient: Only stores one value
//
// Derives:
// - Debug: Can print the state for debugging (e.g., println!("{:?}", state))
// - Clone: Can make copies of the state
// - PartialEq: Can compare states (e.g., if state == PlayerState::Playing)
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerState {
    Stopped, // No audio loaded or playback has been stopped
    Playing, // Audio is currently playing
    Paused,  // Audio is loaded but temporarily paused
}

// ==========================================
// AUDIO PLAYER STRUCT
// ==========================================
// This struct holds all the state needed for audio playback.
//
// Fields explained:
//
// _stream: OutputStream
//   - The underscore prefix tells Rust "I know I'm not using this directly"
//   - BUT it MUST stay alive! If dropped, audio stops working
//   - Think of it as "keeping the speakers turned on"
//   - Technical: It maintains the connection to the OS audio system
//
// sink: Sink
//   - The actual audio player control interface
//   - Provides methods like play(), pause(), stop(), set_volume()
//   - Think of it as the "play/pause/volume buttons"
//   - Technical: It's a queue that decodes and plays audio
//
// state: PlayerState
//   - Tracks whether we're Playing, Paused, or Stopped
//   - We maintain this ourselves because rodio doesn't expose state directly
//   - Useful for UI to show play/pause button state
//
// volume: u32
//   - Current volume level from 0 (mute) to 100 (max)
//   - We store this because rodio uses 0.0-1.0 float range
//   - Easier for users to think in 0-100 percentage
//
// duration: f64
//   - Total length of the current track in seconds
//   - Example: 180.0 = 3 minutes
//   - Set when a track is loaded, used for progress bars
//
// current_title: String
//   - Name of the currently playing track
//   - Useful for displaying "Now Playing: ..."
//   - Empty string when no track is loaded
pub struct AudioPlayer {
    // Note: We don't store OutputStream to avoid async drop issues
    // The stream is leaked intentionally to keep audio working
    sink: Option<Sink>,
    state: PlayerState,
    volume: u32,
    duration: f64,
    current_title: String,
    start_time: Option<Instant>,
    pause_time: Option<Instant>,
    total_paused_duration: Duration,
}

// Implement custom Drop to handle cleanup properly
impl Drop for AudioPlayer {
    fn drop(&mut self) {
        // Stop playback before dropping
        if let Some(sink) = &self.sink {
            sink.stop();
        }
        self.sink = None;
    }
}

// ==========================================
// AUDIO PLAYER IMPLEMENTATION
// ==========================================
impl AudioPlayer {
    // ==========================================
    // CONSTRUCTOR: new()
    // ==========================================
    // Creates a new AudioPlayer with default settings.
    //
    // What happens here:
    // 1. OutputStream::try_default() - Ask the OS for the default audio device
    //    - Returns Result<(OutputStream, OutputStreamHandle), Error>
    //    - The OutputStream keeps the device open
    //    - The OutputStreamHandle is used to create Sinks
    //    - .unwrap() panics if no audio device found (simple error handling for now)
    //
    // 2. Sink::try_new(&stream_handle) - Create a new audio player
    //    - The Sink manages a queue of audio sources
    //    - Connected to the output device via stream_handle
    //    - .unwrap() panics if Sink creation fails
    //
    // 3. Return AudioPlayer with initial values:
    //    - State: Stopped (nothing playing yet)
    //    - Volume: 100 (max volume)
    //    - Duration: 0.0 (no track loaded)
    //    - Title: empty string (no track loaded)
    //
    // Why Self instead of AudioPlayer?
    // - Self is an alias for the type we're implementing (AudioPlayer)
    // - More flexible if you rename the struct later
    pub fn new() -> Self {
        // Try to get the output stream and handle
        // This might fail if there's no audio device available
        let sink = match OutputStream::try_default() {
            Ok((stream, handle)) => {
                // Successfully got audio device, create sink
                match Sink::try_new(&handle) {
                    Ok(sink) => {
                        // Leak the stream to prevent it from being dropped in async context
                        // This is intentional - we want the audio stream to live for the entire program
                        std::mem::forget(stream);
                        Some(sink)
                    }
                    Err(_) => None,
                }
            }
            Err(_) => {
                // No audio device available (e.g., headless server)
                // Continue without audio support
                None
            }
        };

        // Return the AudioPlayer with initial values
        AudioPlayer {
            sink,
            state: PlayerState::Stopped,
            volume: 100,
            duration: 0.0,
            current_title: String::new(),
            start_time: None,
            pause_time: None,
            total_paused_duration: Duration::from_secs(0),
        }
    }

    // ==========================================
    // PLAYBACK CONTROL: play()
    // ==========================================
    // Plays audio from a URL.
    //
    // This downloads the audio stream and plays it through rodio.
    //
    // Parameters:
    // - url: Direct audio stream URL (from yt-dlp)
    // - title: Track title for display
    //
    // Note: This is synchronous and will block briefly while downloading
    // In a real app, you'd want to do this asynchronously or in a background thread
    pub fn play(&mut self, url: &str, title: &str) {
        // Only try to play if we have a sink (audio device available)
        if let Some(sink) = &self.sink {
            // First, stop any currently playing audio
            sink.stop();

            // Try to download and play the audio
            // We'll use a blocking HTTP request (could be improved with async)
            eprintln!("Attempting to download and play: {}", url);

            // Wrap the entire operation in a catch_unwind to prevent panics
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Self::download_and_decode(url)
            }));

            match result {
                Ok(Ok((decoder, duration))) => {
                    // Successfully got the audio!
                    eprintln!("Successfully decoded audio, appending to sink");

                    // Try to append to sink - this can also panic
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        sink.append(decoder);
                    })) {
                        Ok(_) => {
                            // Update our state
                            self.state = PlayerState::Playing;
                            self.current_title = title.to_string();
                            self.duration = duration;
                            self.start_time = Some(Instant::now());
                            self.pause_time = None;
                            self.total_paused_duration = Duration::from_secs(0);
                            eprintln!("Now playing: {}", title);
                        }
                        Err(panic_err) => {
                            eprintln!("Panic while appending to sink: {:?}", panic_err);
                            self.state = PlayerState::Stopped;
                        }
                    }
                }
                Ok(Err(e)) => {
                    // Failed to download/decode
                    eprintln!("Failed to play audio: {}", e);
                    self.state = PlayerState::Stopped;
                }
                Err(panic_err) => {
                    eprintln!("Panic during download/decode: {:?}", panic_err);
                    self.state = PlayerState::Stopped;
                }
            }
        } else {
            eprintln!("No audio device available");
        }
    }

    // Helper function to download and decode audio
    // Returns the decoder and duration
    fn download_and_decode(url: &str) -> Result<(Decoder<Cursor<Vec<u8>>>, f64), Box<dyn std::error::Error>> {
        eprintln!("Starting download from: {}", url);

        // Wrap everything in a catch to prevent panics
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<_, Box<dyn std::error::Error>> {
            // Download the audio data with proper timeout and headers
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
                .build()
                .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

            eprintln!("Sending HTTP GET request...");
            let response = client.get(url)
                .send()
                .map_err(|e| format!("HTTP request failed: {}", e))?;

            eprintln!("Got HTTP response, status: {:?}", response.status());

            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()).into());
            }

            eprintln!("Reading response bytes...");
            let bytes = response.bytes()
                .map_err(|e| format!("Failed to read response bytes: {}", e))?
                .to_vec();

            eprintln!("Downloaded {} bytes", bytes.len());

            if bytes.is_empty() {
                return Err("Downloaded audio is empty".into());
            }

            // Verify we have actual audio data (check for common audio headers)
            if bytes.len() < 100 {
                return Err(format!("Downloaded data too small ({} bytes), likely not valid audio", bytes.len()).into());
            }

            // Create a cursor (in-memory reader) from the bytes
            eprintln!("Creating cursor from bytes...");
            let cursor = Cursor::new(bytes.clone());

            // Decode the audio format (MP3, WAV, etc.)
            eprintln!("Attempting to decode audio...");
            let decoder = Decoder::new(cursor)
                .map_err(|e| {
                    // Log first few bytes for debugging
                    let preview = if bytes.len() > 20 {
                        format!("{:?}...", &bytes[..20])
                    } else {
                        format!("{:?}", bytes)
                    };
                    eprintln!("Audio data preview: {}", preview);
                    format!("Audio decode failed: {}. This might not be a valid audio file.", e)
                })?;

            eprintln!("Audio decoded successfully");

            // Try to calculate duration
            let duration = 0.0;

            Ok((decoder, duration))
        }));

        match result {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(e)) => Err(e),
            Err(panic_info) => {
                eprintln!("Panic caught in download_and_decode: {:?}", panic_info);
                Err(format!("Panic during download/decode: {:?}", panic_info).into())
            }
        }
    }

    // ==========================================
    // PLAYBACK CONTROL: pause()
    // ==========================================
    // Pauses the currently playing audio.
    //
    // What happens:
    // 1. self.sink.pause() - Tell rodio to pause audio output
    //    - Audio remains loaded in memory
    //    - Can be resumed later from the same position
    //    - Does not clear the sink's queue
    //
    // 2. self.state = PlayerState::Paused - Update our internal state
    //    - Important: rodio doesn't track state for us
    //    - We need this for get_state() to return correct value
    //    - Used by UI to show pause button vs play button
    //
    // Note: Calling pause() when already paused is safe (no-op)
    pub fn pause(&mut self) {
        // Tell the sink to pause audio output
        if let Some(sink) = &self.sink {
            sink.pause();
        }

        // Track when we paused for accurate position tracking
        if self.state == PlayerState::Playing {
            self.pause_time = Some(Instant::now());
        }

        // Update our internal state tracking
        self.state = PlayerState::Paused;
    }

    // ==========================================
    // PLAYBACK CONTROL: resume()
    // ==========================================
    // Resumes playback after being paused.
    //
    // What happens:
    // 1. self.sink.play() - Tell rodio to resume audio output
    //    - Continues from where it was paused
    //    - Does not restart from beginning
    //    - Note: method is called "play" but it resumes when paused
    //
    // 2. self.state = PlayerState::Playing - Update our state
    //    - Tracks that we're now actively playing
    //    - Used by UI to show correct button state
    //
    // Note: Calling resume() when already playing is safe (no-op)
    pub fn resume(&mut self) {
        // Tell the sink to resume audio output
        if let Some(sink) = &self.sink {
            sink.play();
        }

        // Update total paused duration
        if let Some(pause_time) = self.pause_time {
            self.total_paused_duration += Instant::now().duration_since(pause_time);
            self.pause_time = None;
        }

        // Update our internal state tracking
        self.state = PlayerState::Playing;
    }

    // ==========================================
    // PLAYBACK CONTROL: toggle_pause()
    // ==========================================
    // Toggles between playing and paused states.
    //
    // This is a convenience method that:
    // - Pauses if currently playing
    // - Resumes if currently paused
    // - Does nothing if stopped (no audio loaded)
    //
    // Why is this useful?
    // - Common in music players: one button for play/pause
    // - Keyboard shortcut (spacebar) typically toggles
    // - Simpler for UI code: just call toggle instead of checking state
    //
    // Implementation note:
    // - We check self.state to decide what to do
    // - We call our own pause() and resume() methods
    // - This reuses logic and ensures state is updated correctly
    pub fn toggle_pause(&mut self) {
        // Check current state and do the opposite
        if self.state == PlayerState::Playing {
            // Currently playing, so pause
            self.pause();
        } else if self.state == PlayerState::Paused {
            // Currently paused, so resume
            self.resume();
        }
        // If stopped, do nothing (can't resume from stopped state)
    }

    // ==========================================
    // PLAYBACK CONTROL: stop()
    // ==========================================
    // Stops playback and clears the audio queue.
    //
    // What happens:
    // 1. self.sink.stop() - Tell rodio to stop and clear
    //    - Immediately stops audio output
    //    - Clears all queued audio
    //    - Cannot be resumed (unlike pause)
    //    - To play again, must call play() with a new source
    //
    // 2. self.state = PlayerState::Stopped - Update our state
    //    - Indicates no audio is loaded
    //    - UI should show "play" button, not "resume"
    //
    // Difference between stop() and pause():
    // - pause(): Audio stays loaded, can resume
    // - stop(): Audio is cleared, must reload to play again
    pub fn stop(&mut self) {
        // Tell the sink to stop and clear audio
        if let Some(sink) = &self.sink {
            sink.stop();
        }

        // Reset timing information
        self.start_time = None;
        self.pause_time = None;
        self.total_paused_duration = Duration::from_secs(0);

        // Update our internal state tracking
        self.state = PlayerState::Stopped;
    }

    // ==========================================
    // SEEKING: seek()
    // ==========================================
    // Seeks to a specific position in the track.
    //
    // Parameters:
    // - seconds: Position to seek to (e.g., 30.0 = 30 seconds into track)
    //
    // Currently unimplemented because:
    // - Rodio's Sink doesn't directly support seeking
    // - Would require:
    //   1. Stopping current playback
    //   2. Reloading audio from the seeked position
    //   3. Resuming playback
    // - For streaming (YouTube), this is complex
    //
    // Alternative approach:
    // - Use a different audio library (symphonia has better seek support)
    // - Or implement buffering and seek within buffer
    //
    // TODO: Decide on seeking strategy
    pub fn seek(&mut self, _seconds: f64) {
        // Underscore prefix on _seconds means "intentionally unused parameter"
        // Prevents compiler warning about unused variable
        unimplemented!("seek() requires more advanced audio control than Sink provides")
    }

    // ==========================================
    // SEEKING: seek_relative()
    // ==========================================
    // Seeks relative to current position.
    //
    // Parameters:
    // - seconds: How many seconds to skip forward/backward
    //   - Positive: Skip forward (e.g., +10.0 = skip ahead 10 seconds)
    //   - Negative: Skip backward (e.g., -10.0 = go back 10 seconds)
    //
    // Currently unimplemented for same reasons as seek().
    //
    // When implemented, would work like:
    // 1. Get current position: let pos = self.get_time_pos()
    // 2. Calculate new position: let new_pos = pos + seconds
    // 3. Clamp to valid range: 0.0 to self.duration
    // 4. Seek to new_pos
    //
    // TODO: Implement when seek() is working
    pub fn seek_relative(&mut self, _seconds: f64) {
        unimplemented!("seek_relative() depends on seek() being implemented")
    }

    // ==========================================
    // VOLUME CONTROL: get_volume()
    // ==========================================
    // Returns the current volume level.
    //
    // Returns: u32 from 0 (mute) to 100 (max)
    //
    // Why return self.volume instead of asking the sink?
    // - The sink stores volume as f32 (0.0 to 1.0)
    // - We store as u32 (0 to 100) for user convenience
    // - This avoids float rounding issues
    // - Guarantees we return exactly what was set
    //
    // Example:
    // - User sets volume to 75
    // - We store 75 as u32
    // - We convert to 0.75 for sink
    // - get_volume() returns 75 (not 74.999... from float conversion)
    pub fn get_volume(&self) -> u32 {
        // Simply return our stored volume value
        self.volume
    }

    // ==========================================
    // VOLUME CONTROL: set_volume()
    // ==========================================
    // Sets the volume level.
    //
    // Parameters:
    // - volume: New volume from 0 (mute) to 100 (max)
    //
    // What happens:
    // 1. Store the volume in self.volume
    //    - Keep track of what user set
    //    - Used by get_volume() to return exact value
    //
    // 2. Convert from 0-100 to 0.0-1.0
    //    - Rodio expects float range
    //    - volume as f32: Converts u32 to f32 (e.g., 75 → 75.0)
    //    - / 100.0: Divides to get 0.0-1.0 (e.g., 75.0 / 100.0 → 0.75)
    //
    // 3. Tell the sink to change volume
    //    - self.sink.set_volume() applies to currently playing audio
    //    - Takes effect immediately (no restart needed)
    //
    // Note: No validation that volume is 0-100
    // - Could add: if volume > 100 { volume = 100; }
    // - For now, trust the caller to provide valid values
    pub fn set_volume(&mut self, volume: u32) {
        // 1. Store the volume for get_volume()
        self.volume = volume;

        // 2. Convert volume from 0-100 range to 0.0-1.0 range
        //    Example: 75 becomes 75.0, then 75.0/100.0 = 0.75
        let rodio_volume = volume as f32 / 100.0;

        // 3. Tell the sink to apply the new volume
        if let Some(sink) = &self.sink {
            sink.set_volume(rodio_volume);
        }
    }

    // ==========================================
    // PLAYBACK INFO: get_time_pos()
    // ==========================================
    // Gets the current playback position in seconds.
    //
    // Returns: f64 representing seconds elapsed
    // Example: 45.5 = 45 and a half seconds into the track
    //
    // Currently unimplemented because:
    // - Rodio's Sink doesn't expose current position
    // - Would need to track manually:
    //   1. Store start time when playback begins
    //   2. Calculate: current_time - start_time
    //   3. Account for pauses (don't count paused time)
    //   4. Reset when track changes
    //
    // Alternative approaches:
    // - Use std::time::Instant to track elapsed time
    // - Update in a background task/thread
    // - Or use a different audio library with position tracking
    //
    // TODO: Implement time tracking system
    pub fn get_time_pos(&self) -> f64 {
        // Calculate elapsed time based on start_time and paused duration
        if let Some(start) = self.start_time {
            let elapsed = Instant::now().duration_since(start);

            // If currently paused, use pause_time instead of now
            let elapsed = if let Some(pause_time) = self.pause_time {
                pause_time.duration_since(start)
            } else {
                elapsed
            };

            // Subtract total paused duration to get actual playback time
            let playback_time = elapsed.saturating_sub(self.total_paused_duration);
            playback_time.as_secs_f64()
        } else {
            0.0
        }
    }

    // ==========================================
    // PLAYBACK INFO: get_duration()
    // ==========================================
    // Gets the total duration of the current track.
    //
    // Returns: f64 representing total length in seconds
    // Example: 180.0 = 3 minutes
    //
    // How this works:
    // - self.duration is set when play() loads a track
    // - For now it's always 0.0 since play() is unimplemented
    // - When play() is implemented, it will:
    //   1. Decode audio to get sample rate and total samples
    //   2. Calculate: duration = total_samples / sample_rate
    //   3. Store in self.duration
    //
    // Used for:
    // - Progress bars (current_pos / duration = percentage)
    // - Displaying track length (3:45, 2:30, etc.)
    // - Seeking validation (can't seek past duration)
    pub fn get_duration(&self) -> f64 {
        // Simply return the stored duration
        // Will be 0.0 until play() is implemented
        self.duration
    }

    // ==========================================
    // PLAYBACK INFO: get_state()
    // ==========================================
    // Gets the current player state.
    //
    // Returns: PlayerState (Stopped, Playing, or Paused)
    //
    // Why clone()?
    // - self.state is owned by the AudioPlayer
    // - We only borrow &self (not &mut self)
    // - Can't move self.state out (would leave field uninitialized!)
    // - So we clone it (make a copy)
    //
    // Is cloning expensive?
    // - No! PlayerState is just an enum tag (1 byte)
    // - Could add Copy trait to make it auto-copy
    // - But Clone is fine and more explicit
    //
    // Used by:
    // - UI to show play/pause button
    // - Logic to decide if toggle should pause or resume
    // - Debugging to check player state
    pub fn get_state(&self) -> PlayerState {
        // Clone and return the current state
        self.state.clone()
    }

    // ==========================================
    // PLAYBACK INFO: is_finished()
    // ==========================================
    // Checks if the current track has finished playing.
    //
    // Returns: bool
    // - true: Track finished (sink is empty)
    // - false: Track still playing or paused
    //
    // This is useful for auto-advancing to next track
    pub fn is_finished(&self) -> bool {
        if let Some(sink) = &self.sink {
            // If sink is empty and we're in playing state, track finished
            sink.empty()
        } else {
            false
        }
    }
}

// ==========================================
// FUTURE IMPROVEMENTS
// ==========================================
// Things to add later:
//
// 1. Playlist support
//    - Queue multiple tracks
//    - Auto-advance to next track
//    - Shuffle and repeat modes
//
// 2. Equalizer
//    - Rodio doesn't have built-in EQ
//    - Would need audio processing library
//
// 3. Gapless playback
//    - Preload next track while current plays
//    - Seamless transition between tracks
//
// 4. Audio visualization
//    - FFT for frequency analysis
//    - Export audio samples for visualization
//
// 5. Better error handling
//    - Return Result<(), Error> instead of unwrap()
//    - Custom error types for different failures
//    - Graceful handling of audio device issues
//
// 6. Cross-platform audio devices
//    - List available devices
//    - Let user choose output device
//    - Handle device disconnection/reconnection
