//! The editor drawn for real: a row being typed into starts empty with the
//! cursor on the first letter of its hint, not in a cell before it.

use late_core::models::profile::Profile;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::state::{EditorState, Field, Page};
use super::ui::{EditorView, draw};

fn render(state: &EditorState) -> Vec<String> {
    render_at(state, 100, 30)
}

fn render_at(state: &EditorState, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw(
                frame,
                frame.area(),
                &EditorView {
                    state,
                    projects: &[],
                    viewer_name: "mat",
                },
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

#[test]
fn an_empty_row_being_typed_puts_the_cursor_on_the_hints_first_letter() {
    let mut editor = EditorState::default();
    editor.open_own(Uuid::now_v7(), None, &Profile::default(), Page::About);
    editor.set_row(1);
    assert_eq!(editor.active_field(), Some(Field::Ide));

    let idle = render(&editor);
    let idle_row = idle
        .iter()
        .find(|line| line.contains("ide "))
        .expect("ide row");
    let hint_at = idle_row.find("nvim, vscode").expect("hint shown when idle");

    editor.start_editing();
    let typing = render(&editor);
    let typing_row = typing
        .iter()
        .find(|line| line.contains("ide "))
        .expect("ide row");
    assert_eq!(
        typing_row.find("nvim, vscode"),
        Some(hint_at),
        "the hint must not shift right when typing starts:\n{}",
        typing.join("\n")
    );
}

#[test]
fn short_editor_scrolls_full_fields_under_the_wheel() {
    use super::state::MouseTarget;
    let mut editor = EditorState::default();
    editor.open_own(Uuid::now_v7(), None, &Profile::default(), Page::Card);
    render_at(&editor, 50, 16);
    editor.mouse.scroll(10, 8, 100, (50, 16));
    let lines = render_at(&editor, 50, 16);
    let (rect, _) = editor
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| *target == MouseTarget::Field(Field::Summary))
        .expect("summary revealed by wheel");
    assert!(lines[usize::from(rect.y)].contains("summary"));
    assert!(rect.bottom() <= 16);
    assert_eq!(editor.row(), 0, "wheel did not move keyboard selection");
}

#[test]
fn an_idle_row_shows_the_start_of_a_long_value() {
    let mut editor = EditorState::default();
    editor.open_own(Uuid::now_v7(), None, &Profile::default(), Page::About);
    editor.set_row(1);
    assert_eq!(editor.active_field(), Some(Field::Ide));
    editor.start_editing();
    editor
        .field_mut(Field::Ide)
        .insert_str(format!("start-{}-end", "x".repeat(150)));
    editor.stop_editing();

    let idle = render(&editor);
    let row = idle
        .iter()
        .find(|line| line.contains("ide "))
        .expect("ide row");
    assert!(
        row.contains("start-"),
        "an idle row begins at its start:\n{row}"
    );
    assert!(!row.contains("-end"), "not at its end:\n{row}");
}
