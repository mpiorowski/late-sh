use ratatui::layout::Rect;

use super::{neighbour_side, tile_rects};
use crate::app::pet::ui::WatchSide;
use crate::app::zen::state::{Dir, Node, TileKind};

#[test]
fn a_tank_tile_touching_the_pet_tile_is_a_neighbour_on_that_side() {
    let area = Rect::new(0, 0, 100, 40);
    let side_by_side = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Pet),
        Node::leaf(TileKind::Aquarium),
    );
    let rects = tile_rects(&side_by_side, area, 1, None);
    assert_eq!(
        neighbour_side(rects[0].1, rects[1].1, 1),
        Some(WatchSide::Right)
    );
    assert_eq!(
        neighbour_side(rects[1].1, rects[0].1, 1),
        Some(WatchSide::Left),
        "the same edge seen from the tank"
    );

    let stacked = Node::split(
        Dir::Column,
        500,
        Node::leaf(TileKind::Aquarium),
        Node::leaf(TileKind::Pet),
    );
    let rects = tile_rects(&stacked, area, 0, None);
    assert_eq!(
        neighbour_side(rects[1].1, rects[0].1, 0),
        Some(WatchSide::Above)
    );
}

#[test]
fn a_tile_across_the_page_or_on_a_diagonal_is_not_a_neighbour() {
    // pet | clock | tank: the clock sits between them.
    let row = Node::split(
        Dir::Row,
        333,
        Node::leaf(TileKind::Pet),
        Node::split(
            Dir::Row,
            500,
            Node::leaf(TileKind::Clock),
            Node::leaf(TileKind::Aquarium),
        ),
    );
    let rects = tile_rects(&row, Rect::new(0, 0, 90, 30), 1, None);
    assert_eq!(neighbour_side(rects[0].1, rects[2].1, 1), None);

    // Corner to corner only: a shared point is not a shared edge.
    let pet = Rect::new(0, 0, 10, 10);
    let tank = Rect::new(10, 10, 10, 10);
    assert_eq!(neighbour_side(pet, tank, 0), None);
}
