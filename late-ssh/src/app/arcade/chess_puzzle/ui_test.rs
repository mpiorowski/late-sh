use super::*;

/// A wide terminal puts the panel beside the board without overlap; a
/// narrow one drops the panel and keeps the board inside the area.
#[test]
fn the_panel_sits_beside_the_board_only_when_it_fits() {
    let wide = layout(Rect::new(0, 0, 120, 36));
    let panel = wide.panel.expect("a panel on a wide terminal");
    assert!(panel.x >= wide.board.right());
    assert!(panel.right() <= 120);

    let narrow = layout(Rect::new(0, 0, 40, 20));
    assert!(narrow.panel.is_none());
    assert!(narrow.board.right() <= 40 && narrow.board.bottom() <= 20);
}

/// A click lands on the square the frame drew, from the player's side:
/// the bottom-left square is a1 for White and h8 for Black.
#[test]
fn a_click_lands_on_the_square_drawn_for_either_side() {
    for area in [Rect::new(0, 0, 120, 40), Rect::new(0, 0, 60, 24)] {
        let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
        let Layout { board, tier, .. } = layout(board_area);
        let x = board.x + tier.gutter as u16 + tier.cw as u16 / 2;
        let y = board.y + 1 + 7 * tier.ch as u16 + tier.ch as u16 / 2;
        assert_eq!(hit_test(area, ChessColor::White, x, y), Some(0), "{area:?}");
        assert_eq!(
            hit_test(area, ChessColor::Black, x, y),
            Some(63),
            "{area:?}"
        );
        assert_eq!(hit_test(area, ChessColor::White, board.x, board.y), None);
    }
}
