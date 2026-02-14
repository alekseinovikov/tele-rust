use std::collections::HashMap;

use crate::telegram::{DialogSummary, MessageSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusArea {
    #[default]
    Chats,
    Messages,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    Recent,
    Alphabetical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Normal,
    Compose,
    Search,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub dialogs: Vec<DialogSummary>,
    pub selected_dialog_id: Option<i64>,
    pub messages_by_dialog: HashMap<i64, Vec<MessageSummary>>,
    pub new_message_count_by_dialog: HashMap<i64, usize>,
    pub is_loading_dialogs: bool,
    pub is_loading_messages: bool,
    pub is_sending_message: bool,
    pub last_error: Option<String>,
    pub should_quit: bool,
    pub focus: FocusArea,
    pub sort_mode: SortMode,
    pub ui_mode: UiMode,
    pub search_query: String,
    pub compose_text: String,
    pub message_scroll_from_bottom: usize,
    pub pending_new_messages_for_selected: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            is_loading_dialogs: true,
            is_loading_messages: false,
            ..Self::default()
        }
    }

    pub fn on_dialogs_loaded(&mut self, dialogs: Vec<DialogSummary>) {
        self.dialogs = dialogs;
        self.new_message_count_by_dialog
            .retain(|dialog_id, _| self.dialogs.iter().any(|dialog| dialog.id == *dialog_id));
        self.is_loading_dialogs = false;
        self.ensure_selection();
    }

    pub fn on_messages_loaded(&mut self, dialog_id: i64, messages: Vec<MessageSummary>) {
        self.messages_by_dialog.insert(dialog_id, messages);
        self.new_message_count_by_dialog.remove(&dialog_id);
        self.is_loading_messages = false;
        if Some(dialog_id) == self.selected_dialog_id {
            self.message_scroll_from_bottom = 0;
            self.pending_new_messages_for_selected = 0;
        }
    }

    pub fn on_message_sent(&mut self, dialog_id: i64, message: MessageSummary) {
        self.append_message_if_missing(dialog_id, message);
        self.is_sending_message = false;
        self.compose_text.clear();
        self.last_error = None;
    }

    pub fn on_incoming_message(&mut self, dialog_id: i64, message: MessageSummary) {
        if !self.append_message_if_missing(dialog_id, message) {
            return;
        }

        if Some(dialog_id) == self.selected_dialog_id {
            if self.message_scroll_from_bottom > 0 {
                self.pending_new_messages_for_selected =
                    self.pending_new_messages_for_selected.saturating_add(1);
            }
        } else {
            let counter = self
                .new_message_count_by_dialog
                .entry(dialog_id)
                .or_insert(0);
            *counter = counter.saturating_add(1);
        }
    }

    pub fn dialog_new_message_count(&self, dialog_id: i64) -> usize {
        self.new_message_count_by_dialog
            .get(&dialog_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn selected_dialog(&self) -> Option<&DialogSummary> {
        self.selected_dialog_id
            .and_then(|id| self.dialogs.iter().find(|dialog| dialog.id == id))
    }

    pub fn selected_dialog_id(&self) -> Option<i64> {
        self.selected_dialog_id
    }

    pub fn selected_dialog_messages(&self) -> &[MessageSummary] {
        if let Some(dialog_id) = self.selected_dialog_id() {
            self.messages_by_dialog
                .get(&dialog_id)
                .map(Vec::as_slice)
                .unwrap_or(&[])
        } else {
            &[]
        }
    }

    pub fn select_prev(&mut self) -> bool {
        let visible = self.visible_dialog_ids();
        if visible.is_empty() {
            return false;
        }

        let Some(current_id) = self.selected_dialog_id else {
            self.selected_dialog_id = Some(visible[0]);
            self.pending_new_messages_for_selected = 0;
            return true;
        };

        let Some(pos) = visible.iter().position(|id| *id == current_id) else {
            self.selected_dialog_id = Some(visible[0]);
            self.pending_new_messages_for_selected = 0;
            return true;
        };

        if pos == 0 {
            return false;
        }

        self.selected_dialog_id = Some(visible[pos - 1]);
        self.message_scroll_from_bottom = 0;
        self.pending_new_messages_for_selected = 0;
        true
    }

    pub fn select_next(&mut self) -> bool {
        let visible = self.visible_dialog_ids();
        if visible.is_empty() {
            return false;
        }

        let Some(current_id) = self.selected_dialog_id else {
            self.selected_dialog_id = Some(visible[0]);
            self.pending_new_messages_for_selected = 0;
            return true;
        };

        let Some(pos) = visible.iter().position(|id| *id == current_id) else {
            self.selected_dialog_id = Some(visible[0]);
            self.pending_new_messages_for_selected = 0;
            return true;
        };

        if pos + 1 >= visible.len() {
            return false;
        }

        self.selected_dialog_id = Some(visible[pos + 1]);
        self.message_scroll_from_bottom = 0;
        self.pending_new_messages_for_selected = 0;
        true
    }

    pub fn visible_dialogs(&self) -> Vec<&DialogSummary> {
        let mut dialogs: Vec<&DialogSummary> = self
            .dialogs
            .iter()
            .filter(|dialog| self.matches_query(dialog))
            .collect();

        if self.sort_mode == SortMode::Alphabetical {
            dialogs.sort_by(|a, b| {
                a.title
                    .to_lowercase()
                    .cmp(&b.title.to_lowercase())
                    .then(a.id.cmp(&b.id))
            });
        }

        dialogs
    }

    pub fn selected_visible_index(&self) -> Option<usize> {
        let selected_id = self.selected_dialog_id?;
        self.visible_dialog_ids()
            .iter()
            .position(|id| *id == selected_id)
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            FocusArea::Chats => FocusArea::Messages,
            FocusArea::Messages => FocusArea::Input,
            FocusArea::Input => FocusArea::Chats,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            FocusArea::Chats => FocusArea::Input,
            FocusArea::Messages => FocusArea::Chats,
            FocusArea::Input => FocusArea::Messages,
        };
    }

    pub fn enter_compose(&mut self) {
        self.ui_mode = UiMode::Compose;
        self.focus = FocusArea::Input;
    }

    pub fn exit_compose(&mut self) {
        self.ui_mode = UiMode::Normal;
    }

    pub fn start_search(&mut self) {
        self.ui_mode = UiMode::Search;
        self.focus = FocusArea::Chats;
    }

    pub fn exit_or_clear_search(&mut self) {
        if self.search_query.is_empty() {
            self.ui_mode = UiMode::Normal;
        } else {
            self.search_query.clear();
            self.ui_mode = UiMode::Normal;
            self.ensure_selection();
        }
    }

    pub fn toggle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Recent => SortMode::Alphabetical,
            SortMode::Alphabetical => SortMode::Recent,
        };
        self.ensure_selection();
    }

    pub fn insert_char(&mut self, ch: char) {
        match self.ui_mode {
            UiMode::Compose => self.compose_text.push(ch),
            UiMode::Search => {
                self.search_query.push(ch);
                self.ensure_selection();
            }
            UiMode::Normal => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.ui_mode {
            UiMode::Compose => {
                self.compose_text.pop();
            }
            UiMode::Search => {
                self.search_query.pop();
                self.ensure_selection();
            }
            UiMode::Normal => {}
        }
    }

    pub fn scroll_messages_up(&mut self) {
        self.message_scroll_from_bottom = self.message_scroll_from_bottom.saturating_add(1);
    }

    pub fn scroll_messages_down(&mut self) {
        if self.message_scroll_from_bottom > 0 {
            self.message_scroll_from_bottom -= 1;
            if self.message_scroll_from_bottom == 0 {
                self.pending_new_messages_for_selected = 0;
            }
        }
    }

    fn matches_query(&self, dialog: &DialogSummary) -> bool {
        if self.search_query.is_empty() {
            return true;
        }

        dialog
            .title
            .to_lowercase()
            .contains(&self.search_query.to_lowercase())
    }

    fn visible_dialog_ids(&self) -> Vec<i64> {
        self.visible_dialogs()
            .iter()
            .map(|dialog| dialog.id)
            .collect()
    }

    fn append_message_if_missing(&mut self, dialog_id: i64, message: MessageSummary) -> bool {
        let messages = self.messages_by_dialog.entry(dialog_id).or_default();
        if messages.iter().any(|existing| existing.id == message.id) {
            return false;
        }
        messages.push(message);
        true
    }

    fn ensure_selection(&mut self) {
        let visible = self.visible_dialog_ids();
        if visible.is_empty() {
            self.selected_dialog_id = None;
            return;
        }

        if !self
            .selected_dialog_id
            .is_some_and(|id| visible.contains(&id))
        {
            self.selected_dialog_id = Some(visible[0]);
            self.message_scroll_from_bottom = 0;
            self.pending_new_messages_for_selected = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialogs() -> Vec<DialogSummary> {
        vec![
            DialogSummary {
                id: 1,
                title: "a".to_string(),
            },
            DialogSummary {
                id: 2,
                title: "b".to_string(),
            },
        ]
    }

    fn message(id: i32, text: &str) -> MessageSummary {
        MessageSummary {
            id,
            from: "x".to_string(),
            text: text.to_string(),
            date: "now".to_string(),
        }
    }

    #[test]
    fn selection_bounds_are_clamped() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());

        assert!(!app.select_prev());
        assert_eq!(app.selected_dialog_id(), Some(1));

        assert!(app.select_next());
        assert_eq!(app.selected_dialog_id(), Some(2));

        assert!(!app.select_next());
        assert_eq!(app.selected_dialog_id(), Some(2));
    }

    #[test]
    fn selected_dialog_id_updates_with_selection() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());

        assert_eq!(app.selected_dialog_id(), Some(1));
        app.select_next();
        assert_eq!(app.selected_dialog_id(), Some(2));
    }

    #[test]
    fn messages_are_stored_by_dialog() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());
        app.on_messages_loaded(1, vec![message(1, "hello")]);

        assert_eq!(app.selected_dialog_messages().len(), 1);
        app.select_next();
        assert!(app.selected_dialog_messages().is_empty());
    }

    #[test]
    fn search_filters_visible_dialogs() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());

        app.start_search();
        app.insert_char('b');

        assert_eq!(app.visible_dialogs().len(), 1);
        assert_eq!(app.selected_dialog_id(), Some(2));
    }

    #[test]
    fn sorting_can_toggle_to_alphabetical() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(vec![
            DialogSummary {
                id: 1,
                title: "zulu".to_string(),
            },
            DialogSummary {
                id: 2,
                title: "alpha".to_string(),
            },
        ]);

        app.toggle_sort_mode();
        let visible = app.visible_dialogs();
        assert_eq!(visible[0].title, "alpha");
        assert_eq!(visible[1].title, "zulu");
    }

    #[test]
    fn message_scroll_is_bottom_relative() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());
        app.on_messages_loaded(
            1,
            vec![message(1, "one"), message(2, "two"), message(3, "three")],
        );

        assert_eq!(app.message_scroll_from_bottom, 0);
        app.scroll_messages_up();
        assert_eq!(app.message_scroll_from_bottom, 1);
        app.scroll_messages_down();
        assert_eq!(app.message_scroll_from_bottom, 0);
    }

    #[test]
    fn message_scroll_resets_when_selecting_another_chat() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());
        app.on_messages_loaded(1, vec![message(1, "hello")]);
        app.message_scroll_from_bottom = 1;

        app.select_next();

        assert_eq!(app.message_scroll_from_bottom, 0);
    }

    #[test]
    fn incoming_non_selected_chat_increments_badge() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());

        app.on_incoming_message(2, message(10, "hello"));

        assert_eq!(app.dialog_new_message_count(2), 1);
        assert_eq!(app.pending_new_messages_for_selected, 0);
    }

    #[test]
    fn incoming_selected_chat_while_at_bottom_does_not_set_badges() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());

        app.on_incoming_message(1, message(10, "hello"));

        assert_eq!(app.dialog_new_message_count(1), 0);
        assert_eq!(app.pending_new_messages_for_selected, 0);
    }

    #[test]
    fn incoming_selected_chat_while_scrolled_up_sets_pending_indicator() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());
        app.on_messages_loaded(1, vec![message(1, "first")]);
        app.scroll_messages_up();

        app.on_incoming_message(1, message(10, "hello"));

        assert_eq!(app.pending_new_messages_for_selected, 1);
        assert_eq!(app.dialog_new_message_count(1), 0);
    }

    #[test]
    fn pending_indicator_clears_when_scrolled_to_bottom() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());
        app.on_messages_loaded(1, vec![message(1, "first")]);
        app.scroll_messages_up();
        app.on_incoming_message(1, message(10, "hello"));

        app.scroll_messages_down();

        assert_eq!(app.pending_new_messages_for_selected, 0);
    }

    #[test]
    fn dialog_badge_clears_when_messages_loaded() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());
        app.on_incoming_message(2, message(10, "hello"));
        app.select_next();

        app.on_messages_loaded(2, vec![message(10, "hello"), message(11, "new")]);

        assert_eq!(app.dialog_new_message_count(2), 0);
    }

    #[test]
    fn duplicate_message_id_is_not_appended_twice() {
        let mut app = AppState::new();
        app.on_dialogs_loaded(dialogs());

        app.on_message_sent(1, message(42, "hello"));
        app.on_incoming_message(1, message(42, "hello"));

        assert_eq!(app.selected_dialog_messages().len(), 1);
    }
}
