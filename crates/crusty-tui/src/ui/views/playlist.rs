use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::config::format_time;

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_my_mix(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let mix_items: Vec<ListItem> = if !app.playlist.loaded_tracks.is_empty() {
        // Show first 50 tracks from loaded playlist
        app.playlist
            .loaded_tracks
            .iter()
            .take(50)
            .enumerate()
            .map(|(i, track)| {
                let duration = format_time(track.duration as f64);
                let content = format!("{}. {} [{}]", i + 1, &track.title, duration);
                ListItem::new(content).style(Style::default().fg(Color::White))
            })
            .collect()
    } else if app.playlist.my_mix_playlists.is_empty() {
        vec![
            ListItem::new("Press 'l' to load a playlist URL")
                .style(Style::default().fg(Color::Yellow)),
            ListItem::new(""),
            ListItem::new("YouTube Music playlists supported!"),
        ]
    } else {
        app.playlist
            .my_mix_playlists
            .iter()
            .enumerate()
            .map(|(i, mix)| {
                let content = if mix.track_count > 0 {
                    format!("{} ({} tracks)", mix.title, mix.track_count)
                } else {
                    mix.title.clone()
                };
                let style = if i == app.ui.selected_mix_item {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(content).style(style)
            })
            .collect()
    };

    let title = if !app.playlist.loaded_name.is_empty() {
        format!("{} - Press [l] to load another", app.playlist.loaded_name)
    } else {
        "Playlists - Press [l] to load playlist URL".to_string()
    };

    let mix_list = List::new(mix_items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(mix_list, area);
}

pub(crate) fn draw_my_mix_expanded(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let mix_items: Vec<ListItem> = app
        .playlist
        .my_mix_playlists
        .iter()
        .enumerate()
        .map(|(i, mix)| {
            let content = format!("{}. {} ({} tracks)", i + 1, mix.title, mix.track_count);
            let style = if i == app.ui.selected_mix_item {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let mix_list = List::new(mix_items)
        .block(Block::default().borders(Borders::ALL).title("My Mix (Expanded) - [j/k] Navigate | [Enter] Add to queue | [m] Collapse | [Shift+m] Refresh"));
    frame.render_widget(mix_list, area);
}

pub(crate) fn draw_playlist_loading_expanded(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    // Expanded playlist loading interface - shows input prominently
    let loading_text = vec![
        "📋 Load Playlist from URL",
        "",
        "Paste your YouTube or YouTube Music playlist URL below:",
        "",
        &format!("URL: {}_", app.playlist.url),
        "",
        "",
        "Instructions:",
        "  • Paste a YouTube Music playlist URL",
        "  • Paste a YouTube playlist URL",
        "  • Press Enter to load",
        "  • Press Esc to cancel",
        "",
        "",
        "Example URLs:",
        "  https://music.youtube.com/playlist?list=...",
        "  https://www.youtube.com/playlist?list=...",
    ]
    .join("\n");

    let loading_widget = Paragraph::new(loading_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Load Playlist (Expanded) - Press [Esc] to cancel"),
        )
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Left);

    frame.render_widget(loading_widget, area);
}
