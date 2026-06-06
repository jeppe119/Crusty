use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_search_results(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let results: Vec<ListItem> = app
        .search
        .results
        .iter()
        .enumerate()
        .map(|(i, video)| {
            let content = video.title.clone();
            let style = if i == app.ui.selected_result {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let results_list = List::new(results).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Search Results"),
    );
    frame.render_widget(results_list, area);
}
