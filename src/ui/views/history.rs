use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_history(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let queue_history = app.queue.get_history();
    let total_history = queue_history.len();

    // Calculate how many items fit in the visible area (fill the whole box like queue!)
    let visible_height = area.height.saturating_sub(2) as usize; // Subtract borders
    let max_items = visible_height.min(total_history);

    let history_items: Vec<ListItem> = queue_history
        .iter()
        .rev() // Show most recent first
        .take(max_items) // Fill the box!
        .map(|track| {
            let content = track.title.clone();
            ListItem::new(content).style(Style::default().fg(Color::DarkGray))
        })
        .collect();

    let history_list =
        List::new(history_items).block(Block::default().borders(Borders::ALL).title(format!(
            "History ({} played) - Press [Shift+H] to expand",
            total_history
        )));
    frame.render_widget(history_list, area);
}

pub(crate) fn draw_history_expanded(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let queue_history = app.queue.get_history();
    let total_history = queue_history.len();

    // Calculate visible window
    let visible_height = area.height.saturating_sub(2) as usize;
    let half_window = visible_height / 2;

    let (start_idx, end_idx) = if total_history <= visible_height {
        (0, total_history)
    } else {
        let start = app.ui.selected_history_item.saturating_sub(half_window);
        let end = (start + visible_height).min(total_history);
        if end == total_history && total_history > visible_height {
            (total_history - visible_height, total_history)
        } else {
            (start, end)
        }
    };

    // Only render visible window
    let history_items: Vec<ListItem> = queue_history
        .iter()
        .rev()
        .enumerate()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .map(|(i, track)| {
            let content = format!("{}. {}", i + 1, &track.title);
            let style = if i == app.ui.selected_history_item {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let scroll_indicator = if total_history > visible_height {
        format!(
            " (Showing {}-{} of {})",
            start_idx + 1,
            end_idx,
            total_history
        )
    } else {
        String::new()
    };

    let history_list =
        List::new(history_items).block(Block::default().borders(Borders::ALL).title(format!(
        "History (Expanded) - {} played{} | [j/k] Navigate | [Shift+C] Clear | [Shift+H] Collapse",
        total_history, scroll_indicator
    )));
    frame.render_widget(history_list, area);
}
