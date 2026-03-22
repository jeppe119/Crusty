// ==========================================
// QUEUE MANAGEMENT MODULE
// ==========================================
// This module manages the playback queue for the music player.
// It handles:
// - Adding tracks to the queue
// - Moving between tracks (next/previous)
// - Maintaining playback history
// - Tracking the currently playing track
//
// Key Concept: VecDeque (pronounced "vec-deck")
// - VecDeque = Vector Double-Ended Queue
// - Efficient at adding/removing from BOTH front and back
// - Perfect for a music queue where we:
//   * Add tracks to the back (queue them up)
//   * Remove tracks from the front (play next)
//   * Sometimes add to front (going back to previous)

use std::collections::VecDeque;

// ==========================================
// TRACK STRUCT
// ==========================================
// Represents a single music track with all its metadata.
//
// Fields explained:
//
// video_id: String
//   - Unique identifier for the YouTube video
//   - Example: "dQw4w9WgXcQ" (from youtube.com/watch?v=dQw4w9WgXcQ)
//   - Used to fetch the video again if needed
//   - Guaranteed unique across all YouTube videos
//
// title: String
//   - The name/title of the video/song
//   - Example: "Rick Astley - Never Gonna Give You Up"
//   - Displayed in the UI
//   - What users see when browsing queue
//
// duration: u64
//   - Length of the track in seconds
//   - u64 = unsigned 64-bit integer (no negative numbers)
//   - Example: 212 = 3 minutes 32 seconds
//   - Used for progress bars and time display
//   - Why u64? Most songs are under 10 minutes, but livestreams can be hours
//
// uploader: String
//   - The channel/user who uploaded the video
//   - Example: "RickAstleyVEVO"
//   - Useful for displaying "Artist" in UI
//   - Helps users identify official vs. cover versions
//
// url: String
//   - The direct audio stream URL
//   - Example: "https://rr3---sn-h5576nez.googlevideo.com/videoplayback?..."
//   - This is what the audio player uses to stream
//   - Extracted by yt-dlp from the YouTube page
//   - Note: These URLs expire after a few hours!
//
// Derives:
// - Debug: Can print track info for debugging (println!("{:?}", track))
// - Clone: Can make copies of tracks (needed for queue operations)
// - Serialize/Deserialize: For saving/loading history to JSON
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub duration: u64,
    pub uploader: String,
    pub url: String,
    pub local_file: Option<String>, // Path to pre-downloaded file
}

// ==========================================
// TRACK IMPLEMENTATION
// ==========================================
impl Track {
    // ==========================================
    // CONSTRUCTOR: new()
    // ==========================================
    // Creates a new Track with all required fields.
    //
    // Why take String instead of &str?
    // - The Track will own these strings (they're not borrowed)
    // - Tracks can live independently and be moved around
    // - Example: Track can be cloned and moved to history
    //
    // Parameters are moved (not copied):
    // - When you pass a String to this function, ownership transfers
    // - The function "consumes" the parameters
    // - Caller can't use them afterward
    //
    // Example usage:
    // ```
    // let track = Track::new(
    //     String::from("dQw4w9WgXcQ"),
    //     String::from("Never Gonna Give You Up"),
    //     212,
    //     String::from("RickAstleyVEVO"),
    //     String::from("https://..."),
    // );
    // ```
    pub fn new(
        video_id: String,
        title: String,
        duration: u64,
        uploader: String,
        url: String,
    ) -> Self {
        // Simply construct and return the Track struct
        // No validation or processing needed (could add later)
        Track {
            video_id,
            title,
            duration,
            uploader,
            url,
            local_file: None, // Not pre-downloaded yet
        }
    }
}

