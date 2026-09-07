use dartboard_core::{Canvas, Pos, RgbColor};
use dartboard_editor::Bounds;
use late_core::models::artboard_piece::{PIECE_MAX_WIDTH, PIECE_MIN_GLYPHS};

use super::{Credit, FrameError, frame_piece};
use crate::app::artboard::provenance::ArtboardProvenance;

/// A board with `painter` owning a 10x5 block at (3, 2) and `other`
/// owning `others` glyphs in the row below it.
fn board(others: usize) -> (Canvas, ArtboardProvenance) {
    let mut canvas = Canvas::with_size(60, 20);
    let mut provenance = ArtboardProvenance::default();
    for y in 2..7 {
        for x in 3..13 {
            let pos = Pos { x, y };
            canvas.set_colored(pos, '#', RgbColor::new(255, 0, 0));
            provenance.set_username(pos, "painter");
        }
    }
    for x in 3..3 + others {
        let pos = Pos { x, y: 7 };
        canvas.set(pos, '*');
        provenance.set_username(pos, "other");
    }
    (canvas, provenance)
}

fn frame(min_x: usize, min_y: usize, max_x: usize, max_y: usize) -> Bounds {
    Bounds {
        min_x,
        max_x,
        min_y,
        max_y,
    }
}

#[test]
fn a_frame_crops_to_its_own_origin_and_credits_every_hand() {
    let (canvas, provenance) = board(5);
    let piece = frame_piece(&canvas, &provenance, frame(3, 2, 12, 7), "painter").expect("hangs");

    assert_eq!((piece.width, piece.height), (10, 6));
    assert_eq!(piece.glyph_count, 55);
    assert_eq!(piece.own_share_percent, 90);
    assert_eq!(piece.canvas.get(Pos { x: 0, y: 0 }), '#');
    assert_eq!(
        piece.canvas.fg(Pos { x: 0, y: 0 }),
        Some(RgbColor::new(255, 0, 0))
    );
    assert_eq!(piece.canvas.get(Pos { x: 0, y: 5 }), '*');
    assert_eq!(
        piece
            .provenance
            .username_at(&piece.canvas, Pos { x: 0, y: 5 }),
        Some("other")
    );
    assert_eq!(
        piece.credits,
        vec![
            Credit {
                username: "painter".to_string(),
                glyphs: 50
            },
            Credit {
                username: "other".to_string(),
                glyphs: 5
            },
        ]
    );
}

#[test]
fn the_hash_ignores_colour_and_position_on_the_board() {
    let (canvas, provenance) = board(0);
    let red = frame_piece(&canvas, &provenance, frame(3, 2, 12, 6), "painter").expect("hangs");

    let mut moved = Canvas::with_size(60, 20);
    let mut moved_provenance = ArtboardProvenance::default();
    for y in 10..15 {
        for x in 20..30 {
            let pos = Pos { x, y };
            moved.set(pos, '#');
            moved_provenance.set_username(pos, "painter");
        }
    }
    let plain =
        frame_piece(&moved, &moved_provenance, frame(20, 10, 29, 14), "painter").expect("hangs");
    assert_eq!(red.content_hash, plain.content_hash);

    let wider = frame_piece(&canvas, &provenance, frame(2, 2, 12, 6), "painter").expect("hangs");
    assert_ne!(
        red.content_hash, wider.content_hash,
        "the frame size is part of it"
    );
}

#[test]
fn the_rails_refuse_small_large_and_borrowed_frames() {
    let (canvas, provenance) = board(0);
    // 10x3 of the block: thirty glyphs, ten short of the floor.
    assert_eq!(
        frame_piece(&canvas, &provenance, frame(3, 2, 12, 4), "painter"),
        Err(FrameError::TooSmall { glyphs: 30 })
    );
    // 10x4 is exactly the floor and hangs.
    let at_floor = frame_piece(&canvas, &provenance, frame(3, 2, 12, 5), "painter").expect("hangs");
    assert_eq!(at_floor.glyph_count, PIECE_MIN_GLYPHS);

    assert_eq!(
        frame_piece(
            &canvas,
            &provenance,
            frame(0, 0, PIECE_MAX_WIDTH, 5),
            "painter"
        ),
        Err(FrameError::TooLarge {
            width: PIECE_MAX_WIDTH + 1,
            height: 6
        })
    );

    assert_eq!(
        frame_piece(&canvas, &provenance, frame(3, 2, 12, 6), "other"),
        Err(FrameError::OwnShareTooLow {
            percent: 0,
            largest_other: Some(Credit {
                username: "painter".to_string(),
                glyphs: 50
            }),
        })
    );
}
