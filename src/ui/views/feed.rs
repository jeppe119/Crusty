//! Feed browser view — displays YouTube Music playlists and their tracks.
//!
//! Two focus modes:
//!
//! **Playlist mode** (default):
//! ```text
//! ┌─ YouTube Music Feed ────────────────────────────────────────────────────┐
//! │ status bar                                                              │
//! ├──────────────────┬──────────────────────────┬───────────────────────────┤
//! │  Sections        │  Playlists               │  Detail                   │
//! │  > My Playlists  │  > Liked Music   8 trk   │  Liked Music              │
//! │    Liked Music   │    My Playlist   12 trk  │  Type: Liked              │
//! ├──────────────────┴──────────────────────────┴───────────────────────────┤
//! │ [j/k] Navigate  [h/l] Expand/Collapse  [Enter] Expand  [a] Add all  [r] Refresh│
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! **Track mode** (after pressing Enter on a playlist):
//! ```text
//! ├──────────────────┬──────────────────────────┬───────────────────────────┤
//! │  Sections        │  Tracks (Liked Music)    │  Track detail             │
//! │    My Playlists  │  > Song A        2:56    │  Song A                   │
//! │  > Liked Music   │    Song B        3:12    │  Artist                   │
//! ├──────────────────┴──────────────────────────┴───────────────────────────┤
//! │ [j/k] Navigate  [h/l] Back  [Enter] Play  [a] Add track  [Esc/f] Close  │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::config::format_time;
use crate::ui::state::{FeedFocus, PlaylistType};

use super::super::app::MusicPlayerApp;

const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(crate) fn draw(app: &MusicPlayerApp, frame: &mut Frame) {
    let area = frame.area();

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" YouTube Music Feed ")
        .style(Style::default().fg(Color::Cyan));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(5),    // body
            Constraint::Length(1), // hint bar
        ])
        .split(inner);

    draw_status_bar(app, frame, rows[0]);
    draw_body(app, frame, rows[1]);
    draw_hint_bar(app, frame, rows[2]);
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

fn draw_status_bar(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let text = if app.feed.tracks_loading {
        let spinner = SPINNER[(app.ui.animation_frame as usize) % SPINNER.len()];
        format!("{spinner} Loading tracks…")
    } else if app.feed.is_loading {
        let spinner = SPINNER[(app.ui.animation_frame as usize) % SPINNER.len()];
        format!("{spinner} Fetching feed via yt-dlp…")
    } else if app.feed.focus == FeedFocus::Tracks {
        let count = app.feed.expanded_tracks.len();
        let sel = app.feed.selected_track + 1;
        format!("Track {sel} / {count}  —  [Enter] Play  [a] Add to queue  [h/l] Back to playlists")
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
            format!("✓ Updated {mins} min ago  ({} sections)", app.feed.sections.len())
        }
    } else {
        String::new()
    };

    let style = if app.feed.last_error.is_some() && app.feed.focus != FeedFocus::Tracks {
        Style::default().fg(Color::Red)
    } else if app.feed.is_loading || app.feed.tracks_loading {
        Style::default().fg(Color::Yellow)
    } else if app.feed.focus == FeedFocus::Tracks {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    frame.render_widget(Paragraph::new(text).style(style), area);
}

// ---------------------------------------------------------------------------
// Body — three columns, content depends on focus mode
// ---------------------------------------------------------------------------

fn draw_body(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22), // sections sidebar
            Constraint::Percentage(45), // playlists or tracks
            Constraint::Percentage(33), // detail pane
        ])
        .split(area);

    draw_sections(app, frame, cols[0]);

    if app.feed.focus == FeedFocus::Tracks {
        draw_tracks(app, frame, cols[1]);
        draw_track_detail(app, frame, cols[2]);
    } else {
        draw_playlists(app, frame, cols[1]);
        draw_playlist_detail(app, frame, cols[2]);
    }
}

