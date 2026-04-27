use crate::app::input::{MouseEventKind, ParsedInput, sanitize_paste_markers};
use crate::app::state::App;

use super::state::{PickerKind, Row, Tab};
use crate::app::settings_modal::state::SettingsModalState;

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if app.settings_modal_state.picker_open() {
        handle_picker_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_username() {
        handle_username_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_bio() {
        handle_bio_input(app, event);
        return;
    }

    // Tab / Shift+Tab switch top-level tabs. Do this before close-event
    // routing so Tab doesn't get eaten as "close".
    match event {
        ParsedInput::Byte(0x09) => {
            app.settings_modal_state.cycle_tab(true);
            return;
        }
        ParsedInput::BackTab => {
            app.settings_modal_state.cycle_tab(false);
            return;
        }
        _ => {}
    }

    if is_close_event(&event) {
        app.show_settings = false;
        return;
    }

    if app.settings_modal_state.selected_tab() == Tab::Bio {
        handle_bio_tab_input(app, event);
        return;
    }

    if app.settings_modal_state.selected_tab() == Tab::Themes {
        handle_themes_tab_input(app, event);
        return;
    }

    if app.settings_modal_state.selected_tab() == Tab::Favorites {
        handle_favorites_tab_input(app, event);
        return;
    }

    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.settings_modal_state.move_row(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.settings_modal_state.move_row(-1),
        ParsedInput::Arrow(b'C') => app.settings_modal_state.cycle_setting(true),
        ParsedInput::Arrow(b'D') => app.settings_modal_state.cycle_setting(false),
        ParsedInput::Byte(b' ') | ParsedInput::Byte(b'\r') => activate_selected_row(app),
        ParsedInput::Char('e') | ParsedInput::Char('E') => activate_selected_row(app),
        _ => {}
    }
}

fn handle_themes_tab_input(app: &mut App, event: ParsedInput) {
    let state: &mut SettingsModalState = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => state.move_theme_cursor(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => state.move_theme_cursor(-1),
        ParsedInput::Arrow(b'D') => state.theme_cursor_left(),
        ParsedInput::Arrow(b'C') => state.theme_cursor_right(),
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') => state.toggle_theme_tree_row(),
        _ => {}
    }
}

/// Favorites tab:
/// - j/k / ↑↓ move the cursor across favorites + the "Add…" row
/// - J/K (shift) or Alt+↑↓ reorder the selected favorite (no-op on "Add…")
/// - d / Backspace / Delete remove the selected favorite
/// - Enter opens the room picker when on "Add…", no-op on a favorite row
fn handle_favorites_tab_input(app: &mut App, event: ParsedInput) {
    let state: &mut SettingsModalState = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') | ParsedInput::Arrow(b'B') => {
            state.move_favorites_cursor(1)
        }
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') | ParsedInput::Arrow(b'A') => {
            state.move_favorites_cursor(-1)
        }
        ParsedInput::Byte(b'J') | ParsedInput::Char('J') | ParsedInput::AltArrow(b'B') => {
            state.reorder_selected_favorite(1)
        }
        ParsedInput::Byte(b'K') | ParsedInput::Char('K') | ParsedInput::AltArrow(b'A') => {
            state.reorder_selected_favorite(-1)
        }
        ParsedInput::Byte(b'd')
        | ParsedInput::Char('d')
        | ParsedInput::Byte(0x7F)
        | ParsedInput::Delete => state.remove_selected_favorite(),
        ParsedInput::Byte(b'\r') | ParsedInput::Char('a') | ParsedInput::Char('A')
            if state.favorites_index_is_add_row() && !state.filtered_rooms().is_empty() =>
        {
            state.open_picker(PickerKind::Room);
        }
        _ => {}
    }
}

/// Bio tab (not editing): Enter begins editing. Everything else ignored —
/// close and tab-switch events were already handled above.
fn handle_bio_tab_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Char('e') | ParsedInput::Char('E') => {
            app.settings_modal_state.start_bio_edit();
        }
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        _ => {}
    }
}

fn open_help(app: &mut App) {
    app.help_modal_state
        .open(crate::app::help_modal::data::HelpTopic::Overview);
    app.show_help = true;
}

pub fn handle_escape(app: &mut App) {
    handle_input(app, ParsedInput::Byte(0x1B));
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q')
    )
}

