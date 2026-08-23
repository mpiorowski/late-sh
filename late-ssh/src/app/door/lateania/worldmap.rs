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
// every boot and never stored. Streaming only renders one neighbourhood, and
// `COMPONENT_MARGIN` is wider than any terminal, so the non-continuous seams
// between reserved blocks never share the screen. What few collisions remain
// are genuine non-Euclidean loops inside the hand-authored core, which the
// collision report measures.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::LazyLock;

use super::world::{Dir, RoomId, World, region_layout, seed_world};

/// The world graph, built once. Deterministic, so it is stable for the whole
/// process and shared by every session's map (coords, exits, POIs all read it).
static WORLD: LazyLock<World> = LazyLock::new(seed_world);

/// The world's coordinate field, derived once from the shared world.
static WORLD_COORDS: LazyLock<HashMap<RoomId, Coord>> = LazyLock::new(|| derive_coords(&WORLD));

fn world() -> &'static World {
    &WORLD
}

/// The process-wide coordinate field. First call builds the world once.
pub fn world_coords() -> &'static HashMap<RoomId, Coord> {
    &WORLD_COORDS
}

/// The zone a room belongs to, from the shared world.
pub fn zone_of(id: RoomId) -> Option<&'static str> {
    world().room(id).map(|r| r.zone)
}

