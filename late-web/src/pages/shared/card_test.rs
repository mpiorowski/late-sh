use dartboard_core::{Pos, RgbColor};

use super::card::{CARD_COLUMNS, CardLine, Ink, canvas, paragraph};

/// A paragraph wraps on words at the card's width, breaks a word wider
/// than a line, and a cut one ends in an ellipsis on its last line.
#[test]
fn a_paragraph_wraps_to_the_card_and_is_cut_with_an_ellipsis() {
    let words = "word ".repeat(30);
    let lines: Vec<String> = paragraph(Ink::Body, &words, 9)
        .into_iter()
        .map(|line| line.text)
        .collect();
    let full = "word ".repeat(13);
    assert_eq!(
        lines,
        vec![full.trim_end(), full.trim_end(), "word word word word"]
    );

    let long = "x".repeat(CARD_COLUMNS + 3);
    let lines: Vec<String> = paragraph(Ink::Body, &long, 9)
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(lines, vec!["x".repeat(CARD_COLUMNS), "xxx".to_string()]);

    let cut: Vec<String> = paragraph(Ink::Body, &words, 2)
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(
        cut,
        vec![
            full.trim_end().to_string(),
            format!("{}…", "word ".repeat(12).trim_end())
        ]
    );
}

/// The card's canvas: the text inside a two-cell margin, each line in
/// its ink, the body in the board's default.
#[test]
fn the_card_draws_each_line_in_its_ink_inside_the_margin() {
    let card = canvas(&[
        CardLine::new(Ink::Title, "HI"),
        CardLine::blank(),
        CardLine::new(Ink::Body, "ok"),
    ]);
    assert_eq!((card.width, card.height), (CARD_COLUMNS + 4, 7));
    assert_eq!(card.get(Pos { x: 2, y: 2 }), 'H');
    assert_eq!(
        card.fg(Pos { x: 3, y: 2 }),
        Some(RgbColor::new(0x4a, 0xde, 0x80))
    );
    assert_eq!(card.get(Pos { x: 2, y: 4 }), 'o');
    assert_eq!(card.fg(Pos { x: 2, y: 4 }), None);
    assert_eq!(card.get(Pos { x: 1, y: 2 }), ' ');
}
