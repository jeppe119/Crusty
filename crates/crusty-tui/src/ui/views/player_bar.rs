use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::config::{clean_title, format_time};
use crusty_core::PlayerState;

use super::super::app::MusicPlayerApp;

/// Get animated download indicator (Pac-Man style).
pub(crate) fn get_download_animation(animation_frame: u8) -> &'static str {
    // Animate every 8 frames (slower animation)
    let frame = (animation_frame / 8) % 4;
    match frame {
        0 => "ᗧ··· ", // Pac-Man open
        1 => "·ᗧ·· ", // Moving right
        2 => "··ᗧ· ", // Moving right
        3 => "···ᗧ ", // Moving right
        _ => "ᗧ··· ",
    }
}

pub(crate) fn draw_player_compact(app: &MusicPlayerApp, frame: &mut Frame, area: Rect) {
    // Single Player box with 3 lines of content inside
    let current_track = app.queue.get_current();

    // Line 1: Now Playing title (rotating if too long)
    let now_playing = if let Some(track) = current_track {
        let clean = clean_title(&track.title);
        let full_text = format!("{} - {}", clean, track.uploader);

        // Scroll text if too long (more than 80 chars)
        if full_text.len() > 80 {
            let raw_scroll_pos = app.ui.title_scroll_offset % full_text.len();
            // Ensure we slice at a valid UTF-8 character boundary
            // Find the nearest valid char boundary at or before raw_scroll_pos
            let mut scroll_pos = raw_scroll_pos;
            while scroll_pos > 0 && !full_text.is_char_boundary(scroll_pos) {
                scroll_pos -= 1;
            }
            let rotated = format!(
                "{}   {}",
                &full_text[scroll_pos..],
                &full_text[..scroll_pos]
            );

            // Also ensure the final slice is at a char boundary
            let max_len = 80.min(rotated.len());
            let mut end_pos = max_len;
            while end_pos > 0 && !rotated.is_char_boundary(end_pos) {
                end_pos -= 1;
            }
            format!("Now Playing: {}", &rotated[..end_pos])
        } else {
            format!("Now Playing: {}", full_text)
        }
    } else {
        "No track playing".to_string()
    };

    // Line 2: Progress bar with bouncing visualization
    let time_pos = app.snapshot.position_secs;
    let player_duration = app.snapshot.duration_secs;

    // ALWAYS prefer player duration (from actual audio) over track.duration (often 0 from flat-playlist)
    // Also use track.duration as last resort if available and > 0
    let duration = if player_duration > 0.0 {
        player_duration
    } else if let Some(track) = current_track {
        if track.duration > 0 {
            track.duration as f64
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Bouncy bars + timer progress bar
    let progress_bar = if app.snapshot.state == PlayerState::Playing {
        // Bouncing bars animation
        let anim_frame = (app.ui.animation_frame / 4) % 8;
        let bars = match anim_frame {
            0 => "▁▂▃▄▅▆▇█▁▂▃▄▅▆▇█▁▂▃▄▅▆▇█",
            1 => "▂▃▄▅▆▇█▇▂▃▄▅▆▇█▇▂▃▄▅▆▇█▇",
            2 => "▃▄▅▆▇█▇▆▃▄▅▆▇█▇▆▃▄▅▆▇█▇▆",
            3 => "▄▅▆▇█▇▆▅▄▅▆▇█▇▆▅▄▅▆▇█▇▆▅",
            4 => "▅▆▇█▇▆▅▄▅▆▇█▇▆▅▄▅▆▇█▇▆▅▄",
            5 => "▆▇█▇▆▅▄▃▆▇█▇▆▅▄▃▆▇█▇▆▅▄▃",
            6 => "▇█▇▆▅▄▃▂▇█▇▆▅▄▃▂▇█▇▆▅▄▃▂",
            7 => "█▇▆▅▄▃▂▁█▇▆▅▄▃▂▁█▇▆▅▄▃▂▁",
            _ => "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄",
        };

        // Just bars + timer
        if duration > 0.0 {
            format!(
                "{} {}/{}",
                bars,
                format_time(time_pos),
                format_time(duration)
            )
        } else {
            format!("{} {}", bars, format_time(time_pos))
        }
    } else {
        // When paused, just show time
        if duration > 0.0 {
            format!("{}/{}", format_time(time_pos), format_time(duration))
        } else {
            "Not playing".to_string()
        }
    };

    // Line 3: Status info
    let state_str = match app.snapshot.state {
        PlayerState::Playing => "▶ Playing",
        PlayerState::Paused => "⏸ Paused",
        PlayerState::Stopped => "⏹ Stopped",
        PlayerState::Loading => "... Loading",
    };

    let volume = app.snapshot.volume;
    let mode_str = if app.ui.music_only_mode {
        "Music"
    } else {
        "All"
    };
    let status_line = format!(
        "{} | Vol: {}% | Queue: {} tracks | [{}]",
        state_str,
        volume,
        app.queue.len(),
        mode_str
    );

    // Combine all 3 lines inside single Player box
    let player_content = format!("{}\n{}\n{}", now_playing, progress_bar, status_line);

    let player_widget = Paragraph::new(player_content)
        .block(Block::default().borders(Borders::ALL).title("Player"))
        .style(Style::default().fg(Color::Cyan));

    frame.render_widget(player_widget, area);
}
