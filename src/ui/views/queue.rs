use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_queue_compact(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    // Show queue items vertically
    let queue_len = app.queue.len();

    if queue_len == 0 {
        let queue_widget =
            Paragraph::new("Queue is empty - Add tracks by pressing Enter on search results")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Queue (0 tracks) - Press 't' to expand for management"),
                );
        frame.render_widget(queue_widget, area);
    } else {
        // Calculate how many items fit in the visible area (fill the whole box!)
        let visible_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let max_items = visible_height.min(queue_len);

        let queue_slice = app.queue.get_queue_slice(0, max_items);

        let items: Vec<ListItem> = queue_slice
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let content = format!("{}. {}", i + 1, &track.title);
                ListItem::new(content).style(Style::default().fg(Color::White))
            })
            .collect();

        let queue_list_widget =
            List::new(items).block(Block::default().borders(Borders::ALL).title(format!(
                "Queue ({} tracks) - Press 't' to expand for management",
                queue_len
            )));
        frame.render_widget(queue_list_widget, area);
    }
}

pub(crate) fn draw_queue_expanded(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let total_tracks = app.queue.len();

    // Calculate visible window (show items around selected item)
    let visible_height = area.height.saturating_sub(2) as usize; // Subtract borders
    let half_window = visible_height / 2;

    let (start_idx, end_idx) = if total_tracks <= visible_height {
        // Show all if fits on screen
        (0, total_tracks)
    } else {
        // Calculate scrolling window
        let start = app.ui.selected_queue_item.saturating_sub(half_window);
        let end = (start + visible_height).min(total_tracks);

        // Adjust if we're at the end
        if end == total_tracks && total_tracks > visible_height {
            (total_tracks - visible_height, total_tracks)
        } else {
            (start, end)
        }
    };

    // Only get visible slice of tracks - huge performance improvement!
    let visible_count = end_idx - start_idx;
    let queue_slice = app.queue.get_queue_slice(start_idx, visible_count);

    let queue_items: Vec<ListItem> = queue_slice
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let actual_idx = start_idx + i;
            let content = format!("{}. {}", actual_idx + 1, &track.title);
            let style = if actual_idx == app.ui.selected_queue_item {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let scroll_indicator = if total_tracks > visible_height {
        format!(
            " (Showing {}-{} of {})",
            start_idx + 1,
            end_idx,
            total_tracks
        )
    } else {
        String::new()
    };

    let queue_list =
        List::new(queue_items).block(Block::default().borders(Borders::ALL).title(format!(
            "Queue (Expanded) - {} tracks{} | [j/k] Navigate | [d] Delete | [t] Collapse",
            total_tracks, scroll_indicator
        )));
    frame.render_widget(queue_list, area);
}
