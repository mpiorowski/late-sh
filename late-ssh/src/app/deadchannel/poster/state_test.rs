use std::collections::HashSet;

use dartboard_core::{Canvas, Pos, RgbColor};
use dartboard_editor::MoveDir;

use super::*;
use crate::app::deadchannel::runner::state::{TINTS, Tint};

/// A canvas painted from `rows` (spaces are bare paper), every glyph in
/// `color`, at `origin`.
fn paint(canvas: &mut Canvas, origin: Pos, rows: &[&str], color: RgbColor) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            if ch != ' ' {
                canvas.put_glyph_colored(
                    Pos {
                        x: origin.x + dx,
                        y: origin.y + dy,
                    },
                    ch,
                    color,
                );
            }
        }
    }
}

/// Typed keys, a selection filled, copied, and pasted in another tint,
/// then saved and burned: the whole canvas after each step.
#[test]
fn a_draft_types_fills_a_selection_and_stamps_a_copy() {
    let mut draft = Draft::new(None, 0);
    for edit in [
        Edit::Type('H'),
        Edit::Type('I'),
        Edit::NextLine,
        Edit::Move(MoveDir::Left),
        Edit::Move(MoveDir::Left),
        Edit::Extend(MoveDir::Right),
        Edit::Extend(MoveDir::Right),
        Edit::Extend(MoveDir::Down),
        Edit::Type('#'),
        Edit::Copy,
        Edit::Deselect,
        Edit::Move(MoveDir::Right),
        Edit::Move(MoveDir::Right),
        Edit::Move(MoveDir::Right),
        Edit::Move(MoveDir::Down),
        Edit::Move(MoveDir::Down),
        Edit::Move(MoveDir::Down),
    ] {
        draft.edit(edit);
    }
    // The next tint round from white is static.
    draft.colour_key();
    assert_eq!(draft.brush, Brush::Tints(Tint::Static));
    draft.edit(Edit::Paste);

    let white = tint_paint(Tint::White);
    let mut expected = blank();
    paint(
        &mut expected,
        Pos { x: 0, y: 0 },
        &["HI", "###", "###"],
        white,
    );
    paint(
        &mut expected,
        Pos { x: 5, y: 5 },
        &["###", "###"],
        tint_paint(Tint::Static),
    );
    assert_eq!(draft.canvas, expected);
    assert_eq!(draft.editor.cursor, Pos { x: 5, y: 5 });
    assert!(!draft.selecting());
    assert!(draft.dirty());

    // Saved, it is what hangs; a stroke after it is not.
    draft.saved(draft.canvas.clone());
    assert!(!draft.dirty());
    draft.edit(Edit::EraseHere);
    let mut erased = expected.clone();
    erased.clear(Pos { x: 5, y: 5 });
    assert_eq!(draft.canvas, erased);
    assert!(draft.dirty());

    // Burned, the wall and the canvas are bare.
    draft.burned();
    assert_eq!(draft.canvas, blank());
    assert!(!draft.dirty());
}

#[test]
fn erasing_takes_the_selection_when_one_is_held() {
    let mut draft = Draft::new(None, 0);
    for edit in [
        Edit::Type('a'),
        Edit::Type('b'),
        Edit::Type('c'),
        Edit::Move(MoveDir::Left),
        Edit::Extend(MoveDir::Left),
        Edit::EraseBack,
    ] {
        draft.edit(edit);
    }
    let mut expected = blank();
    paint(
        &mut expected,
        Pos { x: 0, y: 0 },
        &["a"],
        tint_paint(Tint::White),
    );
    assert_eq!(draft.canvas, expected);
}

/// Without a mark the brush walks the tailor's seven and nothing else:
/// every stroke is one of them, the walk wraps, and no picker opens.
#[test]
fn the_tints_palette_only_ever_paints_the_seven() {
    let mut draft = Draft::new(None, 0);
    let first = draft.brush.clone();
    for _ in 0..TINTS.len() * 2 {
        draft.edit(Edit::Type('▓'));
        draft.colour_key();
        assert_eq!(draft.picker(), None);
    }
    assert_eq!(draft.brush, first);
    let seven: HashSet<RgbColor> = TINTS.into_iter().map(tint_paint).collect();
    let painted: HashSet<RgbColor> = (0..TINTS.len() * 2)
        .map(|x| draft.canvas.fg(Pos { x, y: 0 }).expect("a painted cell"))
        .collect();
    assert_eq!(painted, seven);
}

