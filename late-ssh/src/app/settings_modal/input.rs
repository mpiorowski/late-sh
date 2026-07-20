use crate::app::input::{MouseButton, MouseEventKind, ParsedInput, sanitize_paste_markers};
use crate::app::state::App;

use super::gem::GemKey;
use super::state::{
    AccountRow, BIO_MAX_LEN, FEED_URL_MAX_LEN, IrcTokenFocus, LinkAccountEnterCodeFocus,
    LinkAccountStep, PickerKind, Row, SYSTEM_FIELD_MAX_LEN, Tab, TweakRow, USERNAME_MAX_LEN,
};
use crate::app::common::textarea_input::{
    EditOutcome, handle_multiline_edit, handle_single_line_edit,
};
use crate::app::settings_modal::state::SettingsModalState;

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if app.settings_modal_state.link_account_dialog().open() {
        handle_link_account_dialog_input(app, event);
        return;
    }

    if app.settings_modal_state.delete_account_dialog().open() {
        handle_delete_account_dialog_input(app, event);
        return;
    }

    if app.settings_modal_state.irc_token_dialog().open() {
        handle_irc_token_dialog_input(app, event);
        return;
    }

    if app.settings_modal_state.right_sidebar_components_open() {
        handle_right_sidebar_components_input(app, event);
        return;
    }

    if app.settings_modal_state.picker_open() {
        handle_picker_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_username() {
        handle_username_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_system_field().is_some() {
        handle_system_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_bio() {
        handle_bio_input(app, event);
        return;
    }

    if app.settings_modal_state.editing_feed_url() {
        handle_feed_url_input(app, event);
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

    // Tab-strip clicks and body scroll-wheel are handled at the top level so
    // they work from every tab. Per-tab mouse handlers (e.g. the Special-tab
    // gem) still get a shot at any mouse event we don't claim here.
    if let ParsedInput::Mouse(mouse) = &event
        && handle_top_level_mouse(app, *mouse)
    {
        return;
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

    if app.settings_modal_state.selected_tab() == Tab::Account {
        handle_account_tab_input(app, event);
        return;
    }

    if app.settings_modal_state.selected_tab() == Tab::Feeds {
        handle_feeds_tab_input(app, event);
        return;
    }

    if app.settings_modal_state.selected_tab() == Tab::Tweaks {
        handle_tweaks_tab_input(app, event);
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

fn handle_feeds_tab_input(app: &mut App, event: ParsedInput) {
    let state: &mut SettingsModalState = &mut app.settings_modal_state;
    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') | ParsedInput::Arrow(b'B') => {
            state.move_feed_cursor(1)
        }
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') | ParsedInput::Arrow(b'A') => {
            state.move_feed_cursor(-1)
        }
        ParsedInput::Byte(b'd')
        | ParsedInput::Char('d')
        | ParsedInput::Byte(0x7F)
        | ParsedInput::Delete => state.remove_selected_feed(),
        ParsedInput::Byte(b'r') | ParsedInput::Char('r') | ParsedInput::Char('R') => {
            state.refresh_feeds();
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Char('a') | ParsedInput::Char('A')
            if state.feed_index_is_add_row() =>
        {
            state.start_feed_url_edit();
        }
        _ => {}
    }
}

/// Tweaks tab: a list of fine-grained behavior toggles plus the gem easter
/// egg. `j`/`k`/arrows move between rows, `Enter`/`Space` flip the selected
/// toggle, `←`/`→` cycle enum-like rows, `h`/`l` feed the gem, and a left-click on the gem
/// footprint counts as a gem interaction.
fn handle_tweaks_tab_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.settings_modal_state.move_tweak_row(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.settings_modal_state.move_tweak_row(-1),
        // Enter on the Right sidebar row opens the panel editor; everywhere
        // else (and Space / ← / →) it just flips the selected toggle.
        ParsedInput::Byte(b'\r') | ParsedInput::Char('e' | 'E')
            if app.settings_modal_state.selected_tweak_row() == TweakRow::RightSidebar =>
        {
            app.settings_modal_state.open_right_sidebar_components();
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') => {
            app.settings_modal_state.toggle_selected_tweak()
        }
        ParsedInput::Arrow(b'C') => app.settings_modal_state.cycle_selected_tweak(true),
        ParsedInput::Arrow(b'D') => app.settings_modal_state.cycle_selected_tweak(false),
        ParsedInput::Byte(b'h') | ParsedInput::Char('h') => {
            app.settings_modal_state.gem_mut().handle_key(GemKey::H);
        }
        ParsedInput::Byte(b'l') | ParsedInput::Char('l') => {
            app.settings_modal_state.gem_mut().handle_key(GemKey::L);
        }
        ParsedInput::Mouse(mouse)
            if mouse.kind == MouseEventKind::Down && mouse.button == Some(MouseButton::Left) =>
        {
            let Some(x) = mouse.x.checked_sub(1) else {
                return;
            };
            let Some(y) = mouse.y.checked_sub(1) else {
                return;
            };
            let hit = app
                .settings_modal_state
                .gem()
                .hit_area
                .get()
                .filter(|rect| {
                    x >= rect.x
                        && x < rect.x + rect.width
                        && y >= rect.y
                        && y < rect.y + rect.height
                });
            if hit.is_some() {
                app.settings_modal_state.gem_mut().handle_click();
            }
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
        .set_keep_composer_focused(app.profile_state.profile().keep_composer_focused);
    app.help_modal_state
        .open(crate::app::help_modal::data::HelpTopic::Overview);
    app.show_help = true;
}

fn handle_account_tab_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.settings_modal_state.move_account_row(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.settings_modal_state.move_account_row(-1),
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') => {
            match app.settings_modal_state.selected_account_row() {
                AccountRow::LinkAccounts => app.settings_modal_state.open_link_account_dialog(),
                AccountRow::IrcToken => app.settings_modal_state.open_irc_token_dialog(),
                AccountRow::DeleteAccount => app.settings_modal_state.open_delete_account_dialog(),
            }
        }
        _ => {}
    }
}

pub fn handle_escape(app: &mut App) {
    handle_input(app, ParsedInput::Byte(0x1B));
}

/// Handle tab-strip clicks and body scroll-wheel at the top level. Returns
/// `true` if the event was claimed (caller should `return` early). False
/// otherwise — the event then falls through to per-tab handlers, which may
/// have their own mouse semantics (e.g. the Special-tab gem).
fn handle_top_level_mouse(app: &mut App, mouse: crate::app::input::MouseEvent) -> bool {
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    match mouse.kind {
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            if let Some(tab) = app.settings_modal_state.tab_at_point(x, y) {
                app.settings_modal_state.select_tab(tab);
                return true;
            }
            false
        }
        MouseEventKind::ScrollUp if app.settings_modal_state.body_contains(x, y) => {
            scroll_current_tab(app, -3)
        }
        MouseEventKind::ScrollDown if app.settings_modal_state.body_contains(x, y) => {
            scroll_current_tab(app, 3)
        }
        _ => false,
    }
}

/// Scroll the row cursor on tabs that have one. Returns `true` if the wheel
/// was consumed. Tabs without a list (Bio, Themes-with-its-own-scroll,
/// Special) are left alone here.
fn scroll_current_tab(app: &mut App, delta: isize) -> bool {
    match app.settings_modal_state.selected_tab() {
        Tab::Settings => {
            app.settings_modal_state.move_row(delta);
            true
        }
        Tab::Account => {
            app.settings_modal_state.move_account_row(delta);
            true
        }
        Tab::Feeds => {
            app.settings_modal_state.move_feed_cursor(delta);
            true
        }
        _ => false,
    }
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
        Row::Birthday | Row::Ide | Row::Terminal | Row::Os | Row::Langs => {
            if let Some(field) = crate::app::settings_modal::state::SystemField::from_row(
                app.settings_modal_state.selected_row(),
            ) {
                app.settings_modal_state.start_system_field_edit(field);
            }
        }
        Row::Theme
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

fn handle_right_sidebar_components_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.settings_modal_state.close_right_sidebar_components();
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app
            .settings_modal_state
            .move_right_sidebar_components_cursor(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app
            .settings_modal_state
            .move_right_sidebar_components_cursor(-1),
        // [ / ] reorder the selected panel up / down.
        ParsedInput::Byte(b'[') | ParsedInput::Char('[') => {
            app.settings_modal_state.move_right_sidebar_component(-1)
        }
        ParsedInput::Byte(b']') | ParsedInput::Char(']') => {
            app.settings_modal_state.move_right_sidebar_component(1)
        }
        ParsedInput::Byte(b' ' | b'\r') | ParsedInput::Char('e' | 'E') => {
            app.settings_modal_state.toggle_right_sidebar_component()
        }
        _ => {}
    }
}

fn handle_system_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match handle_single_line_edit(state.system_input_mut(), &event, SYSTEM_FIELD_MAX_LEN) {
        EditOutcome::Submit => state.submit_system_field(),
        EditOutcome::Cancel => state.cancel_system_field_edit(),
        EditOutcome::Handled | EditOutcome::Ignored => {}
    }
}

fn handle_username_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match handle_single_line_edit(state.username_input_mut(), &event, USERNAME_MAX_LEN) {
        EditOutcome::Submit => state.submit_username(),
        EditOutcome::Cancel => state.cancel_username_edit(),
        EditOutcome::Handled | EditOutcome::Ignored => {}
    }
}

