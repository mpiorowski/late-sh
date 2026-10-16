//! The key and the click that open what the live strip shows.

use late_core::models::user::AudioSource;
use ratatui::layout::Position;

use crate::app::{
    chat::state::RoomSlot,
    common::primitives::{Banner, Screen},
    lobby::{daily::state::BoardEntry, modal_input::return_screen_for_opening},
    state::App,
};
use crate::metrics;

use super::pick::LiveSource;

/// Which surface an open came from, for the one source that counts it
/// (a board event: whether the strip or the panel is what brings people).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Strip,
    Panel,
}

/// `o` on the #lounge card opens what the strip is showing. With nothing
/// to open (no strip up, or a result holding it) the key falls through
/// untouched.
pub fn open_from_key(app: &mut App) -> bool {
    match app.live.opens() {
        Some(source) => open(app, source, Origin::Strip),
        None => false,
    }
}

/// `r` on the #lounge card replies to what the strip is showing, when that
/// is something a reply can quote: a shared article. Anything else falls
/// through untouched.
pub fn reply_from_key(app: &mut App) -> bool {
    match app.live.showing() {
        Some(LiveSource::NewsArticle(article_id)) => app.chat.begin_reply_to_article(article_id),
        Some(LiveSource::DailyMatch(_))
        | Some(LiveSource::DailyResult(_))
        | Some(LiveSource::BoothTrack(_))
        | Some(LiveSource::Stream(_))
        | Some(LiveSource::DoorGame(_))
        | Some(LiveSource::BoardEvent(_))
        | Some(LiveSource::Realm(_))
        | None => false,
    }
}

/// A left-click on the strip, the mouse twin of the key. A rect is only
/// recorded on frames where the strip drew something that opens.
pub fn open_from_click(app: &mut App, x: u16, y: u16) -> bool {
    match app.live.hit.get() {
        Some((rect, source)) if rect.contains(Position { x, y }) => {
            open(app, source, Origin::Strip)
        }
        Some(_) | None => false,
    }
}

/// A left-click on a row of the sidebar's Live panel opens what the row
/// lists, as the strip's click does. A blank slot or the `+N more` row
/// opens nothing.
pub fn open_from_panel_click(app: &mut App, x: u16, y: u16) -> bool {
    let Some((rect, sources)) = app.live.panel_hit.get() else {
        return false;
    };
    if !rect.contains(Position { x, y }) {
        return false;
    }
    match sources.get(usize::from(y - rect.y)) {
        Some(Some(source)) => open(app, *source, Origin::Panel),
        Some(None) | None => false,
    }
}

/// `s` then `1`-`4` on Home, or on Zen with a Live tile on the page, opens
/// that row of the Live panel (the sidebar's, or the tile's, which draws
/// the same rows), by the number the row shows: the sources are the ones
/// the last frame drew, so the key and the eye agree. Nothing opens for a
/// blank slot, the `+N more` row, a panel that is not drawn, or any other
/// suffix.
pub fn open_from_prefix(app: &mut App, byte: u8) -> bool {
    let slot = match byte {
        b'1'..=b'4' => usize::from(byte - b'1'),
        _ => return false,
    };
    let Some((_, sources)) = app.live.panel_hit.get() else {
        return false;
    };
    match sources[slot] {
        Some(source) => open(app, source, Origin::Panel),
        None => false,
    }
}