// ---------------------------------------------------------------------------
// Left column — section sidebar (same in both modes)
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
                let label = format!(" {} ({})", section.title, section.items.len());
                let is_active = i == app.feed.selected_section;
                let style = if is_active && app.feed.focus == FeedFocus::Playlists {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else if is_active {
                    Style::default().fg(Color::Cyan)
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
    frame.render_widget(List::new(items).block(block), area);
}

// ---------------------------------------------------------------------------
// Middle column — playlist mode
// ---------------------------------------------------------------------------

fn draw_playlists(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let section_title = app
        .feed
        .sections
        .get(app.feed.selected_section)
        .map(|s| s.title.as_str())
        .unwrap_or("Feed");

    let items: Vec<ListItem> = match app.feed.sections.get(app.feed.selected_section) {
        None => vec![
            ListItem::new("  Press [r] to load feed")
                .style(Style::default().fg(Color::DarkGray)),
        ],
        Some(section) if section.items.is_empty() => {
            vec![ListItem::new("  (no items)").style(Style::default().fg(Color::DarkGray))]
        }
        Some(section) => {
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
                        format!("  {:>3} trk", playlist.track_count_estimate)
                    } else {
                        String::new()
                    };

                    let max_chars = (area.width as usize).saturating_sub(16);
                    let char_count = playlist.title.chars().count();
                    let title = if char_count > max_chars {
                        let t: String = playlist.title.chars().take(max_chars.saturating_sub(1)).collect();
                        format!("{t}…")
                    } else {
                        playlist.title.clone()
                    };

                    let label = format!("{check}{title}{count_str}");
                    let style = if actual_idx == sel {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
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
    frame.render_widget(List::new(items).block(block), area);
}

// ---------------------------------------------------------------------------
// Middle column — track mode
// ---------------------------------------------------------------------------

fn draw_tracks(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let playlist_title = app
        .feed_selected_item_ref()
        .map(|p| p.title.clone())
        .unwrap_or_else(|| "Tracks".to_string());

    let tracks = &app.feed.expanded_tracks;
    let sel = app.feed.selected_track;

    let items: Vec<ListItem> = if tracks.is_empty() {
        vec![ListItem::new("  (no tracks)").style(Style::default().fg(Color::DarkGray))]
    } else {
        let visible_h = area.height.saturating_sub(2) as usize;
        let total = tracks.len();
        let start = if total <= visible_h {
            0
        } else {
            let half = visible_h / 2;
            sel.saturating_sub(half).min(total - visible_h)
        };
        let end = (start + visible_h).min(total);

        tracks[start..end]
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let actual_idx = start + i;
                let dur = format_time(track.duration as f64);

                let max_chars = (area.width as usize).saturating_sub(10);
                let char_count = track.title.chars().count();
                let title = if char_count > max_chars {
                    let t: String = track.title.chars().take(max_chars.saturating_sub(1)).collect();
                    format!("{t}…")
                } else {
                    track.title.clone()
                };

                let label = format!("  {title}  {dur}");
                let style = if actual_idx == sel {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(label).style(style)
            })
            .collect()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} — {} tracks ", playlist_title, tracks.len()))
        .border_style(Style::default().fg(Color::Yellow)); // yellow = track focus active
    frame.render_widget(List::new(items).block(block), area);
}

// ---------------------------------------------------------------------------
// Right column — playlist detail
// ---------------------------------------------------------------------------

fn draw_playlist_detail(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Detail ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(playlist) = app.feed_selected_item_ref() else {
        let empty = Paragraph::new("Select a playlist\nto see details")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    };

    let imported = app.feed.imported_ids.contains(&playlist.id);

    let type_color = match playlist.playlist_type {
        PlaylistType::Mix => Color::Magenta,
        PlaylistType::Recommended => Color::Cyan,
        PlaylistType::ListenAgain => Color::Blue,
        PlaylistType::LibrarySaved => Color::Green,
        PlaylistType::LibraryLiked => Color::Red,
        PlaylistType::Unknown => Color::DarkGray,
    };

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            playlist.title.clone(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Type:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                playlist.playlist_type.to_string(),
                Style::default().fg(type_color).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if playlist.track_count_estimate > 0 {
        lines.push(Line::from(vec![
            Span::styled("Tracks: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                playlist.track_count_estimate.to_string(),
                Style::default().fg(Color::White),
            ),
        ]));
    }

    if let Some(ref desc) = playlist.description {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            desc.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    if imported {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "✓ Added to queue",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Enter] Expand tracks",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "[a]     Add all to queue",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Right column — track detail
// ---------------------------------------------------------------------------

fn draw_track_detail(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Track ")
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(track) = app.feed.expanded_tracks.get(app.feed.selected_track) else {
        frame.render_widget(
            Paragraph::new("No track selected")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    };

    let dur = format_time(track.duration as f64);
    let lines = vec![
        Line::from(Span::styled(
            track.title.clone(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Artist:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(track.uploader.clone(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Duration: ", Style::default().fg(Color::DarkGray)),
            Span::styled(dur, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "[Enter] Play now",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "[a]     Add to queue",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "[h/l]   Back to playlists",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Hint bar
// ---------------------------------------------------------------------------

fn draw_hint_bar(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    let hints = if app.feed.focus == FeedFocus::Tracks {
        vec![
            Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
            Span::raw(" Tracks  "),
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Play  "),
            Span::styled("[a]", Style::default().fg(Color::Green)),
            Span::raw(" Add  "),
            Span::styled("[h/l]", Style::default().fg(Color::Cyan)),
            Span::raw(" Back  "),
            Span::styled("[Esc/f]", Style::default().fg(Color::Red)),
            Span::raw(" Close"),
        ]
    } else {
        vec![
            Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate  "),
            Span::styled("[h/l]", Style::default().fg(Color::Yellow)),
            Span::raw(" Sections  "),
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Expand  "),
            Span::styled("[a]", Style::default().fg(Color::Green)),
            Span::raw(" Add all  "),
            Span::styled("[r]", Style::default().fg(Color::Cyan)),
            Span::raw(" Refresh  "),
            Span::styled("[Esc/f]", Style::default().fg(Color::Red)),
            Span::raw(" Close"),
        ]
    };

    frame.render_widget(Paragraph::new(Line::from(hints)), area);
}