/// Force the coordinate field and the POI index to build now. Both are lazy
/// statics that cost a world-gen apiece; the service calls this at startup so
/// the first player to open the map doesn't pay for them on the render thread,
/// which holds the app mutex.
pub fn warm() {
    LazyLock::force(&WORLD_COORDS);
    LazyLock::force(&POIS);
    LazyLock::force(&LAND_LINKS);
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

/// The widest map viewport we ever paint, in cells. Terminals wider than this
/// are not a thing we render maps into.
pub const MAX_VIEWPORT_COLS: i32 = 400;

/// How far the camera may pan from the player, in cells each way. Far enough to
/// look around any one place, near enough that Enter is never a long way home.
pub const PAN_LIMIT: i32 = MAX_VIEWPORT_COLS;

/// Blank columns left between adjacent components so their bounding boxes never
/// touch in the global field. Two reserved blocks are unrelated places, and a
/// seam between them must never share the screen: the furthest a player can see
/// from where they stand is a full pan plus half a viewport, so the margin has
/// to beat that. (At the original margin of 4, an 80-column map showed five
/// unrelated zones side by side and a forest slab against Embergate's square.)
const COMPONENT_MARGIN: i32 = PAN_LIMIT + MAX_VIEWPORT_COLS;

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
                    // A home's doorway: never walked, so each plot's interior
                    // seeds later as its own island clear of the town grid.
                    || super::housing::crosses_threshold(rid, dest)
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
/// id wins so the picture is stable. This is the fog-less view, used by the
/// dumps and the tests; what players see comes from `map_canvas` (both the
/// live field and the overhead map), which resolves collisions against the
/// player and their explored set instead. `viewport_explored` is that same
/// fog-and-filter resolution without the corridor pass, kept for the tests
/// that pin it.
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

/// Which of two rooms sharing a map cell should be shown. The player's own
/// room always wins; failing that, a room in the same region as the one the
/// player currently stands in wins, so a collision reads as "the place I'm
/// in", not an arbitrary global pick. Since the unfold (`zone_interleaves`
/// keeps it that way) the field's few remaining collisions are same-zone
/// stacks - a named wing folded back over its own zone's side room - but the
/// region preference stays: it is what makes the answer follow the player
/// rather than the id order if a cross-region stack ever returns. The lowest
/// id is the final tie-break, kept only for determinism when neither room
/// matches (or both do).
fn resolve_collision(
    current: RoomId,
    candidate: RoomId,
    player_room: RoomId,
    player_region: Option<&'static str>,
) -> RoomId {
    if current == player_room {
        return current;
    }
    if candidate == player_room {
        return candidate;
    }
    if let Some(region) = player_region {
        let region_of = |id: RoomId| super::world::region_atlas_entry(id).map(|(name, _)| name);
        let current_matches = region_of(current) == Some(region);
        let candidate_matches = region_of(candidate) == Some(region);
        if candidate_matches && !current_matches {
            return candidate;
        }
        if current_matches && !candidate_matches {
            return current;
        }
    }
    current.min(candidate)
}

/// A viewport with fog of war: cells the player hasn't visited read as empty.
/// `visited` is the player's explored-room set.
///
/// Cells are resolved by `resolve_collision`. Resolving before the fog (as a
/// plain filter over `viewport` would) loses to the collision tie-break: see
/// `resolve_collision` for why a player standing in one of a colliding pair
/// must not lose their own cell (or have it painted as an unrelated region).
pub fn viewport_explored(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    cols: i32,
    rows: i32,
    visited: &HashSet<RoomId>,
    player_room: RoomId,
) -> Vec<Vec<Option<RoomId>>> {
    let rx = cols / 2;
    let ry = rows / 2;
    let player_region = super::world::region_atlas_entry(player_room).map(|(name, _)| name);
    let mut at: HashMap<(i32, i32), RoomId> = HashMap::new();
    for (id, c) in visible(coords, center, rx + 1, ry + 1) {
        if id != player_room && !visited.contains(&id) {
            continue;
        }
        at.entry((c.x, c.y))
            .and_modify(|cur| *cur = resolve_collision(*cur, id, player_room, player_region))
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

/// The bounding box of the whole coordinate field, for clamping the camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min: Coord,
    pub max: Coord,
}

static BOUNDS: LazyLock<Bounds> = LazyLock::new(|| derive_bounds(world_coords()));

/// The process-wide field bounds. Cheap after the first call.
pub fn bounds() -> Bounds {
    *BOUNDS
}

pub fn derive_bounds(coords: &HashMap<RoomId, Coord>) -> Bounds {
    let zero = Coord { x: 0, y: 0, z: 0 };
    let mut min = zero;
    let mut max = zero;
    for (i, c) in coords.values().enumerate() {
        if i == 0 {
            min = *c;
            max = *c;
            continue;
        }
        min = Coord {
            x: min.x.min(c.x),
            y: min.y.min(c.y),
            z: min.z.min(c.z),
        };
        max = Coord {
            x: max.x.max(c.x),
            y: max.y.max(c.y),
            z: max.z.max(c.z),
        };
    }
    Bounds { min, max }
}

/// The world-map camera: where the view sits relative to the player's own room.
/// Held per session by `State`; the player's coordinate is passed in on every
/// move so the camera itself stays a pure value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapCamera {
    scroll: (i32, i32),
    level_offset: i32,
}

impl MapCamera {
    /// Offset from the player, in world cells (0, 0 = centred on them).
    pub fn scroll(self) -> (i32, i32) {
        self.scroll
    }

    /// Levels above (+) or below (-) the one the player stands on.
    pub fn level_offset(self) -> i32 {
        self.level_offset
    }

    /// The cell the view is centred on, given where the player stands.
    pub fn center(self, player: Coord) -> Coord {
        Coord {
            x: player.x + self.scroll.0,
            y: player.y + self.scroll.1,
            z: player.z + self.level_offset,
        }
    }

    /// Snap back onto the player, position and level.
    pub fn recenter(&mut self) {
        *self = Self::default();
    }

    /// Pan by one cell, clamped twice: to `PAN_LIMIT` cells from the player,
    /// which is what keeps a panned viewport from ever reaching the next
    /// reserved block, and to the field's own bounds, so a held key cannot walk
    /// the camera into unbounded blank with only Enter to get back.
    pub fn pan(&mut self, player: Coord, bounds: Bounds, dx: i32, dy: i32) {
        let clamp = |want: i32, player: i32, lo: i32, hi: i32| {
            want.clamp(player - PAN_LIMIT, player + PAN_LIMIT)
                .clamp(lo, hi)
                - player
        };
        self.scroll = (
            clamp(
                player.x + self.scroll.0 + dx,
                player.x,
                bounds.min.x,
                bounds.max.x,
            ),
            clamp(
                player.y + self.scroll.1 + dy,
                player.y,
                bounds.min.y,
                bounds.max.y,
            ),
        );
    }

    /// View one level up (+1) or down (-1), clamped to the levels that exist.
    pub fn change_level(&mut self, player: Coord, bounds: Bounds, delta: i32) {
        let want = player.z + self.level_offset + delta;
        self.level_offset = want.clamp(bounds.min.z, bounds.max.z) - player.z;
    }
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
    /// A notable non-boss foe lairing here (epic/legendary rank), if any - the
    /// hunt-worthy targets, distinct from trash spawns.
    pub elite_foe: Option<&'static str>,
    /// A tameable wild beast roaming here, if any.
    pub tameable: Option<&'static str>,
    /// A harvestable resource here (the gather trade worked at it), if any.
    pub gather: Option<GatherPoi>,
}

/// A gather node's skill and level gate, for the map inspector - previously
/// the map only ever showed the skill name, with no way to scout whether a
/// node was even worth the walk before physically standing in its room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatherPoi {
    pub skill: &'static str,
    pub level_req: i32,
}

