// Overhead world map - the coordinate field (slices 1 + 1b).
//
// Derives a spatial (x, y, z) for every room by two mechanisms:
//
// * Procedurally-generated regions (the Reaches, Kaelmyr, the Sunderlakes,
//   Broceliande, the Frontier, the catacombs/thornwood/caverns) are laid out
//   from their generator grids via `world::region_layout`: each zone is an
//   exact w x h block placed at its own reserved origin. Room ids decode
//   straight to cell (x, y), so within a zone the layout is collision-free by
//   construction and zones never overlap.
// * Everything hand-authored (the capitals, roads, villages, housing, the
//   archipelago) has no grid, so it's placed by walking exits (BFS) per
//   connected component, each component shifted clear of the last. BFS never
//   steps into a generated zone (those are already placed).
//
// The world is deterministic (fixed seeds), so this is recomputed identically
// every boot and never stored. Streaming only renders one neighbourhood, so the
// non-continuous seams between reserved blocks never share the screen. What few
// collisions remain are genuine non-Euclidean loops inside the hand-authored
// core, which the collision report measures.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::LazyLock;

use super::world::{Dir, RoomId, World, region_layout, seed_world};

/// The world's coordinate field, derived once. The world is deterministic, so
/// these are stable for the whole process and shared by every session's map.
static WORLD_COORDS: LazyLock<HashMap<RoomId, Coord>> =
    LazyLock::new(|| derive_coords(&seed_world()));

/// The process-wide coordinate field. First call builds it (one world-gen).
pub fn world_coords() -> &'static HashMap<RoomId, Coord> {
    &WORLD_COORDS
}

/// A room's place in the overhead map. `z` is the vertical level: 0 is the
/// surface, negative is underground (down exits), positive is above.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coord {
    /// East is positive.
    pub x: i32,
    /// South is positive (matches `Dir::delta_2d`).
    pub y: i32,
    /// Up is positive, down is negative.
    pub z: i32,
}

/// The vertical step a vertical exit takes, or 0 for a horizontal one.
fn z_step(dir: Dir) -> i32 {
    match dir {
        Dir::Up => 1,
        Dir::Down => -1,
        _ => 0,
    }
}

/// Blank columns left between adjacent components so their bounding boxes never
/// touch in the global field.
const COMPONENT_MARGIN: i32 = 4;

