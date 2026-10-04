use late_core::models::user::AudioSource;

use crate::app::common::primitives::Banner;
use crate::app::state::App;

pub fn handle_music_suffix(app: &mut App, byte: u8, allow_poll_vote: bool) -> bool {
    if allow_poll_vote
        && let Some(option_position) = poll_option_position(byte)
        && app.chat.cast_poll_vote_for_selected_room(option_position)
    {
        return true;
    }

    match byte {
        b'1'..=b'9' => select_slot(app, (byte - b'1') as usize),
        b'r' | b'R' => {
            app.stations_modal_state.open(app.selected_radio_station);
            true
        }
        b'v' | b'V' => {
            let submit_enabled = app.audio.booth_submit_enabled();
            app.booth_modal_state.open(submit_enabled);
            true
        }
        b's' | b'S' => {
            app.audio.booth_skip_vote();
            true
        }
        b'x' | b'X' => {
            let banner = match app.toggle_paired_playback_source() {
                AudioSource::Youtube => "Audio source: YouTube",
                AudioSource::Radio => "Audio source: Radio",
            };
            app.banner = Some(Banner::success(banner));
            true
        }
        _ => false,
    }
}

/// `v1`..`v3`: tune to the station pinned in that slot. Only meaningful
/// while radio is the active source; on YouTube the key is swallowed so a
/// stray digit never lands in the composer. An empty slot, or a digit past
/// the last slot, is a no-op for the same reason.
fn select_slot(app: &mut App, index: usize) -> bool {
    if app.paired_source != AudioSource::Radio {
        return true;
    }
    let Some(station) = app.radio_slots.get(index) else {
        return true;
    };
    app.select_radio_station(station);
    app.banner = Some(Banner::success(&format!(
        "Station: {}",
        super::stations_modal::input::sentence_case(station.label())
    )));
    true
}

fn poll_option_position(byte: u8) -> Option<i32> {
    match byte {
        b'a' | b'A' => Some(1),
        b'b' | b'B' => Some(2),
        b'c' | b'C' => Some(3),
        _ => None,
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
