// Main entry point for YouTube Terminal Music Player
// This is where the application starts

mod player;
mod youtube;
mod ui;

use ui::app::MusicPlayerApp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the music player application
    let mut app = MusicPlayerApp::new();

    // Run the TUI event loop
    app.run().await?;

    Ok(())
}
