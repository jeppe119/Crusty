use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::super::app::MusicPlayerApp;

pub(crate) fn draw_login_screen(app: &MusicPlayerApp, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(10),
            Constraint::Percentage(40),
        ])
        .split(frame.area());

    let login_text = [
        "YouTube Music Player",
        "",
        "Welcome! To access YouTube Music, you'll select",
        "a YouTube account from your browser (Chrome/Firefox).",
        "",
        "Make sure you're logged into YouTube in your browser first.",
        "",
        "Press 'L' to select account",
        "Press 'Q' to quit",
        "",
        if !app.status_message.is_empty() {
            &app.status_message
        } else {
            ""
        },
    ]
    .join("\n");

    let login_widget = Paragraph::new(login_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Login Required"),
        )
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);

    frame.render_widget(login_widget, chunks[1]);
}

pub(crate) fn draw_account_picker(app: &MusicPlayerApp, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Min(10),
            Constraint::Percentage(20),
        ])
        .split(frame.area());

    // Header
    let header_text = [
        "Select YouTube Account",
        "",
        "Use j/k or ↑/↓ to navigate",
        "Press Enter to select",
        "Press Esc to go back",
        "",
    ]
    .join("\n");

    let header = Paragraph::new(header_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Account Selection"),
        )
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);

    frame.render_widget(header, chunks[0]);

    // Account list
    let account_items: Vec<ListItem> = app
        .available_accounts
        .iter()
        .enumerate()
        .map(|(i, account)| {
            let content = account.display_name.clone();
            let style = if i == app.ui.selected_account_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let account_list = List::new(account_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Available Accounts"),
    );

    frame.render_widget(account_list, chunks[1]);
}
