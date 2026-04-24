use crate::app::{
    bonsai::care::{CareMode, branch_targets_for},
    input::{MouseEventKind, ParsedInput},
    state::App,
};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Byte(b'w' | b'W') | ParsedInput::Char('w' | 'W') => water(app),
        ParsedInput::Byte(b'p' | b'P') | ParsedInput::Char('p' | 'P') => prune_tree(app),
        ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => cut_branch(app),
        ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H')
        | ParsedInput::Arrow(b'D') => {
            move_cursor(app, -1, 0);
        }
        ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L')
        | ParsedInput::Arrow(b'C') => {
            move_cursor(app, 1, 0);
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            move_cursor(app, 0, -1);
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            move_cursor(app, 0, 1);
        }
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => move_cursor(app, 0, -1),
            MouseEventKind::ScrollDown => move_cursor(app, 0, 1),
            _ => {}
        },
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

fn water(app: &mut App) {
    if !app.bonsai_state.is_alive {
        app.bonsai_state.respawn();
        app.bonsai_care_state.message = Some("New seed planted".to_string());
        return;
    }
    let gained = app.bonsai_state.water();
    if gained > 0 {
        app.bonsai_care_state.mark_watered();
        app.bonsai_care_state.message = Some(format!("Watered: +{gained} points"));
    } else {
        app.bonsai_care_state.watered = true;
        app.bonsai_care_state.message = Some("Already watered today".to_string());
    }
}

fn prune_tree(app: &mut App) {
    if app.bonsai_state.cut() {
        app.bonsai_care_state.reset_branch_cuts();
        app.bonsai_state.reset_daily_branches();
        app.bonsai_care_state.message = Some("Pruned: -1 stage, new shape".to_string());
    } else if !app.bonsai_state.is_alive {
        app.bonsai_care_state.message = Some("Can't prune a dead tree".to_string());
    } else {
        app.bonsai_care_state.message = Some("Need 100 growth to prune".to_string());
    }
}

fn cut_branch(app: &mut App) {
    enter_prune_mode(app);
    let targets = current_targets(app);
    let Some(branch_id) = app.bonsai_care_state.cut_at_cursor(&targets) else {
        let loss = app.bonsai_state.punish_wrong_cut();
        if loss > 0 {
            app.bonsai_care_state.message = Some(format!("Wrong cut: -{loss} points"));
        } else {
            app.bonsai_care_state.message = Some("Wrong cut".to_string());
        }
        return;
    };
    app.bonsai_state.cut_daily_branch(branch_id);
    if app.bonsai_care_state.all_branches_cut() {
        app.bonsai_care_state.message = Some("Tree preserved".to_string());
    }
}

fn move_cursor(app: &mut App, dx: isize, dy: isize) {
    enter_prune_mode(app);
    let (width, height) = current_art_size(app);
    app.bonsai_care_state.move_cursor(dx, dy, width, height);
}

fn enter_prune_mode(app: &mut App) {
    if app.bonsai_care_state.mode == CareMode::Prune {
        return;
    }
    app.bonsai_care_state.mode = CareMode::Prune;
    let (width, height) = current_art_size(app);
    app.bonsai_care_state
        .set_cursor(width.saturating_sub(1) / 2, height.saturating_sub(1));
}

fn current_targets(app: &App) -> Vec<crate::app::bonsai::care::BranchTarget> {
    let stage = app.bonsai_state.stage();
    let art = crate::app::bonsai::ui::tree_ascii(stage, app.bonsai_state.seed, false);
    branch_targets_for(
        stage,
        app.bonsai_state.seed,
        app.bonsai_care_state.date,
        &art,
        app.bonsai_care_state.branch_goal,
    )
}

fn current_art_size(app: &App) -> (usize, usize) {
    let stage = app.bonsai_state.stage();
    let art = crate::app::bonsai::ui::tree_ascii(stage, app.bonsai_state.seed, false);
    let width = art
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    (width, art.len())
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
