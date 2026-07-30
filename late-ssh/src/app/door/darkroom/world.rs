/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The rules below
 * are transcribed from `script/world.js` and `script/path.js`. See
 * LICENSING.md and NOTICE. */

//! The wasteland: generating the map, walking it, burning supplies, and
//! getting home (or not).
//!
//! A pure rules module. Nothing here renders, saves or logs on its own: every
//! entry point returns a tagged outcome and appends its lines to a `Vec`, and
//! the session decides what to do with them. Expeditions are live play: they
//! run on keypresses and wall clock, never on village time.

use rand::Rng;

use super::data::{Building, Perk, Resource};
use super::model::{Expedition, GRID, Game, WorldMap};
use super::world_data::{self, Tile};

/// Which way a step goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl Direction {
    fn delta(self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::South => (0, 1),
            Direction::East => (1, 0),
            Direction::West => (-1, 0),
        }
    }
}

/// What a step landed on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Step {
    /// Nothing but ground.
    Walked,
    /// The village: the trip is over, safely.
    Home,
    /// A landmark's setpiece starts.
    Setpiece(&'static str),
    /// Something out there picked a fight.
    Fight,
    /// Starved or died of thirst.
    Died,
    /// The edge of the map: nothing happened at all.
    Blocked,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Draw a fresh world: terrain spiralled out from the village, then the
/// landmarks dropped in their rings.
///
/// Generation works on a grid of `Option<Tile>` rather than on the saved map,
/// because upstream's stickiness rule leans on the difference between a
/// square that is barrens and one that has not been written yet.
pub fn generate(rng: &mut impl Rng) -> WorldMap {
    let radius = world_data::RADIUS;
    let size = GRID as usize;
    let mut grid: Vec<Vec<Option<Tile>>> = vec![vec![None; size]; size];
    grid[radius as usize][radius as usize] = Some(Tile::Village);

    for r in 1..=radius {
        for t in 0..(r * 8) {
            let (x, y) = if t < 2 * r {
                (radius - r + t, radius - r)
            } else if t < 4 * r {
                (radius + r, radius - (3 * r) + t)
            } else if t < 6 * r {
                (radius + (5 * r) - t, radius + r)
            } else {
                (radius - r, radius + (7 * r) - t)
            };
            let tile = choose_tile(&grid, x, y, rng);
            grid[x as usize][y as usize] = Some(tile);
        }
    }

    for tile in Tile::ALL {
        let Some(landmark) = tile.landmark() else {
            continue;
        };
        for _ in 0..landmark.num {
            place_landmark(
                &mut grid,
                landmark.min_radius,
                landmark.max_radius,
                tile,
                rng,
            );
        }
    }

    let mut map = WorldMap::blank();
    for x in 0..GRID {
        for y in 0..GRID {
            let tile = grid[x as usize][y as usize].unwrap_or(Tile::Barrens);
            map.set_tile(x, y, tile);
        }
    }
    // The village square is lit from the start, which is what makes the first
    // step out of it visible.
    uncover(&mut map, radius, radius, world_data::LIGHT_RADIUS);
    map
}

/// Upstream's `chooseTile`: written neighbours pull the odds their way, and
/// the rest is the flat terrain distribution.
fn choose_tile(grid: &[Vec<Option<Tile>>], x: i32, y: i32, rng: &mut impl Rng) -> Tile {
    let mut chances: Vec<(Tile, f64)> = Vec::new();
    let mut non_sticky = 1.0;
    let neighbours = [(x, y - 1), (x, y + 1), (x + 1, y), (x - 1, y)];
    for (nx, ny) in neighbours {
        if nx < 0 || ny < 0 || nx >= GRID || ny >= GRID {
            continue;
        }
        let Some(tile) = grid[nx as usize][ny as usize] else {
            continue;
        };
        if tile == Tile::Village {
            // The village must sit in a forest, for the look of the thing.
            return Tile::Forest;
        }
        match chances.iter_mut().find(|(t, _)| *t == tile) {
            Some((_, chance)) => *chance += world_data::STICKINESS,
            None => chances.push((tile, world_data::STICKINESS)),
        }
        non_sticky -= world_data::STICKINESS;
    }
    for tile in [Tile::Forest, Tile::Field, Tile::Barrens] {
        let weight = tile.terrain_prob() * non_sticky;
        match chances.iter_mut().find(|(t, _)| *t == tile) {
            Some((_, chance)) => *chance += weight,
            None => chances.push((tile, weight)),
        }
    }
    chances.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.glyph().cmp(&b.0.glyph()))
    });

    let roll: f64 = rng.r#gen();
    let mut cumulative = 0.0;
    for (tile, chance) in &chances {
        cumulative += chance;
        if roll < cumulative {
            return *tile;
        }
    }
    Tile::Barrens
}

