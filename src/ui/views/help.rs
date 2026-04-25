use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_help_screen(_app: &MusicPlayerApp, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Min(10),
            Constraint::Percentage(10),
        ])
        .split(frame.area());

    let help_text = vec![
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━ KEYBINDS ━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
        "",
        "PLAYBACK:",
        "  Space     Toggle play/pause",
        "  n         Play next track",
        "  p         Play previous track",
        "  ↑/↓       Volume up/down (Shift for +/-5%)",
        "  ←/→       Seek backward/forward (not yet implemented)",
        "",
        "NAVIGATION:",
        "  j/k       Navigate lists (down/up)",
        "  /         Search for music",
        "  l         Load playlist from URL (YouTube/YouTube Music)",
        "  h         Go to Home (My Mix) view",
        "  Esc       Return to previous view",
        "",
        "QUEUE MANAGEMENT:",
        "  Enter     Add selected item to queue",
        "  t         Toggle queue expansion",
        "  d         Delete selected queue item (when queue expanded)",
        "",
        "MY MIX:",
        "  m         Toggle My Mix expansion",
        "  Shift+M   Refresh My Mix (when expanded)",
        "",
        "HISTORY:",
        "  Shift+H   Toggle history expansion",
        "  Shift+C   Clear history (when history expanded)",
        "",
        "FEED BROWSER:",
        "  f         Open YouTube Music Feed Browser",
        "  Esc/f     Close Feed Browser",
        "  j/k       Navigate items (down/up)",
        "  h/l       Switch sections (left/right)",
        "  r         Refresh feed",
        "  Enter     Play selected playlist (Phase 3)",
        "  a         Add selected playlist to queue (Phase 3)",
        "",
        "FILTER:",
        "  Shift+F   Toggle music-only mode (filters tracks >5min)",
        "",
        "OTHER:",
        "  ?         Show this help screen",
        "  q         Quit application",
        "",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
        "",
        "Press '?', 'Esc', or 'q' to close this help screen",
    ]
    .join("\n");

    let help_widget = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left);

    frame.render_widget(help_widget, chunks[1]);
}
