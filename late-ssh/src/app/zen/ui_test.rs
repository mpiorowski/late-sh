use super::{care_bar_spans, hint_line_fitting, station_text};
use crate::app::common::primitives::hint_line;
use crate::app::hub::aquarium::state::CareBar;
use late_core::models::aquarium_care::CARE_DAYS;

fn glyphs(bar: CareBar) -> String {
    care_bar_spans(bar)
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

#[test]
fn the_care_bar_is_always_fourteen_dots() {
    // Five fed days: five full, nine empty.
    assert_eq!(glyphs(CareBar::Streak(5)), "●●●●●○○○○○○○○○");
    // Two unfed days: two full (red), the rest empty.
    assert_eq!(glyphs(CareBar::Dry(2)), "●●○○○○○○○○○○○○");
    // A minded tank shows nothing filled.
    assert_eq!(glyphs(CareBar::Minded), "○".repeat(CARE_DAYS as usize));
    // Nothing past fourteen ever widens the title.
    assert_eq!(glyphs(CareBar::Dry(40)), "●".repeat(CARE_DAYS as usize));
}

#[test]
fn the_player_names_the_station_for_the_streams_that_have_one() {
    use late_core::models::user::{AudioSource, IcecastStream, RadioStation};

    assert_eq!(
        station_text(
            AudioSource::Radio,
            RadioStation::Datawave,
            IcecastStream::Chill
        ),
        "radio · datawave"
    );
    assert_eq!(
        station_text(
            AudioSource::Icecast,
            RadioStation::Datawave,
            IcecastStream::Classical
        ),
        "icecast · classical"
    );
    // YouTube plays the queue, not a station: one word.
    assert_eq!(
        station_text(
            AudioSource::Youtube,
            RadioStation::Datawave,
            IcecastStream::Classical
        ),
        "youtube"
    );
}

#[test]
fn the_footer_drops_whole_hints_instead_of_cutting_one_in_half() {
    let hints = [("←→", "focus"), ("space", "kind"), ("S", "split")];
    let full = hint_line_fitting(&hints, 200);
    assert_eq!(full.width(), hint_line(&hints).width());
    // Room for the first two hints and part of the third: the third goes.
    let two = hint_line(&hints[..2]).width();
    let narrow = hint_line_fitting(&hints, two + 4);
    assert_eq!(narrow.width(), two);
    assert_eq!(hint_line_fitting(&hints, 3).width(), hint_line(&[]).width());
}
