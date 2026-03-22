use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::super::app::MusicPlayerApp;
use super::player_bar::get_download_animation;

pub(crate) fn draw_cache_stats(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let active_count = app.downloads.active_count();
    let cached_count = app.downloads.cached_count();

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
