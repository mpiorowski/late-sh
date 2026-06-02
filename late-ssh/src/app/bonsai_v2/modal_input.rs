use crate::app::{
    bonsai::svc::{BonsaiService, WATER_CHIP_BONUS},
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
            app.bonsai_v2_state.prune_selected();
        }
        ParsedInput::Byte(b'p' | b'P') | ParsedInput::Char('p' | 'P') => {
            app.bonsai_v2_state.pinch_selected();
        }
        ParsedInput::Byte(b's' | b'S') | ParsedInput::Char('s' | 'S') => {
            app.bonsai_v2_state.split_selected();
        }
        ParsedInput::Byte(b'c' | b'C') | ParsedInput::Char('c' | 'C') => copy_snippet(app),
        ParsedInput::Byte(b'\t') => app.bonsai_v2_state.cycle_selection(1),
        ParsedInput::BackTab => app.bonsai_v2_state.cycle_selection(-1),
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.bonsai_v2_state.cycle_selection(1);
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
            MouseEventKind::ScrollUp => app.bonsai_v2_state.cycle_selection(-1),
            MouseEventKind::ScrollDown => app.bonsai_v2_state.cycle_selection(1),
            _ => {}
        },
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

fn steer(app: &mut App, dx: i8, dy: i8) {
    app.bonsai_v2_state.bend_selected(dx, dy);
}

fn water(app: &mut App) {
    let was_dead = !app.bonsai_state.is_alive || !app.bonsai_v2_state.is_alive;
    if !app.bonsai_state.is_alive {
        app.bonsai_state.respawn();
        app.bonsai_care_state
            .reset_for_respawn(app.bonsai_state.seed);
        app.bonsai_state.reset_daily_care_for_respawn(
            app.bonsai_care_state.date,
            app.bonsai_care_state.branch_goal as i32,
        );
    }
    if !app.bonsai_v2_state.is_alive {
        app.bonsai_v2_state.respawn();
    }
    if was_dead {
        app.bonsai_v2_state.message = Some("New dynamic bonsai planted".to_string());
        return;
    }

    let today = BonsaiService::today();
    let v2_water_day = if app.is_admin && app.bonsai_v2_state.last_simulated_date > today {
        app.bonsai_v2_state.last_simulated_date
    } else {
        today
    };
    let repeat_v2_water = app.is_admin && app.bonsai_v2_state.last_watered == Some(v2_water_day);
    let earns_chips = app.bonsai_state.last_watered != Some(today);
    let legacy_gain = app.bonsai_state.water();
    let changed = if app.is_admin {
        app.bonsai_v2_state.admin_water()
    } else {
        app.bonsai_v2_state.water()
    };
    let chip_bonus = if earns_chips {
        format!(", +{WATER_CHIP_BONUS} chips")
    } else {
        String::new()
    };
    let growth_text = legacy_gain
        .map(|gained| {
            if gained > 0 {
                format!("legacy +{gained}")
            } else {
                "legacy maxed".to_string()
            }
        })
        .unwrap_or_else(|| "legacy already watered".to_string());

    if changed {
        let label = if repeat_v2_water {
            "Admin watered again"
        } else {
            "Watered Dynamic Bonsai"
        };
        app.bonsai_v2_state.message = Some(format!("{label} ({growth_text}{chip_bonus})"));
    }
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_bonsai_v2_modal = false;
}

fn open_help(app: &mut App) {
    app.help_modal_state
        .open(crate::app::help_modal::data::HelpTopic::Bonsai);
    app.show_help = true;
}

fn copy_snippet(app: &mut App) {
    app.pending_clipboard = Some(app.bonsai_v2_state.share_snippet());
    app.banner = Some(crate::app::common::primitives::Banner::success(
        "Dynamic Bonsai copied to clipboard!",
    ));
}
