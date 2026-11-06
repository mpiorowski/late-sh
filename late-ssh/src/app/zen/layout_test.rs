use ratatui::layout::Rect;

use super::{
    Chrome, Gaps, neighbour_side, pet_neighbours, rice_areas, row_gutters, tile_inner, tile_rects,
};
use crate::app::pet::ui::{Neighbours, WatchSide};
use crate::app::zen::state::{Dir, Node, RiceLayout, TileKind, TileTitle};

/// The mask with every tile up.
fn all_up(root: &Node) -> Vec<bool> {
    vec![false; root.leaf_count()]
}

#[test]
fn a_tank_tile_touching_the_pet_tile_is_a_neighbour_on_that_side() {
    let area = Rect::new(0, 0, 100, 40);
    let side_by_side = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Pet),
        Node::leaf(TileKind::Aquarium),
    );
    let rects = tile_rects(
        &side_by_side,
        area,
        Gaps::uniform(1),
        &all_up(&side_by_side),
    );
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
    let rects = tile_rects(&stacked, area, Gaps::uniform(0), &all_up(&stacked));
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
    let rects = tile_rects(
        &row,
        Rect::new(0, 0, 90, 30),
        Gaps::uniform(1),
        &all_up(&row),
    );
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
        let rects = tile_rects(
            &rice.root,
            area,
            Gaps::uniform(rice.look.gap as u16),
            &all_up(&rice.root),
        );
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
        row_gutters(&root, area, gaps, &all_up(&root)),
        vec![Rect::new(23, 0, 1, 30)]
    );
    assert!(row_gutters(&root, area, Gaps::uniform(0), &all_up(&root)).is_empty());

    let rules = Chrome::Rules { arrange: false };
    let rect = Rect::new(0, 0, 23, 30);
    // Every tile wears a title row; the page's chat and the rail keep
    // their full width, the rest give up a column of air on each side.
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Chat, true, TileTitle::Named),
        Rect::new(0, 1, 23, 29)
    );
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Rooms, false, TileTitle::Named),
        Rect::new(0, 1, 23, 29)
    );
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Chat, false, TileTitle::Named),
        Rect::new(1, 1, 21, 29)
    );
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Music, false, TileTitle::Named),
        Rect::new(1, 1, 21, 29)
    );
    // The rule alone costs the same row as the name.
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Music, false, TileTitle::Rule),
        Rect::new(1, 1, 21, 29)
    );
    // Hidden takes the row away: a panel told to go bare keeps only its
    // air, the page's chat told to go bare has the whole rect.
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Music, false, TileTitle::Hidden),
        Rect::new(1, 0, 21, 30)
    );
    assert_eq!(
        tile_inner(rect, &rules, TileKind::Chat, true, TileTitle::Hidden),
        rect
    );
    // In arrange mode every tile wears a title, so the focus has a ring,
    // whatever the tile says.
    let arrange = Chrome::Rules { arrange: true };
    assert_eq!(
        tile_inner(rect, &arrange, TileKind::Chat, true, TileTitle::Hidden),
        Rect::new(0, 1, 23, 29)
    );
}

/// The board's day takes the top row and the arrange keys the bottom one;
/// the tiles get what is between, and neither row when it is off.
#[test]
fn the_banner_and_the_footer_each_take_one_row_off_the_tiles() {
    let area = Rect::new(2, 3, 40, 10);
    let both = rice_areas(area, true, true);
    assert_eq!(both.banner, Some(Rect::new(2, 3, 40, 1)));
    assert_eq!(both.tiles, Rect::new(2, 4, 40, 8));
    assert_eq!(both.footer, Some(Rect::new(2, 12, 40, 1)));
    let neither = rice_areas(area, false, false);
    assert_eq!(neither.banner, None);
    assert_eq!(neither.tiles, area);
    assert_eq!(neither.footer, None);
}

