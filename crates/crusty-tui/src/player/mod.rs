// Player module — queue re-export shim.
//
// Audio playback now lives in `crusty_core::engine` (the thread-backed
// `AudioEngine`). The legacy synchronous `AudioPlayer` has been removed.

pub mod queue;
