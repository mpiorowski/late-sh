use ratatui::layout::Rect;

use crate::app::{
    input::{MouseButton, MouseEventKind, ParsedInput},
    state::App,
};

use super::ui::OPTIONS;

/// Handle input while the first-run interaction-mode prompt is up. Any pick
/// applies the mode (flipping the mouse live), persists it, and dismisses the
/// prompt for good.
pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'1') | ParsedInput::Char('1') => choose(app, 0),
        ParsedInput::Byte(b'2') | ParsedInput::Char('2') => choose(app, 1),
        ParsedInput::Byte(b'3') | ParsedInput::Char('3') => choose(app, 2),
        ParsedInput::Arrow(b'A') => {
            app.onboarding_selected = app.onboarding_selected.saturating_sub(1);
        }
        ParsedInput::Arrow(b'B') => {
            app.onboarding_selected = (app.onboarding_selected + 1).min(OPTIONS.len() - 1);
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') | ParsedInput::Byte(b' ') => {
            choose(app, app.onboarding_selected);
        }
        ParsedInput::Mouse(mouse)
            if mouse.kind == MouseEventKind::Down && mouse.button == Some(MouseButton::Left) =>
        {
            if let Some(i) = option_at(app, mouse.x, mouse.y) {
                choose(app, i);
            }
        }
        _ => {}
    }
}

fn choose(app: &mut App, index: usize) {
    let Some((mode, _, _)) = OPTIONS.get(index).copied() else {
        return;
    };
    // Applies live (flips the mouse if needed) and persists on change; also
    // persist unconditionally so choosing the default still records a value and
    // the prompt never returns.
    app.set_interaction_mode(mode);
    app.profile_state
        .service()
        .set_interaction_mode(app.user_id, mode);
    app.needs_interaction_onboarding = false;
}

fn option_at(app: &App, x: u16, y: u16) -> Option<usize> {
    let rects = app.onboarding_option_rects.get();
    rects.iter().position(|rect| {
        rect.is_some_and(|r: Rect| {
            x >= r.x
                && x < r.x.saturating_add(r.width)
                && y >= r.y
                && y < r.y.saturating_add(r.height)
        })
    })
}
