use rand::{SeedableRng, rngs::StdRng};

use super::{Draft, Row};
use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::{Look, Slot, TINTS, Tint, pieces_for};

fn draft() -> Draft {
    // A fixed look: the first piece of every rack in the first tint, the
    // first mark, so every step below lands on a known neighbour.
    let mut rng = StdRng::seed_from_u64(1);
    let mut look = Look::random(&mut rng);
    for slot in [Slot::Hood, Slot::Eyes, Slot::Coat] {
        let first = pieces_for(slot).next().expect("a piece");
        let worn = match slot {
            Slot::Hood => &mut look.hood,
            Slot::Eyes => &mut look.eyes,
            Slot::Coat => &mut look.coat,
        };
        worn.piece = first;
        worn.tint = TINTS[0];
    }
    look.mark = GLYPH_ALPHABET[0];
    Draft::new(look)
}

#[test]
fn the_cursor_walks_the_rows_and_holds_at_both_ends() {
    let mut draft = draft();
    assert_eq!(draft.row, Row::Hood);
    draft.up();
    assert_eq!(draft.row, Row::Hood, "the top holds");
    draft.down();
    draft.down();
    draft.down();
    assert_eq!(draft.row, Row::Mark);
    draft.down();
    assert_eq!(draft.row, Row::Mark, "the bottom holds");
    draft.up();
    assert_eq!(draft.row, Row::Coat);
}

#[test]
fn the_rack_wraps_both_ways_and_only_the_cursor_row_moves() {
    let mut draft = draft();
    let before = draft.look;
    let hoods: Vec<_> = pieces_for(Slot::Hood).collect();

    draft.next();
    assert_eq!(draft.look.hood.piece, hoods[1]);
    assert_eq!(draft.look.eyes, before.eyes, "the other rows hold");
    assert_eq!(draft.look.coat, before.coat);
    assert_eq!(draft.look.mark, before.mark);

    draft.prev();
    draft.prev();
    assert_eq!(
        draft.look.hood.piece,
        hoods[hoods.len() - 1],
        "wraps backward"
    );
    draft.next();
    assert_eq!(draft.look, before, "and forward again");

    draft.down();
    draft.down();
    draft.down();
    draft.prev();
    assert_eq!(
        draft.look.mark,
        GLYPH_ALPHABET[GLYPH_ALPHABET.len() - 1],
        "the mark row walks the alphabet"
    );
    assert_eq!(draft.look.hood, before.hood);
}

#[test]
fn tint_cycles_the_palette_and_never_touches_the_mark() {
    let mut draft = draft();
    assert_eq!(draft.look.hood.tint, Tint::Static);
    draft.tint();
    assert_eq!(draft.look.hood.tint, Tint::Amber);
    for _ in 0..4 {
        draft.tint();
    }
    assert_eq!(
        draft.look.hood.tint,
        Tint::Static,
        "five tints, back around"
    );

    draft.down();
    draft.down();
    draft.down();
    let before = draft.look;
    draft.tint();
    assert_eq!(draft.look, before, "the mark has no tint to pick");
}

#[test]
fn shuffle_is_the_join_dice_and_keeps_the_cursor() {
    let mut draft = draft();
    draft.down();
    let mut rng = StdRng::seed_from_u64(9);
    draft.shuffle(&mut rng);
    let mut same_dice = StdRng::seed_from_u64(9);
    assert_eq!(draft.look, Look::random(&mut same_dice));
    assert_eq!(draft.row, Row::Eyes);
}
