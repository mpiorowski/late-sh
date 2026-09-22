use ratatui::{Terminal, backend::TestBackend};

use super::state::{TagPickerState, TagPickerTarget};
use super::ui::draw;

fn render(state: &TagPickerState) -> String {
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw(frame, frame.area(), state))
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
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_popup_shows_the_chosen_line_the_headings_and_the_cursor() {
    let mut picker = TagPickerState::default();
    picker.open(
        TagPickerTarget::EditorSkills,
        ["rust", "postgres"].map(str::to_string).to_vec(),
    );
    let screen = render(&picker);
    assert!(screen.contains("Pick skills"), "{screen}");
    assert!(
        screen.contains("chosen › rust · postgres  2 of 12"),
        "{screen}"
    );
    assert!(screen.contains("languages"), "{screen}");
    assert!(screen.contains("› ● rust   rustlang"), "{screen}");
    assert!(screen.contains("  ○ go   golang"), "{screen}");
    assert!(screen.contains("Esc"), "{screen}");

    for ch in "k8s".chars() {
        picker.push(ch);
    }
    let screen = render(&picker);
    assert!(screen.contains("search › k8s"), "{screen}");
    assert!(screen.contains("› ○ kubernetes   k8s, helm"), "{screen}");
    assert!(!screen.contains("languages"), "a query hides the headings");
}
