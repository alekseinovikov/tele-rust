mod app;
mod input;
mod telegram;
mod tui;

use std::time::Duration;

use anyhow::Context;
use app::AppState;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use input::{AppCommand, is_quit_hotkey, map_key_event};
use telegram::{AuthFlow, AuthStatus, TelegramEvent, TelegramRequest, spawn_telegram_task};
use tokio::{sync::mpsc, time::interval};
use tracing::error;
use tui::{AuthView, TerminalGuard, draw, draw_auth};

#[derive(Debug, Clone)]
enum AuthScreen {
    Phone,
    Code,
    Password { hint: Option<String> },
}

#[derive(Debug, Default)]
struct AuthUiState {
    input: String,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .ok();

    let mut auth_flow = AuthFlow::connect_from_env()
        .await
        .context("failed to initialize Telegram client")?;

    let mut terminal_guard = TerminalGuard::new().context("failed to initialize terminal")?;

    if let AuthStatus::Authorized = auth_flow.current_status().await? {
        // Existing session is valid; skip login form.
    } else {
        let authorized = run_auth_loop(terminal_guard.terminal_mut(), &mut auth_flow).await?;
        if !authorized {
            return Ok(());
        }
    }

    let (client, updates_rx) = auth_flow.into_client()?;
    let (req_tx, req_rx) = mpsc::channel(32);
    let (event_tx, mut event_rx) = mpsc::channel(64);

    let telegram_handle = spawn_telegram_task(client, updates_rx, req_rx, event_tx);

    req_tx
        .send(TelegramRequest::LoadDialogs)
        .await
        .context("failed to request initial dialog load")?;

    let mut app = AppState::new();
    let mut events = EventStream::new();
    let mut tick = interval(Duration::from_millis(120));

    while !app.should_quit {
        terminal_guard
            .terminal_mut()
            .draw(|f| draw(f, &app))
            .context("failed to draw frame")?;

        tokio::select! {
            _ = tick.tick() => {}
            maybe_evt = events.next() => {
                if let Some(Ok(CrosstermEvent::Key(key))) = maybe_evt {
                    let selected_before = app.selected_dialog_id();
                    match map_key_event(key, app.ui_mode, app.focus) {
                        AppCommand::MoveUp => {
                            app.select_prev();
                        }
                        AppCommand::MoveDown => {
                            app.select_next();
                        }
                        AppCommand::ScrollMessagesUp => {
                            app.scroll_messages_up();
                        }
                        AppCommand::ScrollMessagesDown => {
                            app.scroll_messages_down();
                        }
                        AppCommand::FocusNext => {
                            app.focus_next();
                        }
                        AppCommand::FocusPrev => {
                            app.focus_prev();
                        }
                        AppCommand::EnterCompose => {
                            app.enter_compose();
                        }
                        AppCommand::ExitComposeOrSearch => match app.ui_mode {
                            app::UiMode::Compose => app.exit_compose(),
                            app::UiMode::Search => app.exit_or_clear_search(),
                            app::UiMode::Normal => {}
                        },
                        AppCommand::SubmitMessage => {
                            request_send_message(&req_tx, &mut app).await;
                        }
                        AppCommand::StartSearch => {
                            app.start_search();
                        }
                        AppCommand::ToggleSortMode => {
                            app.toggle_sort_mode();
                        }
                        AppCommand::Backspace => {
                            app.backspace();
                        }
                        AppCommand::InsertChar(ch) => {
                            app.insert_char(ch);
                        }
                        AppCommand::Quit => {
                            app.should_quit = true;
                        }
                        AppCommand::None => {}
                    }

                    if selected_before != app.selected_dialog_id() {
                        request_messages_for_selected(&req_tx, &mut app).await;
                    }
                }
            }
            maybe_tele = event_rx.recv() => {
                match maybe_tele {
                    Some(TelegramEvent::DialogsLoaded(dialogs)) => {
                        let selected_before = app.selected_dialog_id();
                        app.on_dialogs_loaded(dialogs);
                        let selected_after = app.selected_dialog_id();
                        let should_request_messages =
                            selected_after.is_some()
                                && (selected_before != selected_after
                                    || app.selected_dialog_messages().is_empty());
                        if should_request_messages {
                            request_messages_for_selected(&req_tx, &mut app).await;
                        }
                    }
                    Some(TelegramEvent::MessagesLoaded { dialog_id, messages }) => {
                        if Some(dialog_id) == app.selected_dialog_id() {
                            app.on_messages_loaded(dialog_id, messages);
                        }
                    }
                    Some(TelegramEvent::MessageSent { dialog_id, message }) => {
                        app.on_message_sent(dialog_id, message);
                    }
                    Some(TelegramEvent::IncomingMessage { dialog_id, message }) => {
                        app.on_incoming_message(dialog_id, message);
                    }
                    Some(TelegramEvent::Error(err_msg)) => {
                        app.last_error = Some(err_msg);
                        app.is_loading_dialogs = false;
                        app.is_loading_messages = false;
                        app.is_sending_message = false;
                    }
                    None => {
                        app.last_error = Some("telegram task exited".to_string());
                        app.should_quit = true;
                    }
                }
            }
        }
    }

    let _ = req_tx.send(TelegramRequest::Shutdown).await;

