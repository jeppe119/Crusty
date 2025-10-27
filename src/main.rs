// Main entry point for YouTube Terminal Music Player
// This is where the application starts

mod player;
mod youtube;
mod ui;

use ui::app::MusicPlayerApp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Suppress ALSA error messages that pollute TUI
    // These are non-critical audio buffer warnings from the audio system
    std::env::set_var("ALSA_PCM_NO_MMAP", "1");

    // Initialize the music player application
    let mut app = MusicPlayerApp::new();

    // Run the TUI event loop
    app.run().await?;

    Ok(())
}
