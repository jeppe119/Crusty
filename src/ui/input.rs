//! Input handler using the Command pattern.
//!
//! `key_to_command()` is a pure function mapping key events to commands — fully testable.
//! `MusicPlayerApp::execute_command()` dispatches commands with side effects.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ui::state::AppMode;

/// All actions the user can trigger via keyboard input.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AppCommand {
    Quit,
    ShowHelp,
    DismissHelp,
    StartSearch,
    StartLogin,
    StartLoadPlaylist,

    // Playback
    TogglePause,
    NextTrack,
    PreviousTrack,
    VolumeUp { big_step: bool },
    VolumeDown { big_step: bool },
    SeekForward,
    SeekBackward,

    // Navigation
    NavigateDown,
    NavigateUp,
    Select,
    GoHome,
    EscapeBack,

    // Queue / History / Mix toggles
    ToggleQueueExpand,
    ToggleHistoryExpand,
    ToggleMixExpand,
    ToggleMusicOnlyMode,
    RefreshMix,
    Delete,
    ClearHistory,

    // Account picker
    NextAccount,
    PreviousAccount,
    SelectAccount,
    CancelAccountPicker,

    // Search input
    SearchChar(char),
    SearchBackspace,
    SearchSubmit,
    SearchCancel,

    // Playlist URL input
    PlaylistChar(char),
    PlaylistBackspace,
    PlaylistSubmit,
    PlaylistCancel,
}

/// Snapshot of relevant UI state for key mapping decisions.
pub(crate) struct InputContext<'a> {
    pub mode: &'a AppMode,
    pub history_expanded: bool,
    pub search_query_len: usize,
    pub playlist_url_len: usize,
}