// ==========================================
// QUEUE STRUCT
// ==========================================
// Manages the playback queue and track navigation.
//
// Fields explained:
//
// tracks: VecDeque<Track>
//   - The queue of tracks waiting to be played
//   - VecDeque allows efficient operations at both ends:
//     * push_back(): Add new tracks to queue
//     * pop_front(): Get next track to play
//     * push_front(): Add track back if going to previous
//   - Order: Front of queue = plays next, Back of queue = plays last
//   - Think of it like a line at a concert: front gets in first
//
// current_track: Option<Track>
//   - The track that is currently playing (or None if stopped)
//   - Option<T> is Rust's way of representing "maybe has a value"
//     * Some(track): A track is loaded/playing
//     * None: No track is playing
//   - Why Option instead of nullable?
//     * Rust has no null! Option is the safe alternative
//     * Forces you to handle the "no track" case explicitly
//     * Prevents null pointer crashes
//   - Clone is stored here so history can keep the original
//
// history: Vec<Track>
//   - Previously played tracks in order
//   - Vec (regular vector) is fine here because we only add/remove from end
//   - Used for "previous" button functionality
//   - Grows as songs are played (could limit size in production)
//   - Order: [oldest ... newest]
//   - Example: If you played A, B, C, history = [A, B, C]
//
// How the three fields work together:
// - User adds songs → go into `tracks` queue
// - User presses "next" → pop from `tracks`, current goes to `history`
// - User presses "previous" → pop from `history`, current goes back to `tracks`
pub struct Queue {
    tracks: VecDeque<Track>,
    current_track: Option<Track>,
    history: Vec<Track>,
}

// ==========================================
// QUEUE IMPLEMENTATION
// ==========================================
impl Queue {
    // ==========================================
    // CONSTRUCTOR: new()
    // ==========================================
    // Creates a new empty Queue.
    //
    // Initial state:
    // - tracks: Empty VecDeque (no songs queued)
    // - current_track: None (nothing playing)
    // - history: Empty Vec (no songs played yet)
    //
    // This is the starting point for a new music player session.
    pub fn new() -> Self {
        Queue {
            tracks: VecDeque::new(),
            current_track: None,
            history: Vec::new(),
        }
    }

    // ==========================================
    // ADDING TRACKS: add()
    // ==========================================
    // Adds a single track to the back of the queue.
    //
    // Parameters:
    // - track: The track to add (ownership is transferred)
    //
    // What happens:
    // - Track is added to the END of the queue
    // - If queue was empty, this becomes the first track
    // - If queue had tracks, this plays after all existing tracks
    //
    // Example:
    // - Queue: [A, B]
    // - add(C)
    // - Queue: [A, B, C]
    // - Playing order: A → B → C
    //
    // Why &mut self?
    // - We're modifying the queue (adding to it)
    // - Requires mutable borrow
    // - Caller must have mut access to the Queue
    pub fn add(&mut self, track: Track) {
        self.tracks.push_back(track);
    }

    // ==========================================
    // NAVIGATION: next()
    // ==========================================
    // Moves to the next track in the queue.
    //
    // Returns: Option<Track>
    // - Some(track): Successfully got next track
    // - None: Queue is empty, no tracks available
    //
    // What happens (step by step):
    //
    // 1. Save current track to history
    //    - if let Some(track) = self.current_track.take()
    //    - .take() is key! It:
    //      * Removes the value from current_track (sets it to None)
    //      * Returns the removed value
    //      * Allows us to move the track without cloning
    //    - Push to history so "previous" can go back to it
    //
    // 2. Get next track from queue
    //    - self.tracks.pop_front() removes first track from queue
    //    - Returns Option<Track>: Some(track) or None if empty
    //
    // 3. If we got a track:
    //    - Clone it (we need two copies: one to return, one to store)
    //    - Store clone in self.current_track
    //    - Return the original
    //
    // 4. If queue was empty:
    //    - Set current_track to None (nothing playing)
    //    - Return None (tell caller there's no track)
    //
    // Example flow:
    // - Current: A, Queue: [B, C], History: []
    // - next() called
    // - Current: B, Queue: [C], History: [A]
    // - Returns: Some(B)
    //
    // Edge cases:
    // - First track ever: current_track is None, so nothing goes to history
    // - Last track: queue empty after pop, returns None
    pub fn next(&mut self) -> Option<Track> {
        // Step 1: Save current track to history (if there is one)
        // .take() moves the value out of current_track, leaving None behind
        if let Some(track) = self.current_track.take() {
            self.history.push(track);
        }

        // Step 2: Try to get the next track from the queue
        if let Some(track) = self.tracks.pop_front() {
            // Step 3: We got a track! Store it as current and return it
            // Clone because we need to both store and return it
            self.current_track = Some(track.clone());
            return Some(track);
        }

        // Step 4: Queue was empty, no next track available
        self.current_track = None;
        None
    }