    if let Err(join_err) = telegram_handle.await {
        error!("telegram task join error: {join_err}");
    }

    Ok(())
}

async fn run_auth_loop(
    terminal: &mut tui::AppTerminal,
    auth_flow: &mut AuthFlow,
) -> anyhow::Result<bool> {
    let mut screen = AuthScreen::Phone;
    let mut ui_state = AuthUiState::default();
    let mut events = EventStream::new();
    let mut tick = interval(Duration::from_millis(120));

    loop {
        let (title, prompt, masked, hint) = match &screen {
            AuthScreen::Phone => (
                "Telegram Login",
                "Phone number (international format)",
                false,
                None,
            ),
            AuthScreen::Code => ("Telegram Login", "Login code", false, None),
            AuthScreen::Password { hint } => {
                ("Telegram Login", "2FA password", true, hint.as_deref())
            }
        };

        terminal
            .draw(|f| {
                draw_auth(
                    f,
                    &AuthView {
                        title,
                        prompt,
                        input: &ui_state.input,
                        masked,
                        hint,
                        error: ui_state.error.as_deref(),
                    },
                )
            })
            .context("failed to draw auth screen")?;

        tokio::select! {
            _ = tick.tick() => {}
            maybe_evt = events.next() => {
                if let Some(Ok(CrosstermEvent::Key(key))) = maybe_evt {
                    match handle_auth_key(key, &mut ui_state.input) {
                        AuthKeyAction::Submit => {
                            let value = ui_state.input.trim().to_string();
                            if value.is_empty() {
                                ui_state.error = Some("Input must not be empty".to_string());
                                continue;
                            }

                            let result = match &screen {
                                AuthScreen::Phone => auth_flow.submit_phone(&value).await,
                                AuthScreen::Code => auth_flow.submit_code(&value).await,
                                AuthScreen::Password { .. } => auth_flow.submit_password(&value).await,
                            };

                            match result {
                                Ok(status) => {
                                    ui_state.input.clear();
                                    ui_state.error = None;

                                    match status {
                                        AuthStatus::NeedPhone => {
                                            screen = AuthScreen::Phone;
                                        }
                                        AuthStatus::NeedCode => {
                                            screen = AuthScreen::Code;
                                        }
                                        AuthStatus::NeedPassword { hint } => {
                                            screen = AuthScreen::Password { hint };
                                        }
                                        AuthStatus::Authorized => {
                                            return Ok(true);
                                        }
                                    }
                                }
                                Err(err) => {
                                    ui_state.error = Some(err.to_string());
                                }
                            }
                        }
                        AuthKeyAction::Quit => return Ok(false),
                        AuthKeyAction::None => {}
                    }
                }
            }
        }
    }
}

enum AuthKeyAction {
    Submit,
    Quit,
    None,
}

fn handle_auth_key(key: KeyEvent, input: &mut String) -> AuthKeyAction {
    if key.kind != crossterm::event::KeyEventKind::Press {
        return AuthKeyAction::None;
    }

    match key.code {
        KeyCode::Enter => AuthKeyAction::Submit,
        KeyCode::Backspace => {
            input.pop();
            AuthKeyAction::None
        }
        KeyCode::Char(_) if is_quit_hotkey(key) => AuthKeyAction::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => AuthKeyAction::Quit,
        KeyCode::Esc => AuthKeyAction::Quit,
        KeyCode::Char(ch) => {
            input.push(ch);
            AuthKeyAction::None
        }
        _ => AuthKeyAction::None,
    }
}

async fn request_messages_for_selected(req_tx: &mpsc::Sender<TelegramRequest>, app: &mut AppState) {
    if let Some(dialog_id) = app.selected_dialog_id() {
        app.is_loading_messages = true;
        if let Err(err) = req_tx
            .send(TelegramRequest::LoadMessages {
                dialog_id,
                limit: 50,
            })
            .await
        {
            app.last_error = Some(format!("failed to request messages: {err}"));
            app.is_loading_messages = false;
        }
    }
}

async fn request_send_message(req_tx: &mpsc::Sender<TelegramRequest>, app: &mut AppState) {
    if app.is_sending_message {
        return;
    }

    let Some(dialog_id) = app.selected_dialog_id() else {
        app.last_error = Some("No chat selected".to_string());
        return;
    };

    let text = app.compose_text.trim().to_string();
    if text.is_empty() {
        app.last_error = Some("Message must not be empty".to_string());
        return;
    }

    app.is_sending_message = true;
    app.last_error = None;
    if let Err(err) = req_tx
        .send(TelegramRequest::SendMessage { dialog_id, text })
        .await
    {
        app.last_error = Some(format!("failed to request message send: {err}"));
        app.is_sending_message = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn auth_quit_hotkey_supports_russian_alias() {
        let key = KeyEvent::new(KeyCode::Char('й'), KeyModifiers::NONE);
        let action = handle_auth_key(key, &mut String::new());

        assert!(matches!(action, AuthKeyAction::Quit));
    }

    #[test]
    fn auth_regular_chars_are_still_inserted() {
        let mut input = String::new();
        let key = KeyEvent::new(KeyCode::Char('ф'), KeyModifiers::NONE);
        let action = handle_auth_key(key, &mut input);

        assert!(matches!(action, AuthKeyAction::None));
        assert_eq!(input, "ф");
    }
}