/// Pure function: maps a key event + current context to a command.
/// Returns `None` if the key has no binding in the current context.
pub(crate) fn key_to_command(key: KeyEvent, ctx: &InputContext<'_>) -> Option<AppCommand> {
    let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match ctx.mode {
        AppMode::LoginPrompt => match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => Some(AppCommand::Quit),
            KeyCode::Char('l') | KeyCode::Char('L') => Some(AppCommand::StartLogin),
            _ => None,
        },
        AppMode::AccountPicker => match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                Some(AppCommand::CancelAccountPicker)
            }
            KeyCode::Char('j') | KeyCode::Down => Some(AppCommand::NextAccount),
            KeyCode::Char('k') | KeyCode::Up => Some(AppCommand::PreviousAccount),
            KeyCode::Enter => Some(AppCommand::SelectAccount),
            _ => None,
        },
        AppMode::Searching => match key.code {
            KeyCode::Char(c) if ctx.search_query_len < crate::config::MAX_SEARCH_QUERY_LEN => {
                Some(AppCommand::SearchChar(c))
            }
            KeyCode::Backspace => Some(AppCommand::SearchBackspace),
            KeyCode::Enter => Some(AppCommand::SearchSubmit),
            KeyCode::Esc => Some(AppCommand::SearchCancel),
            _ => None,
        },
        AppMode::LoadingPlaylist => match key.code {
            KeyCode::Char(c) if ctx.playlist_url_len < crate::config::MAX_PLAYLIST_URL_LEN => {
                Some(AppCommand::PlaylistChar(c))
            }
            KeyCode::Backspace => Some(AppCommand::PlaylistBackspace),
            KeyCode::Enter => Some(AppCommand::PlaylistSubmit),
            KeyCode::Esc => Some(AppCommand::PlaylistCancel),
            _ => None,
        },
        AppMode::Help => match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => Some(AppCommand::DismissHelp),
            _ => None,
        },
        AppMode::Normal => match key.code {
            KeyCode::Char('q') => Some(AppCommand::Quit),
            KeyCode::Char('?') => Some(AppCommand::ShowHelp),
            KeyCode::Char('/') => Some(AppCommand::StartSearch),
            KeyCode::Char('l') => Some(AppCommand::StartLoadPlaylist),
            KeyCode::Char(' ') => Some(AppCommand::TogglePause),
            KeyCode::Char('n') => Some(AppCommand::NextTrack),
            KeyCode::Char('p') => Some(AppCommand::PreviousTrack),
            KeyCode::Char('t') | KeyCode::Char('T') => Some(AppCommand::ToggleQueueExpand),
            KeyCode::Char('h') if has_shift => Some(AppCommand::ToggleHistoryExpand),
            KeyCode::Char('H') => Some(AppCommand::ToggleHistoryExpand),
            KeyCode::Char('h') => Some(AppCommand::GoHome),
            KeyCode::Char('m') if has_shift => Some(AppCommand::RefreshMix),
            KeyCode::Char('M') => Some(AppCommand::RefreshMix),
            KeyCode::Char('m') => Some(AppCommand::ToggleMixExpand),
            KeyCode::Char('f') => Some(AppCommand::ToggleMusicOnlyMode),
            KeyCode::Char('d') | KeyCode::Char('D') => Some(AppCommand::Delete),
            KeyCode::Char('c') | KeyCode::Char('C') if has_shift && ctx.history_expanded => {
                Some(AppCommand::ClearHistory)
            }
            KeyCode::Esc => Some(AppCommand::EscapeBack),
            KeyCode::Up => Some(AppCommand::VolumeUp {
                big_step: has_shift,
            }),
            KeyCode::Down => Some(AppCommand::VolumeDown {
                big_step: has_shift,
            }),
            KeyCode::Right => Some(AppCommand::SeekForward),
            KeyCode::Left => Some(AppCommand::SeekBackward),
            KeyCode::Char('j') => Some(AppCommand::NavigateDown),
            KeyCode::Char('k') => Some(AppCommand::NavigateUp),
            KeyCode::Enter => Some(AppCommand::Select),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn shift_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctx(mode: &AppMode, he: bool) -> InputContext<'_> {
        InputContext {
            mode,
            history_expanded: he,
            search_query_len: 0,
            playlist_url_len: 0,
        }
    }

    fn cmd(k: KeyEvent, mode: &AppMode, he: bool) -> Option<AppCommand> {
        key_to_command(k, &ctx(mode, he))
    }

    // -- Normal mode tests --

    #[test]
    fn normal_q_quits() {
        assert_eq!(
            cmd(key(KeyCode::Char('q')), &AppMode::Normal, false),
            Some(AppCommand::Quit)
        );
    }

    #[test]
    fn normal_question_mark_shows_help() {
        assert_eq!(
            cmd(key(KeyCode::Char('?')), &AppMode::Normal, false),
            Some(AppCommand::ShowHelp)
        );
    }

    #[test]
    fn normal_slash_starts_search() {
        assert_eq!(
            cmd(key(KeyCode::Char('/')), &AppMode::Normal, false),
            Some(AppCommand::StartSearch)
        );
    }

    #[test]
    fn normal_space_toggles_pause() {
        assert_eq!(
            cmd(key(KeyCode::Char(' ')), &AppMode::Normal, false),
            Some(AppCommand::TogglePause)
        );
    }

    #[test]
    fn normal_n_next_track() {
        assert_eq!(
            cmd(key(KeyCode::Char('n')), &AppMode::Normal, false),
            Some(AppCommand::NextTrack)
        );
    }

    #[test]
    fn normal_p_previous_track() {
        assert_eq!(
            cmd(key(KeyCode::Char('p')), &AppMode::Normal, false),
            Some(AppCommand::PreviousTrack)
        );
    }

    #[test]
    fn normal_j_navigates_down() {
        assert_eq!(
            cmd(key(KeyCode::Char('j')), &AppMode::Normal, false),
            Some(AppCommand::NavigateDown)
        );
    }

    #[test]
    fn normal_k_navigates_up() {
        assert_eq!(
            cmd(key(KeyCode::Char('k')), &AppMode::Normal, false),
            Some(AppCommand::NavigateUp)
        );
    }

    #[test]
    fn normal_d_deletes() {
        assert_eq!(
            cmd(key(KeyCode::Char('d')), &AppMode::Normal, false),
            Some(AppCommand::Delete)
        );
    }

    #[test]
    fn normal_shift_h_toggles_history() {
        assert_eq!(
            cmd(shift_key(KeyCode::Char('H')), &AppMode::Normal, false),
            Some(AppCommand::ToggleHistoryExpand)
        );
    }

    #[test]
    fn normal_shift_c_clears_history_when_expanded() {
        assert_eq!(
            cmd(shift_key(KeyCode::Char('C')), &AppMode::Normal, true),
            Some(AppCommand::ClearHistory)
        );
    }

    #[test]
    fn normal_shift_c_noop_when_history_collapsed() {
        assert_eq!(
            cmd(shift_key(KeyCode::Char('C')), &AppMode::Normal, false),
            None
        );
    }

    #[test]
    fn normal_up_volume_up() {
        assert_eq!(
            cmd(key(KeyCode::Up), &AppMode::Normal, false),
            Some(AppCommand::VolumeUp { big_step: false })
        );
    }

    #[test]
    fn normal_shift_up_big_volume() {
        assert_eq!(
            cmd(shift_key(KeyCode::Up), &AppMode::Normal, false),
            Some(AppCommand::VolumeUp { big_step: true })
        );
    }

    #[test]
    fn normal_enter_selects() {
        assert_eq!(
            cmd(key(KeyCode::Enter), &AppMode::Normal, false),
            Some(AppCommand::Select)
        );
    }

    #[test]
    fn normal_t_toggles_queue() {
        assert_eq!(
            cmd(key(KeyCode::Char('t')), &AppMode::Normal, false),
            Some(AppCommand::ToggleQueueExpand)
        );
    }

    #[test]
    fn normal_m_toggles_mix() {
        assert_eq!(
            cmd(key(KeyCode::Char('m')), &AppMode::Normal, false),
            Some(AppCommand::ToggleMixExpand)
        );
    }

    #[test]
    fn normal_shift_m_refreshes_mix() {
        assert_eq!(
            cmd(shift_key(KeyCode::Char('M')), &AppMode::Normal, false),
            Some(AppCommand::RefreshMix)
        );
    }

    // -- Searching mode tests --

    #[test]
    fn searching_char_appends() {
        assert_eq!(
            key_to_command(
                key(KeyCode::Char('a')),
                &InputContext {
                    mode: &AppMode::Searching,
                    history_expanded: false,
                    search_query_len: 5,
                    playlist_url_len: 0
                }
            ),
            Some(AppCommand::SearchChar('a'))
        );
    }

    #[test]
    fn searching_enter_submits() {
        assert_eq!(
            key_to_command(
                key(KeyCode::Enter),
                &InputContext {
                    mode: &AppMode::Searching,
                    history_expanded: false,
                    search_query_len: 5,
                    playlist_url_len: 0
                }
            ),
            Some(AppCommand::SearchSubmit)
        );
    }

    #[test]
    fn searching_esc_cancels() {
        assert_eq!(
            key_to_command(
                key(KeyCode::Esc),
                &InputContext {
                    mode: &AppMode::Searching,
                    history_expanded: false,
                    search_query_len: 5,
                    playlist_url_len: 0
                }
            ),
            Some(AppCommand::SearchCancel)
        );
    }

    // -- Help mode tests --

    #[test]
    fn help_esc_dismisses() {
        assert_eq!(
            cmd(key(KeyCode::Esc), &AppMode::Help, false),
            Some(AppCommand::DismissHelp)
        );
    }

    // -- Login mode tests --

    #[test]
    fn login_q_quits() {
        assert_eq!(
            cmd(key(KeyCode::Char('q')), &AppMode::LoginPrompt, false),
            Some(AppCommand::Quit)
        );
    }

    #[test]
    fn login_l_starts_login() {
        assert_eq!(
            cmd(key(KeyCode::Char('l')), &AppMode::LoginPrompt, false),
            Some(AppCommand::StartLogin)
        );
    }

    // -- Account picker tests --

    #[test]
    fn account_picker_enter_selects() {
        assert_eq!(
            cmd(key(KeyCode::Enter), &AppMode::AccountPicker, false),
            Some(AppCommand::SelectAccount)
        );
    }

    #[test]
    fn account_picker_esc_cancels() {
        assert_eq!(
            cmd(key(KeyCode::Esc), &AppMode::AccountPicker, false),
            Some(AppCommand::CancelAccountPicker)
        );
    }

    // -- Playlist loading tests --

    #[test]
    fn playlist_char_appends() {
        assert_eq!(
            key_to_command(
                key(KeyCode::Char('x')),
                &InputContext {
                    mode: &AppMode::LoadingPlaylist,
                    history_expanded: false,
                    search_query_len: 0,
                    playlist_url_len: 10
                }
            ),
            Some(AppCommand::PlaylistChar('x'))
        );
    }

    #[test]
    fn playlist_esc_cancels() {
        assert_eq!(
            key_to_command(
                key(KeyCode::Esc),
                &InputContext {
                    mode: &AppMode::LoadingPlaylist,
                    history_expanded: false,
                    search_query_len: 0,
                    playlist_url_len: 0
                }
            ),
            Some(AppCommand::PlaylistCancel)
        );
    }

    #[test]
    fn unknown_key_returns_none() {
        assert_eq!(cmd(key(KeyCode::F(12)), &AppMode::Normal, false), None);
    }
}
