use crate::app::door::hub::state::*;

#[test]
fn selection_clamps_at_both_ends() {
    let mut s = State::default();
    assert_eq!(s.selected_game(), HubGame::Lateania);
    s.select_prev();
    assert_eq!(s.selected_game(), HubGame::Lateania);
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
    s.select(5);
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
