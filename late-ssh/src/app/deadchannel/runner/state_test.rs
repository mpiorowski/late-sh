use rand::SeedableRng;
use rand::rngs::StdRng;
use unicode_width::UnicodeWidthChar;

use super::*;

/// The hard rule behind the whole catalog: a row is exactly five cells
/// and none of them is a wide glyph (CJK, emoji), so the gutter math in
/// the chat rows holds. This does not, and cannot, check East Asian
/// ambiguous width: `width()` counts that whole class as one cell, and
/// the table is made of it, like the rest of the TUI's box drawing.
#[test]
fn every_piece_row_is_five_cells_and_never_wide() {
    for piece in PIECES {
        let cells = piece.row.chars().count();
        assert_eq!(
            cells, PORTRAIT_WIDTH,
            "{} has {cells} cells, want {PORTRAIT_WIDTH}",
            piece.code
        );
        for glyph in piece.row.chars() {
            assert_eq!(
                glyph.width(),
                Some(1),
                "{} carries {glyph:?}, which is not single width",
                piece.code
            );
        }
    }
}

#[test]
fn piece_codes_are_unique_and_prefixed_by_slot() {
    let mut seen = std::collections::HashSet::new();
    for piece in PIECES {
        assert!(seen.insert(piece.code), "{} listed twice", piece.code);
        let prefix = match piece.slot {
            Slot::Hood => "hood.",
            Slot::Eyes => "eyes.",
            Slot::Coat => "coat.",
        };
        assert!(
            piece.code.starts_with(prefix),
            "{} sits in {:?} but is not prefixed {prefix}",
            piece.code,
            piece.slot
        );
    }
}

#[test]
fn a_random_look_round_trips_through_json() {
    let mut rng = StdRng::seed_from_u64(7);
    let look = Look::random(1, &mut rng);
    assert_eq!(look.hood.piece.slot, Slot::Hood);
    assert_eq!(look.eyes.piece.slot, Slot::Eyes);
    assert_eq!(look.coat.piece.slot, Slot::Coat);
    assert!(GLYPH_ALPHABET.contains(&look.mark));

    let json = look.to_json();
    assert_eq!(Look::parse(&json), Ok(look));
}

#[test]
fn the_stored_shape_is_the_documented_contract() {
    let json = serde_json::json!({
        "hood": {"piece": "hood.cross", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "▚"}
    });
    let look = Look::parse(&json).expect("parse");
    assert_eq!(look.hood.piece.code, "hood.cross");
    assert_eq!(look.hood.tint, Tint::Amber);
    assert_eq!(look.eyes.piece.code, "eyes.gem");
    assert_eq!(look.eyes.tint, Tint::White);
    assert_eq!(look.coat.piece.code, "coat.heavy");
    assert_eq!(look.coat.tint, Tint::Static);
    assert_eq!(look.mark, '▚');
    assert_eq!(look.to_json(), json);
}

#[test]
fn unknown_pieces_and_marks_are_rejected_loudly() {
    let unknown_piece = serde_json::json!({
        "hood": {"piece": "hood.nope", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "▚"}
    });
    assert_eq!(
        Look::parse(&unknown_piece),
        Err(LookError::UnknownPiece {
            slot: Slot::Hood,
            code: "hood.nope".to_string()
        })
    );

    // A coat code in the hood slot is unknown too: codes are per slot.
    let wrong_slot = serde_json::json!({
        "hood": {"piece": "coat.heavy", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "▚"}
    });
    assert_eq!(
        Look::parse(&wrong_slot),
        Err(LookError::UnknownPiece {
            slot: Slot::Hood,
            code: "coat.heavy".to_string()
        })
    );

    let bad_mark = serde_json::json!({
        "hood": {"piece": "hood.cross", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "x"}
    });
    assert_eq!(Look::parse(&bad_mark), Err(LookError::UnknownMark('x')));

    assert!(matches!(
        Look::parse(&serde_json::json!({"hood": "hood.cross"})),
        Err(LookError::Shape(_))
    ));
}

/// The unlock ladder, whole: every three levels the rack opens three more
/// pieces per slot and a tint, white alone at the top, and the tables
/// list each slot street first, so the tailor's rack walks up the ladder.
#[test]
fn the_rack_opens_three_pieces_per_slot_and_a_tint_every_three_levels() {
    for slot in [Slot::Hood, Slot::Eyes, Slot::Coat] {
        let levels = pieces_for(slot)
            .map(|piece| piece.level)
            .collect::<Vec<_>>();
        assert_eq!(
            levels,
            vec![1, 1, 1, 4, 4, 4, 7, 7, 7, 10, 10, 10, 13, 13, 13],
            "{slot:?}"
        );
    }
    assert_eq!(TINTS.map(Tint::level), [1, 1, 4, 7, 10, 13, 15]);
    for tint in TINTS {
        assert_eq!(
            serde_json::to_value(tint).expect("a tint serializes"),
            serde_json::json!(tint.name()),
            "the tailor prints the stored name"
        );
    }
    for unlock in UNLOCK_LEVELS {
        let opens = PIECES.iter().any(|piece| piece.level == unlock)
            || TINTS.iter().any(|tint| tint.level() == unlock);
        assert!(
            opens,
            "level {unlock} is an unlock level that opens nothing"
        );
    }
    assert_eq!(
        unlocked_tints(1).collect::<Vec<_>>(),
        vec![Tint::Static, Tint::Amber]
    );
    assert_eq!(unlocked_pieces(Slot::Hood, 6).count(), 6);
    assert_eq!(next_unlock(1), Some(4));
    assert_eq!(next_unlock(12), Some(13));
    assert_eq!(next_unlock(13), Some(15));
    assert_eq!(next_unlock(15), None);
}

#[test]
fn a_random_look_wears_only_what_the_level_unlocked() {
    for level in [1, 7] {
        for seed in 0..200 {
            let look = Look::random(level, &mut StdRng::seed_from_u64(seed));
            for worn in look.rows() {
                assert!(
                    unlocked_pieces(worn.piece.slot, level).any(|piece| piece == worn.piece)
                        && worn.tint.level() <= level,
                    "level {level} seed {seed} wears {} in {:?}",
                    worn.piece.code,
                    worn.tint
                );
            }
        }
    }
}
