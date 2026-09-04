use chrono::NaiveDate;
use late_core::models::paper::{PaperEdition, PaperRoomPage, PaperStatus};
use ratatui::{Terminal, backend::TestBackend, style::Color};
use std::collections::HashSet;
use uuid::Uuid;

use super::draw;
use crate::app::common::theme;
use crate::app::paper::state::{PaperLayout, PaperModal};

fn edition() -> PaperEdition {
    PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        rooms: vec![PaperRoomPage {
            room_id: Uuid::from_u128(1),
            label: "lounge".to_string(),
            member_count: 11,
            kind: "lounge".to_string(),
            permanent: true,
            status: PaperStatus::Ready,
            message_count: 42,
            author_count: 3,
            text: Some("- lounge line one\n- lounge line two".to_string()),
        }],
        sections: Vec::new(),
    }
}

/// Build the modal exactly as the session tick does, under whatever theme
/// is currently set on this thread.
fn build(edition: &PaperEdition) -> PaperModal {
    let rail = [Uuid::from_u128(1)];
    let members: HashSet<Uuid> = rail.iter().copied().collect();
    PaperModal::edition(PaperLayout {
        edition,
        wall: None,
        rail_order: &rail,
        member_room_ids: &members,
        bumped_labels: &[],
    })
}

/// Every foreground colour the modal actually puts on screen, in cell order.
fn drawn_colors(modal: &PaperModal) -> Vec<Color> {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw(frame, frame.area(), modal))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut colors = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            if cell.symbol().trim().is_empty() {
                continue;
            }
            colors.push(cell.fg);
        }
    }
    colors
}

/// The session tick builds the modal, and `render` sets this thread's theme
/// afterwards, so `lay_out` reads whatever the previous render on this tokio
/// worker thread left behind: on a busy server, another reader's theme. Two
/// modals over the same edition, built under different ambient themes, must
/// still draw the same for one reader.
#[test]
fn the_paper_draws_in_the_readers_theme_whoever_built_it() {
    let edition = edition();

    theme::set_current_by_id("dracula");
    let built_on_a_borrowed_thread = build(&edition);
    theme::set_current_by_id("late");
    let built_at_home = build(&edition);

    // The reader's own render pass, both times.
    theme::set_current_by_id("late");
    let borrowed = drawn_colors(&built_on_a_borrowed_thread);
    theme::set_current_by_id("late");
    let home = drawn_colors(&built_at_home);

    assert_eq!(
        borrowed, home,
        "the paper took its colours from whichever session last rendered on this thread"
    );
}
