//! Re-export of shared config from `crusty-core`.
//!
//! The implementation now lives in `crusty_core::config`. This shim keeps the
//! existing `crate::config::*` import paths working across the TUI crate.

pub use crusty_core::config::*;