fn handle_link_account_dialog_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    if state.link_account_dialog().pending() {
        return;
    }

    match event {
        ParsedInput::Byte(0x1B) => state.close_link_account_dialog(),
        ParsedInput::Byte(b'\r') => match state.link_account_dialog().step() {
            LinkAccountStep::EnterCode => state.activate_link_account_enter_code(),
            LinkAccountStep::Confirm => state.submit_link_account_confirmation(),
            LinkAccountStep::Pending => state.close_link_account_dialog(),
        },
        ParsedInput::Byte(b' ')
            if state.link_account_dialog().step() == LinkAccountStep::EnterCode =>
        {
            state.activate_link_account_enter_code();
        }
        ParsedInput::Arrow(b'A')
            if state.link_account_dialog().step() == LinkAccountStep::EnterCode =>
        {
            state.move_link_account_enter_code_focus(LinkAccountEnterCodeFocus::GenerateCode);
        }
        ParsedInput::Arrow(b'B')
            if state.link_account_dialog().step() == LinkAccountStep::EnterCode =>
        {
            state.move_link_account_enter_code_focus(LinkAccountEnterCodeFocus::PeerCode);
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Arrow(b'D')
            if state.link_account_dialog().step() == LinkAccountStep::Confirm =>
        {
            state.select_link_account_main(true);
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Arrow(b'C')
            if state.link_account_dialog().step() == LinkAccountStep::Confirm =>
        {
            state.select_link_account_main(false);
        }
        ParsedInput::Byte(0x15) => state.clear_link_account_input(),
        ParsedInput::Byte(0x01) => state.link_account_cursor_home(),
        ParsedInput::Byte(0x05) => state.link_account_cursor_end(),
        ParsedInput::Home => state.link_account_cursor_home(),
        ParsedInput::End => state.link_account_cursor_end(),
        ParsedInput::Byte(0x7F | 0x08) => state.link_account_backspace(),
        ParsedInput::Delete => state.link_account_delete_right(),
        ParsedInput::CtrlBackspace => state.link_account_delete_word_left(),
        ParsedInput::CtrlDelete => state.link_account_delete_word_right(),
        ParsedInput::Arrow(b'C') => state.link_account_cursor_right(),
        ParsedInput::Arrow(b'D') => state.link_account_cursor_left(),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            state.link_account_cursor_word_right()
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            state.link_account_cursor_word_left()
        }
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            for ch in cleaned.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    state.link_account_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => state.link_account_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            state.link_account_push(byte as char)
        }
        _ => {}
    }
}

