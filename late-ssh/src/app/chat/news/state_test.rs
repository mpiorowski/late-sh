use super::{clamp_index, move_index};

#[test]
fn clamp_index_handles_empty_list() {
    assert_eq!(clamp_index(4, 0), 0);
}

#[test]
fn clamp_index_caps_to_last_item() {
    assert_eq!(clamp_index(9, 3), 2);
}

#[test]
fn move_index_moves_within_bounds() {
    assert_eq!(move_index(2, -1, 5), 1);
    assert_eq!(move_index(2, 2, 5), 4);
}

#[test]
fn move_index_clamps_at_edges() {
    assert_eq!(move_index(0, -1, 5), 0);
    assert_eq!(move_index(4, 1, 5), 4);
}

#[test]
fn move_index_returns_zero_for_empty_list() {
    assert_eq!(move_index(0, 1, 0), 0);
    assert_eq!(move_index(3, -1, 0), 0);
}

#[test]
fn clamp_index_passes_through_when_within_bounds() {
    assert_eq!(clamp_index(1, 5), 1);
}