    // ==========================================
    // NAVIGATION: previous()
    // ==========================================
    // Goes back to the previously played track.
    //
    // Returns: Option<Track>
    // - Some(track): Successfully went back to previous track
    // - None: No history available (this is the first track)
    //
    // What happens (step by step):
    //
    // 1. Save current track back to front of queue
    //    - if let Some(current) = self.current_track.take()
    //    - .take() removes it from current_track
    //    - push_front() adds it to the FRONT of queue (not back!)
    //    - Why front? So next() will play it again
    //    - This is like "undoing" the next() operation
    //
    // 2. Get previous track from history
    //    - self.history.pop() removes the LAST item from history
    //    - Last item = most recently played track
    //    - Returns Option<Track>: Some(track) or None if history empty
    //
    // 3. If we got a track from history:
    //    - Clone it (need to both return and store)
    //    - Store clone as current_track
    //    - Return the original
    //
    // 4. If history was empty:
    //    - Set current_track to None
    //    - Return None (no previous track available)
    //
    // Example flow:
    // - Current: C, Queue: [D], History: [A, B]
    // - previous() called
    // - Current: B, Queue: [C, D], History: [A]
    // - Returns: Some(B)
    //
    // Interplay with next():
    // - A → next() → B → previous() → A
    // - After next(): Current=B, Queue=[C], History=[A]
    // - After previous(): Current=A, Queue=[B, C], History=[]
    // - B was saved back to queue, so next() would play B again!
    pub fn previous(&mut self) -> Option<Track> {
        // Step 1: Save current track back to front of queue (if there is one)
        // This ensures we can go forward again with next()
        if let Some(current) = self.current_track.take() {
            self.tracks.push_front(current);
        }

        // Step 2: Try to get the previous track from history
        // .pop() removes from the end of Vec (most recent history)
        if let Some(prev_track) = self.history.pop() {
            // Step 3: We got a previous track! Store it as current and return it
            self.current_track = Some(prev_track.clone());
            return Some(prev_track);
        }

        // Step 4: No history available (we're at the beginning)
        self.current_track = None;
        None
    }

    // ==========================================
    // QUEUE MANAGEMENT: remove_at()
    // ==========================================
    // Removes a specific track from the queue by index and returns it.
    //
    // Parameters:
    // - index: The position in the queue (0 = next to play, 1 = after that, etc.)
    //
    // Returns: Option<Track>
    // - Some(track): Track that was removed
    // - None: Index was out of bounds (no track at that position)
    //
    // This is similar to remove() but returns the removed track
    // instead of just a boolean. Useful when you need to show
    // what was removed or add it somewhere else.
    pub fn remove_at(&mut self, index: usize) -> Option<Track> {
        if index < self.tracks.len() {
            self.tracks.remove(index)
        } else {
            None
        }
    }

    // ==========================================
    // QUEUE INSPECTION: get_queue_list()
    // ==========================================
    // Gets all tracks in the queue as a Vec.
    //
    // Returns: Vec<Track>
    // - A vector containing clones of all queued tracks
    // - Order preserved (first element = next to play)
    //
    // Why clone?
    // - Queue owns the original tracks
    // - Caller gets independent copies
    // - Caller can modify returned Vec without affecting queue
    //
    // Why return Vec instead of &VecDeque?
    // - Vec is more common/familiar in Rust
    // - UI code expects Vec
    // - .into() automatically converts VecDeque to Vec
    //
    // Performance note:
    // - Cloning all tracks could be expensive for large queues
    // - Alternative: Could return references: Vec<&Track>
    // - But cloning is safer and easier for beginners
    //
    // Use case:
    // - UI needs to display all queued tracks
    // - Showing "Up Next" list to user
    //
    // Example:
    // - Queue internally: VecDeque([A, B, C])
    // - Returns: Vec<Track>([A clone, B clone, C clone])
    pub fn get_queue_list(&self) -> Vec<Track> {
        // Clone the VecDeque and convert to Vec
        // .clone() creates a copy of all tracks
        // .into() converts VecDeque<Track> to Vec<Track>
        self.tracks.clone().into()
    }

