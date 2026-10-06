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
    assert_eq!(state.viewport_start(8, &[1]), 1);
    state.scroll_log(1);
    assert_eq!(state.viewport_start(8, &[1]), 0);
}

#[test]
fn first_moderator_open_displays_command_help_once() {
    let mut state = ModModalState::new();

    state.open(true, None);

    assert!(
        state
            .log()
            .iter()
            .any(|line| line.text == "rename-room <#oldname> <#newname>"),
        "first open should display command help: {:?}",
        state.log()
    );
    let first_len = state.log().len();

    state.open(true, None);

    assert_eq!(
        state.log().len(),
        first_len,
        "subsequent opens should not replay help"
    );
}

#[test]
fn contextual_help_starts_a_fresh_view_and_preserves_history_and_draft() {
    let mut state = ModModalState::new();
    state.open(true, None);
    state.append_info("earlier result");
    state.command_input.insert_str("help artboard");

    for _ in 0..2 {
        let prior_len = state.log().len();
        state.open(true, Some("artboard safety"));
        assert_eq!(
            state.viewport_start(100, &vec![1; state.log().len()]),
            prior_len
        );
        assert_eq!(state.command_text(), "help artboard");
        assert!(
            state.log().iter().skip(prior_len).all(|line| {
                line.kind == ModLogKind::Help && !line.text.contains("rename-room")
            })
        );
        assert_eq!(
            state.log()[prior_len].text,
            "artboard safety view [@user|piece-id-prefix]"
        );
        state.scroll_log(prior_len as i16);
        assert_eq!(state.viewport_start(100, &vec![1; state.log().len()]), 0);
    }
    assert!(state.log().iter().any(|line| line.text == "earlier result"));

    let mut first_open = ModModalState::new();
    first_open.open(true, Some("artboard safety"));
    assert!(
        first_open
            .log()
            .iter()
            .all(|line| { line.kind == ModLogKind::Help && !line.text.contains("rename-room") })
    );
}

#[test]
fn first_non_moderator_open_displays_access_denied() {
    let mut state = ModModalState::new();

    state.open(false, None);

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
    state.append_result(Uuid::now_v7(), true, vec!["ok".to_string()]);
    state.append_input("sessions");

    assert!(
        state
            .log()
            .iter()
            .any(|line| line.kind == ModLogKind::Separator && line.text == COMMAND_SEPARATOR)
    );
}

#[test]
fn help_responses_keep_their_kind_when_requests_finish_out_of_order() {
    let mut state = ModModalState::new();
    let help_id = Uuid::now_v7();
    let action_id = Uuid::now_v7();
    let failed_help_id = Uuid::now_v7();
    state.append_pending(help_id, true);
    state.append_pending(action_id, false);
    state.append_pending(failed_help_id, true);

    state.append_result(action_id, true, vec!["action completed".into()]);
    state.append_result(help_id, true, vec!["help text".into()]);
    state.append_result(failed_help_id, false, vec!["access denied".into()]);

    let results = state.log().iter().skip(3).collect::<Vec<_>>();
    assert_eq!(results[0].kind, ModLogKind::Success);
    assert_eq!(results[1].kind, ModLogKind::Help);
    assert_eq!(results[2].kind, ModLogKind::Error);
    assert!(state.pending_help.is_empty());
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
            presence: crate::app::chat::state::MatchPresence::Here,
            prefix: "@",
            description: None,
        }],
    );

    state.ac_confirm();

    assert_eq!(state.command_text(), "ban server @alice");
    assert!(!state.is_autocomplete_active());
}
