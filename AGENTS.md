# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust terminal Telegram client.
- `src/main.rs`: app entrypoint, auth/chat event loops, orchestration.
- `src/telegram.rs`: Telegram API integration (`grammers-*`), auth flow, dialog/message loading.
- `src/tui.rs`: `ratatui` rendering and terminal lifecycle (`TerminalGuard`).
- `src/app.rs`: UI state model and selection/message state transitions.
- `src/input.rs`: keyboard mapping (`Up`, `Down`, `q`) and input helpers.
- `Cargo.toml` / `Cargo.lock`: dependency and lock metadata.
- Runtime artifacts such as `telegram.session` are local state, not source.

## Build, Test, and Development Commands
- `cargo run`: build and start the client.
- `cargo test`: run unit tests (`app` and `input` modules).
- `cargo check`: fast compile check without producing a binary.
- `cargo fmt`: format code (run before opening a PR).
- `cargo clippy --all-targets --all-features -D warnings`: lint with warnings treated as errors.

## Coding Style & Naming Conventions
- Follow standard Rust style (`rustfmt`, 4-space indentation, trailing commas where idiomatic).
- Use `snake_case` for functions/modules/files, `PascalCase` for structs/enums, `SCREAMING_SNAKE_CASE` for constants.
- Keep modules focused: UI drawing in `tui.rs`, Telegram I/O in `telegram.rs`, state transitions in `app.rs`.
- Prefer explicit error context with `anyhow::Context` at async boundaries.

## Testing Guidelines
- Use Rust’s built-in test framework (`#[cfg(test)]`, `#[test]`).
- Add unit tests alongside the module they validate.
- Test names should describe behavior, e.g. `selection_bounds_are_clamped`.
- Cover state transitions, key mapping, and message/dialog updates when adding features.

## Commit & Pull Request Guidelines
Current history is minimal (`Initial commit`), so use clear imperative commit messages (e.g. `Add TUI auth state machine`).
- Keep commits scoped to one change.
- PRs should include: purpose, key design choices, test evidence (`cargo test` output), and follow-ups.
- Include terminal screenshots or short recordings for TUI behavior changes.

## Security & Configuration Tips
- Never commit secrets. Store `TELEGRAM_API_ID` and `TELEGRAM_API_HASH` in `.env` locally.
- Treat `.env` and `telegram.session` as sensitive local files.
