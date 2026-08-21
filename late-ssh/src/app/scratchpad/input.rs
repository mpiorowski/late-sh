use crate::app::common::primitives::Screen;
use crate::app::common::textarea_input::{EditOutcome, handle_freeform_edit};
use crate::app::input::ParsedInput;
use crate::app::state::App;

/// Full-screen dispatch for the paired editor: forward everything to the
/// shared `TextArea`, same shape as the daily board / house table in
/// `handle_dedicated_screen_input`.
///
/// There is no accept/decline prompt to handle here. Pairing is a mutual
/// `/pair @user` handshake (see `pair.rs`), so a session only ever reaches
/// this screen because its own user asked to.
///
/// The `Cancel` arm below is not the main Esc path. A lone Esc is held by the
/// parser and resolved into `dispatch_escape`, which has its own
/// `Screen::Scratchpad` arm; this one only fires when Esc arrives mid-chunk
/// with other bytes. Both are needed.
pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // Shift+Tab would otherwise fall through to the global page cycle, which
    // changes screen and so silently ends the pairing. There is nothing to
    // cycle on this screen, so swallow it without publishing.
    if matches!(event, ParsedInput::BackTab) {
        return true;
    }
    let Some(state) = app.scratchpad.as_mut() else {
        return false;
    };
    // Mouse: click positions the caret; the wheel pages through the buffer.
    if let ParsedInput::Mouse(mouse) = event {
        use crate::app::input::{MouseButton, MouseEventKind};
        let clicked = mouse.kind == MouseEventKind::Down
            && mouse.button == Some(MouseButton::Left)
            && state.click_to_cursor(mouse.x, mouse.y);
        if clicked {
            state.publish();
        } else if mouse.kind == MouseEventKind::ScrollUp {
            state.scroll_lines(true);
        } else if mouse.kind == MouseEventKind::ScrollDown {
            state.scroll_lines(false);
        }
        return true;
    }
    // Ctrl+L cycles the shared highlighting language. Everywhere else Ctrl+L
    // is the global force-repaint chord; this screen is carved out of it by
    // name in `handle_reserved_global_chord`, so the two never collide. Moving
    // this binding means dropping that carve-out too.
    if matches!(event, ParsedInput::Byte(0x0C)) {
        state.cycle_language();
        return true;
    }
    let max_chars = state.max_chars();
    match handle_freeform_edit(&mut state.editor, event, max_chars) {
        EditOutcome::Handled => {
            state.publish();
            true
        }
        EditOutcome::Cancel => {
            app.set_screen(Screen::Dashboard);
            true
        }
        EditOutcome::Submit => true,
        EditOutcome::Ignored => false,
    }
}
