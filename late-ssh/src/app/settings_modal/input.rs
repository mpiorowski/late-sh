use crate::app::input::{MouseButton, MouseEventKind, ParsedInput, sanitize_paste_markers};
use crate::app::state::App;

use super::gem::GemKey;
use super::mouse::{Field, Target};
use super::state::{
    AccountRow, BIO_MAX_LEN, FEED_URL_MAX_LEN, IrcTokenFocus, LinkAccountEnterCodeFocus,
    LinkAccountStep, PickerKind, Row, SYSTEM_FIELD_MAX_LEN, StatuslinePane, Tab, TweakRow,
    USERNAME_MAX_LEN,
};
use crate::app::common::textarea_input::{
    EditOutcome, handle_multiline_edit, handle_single_line_edit,
};
use crate::app::settings_modal::state::SettingsModalState;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if let ParsedInput::Mouse(mouse) = event {
        handle_mouse(app, mouse);
        return;
    }
    if app.settings_modal_state.mouse_save_pending() {
        return;
    }
    app.settings_modal_state.mouse.reveal_selection();
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

    if app.settings_modal_state.chat_badges_open() {
        handle_chat_badges_input(app, event);
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

    // An open theme search owns Esc: it backs out of the search, not out of
    // settings, the same way the Bio editor keeps its own keys.
    if app.settings_modal_state.selected_tab() == Tab::Themes
        && app.settings_modal_state.theme_searching()
    {
        handle_themes_tab_input(app, event);
        return;
    }

    // Statusline's options pane owns Esc, while Tab and tab-strip clicks
    // above still switch top-level tabs from either pane.
    if app.settings_modal_state.selected_tab() == Tab::Statusline {
        handle_statusline_input(app, event);
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
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') | ParsedInput::Byte(b'\r') => {
            activate_selected_row(app)
        }
        ParsedInput::Char('e') | ParsedInput::Char('E') => activate_selected_row(app),
        _ => {}
    }
}