static POIS: LazyLock<HashMap<RoomId, Poi>> = LazyLock::new(build_pois);

fn build_pois() -> HashMap<RoomId, Poi> {
    let world = world();
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
    // Regional champions: the single toughest non-boss foe in each land. The
    // endgame is wall-to-wall max-level mobs, so a per-room "elite" marker would
    // carpet the map; one apex hunt per region stays rare and worth flagging.
    let mut champ: HashMap<&'static str, (RoomId, &'static str, i32)> = HashMap::new();
    for spawn in &world.spawns {
        if spawn.boss {
            continue;
        }
        let Some((region, _)) = super::world::region_atlas_entry(spawn.home) else {
            continue;
        };
        let lvl = spawn.level();
        champ
            .entry(region)
            .and_modify(|best| {
                if lvl > best.2 {
                    *best = (spawn.home, spawn.name, lvl);
                }
            })
            .or_insert((spawn.home, spawn.name, lvl));
    }
    for (_region, (home, name, _)) in champ {
        map.entry(home).or_default().elite_foe = Some(name);
    }
    for beast in super::taming::wild_beasts() {
        map.entry(beast.home).or_default().tameable =
            Some(super::taming::beast_species(beast.species).name);
    }
    for n in super::world::NODES {
        map.entry(n.home).or_default().gather = Some(GatherPoi {
            skill: n.skill.key(),
            level_req: n.level_req,
        });
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

/// A room's name, for naming a place the player marked without shipping the
/// string through a snapshot (the world is static and process-global).
pub fn room_name(room: RoomId) -> Option<&'static str> {
    world().rooms.get(&room).map(|r| r.name)
}

/// The room under one map cell, resolved exactly as the canvas resolves it
/// (see `resolve_collision`). For answering "what am I pointing at" without
/// building a whole canvas, so input can act on the crosshair.
pub fn room_at(
    coords: &HashMap<RoomId, Coord>,
    at: Coord,
    visited: &HashSet<RoomId>,
    player_room: RoomId,
) -> Option<RoomId> {
    let player_region = super::world::region_atlas_entry(player_room).map(|(name, _)| name);
    visible(coords, at, 0, 0)
        .into_iter()
        .filter(|(id, _)| *id == player_room || visited.contains(id))
        .map(|(id, _)| id)
        .reduce(|a, b| resolve_collision(a, b, player_room, player_region))
}

/// The first step of the shortest walk from `from` to `dest`, and how many
/// rooms that walk is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Route {
    /// Which exit to take from the room you are standing in right now.
    pub next: Dir,
    /// Rooms between here and there, so the line can say how far it still is.
    pub rooms: usize,
}

/// Shortest walk from `from` to `dest`, over rooms the player has already
/// visited.
///
/// Restricting to `visited` is what makes this honest rather than a spoiler
/// machine: it can only ever retrace ground the player has actually walked, so
/// it never reveals an unexplored shortcut, and it needs no gate check either
/// (a room can only be in `visited` if the player legitimately walked into it,
/// which means they passed whatever gate stands in front of it). It is also
/// why a route always exists when the destination is a known room: having been
/// there at all means such a path was once walked.
///
/// The whole point is the *first step*. The map can show a place and still
/// leave "so which way do I actually go" unanswered, because a zone boundary
/// is a jump in the coordinate field rather than a direction. A direction is
/// the one answer that never needs the picture to be legible.
pub fn route(from: RoomId, dest: RoomId, visited: &HashSet<RoomId>) -> Option<Route> {
    if from == dest || !visited.contains(&dest) {
        return None;
    }
    let rooms = &world().rooms;
    // Each frontier entry carries the direction its walk left `from` by, so
    // arriving at `dest` names the first step without rebuilding the path.
    let mut queue: VecDeque<(RoomId, Dir, usize)> = VecDeque::new();
    let mut seen: HashSet<RoomId> = HashSet::from([from]);
    for (dir, &next) in rooms.get(&from)?.exits.iter() {
        if !visited.contains(&next) || !seen.insert(next) {
            continue;
        }
        if next == dest {
            return Some(Route {
                next: *dir,
                rooms: 1,
            });
        }
        queue.push_back((next, *dir, 1));
    }
    while let Some((room, first, depth)) = queue.pop_front() {
        let Some(r) = rooms.get(&room) else { continue };
        for &next in r.exits.values() {
            if !visited.contains(&next) || !seen.insert(next) {
                continue;
            }
            if next == dest {
                return Some(Route {
                    next: first,
                    rooms: depth + 1,
                });
            }
            queue.push_back((next, first, depth + 1));
        }
    }
    None
}

