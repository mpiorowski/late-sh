use late_core::models::character_sheet::{SHEET_BODY_MAX_CHARS, SHEET_NAME_MAX_CHARS};

use crate::app::{
    common::textarea_input::{EditOutcome, handle_multiline_edit, handle_single_line_edit},
    input::{MouseEventKind, ParsedInput},
    state::App,
};

use super::state::SheetField;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.sheet_modal_state;

    if state.editing() {
        let outcome = match state.focus() {
            SheetField::Name => {
                handle_single_line_edit(state.name_input_mut(), &event, SHEET_NAME_MAX_CHARS)
            }
            SheetField::Body => {
                handle_multiline_edit(state.body_input_mut(), &event, SHEET_BODY_MAX_CHARS)
            }
        };
        match (state.focus(), outcome) {
            (_, EditOutcome::Submit) => state.submit_edit(),
            (SheetField::Name, EditOutcome::Cancel) => state.cancel_edit(),
            // Bio convention: leaving the body edit always commits.
            (SheetField::Body, EditOutcome::Cancel) => state.submit_edit(),
            _ => {}
        }
        return;
    }

    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Byte(b'\t') | ParsedInput::BackTab => app.sheet_modal_state.toggle_focus(),
        // In an editable sheet, vertical arrows move focus; in a read-only
        // sheet they fall through to the scroll arms below.
        ParsedInput::Arrow(b'A') if app.sheet_modal_state.editable() => {
            app.sheet_modal_state.set_focus(SheetField::Name)
        }
        ParsedInput::Arrow(b'B') if app.sheet_modal_state.editable() => {
            app.sheet_modal_state.set_focus(SheetField::Body)
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'e') | ParsedInput::Char('e') => {
            app.sheet_modal_state.start_edit()
        }
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') | ParsedInput::Arrow(b'B') => {
            app.sheet_modal_state.scroll_body(1)
        }
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') | ParsedInput::Arrow(b'A') => {
            app.sheet_modal_state.scroll_body(-1)
        }
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.sheet_modal_state.scroll_body(-3),
            MouseEventKind::ScrollDown => app.sheet_modal_state.scroll_body(3),
            _ => {}
        },
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    handle_input(app, ParsedInput::Byte(0x1B));
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_sheet_modal = false;
    app.sheet_modal_state.close();
}
