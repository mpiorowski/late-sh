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
    let look = Look::random(&mut rng);
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