/// Drop one landmark in its ring, on terrain only.
fn place_landmark(
    grid: &mut [Vec<Option<Tile>>],
    min_radius: i32,
    max_radius: i32,
    landmark: Tile,
    rng: &mut impl Rng,
) -> (i32, i32) {
    let radius = world_data::RADIUS;
    let (mut x, mut y) = (radius, radius);
    let mut guard = 0;
    let terrain = |grid: &[Vec<Option<Tile>>], x: i32, y: i32| {
        grid[x as usize][y as usize].is_some_and(Tile::is_terrain)
    };
    while !terrain(grid, x, y) {
        guard += 1;
        if guard > 10_000 {
            // The ring is full. Upstream would spin here; giving up on one
            // landmark beats hanging a session.
            return (x, y);
        }
        let span = (max_radius - min_radius).max(0);
        let r = (rng.r#gen::<f64>() * f64::from(span)).floor() as i32 + min_radius;
        let mut x_dist = (rng.r#gen::<f64>() * f64::from(r)).floor() as i32;
        let mut y_dist = r - x_dist;
        if rng.r#gen::<f64>() < 0.5 {
            x_dist = -x_dist;
        }
        if rng.r#gen::<f64>() < 0.5 {
            y_dist = -y_dist;
        }
        x = (radius + x_dist).clamp(0, radius * 2);
        y = (radius + y_dist).clamp(0, radius * 2);
    }
    grid[x as usize][y as usize] = Some(landmark);
    (x, y)
}

/// Light everything within `r` of a square, in a diamond.
pub fn uncover(map: &mut WorldMap, x: i32, y: i32, r: i32) {
    map.set_seen(x, y);
    for i in -r..=r {
        let span = r - i.abs();
        for j in -span..=span {
            map.set_seen(x + i, y + j);
        }
    }
}

/// The lantern, which the scout perk doubles.
fn light(map: &mut WorldMap, x: i32, y: i32, game: &Game) {
    let radius = match game.has_perk(Perk::Scout) {
        true => world_data::LIGHT_RADIUS * 2,
        false => world_data::LIGHT_RADIUS,
    };
    uncover(map, x, y, radius);
}

/// Which way the compass points: toward the crashed starship.
pub fn compass_dir(map: &WorldMap) -> &'static str {
    let radius = world_data::RADIUS;
    for y in 0..GRID {
        for x in 0..GRID {
            if map.tile(x, y) != Tile::Ship {
                continue;
            }
            let (dx, dy) = (x - radius, y - radius);
            let horizontal = if dx < 0 { "west" } else { "east" };
            let vertical = if dy < 0 { "north" } else { "south" };
            let (ax, ay) = (f64::from(dx.abs()), f64::from(dy.abs()));
            return if ax / 2.0 > ay {
                horizontal
            } else if ay / 2.0 > ax {
                vertical
            } else if dy < 0 && dx < 0 {
                "northwest"
            } else if dy < 0 {
                "northeast"
            } else if dx < 0 {
                "southwest"
            } else {
                "southeast"
            };
        }
    }
    "nowhere"
}

// ---------------------------------------------------------------------------
// Setting out and coming back
// ---------------------------------------------------------------------------

/// Set out. The pack comes out of the store room, and the wanderer starts
/// full of water and health at the village.
pub fn embark(game: &mut Game) -> Expedition {
    // The loadout is a plan, not a claim: the store room may have shrunk
    // since it was packed (a charcutier ate the meat, a thief took the fur),
    // so each line is clamped to what is actually on the shelf, exactly as
    // upstream's outfitting screen re-clamps live.
    let outfit: Vec<(Resource, i64)> = game
        .outfit
        .iter()
        .map(|(item, count)| (*item, (*count).min(game.store(*item))))
        .filter(|(_, count)| *count > 0)
        .collect();
    for (item, count) in &outfit {
        game.add_store(*item, -count);
    }
    let map = game.world.clone().unwrap_or_else(WorldMap::blank);
    Expedition {
        x: world_data::VILLAGE_POS.0,
        y: world_data::VILLAGE_POS.1,
        hp: game.max_health(),
        water: game.max_water(),
        outfit: outfit.into_iter().collect(),
        map,
        ..Expedition::default()
    }
}

