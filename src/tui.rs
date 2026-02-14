use std::io;

use crossterm::{
    ExecutableCommand, cursor,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Margin, Rect},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
};

use crate::app::{AppState, FocusArea, SortMode, UiMode};

pub type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub struct TerminalGuard {
    terminal: AppTerminal,
}

impl TerminalGuard {
    pub fn new() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;

        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(cursor::Hide)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    pub fn terminal_mut(&mut self) -> &mut AppTerminal {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        let _ = self.terminal.backend_mut().execute(cursor::Show);
    }
}

pub fn draw(frame: &mut Frame<'_>, app: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);

    let chats_title = if app.search_query.is_empty() {
        format!("Chats [{}]", sort_label(app.sort_mode))
    } else {
        format!(
            "Chats [{}] /{}",
            sort_label(app.sort_mode),
            app.search_query
        )
    };

    let chats_block = Block::default()
        .borders(Borders::ALL)
        .title(chats_title)
        .border_style(focus_style(app, FocusArea::Chats));

    let visible_dialogs = app.visible_dialogs();
    let chat_items: Vec<ListItem<'_>> = visible_dialogs
        .iter()
        .map(|dialog| {
            let badge = app.dialog_new_message_count(dialog.id);
            if badge > 0 {
                ListItem::new(format!("{} [{}]", dialog.title, badge))
            } else {
                ListItem::new(dialog.title.clone())
            }
        })
        .collect();
    let has_chat_items = !chat_items.is_empty();

    let chats = List::new(chat_items)
        .block(chats_block)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut list_state = ListState::default();
    if has_chat_items {
        list_state.select(app.selected_visible_index());
    }
    frame.render_stateful_widget(chats, panes[0], &mut list_state);
    maybe_render_scrollbar(
        frame,
        panes[0],
        visible_dialogs.len(),
        list_state.offset(),
        list_inner_height(panes[0]),
    );

    let title = app
        .selected_dialog()
        .map(|d| {
            if app.pending_new_messages_for_selected > 0 {
                format!(
                    "Messages - {} ({} new)",
                    d.title, app.pending_new_messages_for_selected
                )
            } else {
                format!("Messages - {}", d.title)
            }
        })
        .unwrap_or_else(|| "Messages".to_string());

    let right_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_style(app, FocusArea::Messages));

    if app.is_loading_dialogs {
        let paragraph = Paragraph::new("Loading chats...".to_string())
            .block(right_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, panes[1]);
    } else if app.is_loading_messages {
        let paragraph = Paragraph::new("Loading messages...".to_string())
            .block(right_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, panes[1]);
    } else if let Some(err) = &app.last_error {
        let paragraph = Paragraph::new(format!("Error: {err}"))
            .block(right_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, panes[1]);
    } else if visible_dialogs.is_empty() {
        let paragraph = Paragraph::new("No chats match search.".to_string())
            .block(right_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, panes[1]);
    } else {
        let lines: Vec<String> = app
            .selected_dialog_messages()
            .iter()
            .map(|message| format!("[{}] {}: {}", message.date, message.from, message.text))
            .collect();

        if lines.is_empty() {
            let paragraph = Paragraph::new("No messages for selected chat.".to_string())
                .block(right_block)
                .wrap(Wrap { trim: false });
            frame.render_widget(paragraph, panes[1]);
        } else {
            let viewport_height = list_inner_height(panes[1]);
            let viewport_width = list_inner_width(panes[1]);
            let content_lines = total_wrapped_line_count(&lines, viewport_width);
            let message_top_offset = message_top_offset(
                content_lines,
                viewport_height,
                app.message_scroll_from_bottom,
            );
            let body = lines.join("\n");
            let paragraph = Paragraph::new(body)
                .block(right_block)
                .scroll((to_u16_saturating(message_top_offset), 0))
                .wrap(Wrap { trim: false });
            frame.render_widget(paragraph, panes[1]);
            maybe_render_scrollbar(
                frame,
                panes[1],
                content_lines,
                message_top_offset,
                viewport_height,
            );
        }
    }

    let input_title = if app.is_sending_message {
        "Input (sending...)"
    } else {
        "Input"
    };
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(input_title)
        .border_style(focus_style(app, FocusArea::Input));
    let input_text = if app.compose_text.is_empty() {
        "Press i to start typing".to_string()
    } else {
        app.compose_text.clone()
    };
    let input_style = if app.compose_text.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let input = Paragraph::new(input_text)
        .style(input_style)
        .block(input_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(input, outer[1]);

    let help = Paragraph::new(hotkeys_text(app)).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, outer[2]);
}

fn sort_label(sort_mode: SortMode) -> &'static str {
    match sort_mode {
        SortMode::Recent => "Recent",
        SortMode::Alphabetical => "A-Z",
    }
}

