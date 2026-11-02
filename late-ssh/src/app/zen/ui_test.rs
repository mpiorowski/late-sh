use super::{
    care_bar_spans, chat_tile_title, draw_board_day, draw_bonsai_tile, draw_music_tile,
    hint_line_fitting, hints_shown, station_text, tile_keys,
};
use crate::app::bonsai::{
    render::canvas_lines,
    state::{BonsaiState, CANVAS_HEIGHT, CANVAS_WIDTH},
};
use crate::app::common::primitives::hint_line;
use crate::app::hub::aquarium::state::CareBar;
use crate::app::zen::state::TileKind;
use late_core::models::aquarium_care::CARE_DAYS;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

fn music_tile_rows(width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| draw_music_tile(frame, Rect::new(0, 0, width, height), "Ambition", "youtube"))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn a_short_music_tile_centres_the_track_over_the_station_and_draws_no_band() {
    let rows = music_tile_rows(24, 9);
    assert!(rows[3].contains("♪ Ambition"), "{rows:#?}");
    assert!(rows[4].contains("youtube"), "{rows:#?}");
    // Nothing else is drawn: the equalizer is the visualizer tile's.
    for (y, row) in rows.iter().enumerate() {
        if y != 3 && y != 4 {
            assert_eq!(row.trim(), "", "{rows:#?}");
        }
    }

    // Two rows is text only; one row keeps the track.
    assert_eq!(music_tile_rows(24, 2)[0].trim(), "♪ Ambition");
    assert_eq!(music_tile_rows(24, 2)[1].trim(), "youtube");
    assert_eq!(music_tile_rows(24, 1)[0].trim(), "♪ Ambition");
}

#[test]
fn a_key_that_works_only_on_the_focused_tile_shows_only_there() {
    let ascii = tile_keys(TileKind::Ascii);
    assert_eq!(hints_shown(ascii, false, false), vec![]);
    assert_eq!(hints_shown(ascii, false, true), vec![]);
    assert_eq!(hints_shown(ascii, true, false), vec![]);
    assert_eq!(hints_shown(ascii, true, true), vec![("[ ] Enter", "pick")]);

    // A global key shows in every mode; a chat tile adds its arrange-only
    // room cycling ahead of it only while focused.
    assert_eq!(
        hints_shown(tile_keys(TileKind::Bonsai), false, false),
        vec![("w", "tend")]
    );
    let chat = tile_keys(TileKind::Chat);
    assert_eq!(hints_shown(chat, false, false), vec![("ctrl+/", "room")]);
    assert_eq!(
        hints_shown(chat, true, true),
        vec![("[ ]", "cycle"), ("ctrl+/", "room")]
    );
    // The rail takes clicks on its rows and no key, so it names none.
    assert_eq!(hints_shown(tile_keys(TileKind::Rooms), true, true), vec![]);
}

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
    use late_core::models::user::{AudioSource, RadioStation};

    let datawave = RadioStation::from_key("datawave").unwrap();
    let classical = RadioStation::from_key("classical").unwrap();
    assert_eq!(
        station_text(AudioSource::Radio, datawave),
        "radio · datawave"
    );
    // House mounts are stations like any other.
    assert_eq!(
        station_text(AudioSource::Radio, classical),
        "radio · classical"
    );
    // YouTube plays the queue, not a station: one word.
    assert_eq!(station_text(AudioSource::Youtube, datawave), "youtube");
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

/// A stream room's tile carries the rail's watcher count after the room,
/// and on a narrow tile the room label shortens so the count is never the
/// part the border clips.
#[test]
fn a_stream_chat_tile_title_keeps_the_watcher_count_on_a_narrow_tile() {
    assert_eq!(chat_tile_title("#lounge", None, 40), "chat · #lounge");
    assert_eq!(
        chat_tile_title("#mat-live", Some("[3]"), 40),
        "chat · #mat-live [3]"
    );
    assert_eq!(
        chat_tile_title("#mat-live", Some("[…]"), 40),
        "chat · #mat-live […]"
    );

    // Four cells go to the corners and the title's padding spaces.
    let title = chat_tile_title("#mat-live", Some("[12]"), 20);
    assert_eq!(title, "chat · #ma… [12]");
    assert_eq!(UnicodeWidthStr::width(title.as_str()), 16);
}

fn bonsai_tile_rows(width: u16, height: u16) -> Vec<String> {
    let state = BonsaiState::fallback(Uuid::nil(), 42);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| draw_bonsai_tile(frame, Rect::new(0, 0, width, height), &state, 0))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// The care modal's canvas as plain rows, each padded to the canvas width.
fn canvas_rows() -> Vec<String> {
    let state = BonsaiState::fallback(Uuid::nil(), 42);
    canvas_lines(&state, true)
        .iter()
        .map(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            format!("{text:<width$}", width = CANVAS_WIDTH)
        })
        .collect()
}

#[test]
fn a_small_bonsai_tile_shows_the_true_canvas_cut_evenly_around_the_trunk() {
    let (width, height) = (41u16, 12u16);
    let rows = bonsai_tile_rows(width, height);
    let cut = (CANVAS_WIDTH - width as usize) / 2;
    let canvas = canvas_rows();
    let expected: Vec<String> = canvas[canvas.len() - height as usize..]
        .iter()
        .map(|row| row.chars().skip(cut).take(width as usize).collect())
        .collect();
    assert_eq!(rows, expected);
}

#[test]
fn a_roomy_bonsai_tile_centers_the_whole_canvas() {
    let (width, height) = (CANVAS_WIDTH as u16 + 20, CANVAS_HEIGHT as u16 + 3);
    let rows = bonsai_tile_rows(width, height);
    let canvas = canvas_rows();
    let tree_rows = &rows[rows.len() - canvas.len()..];
    let expected: Vec<String> = canvas
        .iter()
        .map(|row| format!("{:10}{row}{:10}", "", ""))
        .collect();
    assert_eq!(tree_rows, &expected[..]);
}

/// The board's day reads in amber at the left of its row with the board's
/// key flush right, the text giving way to the key when the row is short.
#[test]
fn the_boards_day_banner_is_the_text_left_and_the_key_right() {
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("terminal");
    terminal
        .draw(|frame| draw_board_day(frame, Rect::new(0, 0, 40, 1), "Hackathon"))
        .expect("draw");
    let row: String = (0..40)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
        .collect();
    assert_eq!(row, format!("Hackathon{}7 for the board", " ".repeat(16)));

    let mut terminal = Terminal::new(TestBackend::new(24, 1)).expect("terminal");
    terminal
        .draw(|frame| draw_board_day(frame, Rect::new(0, 0, 24, 1), "Hackathon and more"))
        .expect("draw");
    let row: String = (0..24)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
        .collect();
    assert!(row.ends_with("7 for the board"), "{row:?}");
    assert!(row.starts_with("Hackath"), "{row:?}");
}
