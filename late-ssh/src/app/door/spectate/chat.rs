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
//   run, so the room is joined and ready the moment F4 opens the chat over
//   the game (`spectate::input`), where it takes every key until F4 or Esc
//   closes it. The game ending closes it too.
//
// `tick` drives both links from `App::tick`.

use std::sync::Arc;

use super::state::{ChatLink, ChatLinkStep, SpectateGame};
use super::svc::LiveGamesService;
use crate::app::chat::state::ChatState;
use crate::app::door::arcade::HandleStatus;
use crate::app::state::App;
use crate::render_signal::RenderSignal;

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
    // game starts, dropped when it ends. A chat open over a game that ended
    // closes first, while its link still names the room.
    let wanted = own_running_games(app);
    if let Some(open) = app.own_watch_chat_open
        && !wanted.iter().any(|(game, _)| *game == open)
    {
        super::input::close_own_chat(app);
        changed = true;
    }
    let (kept, dropped): (Vec<ChatLink>, Vec<ChatLink>) = std::mem::take(&mut app.own_watch_chats)
        .into_iter()
        .partition(|link| {
            wanted
                .iter()
                .any(|(game, name)| link.game() == *game && link.playname() == name)
        });
    app.own_watch_chats = kept;
    changed |= !dropped.is_empty();
    // A composer open in a dropped link's room (the game ended) has nowhere
    // left to send: it closes with the link.
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

/// This session's own running games on the watchable doors, by door and the
/// player's handle.
fn own_running_games(app: &App) -> Vec<(SpectateGame, String)> {
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
        SpectateGame::Zork => {
            let state = app.zork_state.as_ref()?;
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
