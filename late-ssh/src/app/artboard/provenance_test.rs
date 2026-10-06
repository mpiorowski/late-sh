use crate::app::artboard::provenance::*;
use dartboard_core::{Canvas, CanvasOp, Pos, RgbColor};

#[test]
fn paint_cell_tracks_last_writer() {
    let mut provenance = ArtboardProvenance::default();
    let before = Canvas::with_size(8, 4);

    provenance.apply_op(
        &before,
        &CanvasOp::PaintCell {
            pos: Pos { x: 2, y: 1 },
            ch: 'A',
            fg: RgbColor::new(1, 2, 3),
        },
        "mat",
    );

    let mut after = before.clone();
    after.set(Pos { x: 2, y: 1 }, 'A');
    assert_eq!(
        provenance.username_at(&after, Pos { x: 2, y: 1 }),
        Some("mat")
    );
}

#[test]
fn clear_cell_removes_last_writer() {
    let mut provenance = ArtboardProvenance::default();
    let mut before = Canvas::with_size(8, 4);
    before.set(Pos { x: 2, y: 1 }, 'A');
    provenance.set_username(Pos { x: 2, y: 1 }, "mat");

    provenance.apply_op(
        &before,
        &CanvasOp::ClearCell {
            pos: Pos { x: 2, y: 1 },
        },
        "mat",
    );

    let mut after = before.clone();
    after.clear(Pos { x: 2, y: 1 });
    assert_eq!(provenance.username_at(&after, Pos { x: 2, y: 1 }), None);
}

#[test]
fn replace_preserves_unchanged_authors_and_retags_changed_cells() {
    let mut provenance = ArtboardProvenance::default();
    let mut before = Canvas::with_size(8, 4);
    before.set(Pos { x: 1, y: 1 }, 'A');
    before.set(Pos { x: 2, y: 1 }, 'B');
    provenance.set_username(Pos { x: 1, y: 1 }, "alice");
    provenance.set_username(Pos { x: 2, y: 1 }, "bob");

    let mut after = before.clone();
    after.set(Pos { x: 2, y: 1 }, 'C');

    provenance.apply_op(
        &before,
        &CanvasOp::Replace {
            canvas: after.clone(),
        },
        "carol",
    );

    assert_eq!(
        provenance.username_at(&after, Pos { x: 1, y: 1 }),
        Some("alice")
    );
    assert_eq!(
        provenance.username_at(&after, Pos { x: 2, y: 1 }),
        Some("carol")
    );
}
