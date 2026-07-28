use super::state::{Field, Mode};
use crate::app::common::textarea_input::{
    EditOutcome, handle_multiline_edit, handle_single_line_edit,
};
use crate::app::input::ParsedInput;
use crate::app::state::App;

/// Route a key event to the open room-info form. Tab moves between the two
/// fields; everything else goes to the focused field through the shared
/// textarea helpers. Up/down only move fields from the one-line topic, since in
/// the rules block they move the cursor.
pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if matches!(event, ParsedInput::Byte(b'\t') | ParsedInput::BackTab) {
        app.room_info_modal_state.toggle_focus();
        return;
    }

    // A click on a field focuses it; other mouse events are swallowed so they
    // don't leak to the app behind the modal.
    if let ParsedInput::Mouse(mouse) = event {
        use crate::app::input::{MouseButton, MouseEventKind};
        if mouse.kind == MouseEventKind::Down
            && mouse.button == Some(MouseButton::Left)
            && let Some(field) = app.room_info_modal_state.field_at(mouse.x, mouse.y)
        {
            app.room_info_modal_state.set_focus(field);
        }
        return;
    }

    let focus = app.room_info_modal_state.focus();
    let max = focus.max_len();
    let field = app.room_info_modal_state.field_mut(focus);
    let outcome = match focus {
        Field::Topic => handle_single_line_edit(field, &event, max),
        Field::Rules => handle_multiline_edit(field, &event, max),
    };
    match outcome {
        EditOutcome::Handled => {}
        EditOutcome::Submit => submit(app),
        EditOutcome::Cancel => app.room_info_modal_state.close(),
        EditOutcome::Ignored => {
            if matches!(event, ParsedInput::Arrow(b'A') | ParsedInput::Arrow(b'B')) {
                app.room_info_modal_state.toggle_focus();
            }
        }
    }
}

/// Close the form on Esc from the app-level escape dispatch.
pub(crate) fn handle_escape(app: &mut App) {
    app.room_info_modal_state.close();
}

/// Dispatch the form. Both fields are optional: submitting an empty form on
/// create still opens the room, it just has no info yet.
fn submit(app: &mut App) {
    let Some(mode) = app.room_info_modal_state.mode().cloned() else {
        app.room_info_modal_state.close();
        return;
    };
    let (topic, rules) = app.room_info_modal_state.values();
    let opt = |s: String| (!s.is_empty()).then_some(s);
    let user_id = app.user_id;
    match mode {
        Mode::Create { slug } => {
            app.chat.service.create_private_room_with_info_task(
                user_id,
                slug,
                opt(topic),
                opt(rules),
            );
        }
        Mode::Edit { room_id } => {
            let is_mod = app.permissions.can_moderate();
            app.chat
                .service
                .set_room_info_task(user_id, is_mod, room_id, opt(topic), opt(rules));
        }
    }
    app.room_info_modal_state.close();
}
