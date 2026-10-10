use crate::app::door::hub::state::*;

fn public() -> &'static [HubGame] {
    HubGame::roster(false)
}

fn runner() -> &'static [HubGame] {
    HubGame::roster(true)
}

/// The rail is one loop: the cards, then the live rows, and back round.
#[test]
fn the_rail_wraps_through_the_cards_and_the_live_rows() {
    // Three cards, two live rows.
    let walk = |from: RailEntry, forward: bool| rail_step(3, 2, from, forward);
    assert_eq!(walk(RailEntry::Card(0), true), RailEntry::Card(1));
    assert_eq!(walk(RailEntry::Card(2), true), RailEntry::Live(0));
    assert_eq!(walk(RailEntry::Live(0), true), RailEntry::Live(1));
    assert_eq!(walk(RailEntry::Live(1), true), RailEntry::Card(0));

    // Up from the first card is the shortcut to the last live game.
    assert_eq!(walk(RailEntry::Card(0), false), RailEntry::Live(1));
    assert_eq!(walk(RailEntry::Live(0), false), RailEntry::Card(2));
}

/// With nobody playing the cards wrap among themselves.
#[test]
fn the_rail_wraps_the_cards_alone_when_nobody_is_live() {
    assert_eq!(
        rail_step(3, 0, RailEntry::Card(2), true),
        RailEntry::Card(0)
    );
    assert_eq!(
        rail_step(3, 0, RailEntry::Card(0), false),
        RailEntry::Card(2)
    );
}

#[test]
fn select_jumps_directly() {
    let mut s = State::default();
    s.select(public(), 6);
    assert_eq!(s.selected_game(public()), HubGame::GreenDragon);
    s.select(public(), 99);
    assert_eq!(s.selected_game(public()), HubGame::GreenDragon);
}

#[test]
fn all_games_are_listed_in_order() {
    assert_eq!(
        HubGame::ALL.map(HubGame::label),
        [
            "Night City",
            "Lateania",
            "DCSS",
            "NetHack",
            "Brogue",
            "A Dark Room",
            "Minecraft",
            "Green Dragon",
            "Usurper",
            "dopewars",
            "BashQuest",
            "Rebels",
            "CodeKeep",
            "Zork Trilogy"
        ],
    );
}

/// Night City is a runner's card: first in the house for them, nowhere for
/// anyone else.
#[test]
fn night_city_heads_the_runner_roster_only() {
    assert_eq!(runner()[0], HubGame::NightCity);
    assert_eq!(&runner()[1..], public());
    assert!(!public().contains(&HubGame::NightCity));
    assert_eq!(HubGame::NightCity.group(), HubGroup::House);
    assert_eq!(State::default().selected_game(runner()), HubGame::NightCity);
}

/// The selection is a game, not a row: joining #deadchannel puts Night
/// City above it without moving it, and leaving with Night City selected
/// falls back to the top of the roster that is left.
#[test]
fn selection_survives_the_roster_changing() {
    let mut s = State::default();
    s.select_game(runner(), HubGame::Nethack);
    assert_eq!(s.selected_game(public()), HubGame::Nethack);
    assert_eq!(s.selected_game(runner()), HubGame::Nethack);

    let mut s = State::default();
    s.select_game(runner(), HubGame::NightCity);
    assert_eq!(s.selected_game(runner()), HubGame::NightCity);
    assert_eq!(s.selected_game(public()), HubGame::Lateania);
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
    s.select(public(), 0);
    assert_eq!(s.scroll(), 1, "re-selecting the same game keeps the place");
    s.select(public(), 1);
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