/// One cell of the rendered map. Rooms sit on even offsets from the centre and
/// the corridors between them on the odd offsets in between, so the map shows
/// which rooms are actually linked (walkable), not just spatially near.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Empty,
    Room(RoomId),
    /// A horizontal corridor (east/west exit) between two rooms.
    LinkH,
    /// A vertical corridor (north/south exit) between two rooms.
    LinkV,
    /// An exit into the unexplored: a faint half-stub of corridor (`─`/`│`)
    /// showing that this room has a walkable exit into fog you haven't
    /// visited yet. Always a stub, never an arrow - on the field a line means
    /// "walkable path" and nothing else, and arrows read as controls. So a
    /// discovered room's unexplored side never reads as a dead end, with no
    /// spoiler of what's out there.
    Hint(char),
    /// A path-continuation hint to a room you've *already* visited that the map
    /// can't draw right beside it (the hand-authored core doesn't lay perfectly
    /// flat, so some branches scatter, or the link crosses into a whole other
    /// reserved block, like the Sunderlakes hanging off Melvanala). Same stub
    /// glyph as `Hint` (never an arrow), just styled brighter, so a known
    /// non-Euclidean jump reads distinctly from the true edge of your
    /// exploration.
    HintKnown(char),
    /// A room has a way up, down, or both. Drawn in the room's own up-right
    /// corner cell (odd column, odd row), a layer nothing else ever touches:
    /// rooms sit on even/even and corridors on the odd cell between two of
    /// them, so each room owns exactly one free corner and no two rooms can
    /// claim the same one.
    ///
    /// This is not decoration. A flat level cannot draw a vertical link, so
    /// the map used to omit them entirely - and in a world where every zone
    /// chains to the next one by a stair and every continent hangs off
    /// another by a stair, that meant opening the map to find the way onward
    /// showed you everything *except* the way onward. The stair says only
    /// "there is a way through here", never what waits on the far side.
    Stair(char),
}

/// Glyph for a room's vertical exits. A room with both ways reads as `▾`
/// rather than a two-headed arrow: only one cell per room is free (see
/// `Tile::Stair`), an arrow reads as a control on this map where every other
/// glyph is terrain, and down is the way onward everywhere in this world. The
/// room panel's exits line carries the full truth for the rooms with both.
fn stair_glyph(down: bool, up: bool) -> Option<char> {
    match (down, up) {
        (true, _) => Some('\u{25be}'),     // ▾
        (false, true) => Some('\u{25b4}'), // ▴
        (false, false) => None,
    }
}

