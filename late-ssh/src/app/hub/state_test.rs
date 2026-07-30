use crate::app::hub::state::*;
use ratatui::layout::Rect;

#[test]
fn tab_at_point_hits_set_rect() {
    let state = HubState::new();
    let mut rects = [Rect::new(0, 0, 0, 0); HubTab::ALL.len()];
    rects[0] = Rect::new(2, 5, 8, 1); // Shop
    rects[1] = Rect::new(11, 5, 14, 1); // Admin
    state.set_tab_rects(rects);

    assert_eq!(state.tab_at_point(2, 5), Some(HubTab::Shop));
    assert_eq!(state.tab_at_point(9, 5), Some(HubTab::Shop));
    assert_eq!(state.tab_at_point(12, 5), Some(HubTab::Admin));
    assert_eq!(state.tab_at_point(0, 5), None);
    assert_eq!(state.tab_at_point(2, 6), None);
}

#[test]
fn click_tab_detects_double_within_window() {
    let mut state = HubState::new();
    assert!(!state.click_tab(HubTab::Shop));
    // Second click on the same tab within the window counts as a double.
    assert!(state.click_tab(HubTab::Shop));
    // After a double the chain resets: the next click is single again.
    assert!(!state.click_tab(HubTab::Shop));
}

#[test]
fn click_tab_different_tab_resets_chain() {
    let mut state = HubState::new();
    state.click_tab(HubTab::Shop);
    assert!(!state.click_tab(HubTab::Admin));
    assert_eq!(state.selected_tab(), HubTab::Admin);
}

/// A non-admin can never sit on the Admin tab: the visibility guard snaps the
/// selection back to Shop, the one public tab.
#[test]
fn ensure_visible_tab_snaps_non_admin_off_admin() {
    let mut state = HubState::new();
    state.open(HubTab::Admin);
    state.ensure_visible_tab(false);
    assert_eq!(state.selected_tab(), HubTab::Shop);

    state.open(HubTab::Admin);
    state.ensure_visible_tab(true);
    assert_eq!(state.selected_tab(), HubTab::Admin);
}
