//! Keys while the reader is up: scroll, copy, act on the story, close.
//! Everything else is swallowed, so the reader reads like a modal and not
//! a transparency.

use ratatui::layout::Position;

use crate::app::chat;
use crate::app::common::primitives::{Banner, Screen};
use crate::app::input::{
    MouseButton, MouseEvent, MouseEventKind, ParsedInput, sanitize_paste_markers,
};
use crate::app::state::App;

use super::state::ReaderModal;

pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    let Some(modal) = app.chat.reader() else {
        return;
    };
    match event {
        ParsedInput::Mouse(mouse) => {
            if app.interaction_mode.mouse_enabled() && handle_mouse(modal, *mouse) {
                app.chat.close_reader();
            }
        }
        ParsedInput::FocusLost => modal.cancel_drag(),
        ParsedInput::Byte(0x1B) => app.chat.close_reader(),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') | ParsedInput::Arrow(b'B') => {
            modal.scroll(1);
        }
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') | ParsedInput::Arrow(b'A') => {
            modal.scroll(-1);
        }
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') | ParsedInput::PageDown => modal.page(1),
        ParsedInput::Byte(b'b') | ParsedInput::Char('b') | ParsedInput::PageUp => modal.page(-1),
        ParsedInput::Byte(b'g') | ParsedInput::Char('g') | ParsedInput::Home => {
            modal.scroll_to_top();
        }
        ParsedInput::Byte(b'G') | ParsedInput::Char('G') | ParsedInput::End => {
            modal.scroll_to_bottom();
        }
        ParsedInput::Byte(digit @ b'1'..=b'9') => {
            let n = usize::from(digit - b'0');
            if let Some(url) = modal.link(n).map(str::to_owned) {
                copy_link(app, &url);
            }
        }
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => {
            let url = modal.url().to_owned();
            copy_link(app, &url);
        }
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.chat.jump_to_reader_article();
            app.set_screen(Screen::Dashboard);
        }
        // Reply in #lounge quoting the story, as `r` on the live strip or
        // in the News room does, and take the user there to write it.
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') => {
            let article_id = modal.article_id();
            app.chat.close_reader();
            if chat::news::input::reply_in_lounge(app, article_id) {
                app.set_screen(Screen::Dashboard);
            }
        }
        ParsedInput::Byte(b'd') | ParsedInput::Char('d') => {
            let article_id = modal.article_id();
            app.chat.close_reader();
            app.chat.news.delete_article(article_id);
        }
        _ => {}
    }
}

fn copy_link(app: &mut App, url: &str) {
    let cleaned = sanitize_paste_markers(url);
    app.pending_clipboard = Some(cleaned.trim().to_owned());
    app.banner = Some(Banner::success("Link copied!"));
}

/// Returns true only for the close control.
fn handle_mouse(modal: &ReaderModal, mouse: MouseEvent) -> bool {
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
                modal.page(if y < thumb.y { -1 } else { 1 });
            }
        }
        MouseEventKind::Drag if mouse.button == Some(MouseButton::Left) => modal.drag_to(y),
        MouseEventKind::ScrollUp if viewport.popup.contains(point) => modal.scroll(-3),
        MouseEventKind::ScrollDown if viewport.popup.contains(point) => modal.scroll(3),
        _ => {}
    }
    false
}