    // ==========================================
    // QUEUE INSPECTION: get_queue_slice()
    // ==========================================
    // Gets a slice of tracks from the queue without cloning.
    //
    // Parameters:
    // - start: Starting index
    // - count: Number of tracks to return
    //
    // Returns: Vec<&Track>
    // - References to tracks in the specified range
    // - Much faster than get_queue_list() for large queues
    //
    // Use case:
    // - Display only visible portion of queue in UI
    // - Avoid cloning hundreds of tracks when only showing 10
    pub fn get_queue_slice(&self, start: usize, count: usize) -> Vec<&Track> {
        self.tracks.iter().skip(start).take(count).collect()
    }

    /// Returns the number of tracks in the queue (excluding current and history).
    #[must_use]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    // ==========================================
    // PLAYBACK: start_or_next()
    // ==========================================
    // Smart playback method that:
    // - If nothing playing, starts first track (doesn't skip it)
    // - If something playing, goes to next track
    //
    // This prevents the "first press skips first song" bug
    pub fn start_or_next(&mut self) -> Option<Track> {
        if self.current_track.is_none() && !self.tracks.is_empty() {
            // Nothing playing - start first track without history
            let track = self.tracks.pop_front()?;
            self.current_track = Some(track.clone());
            Some(track)
        } else {
            // Already playing - use normal next logic
            self.next()
        }
    }

    // ==========================================
    // QUEUE INSPECTION: is_empty()
    // ==========================================
    // Checks if the queue is empty.
    //
    // Returns: bool
    // - true: No tracks in queue (current_track might still exist though!)
    // - false: At least one track waiting to be played
    //
    // Important distinction:
    // - is_empty() = true means queue is empty
    // - Does NOT mean nothing is playing!
    // - current_track could still have a track
    //
    // Use cases:
    // - Check if there's a "next" track before calling next()
    // - Display "Queue is empty" message in UI
    // - Decide whether to auto-play next track
    //
    // Example:
    // - Queue: [], Current: A, History: [B]
    // - is_empty() returns true (queue empty even though A is playing)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    // ==========================================
    // QUEUE INSPECTION: get_current()
    // ==========================================
    // Gets the currently playing track.
    //
    // Returns: Option<&Track>
    // - Some(&track): Reference to the current track
    // - None: No track is currently playing
    //
    // Why return a reference?
    // - Caller just needs to read track info (title, duration, etc.)
    // - No need to clone the entire track
    // - More efficient
    //
    // Why Option?
    // - current_track is Option<Track>
    // - Could be None if:
    //   * Player just started (no track loaded)
    //   * Reached end of queue
    //   * User stopped playback
    //
    // .as_ref() explained:
    // - Converts Option<Track> to Option<&Track>
    // - Does NOT move or clone the track
    // - Just creates a reference to it
    // - If current_track is Some(track), returns Some(&track)
    // - If current_track is None, returns None
    //
    // Use cases:
    // - Display "Now Playing: Song Name"
    // - Get track info for UI
    // - Check if anything is playing
    //
    // Example:
    // - Current: Some(Track { title: "Song", ... })
    // - get_current() returns Some(&Track { title: "Song", ... })
    //
    // - Current: None
    // - get_current() returns None
    #[must_use]
    pub fn get_current(&self) -> Option<&Track> {
        self.current_track.as_ref()
    }

    // ==========================================
    // QUEUE INSPECTION: get_history()
    // ==========================================
    // Gets the history of played tracks.
    //
    // Returns: &Vec<Track>
    // - Reference to the vector of previously played tracks
    // - Order: [oldest ... newest]
    // - Empty if no tracks have been played yet
    //
    // Use case:
    // - Display "Recently Played" list in UI
    // - Show what tracks were played in this session
    #[must_use]
    pub fn get_history(&self) -> &[Track] {
        &self.history
    }

