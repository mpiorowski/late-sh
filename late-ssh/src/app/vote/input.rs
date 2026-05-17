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
            match app.toggle_paired_playback_source() {
                Some(AudioSource::Youtube) => {
                    app.banner = Some(Banner::success("Paired browser audio: YouTube"));
                }
                Some(AudioSource::Icecast) => {
                    app.banner = Some(Banner::success("Paired browser audio: Icecast"));
                }
                None => {
                    app.banner = Some(Banner::error("No paired browser"));
                }
            }
            true
        }
        // b'z' | b'Z' => {
        //     app.vote.cast_task(Genre::Jazz);
        //     true
        // }
        _ => false,
    }
}
