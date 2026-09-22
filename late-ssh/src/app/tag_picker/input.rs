//! Keys on the picker, and the two doors: `open` seeds it from the field
//! that asked, `close` hands the chosen tags back to that field.

use super::state::{TagPickerState, TagPickerTarget};
use crate::app::directory::editor::state::Field;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::state::App;

pub(crate) fn open(app: &mut App, target: TagPickerTarget) {
    let current = match target {
        TagPickerTarget::SettingsLangs => app.settings_modal_state.draft().langs.clone(),
        TagPickerTarget::EditorSkills => app.directory_editor.tags(Field::Skills).to_vec(),
        TagPickerTarget::EditorLangs => app.directory_editor.tags(Field::Langs).to_vec(),
        TagPickerTarget::JobPost => app.jobs.post.tags().to_vec(),
    };
    app.tag_picker.open(target, current);
}

/// Esc: the chosen tags land on the field that opened the picker. The
/// settings modal saves at once, as its rows do; the editor waits for
/// Ctrl+S like its other rows.
pub(crate) fn close(app: &mut App) {
    let Some((target, chosen)) = app.tag_picker.close() else {
        return;
    };
    match target {
        TagPickerTarget::SettingsLangs => app.settings_modal_state.set_langs(chosen),
        TagPickerTarget::EditorSkills => app.directory_editor.set_tags(Field::Skills, chosen),
        TagPickerTarget::EditorLangs => app.directory_editor.set_tags(Field::Langs, chosen),
        TagPickerTarget::JobPost => app.jobs.post.set_tags(chosen),
    }
}

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if matches!(event, ParsedInput::Byte(0x1B)) {
        close(app);
        return;
    }
    let picker: &mut TagPickerState = &mut app.tag_picker;
    match event {
        ParsedInput::Byte(b'\r') => picker.enter(),
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') => picker.toggle(),
        ParsedInput::Byte(0x7F | 0x08) => picker.backspace(),
        ParsedInput::Arrow(b'B') => picker.move_cursor(1),
        ParsedInput::Arrow(b'A') => picker.move_cursor(-1),
        ParsedInput::PageDown => picker.move_cursor(PAGE),
        ParsedInput::PageUp => picker.move_cursor(-PAGE),
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => picker.move_cursor(-3),
            MouseEventKind::ScrollDown => picker.move_cursor(3),
            MouseEventKind::Down
            | MouseEventKind::Up
            | MouseEventKind::Drag
            | MouseEventKind::Moved
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {}
        },
        ParsedInput::Char(ch) => picker.push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() => picker.push(byte as char),
        _ => {}
    }
}

/// How many tags a page key moves: about a popup's worth.
const PAGE: isize = 10;