fn open(app: &mut App, source: LiveSource, origin: Origin) -> bool {
    match source {
        // Never offered (`LiveState::opens`): the board left the lobby.
        LiveSource::DailyResult(_) => false,
        // The same board the Lobby modal opens, read-only unless the viewer
        // plays in it. Gone between the frame and the key (it finished):
        // nothing to open.
        //
        // The status line's Live segment reaches this from any page, a
        // board or a table included. On the match's own board there is
        // nothing to open, and opening it again would drop the cursor and
        // a pending move. From another board or a table, the way out stays
        // the page that one was opened from.
        LiveSource::DailyMatch(match_id) => {
            let on_its_board = app
                .daily
                .board
                .as_ref()
                .is_some_and(|board| board.match_id == match_id);
            match (on_its_board, app.daily.live_item(match_id)) {
                (true, _) => false,
                (false, Some(item)) => {
                    let return_screen = return_screen_for_opening(app);
                    app.daily
                        .open_board(&item, return_screen, BoardEntry::LoungeStrip);
                    app.set_screen(Screen::DailyMatch);
                    true
                }
                (false, None) => false,
            }
        }
        // Tune in; somebody already on YouTube gets the booth, where the
        // queue can be voted on and added to. Gone between the frame and
        // the key (it was skipped or deleted): nothing to tune in to.
        LiveSource::BoothTrack(item_id) => match app.audio.in_booth(item_id) {
            true => {
                match app.paired_source {
                    AudioSource::Youtube => {
                        let submit_enabled = app.audio.booth_submit_enabled();
                        app.booth_modal_state.open(submit_enabled);
                    }
                    AudioSource::Radio => {
                        app.set_paired_playback_source(AudioSource::Youtube);
                        app.banner = Some(Banner::success("Audio source: YouTube"));
                    }
                }
                true
            }
            false => false,
        },
        // The article modal: Enter copies the link, `n` jumps to it in
        // News. Gone between the frame and the key (deleted): nothing to
        // open.
        LiveSource::NewsArticle(article_id) => app.chat.open_news_modal_for_article(article_id),
        // The streamer's room on Home, as its rail row opens it: joined on
        // the first visit, and the visit counts as a named viewer. Its
        // header carries the watch link. Gone between the frame and the key
        // (the stream ended): nothing to open.
        LiveSource::Stream(streamer_id) => {
            let room = app
                .chat
                .live_streams
                .iter()
                .find(|stream| stream.user_id == streamer_id && stream.live)
                .map(|stream| stream.room_id);
            match room {
                Some(room_id) => {
                    app.chat.reset_composer();
                    app.chat.select_room_slot(RoomSlot::Room(room_id));
                    app.set_screen(Screen::Dashboard);
                    app.sync_visible_chat_room();
                    true
                }
                None => false,
            }
        }
        // The open watch: the game across the page with its watch chat, a
        // stop on the backtick cycle, so the same key hops back out. Gone
        // between the frame and the key (the game ended): nothing to open.
        LiveSource::DoorGame(key) => crate::app::door::spectate::input::open_live_game(app, key),
        // The board, on the event's day, with the event's details open over
        // it: the shared snapshot's copy, so it is up before the page's own
        // load lands. Gone between the frame and the key (over, or taken
        // down): nothing to open.
        LiveSource::BoardEvent(event_id) => {
            let event = app
                .calendar
                .live()
                .into_iter()
                .find(|event| event.id == event_id);
            match event {
                Some(event) => {
                    metrics::record_calendar_open(origin);
                    app.calendar.show_event(event);
                    app.set_screen(Screen::Calendars);
                    true
                }
                None => false,
            }
        }
        // The board of the world the moment happened on, to play or to
        // watch, the way the hub card opens it. On that board already there
        // is nothing to open. Gone between the frame and the key (the world
        // left the listing): nothing to open.
        LiveSource::Realm(game_id) => {
            let on_its_board = app.screen == Screen::Realm
                && app
                    .realm
                    .board
                    .as_ref()
                    .is_some_and(|board| board.game_id == game_id);
            let listed = app
                .realm
                .live()
                .into_iter()
                .chain(app.realm.snapshot.last_finished())
                .any(|game| game.id == game_id);
            match (on_its_board, listed) {
                (true, _) => false,
                (false, true) => {
                    let return_screen = return_screen_for_opening(app);
                    if !app.realm.board.as_ref().is_some_and(|b| b.game_id == game_id) {
                        app.realm.open_board(game_id, return_screen);
                    }
                    app.set_screen(Screen::Realm);
                    true
                }
                (false, false) => false,
            }
        }
    }
}
