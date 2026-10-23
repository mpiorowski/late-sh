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

fn favorite(video_id: &str, title: &str) -> FavoriteItemView {
    FavoriteItemView {
        id: Uuid::nil(),
        video_id: video_id.to_string(),
        title: Some(title.to_string()),
        channel: None,
        duration_ms: None,
        is_stream: false,
    }
}

fn video_ids<'a>(rows: &[HistoryRow<'a>]) -> Vec<&'a str> {
    rows.iter().map(HistoryRow::video_id).collect()
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
    assert_eq!(state.history_rows(&[], &history).len(), history.len());
}

#[test]
fn filter_matches_title_channel_and_video_id_case_insensitively() {
    let mut state = BoothModalState::default();
    state.enter_history_filter();
    for ch in "lofi".chars() {
        state.push_history_filter(ch);
    }
    let history = history();
    let filtered = state.history_rows(&[], &history);
    // "Lofi Beats" (title) and "Lofi Radio" (channel) both match.
    assert_eq!(video_ids(&filtered), vec!["aaa", "ccc"]);

    state.clear_history_filter_query();
    for ch in "DDD".chars() {
        state.push_history_filter(ch);
    }
    let filtered = state.history_rows(&[], &history);
    assert_eq!(video_ids(&filtered), vec!["ddd"]);
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
    state.move_selection(1, state.history_rows(&[], &history).len());
    let row = state.selected_history_row(&[], &history).unwrap();
    assert_eq!(row.video_id(), "ccc");
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

/// Favorites lead the list, a favorite still in History is not listed twice,
/// and the filter reaches favorites too, so selection indexes the same rows
/// the list draws.
#[test]
fn favorites_lead_the_list_once_each_and_the_filter_covers_them() {
    let mut state = BoothModalState::default();
    state.set_focus(BoothFocus::History);
    let history = history();
    let favorites = vec![
        favorite("zzz", "Lofi Forever"),
        favorite("bbb", "Jazz Night"),
    ];

    let rows = state.history_rows(&favorites, &history);
    assert_eq!(video_ids(&rows), vec!["zzz", "bbb", "aaa", "ccc", "ddd"]);
    assert!(matches!(
        rows[1],
        HistoryRow::Favorite {
            played: Some(_),
            ..
        }
    ));
    assert!(matches!(rows[0], HistoryRow::Favorite { played: None, .. }));
    assert_eq!(rows[0].play_count(), None);

    state.enter_history_filter();
    for ch in "lofi".chars() {
        state.push_history_filter(ch);
    }
    let rows = state.history_rows(&favorites, &history);
    assert_eq!(video_ids(&rows), vec!["zzz", "aaa", "ccc"]);
    state.move_selection(1, rows.len());
    assert_eq!(
        state
            .selected_history_row(&favorites, &history)
            .map(|row| row.video_id()),
        Some("aaa")
    );
}
