use crate::app::{
    bonsai::state::BonsaiAction,
    input::{MouseEventKind, ParsedInput},
    state::App,
};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'w' | b'W') | ParsedInput::Char('w' | 'W') => water(app),
        ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => {
            app.bonsai.request(BonsaiAction::Prune);
        }
        ParsedInput::Byte(b'p' | b'P') | ParsedInput::Char('p' | 'P') => {
            app.bonsai.request(BonsaiAction::Pinch);
        }
        ParsedInput::Byte(b's' | b'S') | ParsedInput::Char('s' | 'S') => {
            app.bonsai.request(BonsaiAction::Split);
        }
        ParsedInput::Byte(b'c' | b'C') | ParsedInput::Char('c' | 'C') => copy_snippet(app),
        ParsedInput::Byte(b'\t') => app.bonsai.tree.cycle_selection(1),
        ParsedInput::BackTab => app.bonsai.tree.cycle_selection(-1),
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.bonsai.tree.cycle_selection(1);
        }
        ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H')
        | ParsedInput::Arrow(b'D') => steer(app, -1, 0),
        ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L')
        | ParsedInput::Arrow(b'C') => steer(app, 1, 0),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => steer(app, 0, 1),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => steer(app, 0, -1),
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.bonsai.tree.cycle_selection(-1),
            MouseEventKind::ScrollDown => app.bonsai.tree.cycle_selection(1),
            _ => {}
        },
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

fn steer(app: &mut App, dx: i8, dy: i8) {
    app.bonsai.request(BonsaiAction::Bend { dx, dy });
}

fn water(app: &mut App) {
    // Waters, or replants a dead tree. The service decides which, and
    // whether today's chips are still unclaimed: another session may
    // already have watered. The status row says what happened when the
    // answer comes back.
    app.bonsai.request(BonsaiAction::Water);
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_bonsai_modal = false;
}

fn open_help(app: &mut App) {
    app.help_modal_state
        .open(crate::app::help_modal::data::HelpTopic::Bonsai);
    app.show_help = true;
}

fn copy_snippet(app: &mut App) {
    app.pending_clipboard = Some(app.bonsai.tree.share_snippet());
    app.banner = Some(crate::app::common::primitives::Banner::success(
        "Bonsai copied to clipboard!",
    ));
}
