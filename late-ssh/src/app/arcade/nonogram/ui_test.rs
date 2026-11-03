use super::*;
use crate::app::arcade::nonogram::state::Library;

/// The cursor is a solid amber fill like Solitaire's, so it reads on every
/// palette.
#[test]
fn cursor_is_a_solid_amber_fill() {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
    let mut state = State::new(
        uuid::Uuid::nil(),
        super::super::svc::NonogramService::new(db, tokio::sync::broadcast::channel(4).0),
        Library::default(),
        Vec::new(),
    );
    state.cursor = (0, 0);

    assert_eq!(
        cell_span(&state, 0, 0).style,
        theme::punch_through(theme::AMBER()).add_modifier(Modifier::BOLD)
    );
}

#[test]
fn every_fifth_boundary_is_a_guide_but_never_the_edge() {
    let guides = |len: usize| (0..len).filter(|&i| is_guide(i, len)).collect::<Vec<_>>();
    assert_eq!(guides(5), Vec::<usize>::new());
    assert_eq!(guides(10), [4]);
    assert_eq!(guides(15), [4, 9]);
}

/// A 10x10 board is ruled down its middle: a heavy column guide after the
/// fifth column, met by heavy junctions in the borders, and the fifth row
/// underlined.
#[test]
fn a_ten_by_ten_board_is_ruled_every_fifth_line() {
    let solution = vec![vec![1u8; 10]; 10];
    let (row_clues, col_clues) = late_core::nonogram::derive_clues(&solution);
    let puzzle = late_core::nonogram::NonogramPuzzle {
        id: "10x10-test".to_string(),
        width: 10,
        height: 10,
        row_clues,
        col_clues,
        solution,
        difficulty: "easy".to_string(),
        source: None,
        seed: None,
    };
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
    let state = State::new(
        uuid::Uuid::nil(),
        super::super::svc::NonogramService::new(db, tokio::sync::broadcast::channel(4).0),
        Library::default(),
        Vec::new(),
    );

    let lines = board_lines(&state, &puzzle);
    let text = |line: &Line<'_>| {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    };
    // One column-clue row (every clue is a single 10), then the top border.
    let top = text(&lines[1]);
    let first_row = text(&lines[2]);
    let fifth_row = &lines[6];
    let sixth_row = &lines[7];
    let bottom = text(lines.last().expect("bottom border"));

    assert_eq!(top.matches('┰').count(), 1, "{top}");
    assert_eq!(bottom.matches('┸').count(), 1, "{bottom}");
    assert_eq!(first_row.matches('┃').count(), 1, "{first_row}");
    let underlined = |line: &Line<'_>| {
        line.spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::UNDERLINED))
            .count()
    };
    assert!(underlined(fifth_row) > 0);
    assert_eq!(underlined(sixth_row), 0);
}
