use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use super::*;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn renders_header_rows_and_footer_in_both_formats() {
    let card = ShareCard {
        title: title("Le Word", 214, Some("4/6")),
        rows: vec![
            Row::Glyphs(vec![Glyph::Dark, Glyph::Yellow, Glyph::Green]),
            Row::Text("♠ 13".to_string()),
        ],
    };
    assert_eq!(
        render(&card, ShareFormat::Emoji),
        "late.sh Le Word #214 · 4/6\n⬛🟨🟩\n♠ 13\nssh late.sh"
    );
    assert_eq!(
        render(&card, ShareFormat::Ascii),
        "late.sh Le Word #214 · 4/6\n.+#\n♠ 13\nssh late.sh"
    );
}

#[test]
fn puzzle_number_counts_arcade_days_from_the_epoch() {
    assert_eq!(puzzle_number(ARCADE_EPOCH), 1);
    assert_eq!(puzzle_number(day(2026, 6, 19)), 70);
    assert_eq!(puzzle_number(day(2026, 9, 7)), 150);
}

#[test]
fn day_card_marks_wins_in_lobby_order_with_an_icon_under_each_box() {
    let card = day_card(
        day(2026, 9, 7),
        |puzzle| matches!(puzzle, DailyPuzzle::LeWord | DailyPuzzle::Sudoku),
        41,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Daily #150 · 2/7 · 🔥 41".to_string(),
            rows: vec![
                Row::Glyphs(vec![
                    Glyph::Green,
                    Glyph::Dark,
                    Glyph::Dark,
                    Glyph::Green,
                    Glyph::Dark,
                    Glyph::Dark,
                    Glyph::Dark,
                ]),
                Row::Text("🔤🧊🧩🔢🎨💣🃏".to_string()),
            ],
        }
    );
    assert_eq!(
        render(&card, ShareFormat::Emoji),
        "late.sh Daily #150 · 2/7 · 🔥 41\n🟩⬛⬛🟩⬛⬛⬛\n🔤🧊🧩🔢🎨💣🃏\nssh late.sh"
    );
}

#[test]
fn day_card_without_a_streak_omits_the_flame() {
    let card = day_card(ARCADE_EPOCH, |_| true, 0);
    assert_eq!(card.title, "late.sh Daily #1 · 7/7");
}

#[test]
fn a_bare_title_has_no_result() {
    assert_eq!(title("Rubik's Cube", 82, None), "late.sh Rubik's Cube #82");
}

#[test]
fn half_block_picture_packs_two_rows_per_line_and_drops_empty_lines() {
    let picture = vec![
        vec![true, false, true],
        vec![true, true, false],
        vec![false, false, true],
        vec![false, false, false],
        vec![false, false, false],
        vec![false, false, false],
    ];
    assert_eq!(
        half_block_picture(&picture),
        vec![Row::Text("█▄▀".to_string()), Row::Text("  ▀".to_string())]
    );
}