    // ==========================================
    // QUEUE MANAGEMENT: add_to_history()
    // ==========================================
    // Adds a track directly to history (used when loading from persistence).
    //
    // Parameters:
    // - track: The track to add to history
    //
    // This bypasses normal playback and directly adds to history.
    // Useful for restoring history from saved state.
    pub fn add_to_history(&mut self, track: Track) {
        self.history.push(track);
    }

    // ==========================================
    // QUEUE MANAGEMENT: clear_history()
    // ==========================================
    // Clears all tracks from the history.
    //
    // What happens:
    // - Removes all previously played tracks from history
    // - Does NOT affect current track or queue
    // - After this, history is empty
    //
    // Use case:
    // - User wants to clear their listening history
    // - Privacy concerns
    // - Fresh start
    //
    // Example:
    // - Before: Queue=[A, B], Current=C, History=[D, E, F]
    // - clear_history()
    // - After: Queue=[A, B], Current=C, History=[]
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    // ==========================================
    // QUEUE MANAGEMENT: limit_history()
    // ==========================================
    // Limits history to a maximum size, removing oldest entries.
    //
    // Parameters:
    // - max_size: Maximum number of tracks to keep in history
    //
    // What happens:
    // - If history exceeds max_size, removes oldest tracks
    // - Keeps only the most recent max_size tracks
    // - Does nothing if history is already smaller than max_size
    //
    // Use case:
    // - Prevent memory issues from unlimited history growth
    // - Keep only recent listening history
    //
    // Example:
    // - History: [A, B, C, D, E], max_size=3
    // - limit_history(3)
    // - History: [C, D, E] (kept most recent 3)
    pub fn limit_history(&mut self, max_size: usize) {
        if self.history.len() > max_size {
            let excess = self.history.len() - max_size;
            self.history.drain(0..excess);
        }
    }

    // ==========================================
    // QUEUE MANAGEMENT: restore_queue()
    // ==========================================
    // Restores queue state from persistence.
    //
    // Parameters:
    // - tracks: Vec<Track> - The tracks to restore to the queue
    // - current_track: Option<Track> - The track that was playing (if any)
    //
    // What happens:
    // - Clears current queue
    // - Adds all tracks from persistence
    // - Restores current track (if provided)
    //
    // Use case:
    // - Loading saved queue on app startup
    // - Restoring session after crash
    pub fn restore_queue(&mut self, tracks: Vec<Track>, current_track: Option<Track>) {
        self.tracks.clear();
        for track in tracks {
            self.tracks.push_back(track);
        }
        self.current_track = current_track;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(id: &str) -> Track {
        Track::new(
            id.to_string(),
            format!("Track {id}"),
            120,
            "Uploader".to_string(),
            format!("https://www.youtube.com/watch?v={id}"),
        )
    }

    #[test]
    fn test_new_queue_is_empty() {
        let queue = Queue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.get_current().is_none());
        assert!(queue.get_history().is_empty());
    }