/// Derive an (x, y, z) for every room. Generated zones are placed as exact
/// blocks from `region_layout`; hand-authored rooms are walked out by exits.
pub fn derive_coords(world: &World) -> HashMap<RoomId, Coord> {
    let mut coords: HashMap<RoomId, Coord> = HashMap::new();
    let mut next_base_x: i32 = 0;

    let mut ids: Vec<RoomId> = world.rooms.keys().copied().collect();
    ids.sort_unstable();

    // 1. Generated regions: group rooms by (region, zone) and place each zone in
    //    its own reserved column-block at exact cell offsets. Keys are sorted
    //    (BTreeMap) so the field is stable across boots.
    let mut zones: BTreeMap<(&'static str, u32), (i32, Vec<RoomId>)> = BTreeMap::new();
    for &id in &ids {
        if let Some(p) = region_layout(id) {
            zones
                .entry((p.region, p.zone))
                .or_insert((p.zone_w, Vec::new()))
                .1
                .push(id);
        }
    }
    for (zone_w, rids) in zones.into_values() {
        let origin_x = next_base_x;
        for id in rids {
            // A pure re-decode; matches the grouping above.
            let p = region_layout(id).expect("grouped by region_layout");
            coords.insert(
                id,
                Coord {
                    x: origin_x + p.x,
                    y: p.y,
                    z: p.z,
                },
            );
        }
        next_base_x = origin_x + zone_w + COMPONENT_MARGIN;
    }

    // 2. Hand-authored rooms: walk exits per connected component, each shifted
    //    clear of the last. Start room first so the capitals land together. BFS
    //    (not DFS) means the shortest exit-path wins any clash the graph's
    //    geometry would create, and it never steps into an already-placed zone.
    let mut seeds = Vec::with_capacity(ids.len() + 1);
    seeds.push(world.start_room);
    seeds.extend(ids.iter().copied());

    for seed in seeds {
        if coords.contains_key(&seed) || world.room(seed).is_none() || region_layout(seed).is_some()
        {
            continue;
        }

        let mut rel: HashMap<RoomId, Coord> = HashMap::new();
        rel.insert(seed, Coord { x: 0, y: 0, z: 0 });
        let mut queue = VecDeque::from([seed]);
        while let Some(rid) = queue.pop_front() {
            let here = rel[&rid];
            let Some(room) = world.room(rid) else {
                continue;
            };
            for (dir, &dest) in &room.exits {
                if rel.contains_key(&dest)
                    || coords.contains_key(&dest)
                    || region_layout(dest).is_some()
                {
                    continue;
                }
                let placed = match dir.delta_2d() {
                    Some((dx, dy)) => Coord {
                        x: here.x + dx,
                        y: here.y + dy,
                        z: here.z,
                    },
                    None => Coord {
                        x: here.x,
                        y: here.y,
                        z: here.z + z_step(*dir),
                    },
                };
                rel.insert(dest, placed);
                queue.push_back(dest);
            }
        }

        // Slide the component east so its western edge lands at `next_base_x`.
        let min_x = rel.values().map(|c| c.x).min().unwrap_or(0);
        let shift = next_base_x - min_x;
        let mut max_x = next_base_x;
        for (rid, c) in rel {
            let placed = Coord {
                x: c.x + shift,
                y: c.y,
                z: c.z,
            };
            max_x = max_x.max(placed.x);
            coords.insert(rid, placed);
        }
        next_base_x = max_x + COMPONENT_MARGIN;
    }

    coords
}

/// The streaming primitive: rooms inside a `(2*radius_x+1) x (2*radius_y+1)`
/// window centred on `center`, on the SAME z-level. Far regions and other
/// levels are skipped, so the renderer only ever handles a neighbourhood, and
/// the reserved-block seams between regions never share the screen. Sorted by
/// (y, x) for stable, top-to-bottom rendering.
pub fn visible(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    radius_x: i32,
    radius_y: i32,
) -> Vec<(RoomId, Coord)> {
    let mut out: Vec<(RoomId, Coord)> = coords
        .iter()
        .filter(|(_, c)| {
            c.z == center.z
                && (c.x - center.x).abs() <= radius_x
                && (c.y - center.y).abs() <= radius_y
        })
        .map(|(&id, &c)| (id, c))
        .collect();
    out.sort_by_key(|(_, c)| (c.y, c.x));
    out
}

/// A `cols x rows` grid of room ids centred on `center`, for painting the map.
/// `grid[row][col]` is the room at that screen cell, or `None` for empty space.
/// Where the spatial field still collides (hand-authored core), the lowest room
/// id wins so the picture is stable.
pub fn viewport(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    cols: i32,
    rows: i32,
) -> Vec<Vec<Option<RoomId>>> {
    let rx = cols / 2;
    let ry = rows / 2;
    let mut at: HashMap<(i32, i32), RoomId> = HashMap::new();
    for (id, c) in visible(coords, center, rx + 1, ry + 1) {
        at.entry((c.x, c.y))
            .and_modify(|cur| *cur = (*cur).min(id))
            .or_insert(id);
    }
    let left = center.x - rx;
    let top = center.y - ry;
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| at.get(&(left + c, top + r)).copied())
                .collect()
        })
        .collect()
}

