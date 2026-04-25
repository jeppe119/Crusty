//! Feed browser view — displays YouTube Music personalised playlists.
//!
//! Layout (full-screen, replaces the normal home view):
//!
//! ```text
//! ┌─ Feed Browser ──────────────────────────────────────────────────────────┐
//! │ Status / loading indicator                                              │
//! ├──────────────────┬──────────────────────────┬───────────────────────────┤
//! │  SECTIONS        │  ITEMS                   │  DETAIL                   │
//! │  > My Mixes      │  > Mix 1          50 trk │  Mix 1                    │
//! │    Recommended   │    Mix 2          30 trk │  Type: Mix                │
//! │    Library       │    Mix 3          25 trk │  50 tracks                │
//! │                  │                          │  YouTube Music            │
//! ├──────────────────┴──────────────────────────┴───────────────────────────┤
//! │ [j/k] Items  [h/l] Sections  [Enter] Play  [a] Add  [r] Refresh  [Esc] │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::ui::state::PlaylistType;

use super::super::app::MusicPlayerApp;

// ---------------------------------------------------------------------------
// Spinner frames for the loading animation
// ---------------------------------------------------------------------------

const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(crate) fn draw(app: &MusicPlayerApp, frame: &mut Frame) {
    let area = frame.area();

    // Outer block — full screen
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" YouTube Music Feed ")
        .style(Style::default().fg(Color::Cyan));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // Split into: status bar (1 line) | body | hint bar (1 line)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status / loading line
            Constraint::Min(5),    // main body
            Constraint::Length(1), // keybind hint
        ])
        .split(inner);

    draw_status_bar(app, frame, rows[0]);
    draw_body(app, frame, rows[1]);
    draw_hint_bar(frame, rows[2]);
}

// ---------------------------------------------------------------------------
// Status bar (top line inside the outer block)
// ---------------------------------------------------------------------------

fn draw_status_bar(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let text = if app.feed.is_loading {
        let spinner = SPINNER[(app.ui.animation_frame as usize) % SPINNER.len()];
        format!("{spinner} Fetching YouTube Music feed via yt-dlp…")
    } else if let Some(ref err) = app.feed.last_error {
        format!("⚠  {err}")
    } else if app.feed.sections.is_empty() {
        "No feed loaded — press [r] to fetch".to_string()
    } else if let Some(instant) = app.feed.last_fetch {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            format!("✓ Updated just now  ({} sections)", app.feed.sections.len())
        } else {
            let mins = secs / 60;
            format!(
                "✓ Updated {mins} min ago  ({} sections)",
                app.feed.sections.len()
            )
        }
    } else {
        String::new()
    };

    let style = if app.feed.last_error.is_some() {
        Style::default().fg(Color::Red)
    } else if app.feed.is_loading {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let widget = Paragraph::new(text).style(style);
    frame.render_widget(widget, area);
}

// ---------------------------------------------------------------------------
// Main body — three-column layout
// ---------------------------------------------------------------------------

fn draw_body(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22), // sections sidebar
            Constraint::Percentage(45), // items list
            Constraint::Percentage(33), // detail pane
        ])
        .split(area);

    draw_sections(app, frame, cols[0]);
    draw_items(app, frame, cols[1]);
    draw_detail(app, frame, cols[2]);
}

// ---------------------------------------------------------------------------
// Left column — section sidebar
// ---------------------------------------------------------------------------

