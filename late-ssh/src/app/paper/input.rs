//! Keys while The Late Edition is up: scroll, close. Everything else is
//! swallowed, so the paper reads like a modal and not a transparency.

use ratatui::layout::Position;

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};
use crate::app::state::App;

use super::state::PaperModal;

pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    match event {
        ParsedInput::Mouse(mouse) => {
            if app.interaction_mode.mouse_enabled()
                && app
                    .paper
                    .modal
                    .as_ref()
                    .is_some_and(|modal| handle_mouse(modal, *mouse))
            {
                app.paper.close_modal();
            }
        }
        ParsedInput::FocusLost => {
            if let Some(modal) = &app.paper.modal {
                modal.cancel_drag();
            }
        }
        ParsedInput::Byte(0x1B | b'\r' | b'\n' | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.paper.close_modal();
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(1);
            }
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(-1);
            }
        }
        ParsedInput::PageDown => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(10);
            }
        }
        ParsedInput::PageUp => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll(-10);
            }
        }
        ParsedInput::Home => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.scroll_to_top();
            }
        }
        _ => {}
    }
}

/// Returns true only for the close control; the App owns request cancellation.
fn handle_mouse(modal: &PaperModal, mouse: MouseEvent) -> bool {
    if mouse.kind == MouseEventKind::Up {
        modal.cancel_drag();
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let point = Position::new(x, y);
    let viewport = modal.viewport();
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            modal.cancel_drag();
            if viewport.close.contains(point) {
                return true;
            }
            let thumb = modal.thumb();
            if thumb.contains(point) {
                modal.begin_drag(y);
            } else if viewport.track.contains(point) {
                let page = viewport.body.height.min(i16::MAX as u16) as i16;
                modal.scroll(if y < thumb.y { -page } else { page });
            }
        }
        MouseEventKind::Drag if mouse.button == Some(MouseButton::Left) => modal.drag_to(y),
        MouseEventKind::ScrollUp if viewport.popup.contains(point) => modal.scroll(-3),
        MouseEventKind::ScrollDown if viewport.popup.contains(point) => modal.scroll(3),
        _ => {}
    }
    false
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
