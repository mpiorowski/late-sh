use ratatui::layout::Rect;

use crate::app::door::hub::state::{HubGame, RailEntry};
use crate::app::door::hub::ui::{sidebar_hit_test, watch_pane_area};

/// The full-height sidebar: group headers and blanks are dead cells, game
/// rows map to their index in the roster, and everything right of the
/// sidebar falls through to other handlers.
#[test]
fn clicks_select_games_and_ignore_chrome() {
    // 80x30 body: breathing row at y=0, the cards fit without scrolling and
    // the live section sits under them.
    let body = Rect::new(0, 0, 80, 30);
    // The ` hop hint leads the nav (y=1) and is not selectable, nor is the
    // blank under it (y=2). Then "the house" header (y=3), Lateania (y=4),
    // blank (y=5), "roguelikes" header (y=6), DCSS (y=7), NetHack (y=8),
    // Brogue (y=9), blank (y=10), "remakes" header (y=11), A Dark Room
    // (y=12), blank (y=13), "servers" header (y=14), Minecraft (y=15),
    // blank (y=16), "doors" header (y=17), Green Dragon (y=18).
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 1),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 2),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 3),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 4),
        Some(RailEntry::Card(0))
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 5),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 6),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 7),
        Some(RailEntry::Card(1))
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 9),
        Some(RailEntry::Card(3))
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 11),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 12),
        Some(RailEntry::Card(4))
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 13),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 14),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 15),
        Some(RailEntry::Card(5))
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 17),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 18),
        Some(RailEntry::Card(6))
    );
    // Last games: Rebels at y=22, CodeKeep at y=23.
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 22),
        Some(RailEntry::Card(10))
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 5, 23),
        Some(RailEntry::Card(11))
    );
    // The rule column and the landing pane are not selectable.
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 18, 4),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 0, None, 40, 4),
        None
    );
    // A viewport under the hub's own too-small guard never hit-tests.
    assert_eq!(
        sidebar_hit_test(
            Rect::new(0, 0, 50, 24),
            HubGame::roster(false),
            0,
            0,
            None,
            5,
            2
        ),
        None
    );
}

/// A short viewport scrolls the sidebar to keep the selection visible, and
/// the hit test follows the same window.
#[test]
fn hit_test_follows_the_scroll_window() {
    // Height 10: 8 sidebar rows visible of 23, selection on the last game
    // slides the window to the bottom of the list.
    let body = Rect::new(0, 0, 80, 10);
    // Window starts at row 15 (the blank above "doors"), so y=1 is dead
    // and y=3 lands on Green Dragon...
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 11, 0, None, 5, 1),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 11, 0, None, 5, 3),
        Some(RailEntry::Card(6))
    );
    // ...and the selected CodeKeep row is visible at the window's bottom.
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 11, 0, None, 5, 8),
        Some(RailEntry::Card(11))
    );
}

/// A runner's sidebar opens the house with Night City: its row sits under
/// the header, and every game below it moves down one row and one index.
#[test]
fn runner_sidebar_puts_night_city_first_in_the_house() {
    let body = Rect::new(0, 0, 80, 30);
    let runner = HubGame::roster(true);
    // "the house" header (y=3), Night City (y=4), Lateania (y=5), blank
    // (y=6), "roguelikes" (y=7), DCSS (y=8).
    assert_eq!(sidebar_hit_test(body, runner, 0, 0, None, 5, 3), None);
    assert_eq!(
        sidebar_hit_test(body, runner, 0, 0, None, 5, 4),
        Some(RailEntry::Card(0))
    );
    assert_eq!(runner[0], HubGame::NightCity);
    assert_eq!(
        sidebar_hit_test(body, runner, 0, 0, None, 5, 5),
        Some(RailEntry::Card(1))
    );
    assert_eq!(sidebar_hit_test(body, runner, 0, 0, None, 5, 6), None);
    assert_eq!(
        sidebar_hit_test(body, runner, 0, 0, None, 5, 8),
        Some(RailEntry::Card(2))
    );
    assert_eq!(runner[2], HubGame::Dcss);
}

