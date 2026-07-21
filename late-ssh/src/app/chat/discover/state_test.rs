use crate::app::chat::discover::state::*;
use crate::app::chat::svc::DiscoverRoomItem;
use chrono::Utc;
use uuid::Uuid;

fn item(slug: &str) -> DiscoverRoomItem {
    DiscoverRoomItem {
        room_id: Uuid::from_u128(1),
        slug: slug.to_string(),
        member_count: 1,
        message_count: 0,
        last_message_at: Some(Utc::now()),
        recent: Vec::new(),
    }
}

#[test]
fn start_loading_clears_empty_state_until_items_arrive() {
    let mut state = State::new();

    state.start_loading();

    assert!(state.is_loading());
    assert!(state.visible_items().is_empty());
}

#[test]
fn set_items_marks_loading_complete() {
    let mut state = State::new();
    state.start_loading();

    state.set_items(Vec::new());

    assert!(!state.is_loading());
    assert!(state.visible_items().is_empty());
}

#[test]
fn filter_narrows_visible_items_case_insensitively() {
    let mut state = State::new();
    state.set_items(vec![item("rust"), item("Python"), item("rust-gamedev")]);

    state.start_filter();
    for ch in "RUST".chars() {
        state.push_char(ch);
    }

    let visible: Vec<_> = state
        .visible_items()
        .iter()
        .map(|i| i.slug.clone())
        .collect();
    assert_eq!(visible, vec!["rust", "rust-gamedev"]);
}

#[test]
fn selection_tracks_filtered_list() {
    let mut state = State::new();
    state.set_items(vec![item("alpha"), item("beta"), item("betamax")]);

    state.start_filter();
    for ch in "beta".chars() {
        state.push_char(ch);
    }
    // Query reset selection to the top of the filtered list.
    assert_eq!(state.selected_index(), 0);
    assert_eq!(
        state.selected_item().map(|i| i.slug.clone()),
        Some("beta".into())
    );

    state.move_selection(1);
    assert_eq!(
        state.selected_item().map(|i| i.slug.clone()),
        Some("betamax".into())
    );
    // Cannot move past the end of the filtered list.
    state.move_selection(1);
    assert_eq!(
        state.selected_item().map(|i| i.slug.clone()),
        Some("betamax".into())
    );
}

#[test]
fn cancel_filter_restores_full_list() {
    let mut state = State::new();
    state.set_items(vec![item("alpha"), item("beta")]);

    state.start_filter();
    state.push_char('z');
    assert!(state.visible_items().is_empty());

    state.cancel_filter();
    assert!(!state.is_filtering());
    assert_eq!(state.visible_items().len(), 2);
}
