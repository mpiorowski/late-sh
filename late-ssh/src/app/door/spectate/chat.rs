// Session glue for the watch chat: the room where the people watching one
// player talk. Each player has one permanent room per door
// (`ChatRoom::get_or_create_watch_room`), reached through a `ChatLink`:
//
// - a watcher's link lives on their watch (`spectate::state::State`) and
//   feeds the chat pane beside the watched screen, where they read and type;
// - a player's link (`App::own_watch_chat`) lives while their own game runs
//   and feeds the one line under it. The player only reads: every key they
//   press still goes to the game. The `show_watch_chat` profile setting turns
//   their line off; the watchers keep talking either way.
//
// `tick` drives both links from `App::tick`.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use late_core::models::chat_message::ChatMessage;

use super::state::{ChatLink, ChatLinkStep, SpectateGame};
use super::svc::LiveGamesService;
use crate::app::chat::state::ChatState;
use crate::app::door::arcade::HandleStatus;
use crate::app::state::App;
use crate::render_signal::RenderSignal;
use crate::usernames::UsernameLookup;

/// How long a watcher's message stays under the player's game. Past this the
/// line clears, so a remark from an hour ago never reads as a live one.
const LINE_FRESH_MINUTES: i64 = 10;

/// Drive this session's watch-chat links. Returns whether the watch pane or
/// the player's line gained or lost its room.
pub fn tick(app: &mut App) -> bool {
    let mut changed = false;

    if let Some(state) = app.spectate_state.as_mut() {
        changed |= drive(
            state.chat_mut(),
            &app.live_games,
            &app.chat,
            app.repaint_signal.clone(),
        );
    }

    // The player's own link follows their running game: made when it starts
    // (or the setting comes on), dropped when it ends (or the setting goes
    // off).
    let wanted = own_game(app);
    let held = app
        .own_watch_chat
        .as_ref()
        .map(|link| (link.game(), link.playname()));
    if held != wanted.as_ref().map(|(game, name)| (*game, name.as_str())) {
        app.own_watch_chat = wanted.map(|(game, name)| ChatLink::new(game, name));
        changed = true;
    }
    if let Some(link) = app.own_watch_chat.as_mut() {
        changed |= drive(link, &app.live_games, &app.chat, app.repaint_signal.clone());
    }

    changed
}

/// The running game this session's player should see watcher chat under.
fn own_game(app: &App) -> Option<(SpectateGame, String)> {
    if !app.profile_state.profile().show_watch_chat {
        return None;
    }
    let state = app.dcss_state.as_ref()?;
    if !state.is_running() {
        return None;
    }
    match state.handle_status() {
        HandleStatus::Claimed(handle) => Some((SpectateGame::Dcss, handle)),
        HandleStatus::Loading
        | HandleStatus::Missing { .. }
        | HandleStatus::Claiming
        | HandleStatus::Failed => None,
    }
}

fn drive(
    link: &mut ChatLink,
    live_games: &LiveGamesService,
    chat: &ChatState,
    repaint: Option<Arc<RenderSignal>>,
) -> bool {
    let resolved = live_games.chat_room_id(link.game(), link.playname());
    match link.step(resolved) {
        ChatLinkStep::Resolve => {
            live_games.resolve_chat_room_task(link.game(), link.playname().to_string(), repaint);
            false
        }
        ChatLinkStep::Join(room_id) => {
            chat.join_game_room_chat(room_id);
            true
        }
        ChatLinkStep::Idle => false,
    }
}

/// The newest watcher message, as the player reads it under their game.
#[derive(Debug, PartialEq, Eq)]
pub struct WatchLine {
    pub author: String,
    pub body: String,
    /// A `/me` action, read as `* author body` rather than `author: body`.
    pub is_action: bool,
    /// `now`, `3m`: how long ago it was said.
    pub age: String,
}

/// The line for a room's messages (newest first), or `None` when the newest
/// one is no longer fresh.
pub fn latest_line(
    messages: &[ChatMessage],
    usernames: &UsernameLookup<'_>,
    now: DateTime<Utc>,
) -> Option<WatchLine> {
    let message = messages.first()?;
    let age = now.signed_duration_since(message.created);
    if age > Duration::minutes(LINE_FRESH_MINUTES) {
        return None;
    }
    let author = match usernames.get(&message.user_id) {
        Some(name) => name.clone(),
        None => "someone".to_string(),
    };
    let (text, is_action) = match crate::app::chat::action::parse_action_body(&message.body) {
        Some(action) => (action, true),
        None => (message.body.as_str(), false),
    };
    Some(WatchLine {
        author,
        body: one_line(text),
        is_action,
        age: match age.num_minutes() {
            ..=0 => "now".to_string(),
            minutes => format!("{minutes}m"),
        },
    })
}

/// Text on one row: every run of whitespace (line breaks included) collapses
/// to one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "chat_test.rs"]
mod chat_test;