/// Home safe: commit the map, hand over anything the wasteland gave up, and
/// put the pack back in the store room.
pub fn go_home(game: &mut Game, trip: Expedition, out: &mut Vec<String>) {
    let map = trip.map;
    for building in &trip.cleared {
        if game.building_count(*building) == 0 {
            game.raise(*building);
            if let Some(job) = building.unlocks_jobs().first() {
                out.push(format!(
                    "the {} is ours. {}s can work it.",
                    building.label(),
                    job.label()
                ));
            }
        }
    }
    if trip.found_ship && game.ship.is_none() {
        game.ship = Some(super::model::ShipState::default());
    }
    // The pack goes back on the shelf; only what the wanderer always carries
    // stays packed for next time (upstream `leaveItAtHome`).
    let mut kept = std::collections::BTreeMap::new();
    for (item, count) in &trip.outfit {
        game.add_store(*item, *count);
        if !leave_it_at_home(*item) {
            kept.insert(*item, *count);
        }
    }
    game.outfit = kept;
    game.seen_all_map = map.all_seen();
    // The working copy becomes the committed map: everything uncovered,
    // cleared or roaded this trip is kept, and only now.
    game.world = Some(map);
    game.expedition = None;
}

/// Whether an item is dropped off at home rather than kept in the pack.
fn leave_it_at_home(item: Resource) -> bool {
    if matches!(
        item,
        Resource::CuredMeat
            | Resource::Bullets
            | Resource::EnergyCell
            | Resource::Charm
            | Resource::Medicine
    ) {
        return false;
    }
    if world_data::Weapon::of(item).is_some() {
        return false;
    }
    // Anything the workshop makes stays packed too.
    !super::data::CRAFTABLES
        .iter()
        .any(|craftable| craftable.item == item)
}

/// Died out there. The trip's map changes and its pack are lost, and another
/// expedition has to wait out the cooldown.
pub fn die(game: &mut Game, out: &mut Vec<String>) {
    out.push(world_data::MSG_WORLD_FADES.to_string());
    game.expedition = None;
    game.outfit.clear();
    game.embark_cooldown = world_data::DEATH_COOLDOWN;
}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

/// Take one step. Every consequence of a move lands here: narration, the
/// lantern, supplies, danger, landmarks and the odds of a fight.
pub fn step(
    game: &mut Game,
    trip: &mut Expedition,
    direction: Direction,
    rng: &mut impl Rng,
    out: &mut Vec<String>,
) -> Step {
    let (dx, dy) = direction.delta();
    let (nx, ny) = (trip.x + dx, trip.y + dy);
    if nx < 0 || ny < 0 || nx >= GRID || ny >= GRID {
        return Step::Blocked;
    }
    let from = trip.map.tile(trip.x, trip.y);
    trip.x = nx;
    trip.y = ny;
    let to = trip.map.tile(nx, ny);
    if let Some(message) = world_data::narrate_move(from, to) {
        out.push(message.to_string());
    }
    light(&mut trip.map, nx, ny, game);
    if check_danger(game, trip) {
        out.push(match trip.danger {
            true => world_data::MSG_DANGER.to_string(),
            false => world_data::MSG_SAFER.to_string(),
        });
    }

    if to == Tile::Village {
        return Step::Home;
    }
    let landmark = to.landmark();
    let played_out = trip.map.visited(nx, ny)
        || (to == Tile::Outpost && trip.used_outposts.contains(&format!("{nx},{ny}")));
    if let Some(landmark) = landmark
        && !played_out
    {
        return Step::Setpiece(landmark.scene);
    }
    if !use_supplies(game, trip, out) {
        return Step::Died;
    }
    match check_fight(game, trip, rng) {
        true => Step::Fight,
        false => Step::Walked,
    }
}

/// Whether the danger warning flipped this step.
fn check_danger(game: &Game, trip: &mut Expedition) -> bool {
    let distance = trip.distance();
    if !trip.danger {
        if game.store(Resource::IronArmour) == 0 && distance >= 8 {
            trip.danger = true;
            return true;
        }
        if game.store(Resource::SteelArmour) == 0 && distance >= 18 {
            trip.danger = true;
            return true;
        }
        return false;
    }
    if distance < 8 {
        trip.danger = false;
        return true;
    }
    if distance < 18 && game.store(Resource::IronArmour) > 0 {
        trip.danger = false;
        return true;
    }
    false
}

