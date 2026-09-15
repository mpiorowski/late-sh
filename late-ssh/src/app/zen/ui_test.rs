use super::{
    Care, Chore, PulseView, care_bar_spans, draw_music_tile, draw_pulse_tile, hint_line_fitting,
    station_text,
};
use crate::app::audio::viz::EqState;
use crate::app::common::{primitives::hint_line, theme};
use crate::app::hub::aquarium::state::CareBar;
use late_core::models::aquarium_care::CARE_DAYS;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn music_tile_rows(width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            draw_music_tile(
                frame,
                Rect::new(0, 0, width, height),
                "Ambition",
                "youtube",
                0,
                EqState::Ambient,
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn the_music_tile_pins_track_and_station_to_its_last_two_rows() {
    let rows = music_tile_rows(24, 9);
    assert!(rows[7].contains("♪ Ambition"), "{rows:#?}");
    assert!(rows[8].contains("youtube"), "{rows:#?}");
    // Every row above the text belongs to the visualizer.
    for row in &rows[..7] {
        assert!(!row.contains("Ambition") && !row.contains("youtube"), "{rows:#?}");
    }
    let bar_cells = rows[..7]
        .iter()
        .flat_map(|row| row.chars())
        .filter(|ch| !ch.is_whitespace())
        .count();
    assert!(bar_cells > 0, "{rows:#?}");

    // Two rows is text only; one row keeps the track.
    assert_eq!(music_tile_rows(24, 2)[0].trim(), "♪ Ambition");
    assert_eq!(music_tile_rows(24, 1)[0].trim(), "♪ Ambition");
}

fn glyphs(bar: CareBar) -> String {
    care_bar_spans(bar)
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

#[test]
fn pulse_draws_every_row_even_at_zero_and_colors_each_chore() {
    let pulse = PulseView {
        online: 0,
        chips: 0,
        mentions: 0,
        friends: 0,
        care: Care {
            bonsai: Chore::Done,
            tank: Chore::Due,
            pet: Chore::NotOwned,
        },
    };
    let (width, height) = (30u16, 5u16);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| draw_pulse_tile(frame, Rect::new(0, 0, width, height), &pulse))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..height)
        .map(|y| {
            let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            row.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            "online 0",
            "chips 0",
            "mentions 0",
            "friends 0",
            "care bonsai tank pet"
        ]
    );
    // The chores sit flush right: `bonsai tank pet` is fifteen columns.
    assert_eq!(buffer[(15, 4)].fg, theme::SUCCESS(), "a tended bonsai");
    assert_eq!(buffer[(22, 4)].fg, theme::AMBER(), "a tank still due");
    assert_eq!(buffer[(27, 4)].fg, theme::TEXT_FAINT(), "no pet owned");
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