fn draw_sections(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = if app.feed.sections.is_empty() {
        vec![ListItem::new("  (empty)").style(Style::default().fg(Color::DarkGray))]
    } else {
        app.feed
            .sections
            .iter()
            .enumerate()
            .map(|(i, section)| {
                let count = section.items.len();
                let label = format!(" {} ({})", section.title, count);
                let style = if i == app.feed.selected_section {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(label).style(style)
            })
            .collect()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sections ")
        .border_style(Style::default().fg(Color::DarkGray));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

// ---------------------------------------------------------------------------
// Middle column — items list
// ---------------------------------------------------------------------------

fn draw_items(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let section_title = app
        .feed
        .sections
        .get(app.feed.selected_section)
        .map(|s| s.title.as_str())
        .unwrap_or("Feed");

    let items: Vec<ListItem> = match app.feed.sections.get(app.feed.selected_section) {
        None => vec![
            ListItem::new("  Press [r] to load feed").style(Style::default().fg(Color::DarkGray))
        ],
        Some(section) if section.items.is_empty() => {
            vec![ListItem::new("  (no items)").style(Style::default().fg(Color::DarkGray))]
        }
        Some(section) => {
            // Scrolling window: keep selected item centred
            let visible_h = area.height.saturating_sub(2) as usize;
            let total = section.items.len();
            let sel = app.feed.selected_item;

            let start = if total <= visible_h {
                0
            } else {
                let half = visible_h / 2;
                sel.saturating_sub(half).min(total - visible_h)
            };
            let end = (start + visible_h).min(total);

            section.items[start..end]
                .iter()
                .enumerate()
                .map(|(i, playlist)| {
                    let actual_idx = start + i;
                    let imported = app.feed.imported_ids.contains(&playlist.id);
                    let check = if imported { "✓ " } else { "  " };

                    let count_str = if playlist.track_count_estimate > 0 {
                        format!(" {:>3} trk", playlist.track_count_estimate)
                    } else {
                        String::new()
                    };

                    // Truncate by char count (not bytes) so CJK/emoji titles
                    // never cause a UTF-8 boundary panic.
                    let max_chars = (area.width as usize).saturating_sub(14);
                    let char_count = playlist.title.chars().count();
                    let title = if char_count > max_chars {
                        let truncated: String = playlist
                            .title
                            .chars()
                            .take(max_chars.saturating_sub(1))
                            .collect();
                        format!("{truncated}…")
                    } else {
                        playlist.title.clone()
                    };

                    let label = format!("{check}{title}{count_str}");

                    let style = if actual_idx == sel {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if imported {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    ListItem::new(label).style(style)
                })
                .collect()
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {section_title} "))
        .border_style(Style::default().fg(Color::DarkGray));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

// ---------------------------------------------------------------------------
// Right column — detail pane
// ---------------------------------------------------------------------------

fn draw_detail(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Detail ")
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(section) = app.feed.sections.get(app.feed.selected_section) else {
        let empty = Paragraph::new("Select a playlist\nto see details")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    };

    let Some(playlist) = section.items.get(app.feed.selected_item) else {
        let empty = Paragraph::new("No item selected")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    };

    let imported = app.feed.imported_ids.contains(&playlist.id);

    // Build detail lines
    let mut lines: Vec<Line> = Vec::new();

    // Title (bold, possibly wrapped)
    lines.push(Line::from(Span::styled(
        playlist.title.clone(),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Type badge
    let type_color = match playlist.playlist_type {
        PlaylistType::Mix => Color::Magenta,
        PlaylistType::Recommended => Color::Cyan,
        PlaylistType::ListenAgain => Color::Blue,
        PlaylistType::LibrarySaved => Color::Green,
        PlaylistType::LibraryLiked => Color::Red,
        PlaylistType::Unknown => Color::DarkGray,
    };
    lines.push(Line::from(vec![
        Span::styled("Type:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            playlist.playlist_type.to_string(),
            Style::default().fg(type_color).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Track count
    if playlist.track_count_estimate > 0 {
        lines.push(Line::from(vec![
            Span::styled("Tracks: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                playlist.track_count_estimate.to_string(),
                Style::default().fg(Color::White),
            ),
        ]));
    }

    // Description / channel
    if let Some(ref desc) = playlist.description {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            desc.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Imported marker
    if imported {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "✓ Added to queue",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
    }

    // Actions hint
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Enter] Play now",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "[a]     Add to queue",
        Style::default().fg(Color::DarkGray),
    )));

    let detail = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(detail, inner);
}

// ---------------------------------------------------------------------------
// Bottom hint bar
// ---------------------------------------------------------------------------

fn draw_hint_bar(frame: &mut Frame, area: Rect) {
    let hints = vec![
        Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
        Span::raw(" Items  "),
        Span::styled("[h/l]", Style::default().fg(Color::Yellow)),
        Span::raw(" Sections  "),
        Span::styled("[Enter]", Style::default().fg(Color::Green)),
        Span::raw(" Play  "),
        Span::styled("[a]", Style::default().fg(Color::Green)),
        Span::raw(" Add  "),
        Span::styled("[r]", Style::default().fg(Color::Cyan)),
        Span::raw(" Refresh  "),
        Span::styled("[Esc/f]", Style::default().fg(Color::Red)),
        Span::raw(" Close"),
    ];

    let widget = Paragraph::new(Line::from(hints));
    frame.render_widget(widget, area);
}
