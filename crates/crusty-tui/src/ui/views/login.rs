use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_login_screen(app: &MusicPlayerApp, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(12),
            Constraint::Percentage(35),
        ])
        .split(frame.area());

    let status = if !app.status_message.is_empty() {
        app.status_message.as_str()
    } else {
        ""
    };

    let login_text = format!(
        "Crusty — YouTube Music Player\n\
         \n\
         To access your personalised feed and playlists,\n\
         select a YouTube account from your browser.\n\
         \n\
         Make sure you are logged into YouTube in\n\
         Chrome, Chromium, Firefox, or Zen Browser first.\n\
         \n\
         [l]  Select browser account\n\
         [q]  Quit\n\
         \n\
         {status}"
    );

    let widget = Paragraph::new(login_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Login Required ")
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);

    frame.render_widget(widget, chunks[1]);
}

pub(crate) fn draw_account_picker(app: &MusicPlayerApp, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // header
            Constraint::Min(6),     // account list
            Constraint::Length(3),  // hint bar
        ])
        .split(frame.area());

    // -- Header --
    let current_name = app
        .browser_auth
        .load_selected_account()
        .map(|a| a.display_name)
        .unwrap_or_else(|| "none".to_string());

    let header = Paragraph::new(format!(
        "Current account: {current_name}\n\
         Use j/k to navigate, Enter to select, Esc to cancel"
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Switch Account / Log Out ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .style(Style::default().fg(Color::White))
    .alignment(Alignment::Center);

    frame.render_widget(header, chunks[0]);

    // -- Account list --
    let current_browser = app
        .browser_auth
        .load_selected_account()
        .map(|a| format!("{}:{}", a.browser, a.profile));

    let items: Vec<ListItem> = app
        .available_accounts
        .iter()
        .enumerate()
        .map(|(i, account)| {
            let is_selected = i == app.ui.selected_account_idx;
            let is_logout = account.browser == "logout";

            // Mark the currently active account
            let is_current = current_browser
                .as_deref()
                .map(|cur| cur == format!("{}:{}", account.browser, account.profile))
                .unwrap_or(false);

            let label = if is_current {
                format!("  {}  ← current", account.display_name)
            } else {
                format!("  {}", account.display_name)
            };

            let base_style = if is_logout {
                Style::default().fg(Color::Red)
            } else if is_current {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            let style = if is_selected {
                base_style.add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                base_style
            };

            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Accounts ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(list, chunks[1]);

    // -- Hint bar --
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
        Span::raw(" Navigate  "),
        Span::styled("[Enter]", Style::default().fg(Color::Green)),
        Span::raw(" Select  "),
        Span::styled("[Esc]", Style::default().fg(Color::Red)),
        Span::raw(" Cancel"),
    ]));
    frame.render_widget(hint, chunks[2]);
}
