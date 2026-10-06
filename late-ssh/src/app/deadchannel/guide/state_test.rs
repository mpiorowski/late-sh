use super::State;

#[test]
fn the_guide_opens_at_the_top_and_scrolls_within_the_measured_page() {
    let mut state = State::new();
    assert!(!state.is_open());

    state.open();
    assert!(state.is_open());
    assert_eq!(state.scroll(), 0);

    // Nothing measured yet: a scroll goes nowhere rather than off the page.
    state.scroll_by(3);
    assert_eq!(state.scroll(), 0);

    // Forty lines in a ten-row body: the end is thirty.
    state.record_page(40, 10);
    state.scroll_by(3);
    assert_eq!(state.scroll(), 3);
    state.scroll_by(100);
    assert_eq!(state.scroll(), 30);
    state.scroll_by(-1);
    assert_eq!(state.scroll(), 29);
    state.scroll_by(-100);
    assert_eq!(state.scroll(), 0);

    // A taller frame shortens the page and the next key honors it.
    state.scroll_by(30);
    state.record_page(40, 35);
    state.scroll_by(1);
    assert_eq!(state.scroll(), 5);

    // Reopening starts from the top again.
    state.close();
    assert!(!state.is_open());
    state.open();
    assert_eq!(state.scroll(), 0);
}
