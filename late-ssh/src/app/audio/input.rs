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
        b'1' | b'2' | b'3' | b'4' => select_active_stream(app, byte - b'0'),
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
                AudioSource::Radio => "Audio source: Radio",
                AudioSource::Icecast => "Audio source: Icecast",
            };
            app.banner = Some(Banner::success(banner));
            true
        }
        _ => false,
    }
}

fn select_active_stream(app: &mut App, index: u8) -> bool {
    use late_core::models::user::AudioSource;

    match app.paired_browser_source {
        AudioSource::Icecast => {
            let Some(stream) = super::stations::icecast_stream_by_index(index) else {
                return true;
            };
            app.select_icecast_stream(stream);
            app.banner = Some(Banner::success(&format!("Stream: {}", stream.as_str())));
            true
        }
        AudioSource::Radio => {
            let Some(station) = super::stations::radio_station_by_index(index) else {
                return true;
            };
            app.select_radio_station(station);
            app.banner = Some(Banner::success(&format!("Station: {}", station.as_str())));
            true
        }
        AudioSource::Youtube => true,
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