fn activate_selected_row(app: &mut App) {
    match app.settings_modal_state.selected_row() {
        Row::Username => app.settings_modal_state.start_username_edit(),
        Row::Theme
        | Row::BackgroundColor
        | Row::DashboardHeader
        | Row::RightSidebar
        | Row::GamesSidebar
        | Row::DirectMessages
        | Row::Mentions
        | Row::GameEvents
        | Row::Bell
        | Row::Cooldown
        | Row::NotifyFormat => app.settings_modal_state.cycle_setting(true),
        Row::Country => app.settings_modal_state.open_picker(PickerKind::Country),
        Row::Timezone => app.settings_modal_state.open_picker(PickerKind::Timezone),
    }
}

fn handle_username_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(0x1B) => state.cancel_username_edit(),
        ParsedInput::Byte(b'\r') => state.submit_username(),
        ParsedInput::Byte(0x15) => state.clear_username(),
        ParsedInput::Byte(0x01) => state.username_cursor_home(),
        ParsedInput::Byte(0x05) => state.username_cursor_end(),
        ParsedInput::Byte(0x19) => state.username_paste(),
        ParsedInput::Byte(0x1F) => state.username_undo(),
        ParsedInput::Byte(0x7F) => state.username_backspace(),
        ParsedInput::Delete => state.username_delete_right(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => state.username_delete_word_left(),
        ParsedInput::CtrlDelete => state.username_delete_word_right(),
        ParsedInput::Arrow(b'C') => state.username_cursor_right(),
        ParsedInput::Arrow(b'D') => state.username_cursor_left(),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            state.username_cursor_word_right()
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            state.username_cursor_word_left()
        }
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            for ch in cleaned.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    state.username_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => state.username_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            state.username_push(byte as char)
        }
        _ => {}
    }
}

fn handle_bio_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(0x1B) => state.stop_bio_edit(),
        ParsedInput::Byte(b'\r') => state.stop_bio_edit(),
        ParsedInput::AltEnter | ParsedInput::Byte(b'\n') => state.bio_push('\n'),
        ParsedInput::Byte(0x15) => state.bio_clear(),
        ParsedInput::Byte(0x19) => state.bio_paste(),
        ParsedInput::Byte(0x1F) => state.bio_undo(),
        ParsedInput::Byte(0x17) => state.bio_delete_word_left(),
        ParsedInput::Byte(0x7F) => state.bio_backspace(),
        ParsedInput::Delete => state.bio_delete_right(),
        ParsedInput::CtrlBackspace | ParsedInput::Byte(0x08) => state.bio_delete_word_left(),
        ParsedInput::CtrlDelete => state.bio_delete_word_right(),
        ParsedInput::Arrow(b'A') => state.bio_cursor_up(),
        ParsedInput::Arrow(b'B') => state.bio_cursor_down(),
        ParsedInput::Arrow(b'C') => state.bio_cursor_right(),
        ParsedInput::Arrow(b'D') => state.bio_cursor_left(),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => state.bio_cursor_word_right(),
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => state.bio_cursor_word_left(),
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            let normalized = cleaned.replace("\r\n", "\n").replace('\r', "\n");
            for ch in normalized.chars() {
                if ch == '\n' || (!ch.is_control() && ch != '\u{7f}') {
                    state.bio_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => state.bio_push(ch),
        _ => {}
    }
}

fn handle_picker_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.settings_modal_state.close_picker(),
        ParsedInput::Byte(b'\r') => app.settings_modal_state.apply_picker_selection(),
        ParsedInput::Byte(0x7F) => app.settings_modal_state.picker_backspace(),
        ParsedInput::Arrow(b'B') => app.settings_modal_state.picker_move(1),
        ParsedInput::Arrow(b'A') => app.settings_modal_state.picker_move(-1),
        ParsedInput::PageDown => {
            let page = app
                .settings_modal_state
                .picker()
                .visible_height
                .get()
                .max(1) as isize;
            app.settings_modal_state.picker_move(page);
        }
        ParsedInput::PageUp => {
            let page = app
                .settings_modal_state
                .picker()
                .visible_height
                .get()
                .max(1) as isize;
            app.settings_modal_state.picker_move(-page);
        }
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.settings_modal_state.picker_move(-3),
            MouseEventKind::ScrollDown => app.settings_modal_state.picker_move(3),
            _ => {}
        },
        ParsedInput::Char(ch) if !ch.is_control() => app.settings_modal_state.picker_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.settings_modal_state.picker_push(byte as char)
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_keys_include_esc_and_q() {
        assert!(is_close_event(&ParsedInput::Byte(0x1B)));
        assert!(is_close_event(&ParsedInput::Char('q')));
        assert!(is_close_event(&ParsedInput::Char('Q')));
        assert!(is_close_event(&ParsedInput::Byte(b'q')));
        assert!(is_close_event(&ParsedInput::Byte(b'Q')));
        assert!(!is_close_event(&ParsedInput::Char('?')));
    }
}
