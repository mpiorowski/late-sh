//! The fight panel's action list scrolls to follow the cursor once there are
//! more rows (weapons, hypos, stims, shield, meds...) than fit the modal, so
//! a loaded pack never loses its bottom rows off the edge of the screen.

use super::ui_event::scroll_offset;

/// Everything fits: no scrolling, regardless of where the cursor sits.
#[test]
fn no_scroll_when_everything_fits() {
    assert_eq!(scroll_offset(0, 5, 8), 0);
    assert_eq!(scroll_offset(4, 5, 8), 0);
}

/// The cursor is still inside the first page: no need to scroll yet.
#[test]
fn stays_put_while_cursor_is_on_screen() {
    assert_eq!(scroll_offset(0, 20, 6), 0);
    assert_eq!(scroll_offset(5, 20, 6), 0);
}

/// Moving past the visible window scrolls exactly far enough to keep the
/// cursor as the last visible row, not further.
#[test]
fn follows_the_cursor_past_the_first_page() {
    assert_eq!(scroll_offset(6, 20, 6), 1);
    assert_eq!(scroll_offset(10, 20, 6), 5);
}

/// The list never scrolls past its own end, even with the cursor on the
/// last row.
#[test]
fn clamps_to_the_end_of_the_list() {
    assert_eq!(scroll_offset(19, 20, 6), 14);
}
