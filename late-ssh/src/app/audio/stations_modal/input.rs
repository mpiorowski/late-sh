use late_core::models::user::AudioSource;
use late_core::radio::RADIO_SLOTS;

use crate::app::{common::primitives::Banner, input::ParsedInput, state::App};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.stations_modal_state.close(),
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => {
            app.stations_modal_state.move_selection(-1);
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
            app.stations_modal_state.move_selection(1);
        }
        ParsedInput::PageUp => app.stations_modal_state.move_selection(-8),
        ParsedInput::PageDown => app.stations_modal_state.move_selection(8),
        ParsedInput::Byte(b'\r') => listen_to_selected(app),
        ParsedInput::Char('0') => unpin_selected(app),
        ParsedInput::Char(digit @ '1'..='9') => {
            let index = digit as usize - '1' as usize;
            if index < RADIO_SLOTS {
                pin_selected(app, index);
            }
        }
        ParsedInput::Char('r') | ParsedInput::Char('R') => app.stations_modal_state.close(),
        _ => {}
    }
}

/// Enter: tune the paired client to the highlighted station right away
/// (persisted like `v1`..`v5`), switching the source to radio if YouTube
/// was active so Enter always produces sound. The modal stays open so the
/// listener can keep trying stations.
fn listen_to_selected(app: &mut App) {
    let Some(station) = app.stations_modal_state.selected_station() else {
        return;
    };
    if app.paired_source != AudioSource::Radio {
        app.set_paired_playback_source(AudioSource::Radio);
    }
    app.select_radio_station(station);
    app.banner = Some(Banner::success(&format!(
        "Station: {}",
        sentence_case(station.label())
    )));
}

fn pin_selected(app: &mut App, index: usize) {
    let Some(station) = app.stations_modal_state.selected_station() else {
        return;
    };
    app.pin_radio_slot(index, station);
    app.banner = Some(Banner::success(&format!(
        "Pinned {} to v{}",
        sentence_case(station.label()),
        index + 1
    )));
}

fn unpin_selected(app: &mut App) {
    let Some(station) = app.stations_modal_state.selected_station() else {
        return;
    };
    let Some(index) = app.radio_slots.position_of(station) else {
        return;
    };
    app.unpin_radio_slot(index);
    app.banner = Some(Banner::success(&format!(
        "Unpinned {} from v{}",
        sentence_case(station.label()),
        index + 1
    )));
}

/// Banners keep sentence case ("Station: Classical"); rows keep the
/// lowercase label.
pub(crate) fn sentence_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