/// One mark opens every colour: `^K` opens the picker on the brush, Enter
/// takes what was mixed, and a closed picker leaves the brush alone.
#[test]
fn the_full_palette_paints_a_picked_colour() {
    let mut draft = Draft::new(None, 1);
    draft.colour_key();
    for ch in "c0ffee".chars() {
        draft.picker_mut().expect("the picker is open").type_hex(ch);
    }
    draft.apply_picker();
    assert_eq!(draft.picker(), None);
    draft.edit(Edit::Type('x'));

    draft.colour_key();
    for ch in "ff0000".chars() {
        draft.picker_mut().expect("the picker is open").type_hex(ch);
    }
    draft.close_picker();
    draft.edit(Edit::Type('y'));

    let mut expected = blank();
    paint(
        &mut expected,
        Pos { x: 0, y: 0 },
        &["xy"],
        RgbColor::new(0xc0, 0xff, 0xee),
    );
    assert_eq!(draft.canvas, expected);
}

#[test]
fn the_test_card_is_seven_bars_across_the_whole_poster() {
    let card = test_card(7);
    assert_eq!((card.width, card.height), (POSTER_WIDTH, POSTER_HEIGHT));
    let mut colours = HashSet::new();
    for y in 0..POSTER_HEIGHT {
        for x in 0..POSTER_WIDTH {
            let pos = Pos { x, y };
            assert!(
                matches!(card.get(pos), '█' | '░' | '▒' | '▓'),
                "{pos:?} is part of a bar"
            );
            colours.insert(card.fg(pos).expect("a bar has its tint"));
        }
    }
    assert_eq!(colours, TINTS.into_iter().map(tint_paint).collect());
    // The bars run left to right in the tailor's order.
    assert_eq!(card.fg(Pos { x: 0, y: 0 }), Some(tint_paint(TINTS[0])));
    assert_eq!(
        card.fg(Pos {
            x: POSTER_WIDTH - 1,
            y: POSTER_HEIGHT - 1
        }),
        Some(tint_paint(TINTS[TINTS.len() - 1]))
    );
    // The static is the seed's: the same runner, the same card.
    assert_eq!(test_card(7), card);
    assert_ne!(test_card(8), card);
}

/// Ink is any glyph but a space: bare paper and typed spaces are none, a
/// single letter or the test card is.
#[test]
fn ink_is_any_glyph_but_a_space() {
    let mut spaces = blank();
    spaces.put_glyph(Pos { x: 3, y: 2 }, ' ');
    let mut letter = blank();
    letter.put_glyph(Pos { x: 31, y: 11 }, 'x');

    assert!(!has_ink(&blank()), "bare paper");
    assert!(!has_ink(&spaces), "typed spaces");
    assert!(has_ink(&letter), "one letter in the corner");
    assert!(has_ink(&test_card(7)), "the test card");
}

#[test]
fn a_stored_canvas_of_another_size_is_clipped_to_the_poster() {
    let mut stored = Canvas::with_size(40, 20);
    let red = RgbColor::new(255, 0, 0);
    stored.put_glyph_colored(Pos { x: 31, y: 11 }, 'k', red);
    stored.put_glyph_colored(Pos { x: 32, y: 0 }, 'o', red);
    stored.put_glyph_colored(Pos { x: 0, y: 12 }, 'o', red);
    stored.put_glyph(Pos { x: 3, y: 3 }, 'p');

    let draft = Draft::new(Some(stored), 0);
    let mut expected = blank();
    expected.put_glyph_colored(Pos { x: 31, y: 11 }, 'k', red);
    expected.put_glyph(Pos { x: 3, y: 3 }, 'p');
    assert_eq!(draft.canvas, expected);
    assert!(!draft.dirty());
}

/// A panel too short for the whole poster: walking the cursor below the
/// rows on screen must not move where the next click lands, since the
/// panel always draws the poster from its top left.
#[test]
fn a_click_on_a_cut_off_poster_lands_on_the_cell_under_the_pointer() {
    use dartboard_editor::{AppModifiers, AppPointerButton, AppPointerKind};

    let mut draft = Draft::new(None, 0);
    // A body six rows tall at frame cell (10, 4): the poster's top half.
    draft.record_drawn_at(Rect {
        x: 10,
        y: 4,
        width: 60,
        height: 6,
    });
    let click = |draft: &mut Draft, column: u16, row: u16| {
        for kind in [
            AppPointerKind::Down(AppPointerButton::Left),
            AppPointerKind::Up(AppPointerButton::Left),
        ] {
            draft.point(AppPointerEvent {
                column,
                row,
                kind,
                modifiers: AppModifiers::default(),
            });
        }
    };

    click(&mut draft, 12, 6);
    for _ in 0..8 {
        draft.edit(Edit::Move(MoveDir::Down));
    }
    click(&mut draft, 15, 5);
    draft.edit(Edit::Type('X'));

    let mut expected = blank();
    expected.put_glyph_colored(Pos { x: 5, y: 1 }, 'X', tint_paint(Tint::White));
    assert_eq!(draft.canvas, expected);
}
