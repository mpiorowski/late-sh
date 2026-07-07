use crate::app::{
    hub::state::HubTab,
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

pub fn handle_input(app: &mut App, event: ParsedInput) {
    app.hub_state.ensure_visible_tab(app.is_admin);
    if app.hub_state.selected_tab() == HubTab::Admin
        && crate::app::hub::admin::input::handle_input(app, &event)
    {
        return;
    }

    if app.hub_state.selected_tab() == HubTab::Shop
        && crate::app::hub::shop::input::handle_input(app, &event)
    {
        return;
    }

    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            handle_escape(app)
        }
        ParsedInput::Byte(b'\t') => app.hub_state.select_next_tab(app.is_admin),
        ParsedInput::BackTab => app.hub_state.select_previous_tab(app.is_admin),
        ParsedInput::Arrow(b'C') => app.hub_state.select_next_tab(app.is_admin),
        ParsedInput::Arrow(b'D') => app.hub_state.select_previous_tab(app.is_admin),
        ParsedInput::Char('1') | ParsedInput::Byte(b'1') => {
            app.hub_state.open(HubTab::Shop);
        }
        ParsedInput::Char('2') | ParsedInput::Byte(b'2') => {
            app.hub_state.open(HubTab::Leaderboard);
        }
        ParsedInput::Char('3') | ParsedInput::Byte(b'3') => {
            app.hub_state.open(HubTab::Dailies);
        }
        ParsedInput::Char('4') | ParsedInput::Byte(b'4') => {
            app.hub_state.open(HubTab::Events);
        }
        ParsedInput::Char('5') | ParsedInput::Byte(b'5') if app.is_admin => {
            app.hub_state.open(HubTab::Admin);
        }
        ParsedInput::Mouse(mouse) => handle_mouse(app, mouse),
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    // SGR mouse reports are 1-based; the rect cache stores 0-based ratatui
    // coords, matching the convention used in `settings_modal::input` and
    // `icon_picker::picker`.
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return;
    };
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            if let Some(tab) = app.hub_state.tab_at_point(x, y) {
                // Double-clicking a tab is treated the same as a single click.
                // The keyboard nav contract doesn't define a deeper "activate"
                // verb on hub tabs, and we don't want a stray double tap to
                // surprise newcomers.
                let _ = app.hub_state.click_tab(tab);
            }
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    app.show_hub_modal = false;
}
