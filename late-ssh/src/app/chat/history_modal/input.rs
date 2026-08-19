use late_core::models::chat_message::HistoryDirection;

use crate::app::{input::ParsedInput, state::App};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => {
            app.chat.history_modal.close();
            return;
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => app.chat.history_modal.scroll(-1),
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => app.chat.history_modal.scroll(1),
        ParsedInput::PageUp => app.chat.history_modal.scroll_page(-1),
        ParsedInput::PageDown => app.chat.history_modal.scroll_page(1),
        ParsedInput::Home => app.chat.history_modal.scroll(i32::MIN / 2),
        ParsedInput::End => app.chat.history_modal.scroll_to_bottom(),
        ParsedInput::Mouse(mouse) => {
            use crate::app::input::MouseEventKind;
            match mouse.kind {
                MouseEventKind::ScrollUp => app.chat.history_modal.scroll(-3),
                MouseEventKind::ScrollDown => app.chat.history_modal.scroll(3),
                _ => {}
            }
        }
        _ => {}
    }

    // Any scroll can land on an edge, so both are checked once here rather
    // than from each arm.
    app.chat
        .request_history_page_if_needed(HistoryDirection::Older);
    app.chat
        .request_history_page_if_needed(HistoryDirection::Newer);
}
