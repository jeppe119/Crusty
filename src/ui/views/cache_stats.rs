use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::super::app::MusicPlayerApp;
use super::player_bar::get_download_animation;

pub(crate) fn draw_cache_stats(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    // Cache/Download stats box
    let active_count = *app
        .active_downloads
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let cached_count = app
        .downloaded_files
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .len();

    let cache_info = if active_count > 0 {
        format!(
            "{}\n⬇ {}\n💾 {}",
            get_download_animation(app.ui.animation_frame),
            active_count,
            cached_count
        )
    } else {
        format!("💾\n{}\ncached", cached_count)
    };

    let cache_widget = Paragraph::new(cache_info)
        .block(Block::default().borders(Borders::ALL).title("Cache"))
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);

    frame.render_widget(cache_widget, area);
}