/// Eat, drink, and pay for it. Returns whether the wanderer is still alive.
fn use_supplies(game: &mut Game, trip: &mut Expedition, out: &mut Vec<String>) -> bool {
    trip.food_move += 1;
    trip.water_move += 1;

    let moves_per_food = match game.has_perk(Perk::SlowMetabolism) {
        true => world_data::MOVES_PER_FOOD * 2,
        false => world_data::MOVES_PER_FOOD,
    };
    if trip.food_move >= moves_per_food {
        trip.food_move = 0;
        let meat = trip.carrying(Resource::CuredMeat) - 1;
        if meat == 0 {
            out.push(world_data::MSG_MEAT_OUT.to_string());
            trip.outfit.insert(Resource::CuredMeat, 0);
        } else if meat < 0 {
            trip.outfit.insert(Resource::CuredMeat, 0);
            if !trip.starvation {
                out.push(world_data::MSG_STARVING.to_string());
                trip.starvation = true;
            } else {
                game.starved += 1;
                if game.starved >= world_data::STARVED_PERK_AT
                    && game.add_perk(world_data::STARVED_PERK)
                {
                    out.push(world_data::STARVED_PERK.notify().to_string());
                }
                return false;
            }
        } else {
            trip.starvation = false;
            trip.outfit.insert(Resource::CuredMeat, meat);
            trip.hp = (trip.hp + game.meat_heal()).min(game.max_health());
        }
    }

    let moves_per_water = match game.has_perk(Perk::DesertRat) {
        true => world_data::MOVES_PER_WATER * 2,
        false => world_data::MOVES_PER_WATER,
    };
    if trip.water_move >= moves_per_water {
        trip.water_move = 0;
        let water = trip.water - 1;
        if water == 0 {
            out.push(world_data::MSG_WATER_OUT.to_string());
            trip.water = 0;
        } else if water < 0 {
            trip.water = 0;
            if !trip.thirst {
                out.push(world_data::MSG_THIRST.to_string());
                trip.thirst = true;
            } else {
                game.dehydrated += 1;
                if game.dehydrated >= world_data::STARVED_PERK_AT
                    && game.add_perk(world_data::DEHYDRATED_PERK)
                {
                    out.push(world_data::DEHYDRATED_PERK.notify().to_string());
                }
                return false;
            }
        } else {
            trip.thirst = false;
            trip.water = water;
        }
    }
    true
}

/// Whether something jumps out this step. Never sooner than three moves after
/// the last fight, and the stealthy perk halves the odds.
fn check_fight(game: &Game, trip: &mut Expedition, rng: &mut impl Rng) -> bool {
    trip.fight_move += 1;
    if trip.fight_move <= world_data::FIGHT_DELAY {
        return false;
    }
    let chance = match game.has_perk(Perk::Stealthy) {
        true => world_data::FIGHT_CHANCE * 0.5,
        false => world_data::FIGHT_CHANCE,
    };
    if rng.r#gen::<f64>() < chance {
        trip.fight_move = 0;
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Clearing a dungeon
// ---------------------------------------------------------------------------

/// A cleared landmark becomes an outpost, and a road runs back to whatever is
/// already connected.
pub fn clear_dungeon(trip: &mut Expedition) {
    trip.map.set_tile(trip.x, trip.y, Tile::Outpost);
    draw_road(trip);
}

/// Run a road from here to the nearest road, outpost or the village itself.
pub fn draw_road(trip: &mut Expedition) {
    let start = (trip.x, trip.y);
    let closest = closest_road(&trip.map, start);
    let x_dist = start.0 - closest.0;
    let y_dist = start.1 - closest.1;
    let x_dir = x_dist.signum();
    let y_dir = y_dist.signum();
    let (x_intersect, y_intersect) = if x_dist.abs() > y_dist.abs() {
        (closest.0, closest.1 + y_dist)
    } else {
        (closest.0 + x_dist, closest.1)
    };
    for x in 0..x_dist.abs() {
        let px = closest.0 + x_dir * x;
        if trip.map.tile(px, y_intersect).is_terrain() {
            trip.map.set_tile(px, y_intersect, Tile::Road);
        }
    }
    for y in 0..y_dist.abs() {
        let py = closest.1 + y_dir * y;
        if trip.map.tile(x_intersect, py).is_terrain() {
            trip.map.set_tile(x_intersect, py, Tile::Road);
        }
    }
}

/// Spiral out along manhattan contours until something road-like turns up.
fn closest_road(map: &WorldMap, start: (i32, i32)) -> (i32, i32) {
    let village = world_data::VILLAGE_POS;
    let distance = (start.0 - village.0).abs() + (start.1 - village.1).abs();
    let limit = (distance + 2).pow(2);
    let (mut x, mut y) = (0, 0);
    let (mut dx, mut dy) = (1, -1);
    for _ in 0..limit {
        let (sx, sy) = (start.0 + x, start.1 + y);
        if sx > 0 && sx < GRID - 1 && sy > 0 && sy < GRID - 1 {
            let tile = map.tile(sx, sy);
            let connected = tile == Tile::Road
                || (tile == Tile::Outpost && !(x == 0 && y == 0))
                || tile == Tile::Village;
            if connected {
                return (sx, sy);
            }
        }
        if x == 0 || y == 0 {
            let tmp = dx;
            dx = -dy;
            dy = tmp;
        }
        if x == 0 && y <= 0 {
            x += 1;
        } else {
            x += dx;
            y += dy;
        }
    }
    village
}

/// The mines a cleared setpiece hands over, by scene name.
pub fn mine_for(scene: &str) -> Option<Building> {
    match scene {
        "ironmine" => Some(Building::IronMine),
        "coalmine" => Some(Building::CoalMine),
        "sulphurmine" => Some(Building::SulphurMine),
        _ => None,
    }
}
