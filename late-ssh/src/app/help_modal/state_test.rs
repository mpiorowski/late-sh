use super::data::HelpTopic;
use crate::app::help_modal::state::*;
use ratatui::layout::Rect;

#[test]
fn move_topic_wraps_at_both_ends() {
    let mut state = HelpModalState::new();
    state.move_topic(-1);
    assert_eq!(
        state.selected_topic(),
        HelpTopic::ALL[HelpTopic::ALL.len() - 1]
    );

    state.move_topic(1);
    assert_eq!(state.selected_topic(), HelpTopic::Pair);
}

#[test]
fn topic_at_point_hits_set_rect() {
    let state = HelpModalState::new();
    let mut rects = [Rect::new(0, 0, 0, 0); HelpTopic::ALL.len()];
    rects[0] = Rect::new(2, 5, 10, 1);
    rects[1] = Rect::new(13, 5, 6, 1);
    state.set_tab_rects(rects);

    assert_eq!(state.topic_at_point(2, 5), Some(HelpTopic::ALL[0]));
    assert_eq!(state.topic_at_point(14, 5), Some(HelpTopic::ALL[1]));
    assert_eq!(state.topic_at_point(0, 5), None);
    assert_eq!(state.topic_at_point(2, 6), None);
}

#[test]
fn click_topic_detects_double_within_window() {
    let mut state = HelpModalState::new();
    assert!(!state.click_topic(HelpTopic::News));
    assert!(state.click_topic(HelpTopic::News));
    assert!(!state.click_topic(HelpTopic::News));
}
