use crate::app::{
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Byte(b'\t') => app.profile_modal_state.cycle_tab(1),
        ParsedInput::BackTab => app.profile_modal_state.cycle_tab(-1),
        ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L')
        | ParsedInput::Arrow(b'C') => {
            app.profile_modal_state.cycle_tab(1);
        }
        ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H')
        | ParsedInput::Arrow(b'D') => {
            app.profile_modal_state.cycle_tab(-1);
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            app.profile_modal_state.scroll_by(1);
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            app.profile_modal_state.scroll_by(-1);
        }
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.profile_modal_state.scroll_by(-3),
            MouseEventKind::ScrollDown => app.profile_modal_state.scroll_by(3),
            // A left click outside the modal dismisses it, like clicking off a
            // popup elsewhere; clicks on the modal itself are left alone.
            MouseEventKind::Down if clicked_outside(app, &mouse) => close(app),
            _ => {}
        },
        ParsedInput::PageDown => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(step);
        }
        ParsedInput::PageUp => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(-step);
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    close(app);
}

/// True for a left-button press that lands outside the modal's popup rect
/// (from the last render). SGR mouse cells are 1-indexed; the popup rect is
/// in 0-indexed frame cells, so shift the click by one before testing.
fn clicked_outside(app: &App, mouse: &MouseEvent) -> bool {
    if mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let popup = app.profile_modal_state.popup_area();
    // A zero-size rect means nothing was drawn yet: don't dismiss on it.
    if popup.width == 0 || popup.height == 0 {
        return false;
    }
    !(x >= popup.x
        && x < popup.x.saturating_add(popup.width)
        && y >= popup.y
        && y < popup.y.saturating_add(popup.height))
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(b'q' | b'Q' | 0x1B) | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_profile_modal = false;
    app.profile_modal_state.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_keys_include_printable_q_variants() {
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Byte(b'q')));
        assert!(is_close_event(&ParsedInput::Byte(b'Q')));
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(!is_close_event(&ParsedInput::Char('j')));
    }
}
