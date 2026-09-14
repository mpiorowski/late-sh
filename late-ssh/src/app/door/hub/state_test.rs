use crate::app::door::hub::state::*;

#[test]
fn selection_clamps_at_both_ends() {
    let mut s = State::default();
    assert_eq!(s.selected_game(), HubGame::Lateania);
    s.select_prev();
    assert_eq!(s.selected_game(), HubGame::Lateania);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Minecraft);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Dcss);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Nethack);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Brogue);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Darkroom);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::GreenDragon);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Usurper);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Dopewars);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Bashquest);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Rebels);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Codekeep);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Codekeep);
}

#[test]
fn select_jumps_directly() {
    let mut s = State::default();
    s.select(6);
    assert_eq!(s.selected_game(), HubGame::GreenDragon);
    s.select(99);
    assert_eq!(s.selected_game(), HubGame::GreenDragon);
}

#[test]
fn all_games_are_listed_in_order() {
    assert_eq!(
        HubGame::ALL.map(HubGame::label),
        [
            "Lateania",
            "Minecraft",
            "DCSS",
            "NetHack",
            "Brogue",
            "A Dark Room",
            "Green Dragon",
            "Usurper",
            "dopewars",
            "BashQuest",
            "Rebels",
            "CodeKeep"
        ],
    );
}

/// The sidebar renders one header per group, so a group's games must sit
/// adjacent in `ALL`; an interleaved insertion would repeat its header.
#[test]
fn groups_are_contiguous_in_selector_order() {
    let groups: Vec<HubGroup> = HubGame::ALL.iter().map(|g| g.group()).collect();
    let mut seen: Vec<HubGroup> = Vec::new();
    for group in groups {
        match seen.last() {
            Some(last) if *last == group => {}
            _ => {
                assert!(
                    !seen.contains(&group),
                    "group {group:?} appears in two separate runs of HubGame::ALL"
                );
                seen.push(group);
            }
        }
    }
}

/// A landing scrolls only as far as the renderer last measured, and a newly
/// selected game starts at its top.
#[test]
fn scroll_clamps_to_the_measured_range_and_resets_on_switch() {
    let mut s = State::default();
    s.scroll_down();
    assert_eq!(s.scroll(), 0, "nothing measured yet, nothing to scroll");
    s.max_scroll().set(2);
    s.scroll_down();
    s.scroll_down();
    s.scroll_down();
    assert_eq!(s.scroll(), 2);
    s.scroll_up();
    assert_eq!(s.scroll(), 1);
    s.select(0);
    assert_eq!(s.scroll(), 1, "re-selecting the same game keeps the place");
    s.select_next();
    assert_eq!(s.scroll(), 0);
}

/// After a resize shortens the landing's range, the first press up moves the
/// page instead of spending presses on rows that no longer exist.
#[test]
fn scroll_up_after_the_range_shrinks_moves_at_once() {
    let mut s = State::default();
    s.max_scroll().set(10);
    for _ in 0..10 {
        s.scroll_down();
    }
    s.max_scroll().set(3);
    s.scroll_up();
    assert_eq!(s.scroll(), 2);
}