/// Build a `cols x rows` map canvas centred on `center`, interleaving rooms
/// (even cells) with the corridors between linked rooms (odd cells). Fog of
/// war: a room shows only if visited (or it's the player); a corridor shows
/// only when BOTH its rooms are visited, so paths into the unknown stay hidden.
/// A vertical link has no flat direction to run in, so it is flagged on the
/// room itself as a `Tile::Stair` in that room's corner cell instead. The
/// player's room wins any cell collision so `@` never vanishes under a stacked
/// hand-authored room.
pub fn map_canvas(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    cols: i32,
    rows: i32,
    visited: &HashSet<RoomId>,
    player_room: RoomId,
) -> Vec<Vec<Tile>> {
    let (w, h) = (cols.max(0) as usize, rows.max(0) as usize);
    let mut canvas = vec![vec![Tile::Empty; w]; h];
    if cols <= 0 || rows <= 0 {
        return canvas;
    }
    let cx = cols / 2;
    let cy = rows / 2;
    let seen = |id: RoomId| id == player_room || visited.contains(&id);
    let put = |canvas: &mut Vec<Vec<Tile>>, sc: i32, sr: i32, t: Tile| {
        if !(0..cols).contains(&sc) || !(0..rows).contains(&sr) {
            return;
        }
        // The player's room outranks a collided room on the same cell. Belt
        // and braces: the resolve pass below already guarantees this, but a
        // corridor cell placed after it must not clobber a room cell either.
        if let Tile::Room(existing) = canvas[sr as usize][sc as usize]
            && existing == player_room
        {
            return;
        }
        canvas[sr as usize][sc as usize] = t;
    };

    // Each room spans two screen cells, so a `cols`-wide view is cols/2 rooms;
    // pull a slightly wider window so corridors reaching in are covered.
    let rxw = cols / 4 + 2;
    let ryw = rows / 4 + 2;

    // Resolve which room wins each world coordinate before drawing anything,
    // so a collision paints the room that matches where the player actually
    // is (see `resolve_collision`) instead of whichever happened to be last
    // out of a hash-ordered iterator.
    let player_region = super::world::region_atlas_entry(player_room).map(|(name, _)| name);
    let mut winners: HashMap<(i32, i32), RoomId> = HashMap::new();
    for (id, c) in visible(coords, center, rxw, ryw) {
        if c.z != center.z || !seen(id) {
            continue;
        }
        winners
            .entry((c.x, c.y))
            .and_modify(|cur| *cur = resolve_collision(*cur, id, player_room, player_region))
            .or_insert(id);
    }

    for (&(x, y), &id) in &winners {
        let sc = cx + 2 * (x - center.x);
        let sr = cy + 2 * (y - center.y);
        put(&mut canvas, sc, sr, Tile::Room(id));

        let Some(room) = world().rooms.get(&id) else {
            continue;
        };
        // Flag the ways up and down before walking the flat exits: the match
        // below has nowhere to draw them, which is exactly why they need their
        // own corner cell.
        if let Some(glyph) = stair_glyph(
            room.exits.contains_key(&Dir::Down),
            room.exits.contains_key(&Dir::Up),
        ) {
            put(&mut canvas, sc + 1, sr - 1, Tile::Stair(glyph));
        }
        for (dir, dest) in &room.exits {
            if !seen(*dest) {
                // An exit into the fog: a faint half-stub of path trailing off
                // into the unknown, so a discovered room never reads as
                // stranded. A stub, not an arrow - on the field a line means
                // "walkable path" and nothing else, and arrows read as
                // controls. No spoiler of what waits at the far end.
                let (dx, dy) = match dir {
                    Dir::East => (1, 0),
                    Dir::West => (-1, 0),
                    Dir::North => (0, -1),
                    Dir::South => (0, 1),
                    _ => continue, // up/down: no flat direction to point
                };
                let (hx, hy) = (sc + dx, sr + dy);
                if (0..cols).contains(&hx)
                    && (0..rows).contains(&hy)
                    && canvas[hy as usize][hx as usize] == Tile::Empty
                {
                    let stub = if dx != 0 { '\u{2500}' } else { '\u{2502}' };
                    canvas[hy as usize][hx as usize] = Tile::Hint(stub);
                }
                continue;
            }
            let Some(&dc) = coords.get(dest) else {
                continue;
            };
            if dc.z != center.z {
                continue; // stairs: not drawn on a flat level
            }
            match (dir, dc.x - x, dc.y - y) {
                (Dir::East, 1, 0) => put(&mut canvas, sc + 1, sr, Tile::LinkH),
                (Dir::West, -1, 0) => put(&mut canvas, sc - 1, sr, Tile::LinkH),
                (Dir::North, 0, -1) => put(&mut canvas, sc, sr - 1, Tile::LinkV),
                (Dir::South, 0, 1) => put(&mut canvas, sc, sr + 1, Tile::LinkV),
                _ if dc.z == center.z => {
                    // Linked on the same level but not in the adjacent cell (the
                    // hand-authored core scatters some branches, or the link
                    // crosses into a whole other reserved block, like the
                    // Sunderlakes hanging off Melvanala). This room is already
                    // known, unlike a plain fog `Hint`, so it becomes a
                    // `HintKnown` instead - same stub glyph, styled brighter,
                    // so a discovered non-Euclidean jump reads differently from
                    // the unexplored edge of the map.
                    //
                    // The stub goes on the side the exit is actually walked
                    // out of, NOT toward where the destination happens to sit
                    // in the field. Across reserved blocks that coordinate
                    // delta means nothing - it only records which block was
                    // laid down first - so siding by it drew paths that were
                    // not there. A house door facing east onto a close that
                    // was placed 5,622 cells west drew a west stub, and
                    // walking west then failed. Inventing a path is the worst
                    // thing this map can do, so the exit's own direction is
                    // the only honest answer.
                    let Some((dx, dy)) = dir.delta_2d() else {
                        continue; // up/down: flagged as a Stair, not a stub
                    };
                    let (hx, hy) = (sc + dx, sr + dy);
                    if (0..cols).contains(&hx)
                        && (0..rows).contains(&hy)
                        && canvas[hy as usize][hx as usize] == Tile::Empty
                    {
                        let stub = if dx != 0 { '\u{2500}' } else { '\u{2502}' };
                        canvas[hy as usize][hx as usize] = Tile::HintKnown(stub);
                    }
                }
                _ => {} // stairs (up/down): not drawn on a flat level
            }
        }
    }
    canvas
}

