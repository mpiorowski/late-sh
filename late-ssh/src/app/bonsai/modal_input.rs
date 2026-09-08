use crate::app::{
    bonsai::{
        state::DailyWaterGate,
        svc::{BonsaiService, WATER_CHIP_BONUS},
    },
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
            app.bonsai_state.prune_selected();
        }
        ParsedInput::Byte(b'p' | b'P') | ParsedInput::Char('p' | 'P') => {
            app.bonsai_state.pinch_selected();
        }
        ParsedInput::Byte(b's' | b'S') | ParsedInput::Char('s' | 'S') => {
            app.bonsai_state.split_selected();
        }
        ParsedInput::Byte(b'c' | b'C') | ParsedInput::Char('c' | 'C') => copy_snippet(app),
        ParsedInput::Byte(b'\t') => app.bonsai_state.cycle_selection(1),
        ParsedInput::BackTab => app.bonsai_state.cycle_selection(-1),
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.bonsai_state.cycle_selection(1);
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
            MouseEventKind::ScrollUp => app.bonsai_state.cycle_selection(-1),
            MouseEventKind::ScrollDown => app.bonsai_state.cycle_selection(1),
            _ => {}
        },
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

fn steer(app: &mut App, dx: i8, dy: i8) {
    app.bonsai_state.bend_selected(dx, dy);
}

fn water(app: &mut App) {
    // The first `w` on a dead tree replants; watering starts on the next.
    if !app.bonsai_state.is_alive {
        app.bonsai_state.respawn();
        return;
    }

    // The chips are paid by the service behind the DB's once-per-day gate
    // (`Tree::water_day`); this only decides what the status row says.
    let earns_chips = app.bonsai_state.last_watered != Some(BonsaiService::today());
    let changed = app.bonsai_state.water(daily_water_gate(app));
    if changed && earns_chips {
        app.bonsai_state.message = Some(format!("Watered (+{WATER_CHIP_BONUS} chips)"));
    }
}

/// Admins skip the once-per-day rule for now, so growth can be tested
/// without waiting for tomorrow. See `DailyWaterGate`.
pub(crate) fn daily_water_gate(app: &App) -> DailyWaterGate {
    if app.is_admin {
        DailyWaterGate::AdminBypass
    } else {
        DailyWaterGate::Enforced
    }
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
    app.pending_clipboard = Some(app.bonsai_state.share_snippet());
    app.banner = Some(crate::app::common::primitives::Banner::success(
        "Bonsai copied to clipboard!",
    ));
}
