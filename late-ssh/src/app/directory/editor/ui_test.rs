//! The editor drawn for real: a row being typed into starts empty with the
//! cursor on the first letter of its hint, not in a cell before it.

use late_core::models::profile::Profile;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::state::{EditorState, Field, Page};
use super::ui::{EditorView, draw};

fn render(state: &EditorState) -> Vec<String> {
    let backend = TestBackend::new(100, 30);
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