/// The stock lobby is four rows and the pet three, each under its title
/// rule, and the bonsai under them takes whatever the column has left.
#[test]
fn the_stock_lobby_is_four_rows_the_pet_three_and_the_bonsai_takes_the_rest() {
    let rice = RiceLayout::home();
    let rects = tile_rects(
        &rice.root,
        Rect::new(0, 0, 200, 60),
        Gaps::uniform(rice.look.gap as u16),
        &all_up(&rice.root),
    );
    let (lobby, pet, bonsai) = (rects[6], rects[8], rects[9]);
    assert_eq!(lobby.0, TileKind::Lobby);
    assert_eq!(lobby.1.height, 5);
    assert_eq!(pet.0, TileKind::Pet);
    assert_eq!(pet.1.height, 4);
    assert_eq!(bonsai.0, TileKind::Bonsai);
    assert_eq!(bonsai.1.y, pet.1.bottom());
    assert_eq!(bonsai.1.bottom(), 60);
}

/// A rail beside a section of a reef and a pet over a chat, on Home's rules:
/// the shape a page keeps to #lounge.
fn section_over_chat() -> Node {
    Node::split_first(
        Dir::Row,
        20,
        Node::leaf(TileKind::Rooms),
        Node::split_first(
            Dir::Column,
            10,
            Node::split(
                Dir::Row,
                500,
                Node::leaf(TileKind::Aquarium),
                Node::leaf(TileKind::Pet),
            ),
            Node::leaf(TileKind::Chat),
        ),
    )
}

const RULES_GAPS: Gaps = Gaps { row: 1, col: 0 };

/// Every tile up, the section's gutter is there; one tile of the section down
/// and its sibling has the section whole, with no gutter left between them.
#[test]
fn a_hidden_tile_leaves_its_whole_split_to_its_sibling() {
    let (root, area) = (section_over_chat(), Rect::new(0, 0, 100, 30));
    assert_eq!(
        tile_rects(&root, area, RULES_GAPS, &all_up(&root)),
        vec![
            (TileKind::Rooms, Rect::new(0, 0, 20, 30)),
            (TileKind::Aquarium, Rect::new(21, 0, 39, 10)),
            (TileKind::Pet, Rect::new(61, 0, 39, 10)),
            (TileKind::Chat, Rect::new(21, 10, 79, 20)),
        ]
    );
    assert_eq!(
        row_gutters(&root, area, RULES_GAPS, &all_up(&root)),
        vec![Rect::new(20, 0, 1, 30), Rect::new(60, 0, 1, 10)]
    );

    let reef_down = [false, true, false, false];
    assert_eq!(
        tile_rects(&root, area, RULES_GAPS, &reef_down),
        vec![
            (TileKind::Rooms, Rect::new(0, 0, 20, 30)),
            (TileKind::Aquarium, Rect::default()),
            (TileKind::Pet, Rect::new(21, 0, 79, 10)),
            (TileKind::Chat, Rect::new(21, 10, 79, 20)),
        ]
    );
    assert_eq!(
        row_gutters(&root, area, RULES_GAPS, &reef_down),
        vec![Rect::new(20, 0, 1, 30)]
    );
}

/// The tree holds no section: both tiles of one down, the split over them
/// goes with them, its fixed rows included, and the chat has the column.
#[test]
fn a_section_with_every_tile_hidden_goes_and_the_chat_takes_its_rows() {
    let (root, area) = (section_over_chat(), Rect::new(0, 0, 100, 30));
    let section_down = [false, true, true, false];
    assert_eq!(
        tile_rects(&root, area, RULES_GAPS, &section_down),
        vec![
            (TileKind::Rooms, Rect::new(0, 0, 20, 30)),
            (TileKind::Aquarium, Rect::default()),
            (TileKind::Pet, Rect::default()),
            (TileKind::Chat, Rect::new(21, 0, 79, 30)),
        ]
    );
    assert_eq!(
        row_gutters(&root, area, RULES_GAPS, &section_down),
        vec![Rect::new(20, 0, 1, 30)]
    );
}

/// A tile down has no edge to share: a pet beside a hidden tank does not go
/// and watch it, and a hidden pet watches nothing.
#[test]
fn a_hidden_tile_is_nobodys_neighbour() {
    let area = Rect::new(0, 0, 100, 30);
    let root = Node::split(
        Dir::Row,
        500,
        Node::leaf(TileKind::Pet),
        Node::leaf(TileKind::Aquarium),
    );
    for hidden in [[false, true], [true, false]] {
        let rects = tile_rects(&root, area, RULES_GAPS, &hidden);
        assert_eq!(
            pet_neighbours(&rects, RULES_GAPS),
            Neighbours::default(),
            "with {hidden:?} down"
        );
    }
}