fn handle_themes_tab_input(app: &mut App, event: ParsedInput) {
    let state: &mut SettingsModalState = &mut app.settings_modal_state;

    // While the search line is open every printable key edits the query, so
    // only the arrows move the cursor: j/k have to stay typeable, the same
    // bargain the Discover filter makes.
    if state.theme_searching() {
        match event {
            ParsedInput::Byte(0x1B) => state.cancel_theme_search(),
            ParsedInput::Arrow(b'B') => state.move_theme_cursor(1),
            ParsedInput::Arrow(b'A') => state.move_theme_cursor(-1),
            // Enter keeps the theme the cursor already previewed and puts the
            // full tree back.
            ParsedInput::Byte(b'\r') => state.cancel_theme_search(),
            ParsedInput::Byte(0x15) => state.clear_theme_query(),
            ParsedInput::Byte(0x7F | 0x08) | ParsedInput::Delete => state.backspace_theme_query(),
            ParsedInput::Char(ch) => state.push_theme_query_char(ch),
            ParsedInput::Byte(byte) if (32..127).contains(&byte) => {
                state.push_theme_query_char(byte as char)
            }
            _ => {}
        }
        return;
    }

    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        ParsedInput::Byte(b'/') | ParsedInput::Char('/') => state.start_theme_search(),
        ParsedInput::Byte(b'f' | b'F') | ParsedInput::Char('f' | 'F') => {
            state.toggle_theme_favorite()
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => state.move_theme_cursor(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => state.move_theme_cursor(-1),
        ParsedInput::Arrow(b'D') => state.theme_cursor_left(),
        ParsedInput::Arrow(b'C') => state.theme_cursor_right(),
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') | ParsedInput::Char(' ') => {
            state.toggle_theme_tree_row()
        }
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
        // Enter opens Right sidebar's panel editor. Interaction mode opens its
        // chooser on Enter/Space; Left/Right always retain keyboard cycling.
        ParsedInput::Byte(b'\r') | ParsedInput::Char('e' | 'E')
            if app.settings_modal_state.selected_tweak_row() == TweakRow::RightSidebar =>
        {
            app.settings_modal_state.open_right_sidebar_components();
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') | ParsedInput::Char(' ') => {
            toggle_tweak(app)
        }
        ParsedInput::Arrow(b'C') => cycle_tweak(app, true),
        ParsedInput::Arrow(b'D') => cycle_tweak(app, false),
        ParsedInput::Byte(b'h') | ParsedInput::Char('h') => {
            app.settings_modal_state.gem_mut().handle_key(GemKey::H);
        }
        ParsedInput::Byte(b'l') | ParsedInput::Char('l') => {
            app.settings_modal_state.gem_mut().handle_key(GemKey::L);
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
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') | ParsedInput::Char(' ') => {
            match app.settings_modal_state.selected_account_row() {
                AccountRow::LinkAccounts => app.settings_modal_state.open_link_account_dialog(),
                AccountRow::IrcToken => app.settings_modal_state.open_irc_token_dialog(),
                AccountRow::DeleteAccount => app.settings_modal_state.open_delete_account_dialog(),
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    handle_input(app, ParsedInput::Byte(0x1B));
}

fn handle_mouse(app: &mut App, mouse: crate::app::input::MouseEvent) {
    if !app.interaction_mode.mouse_enabled() {
        return;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return;
    };
    let state = &mut app.settings_modal_state;
    match mouse.kind {
        MouseEventKind::ScrollUp => state.mouse.scroll(x, y, -3, app.size),
        MouseEventKind::ScrollDown => state.mouse.scroll(x, y, 3, app.size),
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            let Some(target) = state.mouse.target(x, y, app.size) else {
                return;
            };
            if state.mouse_save_pending() {
                return;
            }
            let same_editor = matches!(target, Target::Caret(Field::Username, _, _) if state.editing_username())
                || matches!(target, Target::Caret(Field::System, _, _) if state.editing_system_field().is_some())
                || matches!(target, Target::Caret(Field::Bio, _, _) if state.editing_bio())
                || matches!(target, Target::Caret(Field::Feed, _, _) if state.editing_feed_url());
            if state.editing_text() && !same_editor && target != Target::Cancel {
                state.save_before_mouse_navigation(target);
            } else {
                activate_mouse_target(app, target);
            }
        }
        _ => {}
    }
}

pub(crate) fn activate_mouse_target(app: &mut App, target: Target) {
    if app.settings_modal_state.mouse_save_pending() {
        return;
    }
    let state = &mut app.settings_modal_state;
    if state.link_account_dialog().pending() || state.irc_token_dialog().pending() {
        return;
    }
    if state.delete_account_dialog().pending() && target != Target::Close {
        return;
    }
    state.select_mouse_target(target);
    match target {
        Target::Close => {
            if state.link_account_dialog().open() {
                state.close_link_account_dialog();
            } else if state.delete_account_dialog().open() {
                state.close_delete_account_dialog();
            } else if state.irc_token_dialog().open() {
                state.close_irc_token_dialog();
            } else if state.right_sidebar_components_open() {
                state.close_right_sidebar_components();
            } else if state.chat_badges_open() {
                state.close_chat_badges();
            } else if state.picker_open() {
                state.close_picker();
            } else {
                app.show_settings = false;
            }
        }
        Target::Tab(tab) => state.select_tab(tab),
        Target::Row(_) => activate_selected_row(app),
        Target::RowCycle(_, forward) => state.cycle_setting(forward),
        Target::Tweak(TweakRow::RightSidebar) => state.open_right_sidebar_components(),
        Target::Tweak(_) => toggle_tweak(app),
        Target::TweakCycle(_, forward) => cycle_tweak(app, forward),
        Target::SidebarMode => cycle_tweak(app, true),
        Target::SidebarPanels => state.open_right_sidebar_components(),
        Target::Account(row) => match row {
            AccountRow::LinkAccounts => state.open_link_account_dialog(),
            AccountRow::IrcToken => state.open_irc_token_dialog(),
            AccountRow::DeleteAccount => state.open_delete_account_dialog(),
        },
        Target::Theme(_) => {
            state.toggle_theme_tree_row();
            if state.theme_searching() {
                state.save();
            }
        }
        Target::Star(_) => state.toggle_theme_favorite(),
        Target::Search => state.start_theme_search(),
        Target::Status(_) => {
            state.focus_statusline_pane(StatuslinePane::Detail);
            state.mouse.reveal_selection();
        }
        Target::StatusToggle(_) => state.toggle_statusline_component(),
        Target::StatusMove(_, delta) => state.move_statusline_component(delta),
        Target::Dial(_) => {
            state.focus_statusline_pane(StatuslinePane::Detail);
            state.cycle_statusline_dial(true);
        }
        Target::DialCycle(_, forward) => {
            state.focus_statusline_pane(StatuslinePane::Detail);
            state.cycle_statusline_dial(forward);
        }
        Target::Sidebar(_) => state.toggle_right_sidebar_component(),
        Target::SidebarMove(_, delta) => state.move_right_sidebar_component(delta),
        Target::Badge(_) => state.toggle_chat_badge(),
        Target::Pick(_) => apply_picker_selection(app),
        Target::Bio => state.start_bio_edit(),
        Target::Feed(_) => {}
        Target::AddFeed => state.start_feed_url_edit(),
        Target::RemoveFeed => state.remove_selected_feed(),
        Target::RefreshFeeds => state.refresh_feeds(),
        Target::Submit => {} // the acknowledged edit is already committed
        Target::Cancel => {
            if state.editing_username() {
                state.cancel_username_edit();
            } else if state.editing_system_field().is_some() {
                state.cancel_system_field_edit();
            } else if state.editing_feed_url() {
                state.cancel_feed_url_edit();
            }
        }
        Target::Caret(field, row, col) => {
            if field == Field::Bio && !state.editing_bio() {
                state.start_bio_edit();
            }
            state.position_caret(field, row, col);
        }
        Target::GenerateCode => state.generate_link_account_code(),
        Target::LookupCode => {
            state.move_link_account_enter_code_focus(LinkAccountEnterCodeFocus::PeerCode);
            state.activate_link_account_enter_code();
        }
        Target::KeepAccount(keep) => state.select_link_account_main(keep),
        Target::ConfirmLink => state.submit_link_account_confirmation(),
        Target::ConfirmDelete => state.submit_delete_account_confirmation(),
        Target::Irc(focus) => {
            if state.irc_token_dialog().focus() != focus {
                state.move_irc_token_focus(focus);
            }
            state.activate_irc_token_focus();
        }
        Target::DismissToken => state.dismiss_irc_token_reveal(),
        Target::Gem => state.gem_mut().handle_click(),
    }
    app.settings_modal_state.mouse.invalidate();
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
        Row::Ide | Row::Terminal | Row::Os => {
            if let Some(field) = crate::app::settings_modal::state::SystemField::from_row(
                app.settings_modal_state.selected_row(),
            ) {
                app.settings_modal_state.start_system_field_edit(field);
            }
        }
        Row::Langs => crate::app::tag_picker::input::open(
            app,
            crate::app::tag_picker::state::TagPickerTarget::SettingsLangs,
        ),
        Row::Theme
        | Row::AutoTranslate
        | Row::TranslateMine
        | Row::DirectMessages
        | Row::Mentions
        | Row::GameEvents
        | Row::Streams
        | Row::Bell
        | Row::Cooldown
        | Row::NotifyFormat => app.settings_modal_state.cycle_setting(true),
        Row::Country => app.settings_modal_state.open_picker(PickerKind::Country),
        Row::Timezone => app.settings_modal_state.open_picker(PickerKind::Timezone),
        Row::TranslateTo => app.settings_modal_state.open_picker(PickerKind::Language),
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

/// Statusline tab. Two panes: the ordered segment list on the
/// left and the selected segment's dials on the right. `Enter` opens the
/// dials and `Esc` returns to the list; `←`/`→` change the focused dial.
///
/// Reordering is `⇧↑`/`⇧↓`, with `[`/`]` as the fallback for terminals that
/// swallow Shift+Arrow (and as the idiom the sibling sidebar-panel dialog
/// already trains).
fn handle_statusline_input(app: &mut App, event: ParsedInput) {
    let state = &mut app.settings_modal_state;
    let detail = state.statusline_pane() == StatuslinePane::Detail;
    match event {
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => open_help(app),
        // Esc backs out of the dials first, then closes.
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            if detail {
                state.focus_statusline_pane(StatuslinePane::List);
            } else {
                app.show_settings = false;
            }
        }
        // Reorder, from either pane: it acts on the selected segment, and
        // stepping into the dials shouldn't cost the user the shortcut.
        ParsedInput::ShiftArrow(b'A') | ParsedInput::Byte(b'[') | ParsedInput::Char('[') => {
            state.move_statusline_component(-1);
        }
        ParsedInput::ShiftArrow(b'B') | ParsedInput::Byte(b']') | ParsedInput::Char(']') => {
            state.move_statusline_component(1);
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            if detail {
                state.move_statusline_dial(1);
            } else {
                state.move_statusline_cursor(1);
            }
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            if detail {
                state.move_statusline_dial(-1);
            } else {
                state.move_statusline_cursor(-1);
            }
        }
        ParsedInput::Arrow(b'C') if detail => {
            state.cycle_statusline_dial(true);
        }
        ParsedInput::Arrow(b'D') if detail => {
            state.cycle_statusline_dial(false);
        }
        // Space is the list's on/off switch, and the dials' "change this one".
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') => {
            if detail {
                state.cycle_statusline_dial(true);
            } else {
                state.toggle_statusline_component();
            }
        }
        ParsedInput::Byte(b'\r') => {
            if detail {
                state.cycle_statusline_dial(true);
            } else {
                state.focus_statusline_pane(StatuslinePane::Detail);
            }
        }
        _ => {}
    }
}

fn handle_chat_badges_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.settings_modal_state.close_chat_badges();
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => app.settings_modal_state.move_chat_badges_cursor(1),
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => app.settings_modal_state.move_chat_badges_cursor(-1),
        ParsedInput::Byte(b' ' | b'\r') => app.settings_modal_state.toggle_chat_badge(),
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
        ParsedInput::Byte(b'\r') => apply_picker_selection(app),
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
        ParsedInput::Char(ch) if !ch.is_control() => app.settings_modal_state.picker_push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.settings_modal_state.picker_push(byte as char)
        }
        _ => {}
    }
}

