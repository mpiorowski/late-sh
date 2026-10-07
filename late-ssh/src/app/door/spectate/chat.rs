// Session glue for the watch chat: the room where the people watching one
// player talk. Each player has one permanent room per door
// (`ChatRoom::get_or_create_watch_room`), reached through a `ChatLink`:
//
// - a watcher's link lives on their watch (`spectate::state::State`) and
//   feeds the chat pane beside the watched screen, where they read and type.
//   Only an open watch drives it: a preview beside the rail has no chat, so
//   browsing the live rows joins nobody's room;
// - a player's links (`App::own_watch_chats`, one per watchable door with a
//   game of theirs running, detached ones included) live while those games
//   run and feed the read-only pane beside the game on screen (one line
//   under it on a narrow terminal). The player reads, and answers from the
//   pane: F2 or a click there opens the room's composer, the one time keys
//   leave the game (`App::handle_input`). The `show_watch_chat` profile
//   setting turns their side off; the watchers keep talking either way.
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
/// the player's own pane gained or lost its room.
pub fn tick(app: &mut App) -> bool {
    let mut changed = false;

    if let Some(state) = app.spectate_state.as_mut().filter(|state| state.is_open()) {
        changed |= drive(
            state.chat_mut(),
            &app.live_games,
            &app.chat,
            app.repaint_signal.clone(),
        );
    }

    // The player's own links follow their running games: one made when a
    // game starts (or the setting comes on), dropped when it ends (or the
    // setting goes off).
    let wanted = own_games(app);
    let (kept, dropped): (Vec<ChatLink>, Vec<ChatLink>) = std::mem::take(&mut app.own_watch_chats)
        .into_iter()
        .partition(|link| {
            wanted
                .iter()
                .any(|(game, name)| link.game() == *game && link.playname() == name)
        });
    app.own_watch_chats = kept;
    changed |= !dropped.is_empty();
    // A composer open in a dropped link's room (the game ended, or `t`
    // turned the pane off) has nowhere left to send: it closes with the link.
    for link in dropped {
        if let Some(room_id) = link.room_id()
            && app.chat.composer_room_id() == Some(room_id)
        {
            app.chat.reset_composer();
        }
    }
    for (game, name) in wanted {
        let linked = app
            .own_watch_chats
            .iter()
            .any(|link| link.game() == game && link.playname() == name);
        if !linked {
            app.own_watch_chats.push(ChatLink::new(game, name));
            changed = true;
        }
    }
    for link in app.own_watch_chats.iter_mut() {
        changed |= drive(link, &app.live_games, &app.chat, app.repaint_signal.clone());
    }

    changed
}

/// The running games this session's player should see watcher chat beside.
fn own_games(app: &App) -> Vec<(SpectateGame, String)> {
    match app.profile_state.profile().show_watch_chat {
        true => own_running_games(app),
        false => Vec::new(),
    }
}

/// This session's own running games on the watchable doors, by door and the
/// player's handle.
pub(crate) fn own_running_games(app: &App) -> Vec<(SpectateGame, String)> {
    SpectateGame::ALL
        .into_iter()
        .filter_map(|game| Some((game, own_running_handle(app, game)?)))
        .collect()
}

/// The handle this session's player runs `game` under, while a game of
/// theirs is running there.
pub(crate) fn own_running_handle(app: &App, game: SpectateGame) -> Option<String> {
    let (running, handle) = match game {
        SpectateGame::Dcss => {
            let state = app.dcss_state.as_ref()?;
            (state.is_running(), state.handle_status())
        }
        SpectateGame::Nethack => {
            let state = app.nethack_state.as_ref()?;
            (state.is_running(), state.handle_status())
        }
        SpectateGame::Brogue => {
            let state = app.brogue_state.as_ref()?;
            (state.is_running(), state.handle_status())
        }
    };
    match (running, handle) {
        (true, HandleStatus::Claimed(handle)) => Some(handle),
        (false, _)
        | (
            true,
            HandleStatus::Loading
            | HandleStatus::Missing { .. }
            | HandleStatus::Claiming
            | HandleStatus::Failed,
        ) => None,
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

/// The newest watcher message, as the player reads it on the one row under
/// their game when the terminal is too narrow for the pane.
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
