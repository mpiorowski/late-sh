use super::state::{ModLogKind, ModLogLine, ModModalState};
use crate::app::mod_modal::ui::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn draw_log_keeps_latest_line_above_command_input() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut state = ModModalState::new();
    for idx in 0..40 {
        state.append_info(format!("line {idx:02}"));
    }

    terminal
        .draw(|frame| draw(frame, frame.area(), &state))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }

    assert!(
        text.contains("line 39"),
        "latest log line should render above the command box:\n{text}"
    );
}

#[test]
fn draw_mod_modal_renders_mention_autocomplete() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut state = ModModalState::new();
    state.update_autocomplete_matches(
        0,
        String::new(),
        vec![crate::app::chat::state::MentionMatch {
            name: "alice".to_string(),
            online: true,
            prefix: "@",
            description: None,
        }],
    );

    terminal
        .draw(|frame| draw(frame, frame.area(), &state))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }

    assert!(
        text.contains("@mentions") && text.contains("@alice"),
        "autocomplete popup should render above the mod command input:\n{text}"
    );
}

#[test]
fn draw_mod_modal_renders_room_autocomplete() {
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut state = ModModalState::new();
    state.update_autocomplete_matches(
        0,
        String::new(),
        vec![crate::app::chat::state::MentionMatch {
            name: "lounge".to_string(),
            online: true,
            prefix: "#",
            description: None,
        }],
    );

    terminal
        .draw(|frame| draw(frame, frame.area(), &state))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }

    assert!(
        text.contains("#rooms") && text.contains("#lounge"),
        "room autocomplete popup should render above the mod command input:\n{text}"
    );
}
