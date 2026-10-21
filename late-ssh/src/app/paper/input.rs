//! Keys while The Late Edition is up: scroll, leaf to the next printed
//! edition, close. Everything else is swallowed, so the paper reads like a
//! modal and not a transparency.

use late_core::models::paper::EditionStep;

use crate::app::common::scroll_view::ScrollMouse;
use crate::app::input::{MouseEvent, ParsedInput};
use crate::app::state::App;

use super::state::PaperModal;
use super::svc::browse;

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
                modal.view.cancel_drag();
            }
        }
        ParsedInput::Byte(0x1B | b'\r' | b'\n' | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.paper.close_modal();
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.view.scroll(1);
            }
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.view.scroll(-1);
            }
        }
        ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H')
        | ParsedInput::Arrow(b'D') => browse(app, EditionStep::Earlier),
        ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L')
        | ParsedInput::Arrow(b'C') => browse(app, EditionStep::Later),
        ParsedInput::PageDown => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.view.scroll(10);
            }
        }
        ParsedInput::PageUp => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.view.scroll(-10);
            }
        }
        ParsedInput::Home => {
            if let Some(modal) = app.paper.modal.as_mut() {
                modal.view.scroll_to_top();
            }
        }
        _ => {}
    }
}

/// Returns true only for the close control; the App owns request cancellation.
/// A track click pages by the body's full height.
fn handle_mouse(modal: &PaperModal, mouse: MouseEvent) -> bool {
    let page = i32::from(modal.view.viewport().body.height);
    match modal.view.handle_mouse(mouse) {
        ScrollMouse::Done => false,
        ScrollMouse::Close => true,
        ScrollMouse::PageUp => {
            modal.view.scroll(-page);
            false
        }
        ScrollMouse::PageDown => {
            modal.view.scroll(page);
            false
        }
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
