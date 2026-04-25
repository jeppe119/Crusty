//! UI state types extracted from the MusicPlayerApp god object.

use crate::player::queue::Track;
use crate::youtube::extractor::VideoInfo;

/// Application interaction mode — determines which input handler is active.
#[derive(Debug, Default, PartialEq)]
pub(crate) enum AppMode {
    #[default]
    Normal,
    Searching,
    LoginPrompt,
    AccountPicker,
    Help,
    LoadingPlaylist,
    /// The YouTube Music feed browser is open.
    FeedBrowser,
}

/// Which top-level view is currently displayed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum ViewMode {
    #[default]
    Home,
    Search,
}

/// A YouTube "My Mix" auto-generated playlist.
#[derive(Debug, Clone)]
pub(crate) struct MixPlaylist {
    pub title: String,
    pub track_count: usize,
    pub url: String,
}

// ---------------------------------------------------------------------------
// Feed browser types
// ---------------------------------------------------------------------------

/// The category of a YouTube Music playlist entry in the feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PlaylistType {
    /// Auto-generated "My Mix" playlists (`RDCLAK*`, `RDAMPL*`).
    Mix,
    /// Algorithmically recommended playlists (`OLAK5uy_*`).
    Recommended,
    /// "Listen Again" re-recommendation entries.
    ListenAgain,
    /// User-saved or imported playlists (`PL*`, `VL*`).
    LibrarySaved,
    /// The "Liked Music" auto-playlist (`LM`).
    LibraryLiked,
    /// Unrecognised playlist type.
    Unknown,
}

impl std::fmt::Display for PlaylistType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            PlaylistType::Mix => "Mix",
            PlaylistType::Recommended => "Recommended",
            PlaylistType::ListenAgain => "Listen Again",
            PlaylistType::LibrarySaved => "Library",
            PlaylistType::LibraryLiked => "Liked",
            PlaylistType::Unknown => "Playlist",
        };
        write!(f, "{label}")
    }
}

/// A single playlist entry in the YouTube Music feed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FeedPlaylist {
    /// Playlist ID (e.g. `RDCLAK5uy_…`, `OLAK5uy_…`, `PL…`, `LM`).
    pub id: String,
    pub title: String,
    /// Canonical `music.youtube.com/playlist?list=…` URL.
    pub url: String,
    pub playlist_type: PlaylistType,
    /// Approximate track count; `0` if unknown.
    pub track_count_estimate: usize,
    pub thumbnail_url: Option<String>,
    /// Subtitle, channel name, or description from yt-dlp.
    pub description: Option<String>,
}

/// A labelled group of [`FeedPlaylist`] entries (e.g. "My Mixes").
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FeedSection {
    /// Human-readable section title shown in the feed browser.
    pub title: String,
    /// The dominant playlist type for this section.
    pub kind: PlaylistType,
    pub items: Vec<FeedPlaylist>,
}

impl FeedSection {
    /// Bump this when `FeedSection` or `FeedPlaylist` gains/loses a field in a
    /// serde-incompatible way. The `CacheStore` uses it to silently discard
    /// stale cache files rather than failing to deserialize.
    pub(crate) const CACHE_SCHEMA_VERSION: u32 = 1;
}

/// Runtime state for the feed browser view.
#[derive(Debug, Default)]
pub(crate) struct FeedState {
    /// Fetched sections (Mixes, Recommended, Library, …).
    pub sections: Vec<FeedSection>,
    /// Index of the currently highlighted section in the sidebar.
    pub selected_section: usize,
    /// Index of the currently highlighted item within the selected section.
    pub selected_item: usize,
    /// `true` while an async fetch is in progress.
    pub is_loading: bool,
    /// When the feed was last successfully fetched.
    pub last_fetch: Option<std::time::Instant>,
    /// Last error message to display in the feed browser.
    pub last_error: Option<String>,
    /// IDs of playlists that have been imported into the playlist manager
    /// during this session (used to render a ✓ marker).
    pub imported_ids: std::collections::HashSet<String>,
}

/// Serializable snapshot of the queue for persistence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct QueueState {
    pub tracks: Vec<Track>,
    pub current_track: Option<Track>,
}

/// UI selection indices, expansion toggles, and animation state.
#[derive(Debug)]
pub(crate) struct UiState {
    pub selected_result: usize,
    pub selected_queue_item: usize,
    pub selected_mix_item: usize,
    pub selected_history_item: usize,
    pub selected_account_idx: usize,
    pub queue_expanded: bool,
    pub my_mix_expanded: bool,
    pub history_expanded: bool,
    pub playlist_loading_expanded: bool,
    pub animation_frame: u8,
    pub title_scroll_offset: usize,
    pub last_animation_update: std::time::Instant,
    pub music_only_mode: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_result: 0,
            selected_queue_item: 0,
            selected_mix_item: 0,
            selected_history_item: 0,
            selected_account_idx: 0,
            queue_expanded: false,
            my_mix_expanded: false,
            history_expanded: false,
            playlist_loading_expanded: false,
            animation_frame: 0,
            title_scroll_offset: 0,
            last_animation_update: std::time::Instant::now(),
            music_only_mode: true,
        }
    }
}

/// Search-related state.
#[derive(Debug, Default)]
pub(crate) struct SearchState {
    pub results: Vec<VideoInfo>,
    pub query: String,
    pub is_searching: bool,
}

/// Playlist-related state (My Mix + loaded playlists).
#[derive(Debug, Default)]
pub(crate) struct PlaylistState {
    pub my_mix_playlists: Vec<MixPlaylist>,
    pub loaded_tracks: Vec<Track>,
    pub loaded_name: String,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_mode_default_is_normal() {
        assert_eq!(AppMode::default(), AppMode::Normal);
    }

    #[test]
    fn test_view_mode_default_is_home() {
        assert_eq!(ViewMode::default(), ViewMode::Home);
    }

    #[test]
    fn test_view_mode_is_copy() {
        let mode = ViewMode::Search;
        let copy = mode;
        assert_eq!(mode, copy);
    }

    #[test]
    fn test_ui_state_default() {
        let state = UiState::default();
        assert_eq!(state.selected_result, 0);
        assert!(!state.queue_expanded);
        assert_eq!(state.animation_frame, 0);
    }

    #[test]
    fn test_search_state_default() {
        let state = SearchState::default();
        assert!(state.results.is_empty());
        assert!(state.query.is_empty());
        assert!(!state.is_searching);
    }

    #[test]
    fn test_playlist_state_default() {
        let state = PlaylistState::default();
        assert!(state.my_mix_playlists.is_empty());
        assert!(state.loaded_tracks.is_empty());
        assert!(state.loaded_name.is_empty());
        assert!(state.url.is_empty());
    }
}
