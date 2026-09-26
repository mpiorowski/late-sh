use rand::{SeedableRng, rngs::StdRng};

use super::{Draft, Row};
use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::{
    Look, Slot, TINTS, Tint, pieces_for, unlocked_pieces,
};

fn draft(level: i32) -> Draft {
    // A fixed look: the first piece of every rack in the first tint, the
    // first mark, so every step below lands on a known neighbour.
    let mut rng = StdRng::seed_from_u64(1);
    let mut look = Look::random(1, &mut rng);
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
    Draft::new(look, level)
}

#[test]
fn the_cursor_walks_the_rows_and_holds_at_both_ends() {
    let mut draft = draft(15);
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
    let mut draft = draft(15);
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
    let mut draft = draft(15);
    let mut seen = vec![draft.look.hood.tint];
    for _ in 0..TINTS.len() {
        draft.tint();
        seen.push(draft.look.hood.tint);
    }
    assert_eq!(
        seen,
        [TINTS.as_slice(), &[Tint::Static]].concat(),
        "the whole palette at the top, back around"
    );

    draft.down();
    draft.down();
    draft.down();
    let before = draft.look;
    draft.tint();
    assert_eq!(draft.look, before, "the mark has no tint to pick");
}

#[test]
fn shuffle_is_the_join_dice_at_the_drafts_level_and_keeps_the_cursor() {
    let mut draft = draft(7);
    draft.down();
    let mut rng = StdRng::seed_from_u64(9);
    draft.shuffle(&mut rng);
    let mut same_dice = StdRng::seed_from_u64(9);
    assert_eq!(draft.look, Look::random(7, &mut same_dice));
    assert_eq!(draft.row, Row::Eyes);
}

/// The gate: a level-1 runner's racks hold the three street pieces and
/// the two first tints, wrapping, and nothing past them; level 4 opens
/// the next three and phosphor.
#[test]
fn the_racks_are_cut_to_the_level() {
    let mut first = draft(1);
    let hoods: Vec<_> = unlocked_pieces(Slot::Hood, 1).collect();
    assert_eq!(hoods.len(), 3);
    let mut walked = vec![first.look.hood.piece];
    for _ in 0..hoods.len() {
        first.next();
        walked.push(first.look.hood.piece);
    }
    assert_eq!(walked, [hoods.as_slice(), &[hoods[0]]].concat());

    let mut tints = vec![first.look.hood.tint];
    for _ in 0..2 {
        first.tint();
        tints.push(first.look.hood.tint);
    }
    assert_eq!(tints, vec![Tint::Static, Tint::Amber, Tint::Static]);

    let mut fourth = draft(4);
    fourth.prev();
    assert_eq!(
        fourth.look.hood.piece.level, 4,
        "wraps back to level 4's last"
    );
    fourth.tint();
    fourth.tint();
    assert_eq!(fourth.look.hood.tint, Tint::Phosphor);
    fourth.tint();
    assert_eq!(fourth.look.hood.tint, Tint::Static);
}

/// A look off the rack (written before the gate, or by an older replica
/// mid-deploy) is snapped onto it at open: the offending piece and tint
/// fall to the rack's first entry, the rest of the look stays, and the
/// mirror's walks never panic on it.
#[test]
fn a_look_above_the_rack_is_snapped_onto_it_at_open() {
    let mut look = draft(1).look;
    look.hood.piece = pieces_for(Slot::Hood).last().expect("a piece");
    look.hood.tint = Tint::White;
    assert_eq!(look.hood.piece.level, 13);

    let mut snapped = Draft::new(look, 1);
    let first = unlocked_pieces(Slot::Hood, 1).next().expect("a piece");
    assert_eq!(snapped.look.hood.piece, first);
    assert_eq!(snapped.look.hood.tint, Tint::Static);
    assert_eq!(
        snapped.look.eyes, look.eyes,
        "an unlocked slot is untouched"
    );
    assert_eq!(snapped.look.coat, look.coat);
    assert_eq!(snapped.look.mark, look.mark);
    snapped.next();
    snapped.tint();

    let kept = Draft::new(look, 13);
    assert_eq!(
        kept.look.hood.piece, look.hood.piece,
        "the piece is on the rack at 13"
    );
    assert_eq!(kept.look.hood.tint, Tint::Static, "white is not, until 15");
    assert_eq!(
        Draft::new(look, 15).look,
        look,
        "nothing to snap at the top"
    );
}
