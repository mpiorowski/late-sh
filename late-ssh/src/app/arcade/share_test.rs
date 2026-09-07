use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use super::*;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn renders_header_rows_and_footer_in_both_formats() {
    let card = ShareCard {
        title: title("Le Word", 214, "4/6"),
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
fn puzzle_number_is_one_based_from_the_epoch() {
    assert_eq!(puzzle_number(DAY_EPOCH, DAY_EPOCH), 1);
    assert_eq!(
        puzzle_number(epoch(DailyPuzzle::LeWord), day(2026, 6, 19)),
        2
    );
    assert_eq!(puzzle_number(DAY_EPOCH, day(2026, 9, 7)), 150);
}

#[test]
fn day_card_marks_wins_in_lobby_order_and_shows_the_streak() {
    let card = day_card(
        day(2026, 9, 7),
        |puzzle| matches!(puzzle, DailyPuzzle::LeWord | DailyPuzzle::Sudoku),
        41,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Daily #150 · 2/7 · 🔥 41".to_string(),
            rows: vec![Row::Glyphs(vec![
                Glyph::Green,
                Glyph::Dark,
                Glyph::Dark,
                Glyph::Green,
                Glyph::Dark,
                Glyph::Dark,
                Glyph::Dark,
            ])],
        }
    );
}

#[test]
fn day_card_without_a_streak_omits_the_flame() {
    let card = day_card(DAY_EPOCH, |_| true, 0);
    assert_eq!(card.title, "late.sh Daily #1 · 7/7");
}

#[test]
fn ribbon_wraps_and_keeps_only_the_tail_that_fits() {
    let glyphs: Vec<Glyph> = (0..5)
        .map(|i| if i % 2 == 0 { Glyph::Green } else { Glyph::Red })
        .collect();
    assert_eq!(
        ribbon(&glyphs, 2),
        vec![
            Row::Glyphs(vec![Glyph::Green, Glyph::Red]),
            Row::Glyphs(vec![Glyph::Green, Glyph::Red]),
            Row::Glyphs(vec![Glyph::Green]),
        ]
    );
    let long = vec![Glyph::Blue; MAX_ROWS * 3 + 2];
    let rows = ribbon(&long, 3);
    assert_eq!(rows.len(), MAX_ROWS);
}

#[test]
fn half_block_picture_packs_two_rows_per_line() {
    let picture = vec![
        vec![true, false, true],
        vec![true, true, false],
        vec![false, false, true],
    ];
    assert_eq!(
        half_block_picture(&picture),
        vec![Row::Text("█▄▀".to_string()), Row::Text("  ▀".to_string())]
    );
}

#[test]
fn zz_print_every_card() {
    use crate::app::arcade::le_word::state::score_guess;
    use crate::app::arcade::minesweeper::state::Click;
    use crate::app::arcade::rubiks_cube::state::Face;
    use crate::app::arcade::solitaire::state::Suit;
    use late_core::models::chips::Difficulty;
    let d = day(2026, 9, 7);
    let mut out = String::new();
    let mut show = |name: &str, card: ShareCard| {
        out.push_str(&format!("\n=== {name}\n{}\n", render(&card, ShareFormat::Emoji)));
    };
    show("le word", crate::app::arcade::le_word::share::card(d, &[
        score_guess("adieu", "shade"), score_guess("shame", "shade"), score_guess("shade", "shade")], true));
    let pic: Vec<Vec<bool>> = (0..10).map(|r| (0..10).map(|c| {
        let (x, y) = (c as i32 - 4, r as i32 - 4); x * x + y * y <= 12 && !(y == -1 && (x == -2 || x == 2)) && !(y == 2 && x.abs() <= 2)
    }).collect()).collect();
    show("nonograms", crate::app::arcade::nonogram::share::card(d, "easy", &pic));
    show("sudoku", crate::app::arcade::sudoku::share::card(d, "medium", &[2, 1, 5, 3, 4, 9, 6, 8, 7]));
    let mut clicks = vec![Click::Safe; 14]; clicks[5] = Click::Flag; clicks[9] = Click::Boom; clicks[12] = Click::Flag;
    show("minesweeper", crate::app::arcade::minesweeper::share::card(d, "medium", 40, 2, &clicks));
    show("solitaire", crate::app::arcade::solitaire::share::card(d, 3, 612, &[
        (Some(Suit::Spades), 13), (Some(Suit::Hearts), 13), (Some(Suit::Diamonds), 13), (Some(Suit::Clubs), 13)]));
    let faces: Vec<Face> = "RUFLDBRRUFBDLURFDBRULFDBURRFLDBULFRD".chars().map(|ch| match ch {
        'U' => Face::Up, 'D' => Face::Down, 'F' => Face::Front, 'B' => Face::Back, 'R' => Face::Right, _ => Face::Left }).collect();
    show("rubik's cube", crate::app::arcade::rubiks_cube::share::card(d, faces.len() as u32, &faces));
    show("sliding puzzle", crate::app::arcade::sliding_puzzle::share::card(d, Difficulty::Medium, 143, &[
        1, 3, 6, 4, 2, 9, 12, 7, 0, 5, 11, 8, 0, 2, 6, 10]));
    show("day card", day_card(d, |p| matches!(p, DailyPuzzle::LeWord | DailyPuzzle::Sudoku | DailyPuzzle::Minesweeper | DailyPuzzle::Solitaire), 12));
    std::fs::write("/tmp/claude-1000/-home-mat-projects-late-sh/7d5004ad-eb2c-4da7-b59a-e9b43d874a64/scratchpad/cards.txt", out).unwrap();
}
