use super::svc::Genre;
use crate::app::common::primitives::Banner;
use crate::app::state::App;

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'v' | b'V' => {
            app.vote_prefix_armed = true;
            true
        }
        _ => false,
    }
}

pub fn handle_vote_suffix(app: &mut App, byte: u8) -> bool {
    if let Some(option_position) = poll_option_position(byte)
        && app.chat.cast_poll_vote_for_selected_room(option_position)
    {
        return true;
    }

    match byte {
        b'1' | b'l' | b'L' => {
            app.vote.cast_task(Genre::Lofi);
            true
        }
        b'2' | b'a' | b'A' => {
            app.vote.cast_task(Genre::Ambient);
            true
        }
        b'3' | b'c' | b'C' => {
            app.vote.cast_task(Genre::Classic);
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
            use late_core::models::user::AudioSource;
            let banner = match app.toggle_paired_playback_source() {
                AudioSource::Youtube => "Audio source: YouTube",
                AudioSource::Icecast => "Audio source: Icecast",
            };
            app.banner = Some(Banner::success(banner));
            true
        }
        // b'z' | b'Z' => {
        //     app.vote.cast_task(Genre::Jazz);
        //     true
        // }
        _ => false,
    }
}

fn poll_option_position(byte: u8) -> Option<i32> {
    match byte {
        b'1' => Some(1),
        b'2' => Some(2),
        b'3' => Some(3),
        _ => None,
    }
}
