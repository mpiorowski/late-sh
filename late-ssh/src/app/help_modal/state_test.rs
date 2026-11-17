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

/// A search walks on from where it starts, wraps round every topic, and
/// `Backward` walks the other way; case is ignored.
#[test]
fn find_match_walks_on_from_the_start_and_wraps() {
    let lines_of = |topic: HelpTopic| match topic {
        HelpTopic::Pair => vec!["pair one".to_string(), "Needle in pair".to_string()],
        HelpTopic::Chat => vec!["chat".to_string(), "a needle here".to_string()],
        _ => vec!["nothing".to_string()],
    };
    let pair = HelpTopic::Pair.index();
    let forward = SearchDirection::Forward;
    let backward = SearchDirection::Backward;

    assert_eq!(
        find_match("NEEDLE", (pair, 0), forward, lines_of),
        Some((HelpTopic::Pair, 1))
    );
    assert_eq!(
        find_match("needle", (pair, 2), forward, lines_of),
        Some((HelpTopic::Chat, 1)),
        "past the last match in Pair it goes on to the next topic that has one"
    );
    let chat = HelpTopic::Chat.index();
    assert_eq!(
        find_match("needle", (chat, 2), forward, lines_of),
        Some((HelpTopic::Pair, 1)),
        "past the last topic it wraps to the first"
    );
    assert_eq!(
        find_match("needle", (chat, 1), backward, lines_of),
        Some((HelpTopic::Pair, 1))
    );
    assert_eq!(find_match("absent", (pair, 0), forward, lines_of), None);
}

/// Scrolling stops at the end the renderer measured, and `G`/`g` land on
/// the two ends.
#[test]
fn scrolling_stops_at_the_measured_end() {
    let mut state = HelpModalState::new();
    state.set_max_scroll(7);
    state.scroll(100);
    assert_eq!(state.current_scroll(), 7);
    state.scroll_to_top();
    assert_eq!(state.current_scroll(), 0);
    state.scroll(-3);
    assert_eq!(state.current_scroll(), 0);
    state.scroll_to_bottom();
    assert_eq!(state.current_scroll(), 7);
}

/// `G` holds the end across a measure it was not taken against: pressed
/// before the first frame, or before a resize that shortens the text, it
/// reads as the new end instead of scrolling the body off the page, and one
/// row up from it is one row up from that end.
#[test]
fn the_end_follows_a_later_measure() {
    let mut state = HelpModalState::new();
    state.scroll_to_bottom();
    state.set_max_scroll(7);
    assert_eq!(state.current_scroll(), 7);
    state.set_max_scroll(3);
    assert_eq!(state.current_scroll(), 3);
    state.scroll(-1);
    assert_eq!(state.current_scroll(), 2);
}

/// A scroll key that arrives with a topic switch, before the new topic's
/// first frame, is not held to the height of the topic it left.
#[test]
fn a_new_topic_is_not_held_to_the_last_ones_end() {
    let mut state = HelpModalState::new();
    state.set_max_scroll(3);
    state.move_topic(1);
    state.scroll(10);
    state.set_max_scroll(20);
    assert_eq!(state.current_scroll(), 10);
}
