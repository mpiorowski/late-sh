//! Keys while the reader is up: scroll, copy, act on the story, close.
//! Everything else is swallowed, so the reader reads like a modal and not
//! a transparency.

use crate::app::chat;
use crate::app::common::primitives::{Banner, Screen};
use crate::app::common::scroll_view::ScrollMouse;
use crate::app::input::{MouseEvent, ParsedInput, sanitize_paste_markers};
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
        ParsedInput::FocusLost => modal.view().cancel_drag(),
        ParsedInput::Byte(0x1B) => app.chat.close_reader(),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') | ParsedInput::Arrow(b'B') => {
            modal.view().scroll(1);
        }
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') | ParsedInput::Arrow(b'A') => {
            modal.view().scroll(-1);
        }
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') | ParsedInput::PageDown => modal.page(1),
        ParsedInput::Byte(b'b') | ParsedInput::Char('b') | ParsedInput::PageUp => modal.page(-1),
        ParsedInput::Byte(b'g') | ParsedInput::Char('g') | ParsedInput::Home => {
            modal.view().scroll_to_top();
        }
        ParsedInput::Byte(b'G') | ParsedInput::Char('G') | ParsedInput::End => {
            modal.view().scroll_to_bottom();
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
        // A video: hand it to the watch modal, the player or the setup box.
        // Each action here runs before the reader closes: the open reader
        // is how a story in neither the snapshot nor the shelf is found.
        ParsedInput::Byte(b'w') | ParsedInput::Char('w') if modal.watchable() => {
            let article_id = modal.article_id();
            chat::news::input::watch_article(app, article_id);
            app.chat.close_reader();
        }
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.chat.jump_to_reader_article();
            app.set_screen(Screen::Dashboard);
        }
        // Reply in #lounge quoting the story, as `r` in the News room does,
        // and take the user there to write it.
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') => {
            let article_id = modal.article_id();
            let replying = chat::news::input::reply_in_lounge(app, article_id);
            app.chat.close_reader();
            if replying {
                app.set_screen(Screen::Dashboard);
            }
        }
        ParsedInput::Byte(b'd') | ParsedInput::Char('d') => {
            let article_id = modal.article_id();
            app.chat.news.delete_article(article_id);
            app.chat.close_reader();
        }
        // Keep the story for later, or let it go, without leaving it.
        ParsedInput::Byte(b's') | ParsedInput::Char('s') => {
            let article_id = modal.article_id();
            if let Some(outcome) = app.chat.news.toggle_save(article_id) {
                app.banner = Some(chat::news::state::save_banner(outcome));
            }
        }
        _ => {}
    }
}

fn copy_link(app: &mut App, url: &str) {
    let cleaned = sanitize_paste_markers(url);
    app.pending_clipboard = Some(cleaned.trim().to_owned());
    app.banner = Some(Banner::success("Link copied!"));
}

/// Returns true only for the close control. A track click pages the way
/// `Space` and `b` do.
fn handle_mouse(modal: &ReaderModal, mouse: MouseEvent) -> bool {
    match modal.view().handle_mouse(mouse) {
        ScrollMouse::Done => false,
        ScrollMouse::Close => true,
        ScrollMouse::PageUp => {
            modal.page(-1);
            false
        }
        ScrollMouse::PageDown => {
            modal.page(1);
            false
        }
    }
}
