use super::*;

fn item(video_id: &str, title: Option<&str>, channel: Option<&str>) -> HistoryItemView {
    HistoryItemView {
        id: Uuid::nil(),
        video_id: video_id.to_string(),
        title: title.map(str::to_string),
        channel: channel.map(str::to_string),
        duration_ms: None,
        is_stream: false,
        play_count: 0,
        last_played_at_ms: 0,
    }
}

fn history() -> Vec<HistoryItemView> {
    vec![
        item("aaa", Some("Lofi Beats"), Some("ChillHop")),
        item("bbb", Some("Jazz Night"), Some("BlueNote")),
        item("ccc", None, Some("Lofi Radio")),
        item("ddd", Some("Synthwave"), None),
    ]
}

#[test]
fn empty_query_matches_every_row() {
    let state = BoothModalState::default();
    let history = history();
    assert_eq!(state.filtered_history(&history).len(), history.len());
    assert_eq!(state.filtered_history_len(&history), history.len());
}

#[test]
fn filter_matches_title_channel_and_video_id_case_insensitively() {
    let mut state = BoothModalState::default();
    state.enter_history_filter();
    for ch in "lofi".chars() {
        state.push_history_filter(ch);
    }
    let history = history();
    let filtered = state.filtered_history(&history);
    // "Lofi Beats" (title) and "Lofi Radio" (channel) both match.
    assert_eq!(filtered.len(), 2);
    assert_eq!(state.filtered_history_len(&history), 2);
    assert_eq!(filtered[0].video_id, "aaa");
    assert_eq!(filtered[1].video_id, "ccc");

    state.clear_history_filter_query();
    for ch in "DDD".chars() {
        state.push_history_filter(ch);
    }
    let filtered = state.filtered_history(&history);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].video_id, "ddd");
}

#[test]
fn selected_history_item_indexes_into_filtered_list() {
    let mut state = BoothModalState::default();
    state.set_focus(BoothFocus::History);
    state.enter_history_filter();
    for ch in "lofi".chars() {
        state.push_history_filter(ch);
    }
    let history = history();
    // Move to the second filtered row.
    state.move_selection(1, state.filtered_history_len(&history));
    assert_eq!(state.selected_history_item_id(&history), Some(Uuid::nil()));
    let item = state.selected_history_item(&history).unwrap();
    assert_eq!(item.video_id, "ccc");
}

#[test]
fn editing_query_resets_selection_and_cancel_clears_query() {
    let mut state = BoothModalState::default();
    state.enter_history_filter();
    state.selected_history = 3;
    state.push_history_filter('a');
    assert_eq!(state.selected_history, 0);
    assert!(state.history_filter_engaged());

    state.cancel_history_filter();
    assert!(!state.history_filter_active());
    assert!(!state.history_filter_engaged());
    assert_eq!(state.history_filter_query(), "");
}

#[test]
fn filter_query_is_length_capped() {
    let mut state = BoothModalState::default();
    state.enter_history_filter();
    for _ in 0..(HISTORY_FILTER_MAX_LEN + 10) {
        state.push_history_filter('x');
    }
    assert_eq!(
        state.history_filter_query().chars().count(),
        HISTORY_FILTER_MAX_LEN
    );
}
