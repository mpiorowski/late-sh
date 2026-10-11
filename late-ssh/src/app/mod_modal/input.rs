use ratatui_textarea::{Input, Key};

use crate::app::common::readline::ctrl_byte_to_input;
use crate::app::input::{ParsedInput, insert_pasted_text};
use crate::app::{mod_modal::state::ModModalState, state::App};
use crate::moderation::command::{ModCommand, parse_mod_command};

pub(crate) fn open(app: &mut App, help_topic: Option<&str>) {
    app.show_help = false;
    app.show_settings = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_poll_modal = false;
    app.poll_modal_state.close();
    app.show_quit_confirm = false;
    app.mod_modal_state
        .open(app.permissions.can_access_mod_surface(), help_topic);
    app.show_mod_modal = true;
}

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if let ParsedInput::Paste(pasted) = event {
        paste_into_command_input(&mut app.mod_modal_state, &pasted);
        update_autocomplete(app);
        return;
    }

    if app.mod_modal_state.is_autocomplete_active() {
        match event {
            ParsedInput::Byte(0x1B) => {
                app.mod_modal_state.ac_dismiss();
                return;
            }
            ParsedInput::Byte(b'\t') | ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
                app.mod_modal_state.ac_confirm();
                return;
            }
            ParsedInput::Arrow(b'A') => {
                app.mod_modal_state.ac_move_selection(-1);
                return;
            }
            ParsedInput::Arrow(b'B') => {
                app.mod_modal_state.ac_move_selection(1);
                return;
            }
            _ => {}
        }
    }

    match event {
        ParsedInput::Byte(0x1B) => app.show_mod_modal = false,
        ParsedInput::Byte(0x18) => {
            app.mod_modal_state.clear_screen();
            update_autocomplete(app);
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => submit(app),
        ParsedInput::Byte(0x7F | 0x08) => {
            app.mod_modal_state.input(key_input(Key::Backspace));
            update_autocomplete(app);
        }
        ParsedInput::CtrlBackspace => {
            app.mod_modal_state.input(ctrl_input('w'));
            update_autocomplete(app);
        }
        ParsedInput::Delete => {
            app.mod_modal_state.input(key_input(Key::Delete));
            update_autocomplete(app);
        }
        ParsedInput::Home => {
            app.mod_modal_state.input(key_input(Key::Home));
            update_autocomplete(app);
        }
        ParsedInput::End => {
            app.mod_modal_state.input(key_input(Key::End));
            update_autocomplete(app);
        }
        ParsedInput::Arrow(b'A') => app.mod_modal_state.scroll_log(1),
        ParsedInput::Arrow(b'B') => app.mod_modal_state.scroll_log(-1),
        ParsedInput::Arrow(b'C') => {
            app.mod_modal_state.input(key_input(Key::Right));
            update_autocomplete(app);
        }
        ParsedInput::Arrow(b'D') => {
            app.mod_modal_state.input(key_input(Key::Left));
            update_autocomplete(app);
        }
        ParsedInput::CtrlArrow(b'C') => {
            app.mod_modal_state.input(ctrl_key_input(Key::Right));
            update_autocomplete(app);
        }
        ParsedInput::CtrlArrow(b'D') => {
            app.mod_modal_state.input(ctrl_key_input(Key::Left));
            update_autocomplete(app);
        }
        ParsedInput::AltArrow(b'C') => {
            app.mod_modal_state.input(alt_key_input(Key::Right));
            update_autocomplete(app);
        }
        ParsedInput::AltArrow(b'D') => {
            app.mod_modal_state.input(alt_key_input(Key::Left));
            update_autocomplete(app);
        }
        ParsedInput::PageUp => app.mod_modal_state.scroll_log(8),
        ParsedInput::PageDown => app.mod_modal_state.scroll_log(-8),
        ParsedInput::Mouse(mouse) => {
            if let Some(delta) = super::ui::mouse_scroll_delta(mouse) {
                app.mod_modal_state.scroll_log(delta);
            }
        }
        ParsedInput::Char(ch) => {
            app.mod_modal_state.input(key_input(Key::Char(ch)));
            update_autocomplete(app);
        }
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.mod_modal_state
                .input(key_input(Key::Char(byte as char)));
            update_autocomplete(app);
        }
        ParsedInput::Byte(byte) => {
            if let Some(input) = ctrl_byte_to_input(byte) {
                app.mod_modal_state.input(input);
                update_autocomplete(app);
            }
        }
        _ => {}
    }
}

fn submit(app: &mut App) {
    let command = app.mod_modal_state.command_text();
    submit_command(app, command);
    app.mod_modal_state.clear_command();
}

/// Run a command without changing the user's command draft.
pub(crate) fn submit_command(app: &mut App, command: String) {
    if !app.permissions.can_access_mod_surface() {
        app.mod_modal_state
            .append_error("access denied: moderator or admin only");
        return;
    }
    if command.is_empty() {
        app.mod_modal_state.append_info("type help for commands");
        return;
    }
    app.mod_modal_state.append_input(&command);
    let is_help = matches!(parse_mod_command(&command), Ok(ModCommand::Help { .. }));
    let request_id = app.chat.submit_mod_command(command);
    app.mod_modal_state.append_pending(request_id, is_help);
}

fn update_autocomplete(app: &mut App) {
    let Some((trigger_offset, trigger, query)) = app.mod_modal_state.autocomplete_query() else {
        app.mod_modal_state.ac_dismiss();
        return;
    };
    let query_lower = query.to_ascii_lowercase();
    let matches = match trigger {
        '@' => app.chat.username_mention_matches(&query_lower),
        '#' => app.chat.room_name_matches(&query_lower),
        _ => Vec::new(),
    };
    app.mod_modal_state
        .update_autocomplete_matches(trigger_offset, query, matches);
}

fn paste_into_command_input(state: &mut ModModalState, pasted: &[u8]) {
    insert_pasted_text(pasted, |ch| {
        let ch = if ch == '\n' { ' ' } else { ch };
        state.input(key_input(Key::Char(ch)));
    });
}

fn key_input(key: Key) -> Input {
    Input {
        key,
        ctrl: false,
        alt: false,
        shift: false,
    }
}

fn ctrl_input(ch: char) -> Input {
    Input {
        key: Key::Char(ch),
        ctrl: true,
        alt: false,
        shift: false,
    }
}

fn ctrl_key_input(key: Key) -> Input {
    Input {
        key,
        ctrl: true,
        alt: false,
        shift: false,
    }
}

fn alt_key_input(key: Key) -> Input {
    Input {
        key,
        ctrl: false,
        alt: true,
        shift: false,
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
