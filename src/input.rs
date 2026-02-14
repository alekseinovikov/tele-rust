use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::app::{FocusArea, UiMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    MoveUp,
    MoveDown,
    ScrollMessagesUp,
    ScrollMessagesDown,
    FocusNext,
    FocusPrev,
    EnterCompose,
    ExitComposeOrSearch,
    SubmitMessage,
    StartSearch,
    ToggleSortMode,
    Backspace,
    InsertChar(char),
    Quit,
    None,
}

pub fn map_key_event(key: KeyEvent, ui_mode: UiMode, focus: FocusArea) -> AppCommand {
    if key.kind != KeyEventKind::Press {
        return AppCommand::None;
    }

    if key.code == KeyCode::BackTab {
        return AppCommand::FocusPrev;
    }

    match key.code {
        KeyCode::Tab => AppCommand::FocusNext,
        KeyCode::Up => match focus {
            FocusArea::Chats => AppCommand::MoveUp,
            FocusArea::Messages => AppCommand::ScrollMessagesUp,
            FocusArea::Input => AppCommand::None,
        },
        KeyCode::Down => match focus {
            FocusArea::Chats => AppCommand::MoveDown,
            FocusArea::Messages => AppCommand::ScrollMessagesDown,
            FocusArea::Input => AppCommand::None,
        },
        KeyCode::Enter => {
            if ui_mode == UiMode::Compose {
                AppCommand::SubmitMessage
            } else {
                AppCommand::None
            }
        }
        KeyCode::Backspace => AppCommand::Backspace,
        KeyCode::Esc => AppCommand::ExitComposeOrSearch,
        KeyCode::Char('/') if ui_mode != UiMode::Compose => AppCommand::StartSearch,
        KeyCode::Char('s') if focus == FocusArea::Chats && ui_mode != UiMode::Compose => {
            AppCommand::ToggleSortMode
        }
        KeyCode::Char('i') if ui_mode != UiMode::Search => AppCommand::EnterCompose,
        KeyCode::Char('q') => AppCommand::Quit,
        KeyCode::Char(ch) => AppCommand::InsertChar(ch),
        _ => AppCommand::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn key_mapping_is_expected() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let quit = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let slash = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(
            map_key_event(up, UiMode::Normal, FocusArea::Chats),
            AppCommand::MoveUp
        );
        assert_eq!(
            map_key_event(down, UiMode::Normal, FocusArea::Messages),
            AppCommand::ScrollMessagesDown
        );
        assert_eq!(
            map_key_event(quit, UiMode::Normal, FocusArea::Chats),
            AppCommand::Quit
        );
        assert_eq!(
            map_key_event(tab, UiMode::Normal, FocusArea::Chats),
            AppCommand::FocusNext
        );
        assert_eq!(
            map_key_event(slash, UiMode::Normal, FocusArea::Chats),
            AppCommand::StartSearch
        );
        assert_eq!(
            map_key_event(enter, UiMode::Compose, FocusArea::Input),
            AppCommand::SubmitMessage
        );
    }
}
