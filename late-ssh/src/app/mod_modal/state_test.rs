use super::*;

#[test]
fn scrollback_keeps_last_thousand_lines_fifo() {
    let mut state = ModModalState::new();

    for idx in 0..1005 {
        state.append_info(format!("line {idx}"));
    }

    assert_eq!(state.log().len(), 1000);
    assert_eq!(state.log().front().unwrap().text, "line 5");
    assert_eq!(state.log().back().unwrap().text, "line 1004");
}

#[test]
fn clear_screen_preserves_scrollback() {
    let mut state = ModModalState::new();
    state.append_info("before");

    state.clear_screen();

    assert_eq!(state.log().len(), 1);
    assert_eq!(state.viewport_start(8), 1);
    state.scroll_log(1);
    assert_eq!(state.viewport_start(8), 0);
}

#[test]
fn first_moderator_open_displays_command_help_once() {
    let mut state = ModModalState::new();

    state.open(true);

    assert!(
        state
            .log()
            .iter()
            .any(|line| line.text == "rename-room <#oldname> <#newname>"),
        "first open should display command help: {:?}",
        state.log()
    );
    let first_len = state.log().len();

    state.open(true);

    assert_eq!(
        state.log().len(),
        first_len,
        "subsequent opens should not replay help"
    );
}

#[test]
fn first_non_moderator_open_displays_access_denied() {
    let mut state = ModModalState::new();

    state.open(false);

    assert_eq!(state.log().len(), 1);
    assert_eq!(
        state.log().front().unwrap().text,
        "access denied: moderator or admin only"
    );
}

#[test]
fn command_input_adds_separator_between_runs() {
    let mut state = ModModalState::new();

    state.append_input("help");
    state.append_result(true, vec!["ok".to_string()]);
    state.append_input("sessions");

    assert!(
        state
            .log()
            .iter()
            .any(|line| line.kind == ModLogKind::Separator && line.text == COMMAND_SEPARATOR)
    );
}

#[test]
fn autocomplete_query_detects_at_prefixed_current_token() {
    let mut state = ModModalState::new();
    state.command_input.insert_str("ban server @ali");

    assert_eq!(
        state.autocomplete_query(),
        Some((11, '@', "ali".to_string()))
    );
}

#[test]
fn autocomplete_query_detects_hash_prefixed_current_token() {
    let mut state = ModModalState::new();
    state.command_input.insert_str("ban #rust");

    assert_eq!(
        state.autocomplete_query(),
        Some((4, '#', "rust".to_string()))
    );
}

#[test]
fn autocomplete_query_ignores_at_without_word_boundary() {
    let mut state = ModModalState::new();
    state.command_input.insert_str("ban server nope@ali");

    assert_eq!(state.autocomplete_query(), None);
}

#[test]
fn autocomplete_confirm_replaces_query_with_selected_username() {
    let mut state = ModModalState::new();
    state.command_input.insert_str("ban server @ali");
    state.update_autocomplete_matches(
        11,
        "ali".to_string(),
        vec![MentionMatch {
            name: "alice".to_string(),
            online: true,
            prefix: "@",
            description: None,
        }],
    );

    state.ac_confirm();

    assert_eq!(state.command_text(), "ban server @alice");
    assert!(!state.is_autocomplete_active());
}
