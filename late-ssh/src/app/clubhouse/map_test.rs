use super::*;
use std::collections::{HashSet, VecDeque};

#[test]
fn map_rows_fit_declared_width() {
    for (y, row) in MAP.iter().enumerate() {
        let width = row.chars().count();
        assert!(
            width <= MAP_W as usize,
            "row {y} is {width} chars, wider than MAP_W"
        );
    }
    assert_eq!(MAP[0].chars().count(), MAP_W as usize);
    assert_eq!(MAP[MAP_H as usize - 1].chars().count(), MAP_W as usize);
}

#[test]
fn seats_sit_on_seat_anchors() {
    for seat in SEATS.iter().chain(std::iter::once(&GRAYBEARD_SEAT)) {
        assert_eq!(
            char_at(seat.x, seat.y),
            '_',
            "seat at ({}, {}) is not a seat anchor",
            seat.x,
            seat.y
        );
    }
}

#[test]
fn spawn_and_standing_spots_are_walkable() {
    assert!(walkable(SPAWN.0, SPAWN.1));
    for &(x, y) in STANDING_SPOTS {
        assert!(walkable(x, y), "standing spot ({x}, {y}) is blocked");
    }
    for &(x, y) in DOOR_STACK {
        assert!(walkable(x, y), "door-stack slot ({x}, {y}) is blocked");
    }
    for &(x, y) in DOG_WAYPOINTS {
        assert!(walkable(x, y), "dog waypoint ({x}, {y}) is blocked");
    }
    assert!(walkable(BOT_SPOT.0, BOT_SPOT.1), "bot spot is blocked");
}

#[test]
fn door_sign_zone_covers_the_door_lettering() {
    let sign: String = (DOOR_SIGN.x0..=DOOR_SIGN.x1)
        .map(|x| char_at(x, DOOR_SIGN.y0))
        .collect();
    assert_eq!(sign, "╡ door ╞");
}

/// Every cell a player can reach from spawn, by flood fill.
fn reachable_from_spawn() -> HashSet<(u16, u16)> {
    let mut seen = HashSet::from([SPAWN]);
    let mut queue = VecDeque::from([SPAWN]);
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [
            (x + 1, y),
            (x.wrapping_sub(1), y),
            (x, y + 1),
            (x, y.wrapping_sub(1)),
        ] {
            if walkable(nx, ny) && seen.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }
    seen
}

#[test]
fn bar_alley_is_sealed_from_players() {
    // The bartender's alley (behind the counter) and the counter itself
    // must not be reachable: shelf rows above, counter below, wall left,
    // seal column right.
    let reachable = reachable_from_spawn();
    assert!(
        !reachable.contains(&BARTENDER),
        "players can reach the bartender's alley"
    );
    for y in 2..=BAR_COUNTER.y1 {
        for x in 1..=BAR_COUNTER.x1 {
            assert!(!reachable.contains(&(x, y)), "alley leak at ({x}, {y})");
        }
    }
}

#[test]
fn seats_and_spots_are_reachable() {
    let reachable = reachable_from_spawn();
    for seat in SEATS.iter().chain(std::iter::once(&GRAYBEARD_SEAT)) {
        let (x, y) = (seat.x, seat.y);
        // A stool's own parens and leg surround the anchor, so look at
        // the full 8-neighborhood for a walkable approach cell.
        let mut approachable = false;
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                let cell = (
                    x.wrapping_add_signed(dx as i16),
                    y.wrapping_add_signed(dy as i16),
                );
                if reachable.contains(&cell) {
                    approachable = true;
                }
            }
        }
        assert!(approachable, "no way to walk up to the seat at ({x}, {y})");
    }
    for &(x, y) in STANDING_SPOTS {
        assert!(reachable.contains(&(x, y)), "spot ({x}, {y}) unreachable");
    }
}

#[test]
fn interactives_resolve_by_proximity() {
    // Standing in front of the bar.
    assert_eq!(
        nearest_interactive(28, 12, DOG_HOME),
        Some(Interactive::Bartender)
    );
    // Next to the jukebox.
    assert_eq!(
        nearest_interactive(82, 4, DOG_HOME),
        Some(Interactive::Jukebox)
    );
    // In front of the arcade cabinet.
    assert_eq!(
        nearest_interactive(154, 4, DOG_HOME),
        Some(Interactive::Arcade)
    );
    // Under the big door to the door games.
    assert_eq!(
        nearest_interactive(130, 8, DOG_HOME),
        Some(Interactive::Doors)
    );
    // Walking up to the poker table.
    assert_eq!(
        nearest_interactive(145, 15, DOG_HOME),
        Some(Interactive::Poker)
    );
    // Admiring the easel.
    assert_eq!(
        nearest_interactive(19, 33, DOG_HOME),
        Some(Interactive::Easel)
    );
    // Petting distance follows the dog around.
    assert_eq!(
        nearest_interactive(13, 26, DOG_HOME),
        Some(Interactive::Dog)
    );
    assert_eq!(
        nearest_interactive(101, 23, (100, 22)),
        Some(Interactive::Dog)
    );
    // Warming up by the hearth, out of the dog's reach.
    assert_eq!(
        nearest_interactive(25, 23, DOG_HOME),
        Some(Interactive::Fireplace)
    );
    // Middle of the rug: nothing (while the dog is elsewhere).
    assert_eq!(nearest_interactive(100, 22, DOG_HOME), None);
}

#[test]
fn walls_the_counter_and_the_bar_alley_block_movement() {
    assert!(!walkable(0, 25)); // west wall
    assert!(!walkable(28, 5)); // behind the counter
    assert!(!walkable(28, 9)); // the counter top
    assert!(!walkable(28, 10)); // the counter front
    assert!(walkable(28, 11)); // at the bar: head leans over the counter
    assert!(walkable(67, 16)); // right over a table
    assert!(walkable(100, 22)); // rug
    assert!(walkable(SPAWN.0, SPAWN.1)); // welcome mat
}
