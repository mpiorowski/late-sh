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
    // Minecraft (y=5), blank (y=6), "roguelikes" header (y=7), DCSS (y=8),
    // NetHack (y=9), Brogue (y=10), blank (y=11), "remakes" header (y=12),
    // A Dark Room (y=13), Green Dragon (y=14).
    assert_eq!(sidebar_hit_test(body, 0, 5, 1), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 2), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 3), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 4), Some(0));
    assert_eq!(sidebar_hit_test(body, 0, 5, 5), Some(1));
    assert_eq!(sidebar_hit_test(body, 0, 5, 6), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 7), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 8), Some(2));
    assert_eq!(sidebar_hit_test(body, 0, 5, 10), Some(4));
    assert_eq!(sidebar_hit_test(body, 0, 5, 12), None);
    assert_eq!(sidebar_hit_test(body, 0, 5, 13), Some(5));
    assert_eq!(sidebar_hit_test(body, 0, 5, 14), Some(6));
    // Last games: Rebels at y=20, CodeKeep at y=21.
    assert_eq!(sidebar_hit_test(body, 0, 5, 20), Some(10));
    assert_eq!(sidebar_hit_test(body, 0, 5, 21), Some(11));
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
    // Height 10: 8 sidebar rows visible of 21, selection on the last game
    // slides the window to the bottom of the list.
    let body = Rect::new(0, 0, 80, 10);
    // Window starts at row 13 (Green Dragon), so y=1 lands on it...
    assert_eq!(sidebar_hit_test(body, 11, 5, 1), Some(6));
    // ...and the selected CodeKeep row is visible at the window's bottom.
    assert_eq!(sidebar_hit_test(body, 11, 5, 8), Some(11));
}
