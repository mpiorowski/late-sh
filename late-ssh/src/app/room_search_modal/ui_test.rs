use super::*;
use crate::app::{chat::state::RoomSlot, room_search_modal::state::RoomSearchItem};
use uuid::Uuid;

fn item(index: u128, favorite: bool) -> RoomSearchItem {
    RoomSearchItem {
        slot: RoomSlot::Room(Uuid::from_u128(index)),
        label: format!("#{index}"),
        meta: "public room".to_string(),
        unread_count: 0,
        last_message_at: None,
        favorite,
    }
}

fn viewport_contains(rows: &[ResultRow], start: usize, height: usize, selected: usize) -> bool {
    rows.iter()
        .skip(start)
        .take(height)
        .any(|row| *row == ResultRow::Item(selected))
}

#[test]
fn result_view_keeps_selected_visible_after_section_header() {
    let items = vec![
        item(0, true),
        item(1, true),
        item(2, true),
        item(3, false),
        item(4, false),
        item(5, false),
    ];
    let rows = result_rows(&items);
    let start = result_view_start(&rows, 3, 5);

    assert!(viewport_contains(&rows, start, 5, 3));
}

#[test]
fn result_view_can_show_selected_with_one_line_height() {
    let items = vec![item(0, false)];
    let rows = result_rows(&items);
    let start = result_view_start(&rows, 0, 1);

    assert!(viewport_contains(&rows, start, 1, 0));
}

#[test]
fn wrap_plain_respects_width_and_newlines() {
    assert_eq!(
        wrap_plain("alpha beta gamma", 11),
        vec!["alpha beta".to_string(), "gamma".to_string()]
    );
    assert_eq!(
        wrap_plain("one\ntwo", 10),
        vec!["one".to_string(), "two".to_string()]
    );
    assert_eq!(
        wrap_plain("abcdefgh", 3),
        vec!["abc".to_string(), "def".to_string(), "gh".to_string()]
    );
    assert_eq!(wrap_plain("", 10), vec![String::new()]);
}

#[test]
fn context_slot_layout_shrinks_sides_before_hit() {
    // Full pane: 4 context rows either side, 3 hit rows.
    assert_eq!(context_slot_layout(11), (4, 3));
    // Tighter panes shed context rows symmetrically, hit keeps >= 1 row.
    assert_eq!(context_slot_layout(9), (4, 1));
    assert_eq!(context_slot_layout(5), (2, 1));
    assert_eq!(context_slot_layout(3), (1, 1));
    assert_eq!(context_slot_layout(1), (0, 1));
    assert_eq!(context_slot_layout(0), (0, 1));
    // Roomy panes cap context at 4 per side; extra rows go to the hit.
    assert_eq!(context_slot_layout(14), (4, 6));
}

#[test]
fn truncate_tail_keeps_text_before_match() {
    assert_eq!(truncate_tail_to_width("short", 10), "short");
    assert_eq!(truncate_tail_to_width("abcdefgh", 5), "…efgh");
    assert_eq!(truncate_tail_to_width("abc", 0), "");
    assert_eq!(truncate_tail_to_width("abc", 1), "…");
}
