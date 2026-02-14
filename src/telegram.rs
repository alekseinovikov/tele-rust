use std::{collections::HashMap, env, sync::Arc};

use anyhow::{Context, anyhow};
use grammers_client::{
    Client, SignInError, Update, UpdatesConfiguration,
    types::{LoginToken, Media, Message, PasswordToken},
};
use grammers_mtsender::SenderPool;
use grammers_session::{defs::PeerRef, storages::SqliteSession};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
    time::{Duration, interval},
};

const SESSION_FILE: &str = "telegram.session";
const DIALOG_REFRESH_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct DialogSummary {
    pub id: i64,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct MessageSummary {
    pub id: i32,
    pub from: String,
    pub text: String,
    pub date: String,
}

#[derive(Debug)]
pub enum TelegramRequest {
    LoadDialogs,
    LoadMessages { dialog_id: i64, limit: usize },
    SendMessage { dialog_id: i64, text: String },
    Shutdown,
}

#[derive(Debug)]
pub enum TelegramEvent {
    DialogsLoaded(Vec<DialogSummary>),
    MessagesLoaded {
        dialog_id: i64,
        messages: Vec<MessageSummary>,
    },
    MessageSent {
        dialog_id: i64,
        message: MessageSummary,
    },
    IncomingMessage {
        dialog_id: i64,
        message: MessageSummary,
    },
    Error(String),
}

#[derive(Debug)]
pub enum AuthStatus {
    NeedPhone,
    NeedCode,
    NeedPassword { hint: Option<String> },
    Authorized,
}

pub struct AuthFlow {
    client: Client,
    updates_rx: Option<UnboundedReceiver<grammers_session::updates::UpdatesLike>>,
    api_hash: String,
    login_token: Option<LoginToken>,
    password_token: Option<PasswordToken>,
}

impl AuthFlow {
    pub async fn connect_from_env() -> anyhow::Result<Self> {
        let api_id = read_api_id()?;
        let api_hash = read_api_hash()?;

        let session = Arc::new(SqliteSession::open(SESSION_FILE).context("open session file")?);
        let pool = SenderPool::new(Arc::clone(&session), api_id);
        let client = Client::new(&pool);
        let SenderPool {
            runner, updates, ..
        } = pool;
        tokio::spawn(runner.run());

        Ok(Self {
            client,
            updates_rx: Some(updates),
            api_hash,
            login_token: None,
            password_token: None,
        })
    }

    pub async fn current_status(&self) -> anyhow::Result<AuthStatus> {
        if self.client.is_authorized().await? {
            Ok(AuthStatus::Authorized)
        } else {
            Ok(AuthStatus::NeedPhone)
        }
    }

    pub async fn submit_phone(&mut self, phone: &str) -> anyhow::Result<AuthStatus> {
        let token = self
            .client
            .request_login_code(phone, &self.api_hash)
            .await
            .context("request login code")?;

        self.login_token = Some(token);
        Ok(AuthStatus::NeedCode)
    }

    pub async fn submit_code(&mut self, code: &str) -> anyhow::Result<AuthStatus> {
        let token = self
            .login_token
            .as_ref()
            .ok_or_else(|| anyhow!("login code was not requested yet"))?;

        match self.client.sign_in(token, code).await {
            Ok(_) => {
                self.login_token = None;
                Ok(AuthStatus::Authorized)
            }
            Err(SignInError::PasswordRequired(password_token)) => {
                let hint = password_token.hint().map(ToOwned::to_owned);
                self.password_token = Some(password_token);
                Ok(AuthStatus::NeedPassword { hint })
            }
            Err(SignInError::InvalidCode) => Err(anyhow!("invalid login code")),
            Err(SignInError::InvalidPassword) => Err(anyhow!("invalid 2FA password")),
            Err(SignInError::SignUpRequired { .. }) => {
                Err(anyhow!("this phone is not registered on Telegram"))
            }
            Err(SignInError::Other(err)) => Err(anyhow!(err).context("sign-in failure")),
        }
    }

    pub async fn submit_password(&mut self, password: &str) -> anyhow::Result<AuthStatus> {
        let token = self
            .password_token
            .take()
            .ok_or_else(|| anyhow!("2FA password was not requested"))?;

        self.client
            .check_password(token, password)
            .await
            .context("2FA password check")?;

        self.login_token = None;
        Ok(AuthStatus::Authorized)
    }

    pub fn into_client(
        mut self,
    ) -> anyhow::Result<(
        Client,
        UnboundedReceiver<grammers_session::updates::UpdatesLike>,
    )> {
        let updates_rx = self
            .updates_rx
            .take()
            .ok_or_else(|| anyhow!("telegram updates receiver was already taken"))?;
        Ok((self.client, updates_rx))
    }
}

pub fn spawn_telegram_task(
    client: Client,
    updates_rx: UnboundedReceiver<grammers_session::updates::UpdatesLike>,
    req_rx: mpsc::Receiver<TelegramRequest>,
    event_tx: mpsc::Sender<TelegramEvent>,
) -> JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { run_request_loop(client, updates_rx, req_rx, event_tx).await })
}

