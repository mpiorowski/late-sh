use ratatui::layout::Rect;
use crate::app::hub::state::*;

#[test]
fn tab_at_point_hits_set_rect() {
    let state = HubState::new();
    let mut rects = [Rect::new(0, 0, 0, 0); HubTab::ALL.len()];
    rects[0] = Rect::new(2, 5, 8, 1); // Dailies
    rects[1] = Rect::new(11, 5, 14, 1); // Shop
    state.set_tab_rects(rects);

    assert_eq!(state.tab_at_point(2, 5), Some(HubTab::Dailies));
    assert_eq!(state.tab_at_point(9, 5), Some(HubTab::Dailies));
    assert_eq!(state.tab_at_point(12, 5), Some(HubTab::Shop));
    assert_eq!(state.tab_at_point(0, 5), None);
    assert_eq!(state.tab_at_point(2, 6), None);
}

#[test]
fn click_tab_detects_double_within_window() {
    let mut state = HubState::new();
    assert!(!state.click_tab(HubTab::Leaderboard));
    // Second click on the same tab within the window — double.
    assert!(state.click_tab(HubTab::Leaderboard));
    // After a double, the chain resets — next click is single again.
    assert!(!state.click_tab(HubTab::Leaderboard));
}

#[test]
fn click_tab_different_tab_resets_chain() {
    let mut state = HubState::new();
    state.click_tab(HubTab::Shop);
    assert!(!state.click_tab(HubTab::Events));
    assert_eq!(state.selected_tab(), HubTab::Events);
}
