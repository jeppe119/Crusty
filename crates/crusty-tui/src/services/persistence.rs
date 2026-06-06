//! Re-export of the persistence service from `crusty-core`.
//!
//! The implementation now lives in `crusty_core::persistence`. This shim keeps
//! the existing `crate::services::persistence::*` import paths working, including
//! `write_atomic`/`MAX_FILE_SIZE` used by `cache_store`.

pub(crate) use crusty_core::persistence::{write_atomic, MAX_FILE_SIZE, MAX_HISTORY_SIZE};
pub(crate) use crusty_core::{PersistenceService, PlaybackState};
