//! The key and the click that open what the live strip shows.

use late_core::models::user::AudioSource;
use ratatui::layout::Position;

use crate::app::{
    chat::state::RoomSlot,
    common::primitives::{Banner, Screen},
    lobby::{daily::state::BoardEntry, modal_input::return_screen_for_opening},
    state::App,
};

use super::pick::LiveSource;

/// `o` on the #lounge card or on Zen with a Live tile shown, or Enter on a
/// focused Zen Live tile, opens what the strip is showing. With nothing to
/// open (no strip up, or a result holding it) the key falls through
/// untouched.
pub fn open_from_key(app: &mut App) -> bool {
    match app.live.opens() {
        Some(source) => open(app, source),
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
        | None => false,
    }
}

/// A left-click on the strip, the mouse twin of the key. A rect is only
/// recorded on frames where the strip drew something that opens.
pub fn open_from_click(app: &mut App, x: u16, y: u16) -> bool {
    match app.live.hit.get() {
        Some((rect, source)) if rect.contains(Position { x, y }) => open(app, source),
        Some(_) | None => false,
    }
}

fn open(app: &mut App, source: LiveSource) -> bool {
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
    }
}
