//! The key and the click that open what the live strip shows.

use late_core::models::user::AudioSource;
use ratatui::layout::Position;

use crate::app::{
    common::primitives::{Banner, Screen},
    lobby::daily::state::BoardEntry,
    state::App,
};

use super::pick::LiveSource;

/// `o` on the #lounge card opens what the strip is showing. With nothing to
/// open (no strip up, or a result holding it) the key falls through
/// untouched.
pub fn open_from_key(app: &mut App) -> bool {
    match app.live.opens() {
        Some(source) => open(app, source),
        None => false,
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
        // The same board the Lobby modal opens, read-only unless the viewer
        // plays in it. Gone between the frame and the key (it finished):
        // nothing to open.
        LiveSource::DailyMatch(match_id) => match app.daily.live_item(match_id) {
            Some(item) => {
                app.daily
                    .open_board(&item, app.screen, BoardEntry::LoungeStrip);
                app.set_screen(Screen::DailyMatch);
                true
            }
            None => false,
        },
        // Tune in; somebody already on YouTube gets the booth, where the
        // queue can be voted on and added to.
        LiveSource::BoothTrack(_) => {
            match app.paired_source {
                AudioSource::Youtube => {
                    let submit_enabled = app.audio.booth_submit_enabled();
                    app.booth_modal_state.open(submit_enabled);
                }
                AudioSource::Radio | AudioSource::Icecast => {
                    app.set_paired_playback_source(AudioSource::Youtube);
                    app.banner = Some(Banner::success("Audio source: YouTube"));
                }
            }
            true
        }
    }
}