/// Flip the selected tweak, then adopt it as this device's rail layout if it
/// was one of the two rail rows. The modal owns the account default; the device
/// layout is the app's, so the sync lives here rather than inside the modal.
fn toggle_tweak(app: &mut App) {
    if app.settings_modal_state.selected_tweak_row() == TweakRow::InteractionMode {
        app.settings_modal_state
            .open_picker(PickerKind::InteractionMode);
        return;
    }
    app.settings_modal_state.toggle_selected_tweak();
    app.sync_device_rails_from_settings();
}

fn apply_picker_selection(app: &mut App) {
    if let Some(mode) = app.settings_modal_state.picker_interaction_mode() {
        app.set_interaction_mode(mode);
        app.settings_modal_state.set_interaction_mode_display(mode);
    }
    app.settings_modal_state.apply_picker_selection();
}

fn cycle_tweak(app: &mut App, forward: bool) {
    if apply_interaction_mode_row(app, forward) {
        return;
    }
    app.settings_modal_state.cycle_selected_tweak(forward);
    app.sync_device_rails_from_settings();
}

/// If the Input row is selected, cycle the interaction mode on the app (which
/// flips the mouse live and persists) and mirror it into the modal for display.
/// Returns whether it handled the row.
fn apply_interaction_mode_row(app: &mut App, forward: bool) -> bool {
    if app.settings_modal_state.selected_tweak_row() != TweakRow::InteractionMode {
        return false;
    }
    let order = super::state::INTERACTION_MODES;
    let cur = order
        .iter()
        .position(|m| *m == app.interaction_mode)
        .unwrap_or(0);
    let next = if forward {
        (cur + 1) % order.len()
    } else {
        (cur + order.len() - 1) % order.len()
    };
    app.set_interaction_mode(order[next]);
    app.settings_modal_state
        .set_interaction_mode_display(order[next]);
    true
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
