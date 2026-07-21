use super::*;

#[test]
fn paste_into_command_input_strips_markers_and_normalizes_newlines_to_spaces() {
    let mut state = ModModalState::new();

    paste_into_command_input(
        &mut state,
        b"\x1b[200~ban server @alice\r\npolicy\x00\x7f\x1b[201~",
    );

    assert_eq!(state.command_text(), "ban server @alice policy");
}