async fn run_request_loop(
    client: Client,
    updates_rx: UnboundedReceiver<grammers_session::updates::UpdatesLike>,
    mut req_rx: mpsc::Receiver<TelegramRequest>,
    event_tx: mpsc::Sender<TelegramEvent>,
) -> anyhow::Result<()> {
    let mut chat_map: HashMap<i64, PeerRef> = HashMap::new();
    let mut dialogs_dirty = false;
    let mut updates = client.stream_updates(
        updates_rx,
        UpdatesConfiguration {
            catch_up: true,
            ..Default::default()
        },
    );
    let mut refresh_tick = interval(DIALOG_REFRESH_INTERVAL);

    loop {
        tokio::select! {
            maybe_req = req_rx.recv() => {
                let Some(req) = maybe_req else {
                    break;
                };

                match req {
                    TelegramRequest::LoadDialogs => {
                        let result = load_dialogs(&client, &mut chat_map).await;
                        match result {
                            Ok(dialogs) => {
                                let _ = event_tx.send(TelegramEvent::DialogsLoaded(dialogs)).await;
                            }
                            Err(err) => {
                                let _ = event_tx.send(TelegramEvent::Error(err.to_string())).await;
                            }
                        }
                    }
                    TelegramRequest::LoadMessages { dialog_id, limit } => {
                        let result = load_messages(&client, &chat_map, dialog_id, limit).await;
                        match result {
                            Ok(messages) => {
                                let _ = event_tx
                                    .send(TelegramEvent::MessagesLoaded {
                                        dialog_id,
                                        messages,
                                    })
                                    .await;
                            }
                            Err(err) => {
                                let _ = event_tx.send(TelegramEvent::Error(err.to_string())).await;
                            }
                        }
                    }
                    TelegramRequest::SendMessage { dialog_id, text } => {
                        let result = send_message(&client, &chat_map, dialog_id, &text).await;
                        match result {
                            Ok(message) => {
                                let _ = event_tx
                                    .send(TelegramEvent::MessageSent { dialog_id, message })
                                    .await;
                            }
                            Err(err) => {
                                let _ = event_tx.send(TelegramEvent::Error(err.to_string())).await;
                            }
                        }
                    }
                    TelegramRequest::Shutdown => break,
                }
            }
            update_result = updates.next() => {
                match update_result {
                    Ok(Update::NewMessage(message)) if !message.outgoing() => {
                        let dialog_id = message.peer_id().bot_api_dialog_id();
                        let event = TelegramEvent::IncomingMessage {
                            dialog_id,
                            message: summarize_message(&message),
                        };
                        let _ = event_tx.send(event).await;
                        dialogs_dirty = true;
                    }
                    Ok(_) => {}
                    Err(err) => {
                        let _ = event_tx.send(TelegramEvent::Error(err.to_string())).await;
                        break;
                    }
                }
            }
            _ = refresh_tick.tick() => {
                if !dialogs_dirty {
                    continue;
                }

                match load_dialogs(&client, &mut chat_map).await {
                    Ok(dialogs) => {
                        let _ = event_tx.send(TelegramEvent::DialogsLoaded(dialogs)).await;
                        dialogs_dirty = false;
                    }
                    Err(err) => {
                        let _ = event_tx.send(TelegramEvent::Error(err.to_string())).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn load_dialogs(
    client: &Client,
    chat_map: &mut HashMap<i64, PeerRef>,
) -> anyhow::Result<Vec<DialogSummary>> {
    let mut dialogs = client.iter_dialogs();
    let mut out = Vec::new();
    chat_map.clear();

    while let Some(dialog) = dialogs.next().await? {
        let peer = dialog.peer().clone();
        let dialog_id = peer.id().bot_api_dialog_id();
        let title = peer.name().unwrap_or("Unknown").to_string();

        chat_map.insert(dialog_id, PeerRef::from(&peer));
        out.push(DialogSummary {
            id: dialog_id,
            title,
        });
    }

    Ok(out)
}

async fn load_messages(
    client: &Client,
    chat_map: &HashMap<i64, PeerRef>,
    dialog_id: i64,
    limit: usize,
) -> anyhow::Result<Vec<MessageSummary>> {
    let peer = chat_map
        .get(&dialog_id)
        .ok_or_else(|| anyhow!("selected chat is not available in cache"))?;

    let mut iter = client.iter_messages(*peer).limit(limit);
    let mut messages = Vec::new();

    while let Some(msg) = iter.next().await? {
        messages.push(summarize_message(&msg));
    }

    messages.reverse();
    Ok(messages)
}

async fn send_message(
    client: &Client,
    chat_map: &HashMap<i64, PeerRef>,
    dialog_id: i64,
    text: &str,
) -> anyhow::Result<MessageSummary> {
    let peer = chat_map
        .get(&dialog_id)
        .ok_or_else(|| anyhow!("selected chat is not available in cache"))?;

    let sent = client
        .send_message(*peer, text)
        .await
        .context("send message")?;

    Ok(summarize_message(&sent))
}

fn summarize_message(message: &Message) -> MessageSummary {
    let from = message
        .sender()
        .and_then(|sender| sender.name().map(ToOwned::to_owned))
        .unwrap_or_else(|| "Unknown".to_string());

    MessageSummary {
        id: message.id(),
        from,
        text: summarize_message_text(message),
        date: message.date().to_string(),
    }
}

fn summarize_message_text(message: &Message) -> String {
    if !message.text().trim().is_empty() {
        return message.text().to_string();
    }

    if let Some(media) = message.media() {
        match media {
            Media::Sticker(sticker) => {
                let emoji = sticker.emoji().trim();
                if !emoji.is_empty() {
                    format!("sticker: {emoji}")
                } else if sticker.is_animated() {
                    "sticker: animated".to_string()
                } else {
                    "sticker: sticker".to_string()
                }
            }
            _ => "[media]".to_string(),
        }
    } else {
        String::new()
    }
}

fn read_api_id() -> anyhow::Result<i32> {
    let raw = env::var("TELEGRAM_API_ID")
        .context("TELEGRAM_API_ID is not set. Export it before running the app")?;
    raw.parse::<i32>()
        .context("TELEGRAM_API_ID must be a valid integer")
}

fn read_api_hash() -> anyhow::Result<String> {
    env::var("TELEGRAM_API_HASH")
        .context("TELEGRAM_API_HASH is not set. Export it before running the app")
}
