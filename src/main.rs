// Main entry point for YouTube Terminal Music Player
// This is where the application starts

mod config;
mod player;
mod ui;
mod youtube;

use anyhow::Result;
use ui::app::MusicPlayerApp;

#[tokio::main]
async fn main() -> Result<()> {
    // Suppress ALSA error messages that pollute TUI
    // These are non-critical audio buffer warnings from the audio system
    // SAFETY: This runs inside #[tokio::main]'s generated block_on, but set_var executes
    // synchronously on the main thread before any .await yields to worker threads.
    // The tokio runtime's thread pool may exist, but no spawned tasks are running yet.
    unsafe { std::env::set_var("ALSA_PCM_NO_MMAP", "1") };

    // Initialize the music player application
    let mut app = MusicPlayerApp::new()?;

    // Run the TUI event loop
    app.run().await?;

    Ok(())
}