    #[test]
    fn test_add_increases_length() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());
        queue.add(make_track("b"));
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_next_moves_track_to_current() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));

        let track = queue.next();
        assert!(track.is_some());
        assert_eq!(track.unwrap().video_id, "a");
        assert_eq!(queue.get_current().unwrap().video_id, "a");
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_next_saves_current_to_history() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));

        queue.next(); // current = a
        queue.next(); // current = b, history = [a]

        assert_eq!(queue.get_current().unwrap().video_id, "b");
        assert_eq!(queue.get_history().len(), 1);
        assert_eq!(queue.get_history()[0].video_id, "a");
    }

    #[test]
    fn test_next_returns_none_when_empty() {
        let mut queue = Queue::new();
        assert!(queue.next().is_none());
        assert!(queue.get_current().is_none());
    }

    #[test]
    fn test_previous_goes_back() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));
        queue.add(make_track("c"));

        queue.next(); // current = a
        queue.next(); // current = b, history = [a]

        let prev = queue.previous();
        assert!(prev.is_some());
        assert_eq!(prev.unwrap().video_id, "a");
        assert_eq!(queue.get_current().unwrap().video_id, "a");
        // b should be pushed back to front of queue
        assert_eq!(queue.len(), 2); // [b, c]
    }

    #[test]
    fn test_previous_returns_none_without_history() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.next(); // current = a, history = []

        assert!(queue.previous().is_none());
    }

    #[test]
    fn test_remove_at_valid_index() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));
        queue.add(make_track("c"));

        let removed = queue.remove_at(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().video_id, "b");
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_remove_at_invalid_index() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        assert!(queue.remove_at(5).is_none());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_get_queue_list_clones() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));

        let list = queue.get_queue_list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].video_id, "a");
        assert_eq!(list[1].video_id, "b");
    }

    #[test]
    fn test_get_queue_slice() {
        let mut queue = Queue::new();
        for i in 0..10 {
            queue.add(make_track(&i.to_string()));
        }

        let slice = queue.get_queue_slice(2, 3);
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0].video_id, "2");
        assert_eq!(slice[2].video_id, "4");
    }

    #[test]
    fn test_get_queue_slice_beyond_bounds() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));

        let slice = queue.get_queue_slice(0, 100);
        assert_eq!(slice.len(), 1);
    }

    #[test]
    fn test_start_or_next_starts_first() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));

        // First call: should start first track (not skip it)
        let track = queue.start_or_next();
        assert_eq!(track.unwrap().video_id, "a");
        assert!(queue.get_history().is_empty()); // No history yet

        // Second call: should advance to next
        let track = queue.start_or_next();
        assert_eq!(track.unwrap().video_id, "b");
        assert_eq!(queue.get_history().len(), 1);
    }

    #[test]
    fn test_clear_history() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));
        queue.next();
        queue.next();
        assert_eq!(queue.get_history().len(), 1);

        queue.clear_history();
        assert!(queue.get_history().is_empty());
    }

    #[test]
    fn test_limit_history_trims_oldest() {
        let mut queue = Queue::new();
        for i in 0..10 {
            queue.add_to_history(make_track(&i.to_string()));
        }
        assert_eq!(queue.get_history().len(), 10);

        queue.limit_history(3);
        assert_eq!(queue.get_history().len(), 3);
        // Should keep newest: 7, 8, 9
        assert_eq!(queue.get_history()[0].video_id, "7");
        assert_eq!(queue.get_history()[2].video_id, "9");
    }

    #[test]
    fn test_limit_history_no_op_when_under() {
        let mut queue = Queue::new();
        queue.add_to_history(make_track("a"));
        queue.limit_history(100);
        assert_eq!(queue.get_history().len(), 1);
    }

    #[test]
    fn test_restore_queue() {
        let mut queue = Queue::new();
        let tracks = vec![make_track("a"), make_track("b")];
        let current = Some(make_track("c"));

        queue.restore_queue(tracks, current);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.get_current().unwrap().video_id, "c");
    }

    #[test]
    fn test_add_to_history() {
        let mut queue = Queue::new();
        queue.add_to_history(make_track("x"));
        assert_eq!(queue.get_history().len(), 1);
        assert_eq!(queue.get_history()[0].video_id, "x");
    }

    #[test]
    fn test_next_previous_roundtrip() {
        let mut queue = Queue::new();
        queue.add(make_track("a"));
        queue.add(make_track("b"));
        queue.add(make_track("c"));

        queue.next(); // current=a, queue=[b,c], history=[]
        queue.next(); // current=b, queue=[c], history=[a]
        queue.next(); // current=c, queue=[], history=[a,b]

        queue.previous(); // current=b, queue=[c], history=[a]
        queue.previous(); // current=a, queue=[b,c], history=[]

        assert_eq!(queue.get_current().unwrap().video_id, "a");
        assert_eq!(queue.len(), 2);
        assert!(queue.get_history().is_empty());
    }
}
