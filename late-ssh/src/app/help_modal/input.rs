use crate::app::{
    common::primitives::Banner,
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

use super::state::SearchDirection;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    // The `/` prompt takes every key while it is open, `?` and `q`
    // included: they are letters of the query, not the close keys.
    if app.help_modal_state.query().is_some() {
        handle_query_input(app, event);
        return;
    }

    match event {
        ParsedInput::Byte(0x09) => {
            app.help_modal_state.move_topic(1);
            return;
        }
        ParsedInput::BackTab => {
            app.help_modal_state.move_topic(-1);
            return;
        }
        ParsedInput::Mouse(mouse) => {
            handle_mouse(app, mouse);
            return;
        }
        _ => {}
    }

    if is_close_event(&event) {
        app.show_help = false;
        return;
    }

    let half = app.help_modal_state.half_page();
    match event {
        ParsedInput::Char('j') | ParsedInput::Char('J') | ParsedInput::Arrow(b'B') => {
            app.help_modal_state.scroll(1)
        }
        ParsedInput::Char('k') | ParsedInput::Char('K') | ParsedInput::Arrow(b'A') => {
            app.help_modal_state.scroll(-1)
        }
        ParsedInput::Byte(0x04) | ParsedInput::PageDown => app.help_modal_state.scroll(half),
        ParsedInput::Byte(0x15) | ParsedInput::PageUp => app.help_modal_state.scroll(-half),
        ParsedInput::Char('g') | ParsedInput::Home => app.help_modal_state.scroll_to_top(),
        ParsedInput::Char('G') | ParsedInput::End => app.help_modal_state.scroll_to_bottom(),
        ParsedInput::Char('/') => app.help_modal_state.begin_search(),
        ParsedInput::Char('n') => repeat_search(app, SearchDirection::Forward),
        ParsedInput::Char('N') => repeat_search(app, SearchDirection::Backward),
        _ => {}
    }
}

fn handle_query_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => {
            let listen_url = crate::app::state::listen_url(&app.web_url);
            if !app.help_modal_state.submit_search(&listen_url) {
                app.banner = Some(Banner::error("Nothing in the guide matches that"));
            }
        }
        ParsedInput::Byte(0x1B) => app.help_modal_state.cancel_search(),
        ParsedInput::Byte(0x7F | 0x08) => app.help_modal_state.pop_query(),
        ParsedInput::Char(ch) if !ch.is_control() => app.help_modal_state.push_query(ch),
        ParsedInput::Paste(bytes) => String::from_utf8_lossy(&bytes)
            .chars()
            .filter(|ch| !ch.is_control())
            .for_each(|ch| app.help_modal_state.push_query(ch)),
        _ => {}
    }
}

fn repeat_search(app: &mut App, direction: SearchDirection) {
    let listen_url = crate::app::state::listen_url(&app.web_url);
    if !app.help_modal_state.repeat_search(direction, &listen_url) {
        app.banner = Some(Banner::error("No search to repeat, or nothing matches"));
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return;
    };
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            if let Some(topic) = app.help_modal_state.topic_at_point(x, y) {
                // Double-click on a tab is treated as a plain switch: there's
                // no deeper verb here, and the scroll behavior already lives
                // on the wheel.
                let _ = app.help_modal_state.click_topic(topic);
            }
        }
        MouseEventKind::ScrollUp if app.help_modal_state.body_contains(x, y) => {
            app.help_modal_state.scroll(-3);
        }
        MouseEventKind::ScrollDown if app.help_modal_state.body_contains(x, y) => {
            app.help_modal_state.scroll(3);
        }
        _ => {}
    }
}

/// A bare Esc: it closes the `/` prompt first, the guide only when no
/// prompt is open.
pub(crate) fn handle_escape(app: &mut App) {
    match app.help_modal_state.query() {
        Some(_) => app.help_modal_state.cancel_search(),
        None => app.show_help = false,
    }
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'?' | b'q' | b'Q') | ParsedInput::Char('?' | 'q' | 'Q')
    )
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
