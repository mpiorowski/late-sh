use ratatui::layout::Rect;

use crate::app::door::hub::state::HubGame;
use crate::app::door::hub::ui::sidebar_hit_test;

/// The full-height sidebar: group headers and blanks are dead cells, game
/// rows map to their index in the roster, and everything right of the
/// sidebar falls through to other handlers.
#[test]
fn clicks_select_games_and_ignore_chrome() {
    // 80x26 body: breathing row at y=0, rows fit without scrolling.
    let body = Rect::new(0, 0, 80, 26);
    // The ` hop hint leads the nav (y=1) and is not selectable, nor is the
    // blank under it (y=2). Then "the house" header (y=3), Lateania (y=4),
    // blank (y=5), "roguelikes" header (y=6), DCSS (y=7), NetHack (y=8),
    // Brogue (y=9), blank (y=10), "remakes" header (y=11), A Dark Room
    // (y=12), blank (y=13), "servers" header (y=14), Minecraft (y=15),
    // blank (y=16), "doors" header (y=17), Green Dragon (y=18).
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 1),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 2),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 3),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 4),
        Some(0)
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 5),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 6),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 7),
        Some(1)
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 9),
        Some(3)
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 11),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 12),
        Some(4)
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 13),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 14),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 15),
        Some(5)
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 17),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 18),
        Some(6)
    );
    // Last games: Rebels at y=22, CodeKeep at y=23.
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 22),
        Some(10)
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 5, 23),
        Some(11)
    );
    // The rule column and the landing pane are not selectable.
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 18, 4),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 0, 40, 4),
        None
    );
    // A viewport under the hub's own too-small guard never hit-tests.
    assert_eq!(
        sidebar_hit_test(Rect::new(0, 0, 50, 24), HubGame::roster(false), 0, 5, 2),
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
        sidebar_hit_test(body, HubGame::roster(false), 11, 5, 1),
        None
    );
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 11, 5, 3),
        Some(6)
    );
    // ...and the selected CodeKeep row is visible at the window's bottom.
    assert_eq!(
        sidebar_hit_test(body, HubGame::roster(false), 11, 5, 8),
        Some(11)
    );
}

/// A runner's sidebar opens the house with Night City: its row sits under
/// the header, and every game below it moves down one row and one index.
#[test]
fn runner_sidebar_puts_night_city_first_in_the_house() {
    let body = Rect::new(0, 0, 80, 26);
    let runner = HubGame::roster(true);
    // "the house" header (y=3), Night City (y=4), Lateania (y=5), blank
    // (y=6), "roguelikes" (y=7), DCSS (y=8).
    assert_eq!(sidebar_hit_test(body, runner, 0, 5, 3), None);
    assert_eq!(sidebar_hit_test(body, runner, 0, 5, 4), Some(0));
    assert_eq!(runner[0], HubGame::NightCity);
    assert_eq!(sidebar_hit_test(body, runner, 0, 5, 5), Some(1));
    assert_eq!(sidebar_hit_test(body, runner, 0, 5, 6), None);
    assert_eq!(sidebar_hit_test(body, runner, 0, 5, 8), Some(2));
    assert_eq!(runner[2], HubGame::Dcss);
}
