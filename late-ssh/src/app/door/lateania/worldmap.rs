// Overhead world map - slice 1: the coordinate field.
//
// The world is a room *graph*, not a heightmap, so this derives a spatial
// (x, y, z) for every room by walking exits out from the start room. Horizontal
// exits step x/y via `Dir::delta_2d`; up/down exits step z. The world is
// deterministic (fixed seeds), so this is recomputed identically every boot and
// never needs storing.
//
// Rooms reachable from the start form one connected component; portal-only
// regions (catacombs, the archipelago) carry no directional exits and so form
// their own components. Each component is laid out in its own coordinate space
// and then shifted east of the previous one, so the global field has no
// *artificial* cross-region collisions - the only collisions that remain are
// genuine non-Euclidean loops inside a single component, which is exactly what
// the collision report measures. Streaming only ever renders one neighbourhood,
// so those local conflicts never share the screen.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::world::{Dir, RoomId, World};

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

/// Derive an (x, y, z) for every room in the world. Deterministic: the start
/// room's component is laid out first (anchored at the low-x end), then every
/// other component in ascending room-id order, each shifted clear of the last.
pub fn derive_coords(world: &World) -> HashMap<RoomId, Coord> {
    let mut coords: HashMap<RoomId, Coord> = HashMap::new();
    let mut next_base_x: i32 = 0;

    // Seed the start room first so the main landmass sits at the western edge,
    // then walk every room id so nothing reachable only by portal is left out.
    let mut seeds = Vec::with_capacity(world.rooms.len() + 1);
    seeds.push(world.start_room);
    let mut ids: Vec<RoomId> = world.rooms.keys().copied().collect();
    ids.sort_unstable();
    seeds.extend(ids);

    for seed in seeds {
        if coords.contains_key(&seed) || world.room(seed).is_none() {
            continue;
        }

        // BFS this component in its own relative space. BFS (not DFS) means the
        // shortest exit-path to each room wins any clash the graph's geometry
        // would otherwise create - the same rule the side-panel minimap uses.
        let mut rel: HashMap<RoomId, Coord> = HashMap::new();
        rel.insert(seed, Coord { x: 0, y: 0, z: 0 });
        let mut queue = VecDeque::from([seed]);
        while let Some(rid) = queue.pop_front() {
            let here = rel[&rid];
            let Some(room) = world.room(rid) else { continue };
            for (dir, &dest) in &room.exits {
                if rel.contains_key(&dest) || coords.contains_key(&dest) {
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

        // Slide the whole component east so its western edge lands at
        // `next_base_x`, then park the next component clear of this one.
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
