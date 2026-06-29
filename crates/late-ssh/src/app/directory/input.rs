use crate::app::{directory::state::DirectoryTab, input::ParsedInput, state::App};

use super::state::{filtered_profile_indices, filtered_project_indices};

pub(crate) fn handle_search_input(app: &mut App, event: &ParsedInput) -> bool {
    let len = search_result_len(app);
    app.directory_state.clamp_search_selection(len);

    match event {
        ParsedInput::Byte(0x1B) => app.directory_state.exit_search(),
        ParsedInput::Byte(b'\r') => submit_search(app),
        ParsedInput::Byte(0x7F | 0x08) => app.directory_state.search_backspace(),
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
            app.directory_state.move_search_selection(1, len);
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => {
            app.directory_state.move_search_selection(-1, len);
        }
        ParsedInput::PageDown => app.directory_state.move_search_selection(8, len),
        ParsedInput::PageUp => app.directory_state.move_search_selection(-8, len),
        ParsedInput::Char(ch) => app.directory_state.search_push(*ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || *byte == b' ' => {
            app.directory_state.search_push(*byte as char);
        }
        _ => {}
    }

    let len = search_result_len(app);
    app.directory_state.clamp_search_selection(len);
    true
}

fn submit_search(app: &mut App) {
    match app.directory_state.tab {
        DirectoryTab::Profiles => {
            let results = filtered_profile_indices(
                app.chat.work.all_items(),
                app.directory_state.search_query(),
            );
            if let Some((idx, _)) = results.get(app.directory_state.search_selected()) {
                app.chat.work.select_index(*idx);
            }
        }
        DirectoryTab::Projects => {
            let results = filtered_project_indices(
                app.chat.showcase.all_items(),
                app.directory_state.search_query(),
            );
            if let Some((idx, _)) = results.get(app.directory_state.search_selected()) {
                app.chat.showcase.select_index(*idx);
            }
        }
        DirectoryTab::Pinstar => {}
    }
    app.directory_state.exit_search();
}

fn search_result_len(app: &App) -> usize {
    match app.directory_state.tab {
        DirectoryTab::Profiles => filtered_profile_indices(
            app.chat.work.all_items(),
            app.directory_state.search_query(),
        )
        .len(),
        DirectoryTab::Projects => filtered_project_indices(
            app.chat.showcase.all_items(),
            app.directory_state.search_query(),
        )
        .len(),
        DirectoryTab::Pinstar => 0,
    }
}
