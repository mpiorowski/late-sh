//! Watchers' command suggestions for a player's own Zork game. A watcher
//! suggests by starting a watch-chat line with `>` (`>open mailbox`); the
//! newest fresh one waits beside the player's game (`spectate::ui`), and F3
//! types it onto the story's empty prompt without pressing Enter
//! (`State::type_suggestion`), so the player reads it, edits it, or sends
//! it. Nothing a watcher writes reaches the game unless the player presses
//! the key.

use chrono::{DateTime, Duration, Utc};
use late_core::models::chat_message::ChatMessage;
use uuid::Uuid;

use crate::app::door::spectate::state::SpectateGame;
use crate::app::state::App;

/// How long a suggestion waits. Past this it is history, not a suggestion.
const FRESH_MINUTES: i64 = 10;

/// Longest command F3 types. A Zork command is a few words; this keeps a
/// pasted paragraph from filling the story's input line.
pub const COMMAND_MAX_CHARS: usize = 60;

/// One watcher's suggested command, as F3 would type it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// The chat message it came from, so a typed one is not offered again.
    pub message_id: Uuid,
    pub command: String,
}

/// The command in a chat line that suggests one: the text after a leading
/// `>`, cut to printable ASCII (all the story's parser reads, and nothing
/// that could reach the game as a control key), whitespace collapsed,
/// capped. `None` for any other line. A reply is read without the quote of
/// the message it answers, which the composer puts on its first line
/// (`> @author: preview`, `chat/state.rs`) and is not a command.
pub fn command(body: &str) -> Option<String> {
    let own_text = match body.split_once('\n') {
        Some((quote, rest)) if quote.trim_start().starts_with("> @") => rest,
        Some(_) | None => body,
    };
    let rest = own_text.trim_start().strip_prefix('>')?;
    let printable: String = rest
        .chars()
        .map(|ch| match ch.is_ascii_graphic() {
            true => ch,
            false => ' ',
        })
        .collect();
    let command: String = printable
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(COMMAND_MAX_CHARS)
        .collect();
    let command = command.trim_end();
    match command.is_empty() {
        true => None,
        false => Some(command.to_string()),
    }
}

/// The suggestion waiting in a room's `messages` (newest first): the newest
/// fresh one from anyone but the `player`. `None` when that one was already
/// typed: an older suggestion is stale by then, not next in line.
pub fn latest(
    messages: &[ChatMessage],
    player: Uuid,
    typed: Option<Uuid>,
    now: DateTime<Utc>,
) -> Option<Suggestion> {
    let suggestion = messages
        .iter()
        .take_while(|message| {
            now.signed_duration_since(message.created) <= Duration::minutes(FRESH_MINUTES)
        })
        .filter(|message| message.user_id != player)
        .find_map(|message| {
            Some(Suggestion {
                message_id: message.id,
                command: command(&message.body)?,
            })
        })?;
    match typed == Some(suggestion.message_id) {
        true => None,
        false => Some(suggestion),
    }
}

/// Whether the story waits at an empty prompt: the cursor right after a
/// lone `>` on its row, Zork's prompt with nothing typed yet. A half-typed
/// command, a pagination wait, the SAVE description editor, and the
/// RESTORE confirmation all fail it, so typed letters can only ever land
/// on the command line.
pub fn at_empty_prompt(screen: &vt100::Screen) -> bool {
    let (row, col) = screen.cursor_position();
    let (_, cols) = screen.size();
    col == 1 && screen.contents_between(row, 0, row, cols).trim_end() == ">"
}

/// The suggestion waiting beside this session's own Zork game: its watch
/// chat is joined (the `show_watch_chat` setting is on) and a watcher
/// suggested something fresh.
pub fn waiting(app: &App) -> Option<Suggestion> {
    let state = app.zork_state.as_ref()?;
    let room_id = app.own_watch_chat_room_id(SpectateGame::Zork)?;
    latest(
        app.chat.messages_for_room(room_id),
        app.user_id,
        state.typed_suggestion(),
        Utc::now(),
    )
}

#[cfg(test)]
#[path = "suggest_test.rs"]
mod suggest_test;