/// The live section sits straight under the cards: a gap, the rule and the
/// header are dead cells, and each live game answers on both of its rows.
#[test]
fn clicks_select_live_games_under_the_cards() {
    let body = Rect::new(0, 0, 80, 34);
    let public = HubGame::roster(false);
    // The cards end with CodeKeep at y=23. Then the gap (y=24), the rule
    // (y=25), the header (y=26), the first game's name (y=27) and status
    // (y=28), the second game's (y=29, y=30).
    assert_eq!(
        sidebar_hit_test(body, public, 0, 2, None, 5, 23),
        Some(RailEntry::Card(11))
    );
    for dead in [24, 25, 26] {
        assert_eq!(sidebar_hit_test(body, public, 0, 2, None, 5, dead), None);
    }
    assert_eq!(
        sidebar_hit_test(body, public, 0, 2, None, 5, 27),
        Some(RailEntry::Live(0))
    );
    assert_eq!(
        sidebar_hit_test(body, public, 0, 2, None, 5, 28),
        Some(RailEntry::Live(0))
    );
    assert_eq!(
        sidebar_hit_test(body, public, 0, 2, Some(1), 5, 30),
        Some(RailEntry::Live(1))
    );
    // With nobody playing the section is its header and one dead line.
    for dead in [24, 25, 26, 27] {
        assert_eq!(sidebar_hit_test(body, public, 0, 0, None, 5, dead), None);
    }
}

/// On a rail too short for both, the live section is pinned to the bottom
/// and the cards scroll in what is left above it.
#[test]
fn a_short_rail_pins_the_live_section_under_scrolling_cards() {
    // Height 16: a 14-row rail. One live game takes five rows (gap, rule,
    // header, name, status), leaving nine for the 23 card rows.
    let body = Rect::new(0, 0, 80, 16);
    let public = HubGame::roster(false);
    // The cards window starts at the top with the first card selected:
    // Lateania at y=4, and the ninth card row (Brogue, y=9) is the last;
    // the gap, the rule and the header follow it.
    assert_eq!(
        sidebar_hit_test(body, public, 0, 1, None, 5, 4),
        Some(RailEntry::Card(0))
    );
    assert_eq!(
        sidebar_hit_test(body, public, 0, 1, None, 5, 9),
        Some(RailEntry::Card(3))
    );
    for dead in [10, 11, 12] {
        assert_eq!(sidebar_hit_test(body, public, 0, 1, None, 5, dead), None);
    }
    // The live game's two rows sit at the very bottom of the rail.
    assert_eq!(
        sidebar_hit_test(body, public, 0, 1, None, 5, 13),
        Some(RailEntry::Live(0))
    );
    assert_eq!(
        sidebar_hit_test(body, public, 0, 1, None, 5, 14),
        Some(RailEntry::Live(0))
    );
}

/// More live games than half the rail holds: a window that keeps the watched
/// one in view, and the count of the rest is not a click target.
#[test]
fn a_crowded_live_section_follows_the_watched_game() {
    // Height 22: a 20-row rail gives the section ten rows: gap, rule and
    // header, then three games on two rows each and the "more" line.
    let body = Rect::new(0, 0, 80, 22);
    let public = HubGame::roster(false);
    // Nothing watched: the first three of nine. The section fills the last
    // ten rail rows, y=11..=20; names at y=14, 16, 18, "more" at y=20.
    assert_eq!(
        sidebar_hit_test(body, public, 0, 9, None, 5, 14),
        Some(RailEntry::Live(0))
    );
    assert_eq!(
        sidebar_hit_test(body, public, 0, 9, None, 5, 18),
        Some(RailEntry::Live(2))
    );
    assert_eq!(sidebar_hit_test(body, public, 0, 9, None, 5, 20), None);
    // Watching the eighth: the window slides so it is the last one shown.
    assert_eq!(
        sidebar_hit_test(body, public, 0, 9, Some(7), 5, 18),
        Some(RailEntry::Live(7))
    );
    assert_eq!(
        sidebar_hit_test(body, public, 0, 9, Some(7), 5, 14),
        Some(RailEntry::Live(5))
    );
}

/// The watch draws beside the rail, from the row under the frame's top
/// border down to the footer.
#[test]
fn the_watch_pane_is_the_landings_place_plus_its_breathing_row() {
    assert_eq!(
        watch_pane_area(Rect::new(1, 1, 150, 40)),
        Some(Rect::new(20, 1, 131, 39))
    );
    assert_eq!(watch_pane_area(Rect::new(1, 1, 50, 40)), None);
}