fn handle_delete_account_dialog_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    if state.delete_account_dialog().pending() {
        if matches!(event, ParsedInput::Byte(0x1B)) {
            state.close_delete_account_dialog();
        }
        return;
    }
    match event {
        ParsedInput::Byte(0x1B) => state.close_delete_account_dialog(),
        ParsedInput::Byte(b'\r') => state.submit_delete_account_confirmation(),
        ParsedInput::Byte(0x15) => state.clear_delete_account_confirmation(),
        ParsedInput::Byte(0x01) => state.delete_account_cursor_home(),
        ParsedInput::Byte(0x05) => state.delete_account_cursor_end(),
        ParsedInput::Home => state.delete_account_cursor_home(),
        ParsedInput::End => state.delete_account_cursor_end(),
        ParsedInput::Byte(0x7F | 0x08) => state.delete_account_backspace(),
        ParsedInput::Delete => state.delete_account_delete_right(),
        ParsedInput::CtrlBackspace => state.delete_account_delete_word_left(),
        ParsedInput::CtrlDelete => state.delete_account_delete_word_right(),
        ParsedInput::Arrow(b'C') => state.delete_account_cursor_right(),
        ParsedInput::Arrow(b'D') => state.delete_account_cursor_left(),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            state.delete_account_cursor_word_right()
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            state.delete_account_cursor_word_left()
        }
        ParsedInput::Paste(pasted) => {
            let cleaned = sanitize_paste_markers(&String::from_utf8_lossy(&pasted));
            for ch in cleaned.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    state.delete_account_push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => state.delete_account_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            state.delete_account_push(byte as char)
        }
        _ => {}
    }
}

fn handle_irc_token_dialog_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    if state.irc_token_dialog().pending() {
        return;
    }

    if state.irc_token_dialog().revealed_token().is_some() {
        match event {
            ParsedInput::Byte(0x1B | b'\r' | b' ')
            | ParsedInput::Char(_)
            | ParsedInput::Paste(_) => {
                state.dismiss_irc_token_reveal();
            }
            _ => {}
        }
        return;
    }

    match event {
        ParsedInput::Byte(0x1B) => state.close_irc_token_dialog(),
        ParsedInput::Byte(b'\r' | b' ') => state.activate_irc_token_focus(),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A')
        | ParsedInput::Arrow(b'D') => {
            state.move_irc_token_focus(IrcTokenFocus::Primary);
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B')
        | ParsedInput::Arrow(b'C') => {
            state.move_irc_token_focus(IrcTokenFocus::Revoke);
        }
        _ => {}
    }
}

fn handle_feed_url_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match handle_single_line_edit(state.feed_url_input_mut(), &event, FEED_URL_MAX_LEN) {
        EditOutcome::Submit => state.submit_feed_url(),
        EditOutcome::Cancel => state.cancel_feed_url_edit(),
        EditOutcome::Handled | EditOutcome::Ignored => {}
    }
}

fn handle_bio_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    match handle_multiline_edit(state.bio_input_mut(), &event, BIO_MAX_LEN) {
        // Bio convention: Enter and Esc both leave edit mode and save.
        EditOutcome::Submit | EditOutcome::Cancel => state.stop_bio_edit(),
        EditOutcome::Handled | EditOutcome::Ignored => {}
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
#[path = "input_test.rs"]
mod input_test;

