use super::*;
use chrono::NaiveDate;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::deadchannel::street::state::StreetRunner;

fn render(view: &LandingView<'_>, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), view, 0);
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Before the sheet lands the card still reads as the way down: the name,
/// an empty street, and the Enter that goes there.
#[test]
fn the_card_waits_on_the_sheet_and_offers_the_descent() {
    let street = StreetView::new();
    let view = LandingView {
        sheet: None,
        street: &street,
        own_user_id: Uuid::nil(),
        tick: 0,
    };
    let screen = render(&view, 90, 60);
    assert!(screen.contains("█████"), "the name in blocks\n{screen}");
    assert!(
        screen.contains("reading your sheet"),
        "no sheet yet\n{screen}"
    );
    assert!(
        screen.contains("nobody on the street tonight"),
        "an empty street\n{screen}"
    );
    assert!(
        screen.contains("down to Static Row"),
        "the descent\n{screen}"
    );
}

/// The runner's own numbers, today's road, and the others on the street,
/// never counting yourself.
#[test]
fn the_card_reads_the_sheet_and_counts_the_others() {
    let me = Uuid::from_u128(1);
    let mut sheet = Sheet::fresh(me, NaiveDate::from_ymd_opt(2026, 10, 6).unwrap());
    sheet.crystals = 3;
    sheet.kills = 41;
    let mut street = StreetView::new();
    for (id, present) in [(1u128, true), (2, true), (3, false)] {
        street.insert(
            Uuid::from_u128(id),
            StreetRunner {
                x: 0,
                y: 0,
                present,
            },
        );
    }
    let view = LandingView {
        sheet: Some(&sheet),
        street: &street,
        own_user_id: me,
        tick: 0,
    };
    let screen = render(&view, 100, 60);
    assert!(screen.contains("3 crystals"), "the pockets\n{screen}");
    assert!(screen.contains("41 put down"), "the glyphs\n{screen}");
    assert!(
        screen.contains(&format!("0 of {STEPS} steps")),
        "today's road\n{screen}"
    );
    assert!(
        screen.contains("2 other runners on the street, 1 of them looking"),
        "the street, without me\n{screen}"
    );
}

/// The narrowest pane the hub gives a landing stacks the name instead of
/// cutting it, and paints without panicking.
#[test]
fn a_narrow_pane_stacks_the_name() {
    let street = StreetView::new();
    let view = LandingView {
        sheet: None,
        street: &street,
        own_user_id: Uuid::nil(),
        tick: 12_345,
    };
    let narrow = name_rows(41);
    let wide = name_rows(90);
    assert_eq!(narrow.len(), GLYPH_ROWS * 2 + 1);
    assert_eq!(wide.len(), GLYPH_ROWS);
    assert!(narrow[0].len() <= 41);
    render(&view, 41, 20);
}