/// An off-screen point of interest, projected to the map border as a direction
/// arrow. Points the way without revealing the room, so an unexplored boss is a
/// "that way" hint, not a spoiler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MapArrow {
    pub row: usize,
    pub col: usize,
    pub glyph: char,
    pub boss: bool,
}

fn arrow_glyph(dx: i32, dy: i32) -> char {
    match (dx.signum(), dy.signum()) {
        (0, -1) => '\u{2191}',  // ↑
        (0, 1) => '\u{2193}',   // ↓
        (-1, 0) => '\u{2190}',  // ←
        (1, 0) => '\u{2192}',   // →
        (-1, -1) => '\u{2196}', // ↖
        (1, -1) => '\u{2197}',  // ↗
        (-1, 1) => '\u{2199}',  // ↙
        (1, 1) => '\u{2198}',   // ↘
        _ => '\u{2022}',        // • (shouldn't happen for off-screen)
    }
}

/// Border arrows for every boss / tameable POI that is off-screen on the viewed
/// level. On-screen POIs are left to the canvas (a marker if visited, or hidden
/// by fog if not). Deduplicated per border cell, with bosses taking priority.
pub fn poi_arrows(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    cols: i32,
    rows: i32,
) -> Vec<MapArrow> {
    if cols <= 0 || rows <= 0 {
        return Vec::new();
    }
    let cx = cols / 2;
    let cy = rows / 2;
    let mut by_cell: BTreeMap<(usize, usize), (char, bool)> = BTreeMap::new();
    for (room, poi) in pois() {
        let is_boss = poi.boss.is_some();
        if !is_boss && poi.tameable.is_none() {
            continue;
        }
        let Some(&c) = coords.get(room) else {
            continue;
        };
        if c.z != center.z {
            continue;
        }
        // Distinct reserved blocks always sit at least `COMPONENT_MARGIN` apart
        // (see the module doc comment), so a delta within `PAN_LIMIT` guarantees
        // the POI is in the *same* block as the player - the one case where the
        // coordinate delta is a real spatial relationship rather than an
        // accident of which block `derive_coords` laid down first. Anything
        // farther is dropped rather than pointed at with a meaningless
        // direction (see CONTEXT.md §11); the camera could never pan there
        // anyway, since `MapCamera::pan` clamps to the same `PAN_LIMIT`.
        if (c.x - center.x).abs() > PAN_LIMIT || (c.y - center.y).abs() > PAN_LIMIT {
            continue;
        }
        let sc = cx + 2 * (c.x - center.x);
        let sr = cy + 2 * (c.y - center.y);
        if (0..cols).contains(&sc) && (0..rows).contains(&sr) {
            continue; // on-screen: the canvas (or fog) handles it
        }
        let glyph = arrow_glyph(c.x - center.x, c.y - center.y);
        let key = (
            sr.clamp(0, rows - 1) as usize,
            sc.clamp(0, cols - 1) as usize,
        );
        let entry = by_cell.entry(key).or_insert((glyph, is_boss));
        if is_boss && !entry.1 {
            *entry = (glyph, true); // a boss outranks a tame arrow on the same cell
        }
    }
    by_cell
        .into_iter()
        .map(|((row, col), (glyph, boss))| MapArrow {
            row,
            col,
            glyph,
            boss,
        })
        .collect()
}

