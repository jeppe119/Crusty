//! MPRIS2 D-Bus integration (`mpris` feature).
//!
//! Exposes Crusty as `org.mpris.MediaPlayer2.crusty` so that Waybar's `mpris`
//! module, Quickshell's `Quickshell.Services.Mpris`, and `playerctl` can display
//! and control playback. Driven by an [`EngineController`](crate::EngineController)
//! plus a host action channel for queue navigation.
//!
//! The pure snapshot↔MPRIS mappings live in [`mapping`] (unit-tested without a
//! bus); the D-Bus server + signal emission live in [`server`].

pub mod mapping;
pub mod server;

pub use server::{emit_seeked, serve_mpris, CrustyMpris, MprisAction};
