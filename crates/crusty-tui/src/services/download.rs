//! Re-export of the download manager from `crusty-core`.
//!
//! The implementation now lives in `crusty_core::download`. This shim keeps the
//! existing `crate::services::download::*` import paths working.

pub(crate) use crusty_core::DownloadManager;
