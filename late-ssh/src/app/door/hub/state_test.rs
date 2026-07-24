use crate::app::door::hub::state::*;

#[test]
fn selection_clamps_at_both_ends() {
    let mut s = State::default();
    assert_eq!(s.selected_game(), HubGame::Lateania);
    s.select_prev();
    assert_eq!(s.selected_game(), HubGame::Lateania);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Nethack);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Dcss);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Brogue);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Usurper);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::GreenDragon);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Rebels);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Dopewars);
    s.select_next();
    assert_eq!(s.selected_game(), HubGame::Dopewars);
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
            "NetHack",
            "DCSS",
            "Brogue",
            "Usurper",
            "Green Dragon",
            "Rebels",
            "dopewars"
        ],
    );
}
