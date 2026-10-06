use super::state::ModModalState;
use crate::app::common::theme;
use crate::app::mod_modal::ui::*;
use ratatui::{Terminal, backend::TestBackend};
use ratatui::{buffer::Buffer, style::Modifier, text::Line};

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
fn wrapped_safety_help_keeps_the_piece_record_visible_and_history_scrollable() {
    let mut terminal = Terminal::new(TestBackend::new(80, 32)).expect("terminal");
    let mut state = ModModalState::new();
    state.append_info("earlier console history");
    state.open(true, Some("artboard safety"));
    let id = uuid::Uuid::new_v4();
    state.append_result(
        uuid::Uuid::new_v4(),
        true,
        vec![format!("Art id: {id}"), "NSFW (admin marks)".into()],
    );
    terminal
        .draw(|frame| draw(frame, frame.area(), &state))
        .expect("draw");
    text_position(terminal.backend().buffer(), &format!("Art id: {id}"));
    text_position(terminal.backend().buffer(), "NSFW (admin marks)");

    state.scroll_log(1000);
    terminal
        .draw(|frame| draw(frame, frame.area(), &state))
        .expect("draw");
    text_position(terminal.backend().buffer(), "earlier console history");
    text_position(terminal.backend().buffer(), "artboard safety view");
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
            presence: crate::app::chat::state::MatchPresence::Here,
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
            presence: crate::app::chat::state::MatchPresence::Here,
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

fn text_position(buffer: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        let text = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>();
        if let Some(start) = text.find(needle) {
            return (Line::from(&text[..start]).width() as u16, y);
        }
    }
    panic!("missing rendered text: {needle}");
}

#[test]
fn help_highlights_commands_arguments_and_section_titles_in_dark_and_light_themes() {
    for theme_id in ["contrast", "latte"] {
        theme::set_current_by_id(theme_id);
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");
        let mut state = ModModalState::new();
        state.open(true, None);
        terminal
            .draw(|frame| draw(frame, frame.area(), &state))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        let (x, y) = text_position(buffer, "rename-room <#oldname> <#newname>");
        let command = &buffer[(x, y)];
        let spacing = &buffer[(x + 11, y)];
        let argument = &buffer[(x + 12, y)];
        let punctuation_fg = theme::blend_toward(theme::TEXT_FAINT(), argument.bg, 1.0 / 3.0);
        assert_eq!(command.fg, theme::TEXT_BRIGHT());
        assert!(command.modifier.contains(Modifier::BOLD));
        assert_eq!(argument.fg, punctuation_fg);
        assert_eq!(buffer[(x + 13, y)].fg, theme::TEXT_DIM());
        assert_eq!(argument.bg, command.bg);
        assert_ne!(argument.bg, spacing.bg);
        assert!(!argument.modifier.contains(Modifier::BOLD));

        for token in ["<@user|#room|bans|slows|audit|art|help>", "[reason...]"] {
            let (x, y) = text_position(buffer, token);
            for (offset, ch) in token.chars().enumerate() {
                let expected = if matches!(ch, '<' | '>' | '[' | ']' | '|' | '.') {
                    punctuation_fg
                } else {
                    theme::TEXT_DIM()
                };
                assert_eq!(
                    buffer[(x + offset as u16, y)].fg,
                    expected,
                    "{theme_id}: {token}, {ch}"
                );
                assert_eq!(buffer[(x + offset as u16, y)].bg, argument.bg);
            }
        }

        let (x, y) = text_position(buffer, "======== Lounge");
        assert_eq!(buffer[(x, y)].fg, theme::TEXT_DIM());
        assert!(buffer[(x + 9, y)].modifier.contains(Modifier::BOLD));

        let (x, y) = text_position(buffer, "artboard curate");
        assert_eq!(buffer[(x + 9, y)].fg, theme::TEXT_MUTED());

        let (x, y) = text_position(buffer, "view help for art nsfw/sfw commands");
        assert_eq!(buffer[(x, y)].fg, theme::TEXT_DIM());
        assert_eq!(buffer[(x, y)].bg, spacing.bg);
    }
    theme::set_current_by_id("contrast");
}

#[test]
fn focused_help_keeps_explanations_plain_and_other_results_semantic() {
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");
    let mut state = ModModalState::new();
    let help_id = uuid::Uuid::now_v7();
    state.append_pending(help_id, true);
    state.append_result(
        help_id,
        true,
        crate::moderation::command::mod_help_lines(Some("artboard safety")),
    );
    state.append_result(uuid::Uuid::now_v7(), true, vec!["action completed".into()]);
    state.append_error("failed action");
    terminal
        .draw(|frame| draw(frame, frame.area(), &state))
        .expect("draw");
    let buffer = terminal.backend().buffer();

    let (x, y) = text_position(buffer, "artboard safety view");
    assert_eq!(buffer[(x, y)].fg, theme::TEXT_BRIGHT());
    let usage_background = buffer[(x, y)].bg;
    let (x, y) = text_position(buffer, "view @user lists");
    assert_eq!(buffer[(x, y)].fg, theme::TEXT_DIM());
    assert!(!buffer[(x, y)].modifier.contains(Modifier::BOLD));
    assert_ne!(buffer[(x, y)].bg, usage_background);
    let (x, y) = text_position(buffer, "action completed");
    assert_eq!(buffer[(x, y)].fg, theme::SUCCESS());
    let (x, y) = text_position(buffer, "failed action");
    assert_eq!(buffer[(x, y)].fg, theme::ERROR());
}
