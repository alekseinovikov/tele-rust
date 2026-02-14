# tele-rust

`tele-rust` is a terminal Telegram client written in Rust. It uses `grammers` for Telegram API access and `ratatui` + `crossterm` for the terminal UI.

## Features

- Interactive login flow (phone, login code, optional 2FA password)
- Chat list and message view in a terminal UI
- Send messages to the selected chat
- Incremental updates for incoming messages
- Chat search and sort modes
- Keyboard-first navigation (including Russian-layout hotkeys)

## Prerequisites

- Rust toolchain (stable), with `cargo`
- A Telegram API application:
  - `TELEGRAM_API_ID`
  - `TELEGRAM_API_HASH`

Create Telegram API credentials at: <https://my.telegram.org>

## Setup

1. Clone the repository.
2. Create a `.env` file in the project root:

```env
TELEGRAM_API_ID=123456
TELEGRAM_API_HASH=your_api_hash_here
```

The app loads `.env` automatically on startup.

## Build

```bash
cargo check
cargo build
```

## Run

```bash
cargo run
```

On first run, you will be prompted in the terminal to enter:

1. Phone number (international format)
2. Login code from Telegram
3. 2FA password (if enabled)

After successful auth, a local `telegram.session` file is created and reused for future launches.

## Controls

- `Tab` / `Shift+Tab`: cycle focus between panes
- `Up` / `Down`: move in chats or scroll messages (depends on focused pane)
- `i` or `ш`: enter compose mode
- `Enter`: send message (in compose mode)
- `/` or `.`: start chat search
- `s` or `ы`: toggle chat sort mode (in chats pane)
- `Esc`: exit compose/search mode
- `q` or `й`: quit app (normal mode)

## Development

```bash
cargo test
cargo fmt
cargo clippy --all-targets --all-features -D warnings
```

## Project Layout

- `src/main.rs`: app entrypoint, event loops, app orchestration
- `src/telegram.rs`: Telegram API/auth/session integration and background request loop
- `src/tui.rs`: terminal lifecycle and rendering
- `src/app.rs`: app/UI state transitions
- `src/input.rs`: keyboard-to-command mapping

## Security Notes

- Do not commit secrets in `.env`.
- Do not commit `telegram.session` (contains local session state).
- Both are already ignored by `.gitignore`.

## Troubleshooting

- `TELEGRAM_API_ID is not set` / `TELEGRAM_API_HASH is not set`:
  - Ensure `.env` exists in repo root and values are set correctly.
- `TELEGRAM_API_ID must be a valid integer`:
  - Use numeric ID value only.
- Login errors such as invalid code/password:
  - Re-run `cargo run` and retry with the latest code from Telegram.

## License

No license file is currently present in this repository.
