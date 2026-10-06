//! The fight panel's action list scrolls to follow the cursor once there are
//! more rows (weapons, hypos, stims, shield, meds...) than fit the modal, so
//! a loaded pack never loses its bottom rows off the edge of the screen.

use ratatui::text::Line;

use super::ui_event::{body_height, scroll_offset};

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

/// Scene text is authored in long lines (the intro's three run 71, 70 and 97
/// characters) and rendered wrapped in a card at most 62 columns wide, so
/// the body needs the rows the *wrapped* text takes, not one per line. It
/// used to get one per line and clip the second half of every scene.
#[test]
fn the_body_gets_the_rows_its_wrapped_text_needs() {
    let intro = super::scenes_executioner::by_key("executioner-intro").expect("the intro");
    let start = intro.scene("start").expect("the start scene");
    let body: Vec<Line<'_>> = start.text.iter().map(|t| Line::from(*t)).collect();
    assert_eq!(body.len(), 3);
    assert_eq!(
        body_height(&body, 62, 18),
        6,
        "three lines wrap to two rows each at 62 columns"
    );
    assert_eq!(
        body_height(&body, 200, 18),
        3,
        "and fit one to a row when the card is wide"
    );
    // Short bodies keep their floor, and a long one never starves the actions.
    assert_eq!(body_height(&[Line::from("a")], 62, 18), 3);
    assert_eq!(
        body_height(&body, 20, 18),
        10,
        "capped at half the panel plus the floor"
    );
}
