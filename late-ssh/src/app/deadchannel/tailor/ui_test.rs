use rand::{SeedableRng, rngs::StdRng};

use super::{MirrorView, mirror_lines};
use crate::app::deadchannel::runner::state::{Look, Slot, Tint, unlocked_pieces};
use crate::app::deadchannel::tailor::state::Draft;

fn text_of(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_mirror_shows_the_draft_with_the_cursor_row_and_its_rack() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut look = Look::random(1, &mut rng);
    look.hood.tint = Tint::Amber;
    let mut draft = Draft::new(look, 1);
    draft.down();
    let screen = text_of(&mirror_lines(&MirrorView {
        draft: Some(&draft),
        word: Some("the tailor turns the mirror. that is you now."),
        changed: true,
        saving: false,
    }));

    assert!(
        screen.contains(&format!(" {} ", look.hood.piece.row)),
        "{screen}"
    );
    assert!(screen.contains("amber"), "{screen}");
    assert!(
        screen.contains("▸ eyes"),
        "the cursor on the second row\n{screen}"
    );
    assert!(!screen.contains("▸ hood"), "{screen}");
    assert!(
        screen.contains(&format!("[{}]", look.eyes.piece.row)),
        "the worn piece is bracketed on its rack\n{screen}"
    );
    assert!(screen.contains(&format!("[{}]", look.mark)), "{screen}");
    assert!(screen.contains("[s] wear it"), "{screen}");
    assert!(screen.contains("the tailor turns the mirror."), "{screen}");
    assert!(
        screen.contains("level 4 opens 9 new pieces and phosphor."),
        "the next unlock is named\n{screen}"
    );

    let top = Draft::new(look, 15);
    let screen = text_of(&mirror_lines(&MirrorView {
        draft: Some(&top),
        word: None,
        changed: false,
        saving: false,
    }));
    assert!(screen.contains("the whole rack is yours."), "{screen}");

    let empty = text_of(&mirror_lines(&MirrorView {
        draft: None,
        word: None,
        changed: false,
        saving: false,
    }));
    assert!(empty.contains("nothing looks back"), "{empty}");
}

/// A rack shorter than the window is listed once, not wrapped around
/// itself: at level 1 the three street pieces show as three, the worn
/// one bracketed.
#[test]
fn a_short_rack_shows_each_piece_once() {
    let mut rng = StdRng::seed_from_u64(1);
    let look = Look::random(1, &mut rng);
    let draft = Draft::new(look, 1);
    let screen = text_of(&mirror_lines(&MirrorView {
        draft: Some(&draft),
        word: None,
        changed: false,
        saving: false,
    }));
    let hood_line = screen.lines().next().expect("the hood row");
    let rack = hood_line.split_once("◂ ").expect("a rack").1;
    for piece in unlocked_pieces(Slot::Hood, 1) {
        assert_eq!(
            rack.matches(piece.row).count(),
            1,
            "{} once on the rack\n{screen}",
            piece.code
        );
    }
    assert!(
        rack.contains(&format!("[{}]", look.hood.piece.row)),
        "{screen}"
    );
}
