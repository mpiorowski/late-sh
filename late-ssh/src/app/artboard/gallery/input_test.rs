use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::*;
use crate::app::artboard::{
    provenance::ArtboardProvenance,
    state::PAINT_PALETTE,
    svc::{ArtboardSnapshotService, DartboardService, DartboardSnapshot},
};

fn test_state() -> State {
    let snapshot = DartboardSnapshot {
        provenance: ArtboardProvenance::default(),
        your_user_id: Some(1),
        your_color: Some(PAINT_PALETTE[1]),
        ..Default::default()
    };
    State::new(
        DartboardService::disconnected_for_tests(snapshot),
        ArtboardSnapshotService::disabled(),
        crate::app::artboard::gallery::svc::GalleryService::disabled(),
        uuid::Uuid::nil(),
        "viewer".to_string(),
        ArtboardProvenance::default().shared(),
    )
}

/// The 0-based terminal row the rail drew `label` on.
fn drawn_row(state: &State, label: &str) -> u16 {
    let mut terminal = Terminal::new(TestBackend::new(40, 16)).expect("terminal");
    terminal
        .draw(|frame| {
            super::super::ui::draw_rail(frame, Rect::new(0, 2, 21, 14), state);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .find(|&y| {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            line.contains(label)
        })
        .unwrap_or_else(|| panic!("{label:?} not drawn"))
}

fn left_click(column: u16, row: u16) -> ParsedInput {
    // SGR mouse reports are 1-based.
    ParsedInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down,
        button: Some(MouseButton::Left),
        x: column + 1,
        y: row + 1,
        modifiers: Default::default(),
    })
}

#[test]
fn clicking_a_rail_row_picks_the_row_drawn_under_the_pointer() {
    let mut state = test_state();
    let daily = RailRow::Archive(ArtboardSnapshotKind::Daily);
    let row = drawn_row(&state, daily.label());

    let action = handle_event(&mut state, (80, 24), &left_click(3, row));

    assert_eq!(
        action,
        GalleryAction::OpenArchive(ArtboardSnapshotKind::Daily)
    );
    assert_eq!(state.gallery().selected_row(), daily);
}
