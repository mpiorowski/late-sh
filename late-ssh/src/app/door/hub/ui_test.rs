use ratatui::layout::Rect;

use crate::app::door::hub::ui::sidebar_hit_test;

/// The full-height sidebar: group headers and blanks are dead cells, game
/// rows map to their `HubGame::ALL` index, and everything right of the
/// sidebar falls through to other handlers.
#[test]
fn clicks_select_games_and_ignore_chrome() {
    // 80x24 body: breathing row at y=0, rows fit without scrolling.
    let body = Rect::new(0, 0, 80, 24);
    // The ` hop hint leads the nav (y=1) and is not selectable, nor is the
    // blank under it (y=2). Then "the house" header (y=3), Lateania (y=4),
    // blank (y=5), "roguelikes" header (y=6), DCSS (y=7), NetHack (y=8),
    // Brogue (y=9), blank (y=10), "remakes" header (y=11), A Dark Room
    // (y=12), Green Dragon (y=13).
    assert_eq!(sidebar_hit_test(body, 0, 5, 1), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 2), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 3), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 4), Some(0));
    assert_eq!(sidebar_hit_test(body, 0, 5, 5), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 6), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 7), Some(1));
    assert_eq!(sidebar_hit_test(body, 0, 5, 9), Some(3));
    assert_eq!(sidebar_hit_test(body, 0, 5, 11), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 12), Some(4));
    assert_eq!(sidebar_hit_test(body, 0, 5, 13), Some(5));
    // Last games: Rebels at y=19, CodeKeep at y=20.
    assert_eq!(sidebar_hit_test(body, 0, 5, 19), Some(9));
    assert_eq!(sidebar_hit_test(body, 0, 5, 20), Some(10));
    // The rule column and the landing pane are not selectable.
    assert_eq!(sidebar_hit_test(body, 0, 18, 5), None);
    assert_eq!(sidebar_hit_test(body, 0, 40, 5), None);
    // A viewport under the hub's own too-small guard never hit-tests.
    assert_eq!(sidebar_hit_test(Rect::new(0, 0, 50, 24), 0, 5, 2), None);
}

/// A short viewport scrolls the sidebar to keep the selection visible, and
/// the hit test follows the same window.
#[test]
fn hit_test_follows_the_scroll_window() {
    // Height 10: 8 sidebar rows visible of 20, selection on the last game
    // slides the window to the bottom of the list.
    let body = Rect::new(0, 0, 80, 10);
    // Window starts at row 12 (Green Dragon), so y=1 lands on it...
    assert_eq!(sidebar_hit_test(body, 10, 5, 1), Some(5));
    // ...and the selected CodeKeep row is visible at the window's bottom.
    assert_eq!(sidebar_hit_test(body, 10, 5, 8), Some(10));
}
