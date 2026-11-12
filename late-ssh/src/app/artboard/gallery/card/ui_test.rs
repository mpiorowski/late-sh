use chrono::NaiveDate;
use dartboard_core::{Canvas, Pos};
use uuid::Uuid;

use super::lounge_card_lines;
use crate::app::artboard::gallery::frame::Credit;
use crate::app::artboard::gallery::svc::GalleryPiece;

fn piece(width: usize, height: usize) -> GalleryPiece {
    let mut canvas = Canvas::with_size(width, height);
    canvas.set(Pos { x: 0, y: 0 }, '#');
    canvas.set(
        Pos {
            x: width - 1,
            y: height - 1,
        },
        '@',
    );
    GalleryPiece {
        id: Uuid::nil(),
        user_id: Uuid::nil(),
        username: "artist".to_string(),
        title: "Test Art".to_string(),
        width,
        height,
        canvas,
        credits: vec![
            Credit {
                username: "artist".to_string(),
                glyphs: 30,
            },
            Credit {
                username: "bob".to_string(),
                glyphs: 10,
            },
        ],
        period_month: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        applause: 5,
        applauded_by_viewer: false,
        created: chrono::DateTime::UNIX_EPOCH,
    }
}

fn rows(piece: &GalleryPiece, width: usize, gated: bool) -> Vec<String> {
    lounge_card_lines(piece, width, gated)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_piece_that_fits_is_shown_whole_in_its_own_cells() {
    assert_eq!(
        rows(&piece(10, 3), 70, false),
        vec![
            "🎨 \"Test Art\"",
            "#         ",
            "          ",
            "         @",
            "10x3 · with @bob 25% · 5 applause · Enter to view",
            "hang a piece on Artboard (page 4) · monthly prizes up to 40,000 chips",
        ]
    );
}

#[test]
fn a_larger_piece_shows_its_top_left_and_its_size_says_so() {
    let rows = rows(&piece(90, 20), 70, false);
    // Title, twelve rows of the piece, the meta row, the CTA.
    assert_eq!(rows.len(), 15);
    assert_eq!(rows[1], format!("#{}", " ".repeat(69)));
    assert!(
        rows[1..13].iter().all(|row| !row.contains('@')),
        "the far corner is past the cut: {rows:?}"
    );
    assert_eq!(
        rows[13],
        "top left of 90x20 · with @bob 25% · 5 applause · Enter to view"
    );
}

#[test]
fn a_gated_card_names_the_piece_without_showing_it() {
    assert_eq!(
        rows(&piece(10, 3), 80, true),
        vec![
            "🎨 \"Test Art\"",
            "Artboard content disclaimer is on · Enter to view on the Artboard",
            "10x3 · with @bob 25% · 5 applause · Enter to view",
            "hang a piece on Artboard (page 4) · monthly prizes up to 40,000 chips",
        ]
    );
}

#[test]
fn a_narrow_chat_drops_the_preview_and_cuts_every_row_to_fit() {
    let rows = rows(&piece(10, 3), 16, false);
    assert_eq!(
        rows,
        vec!["🎨 \"Test Art\"", "10x3 · with @bo…", "hang a piece on…"]
    );
}
