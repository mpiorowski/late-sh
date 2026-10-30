use ratatui::layout::Rect;

use super::{Chrome, Gaps, neighbour_side, pet_neighbours, row_gutters, tile_inner, tile_rects};
use crate::app::pet::ui::{Neighbours, WatchSide};
use crate::app::zen::state::{Dir, Node, RiceLayout, TileKind};

#[test]
fn a_tank_tile_touching_the_pet_tile_is_a_neighbour_on_that_side() {
    let area = Rect::new(0, 0, 100, 40);
    let side_by_side = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Pet),
        Node::leaf(TileKind::Aquarium),
    );
    let rects = tile_rects(&side_by_side, area, Gaps::uniform(1), None);
    assert_eq!(
        neighbour_side(rects[0].1, rects[1].1, Gaps::uniform(1)),
        Some(WatchSide::Right)
    );
    assert_eq!(
        neighbour_side(rects[1].1, rects[0].1, Gaps::uniform(1)),
        Some(WatchSide::Left),
        "the same edge seen from the tank"
    );

    let stacked = Node::split(
        Dir::Column,
        500,
        Node::leaf(TileKind::Aquarium),
        Node::leaf(TileKind::Pet),
    );
    let rects = tile_rects(&stacked, area, Gaps::uniform(0), None);
    assert_eq!(
        neighbour_side(rects[1].1, rects[0].1, Gaps::uniform(0)),
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
    let rects = tile_rects(&row, Rect::new(0, 0, 90, 30), Gaps::uniform(1), None);
    assert_eq!(
        neighbour_side(rects[0].1, rects[2].1, Gaps::uniform(1)),
        None
    );

    // Corner to corner only: a shared point is not a shared edge.
    let pet = Rect::new(0, 0, 10, 10);
    let tank = Rect::new(10, 10, 10, 10);
    assert_eq!(neighbour_side(pet, tank, Gaps::uniform(0)), None);
}

#[test]
fn the_default_page_puts_the_tank_to_the_pets_left_and_the_bonsai_out_of_reach() {
    let rice = RiceLayout::default();
    for area in [Rect::new(0, 0, 200, 50), Rect::new(0, 0, 120, 30)] {
        let rects = tile_rects(&rice.root, area, Gaps::uniform(rice.look.gap as u16), None);
        assert_eq!(
            pet_neighbours(&rects, Gaps::uniform(rice.look.gap as u16)),
            Neighbours {
                tank: Some(WatchSide::Left),
                bonsai: None,
            },
            "on a {}x{} page",
            area.width,
            area.height
        );
    }
}

#[test]
fn a_zoomed_pet_tile_has_no_neighbours() {
    let rice = RiceLayout::default();
    let area = Rect::new(0, 0, 200, 50);
    let rects = tile_rects(&rice.root, area, Gaps::uniform(rice.look.gap as u16), None);
    let pet = rects
        .iter()
        .position(|(kind, _)| *kind == TileKind::Pet)
        .expect("the default page holds a pet tile");
    let zoomed = tile_rects(
        &rice.root,
        area,
        Gaps::uniform(rice.look.gap as u16),
        Some(pet),
    );
    assert_eq!(
        pet_neighbours(&zoomed, Gaps::uniform(rice.look.gap as u16)),
        Neighbours::default()
    );
}

#[test]
fn the_rules_chrome_paints_gutters_between_columns_and_insets_every_tile_but_the_furniture() {
    let area = Rect::new(0, 0, 100, 30);
    let root = Node::split_first(
        Dir::Row,
        23,
        Node::leaf(TileKind::Rooms),
        Node::split(
            Dir::Column,
            500,
            Node::leaf(TileKind::Chat),
            Node::leaf(TileKind::Music),
        ),
    );
    // One gutter per row split, the split's full height; a column split
    // has none, and neither does a page without a row gap.
    let gaps = Gaps { row: 1, col: 0 };
    assert_eq!(
        row_gutters(&root, area, gaps),
        vec![Rect::new(23, 0, 1, 30)]
    );
    assert!(row_gutters(&root, area, Gaps::uniform(0)).is_empty());

    // The page's chat and the rail draw their own chrome and keep every
    // cell; a panel, and a chat tile that only watches its room, wears a
    // title row and a column of air on each side.
    let rules = Chrome::Rules { arrange: false };
    let rect = Rect::new(0, 0, 23, 30);
    assert_eq!(tile_inner(rect, &rules, TileKind::Chat, true), rect);
    assert_eq!(tile_inner(rect, &rules, TileKind::Rooms, false), rect);
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Chat, false),
        Rect::new(1, 1, 21, 29)
    );
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Music, false),
        Rect::new(1, 1, 21, 29)
    );
    // In arrange mode every tile wears a title, so the focus has a ring.
    let arrange = Chrome::Rules { arrange: true };
    assert_eq!(
        tile_inner(rect, &arrange, TileKind::Chat, true),
        Rect::new(0, 1, 23, 29)
    );
}
