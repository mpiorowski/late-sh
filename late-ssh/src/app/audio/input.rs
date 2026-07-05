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
        b'1' | b'2' | b'3' | b'4' | b'5' => select_active_stream(app, byte - b'0'),
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
            app.banner = Some(Banner::success(&format!(
                "Stream: {}",
                sentence_case(super::stations::icecast_stream_display_name(stream))
            )));
            true
        }
        AudioSource::Radio => {
            let Some(station) = super::stations::radio_station_by_index(index) else {
                return true;
            };
            app.select_radio_station(station);
            app.banner = Some(Banner::success(&format!(
                "Station: {}",
                sentence_case(super::stations::radio_station_display_name(station))
            )));
            true
        }
        AudioSource::Youtube => true,
    }
}

/// Banners keep sentence case ("Stream: Chill"); selector rows in the
/// sidebar keep the lowercase display name.
fn sentence_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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
mod tests {
    use super::poll_option_position;

    #[test]
    fn poll_vote_suffixes_are_letters() {
        assert_eq!(poll_option_position(b'a'), Some(1));
        assert_eq!(poll_option_position(b'b'), Some(2));
        assert_eq!(poll_option_position(b'c'), Some(3));
        assert_eq!(poll_option_position(b'A'), Some(1));
        assert_eq!(poll_option_position(b'B'), Some(2));
        assert_eq!(poll_option_position(b'C'), Some(3));
    }

    #[test]
    fn numeric_suffixes_remain_available_for_music_selection() {
        assert_eq!(poll_option_position(b'1'), None);
        assert_eq!(poll_option_position(b'2'), None);
        assert_eq!(poll_option_position(b'3'), None);
    }
}