fn focus_style(app: &AppState, area: FocusArea) -> Style {
    if app.focus == area {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn hotkeys_text(app: &AppState) -> &'static str {
    match app.ui_mode {
        UiMode::Compose => {
            "Type message | Enter send | Esc stop compose | Tab/Shift+Tab focus | q/й quit"
        }
        UiMode::Search => {
            "Search chats | Type to filter | Backspace edit | Esc clear/exit | Up/Down select | q/й quit"
        }
        UiMode::Normal => match app.focus {
            FocusArea::Chats => {
                "Tab/Shift+Tab focus | Up/Down select chat | i/ш compose | / or . search | s/ы sort | q/й quit"
            }
            FocusArea::Messages => {
                "Tab/Shift+Tab focus | Up/Down scroll messages | i/ш compose | / or . search | q/й quit"
            }
            FocusArea::Input => "Tab/Shift+Tab focus | i/ш compose | / or . search | q/й quit",
        },
    }
}

fn list_inner_height(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(2))
}

fn list_inner_width(area: Rect) -> usize {
    usize::from(area.width.saturating_sub(2))
}

fn total_wrapped_line_count(lines: &[String], width: usize) -> usize {
    if width == 0 {
        return 0;
    }

    lines
        .iter()
        .map(|line| wrapped_line_count(line, width))
        .sum::<usize>()
}

fn wrapped_line_count(line: &str, width: usize) -> usize {
    if width == 0 {
        return 0;
    }

    line.split('\n')
        .map(|segment| {
            let segment_width = segment.chars().count();
            let wrapped = segment_width.div_ceil(width);
            wrapped.max(1)
        })
        .sum::<usize>()
}

fn message_top_offset(
    total_lines: usize,
    viewport_height: usize,
    scroll_from_bottom: usize,
) -> usize {
    let max_top_offset = total_lines.saturating_sub(viewport_height);
    max_top_offset.saturating_sub(scroll_from_bottom.min(max_top_offset))
}

fn maybe_render_scrollbar(
    frame: &mut Frame<'_>,
    area: Rect,
    content_length: usize,
    position: usize,
    viewport_content_length: usize,
) {
    if content_length <= viewport_content_length || viewport_content_length == 0 {
        return;
    }

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    let mut state = ScrollbarState::new(content_length)
        .position(position)
        .viewport_content_length(viewport_content_length);
    frame.render_stateful_widget(
        scrollbar,
        area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut state,
    );
}

fn to_u16_saturating(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

pub struct AuthView<'a> {
    pub title: &'a str,
    pub prompt: &'a str,
    pub input: &'a str,
    pub masked: bool,
    pub hint: Option<&'a str>,
    pub error: Option<&'a str>,
}

pub fn draw_auth(frame: &mut Frame<'_>, view: &AuthView<'_>) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(10),
            Constraint::Percentage(65),
        ])
        .split(frame.area());

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(outer[1]);

    let rendered_input = if view.masked {
        "*".repeat(view.input.chars().count())
    } else {
        view.input.to_string()
    };

    let mut lines = vec![
        format!("{}:", view.prompt),
        rendered_input,
        String::new(),
        "Press Enter to continue".to_string(),
        "Press q/й to quit".to_string(),
    ];

    if let Some(hint) = view.hint {
        lines.insert(3, format!("Hint: {hint}"));
    }

    if let Some(err) = view.error {
        lines.push(String::new());
        lines.push(format!("Error: {err}"));
    }

    let body = lines.join("\n");
    let block = Block::default().borders(Borders::ALL).title(view.title);
    let paragraph = Paragraph::new(body)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, middle[1]);
}

#[cfg(test)]
mod tests {
    use super::{message_top_offset, total_wrapped_line_count, wrapped_line_count};

    #[test]
    fn message_offset_is_bottom_aligned_by_default() {
        assert_eq!(message_top_offset(10, 4, 0), 6);
    }

    #[test]
    fn message_offset_moves_up_when_scrolling_history() {
        assert_eq!(message_top_offset(10, 4, 2), 4);
    }

    #[test]
    fn message_offset_clamps_when_scrolled_too_far() {
        assert_eq!(message_top_offset(10, 4, 100), 0);
    }

    #[test]
    fn message_offset_is_zero_when_content_fits() {
        assert_eq!(message_top_offset(3, 10, 0), 0);
    }

    #[test]
    fn wrapped_line_count_respects_width() {
        assert_eq!(wrapped_line_count("abcd", 2), 2);
    }

    #[test]
    fn wrapped_line_count_handles_newlines() {
        assert_eq!(wrapped_line_count("ab\ncdef", 2), 3);
    }

    #[test]
    fn total_wrapped_line_count_sums_lines() {
        let lines = vec!["abc".to_string(), "defgh".to_string()];
        assert_eq!(total_wrapped_line_count(&lines, 2), 5);
    }
}