/// Border arrows for active-quest target rooms that are off-screen on the
/// viewed level, honoring the same `PAN_LIMIT` honesty filter as `poi_arrows`:
/// a target in another reserved block gets no arrow at all, because across
/// blocks the coordinate delta points nowhere real. The count of targets
/// dropped that way is returned alongside, so the map can say "N marks lie
/// beyond this land" instead of silently showing fewer quests than exist.
pub fn quest_arrows(
    coords: &HashMap<RoomId, Coord>,
    center: Coord,
    cols: i32,
    rows: i32,
    targets: &[RoomId],
) -> (Vec<MapArrow>, usize) {
    if cols <= 0 || rows <= 0 {
        return (Vec::new(), targets.len());
    }
    let cx = cols / 2;
    let cy = rows / 2;
    let mut by_cell: BTreeMap<(usize, usize), char> = BTreeMap::new();
    let mut beyond = 0usize;
    for room in targets {
        let Some(&c) = coords.get(room) else {
            beyond += 1;
            continue;
        };
        if c.z != center.z
            || (c.x - center.x).abs() > PAN_LIMIT
            || (c.y - center.y).abs() > PAN_LIMIT
        {
            beyond += 1;
            continue;
        }
        let sc = cx + 2 * (c.x - center.x);
        let sr = cy + 2 * (c.y - center.y);
        if (0..cols).contains(&sc) && (0..rows).contains(&sr) {
            continue; // on-screen: the canvas draws the quest marker itself
        }
        let glyph = arrow_glyph(c.x - center.x, c.y - center.y);
        by_cell
            .entry((
                sr.clamp(0, rows - 1) as usize,
                sc.clamp(0, cols - 1) as usize,
            ))
            .or_insert(glyph);
    }
    let arrows = by_cell
        .into_iter()
        .map(|((row, col), glyph)| MapArrow {
            row,
            col,
            glyph,
            boss: false,
        })
        .collect();
    (arrows, beyond)
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

/// Two zones pressed together on screen with no gate between the touching
/// rooms: the signature of a fold, where summed exit steps carry a far-away
/// corridor back beside an unrelated place (the Sunken Glade wing climbing the
/// column next to Embergate). One entry per zone pair, worst first.
#[derive(Debug)]
pub struct Interleave {
    pub zone_a: &'static str,
    pub zone_b: &'static str,
    /// Room pairs from the two zones within one cell of each other on the
    /// same level, yet more than `FOLD_WALK_LIMIT` real moves apart.
    pub touching: usize,
    /// One pair to look at: (room in `zone_a`, room in `zone_b`), the
    /// smallest ids among the touching pairs so the report is stable.
    pub example: (RoomId, RoomId),
    /// Moves between the example rooms walking real exits; `None` if no path.
    pub walk: Option<usize>,
}

/// How many real moves apart two rooms drawn side by side may be before that
/// closeness counts as a lie. Rooms around the corner from a shared gate sit
/// diagonal to each other with no direct exit and are a couple of moves apart;
/// that is honest geometry, not a fold.
const FOLD_WALK_LIMIT: usize = 6;

/// Zone pair (ordered by name) mapped to how many cells they touch in and the
/// lowest-id room pair witnessing the fold.
type FoldTally = BTreeMap<(&'static str, &'static str), (usize, (RoomId, RoomId))>;

/// Scan the coordinate field for zone folds. Rooms of different zones sitting
/// within one cell of each other (same z) read as one connected place on the
/// map, so unless they really are a few moves apart (`FOLD_WALK_LIMIT`), that
/// closeness is a lie the embedding tells. An empty report means every
/// apparent neighbourhood on the map is walkable.
pub fn zone_interleaves(world: &World, coords: &HashMap<RoomId, Coord>) -> Vec<Interleave> {
    let mut by_cell: HashMap<(i32, i32, i32), Vec<RoomId>> = HashMap::new();
    for (&rid, &c) in coords {
        by_cell.entry((c.x, c.y, c.z)).or_default().push(rid);
    }

    let mut pairs: FoldTally = BTreeMap::new();
    for (&rid, &c) in coords {
        let Some(room) = world.room(rid) else {
            continue;
        };
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(cell) = by_cell.get(&(c.x + dx, c.y + dy, c.z)) else {
                    continue;
                };
                for &other in cell {
                    // Visit each unordered pair once.
                    if other <= rid {
                        continue;
                    }
                    let Some(there) = world.room(other) else {
                        continue;
                    };
                    if there.zone == room.zone {
                        continue;
                    }
                    // Rooms genuinely a few steps apart may draw side by side.
                    if walk_within(world, rid, other, FOLD_WALK_LIMIT) {
                        continue;
                    }
                    let (key, sample) = if room.zone <= there.zone {
                        ((room.zone, there.zone), (rid, other))
                    } else {
                        ((there.zone, room.zone), (other, rid))
                    };
                    let entry = pairs.entry(key).or_insert((0, sample));
                    entry.0 += 1;
                    entry.1 = entry.1.min(sample);
                }
            }
        }
    }

    let mut out: Vec<Interleave> = pairs
        .into_iter()
        .map(|((zone_a, zone_b), (touching, example))| Interleave {
            zone_a,
            zone_b,
            touching,
            example,
            walk: walk_distance(world, example.0, example.1),
        })
        .collect();
    out.sort_by(|l, r| {
        r.touching
            .cmp(&l.touching)
            .then(l.zone_a.cmp(r.zone_a))
            .then(l.zone_b.cmp(r.zone_b))
    });
    out
}

