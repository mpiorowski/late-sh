use super::wrap_index;

#[test]
fn moving_up_from_the_first_row_wraps_to_the_last() {
    assert_eq!(wrap_index(0, -1, 8), 7);
}

#[test]
fn moving_down_from_the_last_row_wraps_to_the_first() {
    assert_eq!(wrap_index(7, 1, 8), 0);
}

#[test]
fn moves_inside_the_list_do_not_wrap() {
    assert_eq!(wrap_index(3, 1, 8), 4);
    assert_eq!(wrap_index(3, -1, 8), 2);
}

#[test]
fn a_single_entry_list_stays_put_in_both_directions() {
    assert_eq!(wrap_index(0, -1, 1), 0);
    assert_eq!(wrap_index(0, 1, 1), 0);
}

#[test]
fn an_empty_list_pins_at_zero() {
    assert_eq!(wrap_index(0, -1, 0), 0);
    assert_eq!(wrap_index(0, 1, 0), 0);
}
