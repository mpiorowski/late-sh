use super::*;
use std::collections::{HashSet, VecDeque};
use unicode_width::UnicodeWidthChar;

#[test]
fn every_row_fits_the_grid_with_single_width_glyphs() {
    for (y, row) in MAP.iter().enumerate() {
        assert!(
            row.chars().count() <= usize::from(MAP_W),
            "row {y} is too wide"
        );
        for ch in row.chars() {
            assert_eq!(
                ch.width().unwrap_or(1),
                1,
                "row {y} has a wide or zero-width glyph {ch:?}"
            );
        }
    }
    for (y, row) in SOLID.iter().enumerate() {
        assert_eq!(row.len(), usize::from(MAP_W), "solid row {y} width");
        assert!(row.bytes().all(|b| b == b'#' || b == b'.'), "solid row {y}");
    }
}

#[test]
fn walls_facades_carts_and_the_drop_block_the_runner() {
    assert!(walkable(SPAWN.0, SPAWN.1), "spawn stands on open ground");
    assert!(!walkable(0, 25), "west wall");
    assert!(
        !walkable(SIGNS[0].zone.x0, SIGNS[0].zone.y0),
        "inside a facade"
    );
    assert!(!walkable(BOARD.x0, BOARD.y0), "the board kiosk");
    assert!(!walkable(SPAWN.0, DROP.y0 + 3), "the drop");
    assert!(walkable(OPEN.0, OPEN.1), "the street");
}

#[test]
fn every_landmark_is_reachable_from_the_wire() {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    seen.insert(SPAWN);
    queue.push_back(SPAWN);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let nx = x.saturating_add_signed(dx as i16);
            let ny = y.saturating_add_signed(dy as i16);
            if walkable(nx, ny) && seen.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }
    for landmark in Landmark::ALL {
        let reached = seen
            .iter()
            .any(|&(x, y)| nearest_landmark(x, y) == Some(landmark));
        assert!(reached, "{landmark:?} cannot be reached on foot");
    }
}

#[test]
fn every_walker_paces_open_floor() {
    for walker in WALKERS {
        assert!(walker.x1 > walker.x0, "{walker:?} has no stretch");
        for x in walker.x0..=walker.x1 {
            assert!(walkable(x, walker.y), "{walker:?} path blocked at {x}");
        }
    }
}

#[test]
fn the_wire_is_within_reach_at_the_spawn() {
    assert_eq!(nearest_landmark(SPAWN.0, SPAWN.1), Some(Landmark::Wire));
    // Open asphalt, nothing within reach.
    assert_eq!(nearest_landmark(OPEN.0, OPEN.1), None);
}

/// A shop that is not open yet has its name on the wall barely on: no
/// lit sign over the same cells, and no neon on the building. The
/// shops that do something keep theirs.
#[test]
fn a_closed_shop_is_spelled_but_never_lit() {
    assert!(!DARK_SIGNS.is_empty());
    for dark in DARK_SIGNS.iter().map(|sign| sign.zone) {
        assert!(
            !SIGNS
                .iter()
                .any(|sign| sign.zone.contains(dark.x0, dark.y0)),
            "a dark sign is also lit at {dark:?}"
        );
        let building = BUILDINGS
            .iter()
            .find(|building| building.zone.contains(dark.x0, dark.y0))
            .expect("a dark sign is on a building");
        assert!(building.color.is_none(), "a closed shop burns at {dark:?}");
    }
    for landmark in [
        Landmark::Armorer,
        Landmark::Lockers,
        Landmark::Bits,
        Landmark::Blades,
    ] {
        let reach = landmark.reach();
        let building = BUILDINGS
            .iter()
            .find(|building| building.zone.contains(reach.x0, reach.y0))
            .expect("the counter is inside a building");
        assert!(building.color.is_some(), "{landmark:?} is in a lit shop");
    }
}

/// A shop is in reach anywhere on its floor, and nowhere off it: behind
/// the counter, in the back of the room, by the door inside, never from
/// the street outside its walls.
#[test]
fn a_shop_is_in_reach_on_its_whole_floor_and_only_there() {
    for landmark in [
        Landmark::Armorer,
        Landmark::Tailor,
        Landmark::Lockers,
        Landmark::Bands,
        Landmark::Bar,
        Landmark::Repairs,
        Landmark::Bits,
        Landmark::Blades,
        Landmark::Ink,
        Landmark::Prints,
    ] {
        let reach = landmark.reach();
        let shop = BUILDINGS
            .iter()
            .map(|building| building.zone)
            .find(|zone| zone.contains(reach.x0, reach.y0))
            .expect("a shop's reach is inside its building");
        for y in 0..MAP_H {
            for x in 0..MAP_W {
                if !walkable(x, y) {
                    continue;
                }
                let inside = x > shop.x0 && x < shop.x1 && y > shop.y0 && y < shop.y1;
                let here = nearest_landmark(x, y) == Some(landmark);
                assert_eq!(
                    here, inside,
                    "{landmark:?} at ({x}, {y}): in reach {here}, on its floor {inside}"
                );
            }
        }
    }
}