/// A viewport with fog of war: cells the player hasn't visited read as empty.
/// The player's own room is always shown, so the map is never blank where they
/// stand. `visited` is the player's explored-room set.
pub fn viewport_explored(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    cols: i32,
    rows: i32,
    visited: &HashSet<RoomId>,
    player_room: RoomId,
) -> Vec<Vec<Option<RoomId>>> {
    viewport(coords, center, cols, rows)
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| cell.filter(|id| *id == player_room || visited.contains(id)))
                .collect()
        })
        .collect()
}

/// Points of interest at a room, for the map's overlay and cell inspector.
/// All static and deterministic, built once from the mob roster and the taming
/// table.
#[derive(Default, Clone, Debug)]
pub struct Poi {
    /// The zone boss lairing here, if any (a guaranteed rich drop).
    pub boss: Option<&'static str>,
    /// The boss's guaranteed loot, by item name.
    pub reward: Vec<&'static str>,
    /// Names of the mobs that spawn (can be slain) here.
    pub monsters: Vec<&'static str>,
    /// A tameable wild beast roaming here, if any.
    pub tameable: Option<&'static str>,
}

static POIS: LazyLock<HashMap<RoomId, Poi>> = LazyLock::new(build_pois);

fn build_pois() -> HashMap<RoomId, Poi> {
    let world = seed_world();
    let mut map: HashMap<RoomId, Poi> = HashMap::new();
    for spawn in &world.spawns {
        let e = map.entry(spawn.home).or_default();
        e.monsters.push(spawn.name);
        if spawn.boss {
            e.boss = Some(spawn.name);
            for &item_id in spawn.loot {
                if let Some(item) = super::items::item(item_id) {
                    e.reward.push(item.name);
                }
            }
        }
    }
    for beast in super::taming::wild_beasts() {
        map.entry(beast.home).or_default().tameable =
            Some(super::taming::TAMEABLE[beast.species].name);
    }
    map
}

/// The process-wide points-of-interest index (bosses, rewards, monsters,
/// tameable beasts), keyed by room. Built once on first use.
pub fn pois() -> &'static HashMap<RoomId, Poi> {
    &POIS
}

/// The points of interest at a single room, if any.
pub fn poi(room: RoomId) -> Option<&'static Poi> {
    POIS.get(&room)
}

/// Every coordinate shared by more than one room, with the room ids that land
/// there. Sorted for a stable report. An empty map means a perfectly clean
/// spatial field.
pub fn collisions(coords: &HashMap<RoomId, Coord>) -> BTreeMap<Coord, Vec<RoomId>> {
    let mut by_coord: BTreeMap<Coord, Vec<RoomId>> = BTreeMap::new();
    for (&rid, &c) in coords {
        by_coord.entry(c).or_default().push(rid);
    }
    by_coord.retain(|_, ids| ids.len() > 1);
    for ids in by_coord.values_mut() {
        ids.sort_unstable();
    }
    by_coord
}

/// A plain-text picture of one z-level around `center`, `radius` cells each way,
/// for eyeballing that the world lays out sanely before any rendering exists.
/// `@` is the centre room, `#` any other room on that level, a space is empty.
pub fn dump_level(coords: &HashMap<RoomId, Coord>, center: RoomId, radius: i32) -> String {
    let Some(&origin) = coords.get(&center) else {
        return String::from("(centre room has no coordinate)\n");
    };

    // Which rooms sit on each cell of this level.
    let mut occupied: BTreeMap<(i32, i32), RoomId> = BTreeMap::new();
    for (&rid, &c) in coords {
        if c.z == origin.z {
            occupied.entry((c.x, c.y)).or_insert(rid);
        }
    }

    let mut out = String::new();
    for y in (origin.y - radius)..=(origin.y + radius) {
        for x in (origin.x - radius)..=(origin.x + radius) {
            let ch = if (x, y) == (origin.x, origin.y) {
                '@'
            } else if occupied.contains_key(&(x, y)) {
                '#'
            } else {
                ' '
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "worldmap_test.rs"]
mod worldmap_test;
