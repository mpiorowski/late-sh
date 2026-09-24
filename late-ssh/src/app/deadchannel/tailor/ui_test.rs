use rand::{SeedableRng, rngs::StdRng};

use super::{MirrorView, mirror_lines};
use crate::app::deadchannel::runner::state::{Look, Tint};
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
    let mut look = Look::random(&mut rng);
    look.hood.tint = Tint::Amber;
    let mut draft = Draft::new(look);
    draft.down();
    let screen = text_of(&mirror_lines(&MirrorView {
        draft: Some(&draft),
        word: Some("the tailor turns the mirror. that is you now."),
        changed: true,
        saving: false,
    }));

    assert!(screen.contains(&format!(" {} ", look.hood.piece.row)), "{screen}");
    assert!(screen.contains("amber"), "{screen}");
    assert!(screen.contains("▸ eyes"), "the cursor on the second row\n{screen}");
    assert!(!screen.contains("▸ hood"), "{screen}");
    assert!(
        screen.contains(&format!("[{}]", look.eyes.piece.row)),
        "the worn piece is bracketed on its rack\n{screen}"
    );
    assert!(screen.contains(&format!("[{}]", look.mark)), "{screen}");
    assert!(screen.contains("[s] wear it"), "{screen}");
    assert!(screen.contains("the tailor turns the mirror."), "{screen}");

    let empty = text_of(&mirror_lines(&MirrorView {
        draft: None,
        word: None,
        changed: false,
        saving: false,
    }));
    assert!(empty.contains("nothing looks back"), "{empty}");
}
