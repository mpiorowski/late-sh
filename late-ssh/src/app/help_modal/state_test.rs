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

/// Each topic keeps its own place, and a resize drops what every topic
/// measured: nothing scrolls against a body that is no longer on screen.
#[test]
fn each_topic_keeps_its_place_until_a_resize_forgets_the_measure() {
    use crate::app::common::scroll_view::ScrollViewport;
    let viewport = ScrollViewport {
        body: Rect::new(0, 0, 40, 10),
        content_height: 17,
        ..ScrollViewport::default()
    };
    let mut state = HelpModalState::new();
    state.view().set_viewport(viewport);
    state.view().scroll(100);
    assert_eq!(state.view().offset(), 7, "held to the end of the text");
    assert_eq!(state.half_page(), 5);

    state.move_topic(1);
    assert_eq!(state.view().offset(), 0, "the next topic starts at its top");
    state.move_topic(-1);
    assert_eq!(state.view().offset(), 7);

    state.invalidate_views();
    state.view().scroll(-3);
    assert_eq!(state.view().offset(), 7, "unmeasured until the next draw");
    assert!(!state.body_contains(1, 1));
}
