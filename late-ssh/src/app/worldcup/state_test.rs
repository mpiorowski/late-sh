use crate::app::worldcup::state::*;

#[test]
fn toggle_alternates_views() {
    let mut s = State::default();
    assert_eq!(s.view, View::Overview);
    s.toggle_view();
    assert_eq!(s.view, View::Bracket);
    s.toggle_view();
    assert_eq!(s.view, View::Overview);
}

#[test]
fn scroll_is_per_view_and_clamps_at_zero() {
    let mut s = State::default();
    s.scroll_down();
    s.scroll_down();
    assert_eq!(s.overview_scroll, 2);
    assert_eq!(s.bracket_scroll, 0);

    // The bracket keeps its own offset.
    s.toggle_view();
    s.scroll_down();
    assert_eq!(s.bracket_scroll, 1);
    assert_eq!(s.overview_scroll, 2);

    // Can't scroll above the top.
    s.scroll_up();
    s.scroll_up();
    assert_eq!(s.bracket_scroll, 0);
}

#[test]
fn signed_scroll_matches_pageup_convention() {
    let mut s = State::default();
    s.scroll(-3); // down
    assert_eq!(s.overview_scroll, 3);
    s.scroll(2); // up
    assert_eq!(s.overview_scroll, 1);
}
