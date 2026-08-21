use crate::app::chat::discover::state::*;
use crate::app::chat::svc::DiscoverRoomItem;
use chrono::Utc;
use uuid::Uuid;

fn item(slug: &str) -> DiscoverRoomItem {
    DiscoverRoomItem {
        room_id: Uuid::from_u128(1),
        slug: slug.to_string(),
        topic: None,
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

fn item_with_members(slug: &str, member_count: i64) -> DiscoverRoomItem {
    DiscoverRoomItem {
        member_count,
        ..item(slug)
    }
}

#[test]
fn default_order_is_whatever_the_query_returned() {
    let mut state = State::new();
    state.set_items(vec![
        item_with_members("quiet-but-huge", 500),
        item_with_members("busy-today", 3),
    ]);

    assert_eq!(state.sort(), SortMode::Activity);
    assert_eq!(
        state
            .visible_items()
            .iter()
            .map(|i| &i.slug)
            .collect::<Vec<_>>(),
        vec!["quiet-but-huge", "busy-today"]
    );
}

#[test]
fn sorting_by_members_puts_the_biggest_rooms_first() {
    let mut state = State::new();
    state.set_items(vec![
        item_with_members("small", 3),
        item_with_members("huge", 500),
        item_with_members("medium", 40),
    ]);

    state.cycle_sort();

    assert_eq!(state.sort(), SortMode::Members);
    assert_eq!(
        state
            .visible_items()
            .iter()
            .map(|i| &i.slug)
            .collect::<Vec<_>>(),
        vec!["huge", "medium", "small"]
    );

    // And back again, without disturbing the underlying list.
    state.cycle_sort();
    assert_eq!(
        state
            .visible_items()
            .iter()
            .map(|i| &i.slug)
            .collect::<Vec<_>>(),
        vec!["small", "huge", "medium"]
    );
}

/// Re-sorting moves rooms out from under the cursor, so the selection has to
/// go back to the top rather than silently pointing at a different room.
#[test]
fn cycling_the_sort_resets_the_selection() {
    let mut state = State::new();
    state.set_items(vec![
        item_with_members("small", 3),
        item_with_members("huge", 500),
    ]);
    state.move_selection(1);
    assert_eq!(state.selected_index(), 1);

    state.cycle_sort();

    assert_eq!(state.selected_index(), 0);
    assert_eq!(
        state.selected_item().map(|i| i.slug.clone()),
        Some("huge".into())
    );
}

#[test]
fn sorting_applies_to_the_filtered_list_too() {
    let mut state = State::new();
    state.set_items(vec![
        item_with_members("rust-small", 3),
        item_with_members("python", 900),
        item_with_members("rust-huge", 500),
    ]);

    state.start_filter();
    for ch in "rust".chars() {
        state.push_char(ch);
    }
    state.cycle_sort();

    assert_eq!(
        state
            .visible_items()
            .iter()
            .map(|i| &i.slug)
            .collect::<Vec<_>>(),
        vec!["rust-huge", "rust-small"]
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
