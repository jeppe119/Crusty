//! Re-export of the playback queue from `crusty-core`.
//!
//! The implementation now lives in `crusty_core::queue`. This shim keeps the
//! existing `crate::player::queue::{Track, Queue}` import paths working.

pub use crusty_core::{Queue, Track};