/// Whether `to` is reachable from `from` within `limit` moves. A bounded BFS,
/// cheap enough to run per touching pair.
fn walk_within(world: &World, from: RoomId, to: RoomId, limit: usize) -> bool {
    let mut seen: HashSet<RoomId> = HashSet::from([from]);
    let mut queue: VecDeque<(RoomId, usize)> = VecDeque::from([(from, 0)]);
    while let Some((rid, steps)) = queue.pop_front() {
        if rid == to {
            return true;
        }
        if steps == limit {
            continue;
        }
        let Some(room) = world.room(rid) else {
            continue;
        };
        for &dest in room.exits.values() {
            if seen.insert(dest) {
                queue.push_back((dest, steps + 1));
            }
        }
    }
    false
}

/// Shortest path in moves between two rooms, walking real exits.
fn walk_distance(world: &World, from: RoomId, to: RoomId) -> Option<usize> {
    let mut seen: HashSet<RoomId> = HashSet::from([from]);
    let mut queue: VecDeque<(RoomId, usize)> = VecDeque::from([(from, 0)]);
    while let Some((rid, steps)) = queue.pop_front() {
        if rid == to {
            return Some(steps);
        }
        let Some(room) = world.room(rid) else {
            continue;
        };
        for &dest in room.exits.values() {
            if seen.insert(dest) {
                queue.push_back((dest, steps + 1));
            }
        }
    }
    None
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

// ---- The land graph: which countries touch which -------------------------
//
// A schematic, land-level view of the world for the map's second page. Derived
// from the room graph and the atlas regions and nothing else: an edge exists
// exactly where one room's exit lands in another region. It therefore knows
// nothing about titles, bosses, or levels, and so cannot drift out of step
// with the real gates in `svc::can_cross_progression_gate`. What a player
// learns from it is where a land hangs, never what opens it.

/// Region name -> the regions its rooms walk into, in atlas order. A region
/// with no walking neighbours at all (the portal villages and the archipelago
/// islands, which carry no directional exits) maps to an empty list.
static LAND_LINKS: LazyLock<BTreeMap<&'static str, Vec<&'static str>>> =
    LazyLock::new(|| derive_land_links(world()));

pub fn land_links() -> &'static BTreeMap<&'static str, Vec<&'static str>> {
    &LAND_LINKS
}

/// The lands no road reaches, in atlas order: the ones whose rooms hold no
/// directional exit into another region at all, so a waystone is the only way
/// in. Derived like everything else here, never listed by hand.
pub fn portal_lands() -> Vec<&'static str> {
    super::world::region_names()
        .into_iter()
        .filter(|name| LAND_LINKS.get(name).is_none_or(Vec::is_empty))
        .collect()
}

fn region_order() -> HashMap<&'static str, usize> {
    super::world::region_names()
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i))
        .collect()
}

fn derive_land_links(world: &World) -> BTreeMap<&'static str, Vec<&'static str>> {
    let order = region_order();
    let mut links: BTreeMap<&'static str, HashSet<&'static str>> =
        order.keys().map(|name| (*name, HashSet::new())).collect();
    for room in world.rooms.values() {
        let Some((here, _)) = super::world::region_atlas_entry(room.id) else {
            continue;
        };
        for dest in room.exits.values() {
            let Some((there, _)) = super::world::region_atlas_entry(*dest) else {
                continue;
            };
            if there != here {
                links.entry(here).or_default().insert(there);
            }
        }
    }
    links
        .into_iter()
        .map(|(name, set)| {
            let mut neighbours: Vec<&'static str> = set.into_iter().collect();
            neighbours.sort_by_key(|n| order.get(n).copied().unwrap_or(usize::MAX));
            (name, neighbours)
        })
        .collect()
}

#[cfg(test)]
#[path = "worldmap_test.rs"]
mod worldmap_test;
