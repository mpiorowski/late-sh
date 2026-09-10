use ratatui::layout::Rect;

use super::*;
use crate::app::zen::layout::tile_rects;

fn widths(root: &Node, area: Rect, gap: u16) -> Vec<u16> {
    tile_rects(root, area, gap, None)
        .into_iter()
        .map(|(_, rect)| rect.width)
        .collect()
}

fn heights(root: &Node, area: Rect, gap: u16) -> Vec<u16> {
    tile_rects(root, area, gap, None)
        .into_iter()
        .map(|(_, rect)| rect.height)
        .collect()
}

#[test]
fn one_press_moves_a_row_split_by_exactly_one_column_at_every_width() {
    for width in 40..=320u16 {
        for gap in 0..=1u16 {
            let area = Rect::new(0, 0, width, 30);
            let mut root = Node::split(
                Dir::Row,
                640,
                Node::leaf(TileKind::Bonsai),
                Node::leaf(TileKind::Chat),
            );
            let before = widths(&root, area, gap);
            assert!(root.resize_leaf(0, Dir::Row, 1, area, gap));
            let grown = widths(&root, area, gap);
            assert_eq!(
                grown[0],
                before[0] + 1,
                "width {width} gap {gap}: the first tile grows one column"
            );
            assert_eq!(grown[1], before[1] - 1, "and the second gives it up");

            // Shrinking from the second tile's side lands back where it was.
            assert!(root.resize_leaf(1, Dir::Row, 1, area, gap));
            assert_eq!(widths(&root, area, gap), before);
        }
    }
}

#[test]
fn a_deep_column_split_moves_one_row_and_a_row_only_tree_has_no_height() {
    let area = Rect::new(0, 0, 160, 44);
    let mut root = RiceLayout::default().root;
    // Leaves run bonsai, chat, clock, music, lobby, pet, aquarium. Music sits in
    // a column inside a column inside the rail; only the split directly
    // above it moves, so the clock above and the left column stay put.
    let before = heights(&root, area, 1);
    assert!(root.resize_leaf(3, Dir::Column, 1, area, 1));
    let after = heights(&root, area, 1);
    assert_eq!(after[3], before[3] + 1, "music gains one row");
    assert_eq!(after[0], before[0], "the bonsai column is untouched");
    assert_eq!(after[1], before[1]);
    assert_eq!(after[2], before[2], "the clock is untouched");
    assert_eq!(
        after[4] + after[5] + after[6] + 1,
        before[4] + before[5] + before[6],
        "lobby, pet, and the reef give up one row between them"
    );

    let mut row_only = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Bonsai),
        Node::leaf(TileKind::Chat),
    );
    assert!(
        !row_only.resize_leaf(0, Dir::Column, 1, area, 1),
        "no column split above the tile: nothing to trade height with"
    );
}

#[test]
fn shares_stay_inside_their_bounds() {
    let area = Rect::new(0, 0, 100, 30);
    let mut root = Node::split(
        Dir::Row,
        MAX_SHARE,
        Node::leaf(TileKind::Bonsai),
        Node::leaf(TileKind::Chat),
    );
    let before = widths(&root, area, 0);
    assert!(root.resize_leaf(0, Dir::Row, 1, area, 0));
    assert_eq!(widths(&root, area, 0), before, "already at the widest share");
    let Node::Split { share, .. } = root else {
        unreachable!()
    };
    assert_eq!(share, MAX_SHARE);
}
