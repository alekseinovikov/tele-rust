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

const QUIT_HOTKEYS: &[char] = &['q', 'й'];
const COMPOSE_HOTKEYS: &[char] = &['i', 'ш'];
const SORT_HOTKEYS: &[char] = &['s', 'ы'];
const SEARCH_HOTKEYS: &[char] = &['/', '.'];

pub fn is_quit_hotkey(key: KeyEvent) -> bool {
    is_hotkey_char(key, QUIT_HOTKEYS)
}

fn is_compose_hotkey(key: KeyEvent) -> bool {
    is_hotkey_char(key, COMPOSE_HOTKEYS)
}

fn is_sort_hotkey(key: KeyEvent) -> bool {
    is_hotkey_char(key, SORT_HOTKEYS)
}

fn is_search_hotkey(key: KeyEvent) -> bool {
    is_hotkey_char(key, SEARCH_HOTKEYS)
}

fn is_hotkey_char(key: KeyEvent, hotkeys: &[char]) -> bool {
    match key.code {
        KeyCode::Char(ch) => hotkeys.contains(&ch.to_ascii_lowercase()),
        _ => false,
    }
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
        KeyCode::Char(_) if is_search_hotkey(key) && ui_mode != UiMode::Compose => {
            AppCommand::StartSearch
        }
        KeyCode::Char(_)
            if is_sort_hotkey(key) && focus == FocusArea::Chats && ui_mode != UiMode::Compose =>
        {
            AppCommand::ToggleSortMode
        }
        KeyCode::Char(_) if is_compose_hotkey(key) && ui_mode != UiMode::Search => {
            AppCommand::EnterCompose
        }
        KeyCode::Char(_) if is_quit_hotkey(key) && ui_mode == UiMode::Normal => AppCommand::Quit,
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

    #[test]
    fn russian_layout_hotkeys_are_supported() {
        let quit = KeyEvent::new(KeyCode::Char('й'), KeyModifiers::NONE);
        let compose = KeyEvent::new(KeyCode::Char('ш'), KeyModifiers::NONE);
        let sort = KeyEvent::new(KeyCode::Char('ы'), KeyModifiers::NONE);
        let search = KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE);

        assert_eq!(
            map_key_event(quit, UiMode::Normal, FocusArea::Chats),
            AppCommand::Quit
        );
        assert_eq!(
            map_key_event(compose, UiMode::Normal, FocusArea::Chats),
            AppCommand::EnterCompose
        );
        assert_eq!(
            map_key_event(sort, UiMode::Normal, FocusArea::Chats),
            AppCommand::ToggleSortMode
        );
        assert_eq!(
            map_key_event(search, UiMode::Normal, FocusArea::Chats),
            AppCommand::StartSearch
        );
    }

    #[test]
    fn quit_hotkeys_are_text_in_compose_and_search_modes() {
        let quit_en = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let quit_ru = KeyEvent::new(KeyCode::Char('й'), KeyModifiers::NONE);

        assert_eq!(
            map_key_event(quit_en, UiMode::Compose, FocusArea::Input),
            AppCommand::InsertChar('q')
        );
        assert_eq!(
            map_key_event(quit_ru, UiMode::Compose, FocusArea::Input),
            AppCommand::InsertChar('й')
        );
        assert_eq!(
            map_key_event(quit_en, UiMode::Search, FocusArea::Chats),
            AppCommand::InsertChar('q')
        );
        assert_eq!(
            map_key_event(quit_ru, UiMode::Search, FocusArea::Chats),
            AppCommand::InsertChar('й')
        );
    }
}
