use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_help_screen(_app: &MusicPlayerApp, frame: &mut Frame) {
    // Full-screen block
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help — Keybinds ")
        .style(Style::default().fg(Color::Cyan));
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    // Split into two equal columns
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // -----------------------------------------------------------------------
    // Left column
    // -----------------------------------------------------------------------
    let left = vec![
        section("PLAYBACK"),
        bind("Space",   "Toggle play / pause"),
        bind("n",       "Next track"),
        bind("p",       "Previous track"),
        bind("↑ / ↓",   "Volume up / down"),
        bind("Shift+↑↓","Volume +/- 5%"),
        bind("→ / ←",   "Seek forward / backward 10 s"),
        blank(),
        section("NAVIGATION"),
        bind("j / k",   "Navigate lists down / up"),
        bind("/",       "Search for music"),
        bind("l",       "Load playlist from URL"),
        bind("h",       "Go to Home view"),
        bind("Esc",     "Return to previous view"),
        blank(),
        section("QUEUE"),
        bind("Enter",   "Add selected item to queue"),
        bind("t",       "Toggle queue expand"),
        bind("d",       "Delete selected item (queue expanded)"),
        blank(),
        section("MY MIX"),
        bind("m",       "Toggle My Mix expand"),
        bind("Shift+M", "Refresh My Mix (when expanded)"),
        blank(),
        section("HISTORY"),
        bind("Shift+H", "Toggle history expand"),
        bind("Shift+C", "Clear history (when expanded)"),
    ];

    // -----------------------------------------------------------------------
    // Right column
    // -----------------------------------------------------------------------
    let right = vec![
        section("FEED BROWSER"),
        bind("f",       "Open YouTube Music Feed Browser"),
        bind("Esc / f", "Close Feed Browser"),
        bind("j / k",   "Navigate items down / up"),
        bind("h / l",   "Switch sections left / right"),
        bind("r",       "Force-refresh feed (bypasses cache)"),
        bind("Enter",   "Play selected playlist now"),
        bind("a",       "Add selected playlist to queue"),
        blank(),
        section("FILTER"),
        bind("Shift+F", "Toggle music-only mode (>7 min filtered)"),
        blank(),
        section("ACCOUNT"),
        bind("l",       "Select account (login screen)"),
        bind("o",       "Switch account / log out (any time)"),
        blank(),
        section("OTHER"),
        bind("?",       "Show this help screen"),
        bind("q",       "Quit"),
        blank(),
        blank(),
        footer(),
    ];

    let left_widget = Paragraph::new(left)
        .wrap(Wrap { trim: false });
    let right_widget = Paragraph::new(right)
        .wrap(Wrap { trim: false });

    frame.render_widget(left_widget, cols[0]);
    frame.render_widget(right_widget, cols[1]);
}

// ---------------------------------------------------------------------------
// Line builders
// ---------------------------------------------------------------------------

fn section(title: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))
}

fn bind(key: &'static str, desc: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<12}"),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(desc, Style::default().fg(Color::White)),
    ])
}

fn blank() -> Line<'static> {
    Line::from("")
}

fn footer() -> Line<'static> {
    Line::from(Span::styled(
        "Press '?', Esc, or 'q' to close",
        Style::default().fg(Color::DarkGray),
    ))
}
