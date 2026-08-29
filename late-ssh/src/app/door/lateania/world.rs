// Static world definition for Lateania.
//
// Rooms and mob spawns are immutable data, loaded once into the service. This
// seed is 110 rooms spanning nine zones, a continuous descent from the hub town
// of Embergate down through forest, caverns, crypts, mines, an ice peak, a
// sunken citadel, and finally the demon realm of the Obsidian Throne. Each zone
// past the safe hub has regular mobs plus a named boss, scaled by tier.
//
// Zone layout (room id ranges):
//   1-5    Embergate (safe hub)            6-10   King's Road      (tier 1-2)
//   11-30  Whisperwood        (tier 2-3)   31-50  Duskhollow Caverns (tier 3-4)
//   51-65  Drowned Crypts     (tier 4-5)   66-80  Emberpeak Mines  (tier 5-6)
//   81-95  Frostspire Ascent  (tier 6-7)   96-105 The Sunken Citadel (tier 7-8)
//   106-110 The Obsidian Throne (tier 9-10, final boss Mal'gareth)
//
// Content is deliberately data, not code: `seed_world` hardcodes the world, but
// the shape (rooms keyed by id, exits as a direction map) is exactly what a
// future TOML/RON loader will produce. The current authored world has 198 rooms;
// the planned full design target remains 200.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{LazyLock, OnceLock};

use super::damage::{DamageProfile, DamageType, ZoneTheme};
use super::skills::{CraftSkill, GatherSkill};

// ---- Core world types: directions, rooms, spawns, behaviour --------------

/// Compass and vertical directions a player can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl Dir {
    pub fn label(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::North => "n",
            Self::South => "s",
            Self::East => "e",
            Self::West => "w",
            Self::Up => "u",
            Self::Down => "d",
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }

    /// Offset on the overhead map, in (east+, south+) grid steps. Vertical exits
    /// (up/down) have no place on a flat map and return `None`.
    pub fn delta_2d(self) -> Option<(i32, i32)> {
        Some(match self {
            Self::North => (0, -1),
            Self::South => (0, 1),
            Self::East => (1, 0),
            Self::West => (-1, 0),
            Self::Up | Self::Down => return None,
        })
    }

    /// A single compass-arrow glyph for this direction, distinct from the
    /// `▴`/`▾` stair markers (those mean "a staircase is here"; this means
    /// "go this way") so the two never read as the same thing on screen.
    pub fn compass_glyph(self) -> char {
        match self {
            Self::North => '\u{2191}', // ↑
            Self::South => '\u{2193}', // ↓
            Self::East => '\u{2192}',  // →
            Self::West => '\u{2190}',  // ←
            Self::Up => '\u{2B06}',    // ⬆
            Self::Down => '\u{2B07}',  // ⬇
        }
    }
}

pub type RoomId = u32;

/// One authored zone row shared by every continent's `*_ZONES_DATA` table:
/// (zone, adjective, ground, landmark, creatures, three mob names, boss).
type ZoneData = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    [&'static str; 3],
    &'static str,
);

/// Aelunor's glade row: same shape as [`ZoneData`], except the three mob slots
/// are indices into the shared affixed-beast table rather than names.
type GladeData = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    [usize; 3],
    &'static str,
);

/// A single location in the world: a node in the room graph.
#[derive(Clone, Debug)]
pub struct Room {
    pub id: RoomId,
    pub name: &'static str,
    pub desc: &'static str,
    pub zone: &'static str,
    pub exits: HashMap<Dir, RoomId>,
    /// True for towns and other no-combat zones.
    pub safe: bool,
    /// True for a Wildbound-style contested zone where adventurers can fight
    /// each other, not just mobs (see `svc::engage_player`). Never true
    /// together with `safe` - a room is either a haven or a battleground.
    pub pvp: bool,
}

/// A mob template that spawns at a home room.
#[derive(Clone, Debug)]
pub struct MobSpawn {
    pub id: u32,
    pub name: &'static str,
    pub home: RoomId,
    pub max_hp: i32,
    pub damage: i32,
    pub xp: i32,
    /// Seconds before a slain mob respawns.
    pub respawn_secs: u64,
    /// Item ids this mob can drop. Regular mobs have a chance at common gear;
    /// bosses are guaranteed to drop one item from a richer table.
    pub loot: &'static [u32],
    /// True for zone bosses: drops are guaranteed and announced loudly.
    pub boss: bool,
    /// Damage school dealt, plus resisted and weak schools, for interactive combat.
    pub profile: DamageProfile,
}

impl MobSpawn {
    /// The displayed level: "come at this level". A crown reads the target
    /// it is tuned to fall at (`CROWNS`). Everything else reads by its bite:
    /// the level of the prepared character whose crown hits like this,
    /// discounted because a crown is tuned to out-hit its land (a regular's
    /// damage is derived for a 20-tick kill, a zone boss's for 14, the crown's
    /// for 11: `TRASH_BITE_PCT` / `BOSS_BITE_PCT`). Health does not enter it:
    /// a sponge is a longer fight, not a deadlier one. See `level_for_bite`.
    pub fn level(&self) -> i32 {
        if let Some(crown) = CROWNS.iter().find(|c| c.name == self.name) {
            return crown.level;
        }
        let share = if self.boss {
            BOSS_BITE_PCT
        } else {
            TRASH_BITE_PCT
        };
        level_for_bite(self.damage * 100 / share)
    }

    /// A rarity rank (matching the item palette: common/uncommon/rare/epic/
    /// legendary) used to colour the name. Bosses are always legendary; regular
    /// foes scale with level.
    pub fn rank(&self) -> &'static str {
        if self.boss {
            return "legendary";
        }
        match self.level() {
            0..=5 => "common",
            6..=11 => "uncommon",
            12..=19 => "rare",
            20..=31 => "epic",
            _ => "legendary",
        }
    }
}

/// What a mob *does*, beyond standing at its home and trading blows. Stored in a
/// side map (`World::behaviors`) keyed by spawn id so the 37 hand-authored
/// `MobSpawn` literals stay untouched, the same layering the wildlife system
/// uses. A spawn with no entry behaves as [`MobBehavior::Sentinel`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MobBehavior {
    /// Holds its room and only fights when engaged (the legacy behavior).
    #[default]
    Sentinel,
    /// Wanders to a random adjacent room on a cooldown when no one is fighting it.
    Wanderer,
    /// Paces between rooms, leashing back toward its home if it strays too far.
    Patroller,
    /// Stalks the nearest player: steps toward them and gives chase if they flee.
    Hunter,
    /// Hidden from the room view until a player enters, then strikes first.
    Ambusher,
    /// Flees to an adjacent room when its health drops below a third.
    Skirmisher,
    /// Hurls a damage-school attack of its own each combat round.
    Caster(DamageType),
    /// Calls a short-lived add into the fight when first engaged.
    Summoner,
    /// Drags the other mobs sharing its room into the fight when engaged.
    PackHunter,
    /// Hits harder the closer it is to death.
    Brute,
    /// Snatches some of the player's gold, then bolts.
    Thief,
}

/// The immutable world: every room plus the mob roster and per-mob behaviors.
#[derive(Clone, Debug)]
pub struct World {
    pub rooms: HashMap<RoomId, Room>,
    pub spawns: Vec<MobSpawn>,
    pub start_room: RoomId,
    /// Spawn id -> behavior. Missing entries are [`MobBehavior::Sentinel`].
    pub behaviors: HashMap<u32, MobBehavior>,
    /// Zone name -> (min, max) displayed level of the mobs homed there, derived
    /// once at seed time from the spawns themselves so it can never drift from
    /// the real danger. Zones with no mobs (towns, havens) have no entry.
    zone_bands: HashMap<&'static str, (i32, i32)>,
}

// ---- The atlas regions: what counts as a land, and how deep --------------

/// One region's exploration line in the world atlas.
#[derive(Clone, Copy, Debug)]
pub struct RegionProgress {
    pub name: &'static str,
    /// A short danger/kind tag, e.g. "safe", "wilds", "endgame", "sea".
    pub tier: &'static str,
    /// How to reach it, e.g. "the roads", "portal".
    pub note: &'static str,
    /// Rooms in the region that currently exist.
    pub total: usize,
    /// Of those, how many the player has set foot in.
    pub explored: usize,
    /// Whether the player currently stands in this region.
    pub here: bool,
    /// Named bosses lairing in the region (where the great loot is).
    pub bosses: usize,
    /// The (min, max) displayed level of the mobs homed in the region, or None
    /// where nothing hostile lives.
    pub levels: Option<(i32, i32)>,
    /// For a region built as a chain of zones (`LAND_CHAINS`): how many of its
    /// zones the player has set foot in, and how many there are. `None` for a
    /// region with no chain, which the land map then draws as a single node.
    /// This is depth, not room count: a land can be 3 zones deep on 2% of its
    /// rooms, and depth is the number that tells a player how far they are.
    pub chain: Option<(usize, usize)>,
}

/// The world's major regions for the atlas, each `(name, id-lo, id-hi, tier,
/// how-to-reach)`. Ranges match the id blocks the generators use; the atlas
/// counts real rooms and visited rooms within each, so it stays correct as
/// regions grow. Ordered roughly by the journey outward.
const REGIONS: &[(&str, RoomId, RoomId, &str, &str)] = &[
    (
        "Embergate & the King's Road",
        1,
        600,
        "safe / low",
        "your home",
    ),
    ("The Overworld & Capitals", 600, 2000, "wilds", "the roads"),
    ("City Districts", 3000, 3100, "safe", "off the capitals"),
    (
        "The Sunken Catacombs",
        5000,
        5200,
        "endgame",
        "off Tasmania",
    ),
    ("Thornwood Hollows", 5200, 5400, "endgame", "off Melvanala"),
    (
        "The Drowned Caverns",
        5400,
        5600,
        "endgame",
        "off Matlatesh",
    ),
    (
        "Hearthward Close",
        super::housing::HOUSING_BASE,
        super::housing::HOUSING_BASE + 1000,
        "safe / home",
        "off Market Row",
    ),
    ("The Frontier", 2000, 3000, "brutal", "the sealed stair"),
    (
        "The Sundered Reaches",
        10_000,
        11_000,
        "brutal",
        "the Matlatesh sea-gate",
    ),
    (
        "Kaelmyr, the Ashen Reach",
        KAELMYR_BASE,
        KAELMYR_BASE + KAELMYR_ZONES as RoomId * KAELMYR_ZONE_STRIDE,
        "endgame",
        "the Yssgar ash-gate",
    ),
    (
        "The Sunderlakes",
        16_000,
        18_000,
        "peaceful / fishing",
        "off the Melvanala lake",
    ),
    (
        "Broceliande, the Greenwood",
        22_000,
        24_000,
        "moderate / taming",
        "off the Verdant Highlands",
    ),
    (
        "Aelunor, the Faewood",
        AELUNOR_BASE,
        AELUNOR_BASE + AELUNOR_ZONES as RoomId * AELUNOR_ZONE_STRIDE,
        "moderate / taming",
        "off the Amber Savanna",
    ),
    (
        "Silvael",
        SILVAEL_BASE,
        SILVAEL_BASE + SILVAEL_ROOM_COUNT,
        "safe / city",
        "the Faewood's own threshold",
    ),
    (
        "Portal Villages",
        super::archipelago::VILLAGE_BASE,
        super::archipelago::VILLAGE_BASE + 1000,
        "safe",
        "portal",
    ),
    (
        "The Shattered Archipelago",
        super::archipelago::ARCH_BASE,
        super::archipelago::ARCH_BASE
            + super::archipelago::ISLAND_COUNT as RoomId * super::archipelago::ARCH_STRIDE,
        "deadly",
        "portal",
    ),
    (
        "The Wildbound Waste",
        WILDBOUND_BASE,
        WILDBOUND_BASE + 3 * WILDBOUND_BIOME_STRIDE,
        "pvp",
        "the Sand-Wyrm's Maw",
    ),
    (
        "Wayfarer's Hollow",
        TUTORIAL_BASE,
        TUTORIAL_BASE + 5,
        "safe / tutorial",
        "Embergate's square",
    ),
];

/// Every atlas region name, in the order the atlas lists them (roughly the
/// journey outward). The land map lays its tree out in this order, so the two
/// views read in the same sequence.
pub fn region_names() -> Vec<&'static str> {
    REGIONS.iter().map(|&(name, ..)| name).collect()
}

/// The regions built as a chain of zones, each `(region name, first room,
/// rooms reserved per zone, zone count)`. Only the name is written out here;
/// every number comes from the generator's own consts, so a land that grows a
/// zone grows here too. A land absent from this table draws as a single node.
const LAND_CHAINS: &[(&str, RoomId, RoomId, usize)] = &[
    (
        "The Frontier",
        FRONTIER_BASE,
        FRONTIER_W * FRONTIER_H,
        FRONTIER_ZONES,
    ),
    (
        "The Sundered Reaches",
        REACHES_BASE,
        REACHES_ZONE_STRIDE,
        REACHES_ZONES,
    ),
    (
        "Kaelmyr, the Ashen Reach",
        KAELMYR_BASE,
        KAELMYR_ZONE_STRIDE,
        KAELMYR_ZONES,
    ),
    (
        "The Sunderlakes",
        LAKES_BASE,
        LAKES_ZONE_STRIDE,
        LAKES_ZONES,
    ),
    (
        "Broceliande, the Greenwood",
        BROCELIANDE_BASE,
        BROCELIANDE_ZONE_STRIDE,
        BROCELIANDE_ZONES,
    ),
    (
        "Aelunor, the Faewood",
        AELUNOR_BASE,
        AELUNOR_ZONE_STRIDE,
        AELUNOR_ZONES,
    ),
    (
        "The Wildbound Waste",
        WILDBOUND_BASE,
        WILDBOUND_BIOME_STRIDE,
        3,
    ),
];

/// How deep into a chained land the player has walked: zones with at least one
/// visited room, out of the land's zone count. `None` for an unchained land.
fn chain_depth(region: &str, visited: &HashSet<RoomId>) -> Option<(usize, usize)> {
    let &(_, base, stride, zones) = LAND_CHAINS.iter().find(|(name, ..)| *name == region)?;
    let entered = (0..zones)
        .filter(|z| {
            let lo = base + (*z as RoomId) * stride;
            visited.iter().any(|id| (lo..lo + stride).contains(id))
        })
        .count();
    Some((entered, zones))
}

// ---- World queries: rooms, zones, and atlas progress ---------------------

impl World {
    /// The behavior assigned to a spawn id, defaulting to `Sentinel`.
    pub fn behavior_of(&self, spawn_id: u32) -> MobBehavior {
        self.behaviors.get(&spawn_id).copied().unwrap_or_default()
    }
}

impl World {
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }

    /// The (min, max) displayed level of the mobs homed in a zone, or None for
    /// zones without mobs (towns, havens). One glance answers "do I belong
    /// here" - the whole world is self-labelling, no authored data to drift.
    pub fn zone_band(&self, zone: &str) -> Option<(i32, i32)> {
        self.zone_bands.get(zone).copied()
    }

    /// The whole-world atlas: exploration progress for every major region. For
    /// each region it reports how many of its rooms you've set foot in versus how
    /// many exist, how many named bosses lair there (where the great loot is),
    /// and a danger tier. A region you've never entered reads as undiscovered.
    pub fn region_progress(
        &self,
        visited: &HashSet<RoomId>,
        current: RoomId,
    ) -> Vec<RegionProgress> {
        REGIONS
            .iter()
            .map(|&(name, lo, hi, tier, note)| {
                let total = self.rooms.keys().filter(|id| (lo..hi).contains(id)).count();
                let explored = visited.iter().filter(|id| (lo..hi).contains(id)).count();
                let bosses = self
                    .spawns
                    .iter()
                    .filter(|s| s.boss && (lo..hi).contains(&s.home))
                    .count();
                let levels = self
                    .spawns
                    .iter()
                    .filter(|s| (lo..hi).contains(&s.home))
                    .map(|s| s.level())
                    .fold(None, |band: Option<(i32, i32)>, l| match band {
                        Some((min, max)) => Some((min.min(l), max.max(l))),
                        None => Some((l, l)),
                    });
                RegionProgress {
                    name,
                    tier,
                    note,
                    total,
                    explored: explored.min(total),
                    here: (lo..hi).contains(&current),
                    bosses,
                    levels,
                    chain: chain_depth(name, visited),
                }
            })
            .collect()
    }

    /// Build an overhead minimap centred on `current`, spanning `hr` rooms east
    /// and west and `vr` rooms north and south. Visited rooms are drawn solid;
    /// an unvisited room one step from a drawn room becomes a faint frontier
    /// marker so the player can see where there is still to explore. Up/down
    /// exits can't be placed on a flat plane, so they're reported as flags.
    /// Lay visited rooms onto an integer grid by walking exits out from the
    /// current room. BFS, so the shortest path to each room wins any clash
    /// that the world's non-Euclidean geometry might otherwise create.
    /// Exposed for the walkability-invariant test.
    pub(crate) fn minimap_coords(
        &self,
        current: RoomId,
        visited: &HashSet<RoomId>,
        hr: i32,
        vr: i32,
    ) -> HashMap<RoomId, (i32, i32)> {
        let mut coords: HashMap<RoomId, (i32, i32)> = HashMap::new();
        // One room per cell: when the world's folds walk two rooms onto the
        // same square, the first keeps it and the loser stays undrawn (its
        // exits then read as frontier hints, never as another room's lines).
        let mut taken: HashSet<(i32, i32)> = HashSet::new();
        coords.insert(current, (0, 0));
        taken.insert((0, 0));
        let mut queue = VecDeque::from([current]);
        while let Some(rid) = queue.pop_front() {
            let (x, y) = coords[&rid];
            let Some(room) = self.room(rid) else { continue };
            for (dir, &dest) in &room.exits {
                let Some((dx, dy)) = dir.delta_2d() else {
                    continue;
                };
                let (nx, ny) = (x + dx, y + dy);
                if nx.abs() > hr || ny.abs() > vr {
                    continue;
                }
                if !visited.contains(&dest)
                    || coords.contains_key(&dest)
                    || taken.contains(&(nx, ny))
                {
                    continue;
                }
                coords.insert(dest, (nx, ny));
                taken.insert((nx, ny));
                queue.push_back(dest);
            }
        }
        coords
    }

    pub fn minimap(
        &self,
        current: RoomId,
        previous: Option<RoomId>,
        visited: &HashSet<RoomId>,
        hr: i32,
        vr: i32,
    ) -> MiniMap {
        let coords = self.minimap_coords(current, visited, hr, vr);

        // 2. Paint rooms, corridors, and frontier markers. The char grid
        //    interleaves room cells (even indices) with connector cells (odd),
        //    so a (2hr+1) x (2vr+1) room viewport becomes a (4hr+1) x (4vr+1) grid.
        let gw = (2 * hr + 1) as usize * 2 - 1;
        let gh = (2 * vr + 1) as usize * 2 - 1;
        let mut grid = vec![vec![MapCell::Empty; gw]; gh];
        let to_cell = |x: i32, y: i32| (((y + vr) * 2) as usize, ((x + hr) * 2) as usize);

        for (&rid, &(x, y)) in &coords {
            let (r, c) = to_cell(x, y);
            grid[r][c] = if rid == current {
                MapCell::Current
            } else if Some(rid) == previous {
                MapCell::Previous
            } else {
                MapCell::Visited
            };
        }

        for (&rid, &(x, y)) in &coords {
            let Some(room) = self.room(rid) else { continue };
            let (r, c) = to_cell(x, y);
            for (dir, &dest) in &room.exits {
                let Some((dx, dy)) = dir.delta_2d() else {
                    continue;
                };
                let (nx, ny) = (x + dx, y + dy);
                if nx.abs() > hr || ny.abs() > vr {
                    continue;
                }
                let (nr, nc) = to_cell(nx, ny);
                // The iron rule of the map: a drawn line means you can walk it.
                match coords.get(&dest) {
                    // The exit's destination really is the neighbouring cell:
                    // a truthful corridor.
                    Some(&(px, py)) if (px, py) == (nx, ny) => {
                        draw_connector(&mut grid[(r + nr) / 2][(c + nc) / 2], dx, dy);
                    }
                    // The destination is visited but the world's non-Euclidean
                    // folds laid it elsewhere. Drawing a line here would join
                    // two rooms that are NOT linked (the "phantom corridor"
                    // that walks you into "You can't go north") - draw nothing.
                    Some(_) => {}
                    // A corridor leaving the visited set points at somewhere
                    // new - but only onto an empty cell, so it never appears
                    // to join an unrelated room that happens to sit there.
                    None => {
                        if grid[nr][nc] == MapCell::Empty {
                            draw_connector(&mut grid[(r + nr) / 2][(c + nc) / 2], dx, dy);
                            grid[nr][nc] = MapCell::Frontier;
                        }
                    }
                }
            }
        }

        if let Some(previous) = previous
            && let Some(&(px, py)) = coords.get(&previous)
            && (px, py) != (0, 0)
            && px.abs() <= 1
            && py.abs() <= 1
        {
            let (pr, pc) = to_cell(px, py);
            let (cr, cc) = to_cell(0, 0);
            draw_trail_connector(&mut grid[(pr + cr) / 2][(pc + cc) / 2], -px, -py);
        }

        let exits = self.room(current).map(|room| &room.exits);
        MiniMap {
            grid,
            up: exits.is_some_and(|e| e.contains_key(&Dir::Up)),
            down: exits.is_some_and(|e| e.contains_key(&Dir::Down)),
        }
    }
}

// ---- The minimap grid drawn in the room panel ----------------------------

/// What a single char-cell of the overhead minimap shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapCell {
    /// Nothing drawn here.
    Empty,
    /// The room the player is standing in.
    Current,
    /// A room the player has already visited.
    Visited,
    /// The room the player just came from.
    Previous,
    /// An unvisited room one step from somewhere visited - left to explore.
    Frontier,
    /// A horizontal corridor (`-`).
    ConnH,
    /// A vertical corridor (`|`).
    ConnV,
    /// Highlighted connector from the previous room to the current room.
    TrailH,
    TrailV,
}

/// A small overhead map of the explored neighbourhood, ready to paint in the
/// side panel. `grid[row][col]` is a char-cell; `up`/`down` flag vertical exits
/// from the current room that a flat map cannot draw.
#[derive(Clone, Debug, Default)]
pub struct MiniMap {
    pub grid: Vec<Vec<MapCell>>,
    pub up: bool,
    pub down: bool,
}

/// Lay a corridor glyph into a connector cell. Room cells and matching prior
/// corridors are left untouched.
fn draw_connector(cell: &mut MapCell, dx: i32, _dy: i32) {
    let drawn = if dx == 0 {
        MapCell::ConnV
    } else {
        MapCell::ConnH
    };
    *cell = match (*cell, drawn) {
        (MapCell::Empty, glyph) => glyph,
        (existing, _) => existing,
    };
}

fn draw_trail_connector(cell: &mut MapCell, dx: i32, _dy: i32) {
    let drawn = if dx == 0 {
        MapCell::TrailV
    } else {
        MapCell::TrailH
    };
    *cell = drawn;
}

// ---- Lookable room features (the "look at things" layer) ------------------
//
// A Feature is a thing in a room a player must LOOK at to read its description -
// fountains, plaques, distant vistas, scenery. Features are keyed to a room id
// exactly like shops (see items::shop_at), so adding them never disturbs the
// room table or its authored entries.

/// The town squares of the three capitals, each home to a healing fountain and
/// the builder's dedication plaque. These ids are the first (square) room of
/// each capital wing built in `extend_overworld`.
pub const TASMANIA_SQUARE: RoomId = 620;
pub const MELVANALA_SQUARE: RoomId = 660;
pub const MATLATESH_SQUARE: RoomId = 720;

/// Wayfarer's Hollow, the new-player tutorial zone: a five-room hub (hollow
/// plus one room per core system) hung off Embergate's square. Every
/// brand-new character spawns here (`svc::join` calls [`tutorial_start_room`]
/// instead of using `World::start_room`, which stays Embergate's square so
/// map anchoring, recall, and every other "home is room 1" assumption is
/// untouched); a returning character's saved room is unaffected.
pub const TUTORIAL_BASE: RoomId = 40_000;

/// Where a brand-new character first stands. See [`TUTORIAL_BASE`].
pub fn tutorial_start_room() -> RoomId {
    TUTORIAL_BASE
}

/// What kind of lookable thing a feature is. Fountains restore vitals in a safe
/// capital, banks protect gold, and the rest are pure description revealed on look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureKind {
    Scenery,
    Fountain,
    Bank,
    Plaque,
    Vista,
    /// A quest board: examine it to read it and open its picker, where you
    /// accept an open bounty or claim a finished one.
    Board,
    /// A beast stable/menagerie: examine it to open the companion vendor.
    Stable,
    /// A housing clerk: examine it to buy a deed and furnish a home.
    Housing,
    /// A crafting station (forge/workbench/tannery/alchemy lab/cooking fire):
    /// stand here and press the craft key to work its recipes.
    CraftStation(CraftSkill),
    /// A waystone portal: examine it to open the fast-travel network (villages
    /// and the isles of the Shattered Archipelago).
    Portal,
    /// A talking villager (Genesys): examine it to ask them a question and
    /// hear their one line back, sometimes plain color, sometimes a real clue.
    Villager,
}

impl FeatureKind {
    /// Short tag shown beside the feature in the Examine panel.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Scenery => "",
            Self::Fountain => "fountain",
            Self::Bank => "bank",
            Self::Plaque => "plaque",
            Self::Vista => "vista",
            Self::Board => "board",
            Self::Stable => "stable",
            Self::Housing => "clerk",
            Self::CraftStation(skill) => skill.station(),
            Self::Portal => "portal",
            Self::Villager => "villager",
        }
    }
}

/// A lookable thing in a room.
#[derive(Clone, Copy, Debug)]
pub struct Feature {
    pub room: RoomId,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: FeatureKind,
}

const fn feat(room: RoomId, name: &'static str, kind: FeatureKind, desc: &'static str) -> Feature {
    Feature {
        room,
        name,
        desc,
        kind,
    }
}

/// The builder's dedication, engraved on a plaque in every capital. A player
/// only ever reads it by choosing to look at the plaque.
const DEDICATION: &str = "A broad bronze plaque, gone green with the years and polished \
    bright only where countless hands have brushed it in passing. The engraving reads: \
    \"LATEANIA - this world was dreamed, designed, and built by Tasmania of \
    hardlygospel.github.io, raised upon late.sh and the labor of all who tend it. It was \
    made slowly and gladly, as a labor of love, so that strangers far apart might meet \
    here and find adventure together. Look long, traveller, and be welcome.\"";

/// Every capital's quest board reads the same; the runtime offers and claims the
/// bounties tied to that capital's nearby region when one is examined.
const BOARD_DESC: &str = "A weathered board of pinned notices and bounties stands in the \
    square, scrawled by frightened hands and countersigned by the town. Examine it again to \
    take up the next posting, or - if you have earned it - to claim a finished one.";

/// Kaelmyr keeps no towns; its only board is a cairn of scorched stones at the
/// ashen shore, where the survivors of the drowned Reaches and the tribes' few
/// deserters scratch their needs. The runtime offers and claims the same way.
const KAELMYR_BOARD_DESC: &str = "A cairn of scorched stones stands at the ash-gate, hung \
    with charms of bone and glass and scratched all over with the needs of the desperate: \
    survivors washed up from the drowned Reaches, deserters of the tribes, and hunters who \
    mean to walk deeper than sense allows. Examine it again to take up the next posting, or - \
    if you have earned it - to claim a finished one.";

/// Every capital keeps a stable/menagerie; the runtime opens the companion
/// vendor when one is examined, where adventurers buy and feed beasts of war.
const STABLE_DESC: &str = "A long timber stable backs onto the square, loud with the stamp \
    and call of penned beasts and warm with the smell of straw and musk. A weathered \
    beast-master leans on the rail, sizing up passers-by and their purses alike: war hounds \
    strain at their chains, a hooded hawk shifts on its block, and something larger breathes \
    in the dark at the back. Examine it to look over the companions for sale, or to feed and \
    tend the one already at your heel.";

/// Healing fountains share one description; the runtime restores vitals when one
/// is examined in a safe capital.
const FOUNTAIN_DESC: &str = "A broad fountain of pale, sea-worn stone stands at the heart \
    of the square, its tiers brimming with water so clear it seems to hold its own quiet \
    light. Travellers kneel here to wash the road from their faces, and rise with their \
    hurts closed over and their weariness gone. The old folk say the spring beneath was \
    blessed in the city's founding, and that while its waters run, no wound you carry need \
    be the end of you.";

/// Embergate's town well doubles as the recall fountain - the safe heart that
/// all roads, and the word of recall, lead back to.
const EMBERGATE_WELL_DESC: &str = "The old well stands at the square's edge beneath a little \
    tiled roof, its stones gone soft with moss and its bucket-rope worn glassy by ten thousand \
    hands. The water that rises is shockingly cold and clear, and folk say a draught of it on \
    the day you come back to Embergate sets even the deepest weariness to rights and closes \
    whatever the frontier opened in you.";

/// The bank is deliberately in the first safe room so death-risked gold can be
/// protected before pushing into harder regions.
const EMBERGATE_BANK_DESC: &str = "A narrow counting-house window has been built into the \
    old guildhall wall, guarded by iron scrollwork and a sleepy clerk with sharper eyes \
    than their posture suggests. Adventurers slide coin through the grille before heading \
    out beyond the lamps; coin left here survives whatever the road does to its owner.";

/// Every lookable feature in the world, keyed to the room it stands in.
pub const FEATURES: &[Feature] = &[
    // ---- Quest boards (one per capital, themed to its nearby region) -----
    feat(
        TASMANIA_SQUARE,
        "the bounty board",
        FeatureKind::Board,
        BOARD_DESC,
    ),
    feat(
        MELVANALA_SQUARE,
        "the bounty board",
        FeatureKind::Board,
        BOARD_DESC,
    ),
    feat(
        MATLATESH_SQUARE,
        "the bounty board",
        FeatureKind::Board,
        BOARD_DESC,
    ),
    // Kaelmyr's only safe hold: an ashland cairn-board at the Cinderfall Shore
    // ash-gate, where the continent's postings hang. Gated behind the Bane of
    // Yssgar, so only the deepest veterans ever read it.
    feat(
        KAELMYR_BASE,
        "the ash-cairn board",
        FeatureKind::Board,
        KAELMYR_BOARD_DESC,
    ),
    // ---- Stables (one per capital: the companion vendor) ----------------
    feat(1, "the war-stable", FeatureKind::Stable, STABLE_DESC),
    feat(
        TASMANIA_SQUARE,
        "the harbor menagerie",
        FeatureKind::Stable,
        STABLE_DESC,
    ),
    feat(
        MELVANALA_SQUARE,
        "the highland kennels",
        FeatureKind::Stable,
        STABLE_DESC,
    ),
    feat(
        MATLATESH_SQUARE,
        "the oasis beast-market",
        FeatureKind::Stable,
        STABLE_DESC,
    ),
    // ---- Hearthward Close (the housing district clerk) ------------------
    feat(
        super::housing::HOUSING_BASE,
        "the housing clerk",
        FeatureKind::Housing,
        "A patient clerk in an ink-stained coat keeps a tall lectern stacked with deeds, \
         plans, and a fat catalogue of furnishings. Buy a deed to claim one of the close's \
         empty homes as your own, then - standing inside it - order furniture brought in to \
         make it a home worth coming back to.",
    ),
    // ---- Embergate (the town square: recall point + safe haven) ---------
    feat(
        1,
        "the town well",
        FeatureKind::Fountain,
        EMBERGATE_WELL_DESC,
    ),
    feat(
        1,
        "the banker's grille",
        FeatureKind::Bank,
        EMBERGATE_BANK_DESC,
    ),
    // ---- Embergate crafters' row (Market Row, room 3): the craft stations ----
    feat(
        3,
        "the public forge",
        FeatureKind::CraftStation(CraftSkill::Smithing),
        "A great stone forge roars at the head of Market Row, its coals kept lit \
         for any traveller with ore to smelt and the arm to swing a hammer. Anvils, \
         tongs, and quenching-troughs stand ready; smelt ore into ingots here, then \
         beat them into blades and plate.",
    ),
    feat(
        3,
        "the carpenter's workbench",
        FeatureKind::CraftStation(CraftSkill::Woodworking),
        "A long, scarred workbench under an awning, hung with saws, drawknives and \
         clamps and drifted deep in fragrant shavings. Season your logs into planks \
         here, and shape them into bows and hafts.",
    ),
    feat(
        3,
        "the tannery",
        FeatureKind::CraftStation(CraftSkill::Leatherworking),
        "A row of stretching-frames and reeking tan-pits behind the market, where \
         raw hides are cured into supple leather. Not a place to linger downwind - \
         but the only place to turn a kill's hide into armor.",
    ),
    feat(
        3,
        "the alchemy lab",
        FeatureKind::CraftStation(CraftSkill::Alchemy),
        "A cramped stall of bubbling retorts, hanging bunches of dried herbs, and \
         shelves of stoppered vials in every colour of harm and healing. Brew \
         draughts to mend yourself here - or poisons to coat a waiting blade.",
    ),
    feat(
        3,
        "the cooking fire",
        FeatureKind::CraftStation(CraftSkill::Cooking),
        "A broad communal cook-fire with spits, griddles and a blackened stockpot, \
         where the market's traders take their meals. Cook your catch into hot food \
         that restores far more than raw fish ever could.",
    ),
    // ---- Tasmania (harbor capital) --------------------------------------
    feat(
        TASMANIA_SQUARE,
        "the harbor fountain",
        FeatureKind::Fountain,
        FOUNTAIN_DESC,
    ),
    feat(
        TASMANIA_SQUARE,
        "the bronze plaque",
        FeatureKind::Plaque,
        DEDICATION,
    ),
    feat(
        TASMANIA_SQUARE,
        "the harbor",
        FeatureKind::Vista,
        "Past the rooftops the harbor opens wide and silver, crowded with the masts of \
         fishing dhows and far-trading caravels, and beyond the breakwater the Sapphire \
         Coast curves away east into haze. A good road leads down to the water; whatever \
         you can see from here, your feet can reach.",
    ),
    // ---- Melvanala (highland lake capital) ------------------------------
    feat(
        MELVANALA_SQUARE,
        "the mountain fountain",
        FeatureKind::Fountain,
        FOUNTAIN_DESC,
    ),
    feat(
        MELVANALA_SQUARE,
        "the bronze plaque",
        FeatureKind::Plaque,
        DEDICATION,
    ),
    feat(
        MELVANALA_SQUARE,
        "the high lake",
        FeatureKind::Vista,
        "From the terraced square the land falls away to a vast mountain lake, so still it \
         holds the snow-capped peaks upside down upon its face. Switchback paths thread down \
         to its shore and on toward the Verdant Highlands; nothing you see from this height \
         is beyond a day's honest walking.",
    ),
    // ---- Matlatesh (desert capital) -------------------------------------
    feat(
        MATLATESH_SQUARE,
        "the oasis fountain",
        FeatureKind::Fountain,
        FOUNTAIN_DESC,
    ),
    feat(
        MATLATESH_SQUARE,
        "the bronze plaque",
        FeatureKind::Plaque,
        DEDICATION,
    ),
    feat(
        MATLATESH_SQUARE,
        "the desert horizon",
        FeatureKind::Vista,
        "Beyond the mud-brick walls the Sahra Wastes run gold to the edge of the world, and \
         far off a lone mesa stands against the sky like a tombstone for a giant. A caravan \
         road leaves the gate and dwindles toward it; the desert is wide, but every dune you \
         can see has a path across it.",
    ),
    // ---- Wayfarer's Hollow's Tinker's Hall: one scaled-down copy of every
    // craft station, so a newcomer can open the crafting panel immediately. --
    feat(
        TUTORIAL_BASE + 3,
        "the practice forge",
        FeatureKind::CraftStation(CraftSkill::Smithing),
        "A small forge, banked low - just hot enough to smelt a first bar of ore and \
         see how the recipe list actually works.",
    ),
    feat(
        TUTORIAL_BASE + 3,
        "the practice workbench",
        FeatureKind::CraftStation(CraftSkill::Woodworking),
        "A modest bench with one of every tool, none of them worn in yet.",
    ),
    feat(
        TUTORIAL_BASE + 3,
        "the practice tannery",
        FeatureKind::CraftStation(CraftSkill::Leatherworking),
        "A single stretching-frame, kept well clear of the real tannery's smell.",
    ),
    feat(
        TUTORIAL_BASE + 3,
        "the practice alchemy stall",
        FeatureKind::CraftStation(CraftSkill::Alchemy),
        "A tidy little rack of retorts, none of them bubbling with anything dangerous yet.",
    ),
    feat(
        TUTORIAL_BASE + 3,
        "the practice cook-fire",
        FeatureKind::CraftStation(CraftSkill::Cooking),
        "A small, well-tended fire with a spit and a single pot - enough to learn on.",
    ),
];

/// Every named villager in the world, keyed to the room they stand in - the
/// Genesys sub-expansion: a living, breathing world. Each carries one line of
/// dialogue, sometimes plain color, sometimes a real clue about where
/// something in the world can be found. Rendered as a `FeatureKind::Villager`
/// so they slot into the existing Examine (`o`) mechanism, but always
/// announced up front in the room description - a villager never hides.
pub const VILLAGERS: &[Feature] = &[
    feat(
        1,
        "a footsore town crier",
        FeatureKind::Villager,
        "New to Lateania? Market Row's south of here for the smithy and outfitter, and the temple keeps the Dawn's own healers on hand.",
    ),
    feat(
        2,
        "a barkeep wiping down the counter",
        FeatureKind::Villager,
        "The Gilded Flagon never waters its ale, whatever you've heard. Board's by the door if you're after coin work.",
    ),
    feat(
        3,
        "an apprentice smith, soot to the elbows",
        FeatureKind::Villager,
        "Bruna's Ember Forge is right through there. Ask nice and she'll tell you which blade suits your arm.",
    ),
    feat(
        4,
        "a novice of the Dawn",
        FeatureKind::Villager,
        "The Temple keeps the recall fountain lit day and night. Speak the word (r) and you'll always find your way home.",
    ),
    feat(
        5,
        "a gate-warden leaning on her spear",
        FeatureKind::Villager,
        "South Gate's the quiet way out of town. The Greatroad proper starts a few steps past the arch.",
    ),
    feat(
        201,
        "a tailor's boy with pins in his sleeve",
        FeatureKind::Villager,
        "Tomas keeps the Outfitter's Stall stocked past what you'd think - legs and boots too, these days, not just the usual.",
    ),
    feat(
        202,
        "an old woman grinding herbs",
        FeatureKind::Villager,
        "Mirela's Apothecary always has a Phoenix Tonic tucked away for adventurers who've gone in over their heads.",
    ),
    feat(
        203,
        "a sharp-eyed pickpocket, reformed (mostly)",
        FeatureKind::Villager,
        "Pell the Magpie's Curio Cart turns up rings and charms nobody else stocks. Mind your purse near him all the same.",
    ),
    feat(
        204,
        "a bank clerk counting coin",
        FeatureKind::Villager,
        "Bank your gold here before you go anywhere dangerous. Dying with a full purse is a special kind of foolish.",
    ),
    feat(
        205,
        "a watchman pacing the wall",
        FeatureKind::Villager,
        "From up here you can see clean out to the King's Road. Long way to the Frontier stair, longer back.",
    ),
    feat(
        620,
        "a harbor porter hauling crates",
        FeatureKind::Villager,
        "Ships in from three ports today. If you're hunting fish, the Sunderlakes treat a rod kinder than the open sea does.",
    ),
    feat(
        621,
        "a chandler weighing rope",
        FeatureKind::Villager,
        "The Saltwind Wharves are just down the way, if you want a proper look at the harbour district.",
    ),
    feat(
        622,
        "a fishwife crying the day's catch",
        FeatureKind::Villager,
        "Best bream in Tasmania, fresh off the boat. Mind the gulls, they've no manners at all.",
    ),
    feat(
        623,
        "a acolyte lighting storm-candles",
        FeatureKind::Villager,
        "The Cathedral keeps a candle burning for every sailor lost to the deep. Quiet a place as you'll find in this city.",
    ),
    feat(
        624,
        "a lighthouse-keeper's apprentice",
        FeatureKind::Villager,
        "Climb the stair some evening - on a clear night you can just make out the Sundered Reaches on the horizon.",
    ),
    feat(
        625,
        "a clerk with an armful of ledgers",
        FeatureKind::Villager,
        "The Governor's business is her own, but the Terrace view is free to anyone who climbs it.",
    ),
    feat(
        626,
        "a watch-captain scanning the bay",
        FeatureKind::Villager,
        "From the Watchtower Crown you can see every mast in harbour. Nothing gets in or out of Tasmania I don't know about.",
    ),
    feat(
        660,
        "a coppersmith's apprentice",
        FeatureKind::Villager,
        "Melvanala's high and cold, but the Lakeshore Square never freezes over. Something about the hot springs below.",
    ),
    feat(
        661,
        "a coppersmith hammering a kettle",
        FeatureKind::Villager,
        "The Coppersmith's Steps have been in my family three generations. Mind the wet stone in the rain.",
    ),
    feat(
        662,
        "a pilgrim resting on the stair",
        FeatureKind::Villager,
        "Long climb to the monastery. Worth it, they say, if you've a question only the quiet can answer.",
    ),
    feat(
        663,
        "a gardener pruning terrace vines",
        FeatureKind::Villager,
        "The Hanging Gardens bloom even in the frost. Nobody's quite explained how.",
    ),
    feat(
        664,
        "a monk sweeping the gate",
        FeatureKind::Villager,
        "The monastery takes in anyone who knocks, adventurer or not. Just leave your blade at the door.",
    ),
    feat(
        665,
        "a bell-ringer counting the hours",
        FeatureKind::Villager,
        "The Bell Tower rings the watches for the whole city. You get used to it. Eventually.",
    ),
    feat(
        666,
        "an old sky-priest at the ledge",
        FeatureKind::Villager,
        "The Sky-Burial Ledge is sacred ground. Melvanala sends its dead to the wind up here, not the earth.",
    ),
    feat(
        720,
        "a caravan guide counting camels",
        FeatureKind::Villager,
        "Matlatesh runs on the caravan trade. Miss the Oasis Square at dawn and you'll miss half the city's business.",
    ),
    feat(
        721,
        "a spice trader weighing saffron",
        FeatureKind::Villager,
        "The Spice Souk sells things you won't find anywhere else in Lateania. Ask about the frost-bloom, if you dare the price.",
    ),
    feat(
        722,
        "a caravanserai keeper",
        FeatureKind::Villager,
        "Beds and water for man and beast alike. The desert doesn't forgive travelers who skip a night here.",
    ),
    feat(
        723,
        "a young astronomer squinting at charts",
        FeatureKind::Villager,
        "The College maps the stars over Kaelmyr too, when the ash-clouds allow it. Strange skies out that way.",
    ),
    feat(
        724,
        "a gardener tending the water-garden",
        FeatureKind::Villager,
        "The Sultana's Garden is the coolest place in the city come midday. Even the guards linger here.",
    ),
    feat(
        725,
        "a potter shaping wet clay",
        FeatureKind::Villager,
        "Every jar in the Potter's Quarter is thrown by hand. Buy one before you head into the wastes; you'll want the water.",
    ),
    feat(
        726,
        "a muezzin descending the minaret",
        FeatureKind::Villager,
        "From the High Minaret you can see clear to the Ashen Wastes. Cold comfort, that view.",
    ),
    feat(
        2000,
        "a haggard veteran adventurer",
        FeatureKind::Villager,
        "The Frontier proper starts past this rise. Twenty zones deep and every one meaner than the last. Go in ready or don't go in at all.",
    ),
    feat(
        3000,
        "a lamplighter making his rounds",
        FeatureKind::Villager,
        "The Lamplit Quarter never really goes dark. Guildhall's just through there if you're after work.",
    ),
    feat(
        3001,
        "an off-duty guard soaking sore feet",
        FeatureKind::Villager,
        "The baths are the one place in Embergate nobody talks business. Come in swinging a sword and they'll throw you right back out.",
    ),
    feat(
        3002,
        "a scarred veteran nursing bad ale",
        FeatureKind::Villager,
        "Every company that's ever mattered started at that guildhall bar. Most of them didn't end well. Still worth a look at the boards.",
    ),
    feat(
        3003,
        "a tinker haggling over a broken lock",
        FeatureKind::Villager,
        "Tinker's Row will fix anything for the right coin, no questions asked about where it came from.",
    ),
    feat(
        3004,
        "a mourner tending the shrine garden",
        FeatureKind::Villager,
        "Folk come here to grieve or give thanks, adventurer or not. Even the noise of the square goes quiet at the gate.",
    ),
    feat(
        3010,
        "a net-mender squinting at torn twine",
        FeatureKind::Villager,
        "The Saltwind Wharves smell of brine and money changing hands. Mind the harbour cats, they run their own commerce down here.",
    ),
    feat(
        3011,
        "a fishmonger stacking crushed ice",
        FeatureKind::Villager,
        "Freshest catch in Tasmania, right here at the Fishmarket. Come early or come empty-handed.",
    ),
    feat(
        3012,
        "a cartographer inking a new coastline",
        FeatureKind::Villager,
        "Every chart in this loft is hand-drawn. Ask nicely and they might show you a corner of the map you haven't walked yet.",
    ),
    feat(
        3013,
        "a harbourmaster's clerk with tar on his hands",
        FeatureKind::Villager,
        "Nothing crosses this water the harbourmaster hasn't already written down. Best source in the city if you need to know a ship's business.",
    ),
    feat(
        3014,
        "a sailor lighting a candle before departure",
        FeatureKind::Villager,
        "The Storm-Chapel keeps a candle burning for every soul that goes down to the sea. Wind through that door sounds like a hymn some nights.",
    ),
    feat(
        3020,
        "a terrace gardener trimming frost-vines",
        FeatureKind::Villager,
        "The Hightarn Terraces catch the last of the sun before the mountain swallows it. Best view in Melvanala, and free.",
    ),
    feat(
        3021,
        "a lamplighter walking the Mirrorlake Walk",
        FeatureKind::Villager,
        "Water's so still up here it doubles the peaks. Whole terrace feels like it's floating between two skies at dusk.",
    ),
    feat(
        3022,
        "a stonecutter with dust in his beard",
        FeatureKind::Villager,
        "The Stonecutters' Court has been chipping away at that mountain for longer than the city's had a name. Watch your step, the ground's uneven with old spoil.",
    ),
    feat(
        3023,
        "a longhall regular, three drinks deep",
        FeatureKind::Villager,
        "Come in cold, leave as kin. That's the Alewife's rule, and she's never broken it once in forty years.",
    ),
    feat(
        3024,
        "a pilgrim filling a waterskin",
        FeatureKind::Villager,
        "The Snowmelt Spring never runs dry, not even in high summer. They say a sip carries off whatever's ailing you.",
    ),
    feat(
        3030,
        "a rug merchant calling out prices",
        FeatureKind::Villager,
        "The Sunbaked Bazaar never truly closes. Come at night if you want the honest prices, not the tourist ones.",
    ),
    feat(
        3031,
        "a spice-seller fanning away flies",
        FeatureKind::Villager,
        "Real frost-bloom, real saffron, real everything - the Spice Bazaar doesn't deal in the cheap stuff.",
    ),
    feat(
        3032,
        "a glassblower shaping molten sand",
        FeatureKind::Villager,
        "Every piece in the Glassblowers' Souk is one of a kind. Drop one and it's gone for good, so mind your elbows.",
    ),
    feat(
        3033,
        "a caravan master counting his camels twice",
        FeatureKind::Villager,
        "The desert doesn't forgive a caravan that leaves short a water-skin. Stock up here before you head anywhere past the walls.",
    ),
    feat(
        3034,
        "a botanist misting rare blooms",
        FeatureKind::Villager,
        "The Oasis Conservatory grows things that shouldn't survive this far from water. Nobody's quite explained how, same as the terraces up north.",
    ),
    feat(
        5000,
        "a grim-faced gravedigger",
        FeatureKind::Villager,
        "The Sunken Catacombs took my brother. Whatever's down there, it's not resting easy. Go in armed, and go in ready to leave family behind you.",
    ),
    feat(
        5200,
        "a woodcutter refusing to go further",
        FeatureKind::Villager,
        "Thornwood Hollows past this gate. My axe won't touch those brambles - they bleed something that isn't sap.",
    ),
    feat(
        5416,
        "a half-drowned fisherman, shivering",
        FeatureKind::Villager,
        "The Drowned Caverns pulled me under once and spat me back up. I don't go past the Tide Mouth anymore. You'd be wise to think twice yourself.",
    ),
    feat(
        10000,
        "a scarred sea-captain staring at the horizon",
        FeatureKind::Villager,
        "The Sundered Reaches lie past this shallows. Whatever's out there rides harder than the Frontier ever did. The King Who Was Promised Nothing was only the beginning.",
    ),
    feat(
        12000,
        "an ash-caked pilgrim at the gate",
        FeatureKind::Villager,
        "Kaelmyr keeps no towns past this shore, only ash and worse. If you're going in, go in remembering the way back out.",
    ),
    feat(
        8000,
        "a lamp-keeper trimming wicks",
        FeatureKind::Villager,
        "Lantern Cove's the quiet end of Hearthward Close. Good place to settle, if you've earned a home yet.",
    ),
    feat(
        8001,
        "a retired adventurer tending a garden",
        FeatureKind::Villager,
        "Emberfall Rest is where the old companies come to put their feet up. Nobody fights here. Nobody has to anymore.",
    ),
    feat(
        8002,
        "a mist-wrapped groundskeeper",
        FeatureKind::Villager,
        "Hollowmere's always a little foggy, even at noon. Folk say it's peaceful. I say it's just cold.",
    ),
    feat(
        8003,
        "a kite-flyer watching the wind",
        FeatureKind::Villager,
        "Best view in Hearthward Close from up here at Skyreach Landing. Worth the climb on a clear day.",
    ),
    feat(
        9000,
        "a housing clerk with a ledger under one arm",
        FeatureKind::Villager,
        "Buy a deed here whenever you're ready to plant roots - a wattle hut to start, a wizard's tower if you've the coin for it.",
    ),
    feat(
        16000,
        "a reed-cutter with wet hands",
        FeatureKind::Villager,
        "Forty species of fish swim these waters, if you've the patience for a rod and line.",
    ),
    feat(
        16088,
        "an old angler mending a net",
        FeatureKind::Villager,
        "The lakes are gentle water, not the wilds - good country to fish, rest, and not get yourself killed.",
    ),
    feat(
        16176,
        "a lantern-keeper at the dock",
        FeatureKind::Villager,
        "Every landing along these reed-mazes carries its own band of fish. Work your way through all fourteen and you'll have quite a collection.",
    ),
    feat(
        16264,
        "a heron-watcher, quiet as the water",
        FeatureKind::Villager,
        "Peaceful out here, compared to the Frontier. Nobody's come to any harm on these waters in longer than I can remember.",
    ),
    feat(
        16352,
        "a basket-weaver working reeds",
        FeatureKind::Villager,
        "The deep spring further in is said to hold something worth the effort of finding it. Bring more than a fishing rod.",
    ),
    feat(
        16453,
        "a ferry-hand poling a flat boat",
        FeatureKind::Villager,
        "Mind the fog on the caverns further along - easy to lose the path if you're not watching your step.",
    ),
    feat(
        16528,
        "a mist-wrapped hermit",
        FeatureKind::Villager,
        "A quiet trade, fishing. No monsters worth mentioning, just patience and good bait.",
    ),
    feat(
        16616,
        "a young net-mender",
        FeatureKind::Villager,
        "The lake-notables mostly keep to themselves. Leave them be and they'll do the same for you.",
    ),
    feat(
        16704,
        "a lake-warden counting boats",
        FeatureKind::Villager,
        "Forty species of fish swim these waters, if you've the patience for a rod and line.",
    ),
    feat(
        16792,
        "a duck-caller with a battered pipe",
        FeatureKind::Villager,
        "The lakes are gentle water, not the wilds - good country to fish, rest, and not get yourself killed.",
    ),
    feat(
        16880,
        "a water-witch reading ripples",
        FeatureKind::Villager,
        "Every landing along these reed-mazes carries its own band of fish. Work your way through all fourteen and you'll have quite a collection.",
    ),
    feat(
        16982,
        "a boat-builder planing wood",
        FeatureKind::Villager,
        "Peaceful out here, compared to the Frontier. Nobody's come to any harm on these waters in longer than I can remember.",
    ),
    feat(
        17056,
        "an eel-trapper checking his lines",
        FeatureKind::Villager,
        "The deep spring further in is said to hold something worth the effort of finding it. Bring more than a fishing rod.",
    ),
    feat(
        17144,
        "a still-water fisherman",
        FeatureKind::Villager,
        "Mind the fog on the caverns further along - easy to lose the path if you're not watching your step.",
    ),
    feat(
        20000,
        "a shipwrecked sailor, still shaking",
        FeatureKind::Villager,
        "These isles ride the same cruel curve as Kaelmyr, or worse. Whatever you're hunting, it'll find you first out here.",
    ),
    feat(
        20050,
        "a portal-warden clutching her waystone",
        FeatureKind::Villager,
        "The waystone network is the only safe thing about the Shattered Archipelago. Step off the landing at your own risk.",
    ),
    feat(
        20100,
        "a scavenger sorting driftwood",
        FeatureKind::Villager,
        "Every island's got its own boss, its own name, its own way of trying to kill you. Come prepared or come to grief.",
    ),
    feat(
        20150,
        "a marooned cartographer",
        FeatureKind::Villager,
        "I've seen adventurers turn back at this very landing more times than I can count. No shame in it.",
    ),
    feat(
        20200,
        "a nervous lookout",
        FeatureKind::Villager,
        "The deadliest ground in Lateania, they call it. I believe them. I've not left this landing in months.",
    ),
    feat(
        20250,
        "a salt-crusted hermit",
        FeatureKind::Villager,
        "Whatever you find out on these isles, it'll be worth more than anything the Reaches or Kaelmyr ever offered. If you survive to carry it home.",
    ),
    feat(
        20300,
        "a survivor of the last landing party",
        FeatureKind::Villager,
        "These isles ride the same cruel curve as Kaelmyr, or worse. Whatever you're hunting, it'll find you first out here.",
    ),
    feat(
        20350,
        "a bone-collector",
        FeatureKind::Villager,
        "The waystone network is the only safe thing about the Shattered Archipelago. Step off the landing at your own risk.",
    ),
    feat(
        20400,
        "a tide-reader murmuring to herself",
        FeatureKind::Villager,
        "Every island's got its own boss, its own name, its own way of trying to kill you. Come prepared or come to grief.",
    ),
    feat(
        20450,
        "a lantern-keeper who won't say why she stays",
        FeatureKind::Villager,
        "I've seen adventurers turn back at this very landing more times than I can count. No shame in it.",
    ),
    feat(
        20500,
        "a shipwrecked sailor, still shaking",
        FeatureKind::Villager,
        "The deadliest ground in Lateania, they call it. I believe them. I've not left this landing in months.",
    ),
    feat(
        20550,
        "a portal-warden clutching her waystone",
        FeatureKind::Villager,
        "Whatever you find out on these isles, it'll be worth more than anything the Reaches or Kaelmyr ever offered. If you survive to carry it home.",
    ),
    feat(
        20600,
        "a scavenger sorting driftwood",
        FeatureKind::Villager,
        "These isles ride the same cruel curve as Kaelmyr, or worse. Whatever you're hunting, it'll find you first out here.",
    ),
    feat(
        20650,
        "a marooned cartographer",
        FeatureKind::Villager,
        "The waystone network is the only safe thing about the Shattered Archipelago. Step off the landing at your own risk.",
    ),
    feat(
        20700,
        "a nervous lookout",
        FeatureKind::Villager,
        "Every island's got its own boss, its own name, its own way of trying to kill you. Come prepared or come to grief.",
    ),
    feat(
        20750,
        "a salt-crusted hermit",
        FeatureKind::Villager,
        "I've seen adventurers turn back at this very landing more times than I can count. No shame in it.",
    ),
    feat(
        20800,
        "a survivor of the last landing party",
        FeatureKind::Villager,
        "The deadliest ground in Lateania, they call it. I believe them. I've not left this landing in months.",
    ),
    feat(
        20850,
        "a bone-collector",
        FeatureKind::Villager,
        "Whatever you find out on these isles, it'll be worth more than anything the Reaches or Kaelmyr ever offered. If you survive to carry it home.",
    ),
    feat(
        20900,
        "a tide-reader murmuring to herself",
        FeatureKind::Villager,
        "These isles ride the same cruel curve as Kaelmyr, or worse. Whatever you're hunting, it'll find you first out here.",
    ),
    feat(
        20950,
        "a lantern-keeper who won't say why she stays",
        FeatureKind::Villager,
        "The waystone network is the only safe thing about the Shattered Archipelago. Step off the landing at your own risk.",
    ),
    feat(
        22000,
        "a beast-tamer resting against a mossy stone",
        FeatureKind::Villager,
        "Every beast in Broceliande can be tamed, if you've the patience for it - from the humblest hare to beasts fit to ride.",
    ),
    feat(
        22099,
        "a forester with a hound at heel",
        FeatureKind::Villager,
        "The rideable mounts roam deep in this wood. Palfreys and elks near the eaves, the truly mythical things much further in.",
    ),
    feat(
        22198,
        "a druid tending a circle of stones",
        FeatureKind::Villager,
        "Sixty beasts call this Greenwood home, small and mythical alike. Spend a season here and you'll have met most of them.",
    ),
    feat(
        22297,
        "a woodward marking trees for the season",
        FeatureKind::Villager,
        "The deeper you go, the harder the taming and the stranger the company. The World-Oak's crown holds the oldest of them all.",
    ),
    feat(
        22396,
        "a ranger stringing a new bow",
        FeatureKind::Villager,
        "Broceliande's a moderate wood, not a brutal one - but don't mistake that for safe. Something in every zone can still put you on your back.",
    ),
    feat(
        22495,
        "a beekeeper smoking a wild hive",
        FeatureKind::Villager,
        "A tamed beast strong enough to ride will carry you leagues in a single stride, if you earn its trust.",
    ),
    feat(
        22594,
        "an old huntress sharpening a knife",
        FeatureKind::Villager,
        "Every beast in Broceliande can be tamed, if you've the patience for it - from the humblest hare to beasts fit to ride.",
    ),
    feat(
        22693,
        "a herbalist gathering dew",
        FeatureKind::Villager,
        "The rideable mounts roam deep in this wood. Palfreys and elks near the eaves, the truly mythical things much further in.",
    ),
    feat(
        22807,
        "a stablehand leading a skittish colt",
        FeatureKind::Villager,
        "Sixty beasts call this Greenwood home, small and mythical alike. Spend a season here and you'll have met most of them.",
    ),
    feat(
        22891,
        "a green-robed acolyte of the Greenwood",
        FeatureKind::Villager,
        "The deeper you go, the harder the taming and the stranger the company. The World-Oak's crown holds the oldest of them all.",
    ),
    feat(
        22990,
        "a beast-tamer resting against a mossy stone",
        FeatureKind::Villager,
        "Broceliande's a moderate wood, not a brutal one - but don't mistake that for safe. Something in every zone can still put you on your back.",
    ),
    feat(
        23103,
        "a forester with a hound at heel",
        FeatureKind::Villager,
        "A tamed beast strong enough to ride will carry you leagues in a single stride, if you earn its trust.",
    ),
    feat(
        23188,
        "a druid tending a circle of stones",
        FeatureKind::Villager,
        "Every beast in Broceliande can be tamed, if you've the patience for it - from the humblest hare to beasts fit to ride.",
    ),
    feat(
        23287,
        "a woodward marking trees for the season",
        FeatureKind::Villager,
        "The rideable mounts roam deep in this wood. Palfreys and elks near the eaves, the truly mythical things much further in.",
    ),
    feat(
        23386,
        "a ranger stringing a new bow",
        FeatureKind::Villager,
        "Sixty beasts call this Greenwood home, small and mythical alike. Spend a season here and you'll have met most of them.",
    ),
    feat(
        23485,
        "a beekeeper smoking a wild hive",
        FeatureKind::Villager,
        "The deeper you go, the harder the taming and the stranger the company. The World-Oak's crown holds the oldest of them all.",
    ),
    feat(
        23584,
        "an old huntress sharpening a knife",
        FeatureKind::Villager,
        "Broceliande's a moderate wood, not a brutal one - but don't mistake that for safe. Something in every zone can still put you on your back.",
    ),
    feat(
        23707,
        "a herbalist gathering dew",
        FeatureKind::Villager,
        "A tamed beast strong enough to ride will carry you leagues in a single stride, if you earn its trust.",
    ),
    feat(
        23782,
        "a stablehand leading a skittish colt",
        FeatureKind::Villager,
        "Every beast in Broceliande can be tamed, if you've the patience for it - from the humblest hare to beasts fit to ride.",
    ),
    feat(
        23881,
        "a green-robed acolyte of the Greenwood",
        FeatureKind::Villager,
        "The rideable mounts roam deep in this wood. Palfreys and elks near the eaves, the truly mythical things much further in.",
    ),
    feat(
        600,
        "a footsore pilgrim",
        FeatureKind::Villager,
        "The King's Road runs true between the four capitals - lose your way and you've not been paying attention.",
    ),
    feat(
        601,
        "a peddler with a heavy pack",
        FeatureKind::Villager,
        "Watch for wandering game along the verges. A hunter with a keen eye eats well on this stretch.",
    ),
    feat(
        602,
        "a shepherd counting his flock",
        FeatureKind::Villager,
        "The Sunderlakes lie off toward Melvanala's lake, if fishing's more your speed than fighting.",
    ),
    feat(
        603,
        "a wandering minstrel",
        FeatureKind::Villager,
        "Broceliande hangs off the Verdant Highlands further along. Good country for taming, they say.",
    ),
    feat(
        604,
        "a tired courier",
        FeatureKind::Villager,
        "There's a sealed stair somewhere past Embergate that leads into the Frontier proper. I've never had the nerve to take it.",
    ),
    feat(
        605,
        "a farmer leading a cart",
        FeatureKind::Villager,
        "Bandits used to work this road. Haven't seen one in an age - whatever's scaring them off, I don't want to meet it either.",
    ),
    feat(
        606,
        "a road-warden on patrol",
        FeatureKind::Villager,
        "The road's safe enough by daylight. Make camp before dark if you can help it.",
    ),
    feat(
        607,
        "a tinker's apprentice",
        FeatureKind::Villager,
        "Every capital's got its own character. Tasmania smells of salt, Melvanala of cold stone, Matlatesh of spice and sand.",
    ),
    feat(
        608,
        "a traveling preacher",
        FeatureKind::Villager,
        "The King's Road runs true between the four capitals - lose your way and you've not been paying attention.",
    ),
    feat(
        640,
        "a lost-looking merchant's clerk",
        FeatureKind::Villager,
        "Watch for wandering game along the verges. A hunter with a keen eye eats well on this stretch.",
    ),
    feat(
        641,
        "a footsore pilgrim",
        FeatureKind::Villager,
        "The Sunderlakes lie off toward Melvanala's lake, if fishing's more your speed than fighting.",
    ),
    feat(
        642,
        "a peddler with a heavy pack",
        FeatureKind::Villager,
        "Broceliande hangs off the Verdant Highlands further along. Good country for taming, they say.",
    ),
    feat(
        643,
        "a shepherd counting his flock",
        FeatureKind::Villager,
        "There's a sealed stair somewhere past Embergate that leads into the Frontier proper. I've never had the nerve to take it.",
    ),
    feat(
        644,
        "a wandering minstrel",
        FeatureKind::Villager,
        "Bandits used to work this road. Haven't seen one in an age - whatever's scaring them off, I don't want to meet it either.",
    ),
    feat(
        645,
        "a tired courier",
        FeatureKind::Villager,
        "The road's safe enough by daylight. Make camp before dark if you can help it.",
    ),
    feat(
        646,
        "a farmer leading a cart",
        FeatureKind::Villager,
        "Every capital's got its own character. Tasmania smells of salt, Melvanala of cold stone, Matlatesh of spice and sand.",
    ),
    feat(
        647,
        "a road-warden on patrol",
        FeatureKind::Villager,
        "The King's Road runs true between the four capitals - lose your way and you've not been paying attention.",
    ),
    feat(
        648,
        "a tinker's apprentice",
        FeatureKind::Villager,
        "Watch for wandering game along the verges. A hunter with a keen eye eats well on this stretch.",
    ),
    feat(
        649,
        "a traveling preacher",
        FeatureKind::Villager,
        "The Sunderlakes lie off toward Melvanala's lake, if fishing's more your speed than fighting.",
    ),
    feat(
        650,
        "a lost-looking merchant's clerk",
        FeatureKind::Villager,
        "Broceliande hangs off the Verdant Highlands further along. Good country for taming, they say.",
    ),
    feat(
        680,
        "a footsore pilgrim",
        FeatureKind::Villager,
        "There's a sealed stair somewhere past Embergate that leads into the Frontier proper. I've never had the nerve to take it.",
    ),
    feat(
        681,
        "a peddler with a heavy pack",
        FeatureKind::Villager,
        "Bandits used to work this road. Haven't seen one in an age - whatever's scaring them off, I don't want to meet it either.",
    ),
    feat(
        682,
        "a shepherd counting his flock",
        FeatureKind::Villager,
        "The road's safe enough by daylight. Make camp before dark if you can help it.",
    ),
    feat(
        683,
        "a wandering minstrel",
        FeatureKind::Villager,
        "Every capital's got its own character. Tasmania smells of salt, Melvanala of cold stone, Matlatesh of spice and sand.",
    ),
    feat(
        684,
        "a tired courier",
        FeatureKind::Villager,
        "The King's Road runs true between the four capitals - lose your way and you've not been paying attention.",
    ),
    feat(
        685,
        "a farmer leading a cart",
        FeatureKind::Villager,
        "Watch for wandering game along the verges. A hunter with a keen eye eats well on this stretch.",
    ),
    feat(
        686,
        "a road-warden on patrol",
        FeatureKind::Villager,
        "The Sunderlakes lie off toward Melvanala's lake, if fishing's more your speed than fighting.",
    ),
    feat(
        687,
        "a tinker's apprentice",
        FeatureKind::Villager,
        "Broceliande hangs off the Verdant Highlands further along. Good country for taming, they say.",
    ),
    feat(
        688,
        "a traveling preacher",
        FeatureKind::Villager,
        "There's a sealed stair somewhere past Embergate that leads into the Frontier proper. I've never had the nerve to take it.",
    ),
    feat(
        689,
        "a lost-looking merchant's clerk",
        FeatureKind::Villager,
        "Bandits used to work this road. Haven't seen one in an age - whatever's scaring them off, I don't want to meet it either.",
    ),
    // ---- The Wildbound Waste's three gate towns (rooms 30000+) -----------
    feat(
        WILDBOUND_BASE,
        "a grim-faced muster sergeant",
        FeatureKind::Villager,
        "Everyone past that gate is fair game, friend or not. Watch the ones who watch you back a little too long.",
    ),
    feat(
        WILDBOUND_BASE + 1,
        "a bandaged veteran of the Wood",
        FeatureKind::Villager,
        "Went in a party of six. Came out alone. The Wood took the others; I couldn't tell you which ones were mobs.",
    ),
    feat(
        WILDBOUND_BASE + 2,
        "a scarred scavenger",
        FeatureKind::Villager,
        "Bring me anything with teeth still attached and I'll make it worth your while. Coin's no good where you're headed anyway.",
    ),
    feat(
        WILDBOUND_BASE + 3,
        "a watchman who won't meet your eyes",
        FeatureKind::Villager,
        "Last Watch is the last honest ground you'll stand on for a while. Past this gate, trust nothing that smiles.",
    ),
    feat(
        WILDBOUND_BASE + WILDBOUND_BIOME_STRIDE,
        "a gravedigger with too much work",
        FeatureKind::Villager,
        "The Hollowdeep doesn't care if what killed you had a pulse. Dead's dead, down there.",
    ),
    feat(
        WILDBOUND_BASE + WILDBOUND_BIOME_STRIDE + 1,
        "a vigil-keeper counting candles",
        FeatureKind::Villager,
        "We keep the lights burning so the ones still down there have something to find their way back to. Some do.",
    ),
    feat(
        WILDBOUND_BASE + WILDBOUND_BIOME_STRIDE + 2,
        "a thin man buying grave-goods",
        FeatureKind::Villager,
        "I don't ask what it used to be attached to, and you don't ask why I pay so well. Barrowgate manners.",
    ),
    feat(
        WILDBOUND_BASE + WILDBOUND_BIOME_STRIDE + 3,
        "a stair-warden with a cold brazier",
        FeatureKind::Villager,
        "No door's ever been needed here. Nothing in the Hollowdeep has once knocked politely.",
    ),
    feat(
        WILDBOUND_BASE + 2 * WILDBOUND_BIOME_STRIDE,
        "a leather-faced outrider",
        FeatureKind::Villager,
        "Ashhold's the last word before the Flats. Past here it's just you, the heat, and whatever else came looking for a fight.",
    ),
    feat(
        WILDBOUND_BASE + 2 * WILDBOUND_BIOME_STRIDE + 1,
        "a woman who stopped counting the days",
        FeatureKind::Villager,
        "Nobody remembers what drove them out here. The Flats have a way of burning your old life off you along with everything else.",
    ),
    feat(
        WILDBOUND_BASE + 2 * WILDBOUND_BIOME_STRIDE + 2,
        "a one-armed glasswright",
        FeatureKind::Villager,
        "Every blade I sell came out of something bigger than you. Try not to think about that part too hard.",
    ),
    feat(
        WILDBOUND_BASE + 2 * WILDBOUND_BIOME_STRIDE + 3,
        "a sentry watching the heat-shimmer",
        FeatureKind::Villager,
        "You can see it moving out there sometimes, if the light's wrong. Don't point. It notices pointing.",
    ),
    // ---- Wayfarer's Hollow: the new-player tutorial zone ------------------
    feat(
        TUTORIAL_BASE,
        "a weathered instructor",
        FeatureKind::Villager,
        "Take your time. Nothing here can truly hurt you, and Embergate isn't going anywhere - press r whenever you're ready to see the real town.",
    ),
    feat(
        TUTORIAL_BASE + 2,
        "a patient trade-keeper",
        FeatureKind::Villager,
        "Press y and you'll take from whatever's here you're able to work. Every trade in the world starts exactly this simply.",
    ),
    feat(
        TUTORIAL_BASE + 3,
        "a soot-streaked tinker",
        FeatureKind::Villager,
        "Stand at any station and press u. You'll see every recipe it knows, and which ones you can actually make right now.",
    ),
    feat(
        TUTORIAL_BASE + 4,
        "an old archivist",
        FeatureKind::Villager,
        "Whatever you chose, you chose well. But it never hurts to know what everyone else in the tavern can do.",
    ),
    // ---- Aelunor's Wood-Gates: a warden or watcher at every zone's one safe
    // threshold, each with a line about their own glade and its boss. ----
    feat(
        25_012,
        "a sun-freckled elf ranger restringing her bow",
        FeatureKind::Villager,
        "Silverleaf Eaves is gentle enough for a first walk in the wood, but the Hollow-Elf Warlord doesn't share that opinion. Mind yourself past the willow arch.",
    ),
    feat(
        25_083,
        "a hooded druid listening to the standing stones",
        FeatureKind::Villager,
        "The Boughs don't just whisper, they warn. Thistlewitch keeps her bramble court somewhere past them - best not go looking for her unready.",
    ),
    feat(
        25_156,
        "a moss-flecked hermit sunk waist-deep in his garden",
        FeatureKind::Villager,
        "The moss out here grows a little too fast for my liking. The Ancient sleeps somewhere deep in it, and I mean to let it stay asleep.",
    ),
    feat(
        25_237,
        "a high elf huntsman polishing an old horn",
        FeatureKind::Villager,
        "Follow the light and you'll find the Erlking's Huntsman's altar. Follow it too far and you'll find him.",
    ),
    feat(
        25_300,
        "a fae child chasing drifting thistledown",
        FeatureKind::Villager,
        "The down never settles here, and neither does the Nightshade Nymph-Queen's temper. Watch your step past the hollow.",
    ),
    feat(
        25_372,
        "a warden who never once steps inside the fae-ring",
        FeatureKind::Villager,
        "Nothing grows in the circle, and nothing that walks in ever quite walks the same way out. The Ringmother minds it close.",
    ),
    feat(
        25_443,
        "a night-blooming druid tending petals by lamplight",
        FeatureKind::Villager,
        "These blossoms only open after dark, same as what stalks them. Best not linger past sundown.",
    ),
    feat(
        25_515,
        "a fen-wisp catcher with jars of pale light",
        FeatureKind::Villager,
        "The water mirrors the sky too well out there. The Seer-Queen's said to read futures in it, if you're brave or foolish enough to ask her.",
    ),
    feat(
        25_587,
        "a root-cutter missing two fingers",
        FeatureKind::Villager,
        "Wychroot's less a place than a tangle. The Revenant-Lord's been dead longer than the roots, and minds the deeps just the same.",
    ),
    feat(
        25_661,
        "a silver-fingered weaver untangling gossamer",
        FeatureKind::Villager,
        "Loom-fae mind their threads close. Pull one wrong and the Loomweaver herself comes to see who's meddling.",
    ),
    feat(
        25_732,
        "a moonlit warden bathing an old wound in the spring",
        FeatureKind::Villager,
        "The Moonwell only ever shows the moon, whatever the hour. Its Warden's kinder than most out here, but kind isn't the same as safe.",
    ),
    feat(
        25_804,
        "an ancient elf keeper bowed low before the great tree",
        FeatureKind::Villager,
        "The Heartwood's older than Aelunor's own name. The Erlqueen keeps its heart, and precious few who go to meet her come back to tell it.",
    ),
    // ---- Silvael, the Faewood's own city -------------------------------
    feat(
        SILVAEL_BASE,
        "a high elf herald reading out the day's tidings",
        FeatureKind::Villager,
        "Silvael keeps no wall and charges no toll - the wood itself decides who's welcome, and so far it's decided that's near everyone. Mind the Wildwood Gate after dark all the same.",
    ),
    feat(
        SILVAEL_BASE + 1,
        "a warden of the Wildwood Gate, spear planted root-deep",
        FeatureKind::Villager,
        "Silverleaf Eaves is gentle by Faewood standards. Every glade deeper in gets less so. Ask after a zone's own boss before you walk in past its wood-gate, if you'd rather not meet it by surprise.",
    ),
    feat(
        SILVAEL_BASE + 2,
        "Aelwen Songleaf's apprentice, sorting charms by colour",
        FeatureKind::Villager,
        "Aelwen prices by whether she likes you, not by what a thing's worth. Compliment the weave and you'll do better than haggling.",
    ),
    feat(
        SILVAEL_BASE + 3,
        "a druid's apprentice grinding dried moonwell-root",
        FeatureKind::Villager,
        "Branwen's tinctures aren't sold so much as earned. Bring her something interesting out of the wood and she'll usually trade fair.",
    ),
    feat(
        SILVAEL_BASE + 4,
        "an elf child skipping stones that never quite sink",
        FeatureKind::Villager,
        "They say the Moonwell shows you something true if you look long enough. Mostly it's just shown me my own tired face.",
    ),
    feat(
        SILVAEL_BASE + 5,
        "a druid novice tending the standing stones",
        FeatureKind::Villager,
        "The Circle's kept its watch over Aelunor longer than Silvael's had a name. Whatever's out there, they'd know first.",
    ),
    feat(
        SILVAEL_BASE + 6,
        "a high elf archivist glaring at anyone who touches the shelves",
        FeatureKind::Villager,
        "Every bark-bound book on these terraces came out of the wood itself, one way or another. Ask nicely and I might actually let you read one.",
    ),
    feat(
        SILVAEL_BASE + 7,
        "a beastkeeper hung with bells and half-chewed tame-charms",
        FeatureKind::Villager,
        "Aelunor's fae beasts aren't for sale, not here, not anywhere in Silvael. Earn the wood's trust in Animal Taming out past the gate, and one will come to you on its own.",
    ),
];

pub fn features_at(room: RoomId) -> Vec<&'static Feature> {
    // Indexed once: this is called per map cell per frame (via the field's
    // service glyph) and per snapshot, and a linear scan of FEATURES plus 146
    // villagers plus the waystones was the hottest part of both.
    static BY_ROOM: OnceLock<HashMap<RoomId, Vec<&'static Feature>>> = OnceLock::new();
    let by_room = BY_ROOM.get_or_init(|| {
        let mut by_room: HashMap<RoomId, Vec<&'static Feature>> = HashMap::new();
        for f in FEATURES
            .iter()
            .chain(VILLAGERS.iter())
            .chain(waystone_features().iter())
            .chain(tome_feature().iter())
        {
            by_room.entry(f.room).or_default().push(f);
        }
        by_room
    });
    by_room.get(&room).cloned().unwrap_or_default()
}

// ---- Waystones: the Ways menu, and what it will carry you to -------------

const PORTAL_DESC: &str = "A ring of standing waystones hums with a soft blue light, the air \
    inside it rippling like a heat-haze over water. Step through and it will carry you in a \
    breath to any other waystone you know of - the far villages, the drowned isles of the \
    Shattered Archipelago, or the gate of a far country. Press i to open the ways.";

/// The mainland waystones: Embergate's square plus each far country's safe
/// gate room, so a recall to town never means re-walking a whole gate chain.
/// These carry no progression rules of their own. A title is permission to
/// *enter* a land and is checked exactly once, where you walk in
/// (`svc::can_cross_progression_gate`); a waystone is permission to *skip the
/// trip*, and opens only once the player has stood in it (`waystone_is_known`).
/// The sealed continents need no second check here: a visited set cannot hold
/// a Reaches or Kaelmyr room unless the walking gate already let the player by.
pub const CONTINENT_WAYSTONES: &[(&str, RoomId)] = &[
    ("Embergate, the Town Square", 1),
    ("the Sunderlakes landing", LAKES_BASE),
    ("Broceliande, the forest gate", BROCELIANDE_BASE),
    ("the Sundered Reaches sea-gate", REACHES_BASE),
    ("Cinderfall Shore, Kaelmyr", KAELMYR_BASE),
    ("Last Watch, the Wildbound Waste", WILDBOUND_BASE),
];

/// Every destination the Ways can offer: the mainland continent gates first,
/// then the archipelago villages and island landings. Filter with
/// `waystone_is_known` before showing or honouring one.
pub fn waystone_destinations() -> Vec<(&'static str, RoomId)> {
    let mut out: Vec<(&'static str, RoomId)> = CONTINENT_WAYSTONES.to_vec();
    out.extend(super::archipelago::portal_destinations());
    out
}

/// Whether the Ways will carry this player to `dest`. A mainland gate answers
/// only once they have stood in it, so fast travel shortens a road already
/// walked instead of replacing the walk. The archipelago is always open: its
/// villages and island landings have no directional exits at all, so a
/// visited rule would orphan the whole region, and nothing in progression
/// routes through it.
pub fn waystone_is_known(dest: RoomId, visited: &HashSet<RoomId>) -> bool {
    if CONTINENT_WAYSTONES.iter().any(|(_, room)| *room == dest) {
        visited.contains(&dest)
    } else {
        true
    }
}

/// Portal (and, for the villages, fountain) features for the runtime-generated
/// fast-travel network - the Embergate waystone, the villages, and every island
/// landing. Built once and leaked into a `'static` so they read like any other
/// authored feature.
fn waystone_features() -> &'static [Feature] {
    static F: OnceLock<Vec<Feature>> = OnceLock::new();
    F.get_or_init(|| {
        let mut v = Vec::new();
        // The mainland gateways into the network: Embergate's square plus each
        // far country's safe gate room.
        for (_, room) in CONTINENT_WAYSTONES {
            let name = if *room == 1 {
                "the town waystone"
            } else {
                "the gate waystone"
            };
            v.push(feat(*room, name, FeatureKind::Portal, PORTAL_DESC));
        }
        for i in 0..super::archipelago::VILLAGES.len() {
            let room = super::archipelago::village_room(i);
            v.push(feat(
                room,
                "the village waystone",
                FeatureKind::Portal,
                PORTAL_DESC,
            ));
            v.push(feat(
                room,
                "the village fountain",
                FeatureKind::Fountain,
                FOUNTAIN_DESC,
            ));
        }
        for i in 0..super::archipelago::ISLAND_COUNT {
            v.push(feat(
                super::archipelago::island_entrance(i),
                "the island waystone",
                FeatureKind::Portal,
                PORTAL_DESC,
            ));
        }
        v
    })
}

/// Wayfarer's Hollow's Hall of Callings: one lookable tome summarising every
/// playable class, built from the same canonical `Class::tagline`/`resource`/
/// `trait_name` data the character sheet and class-select screen already use,
/// so the tutorial can never drift out of sync with what a class actually
/// does. Generated once and leaked to `'static`, same as `waystone_features`.
fn tome_feature() -> &'static [Feature] {
    static F: OnceLock<Vec<Feature>> = OnceLock::new();
    F.get_or_init(|| {
        use std::fmt::Write;
        let mut body = String::from(
            "Its pages turn themselves to whatever calling draws your eye. Each carries \
             its resource, its defining trait, and a line on how it fights:\n\n",
        );
        for class in super::classes::Class::ALL {
            let _ = writeln!(
                body,
                "{} ({}, {}): {}",
                class.name(),
                class.resource().label(),
                class.trait_name(),
                class.tagline()
            );
        }
        body.push_str(
            "\nNo entry runs longer than this - the tome believes the doing teaches \
             better than the reading ever could.",
        );
        vec![feat(
            TUTORIAL_BASE + 4,
            "the Tome of the Seventeen Callings",
            FeatureKind::Plaque,
            Box::leak(body.into_boxed_str()),
        )]
    })
}

/// The crafting skills whose stations stand in a room (empty if none). Used to
/// gate crafting and to build the craft panel.
pub fn craft_stations_at(room: RoomId) -> Vec<CraftSkill> {
    FEATURES
        .iter()
        .filter(|f| f.room == room)
        .filter_map(|f| match f.kind {
            FeatureKind::CraftStation(skill) => Some(skill),
            _ => None,
        })
        .collect()
}

// ---- Wildlife: critters you can feed, and the perks they leave -----------

/// A small benefit a Boon creature confers while you share its room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perk {
    /// Heartening presence - a brief might (outgoing-damage) buff on arrival.
    Embolden,
    /// Restful presence - restores a little health on arrival.
    Mend,
    /// Quickening presence - restores a little resource on arrival.
    Quicken,
}

impl Perk {
    pub fn label(self) -> &'static str {
        match self {
            Self::Embolden => "emboldened",
            Self::Mend => "mended",
            Self::Quicken => "quickened",
        }
    }
}

/// What a wild creature is, and how (if at all) you can interact with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CritterKind {
    /// Ambient and untouchable - too quick or too wild to catch (squirrels, deer).
    Skittish,
    /// Small game you can hunt (attack) for a little xp when no foe is about.
    Game,
    /// Tame or kindly presence that grants a perk while you share its room.
    Boon(Perk),
}

/// A wild NPC keyed to the room it lives in. Critters are not combatants: they
/// live alongside the mob system rather than inside it.
#[derive(Clone, Debug)]
pub struct CritterSpawn {
    pub home: RoomId,
    pub name: &'static str,
    pub kind: CritterKind,
    /// Short flavour shown in the Wildlife list.
    pub note: &'static str,
    /// Reward for hunting, for `Game` critters.
    pub xp: i32,
    /// An alternate line for when the creature is grounded instead of aloft
    /// (Genesys): a bird mostly wheels overhead, but sometimes it's perched
    /// nearby instead. `None` for critters that don't fly.
    pub perch_note: Option<&'static str>,
    /// A creature out of legend rather than the mundane world (Genesys):
    /// shown with its own colour in the Wildlife list.
    pub mythical: bool,
    /// Can be won over as a stray companion (Genesys) - fed and looked after
    /// over several consecutive days, it joins you on top of any other
    /// companion you already keep. See `svc::feed_wild_critter`.
    pub adoptable: bool,
}

impl CritterSpawn {
    /// The line to show right now: a bird with a `perch_note` alternates
    /// between wheeling overhead and perched nearby, toggling every few
    /// minutes (real time, bucketed) so it isn't the same read every visit.
    /// Everything else always shows its one `note`.
    pub fn display_note(&self, moment_bucket: u64) -> &'static str {
        match self.perch_note {
            Some(perched) if (moment_bucket ^ self.home as u64).is_multiple_of(3) => perched,
            _ => self.note,
        }
    }
}

const fn critter(
    home: RoomId,
    name: &'static str,
    kind: CritterKind,
    note: &'static str,
    xp: i32,
) -> CritterSpawn {
    CritterSpawn {
        home,
        name,
        kind,
        note,
        xp,
        perch_note: None,
        mythical: false,
        adoptable: false,
    }
}

/// A Genesys wildlife entry: same shape as `critter`, plus a perch/mythical/
/// adoptable trio. `perch_note` is `Some` for birds that are sometimes seen
/// grounded instead of aloft.
#[allow(clippy::too_many_arguments)]
const fn genesys_critter(
    home: RoomId,
    name: &'static str,
    kind: CritterKind,
    note: &'static str,
    xp: i32,
    perch_note: Option<&'static str>,
    mythical: bool,
    adoptable: bool,
) -> CritterSpawn {
    CritterSpawn {
        home,
        name,
        kind,
        note,
        xp,
        perch_note,
        mythical,
        adoptable,
    }
}

/// Every wild creature in the world, keyed to its home room. Some you can hunt,
/// most you can only watch, and a few good souls lend you a perk for passing by.
pub const WILDLIFE: &[CritterSpawn] = &[
    // ---- Embergate Town Square (1): a lived-in town menagerie ------------
    critter(
        1,
        "a red squirrel",
        CritterKind::Skittish,
        "racing along the well's mossy lip",
        0,
    ),
    critter(
        1,
        "a flock of rock-doves",
        CritterKind::Skittish,
        "bickering over crumbs at the baker's door",
        0,
    ),
    critter(
        1,
        "a hearth-cat",
        CritterKind::Boon(Perk::Mend),
        "dozing warm beside the great brazier",
        0,
    ),
    critter(
        1,
        "the ostler's grey mare",
        CritterKind::Boon(Perk::Embolden),
        "stamping proud at the stable rail",
        0,
    ),
    // ---- Capitals: each its own creature + a kindly boon ----------------
    critter(
        TASMANIA_SQUARE,
        "a wheeling gull",
        CritterKind::Skittish,
        "screaming over the masts",
        0,
    ),
    critter(
        TASMANIA_SQUARE,
        "a wharf cat",
        CritterKind::Boon(Perk::Quicken),
        "watching the nets with green eyes",
        0,
    ),
    critter(
        MELVANALA_SQUARE,
        "a mountain hare",
        CritterKind::Skittish,
        "still as stone on the terrace",
        0,
    ),
    critter(
        MELVANALA_SQUARE,
        "a tame raven",
        CritterKind::Boon(Perk::Embolden),
        "perched black on the shrine-post",
        0,
    ),
    critter(
        MATLATESH_SQUARE,
        "a sand-fox",
        CritterKind::Skittish,
        "ears up at the gate's shade",
        0,
    ),
    critter(
        MATLATESH_SQUARE,
        "a couched camel",
        CritterKind::Boon(Perk::Quicken),
        "chewing by the oasis wall",
        0,
    ),
    // ---- The Greatroad & wilds (600+): game to hunt, deer to admire -----
    critter(
        600,
        "a fat marsh-rat",
        CritterKind::Game,
        "nosing through the verge",
        6,
    ),
    critter(
        601,
        "a wild rabbit",
        CritterKind::Game,
        "frozen mid-hop on the bank",
        5,
    ),
    critter(
        602,
        "a covey of quail",
        CritterKind::Game,
        "ready to burst from the grass",
        8,
    ),
    critter(
        603,
        "a roe deer",
        CritterKind::Skittish,
        "watching from the treeline",
        0,
    ),
    critter(
        604,
        "a wild boar",
        CritterKind::Game,
        "rooting under the oaks",
        16,
    ),
    critter(
        605,
        "a red fox",
        CritterKind::Skittish,
        "trotting the hedgerow",
        0,
    ),
    // ---- Genesys: birds aloft/perched, and adoptable strays --------------
    genesys_critter(
        1,
        "a flock of starlings",
        CritterKind::Skittish,
        "wheeling in tight loops over the well",
        0,
        Some("lined up along the guildhall eaves"),
        false,
        false,
    ),
    genesys_critter(
        3,
        "a kestrel",
        CritterKind::Skittish,
        "hanging on the wind above the forge chimney",
        0,
        Some("gripping the smithy's weathervane, dead still"),
        false,
        false,
    ),
    genesys_critter(
        5,
        "a pair of ravens",
        CritterKind::Skittish,
        "circling South Gate, croaking to each other",
        0,
        Some("hunched together on the gatehouse rail"),
        false,
        false,
    ),
    genesys_critter(
        620,
        "a wheeling albatross",
        CritterKind::Skittish,
        "riding the harbour thermals on locked wings",
        0,
        Some("resting on the lighthouse rail, folded and huge"),
        false,
        false,
    ),
    genesys_critter(
        624,
        "a cormorant",
        CritterKind::Skittish,
        "skimming low over the harbour swell",
        0,
        Some("drying its wings on the lighthouse stair"),
        false,
        false,
    ),
    genesys_critter(
        660,
        "a golden eagle",
        CritterKind::Skittish,
        "circling the high crags on a rising thermal",
        0,
        Some("perched on the bell tower's very peak"),
        false,
        false,
    ),
    genesys_critter(
        665,
        "a flock of mountain doves",
        CritterKind::Skittish,
        "wheeling white against the grey stone",
        0,
        Some("crowded along the bell tower's ledge"),
        false,
        false,
    ),
    genesys_critter(
        720,
        "a desert falcon",
        CritterKind::Skittish,
        "hunting the thermals over the dunes",
        0,
        Some("hooded and still on the minaret's shoulder"),
        false,
        false,
    ),
    genesys_critter(
        600,
        "a barn swallow",
        CritterKind::Skittish,
        "cutting low arcs over the roadside grass",
        0,
        Some("lined up with its kin along a fence rail"),
        false,
        false,
    ),
    genesys_critter(
        601,
        "a flock of starlings",
        CritterKind::Skittish,
        "turning as one dark cloud over the verge",
        0,
        Some("settled thick in a roadside hedge"),
        false,
        false,
    ),
    genesys_critter(
        3010,
        "a wheeling gull",
        CritterKind::Skittish,
        "screaming over the Saltwind masts",
        0,
        Some("standing one-legged on a mooring post"),
        false,
        false,
    ),
    genesys_critter(
        3030,
        "a desert lark",
        CritterKind::Skittish,
        "singing high over the Sunbaked Bazaar",
        0,
        Some("hopping between the rug-stalls, unbothered"),
        false,
        false,
    ),
    genesys_critter(
        2000,
        "a storm-hawk",
        CritterKind::Skittish,
        "riding the Frontier's own bad weather like it was nothing",
        0,
        Some("gripping a dead tree at the rise, feathers crackling faintly with static"),
        true,
        false,
    ),
    genesys_critter(
        5000,
        "an ash-wraith crow",
        CritterKind::Skittish,
        "circling the Catacombs' mouth in dead silence, no wingbeat at all",
        0,
        Some("sitting on a headstone, watching you with eyes like coals"),
        true,
        false,
    ),
    genesys_critter(
        5200,
        "a bramble-owl",
        CritterKind::Skittish,
        "gliding silent between the Thornwood's black branches",
        0,
        Some("perched low in the bramble gate, feathers grown through with thorn"),
        true,
        false,
    ),
    genesys_critter(
        5416,
        "a tide-wraith gull",
        CritterKind::Skittish,
        "wheeling over the Tide Mouth, crying with a human voice",
        0,
        Some("standing dead still on the waterline, not a feather wet"),
        true,
        false,
    ),
    genesys_critter(
        10000,
        "a storm-petrel of the Reaches",
        CritterKind::Skittish,
        "skimming the shallows just ahead of a squall that isn't there yet",
        0,
        Some("riding a half-sunk piling, utterly unbothered by the swell"),
        true,
        false,
    ),
    genesys_critter(
        12000,
        "a cinder-swift",
        CritterKind::Skittish,
        "cutting through the ash-fall too fast to properly see",
        0,
        Some("resting on a scorched stone, wings smouldering faintly at the tips"),
        true,
        false,
    ),
    genesys_critter(
        16000,
        "a moon-heron",
        CritterKind::Skittish,
        "gliding low over the reed-maze in dead silence",
        0,
        Some("standing one-legged in the shallows, feathers faintly silvered"),
        true,
        false,
    ),
    genesys_critter(
        22000,
        "a fae-wren",
        CritterKind::Skittish,
        "flitting between the eaves faster than the eye follows",
        0,
        Some("perched on a low branch, glowing faintly green at the throat"),
        true,
        false,
    ),
    genesys_critter(
        1,
        "a scruffy stray dog",
        CritterKind::Skittish,
        "trotting hopeful circles around anyone eating lunch by the well",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        2,
        "a one-eyed tavern cat",
        CritterKind::Skittish,
        "sprawled across the warmest flagstone by the Gilded Flagon's hearth",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        3,
        "a soot-streaked forge cat",
        CritterKind::Skittish,
        "dozing on a pile of scrap iron, utterly unbothered by the hammering",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        4,
        "a temple hound",
        CritterKind::Skittish,
        "lying at the Dawn's threshold, head on its paws, watching everyone who passes",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        5,
        "a gate-watch mutt",
        CritterKind::Skittish,
        "trotting the wall with the guards like it's on shift too",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        620,
        "a salt-crusted wharf dog",
        CritterKind::Skittish,
        "nosing through the fish-crates, hoping nobody's watching",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        660,
        "a shaggy mountain dog",
        CritterKind::Skittish,
        "curled against the cold stone of the Lakeshore Square",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        720,
        "a lean desert cat",
        CritterKind::Skittish,
        "stretched in a strip of shade, tail flicking at the flies",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        3000,
        "a guildhall cat",
        CritterKind::Skittish,
        "asleep on the noticeboard, using an old bounty notice as a pillow",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        3010,
        "a fishmonger's cat",
        CritterKind::Skittish,
        "working the Fishmarket stalls like it owns every one of them",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        3020,
        "a terrace-garden cat",
        CritterKind::Skittish,
        "stalking something invisible through the frost-vines",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        3030,
        "a bazaar puppy",
        CritterKind::Skittish,
        "tangled in a rug merchant's spare cloth, tail going nonstop",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        8000,
        "an old lantern-dog",
        CritterKind::Skittish,
        "keeping the lamp-keeper company on his slow rounds of Lantern Cove",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        9000,
        "a hearthward tabby",
        CritterKind::Skittish,
        "sunning itself on the clerk's windowsill, ledger be damned",
        0,
        None,
        false,
        true,
    ),
    genesys_critter(
        2000,
        "a scarred ash-wolf pup",
        CritterKind::Skittish,
        "watching the Frontier stair with eyes too old for its size",
        0,
        None,
        true,
        true,
    ),
    genesys_critter(
        22000,
        "a moon-hound kit",
        CritterKind::Skittish,
        "padding silent circles at the forest gate, coat like poured moonlight",
        0,
        None,
        true,
        true,
    ),
    genesys_critter(
        16000,
        "a reed-cat",
        CritterKind::Skittish,
        "crouched in the shallows, dry as a bone despite the water",
        0,
        None,
        true,
        true,
    ),
    genesys_critter(
        10000,
        "a storm-touched sea-pup",
        CritterKind::Skittish,
        "shaking off spray that never quite lands on it",
        0,
        None,
        true,
        true,
    ),
    genesys_critter(
        12000,
        "a cinder-kit",
        CritterKind::Skittish,
        "curled on warm ash, smoke curling harmlessly off its whiskers",
        0,
        None,
        true,
        true,
    ),
    genesys_critter(
        5416,
        "a barrow-hound",
        CritterKind::Skittish,
        "sitting patient at the Tide Mouth, exactly where the light doesn't reach",
        0,
        None,
        true,
        true,
    ),
];

pub fn critters_at(room: RoomId) -> Vec<&'static CritterSpawn> {
    WILDLIFE.iter().filter(|c| c.home == room).collect()
}

/// Stable global index of a critter (its position in `WILDLIFE`), used to key
/// per-world hunt cooldowns.
pub fn critter_index(c: &CritterSpawn) -> Option<usize> {
    WILDLIFE.iter().position(|w| std::ptr::eq(w, c))
}

// ---- Resource nodes: what you chop, mine, fish, forage, and skin ---------

/// A harvestable resource node fixed to a room: a tree stand, an ore vein, a
/// fishing spot, or a herb/skinning patch. Modelled exactly like wildlife -
/// static data keyed to a home room, with a per-node respawn cooldown tracked on
/// the service (`gathered`). Harvesting one grants its raw material plus skill
/// xp, gated behind a minimum skill level so higher tiers stay out of reach
/// until the trade is trained up.
#[derive(Clone, Copy, Debug)]
pub struct ResourceNode {
    pub home: RoomId,
    pub skill: GatherSkill,
    /// What the player sees and acts on, e.g. "an old oak wood".
    pub name: &'static str,
    /// Short flavour shown in the Resources list.
    pub note: &'static str,
    /// Tier 0..5, low to high; sets the material and the difficulty band.
    pub tier: u8,
    /// Minimum skill level required to work it.
    pub level_req: i32,
    /// Item id granted on a successful harvest (derived from skill + tier).
    pub yield_item: u32,
    /// Skill xp granted per harvest.
    pub xp: i32,
}

const fn node(
    home: RoomId,
    skill: GatherSkill,
    name: &'static str,
    note: &'static str,
    tier: u8,
    level_req: i32,
    xp: i32,
) -> ResourceNode {
    ResourceNode {
        home,
        skill,
        name,
        note,
        tier,
        level_req,
        // The yield is fixed by the node's skill and tier, so the material can
        // never drift out of sync with its source.
        yield_item: super::items::material_id(skill.index(), tier as u32),
        xp,
    }
}

/// A resource node whose harvest yields an *explicit* item rather than the
/// tiered material derived from `(skill, tier)`. This lets a gathering spot hand
/// out a specific catalog item - e.g. one of the forty Sunderlakes fish - while
/// still training its skill and respawning exactly like any other node (the
/// gather flow in `svc.rs` reads `yield_item` directly, so no new mechanic is
/// needed). The tier still sets the difficulty band; `level_req` gates it.
#[allow(clippy::too_many_arguments)]
const fn node_yielding(
    home: RoomId,
    skill: GatherSkill,
    name: &'static str,
    note: &'static str,
    tier: u8,
    level_req: i32,
    yield_item: u32,
    xp: i32,
) -> ResourceNode {
    ResourceNode {
        home,
        skill,
        name,
        note,
        tier,
        level_req,
        yield_item,
        xp,
    }
}

/// Every harvestable node in the world, keyed to its home room. Tiers climb with
/// distance/difficulty from Embergate: roadside starters near town, mid materials
/// out in the overworld wings, and the best materials deep in the harder zones.
pub const NODES: &[ResourceNode] = &[
    // ---- Woodcutting: birch -> oak -> ash -> yew -> ironbark ------------
    node(
        600,
        GatherSkill::Woodcutting,
        "a stand of roadside birch",
        "slim white birches along the verge",
        0,
        1,
        12,
    ),
    node(
        680,
        GatherSkill::Woodcutting,
        "an old oak wood",
        "gnarled oaks on the hillside",
        1,
        8,
        30,
    ),
    node(
        684,
        GatherSkill::Woodcutting,
        "a grove of mountain ash",
        "straight ash on the high slopes",
        2,
        16,
        70,
    ),
    node(
        688,
        GatherSkill::Woodcutting,
        "an ancient yew",
        "a black, age-twisted yew",
        3,
        26,
        150,
    ),
    node(
        803,
        GatherSkill::Woodcutting,
        "ironbark rooted in the dark",
        "iron-hard trunks in the fungal deep",
        4,
        38,
        320,
    ),
    // ---- Mining: copper -> tin -> iron -> silver -> mithril -------------
    node(
        601,
        GatherSkill::Mining,
        "a weathered copper outcrop",
        "green-streaked stone by the road",
        0,
        1,
        12,
    ),
    node(
        740,
        GatherSkill::Mining,
        "a tin-streaked rockface",
        "pale ore in the desert rock",
        1,
        8,
        30,
    ),
    node(
        743,
        GatherSkill::Mining,
        "a deep iron seam",
        "red iron banding the cliff",
        2,
        16,
        70,
    ),
    node(
        748,
        GatherSkill::Mining,
        "a glinting silver vein",
        "silver threading the deep rock",
        3,
        26,
        150,
    ),
    node(
        750,
        GatherSkill::Mining,
        "a mithril lode",
        "the fabled blue-white ore",
        4,
        38,
        320,
    ),
    // ---- Fishing: bream -> trout -> pike -> sturgeon -> moonscale -------
    node(
        620,
        GatherSkill::Fishing,
        "the harbor shallows",
        "small fish in the clear shallows",
        0,
        1,
        12,
    ),
    node(
        720,
        GatherSkill::Fishing,
        "the oasis pool",
        "fish rising in the still oasis",
        0,
        1,
        12,
    ),
    node(
        660,
        GatherSkill::Fishing,
        "the high lake shore",
        "trout holding in the cold lake",
        1,
        8,
        30,
    ),
    node(
        641,
        GatherSkill::Fishing,
        "a tidal cove",
        "pike hunting the running cove",
        2,
        16,
        70,
    ),
    node(
        701,
        GatherSkill::Fishing,
        "a black fen pool",
        "something big turns in the dark water",
        3,
        26,
        150,
    ),
    node(
        650,
        GatherSkill::Fishing,
        "the kraken's deep",
        "pale shapes in the abyssal water",
        4,
        38,
        320,
    ),
    // ---- Foraging: marsh sage -> redleaf -> bloodthistle -> frostbloom -> sunmoss
    node(
        603,
        GatherSkill::Foraging,
        "a verge of marsh sage",
        "grey-green sage along the ditch",
        0,
        1,
        12,
    ),
    node(
        682,
        GatherSkill::Foraging,
        "a redleaf meadow",
        "red-veined leaves in the meadow",
        1,
        8,
        30,
    ),
    node(
        700,
        GatherSkill::Foraging,
        "a bloodthistle bog",
        "dark thistles standing in the mire",
        2,
        16,
        70,
    ),
    node(
        690,
        GatherSkill::Foraging,
        "a cold high meadow",
        "pale blooms rimed with frost",
        3,
        26,
        150,
    ),
    node(
        801,
        GatherSkill::Foraging,
        "a cavern lit by sunmoss",
        "moss that glows faintly in the dark",
        4,
        38,
        320,
    ),
    // ---- Skinning: rough -> thick -> boar -> bear -> direhide -----------
    node(
        604,
        GatherSkill::Skinning,
        "fresh boar sign under the oaks",
        "trampled ground and shed bristles",
        0,
        1,
        12,
    ),
    node(
        760,
        GatherSkill::Skinning,
        "a well-worn game trail",
        "hoof-churned earth on the savanna",
        1,
        8,
        30,
    ),
    node(
        762,
        GatherSkill::Skinning,
        "a razorback wallow",
        "a muddy wallow, still warm",
        2,
        16,
        70,
    ),
    node(
        765,
        GatherSkill::Skinning,
        "a cave-bear's kill",
        "a fresh kill dragged half-eaten",
        3,
        26,
        150,
    ),
    node(
        767,
        GatherSkill::Skinning,
        "a dire-beast lair",
        "old bones and a rank, huge musk",
        4,
        38,
        320,
    ),
    // ---- Fishing: the forty Sunderlakes fish, spread across the maze zones
    // by prestige (four per zone), gated by rising Fishing level (see
    // extend_lakes / lakes_fish_for_zone). Homes sit on always-present maze
    // cells, so every node room is real. Yields are explicit fish items.
    node_yielding(
        16005,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Silver Minnow",
        0,
        1,
        4600,
        15,
    ),
    node_yielding(
        16027,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Reed Perch",
        0,
        2,
        4601,
        18,
    ),
    node_yielding(
        16049,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Mudsnout Carp",
        0,
        3,
        4602,
        21,
    ),
    node_yielding(
        16071,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Copperscale Roach",
        0,
        4,
        4603,
        24,
    ),
    node_yielding(
        16093,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Marsh Bream",
        0,
        6,
        4604,
        27,
    ),
    node_yielding(
        16115,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Bristle Loach",
        0,
        7,
        4605,
        30,
    ),
    node_yielding(
        16137,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Fenwater Tench",
        0,
        8,
        4606,
        33,
    ),
    node_yielding(
        16159,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Islet Rudd",
        0,
        9,
        4607,
        36,
    ),
    node_yielding(
        16269,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Blue Mere Trout",
        1,
        11,
        4608,
        39,
    ),
    node_yielding(
        16291,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Ghost Grayling",
        1,
        12,
        4609,
        42,
    ),
    node_yielding(
        16313,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Cavern Blindfish",
        1,
        13,
        4610,
        45,
    ),
    node_yielding(
        16335,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Reedmace Pike",
        1,
        14,
        4611,
        48,
    ),
    node_yielding(
        16357,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Sunken Char",
        1,
        16,
        4612,
        51,
    ),
    node_yielding(
        16379,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Drowned Valley Eel",
        1,
        17,
        4613,
        54,
    ),
    node_yielding(
        16401,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Lanternjaw",
        1,
        18,
        4614,
        57,
    ),
    node_yielding(
        16423,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Silt-Gilded Barbel",
        1,
        19,
        4615,
        60,
    ),
    node_yielding(
        16533,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Moonpale Salmon",
        2,
        21,
        4616,
        63,
    ),
    node_yielding(
        16555,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Glasswater Sturgeon",
        2,
        22,
        4617,
        66,
    ),
    node_yielding(
        16577,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Meregleam Tench",
        2,
        23,
        4618,
        69,
    ),
    node_yielding(
        16599,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Stormfin Bass",
        2,
        24,
        4619,
        72,
    ),
    node_yielding(
        16621,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Hollow-Cavern Ray",
        2,
        26,
        4620,
        75,
    ),
    node_yielding(
        16643,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Bittern's Bane",
        2,
        27,
        4621,
        78,
    ),
    node_yielding(
        16665,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Amberweed Golden",
        2,
        28,
        4622,
        81,
    ),
    node_yielding(
        16687,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Frostmere Whitefish",
        2,
        29,
        4623,
        84,
    ),
    node_yielding(
        16797,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Kingfisher's Prize",
        3,
        31,
        4624,
        87,
    ),
    node_yielding(
        16819,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Deep Meregold",
        3,
        32,
        4625,
        90,
    ),
    node_yielding(
        16841,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Silverback Salmon",
        3,
        33,
        4626,
        93,
    ),
    node_yielding(
        16863,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Drowned-God Carp",
        3,
        34,
        4627,
        96,
    ),
    node_yielding(
        16885,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Voidmere Sturgeon",
        3,
        36,
        4628,
        99,
    ),
    node_yielding(
        16907,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for Ghostlight Pike",
        3,
        37,
        4629,
        102,
    ),
    node_yielding(
        16929,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Tempest Marlin",
        3,
        38,
        4630,
        105,
    ),
    node_yielding(
        16951,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Abyss Anglerfish",
        3,
        39,
        4631,
        108,
    ),
    node_yielding(
        17061,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Sunderlake Leviathan",
        4,
        41,
        4632,
        111,
    ),
    node_yielding(
        17083,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for The Mere-Mother",
        4,
        42,
        4633,
        114,
    ),
    node_yielding(
        17105,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Moonscale Royal",
        4,
        43,
        4634,
        117,
    ),
    node_yielding(
        17127,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for Drowned Crown Bass",
        4,
        44,
        4635,
        120,
    ),
    node_yielding(
        17149,
        GatherSkill::Fishing,
        "a reed-fringed fishing stand",
        "cast a line from the worn planks for Heartglow Trout",
        4,
        46,
        4636,
        123,
    ),
    node_yielding(
        17171,
        GatherSkill::Fishing,
        "a quiet backwater pool",
        "a slow eddy where the big ones hold for The Fathom-King",
        4,
        47,
        4637,
        126,
    ),
    node_yielding(
        17193,
        GatherSkill::Fishing,
        "a deep-channel angling spot",
        "dark water dropping away to the deep for Weeping Silverfin",
        4,
        48,
        4638,
        129,
    ),
    node_yielding(
        17215,
        GatherSkill::Fishing,
        "a still lily-shadowed pool",
        "fish rise among the floating pads for The First Fish",
        4,
        49,
        4639,
        132,
    ),
    // ---- Wildbound (tier 6): the trades' summit, out in the far lands ----
    node(
        BROCELIANDE_BASE + 3,
        GatherSkill::Woodcutting,
        "a worldtree sapling",
        "impossibly old for a sapling; the grain hums under a hand",
        5,
        55,
        600,
    ),
    node(
        BROCELIANDE_BASE + 41,
        GatherSkill::Woodcutting,
        "a fallen worldtree bough",
        "a limb the storms brought down whole",
        5,
        55,
        600,
    ),
    node(
        KAELMYR_BASE + 5,
        GatherSkill::Mining,
        "a starmetal seam",
        "ore that fell burning from the sky, long ago",
        5,
        55,
        600,
    ),
    node(
        KAELMYR_BASE + 52,
        GatherSkill::Mining,
        "a sky-iron crater",
        "the walls still glitter where the star broke",
        5,
        55,
        600,
    ),
    node(
        LAKES_BASE + 7,
        GatherSkill::Fishing,
        "an abyssal spring",
        "the water goes down further than light does",
        5,
        55,
        600,
    ),
    node(
        LAKES_BASE + 44,
        GatherSkill::Fishing,
        "a drowned sinkhole",
        "something vast keeps the eels fat down there",
        5,
        55,
        600,
    ),
    node(
        BROCELIANDE_BASE + 77,
        GatherSkill::Foraging,
        "a dreamlotus pool",
        "the blooms only open for those patient enough to watch",
        5,
        55,
        600,
    ),
    node(
        LAKES_BASE + 91,
        GatherSkill::Foraging,
        "a dreamlotus shallows",
        "petals drift on water that never ripples",
        5,
        55,
        600,
    ),
    node(
        REACHES_BASE + 9,
        GatherSkill::Skinning,
        "a wyrm kill-site",
        "whatever brought it down did not stay to feed",
        5,
        55,
        600,
    ),
    node(
        KAELMYR_BASE + 88,
        GatherSkill::Skinning,
        "a wyrm moulting-ground",
        "shed scale and hide, acres of it",
        5,
        55,
        600,
    ),
    // ---- Wayfarer's Hollow's Gathering Glade: one tier-0 node per trade,
    // all in one room, purely so `y` can be tried immediately by anyone. ----
    node(
        TUTORIAL_BASE + 2,
        GatherSkill::Woodcutting,
        "a sapling stand",
        "young trees planted for practising hands",
        0,
        1,
        12,
    ),
    node(
        TUTORIAL_BASE + 2,
        GatherSkill::Mining,
        "a shallow ore seam",
        "soft ore breaking the surface",
        0,
        1,
        12,
    ),
    node(
        TUTORIAL_BASE + 2,
        GatherSkill::Fishing,
        "a stocked practice pool",
        "slow, obliging fish",
        0,
        1,
        12,
    ),
    node(
        TUTORIAL_BASE + 2,
        GatherSkill::Foraging,
        "a patch of hardy herbs",
        "common roadside herbs",
        0,
        1,
        12,
    ),
    node(
        TUTORIAL_BASE + 2,
        GatherSkill::Skinning,
        "a practice hide-rack",
        "cured hides set out to learn on",
        0,
        1,
        12,
    ),
];

pub fn nodes_at(room: RoomId) -> Vec<&'static ResourceNode> {
    NODES.iter().filter(|n| n.home == room).collect()
}

/// Stable global index of a node (its position in `NODES`), used to key
/// per-world harvest cooldowns.
pub fn node_index(n: &ResourceNode) -> Option<usize> {
    NODES.iter().position(|x| std::ptr::eq(x, n))
}

// ---- seed_world: the authored core, then every extension wing ------------

fn room(
    id: RoomId,
    name: &'static str,
    zone: &'static str,
    safe: bool,
    desc: &'static str,
    exits: &[(Dir, RoomId)],
) -> Room {
    Room {
        id,
        name,
        desc,
        zone,
        safe,
        pvp: false,
        exits: exits.iter().copied().collect(),
    }
}

/// Build the vertical-slice world: Embergate (safe hub) + the King's Road.
pub fn seed_world() -> World {
    let rooms = vec![
        room(
            1,
            "Embergate - Town Square",
            "Embergate",
            true,
            "Lanternlight pools on worn cobbles, and the great bronze brazier at the \
             square's heart throws a restless amber glow over the town that takes its \
             name from it. Embergate hums with evening trade: a fiddler saws by the \
             well, children chase a dog between the legs of off-duty guardsmen, and \
             the smell of the baker's last loaves hangs warm in the air. A notice \
             board leans by the well, thick with bounties and lost-cat pleas alike. Near \
             the brazier, old stone steps descend behind ironwork and warning plaques, \
             less a shortcut than a sealed road into old danger. The Gilded Flagon glows north, the temple \
             west, Market Row east, and the South Gate and open road lie south.",
            &[
                (Dir::North, 2),
                (Dir::East, 3),
                (Dir::West, 4),
                (Dir::South, 5),
            ],
        ),
        room(
            2,
            "Embergate - The Gilded Flagon",
            "Embergate",
            true,
            "Woodsmoke, spilled ale, and roasting meat tangle in the air of the town's \
             beloved tavern. A great hearth roars at one end; long tables run with \
             candle-wax and carved initials. Adventurers swap tall tales over tankards, \
             a card game simmers toward a brawl in the corner, and the barkeep polishes \
             a horn cup that will never come clean. It is warm, loud, and safe - the \
             last of those rarer than the others. A side door out back leads north to \
             Wayfarer's Hollow, where the newest faces in the room learned their trade; \
             the square lies south.",
            &[(Dir::South, 1), (Dir::North, TUTORIAL_BASE)],
        ),
        room(
            3,
            "Embergate - Market Row & the Ember Forge",
            "Embergate",
            true,
            "The lane narrows into a clamor of commerce, awnings snapping overhead and \
             barkers crying their wares. At the far end the open front of the Ember \
             Forge breathes furnace-heat into the street, where BRUNA IRONHAND, the \
             town smith, works a glowing billet with blows that ring off the rooftops. \
             Racks of blades, bows, and staves gleam at her shoulder, for sale to any \
             who can pay. The square lies west; the rest of the market district opens \
             east.",
            &[(Dir::West, 1), (Dir::East, 201)],
        ),
        room(
            4,
            "Embergate - Temple of the Dawn",
            "Embergate",
            true,
            "Pale columns rise toward a domed ceiling painted with a sunrise so vivid it \
             seems to warm the cold stone beneath. Clerics in white move in hushed \
             procession, and a hundred candles gutter at the feet of a gilded sun. Here \
             the wounded are mended and the dead are mourned; here, it is said, a fallen \
             adventurer's spirit is gathered up and returned to the world. A sense of \
             grave, patient mercy fills the air. This is a sanctuary, not a road; \
             the square lies east.",
            &[(Dir::East, 1)],
        ),
        room(
            5,
            "Embergate - South Gate",
            "Embergate",
            true,
            "A heavy iron portcullis stands raised on chains thick as a man's arm, \
             and beneath its teeth the last of Embergate's lanternlight gives way to \
             the open dark. Beyond the gate the King's Road unspools into rolling \
             country, pale under the moon and loud with crickets, and a bored \
             gate-guard leans on his halberd and warns every passing adventurer that \
             the road is safe only as far as he can see it. The square lies north; \
             the open road runs south.",
            &[(Dir::North, 1), (Dir::South, 6)],
        ),
        // ---- Wayfarer's Hollow (safe, rooms 40000+): the new-player tutorial
        // zone. Every brand-new character spawns here (see `join`/`tutorial_
        // start_room`), never dangerous, one room per core system, hung off
        // the Gilded Flagon (room 2) by a normal walk north - room 1 itself
        // has no free direction left (Down is Frontier, Up is the city
        // district). `r` (recall) already works from anywhere in the game, so
        // leaving for the real Embergate is always just a keypress away - the
        // join-time message says so.
        room(
            TUTORIAL_BASE,
            "Wayfarer's Hollow",
            "Wayfarer's Hollow",
            true,
            "A round, sheltered yard behind the Gilded Flagon, floored in raked sand \
             and ringed by low benches, built for exactly one purpose: teaching \
             newcomers the shape of the world before it teaches them the hard way. \
             A weathered instructor in a patched coat watches over the yard, \
             unhurried, ready to point anyone in whatever direction they're curious \
             about. A training yard for the sword lies north, a gathering glade east, \
             the Hall of Callings west, steps lead down to a tinker's hall, and the \
             tavern's side door leads back south to Embergate proper.",
            &[
                (Dir::North, TUTORIAL_BASE + 1),
                (Dir::East, TUTORIAL_BASE + 2),
                (Dir::South, 2),
                (Dir::West, TUTORIAL_BASE + 4),
                (Dir::Down, TUTORIAL_BASE + 3),
            ],
        ),
        room(
            TUTORIAL_BASE + 1,
            "Wayfarer's Hollow - the Training Yard",
            "Wayfarer's Hollow",
            false,
            "A ring of packed earth, scuffed pale by countless practice bouts, holds a \
             stuffed straw dummy lashed to a stout post at its center - patched, \
             re-patched, and clearly none the worse for it. It swings back with a \
             padded fist when struck, just hard enough to teach without ever really \
             hurting: a fair place to learn to close for the attack, work an ability \
             off the bar, and flee before real harm ever finds you. The Hollow lies \
             south.",
            &[(Dir::South, TUTORIAL_BASE)],
        ),
        room(
            TUTORIAL_BASE + 2,
            "Wayfarer's Hollow - the Gathering Glade",
            "Wayfarer's Hollow",
            true,
            "A tidy little clearing planted, a touch too conveniently, with one of \
             everything: a stand of saplings, a vein of soft ore breaking the surface, \
             a stocked fishing pool, a patch of hardy herbs, and the hide-strewn \
             remains of a hunter's practice runs. Every gathering trade can be tried \
             here at once, tools or none - the glade wants you to learn the reach of \
             `y`, not to make you go looking for it. The Hollow lies west.",
            &[(Dir::West, TUTORIAL_BASE)],
        ),
        room(
            TUTORIAL_BASE + 3,
            "Wayfarer's Hollow - the Tinker's Hall",
            "Wayfarer's Hollow",
            true,
            "A small covered hall below the Hollow's yard, holding a scaled-down copy \
             of every craft station Market Row has to offer - forge, workbench, \
             tannery, alchemy lab, and cooking fire, all cold and quiet and waiting. \
             Nothing here is rare or valuable; it exists purely so a newcomer can open \
             the crafting panel, see what each trade actually makes, and understand \
             the shape of the gather-then-craft chain before it matters. Steps lead \
             back up to the Hollow.",
            &[(Dir::Up, TUTORIAL_BASE)],
        ),
        room(
            TUTORIAL_BASE + 4,
            "Wayfarer's Hollow - the Hall of Callings",
            "Wayfarer's Hollow",
            true,
            "Portraits line this quiet round room, one for each calling a soul might \
             answer in Lateania, painted in a style too old for any of these young \
             instructors to have made themselves. A great iron-bound tome rests open \
             on a lectern at the room's heart, its pages turning slowly on their own \
             to whichever calling a reader's attention settles on. It is a place for \
             reading, not fighting - the Hollow lies east.",
            &[(Dir::East, TUTORIAL_BASE)],
        ),
        // ---- Embergate shop district (safe) -----------------------------
        room(
            201,
            "Embergate - The Outfitter's Stall",
            "Embergate",
            true,
            "The market widens into a square of canvas stalls. Dominating it is the \
             Outfitter's, where TOMAS THREADNEEDLE presides over teetering heaps of \
             boiled leather, riveted mail, woven robes, and stout boots, all of it for \
             sale. He squints at every passerby as though measuring them for a coffin or \
             a cuirass, whichever they need first. The forge lies west; the lane runs on \
             north and east.",
            &[(Dir::West, 3), (Dir::North, 202), (Dir::East, 203)],
        ),
        room(
            202,
            "Embergate - The Apothecary",
            "Embergate",
            true,
            "A narrow shopfront crammed floor to rafter with bottles, jars, and bundled \
             herbs that fill the air with a sharp green reek. OLD MIRELA, bent nearly \
             double, shuffles between the shelves dispensing draughts and elixirs to any \
             with coin and an ailment. A cauldron mutters in the back. Nothing here is \
             quite labeled, but she always seems to know which bottle is which. The \
             outfitter's lies south.",
            &[(Dir::South, 201)],
        ),
        room(
            203,
            "Embergate - The Curio Cart",
            "Embergate",
            true,
            "A gaudy painted cart blocks half the lane, hung with charms, rings, and \
             trinkets that wink in the lanternlight. PELL THE MAGPIE leans against it \
             with a grin too wide to wholly trust, talking up the luck and virtue of his \
             wares to a skeptical crowd. Some of it may even be enchanted. The \
             outfitter's lies west; a quieter street runs east.",
            &[(Dir::West, 201), (Dir::East, 204)],
        ),
        room(
            204,
            "Embergate - The Bank of Embergate",
            "Embergate",
            true,
            "A squat, iron-doored building stands aloof from the market bustle, the only \
             stone-built shop on the row. Within, a humorless clerk tallies coin behind a \
             grille and a vault door broods at the back. Adventurers store their \
             hard-won gold here against the day a dungeon empties their purse. The curio \
             cart lies west; the town wall walk runs north.",
            &[(Dir::West, 203), (Dir::North, 205)],
        ),
        room(
            205,
            "Embergate - The Wall Walk",
            "Embergate",
            true,
            "Stone steps climb to a parapet atop the town wall, where a single guardsman \
             keeps a bored vigil over the dark country beyond. From here all of Embergate \
             spreads out below, lamplit and small and worth defending, and past the wall \
             the King's Road runs off into a night full of teeth. The bank lies back down \
             to the south.",
            &[(Dir::South, 204)],
        ),
        room(
            6,
            "The King's Road - Open Country",
            "King's Road",
            false,
            "The cobbles give way to packed earth rutted by cart-wheels, and the \
             ordered safety of the town falls away with them. Tall grass whispers \
             and bows on either side of the road, full of the small rustlings of \
             night creatures, and the town wall recedes behind you into the dark to \
             the north. Ahead the road runs on south into open, unguarded country.",
            &[(Dir::North, 5), (Dir::South, 7)],
        ),
        room(
            7,
            "The King's Road - The Old Milestone",
            "King's Road",
            false,
            "A mossy old milestone leans at the verge, its carved leagues to far \
             cities worn nearly smooth by weather and the idle hands of resting \
             travellers. A thin trail forks away east into a dark bramble thicket, \
             the grass beside it beaten down by something that left no clear track, \
             while the King's Road itself runs on south. The way back to the gate is \
             north.",
            &[(Dir::North, 6), (Dir::East, 8), (Dir::South, 9)],
        ),
        room(
            8,
            "The King's Road - Bramble Thicket",
            "King's Road",
            false,
            "The trail chokes to a dead end in a clearing walled on every side by \
             thorns grown high as a horse, their black branches hung with tufts of \
             snagged wool and worse. Something heavy has trampled the grass flat here \
             quite recently, and the air carries a rank animal musk that prickles the \
             back of the neck. The only way out is back west the way you came.",
            &[(Dir::West, 7)],
        ),
        room(
            9,
            "The King's Road - Ruined Watchtower",
            "King's Road",
            false,
            "A toppled watchtower slumps against the hillside, its stones black and \
             scorched and its timbers long since fallen to charcoal, a relic of some \
             border war no living song remembers. Crows have made the ruin their own, \
             and they watch your passing with a patience that feels less than \
             natural. The road continues south into a shadowed defile, and the way \
             back to safer ground is north.",
            &[(Dir::North, 7), (Dir::South, 10)],
        ),
        room(
            10,
            "The King's Road - The Defile",
            "King's Road",
            false,
            "Steep banks close in on a gloomy cut in the hills. The road ends \
             where a landslide once buried it; a narrow game-trail slips south \
             beneath leaning pines into older, darker country. The way back is \
             north.",
            &[(Dir::North, 9), (Dir::South, 11)],
        ),
        // ---- Whisperwood (forest, tier 2-3) -----------------------------
        room(
            11,
            "Whisperwood - The Threshold Oaks",
            "Whisperwood",
            false,
            "Two oaks older than the kingdom lean together to form a living arch, \
             their bark carved with charms so weathered they read only as scars. \
             The air past them is cooler, greener, and somehow listening. The \
             trail back climbs north toward the defile.",
            &[(Dir::North, 10), (Dir::South, 12)],
        ),
        room(
            12,
            "Whisperwood - Fernlight Hollow",
            "Whisperwood",
            false,
            "Sunlight falls in slow green coins through a canopy so high it feels \
             like a cathedral roof. Knee-deep ferns drink the light and hide the \
             ground entirely. Paths press south and east; the oaks stand north.",
            &[(Dir::North, 11), (Dir::South, 13), (Dir::East, 14)],
        ),
        room(
            13,
            "Whisperwood - The Murmuring Path",
            "Whisperwood",
            false,
            "The forest earns its name here: a wind you cannot feel moves the high \
             leaves in long sighing syllables, almost words. You keep turning to \
             answer someone who is not there. North and south the path runs on.",
            &[(Dir::North, 12), (Dir::South, 15)],
        ),
        room(
            14,
            "Whisperwood - The Toadstool Ring",
            "Whisperwood",
            false,
            "A perfect circle of scarlet toadstools rings a patch of unnaturally \
             soft moss. Old instinct tells you not to step inside it, and older \
             instinct tells you why. The hollow lies back to the west.",
            &[(Dir::West, 12)],
        ),
        room(
            15,
            "Whisperwood - The Leaning Birches",
            "Whisperwood",
            false,
            "Pale birches lean every direction at once, as though the ground had \
             shrugged a century ago and never settled. Their peeling bark hangs in \
             curls like discarded parchment. Ways lead north, south, and west.",
            &[(Dir::North, 13), (Dir::South, 16), (Dir::West, 17)],
        ),
        room(
            16,
            "Whisperwood - Wolf-Run Gully",
            "Whisperwood",
            false,
            "The land folds into a shallow gully floored with cracked mud, printed \
             over and over with the splayed tracks of a hunting pack. Tufts of grey \
             fur snag the bramble at nose height. The path continues north and south.",
            &[(Dir::North, 15), (Dir::South, 18)],
        ),
        room(
            17,
            "Whisperwood - The Hermit's Cairn",
            "Whisperwood",
            false,
            "A waist-high pile of river stones marks a grave no one tends. Someone \
             has balanced a single acorn on the topmost rock; it has not fallen, \
             though the wind worries everything else here. East returns to the birches.",
            &[(Dir::East, 15)],
        ),
        room(
            18,
            "Whisperwood - Spider-Silk Crossing",
            "Whisperwood",
            false,
            "Sheets of web span the gap between two dead elms, jeweled with dew and \
             the husks of things that stopped struggling long ago. The strands hum \
             faintly when you breathe. Paths lead north, south, and east.",
            &[(Dir::North, 16), (Dir::South, 19), (Dir::East, 20)],
        ),
        room(
            19,
            "Whisperwood - The Sunken Brook",
            "Whisperwood",
            false,
            "A clear brook has cut itself a channel so deep it runs below the roots, \
             chuckling in the dark a body's length beneath your feet. The forest \
             smells of cold stone and watercress. North and south the way goes on.",
            &[(Dir::North, 18), (Dir::South, 21)],
        ),
        room(
            20,
            "Whisperwood - The Weaver's Hollow",
            "Whisperwood",
            false,
            "Every branch in this dead-end hollow is strung with web until the trees \
             wear grey lace gowns. Small wrapped bundles turn slowly on invisible \
             threads. Nothing here is alive that should be. The crossing lies west.",
            &[(Dir::West, 18)],
        ),
        room(
            21,
            "Whisperwood - Stag-Horn Clearing",
            "Whisperwood",
            false,
            "Sun pours into a wide clearing where the bleached antlers of some \
             enormous stag rise from the grass like the rafters of a roofless hall. \
             Songbirds nest in the tines. The path runs north and south.",
            &[(Dir::North, 19), (Dir::South, 22)],
        ),
        room(
            22,
            "Whisperwood - The Crossroads Stone",
            "Whisperwood",
            false,
            "A moss-furred standing stone leans at the meeting of three trails, its \
             carved hand pointing nowhere that still exists. Offerings rot at its \
             base: bread, a copper ring, a child's wooden horse. Ways lead north, \
             south, and west.",
            &[(Dir::North, 21), (Dir::South, 23), (Dir::West, 24)],
        ),
        room(
            23,
            "Whisperwood - The Hanging Vale",
            "Whisperwood",
            false,
            "The trees thin over a vale where curtains of pale moss hang so thick \
             they brush your shoulders as you pass, cool and faintly damp, like the \
             hands of the polite dead. The path presses north and south.",
            &[(Dir::North, 22), (Dir::South, 25)],
        ),
        room(
            24,
            "Whisperwood - The Drowned Shrine",
            "Whisperwood",
            false,
            "A forgotten woodland shrine has sunk to its shoulders in black bog \
             water, only the carved face of some antlered god still breaking the \
             surface, watching the sky. Frogs go silent as you arrive. East returns \
             to the crossroads.",
            &[(Dir::East, 22)],
        ),
        room(
            25,
            "Whisperwood - The Char Circle",
            "Whisperwood",
            false,
            "A ring of trees stands black and branchless, killed by a fire that \
             never spread past their own trunks. In the center the ground is glassy \
             and warm. The forest leans away from this place. North and south remain.",
            &[(Dir::North, 23), (Dir::South, 26)],
        ),
        room(
            26,
            "Whisperwood - The Greenway Fork",
            "Whisperwood",
            false,
            "The undergrowth opens onto an ancient greenway, a road of turf so \
             straight it must have been laid by hands, now half-swallowed by the \
             forest reclaiming its own. Paths lead north, south, and east.",
            &[(Dir::North, 25), (Dir::South, 27), (Dir::East, 28)],
        ),
        room(
            27,
            "Whisperwood - The Lantern Trees",
            "Whisperwood",
            false,
            "Clusters of luminous fungus climb these trunks in spiral ladders, \
             casting a soft blue-green glow that makes the dusk beneath the canopy \
             into a perpetual underwater twilight. The way runs north and south.",
            &[(Dir::North, 26), (Dir::South, 29)],
        ),
        room(
            28,
            "Whisperwood - The Elder Grove",
            "Whisperwood",
            false,
            "At the heart of a ring of bowing trees stands one vast and ancient \
             treant, bark like cliff-stone, eyes like two cold green moons opening \
             slowly as you intrude on a silence kept for a thousand years. The \
             greenway lies west.",
            &[(Dir::West, 26)],
        ),
        room(
            29,
            "Whisperwood - The Root Stair",
            "Whisperwood",
            false,
            "The land tilts downward and the roots of the great trees arrange \
             themselves into a rough descending stair, slick with leaf-mould and \
             generations of fallen rain. Cold air rises from below. North climbs \
             back; south leads on.",
            &[(Dir::North, 27), (Dir::South, 30)],
        ),
        room(
            30,
            "Whisperwood - The Sinking Gate",
            "Whisperwood",
            false,
            "The forest floor opens at last into a sinkhole ringed by exposed roots, \
             a black throat breathing cave-cold air up into the green world. A rope \
             ladder, half-rotted, descends into the dark. North returns to the wood.",
            &[(Dir::North, 29), (Dir::Down, 31)],
        ),
        // ---- Duskhollow Caverns (caves & undead, tier 3-4) --------------
        room(
            31,
            "Duskhollow Caverns - The Drip Gallery",
            "Duskhollow Caverns",
            false,
            "Your boots find stone. The cavern mouth drips in slow, patient music, \
             each drop ringing in a darkness so complete your lantern seems an \
             apology. Daylight is a memory up the ladder, north and above. The cave \
             pushes south.",
            &[(Dir::Up, 30), (Dir::South, 32)],
        ),
        room(
            32,
            "Duskhollow Caverns - The Forking Throat",
            "Duskhollow Caverns",
            false,
            "The passage splits around a pillar of fused stalactite and stalagmite, \
             a stone hourglass taller than three men. Cold draughts breathe from \
             both branches. Ways lead north, south, and east.",
            &[(Dir::North, 31), (Dir::South, 33), (Dir::East, 34)],
        ),
        room(
            33,
            "Duskhollow Caverns - The Whispering Crawl",
            "Duskhollow Caverns",
            false,
            "The ceiling drops until you must stoop, and the walls press close \
             enough to scrape both shoulders. Your own breathing comes back to you \
             changed, as though the rock were trying the sound in its mouth. North \
             and south.",
            &[(Dir::North, 32), (Dir::South, 35)],
        ),
        room(
            34,
            "Duskhollow Caverns - The Ossuary Niche",
            "Duskhollow Caverns",
            false,
            "Someone stacked bones here, long ago and with terrible care: a wall of \
             skulls mortared with smaller bones, every empty socket aimed at the \
             room's one entrance. They have been waiting for company. West returns \
             to the fork.",
            &[(Dir::West, 32)],
        ),
        room(
            35,
            "Duskhollow Caverns - The Black Mirror",
            "Duskhollow Caverns",
            false,
            "A still pool fills the cavern floor, so utterly without ripple it \
             throws your lanternlight back like polished obsidian. Something pale \
             rests at the bottom, and you decide not to learn what. Ways lead north, \
             south, and west.",
            &[(Dir::North, 33), (Dir::South, 36), (Dir::West, 37)],
        ),
        room(
            36,
            "Duskhollow Caverns - The Stalactite Nave",
            "Duskhollow Caverns",
            false,
            "The chamber soars into a forest of hanging stone, fang upon fang \
             vanishing into a dark the lantern cannot reach. Drips fall from \
             impossible heights and burst cold against your neck. North and south \
             continue.",
            &[(Dir::North, 35), (Dir::South, 38)],
        ),
        room(
            37,
            "Duskhollow Caverns - The Sealed Door",
            "Duskhollow Caverns",
            false,
            "A door of iron-banded oak, swollen and black with damp, has been chained \
             shut from this side and then, for good measure, from this side again. \
             Something scratches the far face, slow and tireless. East returns to \
             the pool.",
            &[(Dir::East, 35)],
        ),
        room(
            38,
            "Duskhollow Caverns - The Crystal Vein",
            "Duskhollow Caverns",
            false,
            "A seam of clouded crystal threads the wall here, catching the lantern \
             and breaking it into a hundred trapped sparks that seem to drift like \
             slow snow inside the stone. It is beautiful and it is cold. Ways lead \
             north, south, and east.",
            &[(Dir::North, 36), (Dir::South, 39), (Dir::East, 40)],
        ),
        room(
            39,
            "Duskhollow Caverns - The Slumping Stair",
            "Duskhollow Caverns",
            false,
            "Steps cut by long-dead miners sag and slide underfoot, half-melted by \
             the patient creep of mineral water. Each one bears a worn carved \
             number in a counting-script no living tongue still speaks. North and \
             south.",
            &[(Dir::North, 38), (Dir::South, 41)],
        ),
        room(
            40,
            "Duskhollow Caverns - The Gnawed Larder",
            "Duskhollow Caverns",
            false,
            "Sacks and barrels rot in a side-chamber some lost expedition used for \
             stores. Everything organic has been gnawed to lace by teeth too \
             numerous and too small to think about. West returns to the vein.",
            &[(Dir::West, 38)],
        ),
        room(
            41,
            "Duskhollow Caverns - The Cold Hearth",
            "Duskhollow Caverns",
            false,
            "A ring of fire-blackened stones holds a heap of ash that has not felt \
             warmth in centuries, yet the air above it shimmers as though it \
             remembers being hot. Bedrolls lie around it, occupied by their owners \
             still. North and south.",
            &[(Dir::North, 39), (Dir::South, 42)],
        ),
        room(
            42,
            "Duskhollow Caverns - The Hanging Bridge",
            "Duskhollow Caverns",
            false,
            "A natural bridge of stone arches over a chasm whose bottom your lantern \
             never finds. Far below, something moves with a dragging, wet \
             deliberation. Best to cross quickly. Ways lead north, south, and west.",
            &[(Dir::North, 41), (Dir::South, 43), (Dir::West, 44)],
        ),
        room(
            43,
            "Duskhollow Caverns - The Fungal Garden",
            "Duskhollow Caverns",
            false,
            "Pale mushrooms grow waist-high in nightmare profusion, their caps \
             exhaling faint spores that prickle in the lungs and paint the lantern \
             with a sickly halo. Things have been harvesting them. North and south.",
            &[(Dir::North, 42), (Dir::South, 45)],
        ),
        room(
            44,
            "Duskhollow Caverns - The Throne of Bones",
            "Duskhollow Caverns",
            false,
            "A dead-end vault where the cavern floor rises into a dais, and upon it \
             a throne built entirely of fused skeletons leers in the lanternlight. \
             Its occupant lifts a crowned skull and regards you with two points of \
             cold blue fire. The bridge lies east.",
            &[(Dir::East, 42)],
        ),
        room(
            45,
            "Duskhollow Caverns - The Weeping Wall",
            "Duskhollow Caverns",
            false,
            "Mineral water sheets down a vast flowstone wall in an endless silver \
             curtain, and the sound is so like grief that you find your own throat \
             tightening for no reason you can name. North and south go on.",
            &[(Dir::North, 43), (Dir::South, 46)],
        ),
        room(
            46,
            "Duskhollow Caverns - The Echo Junction",
            "Duskhollow Caverns",
            false,
            "Five passages meet in a domed chamber that returns every sound \
             threefold, so that your single footstep becomes a marching company and \
             your whisper an argument. Ways lead north, south, and east.",
            &[(Dir::North, 45), (Dir::South, 47), (Dir::East, 48)],
        ),
        room(
            47,
            "Duskhollow Caverns - The Salt Flats",
            "Duskhollow Caverns",
            false,
            "An ancient sea died here and left its ghost: a flat white plain of \
             salt crust that crunches like thin ice underfoot, glittering to the \
             edge of the light. The air tastes of old oceans. North and south.",
            &[(Dir::North, 46), (Dir::South, 49)],
        ),
        room(
            48,
            "Duskhollow Caverns - The Miner's End",
            "Duskhollow Caverns",
            false,
            "A pick still stands buried in the dead-end wall where its owner left it, \
             and its owner left it because its owner is still here, slumped in the \
             corner, patient as the stone. West returns to the junction.",
            &[(Dir::West, 46)],
        ),
        room(
            49,
            "Duskhollow Caverns - The Drowned Stair",
            "Duskhollow Caverns",
            false,
            "Steps descend into black water that has risen to swallow them, and \
             keeps rising, drip by patient drip. The air grows colder and carries \
             the green reek of a flooded tomb. North climbs back; south wades on.",
            &[(Dir::North, 47), (Dir::South, 50)],
        ),
        room(
            50,
            "Duskhollow Caverns - The Sunken Arch",
            "Duskhollow Caverns",
            false,
            "A carved arch stands half-submerged at the cavern's lowest point, its \
             keystone graven with a drowned crown. Beyond and below it the water \
             becomes a flooded stair down into a deeper, older dark. North leads \
             back up.",
            &[(Dir::North, 49), (Dir::Down, 51)],
        ),
        // ---- Drowned Crypts (water & undead, tier 4-5) ------------------
        room(
            51,
            "Drowned Crypts - The Tide Vestibule",
            "Drowned Crypts",
            false,
            "You descend into a flooded hall where black water laps at carved \
             sarcophagi like moored boats. The cold is total and intimate, the kind \
             that settles in the marrow and stays. Up returns to the caverns; the \
             crypt drops away below.",
            &[(Dir::Up, 50), (Dir::Down, 52)],
        ),
        room(
            52,
            "Drowned Crypts - The Sarcophagus Row",
            "Drowned Crypts",
            false,
            "Stone coffins line both walls, their lids carved with the serene faces \
             of the long-dead. Several lids lie aside in the water. The faces beneath \
             are no longer serene. Ways lead up, south, and east.",
            &[(Dir::Up, 51), (Dir::South, 53), (Dir::East, 54)],
        ),
        room(
            53,
            "Drowned Crypts - The Wading Nave",
            "Drowned Crypts",
            false,
            "The water rises to your thighs here, cold enough to ache, and things \
             brush your legs in the dark that you choose to believe are only weeds. \
             The current pulls gently south. North and south.",
            &[(Dir::North, 52), (Dir::South, 55)],
        ),
        room(
            54,
            "Drowned Crypts - The Reliquary",
            "Drowned Crypts",
            false,
            "Niches in this dead-end chamber once held holy relics; now they hold \
             only silt and the small bones of the desperate who came seeking them. \
             A single gold leaf still glints underwater. West returns to the row.",
            &[(Dir::West, 52)],
        ),
        room(
            55,
            "Drowned Crypts - The Black Font",
            "Drowned Crypts",
            false,
            "A great basin dominates the chamber, brimming with water blacker than \
             the dark around it. The surface holds a perfect, impossible stillness, \
             and your reflection in it is slow to copy your movements. North and \
             south.",
            &[(Dir::North, 53), (Dir::South, 56)],
        ),
        room(
            56,
            "Drowned Crypts - The Pillared Deep",
            "Drowned Crypts",
            false,
            "Rows of columns march off into water and darkness, each one carved as a \
             shrouded mourner, each one bowing slightly inward, so that to walk among \
             them is to be escorted by a procession of the grieving stone. Ways lead \
             north, south, and west.",
            &[(Dir::North, 55), (Dir::South, 57), (Dir::West, 58)],
        ),
        room(
            57,
            "Drowned Crypts - The Catafalque",
            "Drowned Crypts",
            false,
            "A raised bier stands clear of the flood, draped in rotted velvet that \
             still holds, somehow, a deep imperial purple. The body upon it is gone. \
             The shape pressed into the velvet suggests it merely rose and walked \
             away. North and south.",
            &[(Dir::North, 56), (Dir::South, 59)],
        ),
        room(
            58,
            "Drowned Crypts - The Oubliette",
            "Drowned Crypts",
            false,
            "A forgetting-hole: a dead-end shaft where prisoners were lowered and \
             the rope cut. The water here is deepest, and full of the patient, \
             upturned faces of everyone the crypt has ever swallowed. East returns \
             to the deep.",
            &[(Dir::East, 56)],
        ),
        room(
            59,
            "Drowned Crypts - The Choir of Salt",
            "Drowned Crypts",
            false,
            "Stalactites of crystallized brine hang in ranks like organ pipes, and \
             when the slow current stirs the flood they keen a single sustained note \
             that you feel in your teeth more than hear. North, and down.",
            &[(Dir::North, 57), (Dir::Down, 60)],
        ),
        room(
            60,
            "Drowned Crypts - The Sunken Crossing",
            "Drowned Crypts",
            false,
            "Submerged steps lead up onto a broad landing where three flooded halls \
             converge, their arches reflected in the still water until you cannot \
             tell stone from its double. Ways lead up, south, and east.",
            &[(Dir::Up, 59), (Dir::South, 61), (Dir::East, 62)],
        ),
        room(
            61,
            "Drowned Crypts - The Pauper's Vault",
            "Drowned Crypts",
            false,
            "Here the dead were given no coffins, only shelves, and the shelves have \
             long since spilled their burden into the flood. The water is thick with \
             the anonymous dead, turning slowly in the current. North and south.",
            &[(Dir::North, 60), (Dir::South, 63)],
        ),
        room(
            62,
            "Drowned Crypts - The Lich's Sanctum",
            "Drowned Crypts",
            false,
            "The water falls away into a dry, candle-ringed sanctum where a robed \
             figure bends over a book bound in something that was once a face. It \
             does not turn. It says, in a voice like a closing tomb, that it has \
             been expecting you. The crossing lies west.",
            &[(Dir::West, 60)],
        ),
        room(
            63,
            "Drowned Crypts - The Weed-Choked Hall",
            "Drowned Crypts",
            false,
            "Pale subterranean weed has colonized this hall in drifting curtains, \
             feeding on the dead and on the dark, and it parts reluctantly as you \
             pass, closing again behind you like a held breath let go. North and south.",
            &[(Dir::North, 61), (Dir::South, 64)],
        ),
        room(
            64,
            "Drowned Crypts - The Last Lantern",
            "Drowned Crypts",
            false,
            "A bronze lantern hangs from the vaulted ceiling, and impossibly, \
             improbably, a small cold flame still burns within it, untended for \
             centuries. By its light the water ahead glitters with a different, \
             warmer mineral. North and south.",
            &[(Dir::North, 63), (Dir::South, 65)],
        ),
        room(
            65,
            "Drowned Crypts - The Ember Stair",
            "Drowned Crypts",
            false,
            "The flood drains away down a stair cut from raw red stone, and the air \
             changes utterly: drier, sharper, carrying the faraway tang of smoke and \
             hot metal. Something deep in the rock is awake and burning. North \
             returns to the crypts; the stair drops toward the heat.",
            &[(Dir::North, 64), (Dir::Down, 66)],
        ),
        // ---- Emberpeak Mines (fire & dwarven ruin, tier 5-6) ------------
        room(
            66,
            "Emberpeak Mines - The Cinder Gate",
            "Emberpeak Mines",
            false,
            "You descend into a hewn hall where the very walls hold a sullen red \
             warmth, and runes carved by long-vanished dwarves still glow faintly in \
             the heat. Up leads back to the cold crypts; the mines open north.",
            &[(Dir::Up, 65), (Dir::North, 67)],
        ),
        room(
            67,
            "Emberpeak Mines - The Ore-Cart Junction",
            "Emberpeak Mines",
            false,
            "Rusted rails cross and recross the floor, and a single ore-cart sits \
             where it stopped an age ago, still heaped with raw red ingots no one \
             ever came to claim. The metal is warm to the touch. Ways lead south, \
             north, and east.",
            &[(Dir::South, 66), (Dir::North, 68), (Dir::East, 69)],
        ),
        room(
            68,
            "Emberpeak Mines - The Bellows Hall",
            "Emberpeak Mines",
            false,
            "Vast leather bellows, big as houses and cracked with age, flank a forge \
             channel cut into the floor. Far below, magma still pulses, and with \
             each pulse the dead bellows seem to stir, exhaling a gust of furnace \
             air. South and north.",
            &[(Dir::South, 67), (Dir::North, 70)],
        ),
        room(
            69,
            "Emberpeak Mines - The Collapsed Drift",
            "Emberpeak Mines",
            false,
            "A mining drift ends in a wall of fallen rubble, and pinned within it, \
             reaching, are the fossilized arms of the miners who did not get out. The \
             stone here ticks with trapped heat. West returns to the junction.",
            &[(Dir::West, 67)],
        ),
        room(
            70,
            "Emberpeak Mines - The Glass Foundry",
            "Emberpeak Mines",
            false,
            "The floor of this chamber is a frozen river of slag glass, swirled black \
             and red and gold, smooth enough to skate and just warm enough to remind \
             you what made it. Shapes are suspended within it. South and north.",
            &[(Dir::South, 68), (Dir::North, 71)],
        ),
        room(
            71,
            "Emberpeak Mines - The Anvil of Kings",
            "Emberpeak Mines",
            false,
            "A single anvil the size of a cart squats on a basalt plinth, its face \
             worn into a shallow valley by ten thousand vanished hands. Strike it and \
             the whole mountain answers in a low bronze hum. Ways lead south, north, \
             and west.",
            &[(Dir::South, 70), (Dir::North, 72), (Dir::West, 73)],
        ),
        room(
            72,
            "Emberpeak Mines - The Smelter's Gallery",
            "Emberpeak Mines",
            false,
            "Crucibles line a long gallery, each still cupping a disc of cooled \
             metal, each disc stamped with the seal of a dwarven house that no longer \
             exists in any memory but this one. The heat presses close. South and north.",
            &[(Dir::South, 71), (Dir::North, 74)],
        ),
        room(
            73,
            "Emberpeak Mines - The Slag Pit",
            "Emberpeak Mines",
            false,
            "Waste from a thousand years of smelting was tipped into this dead-end \
             pit, and it never fully cooled. A crust shifts over molten depths, and \
             the air above it bends with heat. Something basks half-submerged. East \
             returns to the anvil.",
            &[(Dir::East, 71)],
        ),
        room(
            74,
            "Emberpeak Mines - The Vein of Fire",
            "Emberpeak Mines",
            false,
            "A seam of raw firegold threads the wall, so hot it glows from within the \
             stone, lighting the chamber in a restless amber pulse like a heartbeat. \
             To mine it would be to mine a coal still burning. South and north.",
            &[(Dir::South, 72), (Dir::North, 75)],
        ),
        room(
            75,
            "Emberpeak Mines - The Cathedral Forge",
            "Emberpeak Mines",
            false,
            "The mine opens into a forge built like a temple, its central furnace a \
             chimney of carved stone rising beyond the lantern's reach. The dwarves \
             worshipped fire here, and fire, it seems, still attends. Ways lead south, \
             north, and east.",
            &[(Dir::South, 74), (Dir::North, 76), (Dir::East, 77)],
        ),
        room(
            76,
            "Emberpeak Mines - The Quenching Pools",
            "Emberpeak Mines",
            false,
            "Stone troughs that once cooled fresh-forged blades now hold black, \
             scummed water that steams without cease. The hiss is constant, almost a \
             voice, and the steam takes shapes you would rather it did not. South and north.",
            &[(Dir::South, 75), (Dir::North, 78)],
        ),
        room(
            77,
            "Emberpeak Mines - The Magma Heart",
            "Emberpeak Mines",
            false,
            "A dead-end cavern open to the mountain's molten core, a lake of fire \
             whose light hurts to look upon. From its surface a vast figure heaves \
             itself upright, basalt and lava, sloughing flame, turning a furnace gaze \
             upon the small cold thing that has entered its house. The forge lies west.",
            &[(Dir::West, 75)],
        ),
        room(
            78,
            "Emberpeak Mines - The Ascending Flue",
            "Emberpeak Mines",
            false,
            "A great chimney climbs the chamber, and the updraft through it is fierce \
             and hot, carrying sparks like upward-falling stars. Iron rungs set into \
             the flue lead toward a distant, paler light. South and north.",
            &[(Dir::South, 76), (Dir::North, 79)],
        ),
        room(
            79,
            "Emberpeak Mines - The Frost-Cracked Tunnel",
            "Emberpeak Mines",
            false,
            "Strangely, the heat fails here all at once, and the walls wear a rime of \
             frost that has no business this deep in a burning mountain. Your breath \
             fogs. Something cold is bleeding down from above. South and north.",
            &[(Dir::South, 78), (Dir::North, 80)],
        ),
        room(
            80,
            "Emberpeak Mines - The Rimeward Gate",
            "Emberpeak Mines",
            false,
            "The tunnel ends at a gate of fused ice and iron, beyond which a stair \
             climbs into killing cold and white light. Warm air dies against it. The \
             mines fall away south; up leads into winter.",
            &[(Dir::South, 79), (Dir::Up, 81)],
        ),
        // ---- Frostspire Ascent (ice mountain, tier 6-7) -----------------
        room(
            81,
            "Frostspire Ascent - The Threshold of Ice",
            "Frostspire Ascent",
            false,
            "You emerge onto a mountainside of blue glacial ice, and the cold takes \
             your breath as a physical theft. Wind screams past, carrying snow like \
             ground glass. Down returns to the warm dark; the ascent climbs up \
             from here.",
            &[(Dir::Down, 80), (Dir::Up, 82)],
        ),
        room(
            82,
            "Frostspire Ascent - The Wind-Carved Pass",
            "Frostspire Ascent",
            false,
            "The path threads a pass where the wind has sculpted the ice into a \
             gallery of blades and figures, frozen courtiers bowing eternally to a \
             gale that never tires of them. Ways lead down, north, and east.",
            &[(Dir::Down, 81), (Dir::North, 83), (Dir::East, 84)],
        ),
        room(
            83,
            "Frostspire Ascent - The Glass Stair",
            "Frostspire Ascent",
            false,
            "Steps of clear ice climb the slope, and through them you can see down \
             into the glacier's heart, where dark shapes are frozen at depths no \
             summer will ever reach. Do not look too long. South and north.",
            &[(Dir::South, 82), (Dir::North, 85)],
        ),
        room(
            84,
            "Frostspire Ascent - The Frozen Caravan",
            "Frostspire Ascent",
            false,
            "A merchant train lies where the cold caught it: ponies, carts, and \
             huddled drivers all locked in clear ice, perfectly preserved, their last \
             expressions still legible. A dead-end, and a warning. West returns to \
             the pass.",
            &[(Dir::West, 82)],
        ),
        room(
            85,
            "Frostspire Ascent - The Singing Crevasse",
            "Frostspire Ascent",
            false,
            "A crevasse splits the path, and the wind crossing its mouth draws from \
             the depths a sound between a flute and a scream, rising and falling, a \
             song the mountain has practiced for ten thousand winters. South and north.",
            &[(Dir::South, 83), (Dir::North, 86)],
        ),
        room(
            86,
            "Frostspire Ascent - The Aurora Shelf",
            "Frostspire Ascent",
            false,
            "A broad ice shelf opens to the sky, and overhead the aurora pours in \
             silent rivers of green and violet light, painting the snow in colors \
             that have no warmth in them at all. Ways lead south, north, and west.",
            &[(Dir::South, 85), (Dir::North, 87), (Dir::West, 88)],
        ),
        room(
            87,
            "Frostspire Ascent - The Hoarfrost Shrine",
            "Frostspire Ascent",
            false,
            "A shrine to some forgotten winter-god stands sheathed in feathered \
             hoarfrost, its offering-bowl heaped with frozen coins and the frozen \
             hands of those who lingered to leave them. The cold here has intent. \
             South and north.",
            &[(Dir::South, 86), (Dir::North, 89)],
        ),
        room(
            88,
            "Frostspire Ascent - The Wendigo's Larder",
            "Frostspire Ascent",
            false,
            "A dead-end ice cave hung with frozen carcasses, neatly butchered, \
             neatly stored, by something that understands winter and is patient with \
             it. Not all the carcasses are animals. The shelf lies east.",
            &[(Dir::East, 86)],
        ),
        room(
            89,
            "Frostspire Ascent - The Knife-Edge Ridge",
            "Frostspire Ascent",
            false,
            "The path narrows to a spine of wind-scoured ice with a killing drop to \
             either hand, the whole world falling away into white cloud below. You \
             cross it one careful step at a time. South and north.",
            &[(Dir::South, 87), (Dir::North, 90)],
        ),
        room(
            90,
            "Frostspire Ascent - The Sky Altar",
            "Frostspire Ascent",
            false,
            "A flat shelf near the summit holds an altar of black stone, the only \
             dark thing in all this white, swept perpetually clear of snow by a wind \
             that seems to serve it. Ways lead south, north, and east.",
            &[(Dir::South, 89), (Dir::North, 91), (Dir::East, 92)],
        ),
        room(
            91,
            "Frostspire Ascent - The Last Camp",
            "Frostspire Ascent",
            false,
            "A ring of frozen tents marks where some expedition made its final stand \
             against the mountain. The cold preserved everything: the banked fire, \
             the open journals, the climbers in their bags, sleeping the sleep that \
             does not end. South and north.",
            &[(Dir::South, 90), (Dir::North, 93)],
        ),
        room(
            92,
            "Frostspire Ascent - The Wyrm's Eyrie",
            "Frostspire Ascent",
            false,
            "A dead-end hollow scoured into the peak itself, floored with the picked \
             bones of centuries of prey. Ice crusts the walls in great raked furrows. \
             Something vast and white uncoils from the frost, and the storm itself \
             seems to draw breath. The altar lies west.",
            &[(Dir::West, 90)],
        ),
        room(
            93,
            "Frostspire Ascent - The Cloud-Breaking Stair",
            "Frostspire Ascent",
            false,
            "The stair climbs through the cloud-deck at last, and breaks above it \
             into a thin, brilliant, freezing sunlight, the whole storm reduced to a \
             white sea churning beneath your feet. South and north.",
            &[(Dir::South, 91), (Dir::North, 94)],
        ),
        room(
            94,
            "Frostspire Ascent - The Summit Approach",
            "Frostspire Ascent",
            false,
            "The peak is close now, a black needle of stone breaking through the ice, \
             and set into its base is a doorway too straight and too dark to be \
             natural, exhaling a cold that even the mountain does not own. South and north.",
            &[(Dir::South, 93), (Dir::North, 95)],
        ),
        room(
            95,
            "Frostspire Ascent - The Sunken Gate",
            "Frostspire Ascent",
            false,
            "A vast gate of black basalt stands half-buried in the summit ice, its \
             lintel carved with a citadel that should not be here, on a peak, at the \
             top of the world. The way in leads down, into stone, into the past. \
             South returns to the snow.",
            &[(Dir::South, 94), (Dir::Down, 96)],
        ),
        // ---- The Sunken Citadel (megadungeon, tier 7-8) -----------------
        room(
            96,
            "The Sunken Citadel - The Hall of Entry",
            "The Sunken Citadel",
            false,
            "You pass from ice into a hall of black stone so vast the lantern cannot \
             find its roof, and the cold here is not winter's cold but something \
             older and more deliberate. The gate is up and behind; the citadel \
             opens north.",
            &[(Dir::Up, 95), (Dir::North, 97)],
        ),
        room(
            97,
            "The Sunken Citadel - The Gallery of Kings",
            "The Sunken Citadel",
            false,
            "Statues of armored kings line a processional gallery, each twice the \
             height of a man, each with its carved face deliberately, completely \
             chiseled away. Whatever they ruled wished them forgotten. Ways lead \
             south, north, and east.",
            &[(Dir::South, 96), (Dir::North, 98), (Dir::East, 99)],
        ),
        room(
            98,
            "The Sunken Citadel - The Shattered Rotunda",
            "The Sunken Citadel",
            false,
            "A domed chamber lies cracked open by some ancient cataclysm, its mosaic \
             floor depicting a war between things with too many wings, half of it \
             fallen into a chasm that swallowed the rest of the story. South and north.",
            &[(Dir::South, 97), (Dir::North, 100)],
        ),
        room(
            99,
            "The Sunken Citadel - The Reliquary of Saints",
            "The Sunken Citadel",
            false,
            "Glass cases line this dead-end vault, each meant to hold a holy bone, \
             each shattered from within. Whatever sainthood was kept here did not \
             stay dead, and did not stay holy. West returns to the gallery.",
            &[(Dir::West, 97)],
        ),
        room(
            100,
            "The Sunken Citadel - The Drowned Throne Room",
            "The Sunken Citadel",
            false,
            "Black water fills the lower half of a throne room built for giants, and \
             the throne itself rises from the flood, empty, its arms gripped by \
             skeletal hands that did not belong to whoever last sat there. South and north.",
            &[(Dir::South, 98), (Dir::North, 101)],
        ),
        room(
            101,
            "The Sunken Citadel - The Iron Library",
            "The Sunken Citadel",
            false,
            "Books bound in beaten iron fill shelves three storeys high, their pages \
             metal leaf, their words etched in a script that hurts to focus on. Some \
             volumes are chained shut. Some chains have been broken outward. Ways lead \
             south, north, and west.",
            &[(Dir::South, 100), (Dir::North, 102), (Dir::West, 103)],
        ),
        room(
            102,
            "The Sunken Citadel - The Orrery Vault",
            "The Sunken Citadel",
            false,
            "A great brass orrery hangs broken in the dark, its planets stilled \
             mid-orbit, and the constellation it models is no sky you have ever seen \
             or would wish to. One sphere, black and unlabeled, still slowly turns. \
             South and north.",
            &[(Dir::South, 101), (Dir::North, 104)],
        ),
        room(
            103,
            "The Sunken Citadel - The Oath-Breaker's Cell",
            "The Sunken Citadel",
            false,
            "A dead-end chapel-cell where a paladin was once walled up alive for a \
             sin the citadel would not name. The wall is broken now, from the inside, \
             and the figure that kneels in the rubble lifts a ruined helm and a \
             blackened sword. The library lies east.",
            &[(Dir::East, 101)],
        ),
        room(
            104,
            "The Sunken Citadel - The Gallery of Whispers",
            "The Sunken Citadel",
            false,
            "A long hall where the black stone has been worked into ten thousand \
             carved mouths, all open, and as you pass each one breathes a single word \
             of a sentence ten thousand years long that no one was ever meant to hear \
             the end of. South and north.",
            &[(Dir::South, 102), (Dir::North, 105)],
        ),
        room(
            105,
            "The Sunken Citadel - The Obsidian Descent",
            "The Sunken Citadel",
            false,
            "The floor falls away into a stair of polished obsidian spiraling down \
             into a red-lit dark, and the heat that rises from below is not fire's \
             heat but the warmth of something vast and living and awake. South leads \
             back; down leads to the throne beneath.",
            &[(Dir::South, 104), (Dir::Down, 106)],
        ),
        // ---- The Obsidian Throne (endgame demon realm, tier 9-10) -------
        room(
            106,
            "The Obsidian Throne - The Threshold of Embers",
            "The Obsidian Throne",
            false,
            "You step into a realm that is no longer stone but something between \
             flesh and volcanic glass, and it is warm, and it pulses, and it knows \
             you are here. The stair climbs up behind you toward the world; the \
             throne-realm spreads south.",
            &[(Dir::Up, 105), (Dir::South, 107)],
        ),
        room(
            107,
            "The Obsidian Throne - The Avenue of the Damned",
            "The Obsidian Throne",
            false,
            "A wide black road runs between two endless rows of the bound damned, \
             figures of ash and ember frozen mid-scream, lighting your way with the \
             dull red glow of their own slow burning. They turn their heads to watch \
             you pass. North and south.",
            &[(Dir::North, 106), (Dir::South, 108)],
        ),
        room(
            108,
            "The Obsidian Throne - The Court of Cinders",
            "The Obsidian Throne",
            false,
            "A vast antechamber where lesser demons hold a mockery of court, perched \
             on thrones of cooling lava, their attention turning to you all at once \
             like a hundred furnace doors swinging open. Ways lead north, south, and east.",
            &[(Dir::North, 107), (Dir::South, 109), (Dir::East, 110)],
        ),
        room(
            109,
            "The Obsidian Throne - The Well of Souls",
            "The Obsidian Throne",
            false,
            "A dead-end shaft plunges into a red abyss, and from it rises a column \
             of the screaming, swirling damned, an updraft of agony that lights the \
             whole chamber the color of a wound. The court lies north.",
            &[(Dir::North, 108)],
        ),
        room(
            110,
            "The Obsidian Throne - The Throne of Mal'gareth",
            "The Obsidian Throne",
            false,
            "The world ends in a chamber of black glass and red fire, and upon a \
             throne grown from the realm itself sits the Archdemon Mal'gareth, vast \
             and patient and terribly amused, rising now to its full and dreadful \
             height to greet the mortal who came so very far only to kneel. The court \
             lies west.",
            &[(Dir::West, 108)],
        ),
    ];

    let spawns = vec![
        MobSpawn {
            id: 1,
            name: "a scrawny goblin",
            home: 6,
            max_hp: 18,
            damage: 3,
            xp: 12,
            respawn_secs: 30,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 2,
            name: "a road bandit",
            home: 8,
            max_hp: 26,
            damage: 5,
            xp: 20,
            respawn_secs: 45,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 3,
            name: "a gaunt wolf",
            home: 9,
            max_hp: 22,
            damage: 4,
            xp: 16,
            respawn_secs: 40,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        // ---- Whisperwood (tier 2-3) -------------------------------------
        MobSpawn {
            id: 10,
            name: "a snarling wolf",
            home: 16,
            max_hp: 30,
            damage: 6,
            xp: 26,
            respawn_secs: 45,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 11,
            name: "a giant forest spider",
            home: 18,
            max_hp: 34,
            damage: 7,
            xp: 30,
            respawn_secs: 50,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Poison, None, None),
        },
        MobSpawn {
            id: 12,
            name: "a bog-rotted corpse",
            home: 24,
            max_hp: 38,
            damage: 6,
            xp: 32,
            respawn_secs: 50,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Boss: Whisperwood
        MobSpawn {
            id: 13,
            name: "the Elder Treant",
            home: 28,
            max_hp: 120,
            damage: 12,
            xp: 150,
            respawn_secs: 300,
            loot: &[1006, 1110, 1111, 1201, 1301],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Physical),
                Some(DamageType::Fire),
            ),
        },
        // ---- Duskhollow Caverns (tier 3-4) ------------------------------
        MobSpawn {
            id: 20,
            name: "a clattering skeleton",
            home: 34,
            max_hp: 44,
            damage: 8,
            xp: 40,
            respawn_secs: 55,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 21,
            name: "a cave lurker",
            home: 40,
            max_hp: 50,
            damage: 9,
            xp: 46,
            respawn_secs: 55,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 22,
            name: "a grave-cold wraith",
            home: 48,
            max_hp: 54,
            damage: 10,
            xp: 52,
            respawn_secs: 60,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Boss: Duskhollow Caverns
        MobSpawn {
            id: 23,
            name: "the Bone Tyrant",
            home: 44,
            max_hp: 180,
            damage: 16,
            xp: 220,
            respawn_secs: 300,
            loot: &[1105, 1112, 1113, 1202, 1302],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // ---- Drowned Crypts (tier 4-5) ----------------------------------
        MobSpawn {
            id: 30,
            name: "a drowned revenant",
            home: 54,
            max_hp: 60,
            damage: 11,
            xp: 60,
            respawn_secs: 60,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 31,
            name: "a crypt ghoul",
            home: 58,
            max_hp: 66,
            damage: 12,
            xp: 66,
            respawn_secs: 60,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 32,
            name: "a pale drowned thing",
            home: 61,
            max_hp: 70,
            damage: 13,
            xp: 72,
            respawn_secs: 65,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // Boss: Drowned Crypts
        MobSpawn {
            id: 33,
            name: "the Lich Vael",
            home: 62,
            max_hp: 240,
            damage: 20,
            xp: 320,
            respawn_secs: 360,
            loot: &[1008, 1115, 1204, 1302],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // ---- Emberpeak Mines (tier 5-6) ---------------------------------
        MobSpawn {
            id: 40,
            name: "a molten husk",
            home: 69,
            max_hp: 78,
            damage: 14,
            xp: 80,
            respawn_secs: 65,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        MobSpawn {
            id: 41,
            name: "a forge-wight",
            home: 72,
            max_hp: 84,
            damage: 15,
            xp: 88,
            respawn_secs: 70,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        MobSpawn {
            id: 42,
            name: "an ember salamander",
            home: 73,
            max_hp: 90,
            damage: 16,
            xp: 96,
            respawn_secs: 70,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        // Boss: Emberpeak Mines
        MobSpawn {
            id: 43,
            name: "the Magma Colossus",
            home: 77,
            max_hp: 320,
            damage: 26,
            xp: 440,
            respawn_secs: 360,
            loot: &[1009, 1116, 1117, 1205, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        // ---- Frostspire Ascent (tier 6-7) -------------------------------
        MobSpawn {
            id: 50,
            name: "a frost-bound revenant",
            home: 84,
            max_hp: 96,
            damage: 17,
            xp: 104,
            respawn_secs: 70,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        MobSpawn {
            id: 51,
            name: "a rime-clawed wendigo",
            home: 88,
            max_hp: 104,
            damage: 19,
            xp: 116,
            respawn_secs: 75,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        MobSpawn {
            id: 52,
            name: "an ice-wraith",
            home: 91,
            max_hp: 110,
            damage: 20,
            xp: 124,
            respawn_secs: 75,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // Boss: Frostspire Ascent
        MobSpawn {
            id: 53,
            name: "the Wyrm of Frostspire",
            home: 92,
            max_hp: 420,
            damage: 32,
            xp: 600,
            respawn_secs: 420,
            loot: &[1007, 1117, 1205, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // ---- The Sunken Citadel (tier 7-8) ------------------------------
        // Bloodless citadel constructs: they shrug off venom, not steel. A
        // Physical resist on a regular is a zone-wide tax on the seven
        // Physical-locked classes with no counterplay, so it lives on bosses
        // only (the world resist/weak pass, rule 2 - enforced globally by
        // `no_regular_resists_physical_and_nothing_is_weak_to_physical`).
        MobSpawn {
            id: 60,
            name: "a faceless sentinel",
            home: 99,
            max_hp: 120,
            damage: 22,
            xp: 140,
            respawn_secs: 80,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Poison),
                Some(DamageType::Arcane),
            ),
        },
        MobSpawn {
            id: 61,
            name: "an iron-bound horror",
            home: 100,
            max_hp: 130,
            damage: 24,
            xp: 152,
            respawn_secs: 80,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Poison),
                Some(DamageType::Arcane),
            ),
        },
        MobSpawn {
            id: 62,
            name: "a whispering shade",
            home: 104,
            max_hp: 140,
            damage: 26,
            xp: 164,
            respawn_secs: 85,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Boss: The Sunken Citadel
        MobSpawn {
            id: 63,
            name: "the Fallen Paladin",
            home: 103,
            max_hp: 520,
            damage: 38,
            xp: 820,
            respawn_secs: 420,
            loot: &[1109, 1118, 1202, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Holy,
                Some(DamageType::Physical),
                Some(DamageType::Shadow),
            ),
        },
        // ---- The Obsidian Throne (tier 9-10) ----------------------------
        MobSpawn {
            id: 70,
            name: "a cinder fiend",
            home: 107,
            max_hp: 160,
            damage: 30,
            xp: 200,
            respawn_secs: 90,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 71,
            name: "a lava-throned demon",
            home: 108,
            max_hp: 180,
            damage: 33,
            xp: 230,
            respawn_secs: 90,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 72,
            name: "a soul-wracked horror",
            home: 109,
            max_hp: 200,
            damage: 36,
            xp: 260,
            respawn_secs: 95,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Final boss of the authored core (the Frontier and the continents were
        // hung past him later). Damage sits at ~1.6x the trash of his approach,
        // matching every other boss on this ladder: at the old 48 he was the
        // only one you could out-tank rather than out-play, a sponge players
        // walked through. `Spawn::level()` is unchanged by this (he stays Lv61
        // until 1140 power, i.e. damage 85), so the ladder's numbers hold.
        MobSpawn {
            id: 73,
            name: "the Archdemon Mal'gareth",
            home: 110,
            max_hp: 800,
            damage: 58,
            xp: 1500,
            respawn_secs: 600,
            loot: &[1009, 1119, 1205, 1401],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Wayfarer's Hollow's training dummy: generous hp so a fight lasts a
        // few rounds, near-nothing damage so it can never actually kill a
        // fresh level-1 character, fast respawn so the yard is never empty.
        MobSpawn {
            id: 40_000,
            name: "a straw training dummy",
            home: TUTORIAL_BASE + 1,
            max_hp: 60,
            damage: 1,
            xp: 5,
            respawn_secs: 15,
            loot: &[],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
    ];

    let mut rooms: HashMap<RoomId, Room> = rooms.into_iter().map(|r| (r.id, r)).collect();
    let mut spawns = spawns;

    // Append the deeper-exploration wings (rooms 300+), reciprocal by construction.
    extend_world(&mut rooms, &mut spawns);

    // Append the overworld: 100 rooms of new biomes and the three capital cities
    // (rooms 600+), reachable from Embergate's South Gate.
    extend_overworld(&mut rooms, &mut spawns);

    // Append the Frontier: 1000 procedurally-composed rooms across twenty themed
    // zones (rooms 2000+), hung off Embergate and populated with the 40-type
    // frontier roster and generated loot.
    extend_frontier(&mut rooms, &mut spawns);

    // Append the living-world maze/cave regions, each hung off a capital and
    // populated with roaming, behavior-driven foes:
    //   - Sunken Catacombs (rooms 5000+, off Tasmania) - undead crypt maze
    //   - Thornwood Hollows (rooms 5200+, off Melvanala) - forest maze
    //   - Drowned Caverns  (rooms 5400+, off Matlatesh) - cellular-automata cave
    let mut behaviors: HashMap<u32, MobBehavior> = HashMap::new();
    extend_catacombs(&mut rooms, &mut spawns, &mut behaviors);
    extend_thornwood(&mut rooms, &mut spawns, &mut behaviors);
    extend_caverns(&mut rooms, &mut spawns, &mut behaviors);

    // Append the Sundered Reaches: a second ~900-room continent (rooms 10000+),
    // a drowned sea-realm of braided mazes and organic caverns hung off the
    // Matlatesh capital. Runs after the maze regions so its free-direction
    // gateway search avoids the cavern portal.
    extend_reaches(&mut rooms, &mut spawns, &mut behaviors);

    // Append Kaelmyr, the Ashen Reach: a third ~1900-room continent (rooms
    // 12000+), a burnt ash-land of braided mazes and organic calderas hung off
    // Yssgar's chamber in the Reaches and gated behind the Bane of Yssgar. Runs
    // after the Reaches so its sea-gate search can find Yssgar's home room.
    extend_kaelmyr(&mut rooms, &mut spawns, &mut behaviors);

    // Append the Sunderlakes: a large, peaceful ~1200-room water country (rooms
    // 16000+) of reed-mazes and flooded caverns hung off the Melvanala high lake
    // by a normal walk. Mid-game friendly; the draw is the fishing (forty fish
    // species caught at Fishing-gated resource nodes).
    extend_lakes(&mut rooms, &mut spawns, &mut behaviors);

    // Append Broceliande, the Greenwood: a fourth ~2000-room continent (rooms
    // 22000+) of deep-green oakwoods and steaming jungles, druid circles and
    // briar mazes, hung off the Verdant Highlands (the Faerie Hollow) by a normal
    // walk. A moderate green country and the home of the fifty tameable beasts of
    // the animal-taming trade (whose roaming spots are seeded in `taming.rs`).
    extend_broceliande(&mut rooms, &mut spawns, &mut behaviors);

    // Append Aelunor, the Faewood: a twelve-zone sprawling forest (rooms
    // 25000+) of elves, high elves, druids, and fae, every zone an organic
    // cavern-carved glade (never a maze, never a grid), hung off the Amber
    // Savanna's terminal room by a normal walk east. Home of the Aelunor
    // hundred-creature roster and its own city, Silvael.
    extend_aelunor(&mut rooms, &mut spawns, &mut behaviors);
    extend_silvael(&mut rooms);

    // Append the Wildbound Waste: a Felucca-style pvp continent (rooms
    // 30000+) of three chained biomes - Duskmire Wood, the Hollowdeep, and
    // the Scorched Flats - hung off the Sahra Wastes' Sand-Wyrm's Maw. Every
    // field room here is `pvp: true`; only its three small gate towns are
    // safe. Runs after Broceliande so its gateway search finds room 751.
    extend_wildbound(&mut rooms, &mut spawns, &mut behaviors);

    // Flesh out the four capitals with a district of new safe rooms each.
    extend_cities(&mut rooms);

    // Append the player-housing district (Hearthward Close, rooms 9000+), a
    // public street of claimable homes hung off Embergate's Market Row. No mobs:
    // homes are safe. Ownership and furnishings are runtime side-state.
    extend_housing(&mut rooms);

    // Append the Shattered Archipelago: safe portal-linked villages (rooms 8000+)
    // and a thousand rooms of maze/cavern islands (rooms 20000+), each with a
    // boss. Reached by waystone portals, not by walking (see `svc.rs`).
    extend_villages(&mut rooms);
    extend_archipelago(&mut rooms, &mut spawns, &mut behaviors);

    tune_spawn_balance(&mut spawns);
    tune_crowns(&mut spawns);

    // Per-zone level bands, read off the tuned spawns so the numbers players
    // see ("King's Road · Lv 2-5") always reflect what actually prowls there.
    let mut zone_bands: HashMap<&'static str, (i32, i32)> = HashMap::new();
    for s in &spawns {
        let Some(room) = rooms.get(&s.home) else {
            continue;
        };
        let level = s.level();
        zone_bands
            .entry(room.zone)
            .and_modify(|(lo, hi)| {
                *lo = (*lo).min(level);
                *hi = (*hi).max(level);
            })
            .or_insert((level, level));
    }

    World {
        rooms,
        spawns,
        start_room: 1,
        behaviors,
        zone_bands,
    }
}

// ---- The Shattered Archipelago: villages + island isles (rooms 8000/20000+) --

/// Build the safe portal villages (rooms 8000+). Each is a single flavourful
/// haven with a waystone (and a fountain, via `waystone_features`); they are
/// reached only by portal, so they carry no directional exits.
fn extend_villages(rooms: &mut HashMap<RoomId, Room>) {
    for (i, (name, blurb)) in super::archipelago::VILLAGES.iter().enumerate() {
        let id = super::archipelago::village_room(i);
        rooms.insert(
            id,
            Room {
                id,
                name,
                desc: blurb,
                zone: name,
                safe: true,
                pvp: false,
                exits: HashMap::new(),
            },
        );
    }
}

/// Build the twenty islands (rooms 20000+). Each island is carved as a braided
/// maze or an organic cavern - never a grid - with its own scenery and a named
/// boss in the deepest room. Islands are independent (reached by portal), so
/// each landing is always safe.
/// Archipelago mobs live above the Reaches so they ride the same balance
/// multipliers (their authored base stats sit on that curve). Clear of the
/// Reaches' actual ids.
const ARCH_SPAWN_ID_START: u32 = 970_000;

/// An island boss's loot: the Reaches table it always drew from, plus the
/// island's own two Wildbound finds - a real step past even Kaelmyr, since
/// the Archipelago rides the same endgame curve one continent further.
fn archipelago_boss_loot(isle: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        (0..super::archipelago::ISLAND_COUNT)
            .map(|i| {
                let mut v = super::items::reaches_loot(i).to_vec();
                v.extend(super::items::archipelago_find_ids(i));
                v
            })
            .collect()
    });
    tables[isle.min(super::archipelago::ISLAND_COUNT - 1)].as_slice()
}

#[allow(clippy::needless_range_loop, clippy::type_complexity)]
fn extend_archipelago(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    use super::archipelago::{ARCH_H, ARCH_SEED, ARCH_W, ISLANDS, island_entrance};
    let (w, h) = (ARCH_W, ARCH_H);
    let n = w * h;
    let mut spawn_id: u32 = ARCH_SPAWN_ID_START;

    for (isle, &(iname, adj, ground, feature, creature, mob_names, boss)) in
        ISLANDS.iter().enumerate()
    {
        let ibase = island_entrance(isle);
        let tier = (isle + 14) as i32; // the isles sit at and beyond the Reaches
        let mut rng = MazeRng::new(ARCH_SEED ^ (isle as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

        // Every third island is an organic cavern; the rest are braided mazes. A
        // cavern that comes out too sparse falls back to a maze so none is empty.
        let cavern_floor = if isle % 3 == 2 {
            let floor = carve_cavern(w, h, &mut rng);
            (floor.iter().filter(|f| **f).count() >= 20).then_some(floor)
        } else {
            None
        };
        let (entrance, reachable, dist, cell_exits): (
            usize,
            Vec<bool>,
            Vec<usize>,
            Vec<Vec<(Dir, usize)>>,
        ) = if let Some(floor) = cavern_floor {
            let entrance = (0..n).find(|&i| floor[i]).unwrap_or(0);
            let dist = cavern_distances(&floor, w, h, entrance);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut ev = Vec::new();
                    if !reachable[c] {
                        return ev;
                    }
                    let (x, y) = (c % w, c / w);
                    let consider = |nx: i64, ny: i64, d: Dir, ev: &mut Vec<(Dir, usize)>| {
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let nb = ny as usize * w + nx as usize;
                            if reachable[nb] {
                                ev.push((d, nb));
                            }
                        }
                    };
                    consider(x as i64, y as i64 - 1, Dir::North, &mut ev);
                    consider(x as i64 + 1, y as i64, Dir::East, &mut ev);
                    consider(x as i64, y as i64 + 1, Dir::South, &mut ev);
                    consider(x as i64 - 1, y as i64, Dir::West, &mut ev);
                    ev
                })
                .collect();
            (entrance, reachable, dist, exits)
        } else {
            let open = carve_maze(w, h, &mut rng);
            let dist = maze_distances(&open, w, h, 0);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut ev = Vec::new();
                    if !reachable[c] {
                        return ev;
                    }
                    for d in 0..4 {
                        if open[c][d]
                            && let Some(nb) = maze_neighbor(c, d, w, h)
                        {
                            ev.push((DIRS[d], nb));
                        }
                    }
                    ev
                })
                .collect();
            (0, reachable, dist, exits)
        };

        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let zone: &'static str = leak(iname.to_string());

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = ibase + cell as RoomId;
            let is_entrance = cell == entrance;
            let is_boss = cell == deepest && cell != entrance;
            let degree = cell_exits[cell].len();

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, ibase + *nb as RoomId))
                .collect();

            let name: &'static str = if is_entrance {
                leak(format!("{iname} - the Waystone Landing"))
            } else if is_boss {
                leak(format!("{iname} - the Wild Heart"))
            } else {
                leak(format!("{iname} - {}", FRONTIER_PLACES[cell % 10]))
            };
            let desc: &'static str =
                leak(frontier_desc(adj, ground, feature, creature, cell as u32));

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone,
                    safe: is_entrance, // every landing is a safe haven with a portal
                    pvp: false,
                    exits,
                },
            );

            if is_entrance {
                continue;
            }

            let depth = dist[cell] as i32;
            let (mob_name, behavior, boss_mob, hp, dmg) = if is_boss {
                (
                    boss,
                    MobBehavior::Brute,
                    true,
                    1500 + tier * 240,
                    66 + tier * 6,
                )
            } else if degree == 1 {
                (
                    mob_names[0],
                    MobBehavior::Ambusher,
                    false,
                    860 + tier * 62 + depth * 6,
                    58 + tier * 4 + depth,
                )
            } else if degree >= 3 {
                (
                    mob_names[1],
                    if rng.chance(50) {
                        MobBehavior::PackHunter
                    } else {
                        MobBehavior::Summoner
                    },
                    false,
                    940 + tier * 72 + depth * 6,
                    60 + tier * 5 + depth,
                )
            } else {
                if rng.chance(35) {
                    continue;
                }
                let behavior = match rng.below(4) {
                    0 => MobBehavior::Wanderer,
                    1 => MobBehavior::Patroller,
                    2 => MobBehavior::Hunter,
                    _ => MobBehavior::Caster(DamageType::Frost),
                };
                (
                    mob_names[2],
                    behavior,
                    false,
                    860 + tier * 62 + depth * 6,
                    58 + tier * 4 + depth,
                )
            };
            let attack = match behavior {
                MobBehavior::Caster(school) => school,
                _ => DamageType::Physical,
            };
            // The boss wears the island's weakness but never its resist:
            // prep is a pure reward on the fight players provision for.
            let theme = super::archipelago::ISLAND_THEMES[isle];
            let profile = if boss_mob {
                DamageProfile::new(attack, None, theme.weak())
            } else {
                DamageProfile::new(attack, theme.resist(), theme.weak())
            };
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: hp,
                damage: dmg,
                xp: if boss_mob {
                    760 + tier * 92
                } else {
                    210 + tier * 40 + depth * 5
                },
                respawn_secs: if boss_mob { 600 } else { 90 },
                loot: if boss_mob {
                    archipelago_boss_loot(isle)
                } else {
                    super::items::reaches_loot(isle)
                },
                boss: boss_mob,
                profile,
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }
    }
}

// ---- The Sunken Catacombs: a braided maze region (rooms 5000+) ------------
//
// Unlike the Frontier's 10x5 grids (every cell wired to all four neighbours),
// the Catacombs are carved as a maze: a recursive-backtracker passes over a
// logical grid and only opens the walls it visits, then a braiding pass knocks
// a few extra walls through so the result has dead-ends, winding corridors,
// junctions, and loops rather than uniform blocks. Generation is fully
// deterministic (fixed-seed xorshift) so the world is identical every boot and
// the invariant tests stay stable.

const CATACOMBS_BASE: RoomId = 5000;
const CATACOMBS_W: usize = 12;
const CATACOMBS_H: usize = 8;
const CATACOMBS_SPAWN_ID_START: u32 = 800_000;
const CATACOMBS_SEED: u64 = 0xCA7A_C0DE_u64;
const CATACOMBS_REGULAR_HP_CAP: i32 = 220;
const CATACOMBS_REGULAR_DAMAGE_CAP: i32 = 18;

// Thornwood Hollows: a second braided maze (same carver as the Catacombs) with
// a living-forest skin, hung off the Melvanala capital. Rooms 5200+.
const THORNWOOD_BASE: RoomId = 5200;
const THORNWOOD_W: usize = 12;
const THORNWOOD_H: usize = 8;
const THORNWOOD_SPAWN_ID_START: u32 = 810_000;
const THORNWOOD_SEED: u64 = 0x7B05_C0DE_u64;
const THORNWOOD_REGULAR_HP_CAP: i32 = 225;
const THORNWOOD_REGULAR_DAMAGE_CAP: i32 = 18;

// Drowned Caverns: an organic cave region carved by cellular automata (not a
// maze), hung off the Matlatesh capital. Rooms 5400+ (sparse: only floor cells
// in the largest connected cavern become rooms).
const CAVERNS_BASE: RoomId = 5400;
const CAVERNS_W: usize = 14;
const CAVERNS_H: usize = 10;
const CAVERNS_SPAWN_ID_START: u32 = 820_000;
const CAVERNS_SEED: u64 = 0xCA7E_0CEA_u64;
const CAVERNS_REGULAR_HP_CAP: i32 = 240;
const CAVERNS_REGULAR_DAMAGE_CAP: i32 = 19;

/// A tiny deterministic xorshift64 PRNG, so maze carving never depends on the
/// global RNG (the world must build identically every time).
struct MazeRng(u64);

impl MazeRng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next_u64() % 100 < pct
    }
}

/// Open-wall flags per cell in [N, E, S, W] order, matching the deltas below.
type Walls = [bool; 4];
const DIRS: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

/// The in-bounds neighbour cell index in direction `d`, if any.
fn maze_neighbor(cell: usize, d: usize, w: usize, h: usize) -> Option<usize> {
    let (cx, cy) = (cell % w, cell / w);
    match d {
        0 if cy > 0 => Some(cell - w),
        1 if cx + 1 < w => Some(cell + 1),
        2 if cy + 1 < h => Some(cell + w),
        3 if cx > 0 => Some(cell - 1),
        _ => None,
    }
}

/// Carve a braided maze over `w*h` cells: a perfect maze via randomized DFS,
/// then ~30% of dead-ends opened to make loops. Returns the open-wall flags.
#[allow(clippy::needless_range_loop)] // `d` indexes the [N,E,S,W] wall array AND maps to a Dir
fn carve_maze(w: usize, h: usize, rng: &mut MazeRng) -> Vec<Walls> {
    let n = w * h;
    let mut open = vec![[false; 4]; n];
    let mut visited = vec![false; n];
    let mut stack = vec![0usize];
    visited[0] = true;
    while let Some(&cur) = stack.last() {
        let mut frontier: Vec<(usize, usize)> = Vec::new();
        for d in 0..4 {
            if let Some(nb) = maze_neighbor(cur, d, w, h)
                && !visited[nb]
            {
                frontier.push((d, nb));
            }
        }
        if frontier.is_empty() {
            stack.pop();
            continue;
        }
        let (d, nb) = frontier[rng.below(frontier.len())];
        open[cur][d] = true;
        open[nb][(d + 2) % 4] = true;
        visited[nb] = true;
        stack.push(nb);
    }
    // Braid: relieve dead-ends so the maze has loops, not just one true path.
    for cell in 0..n {
        if open[cell].iter().filter(|o| **o).count() != 1 || !rng.chance(30) {
            continue;
        }
        let mut cand: Vec<(usize, usize)> = Vec::new();
        for d in 0..4 {
            if !open[cell][d]
                && let Some(nb) = maze_neighbor(cell, d, w, h)
            {
                cand.push((d, nb));
            }
        }
        if !cand.is_empty() {
            let (d, nb) = cand[rng.below(cand.len())];
            open[cell][d] = true;
            open[nb][(d + 2) % 4] = true;
        }
    }
    open
}

/// BFS distance from `start` over the carved passages; `usize::MAX` if unreached.
#[allow(clippy::needless_range_loop)] // `d` indexes the [N,E,S,W] wall array AND maps to a Dir
fn maze_distances(open: &[Walls], w: usize, h: usize, start: usize) -> Vec<usize> {
    let mut dist = vec![usize::MAX; open.len()];
    dist[start] = 0;
    let mut queue = VecDeque::from([start]);
    while let Some(cell) = queue.pop_front() {
        for d in 0..4 {
            if open[cell][d]
                && let Some(nb) = maze_neighbor(cell, d, w, h)
                && dist[nb] == usize::MAX
            {
                dist[nb] = dist[cell] + 1;
                queue.push_back(nb);
            }
        }
    }
    dist
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn capped_depth_scale(base: i32, per_depth: i32, depth: i32, cap: i32) -> i32 {
    (base + depth.max(0) * per_depth).min(cap)
}

/// Build the Sunken Catacombs maze, its roaming undead, and the behavior map,
/// and hang the entrance off the Tasmania capital square.
#[allow(clippy::needless_range_loop)] // `d` indexes the [N,E,S,W] wall array AND maps to a Dir
fn extend_catacombs(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (CATACOMBS_W, CATACOMBS_H);
    let mut rng = MazeRng::new(CATACOMBS_SEED);
    let open = carve_maze(w, h, &mut rng);
    let dist = maze_distances(&open, w, h, 0);
    // The goal vault is the reachable cell farthest from the entrance.
    let vault = (0..w * h)
        .filter(|&c| dist[c] != usize::MAX)
        .max_by_key(|&c| dist[c])
        .unwrap_or(0);

    const ATMOS: [&str; 6] = [
        "Bone-dust hangs in the still air",
        "Water seeps black between the flagstones",
        "Niche after niche gapes empty in the walls",
        "Cold breathes up from somewhere below",
        "Guttering grave-lamps throw long shadows",
        "Roots have prised the old masonry apart",
    ];
    const SHAPE: [&str; 6] = [
        "a low barrel-vaulted passage",
        "a cramped ossuary gallery",
        "a junction of slumping arches",
        "a collapsed burial chamber",
        "a winding stair-cut tunnel",
        "a pillared crypt-hall",
    ];
    const SOUND: [&str; 6] = [
        "water drips somewhere out of sight",
        "your own breath sounds too loud",
        "something skitters away unseen",
        "the dark swallows every echo",
        "a draught moans through unseen cracks",
        "loose grit shifts underfoot",
    ];
    const DETAIL: [&str; 6] = [
        "Centuries of grave-goods have long since been looted",
        "Faded sigils ward the lintels against whatever sleeps below",
        "Stacked skulls watch from the shadowed niches",
        "The flagstones are worn smooth by older feet than yours",
        "Damp has bloomed the walls with pale, patient fungus",
        "A cold current tugs steadily on toward the deep",
    ];

    let zone: &'static str = "The Sunken Catacombs";
    let mut spawn_id = CATACOMBS_SPAWN_ID_START;
    // Undead resist shadow and decay, but holy light withers them.
    let undead = DamageProfile::new(
        DamageType::Physical,
        Some(DamageType::Shadow),
        Some(DamageType::Holy),
    );

    for cell in 0..w * h {
        if dist[cell] == usize::MAX {
            continue; // unreachable pocket (shouldn't happen post-braid, but be safe)
        }
        let id = CATACOMBS_BASE + cell as u32;
        let degree = open[cell].iter().filter(|o| **o).count();
        let is_entrance = cell == 0;
        let is_vault = cell == vault;

        let mut exits: HashMap<Dir, RoomId> = HashMap::new();
        for d in 0..4 {
            if open[cell][d]
                && let Some(nb) = maze_neighbor(cell, d, w, h)
            {
                exits.insert(DIRS[d], CATACOMBS_BASE + nb as u32);
            }
        }

        let name: &'static str = if is_entrance {
            "Catacombs - Mouth of the Crypt"
        } else if is_vault {
            "Catacombs - The Drowned Reliquary"
        } else {
            leak(format!(
                "Catacombs - {}",
                SHAPE[(cell.wrapping_mul(7)) % SHAPE.len()]
            ))
        };
        let desc: &'static str = if is_entrance {
            "A stair descends from the Tasmania boneyard into still, lamp-lit dark. \
             This threshold is hallowed ground - nothing dead will cross it. The \
             passages beyond branch and double back into the deep."
        } else if is_vault {
            leak(format!(
                "The maze gives onto a flooded reliquary, its black water mirroring \
                 a vaulted ceiling lost in dark. {}. Whatever the Catacombs were \
                 built to keep, it waits here.",
                ATMOS[(cell.wrapping_mul(5)) % ATMOS.len()]
            ))
        } else {
            leak(format!(
                "You stand in {}, its stones slick and cold. {}, and {}. {}. {}.",
                SHAPE[(cell.wrapping_mul(7)) % SHAPE.len()],
                ATMOS[(cell.wrapping_mul(3)) % ATMOS.len()],
                SOUND[(cell.wrapping_mul(11)) % SOUND.len()],
                DETAIL[(cell.wrapping_mul(13)) % DETAIL.len()],
                if degree >= 3 {
                    "Several passages meet here"
                } else if degree == 1 {
                    "The way ends in a sealed burial cell"
                } else {
                    "The corridor presses on into the dark"
                }
            ))
        };

        rooms.insert(
            id,
            Room {
                id,
                name,
                desc,
                zone,
                safe: is_entrance,
                pvp: false,
                exits,
            },
        );

        if is_entrance {
            continue;
        }

        // Place a behavior-driven undead based on the room's role in the maze.
        let depth = dist[cell] as i32;
        let (mob_name, behavior, boss, hp, dmg) = if is_vault {
            ("The Bonewright Lich", MobBehavior::Summoner, true, 360, 22)
        } else if degree == 1 {
            // Dead-end lairs: things that lie in wait.
            if rng.chance(50) {
                (
                    "a Tomb Lurker",
                    MobBehavior::Ambusher,
                    false,
                    capped_depth_scale(90, 6, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                )
            } else {
                (
                    "a Grave Rat",
                    MobBehavior::Thief,
                    false,
                    capped_depth_scale(60, 4, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(8, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                )
            }
        } else if degree >= 3 {
            // Junctions: things that bring friends.
            if rng.chance(55) {
                (
                    "a Ghoul Packmaster",
                    MobBehavior::PackHunter,
                    false,
                    capped_depth_scale(110, 6, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(13, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                )
            } else {
                (
                    "a Bone Broodmother",
                    MobBehavior::Summoner,
                    false,
                    capped_depth_scale(120, 6, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                )
            }
        } else {
            // Corridors: things that move.
            match rng.below(5) {
                0 => (
                    "a Shambling Skeleton",
                    MobBehavior::Wanderer,
                    false,
                    capped_depth_scale(80, 5, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                ),
                1 => (
                    "a Crypt Wight",
                    MobBehavior::Patroller,
                    false,
                    capped_depth_scale(95, 5, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(11, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                ),
                2 => (
                    "a Barrow Wraith",
                    MobBehavior::Hunter,
                    false,
                    capped_depth_scale(90, 5, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                ),
                3 => (
                    "a Pale Acolyte",
                    MobBehavior::Caster(DamageType::Shadow),
                    false,
                    capped_depth_scale(85, 5, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                ),
                _ => (
                    "a Cinder Shade",
                    MobBehavior::Caster(DamageType::Fire),
                    false,
                    capped_depth_scale(85, 5, depth, CATACOMBS_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, CATACOMBS_REGULAR_DAMAGE_CAP),
                ),
            }
        };
        // Leave some corridors quiet so the maze breathes.
        if !is_vault && degree == 2 && rng.chance(35) {
            continue;
        }

        let profile = match behavior {
            MobBehavior::Caster(school) => {
                DamageProfile::new(school, Some(DamageType::Shadow), Some(DamageType::Holy))
            }
            _ => undead,
        };
        spawns.push(MobSpawn {
            id: spawn_id,
            name: mob_name,
            home: id,
            max_hp: hp,
            damage: dmg,
            xp: 30 + depth * 8 + if boss { 400 } else { 0 },
            respawn_secs: if boss { 600 } else { 75 },
            loot: if boss {
                CATACOMBS_BOSS_LOOT
            } else {
                CATACOMBS_COMMON_LOOT
            },
            boss,
            profile,
        });
        behaviors.insert(spawn_id, behavior);
        spawn_id += 1;
    }

    // Hang the crypt mouth off the Tasmania capital square via a free direction.
    let entrance = CATACOMBS_BASE;
    let portal = [Dir::Down, Dir::East, Dir::West, Dir::North]
        .into_iter()
        .find(|d| {
            rooms
                .get(&TASMANIA_SQUARE)
                .is_some_and(|r| !r.exits.contains_key(d))
        })
        .unwrap_or(Dir::Down);
    if let Some(sq) = rooms.get_mut(&TASMANIA_SQUARE) {
        sq.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), TASMANIA_SQUARE);
    }
}

// ---- Thornwood Hollows: a living-forest braided maze (rooms 5200+) --------
//
// Same `carve_maze` as the Catacombs, dressed as a tangled wood and stocked
// with beasts and fae: pack-hunters at the junctions, ambushers in the
// dead-end thickets. Hung off the Melvanala capital.
#[allow(clippy::needless_range_loop)] // `d` indexes the [N,E,S,W] wall array AND maps to a Dir
fn extend_thornwood(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (THORNWOOD_W, THORNWOOD_H);
    let mut rng = MazeRng::new(THORNWOOD_SEED);
    let open = carve_maze(w, h, &mut rng);
    let dist = maze_distances(&open, w, h, 0);
    let vault = (0..w * h)
        .filter(|&c| dist[c] != usize::MAX)
        .max_by_key(|&c| dist[c])
        .unwrap_or(0);

    const ATMOS: [&str; 6] = [
        "Dappled green light filters through a roof of leaves",
        "Brambles claw at your sleeves from every side",
        "Toadstools crowd the roots in pale rings",
        "Birdsong stops the moment you stand still",
        "A mist beads cold on the ferns",
        "Old growth leans close overhead",
    ];
    const SHAPE: [&str; 6] = [
        "a close green tunnel of thorn",
        "a deer-trodden hollow",
        "a fork of root-buckled paths",
        "a fern-choked dell",
        "a moss-soft glade",
        "a stand of grey old oaks",
    ];
    const SOUND: [&str; 6] = [
        "wind hisses through the canopy",
        "something heavy moves off through the brush",
        "a branch cracks behind you",
        "water chuckles in an unseen brook",
        "leaves whisper with no wind to move them",
        "a bird shrieks once and falls silent",
    ];
    const DETAIL: [&str; 6] = [
        "Game-trails knot and double back through the thorns",
        "Strange cairns of antler and bone mark the way",
        "Fae-rings of mushroom dot the shadowed turf",
        "Claw-scored bark warns off the wise",
        "Spider-silk catches the light between the boughs",
        "The wood seems to lean in and listen",
    ];

    let zone: &'static str = "The Thornwood Hollows";
    let mut spawn_id = THORNWOOD_SPAWN_ID_START;
    // Beasts: hardy and physical, but fire drives them off; some shrug off frost.
    let beast = DamageProfile::new(
        DamageType::Physical,
        Some(DamageType::Frost),
        Some(DamageType::Fire),
    );

    for cell in 0..w * h {
        if dist[cell] == usize::MAX {
            continue;
        }
        let id = THORNWOOD_BASE + cell as u32;
        let degree = open[cell].iter().filter(|o| **o).count();
        let is_entrance = cell == 0;
        let is_vault = cell == vault;

        let mut exits: HashMap<Dir, RoomId> = HashMap::new();
        for d in 0..4 {
            if open[cell][d]
                && let Some(nb) = maze_neighbor(cell, d, w, h)
            {
                exits.insert(DIRS[d], THORNWOOD_BASE + nb as u32);
            }
        }

        let name: &'static str = if is_entrance {
            "Thornwood - The Bramble Gate"
        } else if is_vault {
            "Thornwood - The Heart-Tree Grove"
        } else {
            leak(format!(
                "Thornwood - {}",
                SHAPE[(cell.wrapping_mul(7)) % SHAPE.len()]
            ))
        };
        let desc: &'static str = if is_entrance {
            "A deer-path leaves the Melvanala lakeside and slips under the eaves \
             of the old wood. The verge is tended ground - no beast will set foot \
             on it. Beyond, the green tunnels branch and tangle without end."
        } else if is_vault {
            leak(format!(
                "The thicket opens on a ring of standing oaks about one vast, \
                 silver-barked heart-tree. {}. The very air hums, and the leaves \
                 turn as one to watch you. Something ancient keeps this grove, and \
                 it has noticed that you came.",
                ATMOS[(cell.wrapping_mul(5)) % ATMOS.len()]
            ))
        } else {
            leak(format!(
                "You push into {}. {}, and {}. {}. {}. Old magic lies thick under the leaf-mould here.",
                SHAPE[(cell.wrapping_mul(7)) % SHAPE.len()],
                ATMOS[(cell.wrapping_mul(3)) % ATMOS.len()],
                SOUND[(cell.wrapping_mul(11)) % SOUND.len()],
                DETAIL[(cell.wrapping_mul(13)) % DETAIL.len()],
                if degree >= 3 {
                    "Trails meet and part here"
                } else if degree == 1 {
                    "The thorns close to a dead end"
                } else {
                    "The trail winds deeper in"
                }
            ))
        };

        rooms.insert(
            id,
            Room {
                id,
                name,
                desc,
                zone,
                safe: is_entrance,
                pvp: false,
                exits,
            },
        );
        if is_entrance {
            continue;
        }

        let depth = dist[cell] as i32;
        let (mob_name, behavior, boss, hp, dmg) = if is_vault {
            ("the Elder Dryad", MobBehavior::Summoner, true, 360, 22)
        } else if degree == 1 {
            if rng.chance(50) {
                (
                    "a Lurking Broodspider",
                    MobBehavior::Ambusher,
                    false,
                    capped_depth_scale(95, 6, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                )
            } else {
                (
                    "a Sly Vulpin",
                    MobBehavior::Thief,
                    false,
                    capped_depth_scale(65, 4, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(8, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                )
            }
        } else if degree >= 3 {
            if rng.chance(60) {
                (
                    "a Dire Wolf Alpha",
                    MobBehavior::PackHunter,
                    false,
                    capped_depth_scale(115, 6, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(13, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                )
            } else {
                (
                    "a Thornback Matron",
                    MobBehavior::Summoner,
                    false,
                    capped_depth_scale(120, 6, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                )
            }
        } else {
            match rng.below(5) {
                0 => (
                    "a Tusked Boar",
                    MobBehavior::Wanderer,
                    false,
                    capped_depth_scale(90, 5, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(11, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                ),
                1 => (
                    "a Wood-Stalker",
                    MobBehavior::Hunter,
                    false,
                    capped_depth_scale(90, 5, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                ),
                2 => (
                    "an Antlered Sentinel",
                    MobBehavior::Patroller,
                    false,
                    capped_depth_scale(100, 5, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(11, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                ),
                3 => (
                    "a Spiteful Pixie",
                    MobBehavior::Caster(DamageType::Arcane),
                    false,
                    capped_depth_scale(80, 5, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                ),
                _ => (
                    "a Will-o'-Wisp",
                    MobBehavior::Caster(DamageType::Fire),
                    false,
                    capped_depth_scale(80, 5, depth, THORNWOOD_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, THORNWOOD_REGULAR_DAMAGE_CAP),
                ),
            }
        };
        if !is_vault && degree == 2 && rng.chance(35) {
            continue;
        }

        let profile = match behavior {
            MobBehavior::Caster(school) => {
                DamageProfile::new(school, Some(DamageType::Frost), Some(DamageType::Fire))
            }
            _ => beast,
        };
        spawns.push(MobSpawn {
            id: spawn_id,
            name: mob_name,
            home: id,
            max_hp: hp,
            damage: dmg,
            xp: 30 + depth * 8 + if boss { 400 } else { 0 },
            respawn_secs: if boss { 600 } else { 75 },
            loot: if boss {
                THORNWOOD_BOSS_LOOT
            } else {
                THORNWOOD_COMMON_LOOT
            },
            boss,
            profile,
        });
        behaviors.insert(spawn_id, behavior);
        spawn_id += 1;
    }

    let entrance = THORNWOOD_BASE;
    let portal = [Dir::North, Dir::East, Dir::West, Dir::Down]
        .into_iter()
        .find(|d| {
            rooms
                .get(&MELVANALA_SQUARE)
                .is_some_and(|r| !r.exits.contains_key(d))
        })
        .unwrap_or(Dir::North);
    if let Some(sq) = rooms.get_mut(&MELVANALA_SQUARE) {
        sq.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), MELVANALA_SQUARE);
    }
}

// ---- Drowned Caverns: a cellular-automata cave (rooms 5400+) --------------
//
// Unlike the maze regions, the caverns are grown, not carved: a noise field is
// smoothed by a few cellular-automata passes into open chambers and winding
// galleries, then only the single largest connected pocket is kept (so there
// are never unreachable rooms). Each surviving floor cell is a room linked to
// its orthogonal floor neighbours.

/// Grow a cave: returns a floor mask over `w*h` cells, true only for cells in
/// the largest connected open region. Deterministic for a fixed seed.
#[allow(clippy::needless_range_loop)] // `i` indexes the flat cell grid by (x,y) math
fn carve_cavern(w: usize, h: usize, rng: &mut MazeRng) -> Vec<bool> {
    let n = w * h;
    let mut cell = vec![false; n];
    for i in 0..n {
        let (x, y) = (i % w, i / w);
        // A solid rock border frames the cave; the interior starts as noise.
        cell[i] = !(x == 0 || y == 0 || x == w - 1 || y == h - 1) && rng.chance(54);
    }
    // Smooth: a cell is open unless it is crowded by rock (classic 4-5 rule).
    for _ in 0..4 {
        let mut next = cell.clone();
        for i in 0..n {
            let (x, y) = (i % w, i / w);
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                next[i] = false;
                continue;
            }
            let mut rock = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = (x as i32 + dx) as usize;
                    let ny = (y as i32 + dy) as usize;
                    if !cell[ny * w + nx] {
                        rock += 1;
                    }
                }
            }
            next[i] = rock < 5;
        }
        cell = next;
    }
    // Keep only the largest connected open pocket so nothing is stranded.
    let mut comp = vec![usize::MAX; n];
    let mut sizes: Vec<usize> = Vec::new();
    for start in 0..n {
        if !cell[start] || comp[start] != usize::MAX {
            continue;
        }
        let cid = sizes.len();
        let mut stack = vec![start];
        comp[start] = cid;
        let mut size = 0usize;
        while let Some(c) = stack.pop() {
            size += 1;
            let (x, y) = (c % w, c / w);
            let push = |nx: usize, ny: usize, stack: &mut Vec<usize>, comp: &mut Vec<usize>| {
                let ni = ny * w + nx;
                if cell[ni] && comp[ni] == usize::MAX {
                    comp[ni] = cid;
                    stack.push(ni);
                }
            };
            if x > 0 {
                push(x - 1, y, &mut stack, &mut comp);
            }
            if x + 1 < w {
                push(x + 1, y, &mut stack, &mut comp);
            }
            if y > 0 {
                push(x, y - 1, &mut stack, &mut comp);
            }
            if y + 1 < h {
                push(x, y + 1, &mut stack, &mut comp);
            }
        }
        sizes.push(size);
    }
    let largest = sizes
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| **s)
        .map(|(i, _)| i);
    (0..n)
        .map(|i| largest.is_some_and(|lc| comp[i] == lc))
        .collect()
}

/// BFS distances over a cavern floor mask (4-neighbour), `usize::MAX` if unreached.
fn cavern_distances(floor: &[bool], w: usize, h: usize, start: usize) -> Vec<usize> {
    let mut dist = vec![usize::MAX; floor.len()];
    if !floor.get(start).copied().unwrap_or(false) {
        return dist;
    }
    dist[start] = 0;
    let mut queue = VecDeque::from([start]);
    while let Some(c) = queue.pop_front() {
        let (x, y) = (c % w, c / w);
        let step = |nx: usize, ny: usize, dist: &mut Vec<usize>, queue: &mut VecDeque<usize>| {
            let ni = ny * w + nx;
            if floor[ni] && dist[ni] == usize::MAX {
                dist[ni] = dist[c] + 1;
                queue.push_back(ni);
            }
        };
        if x > 0 {
            step(x - 1, y, &mut dist, &mut queue);
        }
        if x + 1 < w {
            step(x + 1, y, &mut dist, &mut queue);
        }
        if y > 0 {
            step(x, y - 1, &mut dist, &mut queue);
        }
        if y + 1 < h {
            step(x, y + 1, &mut dist, &mut queue);
        }
    }
    dist
}

fn extend_caverns(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (CAVERNS_W, CAVERNS_H);
    let mut rng = MazeRng::new(CAVERNS_SEED);
    let floor = carve_cavern(w, h, &mut rng);
    let entrance_cell = (0..w * h).find(|&i| floor[i]).unwrap_or(0);
    let dist = cavern_distances(&floor, w, h, entrance_cell);
    let vault = (0..w * h)
        .filter(|&c| dist[c] != usize::MAX)
        .max_by_key(|&c| dist[c])
        .unwrap_or(entrance_cell);

    const ATMOS: [&str; 6] = [
        "Dripping echoes lose themselves in the black",
        "Pale blind things flit from your torchlight",
        "The air is thick with brine and old water",
        "Flowstone glistens down every wall",
        "Phosphor fungus glows a sickly green",
        "A slow tide breathes somewhere below",
    ];
    const SHAPE: [&str; 6] = [
        "a dripping flowstone gallery",
        "a low crawl between slick boulders",
        "a vaulted sounding-chamber",
        "a brink above a sump of black water",
        "a forest of dripping columns",
        "a rubble-strewn collapse",
    ];
    const SOUND: [&str; 6] = [
        "water ticks from the unseen roof",
        "a far-off rockfall mutters and dies",
        "your light gutters in a cold draught",
        "something wet slides across stone",
        "the tide sighs in and out",
        "an echo answers that you did not make",
    ];
    const DETAIL: [&str; 6] = [
        "Eyeless cave-life clusters in the damp",
        "Salt rimes the high-water mark on the walls",
        "Bones of the drowned have fetched up in the cracks",
        "Curtains of mineral hang razor-thin",
        "The floor shelves away into lightless water",
        "Old scratch-marks score the softer stone",
    ];

    let zone: &'static str = "The Drowned Caverns";
    let mut spawn_id = CAVERNS_SPAWN_ID_START;
    // Aberrations: slimy and physical, weak to fire, half-resistant to frost.
    let aberration = DamageProfile::new(
        DamageType::Physical,
        Some(DamageType::Frost),
        Some(DamageType::Fire),
    );

    for cell in 0..w * h {
        if !floor[cell] {
            continue;
        }
        let id = CAVERNS_BASE + cell as u32;
        let (x, y) = (cell % w, cell / w);
        let is_entrance = cell == entrance_cell;
        let is_vault = cell == vault;

        let mut exits: HashMap<Dir, RoomId> = HashMap::new();
        let mut degree = 0;
        let connect = |nx: usize, ny: usize, dir: Dir, exits: &mut HashMap<Dir, RoomId>| {
            let ni = ny * w + nx;
            if floor[ni] {
                exits.insert(dir, CAVERNS_BASE + ni as u32);
                true
            } else {
                false
            }
        };
        if y > 0 && connect(x, y - 1, Dir::North, &mut exits) {
            degree += 1;
        }
        if x + 1 < w && connect(x + 1, y, Dir::East, &mut exits) {
            degree += 1;
        }
        if y + 1 < h && connect(x, y + 1, Dir::South, &mut exits) {
            degree += 1;
        }
        if x > 0 && connect(x - 1, y, Dir::West, &mut exits) {
            degree += 1;
        }

        let name: &'static str = if is_entrance {
            "Caverns - The Tide Mouth"
        } else if is_vault {
            "Caverns - The Tidal Abyss"
        } else {
            leak(format!(
                "Caverns - {}",
                SHAPE[(cell.wrapping_mul(7)) % SHAPE.len()]
            ))
        };
        let desc: &'static str = if is_entrance {
            "The Matlatesh cisterns drain through a fissure into a vast, breathing \
             dark. The lip of the cave is dry and safe; past it the stone is wet \
             and the passages wander where the water once did."
        } else if is_vault {
            leak(format!(
                "The galleries fall away into a drowned abyss, its surface black \
                 and unmoving as glass. {}. The cold here is the cold of deep water \
                 that has never seen the sun. Something vast waits beneath it, and \
                 the stillness is its held breath.",
                ATMOS[(cell.wrapping_mul(5)) % ATMOS.len()]
            ))
        } else {
            leak(format!(
                "You edge into {}. {}, and {}. {}. {}. The dark has had a long age to grow patient down here.",
                SHAPE[(cell.wrapping_mul(7)) % SHAPE.len()],
                ATMOS[(cell.wrapping_mul(3)) % ATMOS.len()],
                SOUND[(cell.wrapping_mul(11)) % SOUND.len()],
                DETAIL[(cell.wrapping_mul(13)) % DETAIL.len()],
                if degree >= 3 {
                    "Galleries open on every side"
                } else if degree <= 1 {
                    "The passage pinches shut"
                } else {
                    "The cave winds on"
                }
            ))
        };

        rooms.insert(
            id,
            Room {
                id,
                name,
                desc,
                zone,
                safe: is_entrance,
                pvp: false,
                exits,
            },
        );
        if is_entrance {
            continue;
        }

        let depth = dist[cell] as i32;
        let (mob_name, behavior, boss, hp, dmg) = if is_vault {
            ("the Abyss-Thing", MobBehavior::Brute, true, 380, 24)
        } else if degree <= 1 {
            if rng.chance(55) {
                (
                    "a Gloom Lurker",
                    MobBehavior::Ambusher,
                    false,
                    capped_depth_scale(100, 6, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                )
            } else {
                (
                    "a Cave Brute",
                    MobBehavior::Brute,
                    false,
                    capped_depth_scale(120, 6, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(13, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                )
            }
        } else if degree >= 3 {
            if rng.chance(55) {
                (
                    "a Brood-Tender",
                    MobBehavior::Summoner,
                    false,
                    capped_depth_scale(120, 6, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                )
            } else {
                (
                    "a Pack of Cave Stalkers",
                    MobBehavior::PackHunter,
                    false,
                    capped_depth_scale(115, 6, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(13, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                )
            }
        } else {
            match rng.below(4) {
                0 => (
                    "a Blind Crawler",
                    MobBehavior::Wanderer,
                    false,
                    capped_depth_scale(95, 5, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(11, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                ),
                1 => (
                    "a Deep Stalker",
                    MobBehavior::Hunter,
                    false,
                    capped_depth_scale(95, 5, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(12, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                ),
                2 => (
                    "a Brine Caller",
                    MobBehavior::Caster(DamageType::Frost),
                    false,
                    capped_depth_scale(90, 5, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                ),
                _ => (
                    "a Sparkmaw Eel",
                    MobBehavior::Caster(DamageType::Lightning),
                    false,
                    capped_depth_scale(90, 5, depth, CAVERNS_REGULAR_HP_CAP),
                    capped_depth_scale(10, 1, depth, CAVERNS_REGULAR_DAMAGE_CAP),
                ),
            }
        };
        if !is_vault && degree == 2 && rng.chance(35) {
            continue;
        }

        let profile = match behavior {
            MobBehavior::Caster(school) => {
                DamageProfile::new(school, Some(DamageType::Frost), Some(DamageType::Fire))
            }
            _ => aberration,
        };
        spawns.push(MobSpawn {
            id: spawn_id,
            name: mob_name,
            home: id,
            max_hp: hp,
            damage: dmg,
            xp: 32 + depth * 8 + if boss { 420 } else { 0 },
            respawn_secs: if boss { 600 } else { 75 },
            loot: if boss {
                CAVERNS_BOSS_LOOT
            } else {
                CAVERNS_COMMON_LOOT
            },
            boss,
            profile,
        });
        behaviors.insert(spawn_id, behavior);
        spawn_id += 1;
    }

    let entrance = CAVERNS_BASE + entrance_cell as u32;
    let portal = [Dir::Down, Dir::East, Dir::West, Dir::North]
        .into_iter()
        .find(|d| {
            rooms
                .get(&MATLATESH_SQUARE)
                .is_some_and(|r| !r.exits.contains_key(d))
        })
        .unwrap_or(Dir::Down);
    if let Some(sq) = rooms.get_mut(&MATLATESH_SQUARE) {
        sq.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), MATLATESH_SQUARE);
    }
}

fn scale_i32(value: i32, numerator: i32, denominator: i32) -> i32 {
    (((value as i64) * (numerator as i64) + (denominator as i64 - 1)) / denominator as i64).max(1)
        as i32
}

fn scale_u64(value: u64, numerator: u64, denominator: u64) -> u64 {
    (value * numerator).div_ceil(denominator).max(1)
}

fn is_living_dark_spawn(id: u32) -> bool {
    (CATACOMBS_SPAWN_ID_START..CATACOMBS_SPAWN_ID_START + 10_000).contains(&id)
        || (THORNWOOD_SPAWN_ID_START..THORNWOOD_SPAWN_ID_START + 10_000).contains(&id)
        || (CAVERNS_SPAWN_ID_START..CAVERNS_SPAWN_ID_START + 10_000).contains(&id)
}

/// A regular bites at about this share of its land's crown, a zone boss at
/// about this share (see `MobSpawn::level`).
const TRASH_BITE_PCT: i32 = 70;
const BOSS_BITE_PCT: i32 = 85;

/// The level of the prepared character whose crown hits for `bite`, read off
/// the `CROWNS` ladder: linear between neighbouring crowns, extrapolated
/// past either end, clamped to the level range.
fn level_for_bite(bite: i32) -> i32 {
    let first = CROWNS[0];
    if bite <= first.damage {
        return (first.level * bite / first.damage).clamp(1, first.level);
    }
    for w in CROWNS.windows(2) {
        let (a, b) = (w[0], w[1]);
        if bite <= b.damage {
            if b.damage == a.damage {
                return b.level;
            }
            return a.level + (b.level - a.level) * (bite - a.damage) / (b.damage - a.damage);
        }
    }
    let (a, b) = (CROWNS[CROWNS.len() - 2], CROWNS[CROWNS.len() - 1]);
    let past = (bite - b.damage) * (b.level - a.level) / (b.damage - a.damage).max(1);
    (b.level + past).clamp(1, super::classes::Class::MAX_LEVEL)
}

/// One of the fourteen bosses on the road players actually walk, with the
/// numbers it is fielded at. See [`CROWNS`].
#[derive(Clone, Copy, Debug)]
pub struct Crown {
    pub name: &'static str,
    /// The level a prepared character takes it at; also its displayed level.
    pub level: i32,
    pub max_hp: i32,
    pub damage: i32,
}

/// Ticks a median prepared character needs to kill a crown.
pub const CROWN_KILL_TICKS: i32 = 14;
/// Ticks a crown needs to kill that character with no draught drunk. Shorter
/// than the kill, so every crown is a race the prepared character wins on
/// potions, self-heals, wards and the companion, and an unprepared one loses.
pub const CROWN_SURVIVE_TICKS: i32 = 11;

/// The crowns: the authored core's seven bosses, the three living-dark seals,
/// the Frontier King, Yssgar, and the two Kaethyrs. Applied over
/// `tune_spawn_balance` by `tune_crowns`, so nothing upstream (authored
/// literals, land multipliers) decides what a crown is.
///
/// **Derived, not authored by feel.** Each row is `(name, level, max_hp,
/// damage)` where `level` is the target the crown is tuned to fall at: a
/// *prepared* character of that level (the tier's kit, the oil the crown is
/// weak to, three draughts, and from the Reaches on a maxed companion).
/// `max_hp` is the median prepared dps at that kit times `CROWN_KILL_TICKS`;
/// `damage` is the median prepared health pool over `CROWN_SURVIVE_TICKS`
/// plus what that kit's armor blunts (half of it for a Physical striker, a
/// quarter otherwise). The inputs are printed by the arena's `arena_crown_yardstick`
/// and the outcome is pinned by
/// `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in`
/// (`arena_test.rs`): every calling wins prepared, the median kill is a real
/// fight, and a walk-in a few levels lower in the previous tier loses.
/// Re-derive a row when the player curve moves; the contract says when.
///
/// The story this encodes: the grind to 100 is long by design, so the last
/// crown falls to a prepared L80 and 80-100 is prestige; the first crown is
/// a real fight at L12 with the right prep (the Treant teaches the oil).
pub const CROWNS: &[Crown] = &[
    Crown {
        name: "the Elder Treant",
        level: 12,
        max_hp: 1160,
        damage: 19,
    },
    Crown {
        name: "the Bone Tyrant",
        level: 16,
        max_hp: 1806,
        damage: 28,
    },
    Crown {
        name: "the Lich Vael",
        level: 20,
        max_hp: 2100,
        damage: 32,
    },
    Crown {
        name: "the Magma Colossus",
        level: 24,
        max_hp: 2324,
        damage: 35,
    },
    Crown {
        name: "the Wyrm of Frostspire",
        level: 27,
        max_hp: 2982,
        damage: 45,
    },
    Crown {
        name: "the Fallen Paladin",
        level: 30,
        max_hp: 3248,
        damage: 48,
    },
    Crown {
        name: "the Archdemon Mal'gareth",
        level: 35,
        max_hp: 4088,
        damage: 62,
    },
    Crown {
        name: "The Bonewright Lich",
        level: 40,
        max_hp: 4508,
        damage: 75,
    },
    Crown {
        name: "the Elder Dryad",
        level: 40,
        max_hp: 4508,
        damage: 75,
    },
    Crown {
        name: "the Abyss-Thing",
        level: 40,
        max_hp: 4508,
        damage: 75,
    },
    Crown {
        name: "the King Who Was Promised Nothing",
        level: 55,
        max_hp: 9926,
        damage: 138,
    },
    Crown {
        name: "Yssgar, the Sundering Deep",
        level: 65,
        max_hp: 17528,
        damage: 253,
    },
    Crown {
        name: "Kaethyr the Unquenched, Ashen King of Kaelmyr",
        level: 75,
        max_hp: 22722,
        damage: 368,
    },
    Crown {
        name: "Kaethyr Ascendant, Who Sang the God Awake",
        level: 80,
        max_hp: 24542,
        damage: 397,
    },
];

/// The level a named crown is tuned to fall at. A name that is not a crown is
/// a programming error, not a runtime case.
pub fn crown_level(name: &str) -> i32 {
    CROWNS
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} is not a crown"))
        .level
}

/// Field every crown at its `CROWNS` numbers. Panics if a crown's spawn is
/// missing: a renamed boss must be renamed here too, loudly.
fn tune_crowns(spawns: &mut [MobSpawn]) {
    for crown in CROWNS {
        let spawn = match spawns.iter_mut().find(|s| s.name == crown.name) {
            Some(s) => s,
            None => panic!("crown {:?} has no spawn", crown.name),
        };
        spawn.max_hp = crown.max_hp;
        spawn.damage = crown.damage;
    }
}

/// The tuning band a spawn belongs to, by id range (the per-land
/// `*_SPAWN_ID_START` consts). Everything not named is the gentle overworld
/// bucket: the authored core, the Sunderlakes, Broceliande, Aelunor, and the
/// Wildbound Waste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Band {
    Overworld,
    LivingDark,
    Frontier,
    Reaches,
    Kaelmyr,
    Archipelago,
}

fn band_of(id: u32) -> Band {
    if is_living_dark_spawn(id) {
        Band::LivingDark
    } else if (FRONTIER_SPAWN_ID_START..REACHES_SPAWN_ID_START).contains(&id) {
        Band::Frontier
    } else if (REACHES_SPAWN_ID_START..KAELMYR_SPAWN_ID_START).contains(&id) {
        Band::Reaches
    } else if (KAELMYR_SPAWN_ID_START..ARCH_SPAWN_ID_START).contains(&id) {
        Band::Kaelmyr
    } else if (ARCH_SPAWN_ID_START..LAKES_SPAWN_ID_START).contains(&id) {
        Band::Archipelago
    } else {
        Band::Overworld
    }
}

/// Scale every authored spawn into the band its land plays at. One row per
/// (band, boss-or-regular); a land that reads out of band against its crown
/// (`the_trash_on_a_crowns_doorstep_is_in_band`, `arena_test.rs`) is fixed
/// here, in its row, never mob by mob. The three crowned endgame lands are
/// calibrated at their deepest zone against the crown that stands there
/// (a regular dies in ~3 prepared ticks and needs 15+ to kill you, casters
/// included since armor blunts a school only by a quarter; a zone boss ~8
/// and ~14), so what their generators author is what is fielded;
/// the Frontier's generator was re-sloped for that and its row is 1:1. The
/// Archipelago keeps the old endgame multipliers on purpose: it is ungated,
/// portal-reachable, and deadly by design. Crowns are re-fielded afterwards
/// by `tune_crowns`, so nothing here decides what a crown is.
fn tune_spawn_balance(spawns: &mut [MobSpawn]) {
    for spawn in spawns {
        let band = band_of(spawn.id);
        let (hp_num, hp_den, dmg_num, dmg_den, xp_num, xp_den) = match (band, spawn.boss) {
            (Band::Overworld, true) => (3, 2, 5, 4, 4, 5),
            (Band::Overworld, false) => (6, 5, 6, 5, 9, 8),
            (Band::LivingDark, true) => (6, 1, 7, 2, 2, 1),
            (Band::LivingDark, false) => (13, 4, 5, 2, 3, 2),
            (Band::Frontier, true) => (1, 1, 1, 1, 4, 3),
            (Band::Frontier, false) => (1, 1, 1, 1, 3, 2),
            (Band::Reaches, true) => (7, 6, 7, 8, 4, 3),
            (Band::Reaches, false) => (4, 3, 4, 5, 3, 2),
            (Band::Kaelmyr, true) => (5, 6, 4, 5, 4, 3),
            (Band::Kaelmyr, false) => (4, 5, 2, 3, 3, 2),
            (Band::Archipelago, true) => (12, 5, 21, 10, 4, 3),
            (Band::Archipelago, false) => (2, 1, 19, 10, 3, 2),
        };
        spawn.max_hp = scale_i32(spawn.max_hp, hp_num, hp_den);
        spawn.damage = scale_i32(spawn.damage, dmg_num, dmg_den);
        spawn.xp = scale_i32(spawn.xp, xp_num, xp_den);
        if !spawn.boss {
            let endgame = matches!(
                band,
                Band::Frontier | Band::Reaches | Band::Kaelmyr | Band::Archipelago
            );
            spawn.respawn_secs = if endgame {
                scale_u64(spawn.respawn_secs, 3, 4).max(60)
            } else {
                scale_u64(spawn.respawn_secs, 4, 5).max(25)
            };
        }
    }
}

// ---- The Frontier (procedural expansion) --------------------------------
//
// Twenty themed zones, each a 10x5 grid of 50 rooms, chained one below the next and
// hung off Embergate's square. Rooms, names and descriptions are composed
// deterministically from per-zone flavour and leaked to 'static (the world is
// built once at startup). Each zone fields three regular mob types and a boss;
// loot is the generated frontier catalog for that tier.

const FRONTIER_BASE: RoomId = 2000;
const FRONTIER_W: u32 = 10;
const FRONTIER_H: u32 = 5;
const FRONTIER_ZONES: usize = FRONTIER_ZONES_DATA.len();
const FRONTIER_SPAWN_ID_START: u32 = 900_000;

/// The first Frontier room, reached from Embergate's square through the old
/// gateway stair.
pub fn frontier_entrance_room() -> RoomId {
    FRONTIER_BASE
}

/// The safe entrance cell of Frontier zone `z` (0-based), for tracking a zone
/// quest on the world map.
pub fn frontier_zone_entrance(z: usize) -> RoomId {
    FRONTIER_BASE + (z as u32) * FRONTIER_W * FRONTIER_H
}

pub fn is_frontier_room(id: RoomId) -> bool {
    (FRONTIER_BASE..FRONTIER_BASE + FRONTIER_ZONES as u32 * FRONTIER_W * FRONTIER_H).contains(&id)
}

// ---- City districts: flesh out the four capitals (rooms 3000+) ------------
//
// Each capital gains a short district of safe, flavourful rooms hung off its
// square via a free direction, so the cities feel like places to linger rather
// than waypoints. Rooms are authored from a per-city theme; ids start at 3000
// (free, between the Frontier band and the living-world mazes).
fn extend_cities(rooms: &mut HashMap<RoomId, Room>) {
    // (square, city name, district label, portal, street, [4 (room-name,
    // room-desc) pairs]). `portal` is the free direction the district opens off
    // the square; `street` is the step into each haunt in turn, so a district
    // can turn a corner or drop a stair instead of running one straight line.
    // Both are authored per city rather than derived: which way a district
    // faces decides where it lands on the world map, and a district that walks
    // back over its own capital's road is a fold (see `worldmap`'s
    // `zone_interleaves`). Each description is at least two sentences and a
    // paragraph long, to satisfy the world invariants. Ids start at 3000 (free,
    // between Frontier and mazes).
    #[allow(clippy::type_complexity)]
    const CITIES: [(RoomId, &str, &str, Dir, [Dir; 4], [(&str, &str); 4]); 4] = [
        (
            1,
            "Embergate",
            "the Lamplit Quarter",
            // Up onto the terraced quarter above the square, then east along it.
            Dir::Up,
            [Dir::East, Dir::East, Dir::East, Dir::East],
            [
                (
                    "the Lamplit Baths",
                    "Vaulted bath-houses breathe steam into the lamplight, and off-duty guards and road-worn travellers soak the miles from their bones in tiled pools. An attendant moves among them hawking hot towels and colder gossip, and for a copper you may join them and hear the whole city's business.",
                ),
                (
                    "the Adventurers' Guildhall",
                    "A long timbered hall hangs with battered shields and the pennants of a hundred dead and living companies, its walls papered with maps and notices of the missing. The ale is bad on purpose so no one lingers past their business, yet somehow the benches are always full of half-told stories.",
                ),
                (
                    "Tinker's Row",
                    "A crooked lane of workshops where smiths, gluers, and gear-cutters ply their trades cheek by jowl. The air is bright with sparks and loud with the ring of small hammers and smaller arguments, and a careful eye can find a clever thing here that no shop would ever stock.",
                ),
                (
                    "the Shrine Garden",
                    "Behind the temple a walled garden keeps its quiet, its pale gravel raked into slow rings around a single old plum tree. Here the grieving and the grateful sit alike on stone benches beneath the Dawn's open sky, and even the noise of the square seems to lower its voice at the gate.",
                ),
            ],
        ),
        (
            TASMANIA_SQUARE,
            "Tasmania",
            "the Saltwind Wharves",
            // West out of the square, then down the harbour stair and north
            // along the water, away from the Greatroad and Embergate.
            Dir::West,
            [Dir::Down, Dir::North, Dir::North, Dir::North],
            [
                (
                    "the Fishmarket",
                    "Trestle stalls glitter with the morning's catch laid out on crushed ice, and fishwives cry their prices over the wheeling gulls. Beneath the boards the harbour cats conduct their own grey commerce, and the whole quarter smells of brine, smoke, and money changing hands.",
                ),
                (
                    "the Cartographers' Loft",
                    "Up a salt-bleached stair waits a loft of long tables where chart-makers ink the coasts in patient, hair-fine lines. The smell is of vellum and pitch and cold tea, and every wall holds a painted sea you have not yet sailed and perhaps were never meant to.",
                ),
                (
                    "the Harbourmaster's Office",
                    "A brass-and-mahogany office smelling of tar and ledgers stands with its windows full of swaying masts. The harbourmaster knows every hull in the bay and the debts of every captain besides, and very little crosses this water that she has not already written down.",
                ),
                (
                    "the Storm-Chapel",
                    "A squat chapel of black sea-rock crouches at the wharf's end, where sailors light candles before a voyage and leave them burning long after. Its altar lies heaped with the small offerings of those who go down to the sea, and the wind through its door sounds remarkably like a hymn.",
                ),
            ],
        ),
        (
            MELVANALA_SQUARE,
            "Melvanala",
            "the Hightarn Terraces",
            // Up onto the terraces cut above the lakeshore, then east.
            Dir::Up,
            [Dir::East, Dir::East, Dir::East, Dir::East],
            [
                (
                    "the Mirrorlake Walk",
                    "A balustraded walk runs along the lakeshore where the water lies so still it doubles the snow-peaks upon its face. At dusk the lamplighters move along it in slow procession, and the whole terrace seems to hang suspended between two identical skies.",
                ),
                (
                    "the Stonecutters' Court",
                    "A court stands ringed with the workshops of masons and lapidaries, its ground gone pale with a permanent dust of stone. Here and there it glints where some careless apprentice spilled a pocket of uncut gems, and the patient tap of chisels never altogether stops.",
                ),
                (
                    "the Alewife's Longhall",
                    "A warm, low longhall sits thick with peat-smoke and the rise and fall of song, its rafters black with the winters of its hearth. The famous highland brew is poured here by the yard, and strangers who come in cold leave as friends, as kin, or not at all.",
                ),
                (
                    "the Snowmelt Spring",
                    "A carved grotto receives the mountain's coldest, clearest water into a worn stone basin fed from somewhere far above. Pilgrims kneel to drink and rise gasping at the chill, and they will swear to you afterward that it carried off whatever ailed them.",
                ),
            ],
        ),
        (
            MATLATESH_SQUARE,
            "Matlatesh",
            "the Sunbaked Bazaar",
            // North off the square and on north into the dunes, clear of the
            // Greatroad running east from Matlatesh's gate.
            Dir::North,
            [Dir::North, Dir::North, Dir::North, Dir::North],
            [
                (
                    "the Spice Bazaar",
                    "A canvas-shaded maze of stalls lies heaped with saffron, dried citron, and peppers that seem to colour the very air you breathe. The haggling here never altogether stops, and a glass of sweet mint tea is always pressed upon you before any honest price is named.",
                ),
                (
                    "the Glassblowers' Souk",
                    "A souk of roaring furnaces opens off the lane, where glassblowers spin molten gobs into lamps, beads, and impossible birds. The heat stands like a wall at its mouth, and the finished wares catch the desert light along the shelves like rows of trapped and patient fire.",
                ),
                (
                    "the Caravanserai",
                    "A great mud-brick courtyard receives the caravans, where weary beasts and wearier drivers rest beneath the arcades. The air is loud with camels and a dozen tongues at once, and every traveller here carries a rumour, a contract, or a knife from somewhere even drier.",
                ),
                (
                    "the Oasis Conservatory",
                    "A high-walled garden the desert is forbidden to enter keeps its date palms, its tiled pool, and its astonishing birdsong. It is kept green at ruinous expense as a standing boast against the dunes, and to sit in its shade is the closest thing to wealth a poor traveller may borrow.",
                ),
            ],
        ),
    ];

    for (c, &(square, city, district, portal, street, district_rooms)) in CITIES.iter().enumerate()
    {
        let base = 3000 + (c as RoomId) * 10;
        let back_to_square = portal.opposite();
        // The district is a walkable street: the spine faces the square and the
        // several haunts chain on from it, so you can stroll through them rather
        // than dead-ending back at the spine from each.
        assert!(
            rooms
                .get(&square)
                .is_some_and(|r| !r.exits.contains_key(&portal)),
            "{district}'s portal {portal:?} is already taken on room {square}"
        );
        // The exits maps below are built wholesale, not through `link`, so a
        // street step doubling back on the previous one would silently
        // overwrite the back-link and make the street one-way. Refuse the
        // authoring instead.
        assert!(
            street[0] != back_to_square,
            "{district}'s first street step walks straight back into the square"
        );
        for k in 0..street.len() - 1 {
            assert!(
                street[k + 1] != street[k].opposite(),
                "{district}'s street step {} doubles back on step {}",
                k + 1,
                k
            );
        }
        let zone: &'static str = district;
        let spine = base;
        rooms.insert(
            spine,
            Room {
                id: spine,
                name: zone,
                zone,
                safe: true,
                pvp: false,
                desc: Box::leak(
                    format!(
                        "{district} opens off the {city} square, the livelier heart of the city where folk gather to trade, to drink, to worship, and to waste an idle hour. Its several haunts line the street that runs on from here, and the ordinary noise of living fills the air from dawn until well past dark."
                    )
                    .into_boxed_str(),
                ),
                exits: [(back_to_square, square), (street[0], base + 1)]
                    .into_iter()
                    .collect(),
            },
        );
        if let Some(sq) = rooms.get_mut(&square) {
            sq.exits.insert(portal, spine);
        }
        // Chain the haunts in a line: each links back down the street (to the spine
        // or the previous haunt) and, unless it is the last, on to the next.
        let n = district_rooms.len();
        for (k, (rname, rdesc)) in district_rooms.iter().enumerate() {
            let id = base + 1 + k as RoomId;
            let prev = if k == 0 { spine } else { base + k as RoomId };
            let mut exits: Vec<(Dir, RoomId)> = vec![(street[k].opposite(), prev)];
            if k + 1 < n {
                exits.push((street[k + 1], base + 2 + k as RoomId));
            }
            rooms.insert(
                id,
                Room {
                    id,
                    name: rname,
                    zone,
                    safe: true,
                    pvp: false,
                    desc: rdesc,
                    exits: exits.into_iter().collect(),
                },
            );
        }
    }
}

// ---- The Sundered Reaches: a second 1000-room continent (rooms 10000+) -----
//
// A drowned, storm-wracked sea-realm of sinking isles, sunken cities, and the
// abyss below - the same proven 20-zone × 10×5-grid generator as the Frontier,
// with its own themed zones, a named boss per zone, and tier-scaled loot. Hung
// off the Matlatesh desert capital via a sea-gate. Generation is data-driven and
// deterministic, so the strict world invariants stay green.
const REACHES_BASE: RoomId = 10_000;
const REACHES_W: usize = 10;
const REACHES_H: usize = 5;
const REACHES_ZONES: usize = REACHES_ZONES_DATA.len();
const REACHES_SPAWN_ID_START: u32 = 950_000;
const REACHES_SEED: u64 = 0x5EA_D4EAD_u64;
/// Each zone reserves this many room ids (a `REACHES_W`×`REACHES_H` cell field).
const REACHES_ZONE_STRIDE: u32 = (REACHES_W * REACHES_H) as u32;

/// Which Reaches zones are carved as organic caverns rather than braided mazes -
/// the deep, drowned, cave-like ones. The rest are mazes.
const fn reaches_zone_is_cavern(z: usize) -> bool {
    matches!(z, 7 | 9 | 13 | 15 | 17 | 19)
}

pub fn is_reaches_room(id: RoomId) -> bool {
    (REACHES_BASE..REACHES_BASE + REACHES_ZONES as u32 * REACHES_ZONE_STRIDE).contains(&id)
}

/// The world resist/weak pass (spec: CONTEXT.md, same-named section): one theme per
/// Reaches zone, in `REACHES_ZONES_DATA` order. Regulars inherit the theme's
/// profile; the zone boss wears the theme's weakness but never its resist
/// (prep is a pure reward on the fight players provision for).
const REACHES_ZONE_THEMES: [ZoneTheme; REACHES_ZONES] = [
    ZoneTheme::Tidal,     // Saltmarsh Shallows
    ZoneTheme::Tidal,     // Wreckers' Coast
    ZoneTheme::Resonant,  // Weeping Cliffs
    ZoneTheme::Verdant,   // Kelpwood Drowned
    ZoneTheme::Fae,       // Sirens' Reef
    ZoneTheme::Haunted,   // Sinking Isles
    ZoneTheme::Storm,     // Stormwall Straits
    ZoneTheme::Drowned,   // Brine Caverns
    ZoneTheme::Haunted,   // Sunken Valmaris
    ZoneTheme::Drowned,   // Pearl Abyss
    ZoneTheme::Beastwild, // Coral Throne Reach
    ZoneTheme::Crystal,   // Glass Currents
    ZoneTheme::Beastwild, // Leviathan's Wake
    ZoneTheme::Undead,    // Mourning Depths
    ZoneTheme::Storm,     // Tempest Spire Reach
    ZoneTheme::Beastwild, // Trench of Maws
    ZoneTheme::Profane,   // Drowned Pantheon
    ZoneTheme::Resonant,  // Black Maelstrom
    ZoneTheme::Fae,       // Abyssal Court
    ZoneTheme::Profane,   // Sundering Deep
];

/// Twenty zones of the Sundered Reaches: (zone, adjective, ground, landmark,
/// creatures, three mob names, boss). Reuses `frontier_desc` for prose.
const REACHES_ZONES_DATA: [ZoneData; 20] = [
    (
        "Saltmarsh Shallows",
        "brackish",
        "sucking tidal mud",
        "a half-sunk fishing shrine",
        "marsh-lurkers",
        [
            "a bog-drowned thrall",
            "a reed-stalker",
            "a brine-bloated hound",
        ],
        "Old Maw the Tidejaw",
    ),
    (
        "Wreckers' Coast",
        "wind-scoured",
        "shingle and broken spar",
        "the ribs of a shattered galleon",
        "wreck-ghouls",
        [
            "a drowned wrecker",
            "a barnacled brute",
            "a gull-eyed scavenger",
        ],
        "Captain Sull the Unsunk",
    ),
    (
        "Weeping Cliffs",
        "rain-lashed",
        "slick black basalt",
        "a weather-worn lighthouse",
        "cliff-harpies",
        ["a storm-harpy", "a cliff-clinger", "a salt-mad hermit"],
        "Maelys of the Hundred Falls",
    ),
    (
        "Kelpwood Drowned",
        "green-gloomed",
        "rotting kelp",
        "a forest of petrified masts",
        "kelp-stranglers",
        ["a kelp-strangler", "a drowned dryad", "a tide-wight"],
        "The Verdant Drowned King",
    ),
    (
        "Sirens' Reef",
        "coral-jagged",
        "razor coral",
        "a reef of singing bones",
        "siren-kin",
        [
            "a luring siren",
            "a reef-shark thrall",
            "a pearl-eyed drowner",
        ],
        "Nauthis the Reefsinger",
    ),
    (
        "Sinking Isles",
        "fog-bound",
        "subsiding sand",
        "a town swallowed to its rooftops",
        "isle-revenants",
        [
            "a sinking-isle ghoul",
            "a fog-walker",
            "a drowned bellringer",
        ],
        "The Warden of Nine Sunk Bells",
    ),
    (
        "Stormwall Straits",
        "thunder-haunted",
        "wave-swept rock",
        "a broken sea-fort",
        "storm-thralls",
        [
            "a stormbound corsair",
            "a lightning-scarred brute",
            "a gale-wraith",
        ],
        "Vexhal, Voice of the Storm",
    ),
    (
        "Brine Caverns",
        "lightless",
        "tide-cut limestone",
        "a cavern of dripping stalactites",
        "cave-anglers",
        ["a blind cave-angler", "a brine-crawler", "a pallid drowner"],
        "The Lanternless Hunger",
    ),
    (
        "Sunken Valmaris",
        "moss-drowned",
        "silted marble",
        "the flooded plaza of a dead city",
        "city-drowned",
        [
            "a Valmaran revenant",
            "a coral-grown sentinel",
            "a drowned magister",
        ],
        "Empress Calyx, Still Crowned",
    ),
    (
        "Pearl Abyss",
        "black-fathomed",
        "abyssal silt",
        "a trench of bioluminal bloom",
        "abyss-things",
        [
            "an abyssal feeler",
            "a glow-lure horror",
            "a pressure-wraith",
        ],
        "That Which Pearls the Dark",
    ),
    (
        "Coral Throne Reach",
        "blood-coral",
        "calcified bone",
        "a throne grown of living reef",
        "throne-guard",
        ["a coral knight", "a reef-bound zealot", "a polyp-swarm"],
        "The Coral Tyrant",
    ),
    (
        "Glass Currents",
        "glassy",
        "obsidian shard-sand",
        "a river of slow black glass",
        "glass-stalkers",
        [
            "a glass-skinned hunter",
            "a shard-revenant",
            "a mirror-drowner",
        ],
        "Sieth of the Cutting Tide",
    ),
    (
        "Leviathan's Wake",
        "oil-dark",
        "whale-bone scree",
        "the spine of a beached leviathan",
        "wake-feeders",
        ["a leviathan parasite", "a bone-picker", "a gut-crawler"],
        "The Wake-Thing",
    ),
    (
        "Mourning Depths",
        "ash-grey",
        "drowned grave-silt",
        "a fathom-deep field of cairns",
        "depth-mourners",
        [
            "a mourning revenant",
            "a grave-tide wraith",
            "a sorrow-drowned",
        ],
        "The Keeper of Drowned Years",
    ),
    (
        "Tempest Spire Reach",
        "storm-crowned",
        "wind-bared stone",
        "a spire that splits the lightning",
        "spire-stalkers",
        [
            "a tempest acolyte",
            "a thunder-thrall",
            "a stormcalled wraith",
        ],
        "Aurex, the Spire's Wrath",
    ),
    (
        "Trench of Maws",
        "abyssal",
        "trench-dark muck",
        "a chasm lined with teeth",
        "trench-maws",
        ["a trench-maw spawn", "a gulper horror", "a swallowing dark"],
        "The All-Devouring Trench",
    ),
    (
        "Drowned Pantheon",
        "god-haunted",
        "temple silt",
        "the toppled idols of drowned gods",
        "godless-drowned",
        [
            "a fallen god's herald",
            "a temple-drowned zealot",
            "an idol-wraith",
        ],
        "The Last Drowned God",
    ),
    (
        "Black Maelstrom",
        "vortex-torn",
        "spinning wrack",
        "the eye of an endless whirlpool",
        "maelstrom-born",
        [
            "a maelstrom revenant",
            "a churning horror",
            "a vortex-wraith",
        ],
        "The Maelstrom's Heart",
    ),
    (
        "Abyssal Court",
        "crushing-dark",
        "court-silt of the deep",
        "a sunken court of cold thrones",
        "court-drowned",
        [
            "an abyssal courtier",
            "a deep-bound knight",
            "a fathom-lord's guard",
        ],
        "The Fathom Lord",
    ),
    (
        "Sundering Deep",
        "world-ending dark",
        "the floor of all seas",
        "the wound where the world drinks",
        "the unsounded",
        [
            "a herald of the deep",
            "an unsounded terror",
            "a drowner-of-worlds",
        ],
        "Yssgar, the Sundering Deep",
    ),
];

#[allow(clippy::needless_range_loop, clippy::type_complexity)]
fn extend_reaches(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (REACHES_W, REACHES_H);
    let n = w * h;
    let mut spawn_id: u32 = REACHES_SPAWN_ID_START;
    // The deepest (boss) room of the previous zone, to chain the realm together.
    let mut prev_exit: Option<RoomId> = None;

    for (z, &(zname, adj, ground, feature, creature, mob_names, boss)) in
        REACHES_ZONES_DATA.iter().enumerate()
    {
        let zbase = REACHES_BASE + (z as u32) * REACHES_ZONE_STRIDE;
        let tier = (z + 12) as i32; // the Reaches sit beyond even the Frontier's tiers
        let mut rng = MazeRng::new(REACHES_SEED ^ (z as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

        // Carve the zone as either a braided maze or an organic cavern, and reduce
        // both to a common form: which cells are real rooms, their distance from
        // the entrance, and their open exits. No uniform grids here. A cavern that
        // comes out too sparse on its seed falls back to a maze so no zone is empty.
        let cavern_floor = if reaches_zone_is_cavern(z) {
            let floor = carve_cavern(w, h, &mut rng);
            (floor.iter().filter(|f| **f).count() >= 20).then_some(floor)
        } else {
            None
        };
        let (entrance, reachable, dist, cell_exits): (
            usize,
            Vec<bool>,
            Vec<usize>,
            Vec<Vec<(Dir, usize)>>,
        ) = if let Some(floor) = cavern_floor {
            let entrance = (0..n).find(|&i| floor[i]).unwrap_or(0);
            let dist = cavern_distances(&floor, w, h, entrance);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    let (x, y) = (c % w, c / w);
                    let consider = |nx: i64, ny: i64, d: Dir, v: &mut Vec<(Dir, usize)>| {
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let nb = ny as usize * w + nx as usize;
                            if reachable[nb] {
                                v.push((d, nb));
                            }
                        }
                    };
                    consider(x as i64, y as i64 - 1, Dir::North, &mut v);
                    consider(x as i64 + 1, y as i64, Dir::East, &mut v);
                    consider(x as i64, y as i64 + 1, Dir::South, &mut v);
                    consider(x as i64 - 1, y as i64, Dir::West, &mut v);
                    v
                })
                .collect();
            (entrance, reachable, dist, exits)
        } else {
            let open = carve_maze(w, h, &mut rng);
            let dist = maze_distances(&open, w, h, 0);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    for d in 0..4 {
                        if open[c][d]
                            && let Some(nb) = maze_neighbor(c, d, w, h)
                        {
                            v.push((DIRS[d], nb));
                        }
                    }
                    v
                })
                .collect();
            (0, reachable, dist, exits)
        };

        // The zone boss waits in the cell farthest from the entrance.
        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let zone: &'static str = Box::leak(format!("The {zname}").into_boxed_str());

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = zbase + cell as u32;
            let is_entrance = cell == entrance;
            let is_boss = cell == deepest && cell != entrance;
            let degree = cell_exits[cell].len();

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, zbase + *nb as u32))
                .collect();

            let name: &'static str = if is_entrance {
                Box::leak(format!("{zname} - the Tidewatch").into_boxed_str())
            } else if is_boss {
                Box::leak(format!("{zname} - the Drowned Heart").into_boxed_str())
            } else {
                Box::leak(format!("{zname} - {}", FRONTIER_PLACES[cell % 10]).into_boxed_str())
            };
            let desc: &'static str = Box::leak(
                frontier_desc(adj, ground, feature, creature, cell as u32).into_boxed_str(),
            );

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone,
                    safe: is_entrance && z == 0, // only the realm's sea-gate is safe
                    pvp: false,
                    exits,
                },
            );

            if is_entrance {
                continue;
            }

            // Behaviour-driven foes by the room's role: dead-ends ambush, junctions
            // swarm, corridors patrol or cast; the deepest cell holds the boss.
            let depth = dist[cell] as i32;
            let storm = z >= 6; // the deeper Reaches crackle with the storm
            let (mob_name, behavior, boss_mob, hp, dmg) = if is_boss {
                (
                    boss,
                    MobBehavior::Brute,
                    true,
                    1400 + tier * 230,
                    64 + tier * 6,
                )
            } else if degree == 1 {
                (
                    mob_names[0],
                    MobBehavior::Ambusher,
                    false,
                    820 + tier * 60 + depth * 6,
                    56 + tier * 4 + depth,
                )
            } else if degree >= 3 {
                (
                    mob_names[1],
                    if rng.chance(50) {
                        MobBehavior::PackHunter
                    } else {
                        MobBehavior::Summoner
                    },
                    false,
                    900 + tier * 70 + depth * 6,
                    58 + tier * 5 + depth,
                )
            } else {
                // Leave some corridors quiet so the realm breathes.
                if rng.chance(35) {
                    continue;
                }
                let behavior = match rng.below(4) {
                    0 => MobBehavior::Wanderer,
                    1 => MobBehavior::Patroller,
                    2 => MobBehavior::Hunter,
                    _ => MobBehavior::Caster(if storm {
                        DamageType::Lightning
                    } else {
                        DamageType::Frost
                    }),
                };
                (
                    mob_names[2],
                    behavior,
                    false,
                    820 + tier * 60 + depth * 6,
                    56 + tier * 4 + depth,
                )
            };
            let attack = match behavior {
                MobBehavior::Caster(school) => school,
                _ => DamageType::Physical,
            };
            // The boss wears the zone's weakness but never its resist:
            // prep is a pure reward on the fight players provision for.
            let theme = REACHES_ZONE_THEMES[z];
            let profile = if boss_mob {
                DamageProfile::new(attack, None, theme.weak())
            } else {
                DamageProfile::new(attack, theme.resist(), theme.weak())
            };
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: hp,
                damage: dmg,
                // XP hands off from the late Frontier and climbs past it: entry
                // bosses trail the King a little, Yssgar clears him by half again.
                xp: if boss_mob {
                    720 + tier * 90
                } else {
                    200 + tier * 40 + depth * 5
                },
                respawn_secs: if boss_mob { 600 } else { 90 },
                loot: super::items::reaches_loot(z),
                boss: boss_mob,
                profile,
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }

        // Chain this zone to the previous one: the prior boss room descends to
        // this zone's sea-gate, and back up again.
        let entrance_id = zbase + entrance as u32;
        if let Some(prev) = prev_exit {
            if let Some(r) = rooms.get_mut(&prev) {
                r.exits.insert(Dir::Down, entrance_id);
            }
            if let Some(r) = rooms.get_mut(&entrance_id) {
                r.exits.insert(Dir::Up, prev);
            }
        }
        prev_exit = Some(zbase + deepest as u32);
    }

    // Hang the sea-gate off the Matlatesh desert capital so the whole realm is
    // reachable; the first zone's entrance is the only safe waystation.
    let entrance = REACHES_BASE;
    let portal = [Dir::Down, Dir::Up, Dir::West]
        .into_iter()
        .find(|d| {
            rooms
                .get(&MATLATESH_SQUARE)
                .is_some_and(|r| !r.exits.contains_key(d))
        })
        .unwrap_or(Dir::Down);
    if let Some(hub) = rooms.get_mut(&MATLATESH_SQUARE) {
        hub.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), MATLATESH_SQUARE);
    }
}

// ==== Kaelmyr, the Ashen Reach ============================================
//
// A THIRD continent (rooms 12000+), hung off the deepest room of the Sundered
// Reaches - Yssgar's drowned chamber - and gated behind the Bane of Yssgar
// title, the deepest end-game crown in the game. Where the Reaches are a
// drowned sea-realm, Kaelmyr is its opposite: a burnt, ash-choked landmass
// torn loose from the seabed when Yssgar was slain and the seas drained into
// the wound he left. The surfacing land baked sunless under an ash-sky, older
// than Lateania itself, and five peoples cling to its cinders.
//
//   HISTORY. Long before the drowned cities of the Reaches, Kaelmyr floated
//   above the Sundering Deep, a green shelf of the world's first age. When the
//   Deep was unmade, the shelf broke and rose burning through the drained seas.
//   Its people did not all die. The Sundering is remembered here not as an end
//   but as a beginning: the day the ash-sky closed and the calderas woke.
//
//   TRIBES woven through the twenty zones:
//     * The Emberkin    - ash-shamans and fire-cultists of the western calderas,
//                          who read prophecy in cinder and keep the pyres lit.
//     * The Cinderbound  - the bound dead, revenants shackled to labour the ash
//                          by older masters; some have slipped their chains.
//     * The Gloamwrights - glass-and-obsidian artificers of the black deserts,
//                          who forge weapons the sun never touched.
//     * The Stormheld    - sky-clans of the storm-spires who never set foot on
//                          ash, striking down from the thunderheads.
//     * The Hollow Choir - the final cult at the continent's wound, who sing to
//                          the drowned god sleeping beneath and mean to wake it.
//
//   THROUGH-LINE. The zones march west-to-east and down: from the ashen shore
//   where the Reaches spill their dead, through the Emberkin calderas, the
//   Cinderbound labour-fields, the Gloamwright glass deserts, up into the
//   Stormheld spires, and down at last into the Hollow Choir's wound, where the
//   Ashen King, Kaethyr the Unquenched, has ruled since the Sundering and means
//   to sing the sleeping god awake.

pub const KAELMYR_BASE: RoomId = 12_000;
const KAELMYR_W: usize = 13;
const KAELMYR_H: usize = 9;
const KAELMYR_ZONES: usize = KAELMYR_ZONES_DATA.len();
/// Kaelmyr mob ids sit in a fresh band above the Reaches (which use 950000+).
const KAELMYR_SPAWN_ID_START: u32 = 960_000;
const KAELMYR_SEED: u64 = 0xA54E_D4EA_D000_u64;
/// Each zone reserves this many room ids (a `KAELMYR_W`×`KAELMYR_H` cell field).
const KAELMYR_ZONE_STRIDE: u32 = (KAELMYR_W * KAELMYR_H) as u32;

/// Which Kaelmyr zones are carved as organic caverns (calderas, lava-tubes, and
/// the drowned wound) rather than braided mazes. The rest are mazes. Never a grid.
const fn kaelmyr_zone_is_cavern(z: usize) -> bool {
    matches!(z, 2 | 8 | 14 | 19)
}

pub fn is_kaelmyr_room(id: RoomId) -> bool {
    (KAELMYR_BASE..KAELMYR_BASE + KAELMYR_ZONES as u32 * KAELMYR_ZONE_STRIDE).contains(&id)
}

/// The world resist/weak pass (spec: CONTEXT.md, same-named section): one theme per
/// Kaelmyr zone, in `KAELMYR_ZONES_DATA` order. Regulars inherit the theme's
/// profile; the zone boss wears the theme's weakness but never its resist
/// (prep is a pure reward on the fight players provision for).
/// The burnt continent leans Frost-weak on purpose: fear of the cold is the
/// land's through-line.
const KAELMYR_ZONE_THEMES: [ZoneTheme; KAELMYR_ZONES] = [
    ZoneTheme::Sunscorched, // Cinderfall Shore
    ZoneTheme::Sunscorched, // Emberkin Terraces
    ZoneTheme::Ashen,       // Calder Vhael
    ZoneTheme::Sunscorched, // Pyre-Roads
    ZoneTheme::Beastwild,   // Sunless Vents
    ZoneTheme::Haunted,     // Cinderbound Fields
    ZoneTheme::Construct,   // Slagworks Ruin
    ZoneTheme::Undead,      // Ashen Barrows
    ZoneTheme::Crystal,     // Gloamwright Deeps
    ZoneTheme::Crystal,     // Black Deserts
    ZoneTheme::Crystal,     // Volcanoglass Reach
    ZoneTheme::Ashen,       // Ashfall Wastes
    ZoneTheme::Resonant,    // Stormheld Ascent
    ZoneTheme::Storm,       // Thunderspires
    ZoneTheme::Resonant,    // Cinder-Storms
    ZoneTheme::Resonant,    // Hollowing
    ZoneTheme::Profane,     // Choirhold Caverns
    ZoneTheme::Tidal,       // Drowned Wound
    ZoneTheme::Beastwild,   // Unquenched Throne
    ZoneTheme::Profane,     // Sundering Wound
];

/// Twenty zones of Kaelmyr: (zone, adjective, ground, landmark, creatures, three
/// mob names, boss). The tribe threading is carried in the mob/boss names and
/// the landmarks; `kaelmyr_desc` supplies the paragraph prose.
const KAELMYR_ZONES_DATA: [ZoneData; 20] = [
    (
        "Cinderfall Shore",
        "ash-choked",
        "grey cinder-drift",
        "the Reaches' dead heaped on a burnt strand",
        "shore-scavengers",
        [
            "a tide-cast revenant",
            "a cinder-crawler",
            "an ash-gorged carrion-thing",
        ],
        "Warden Vosk of the Drowned Gate",
    ),
    (
        "Emberkin Terraces",
        "smouldering",
        "warm pumice",
        "the smoke-terraces of the ash-shamans",
        "Emberkin zealots",
        ["an Emberkin acolyte", "a pyre-tender", "a cinder-reader"],
        "Mother Ashglass, First of the Emberkin",
    ),
    (
        "Calder Vhael",
        "furnace-hot",
        "cracked black glass",
        "a living caldera that breathes fire",
        "flame-born",
        ["a magma-drake whelp", "a living cinder", "an ember-wraith"],
        "Vhael, the Breathing Caldera",
    ),
    (
        "Pyre-Roads",
        "smoke-blind",
        "road-ash trodden hard",
        "the funeral roads the Emberkin walk their dead",
        "pyre-walkers",
        ["a masked pyre-priest", "an ash-pilgrim", "a cinder-hound"],
        "The Grand Cindarch",
    ),
    (
        "Sunless Vents",
        "sulphur-reeking",
        "hot vent-mud",
        "a field of shrieking fumaroles",
        "vent-dwellers",
        [
            "a sulphur-scaled lurker",
            "a vent-crawler",
            "a fume-choked horror",
        ],
        "The Vent-Mother",
    ),
    (
        "Cinderbound Fields",
        "chain-scarred",
        "trampled slag",
        "the labour-fields of the shackled dead",
        "the shackled dead",
        [
            "a chained cinderbound",
            "an overseer-wraith",
            "a slag-hauler revenant",
        ],
        "The Chainmaster Undying",
    ),
    (
        "Slagworks Ruin",
        "iron-stained",
        "cooled slag-heaps",
        "the broken foundries of a dead age",
        "foundry-haunts",
        ["a molten revenant", "a bellows-thrall", "a slag-golem"],
        "The Furnace-Lord",
    ),
    (
        "Ashen Barrows",
        "grave-still",
        "barrow-ash",
        "the burial-mounds of the Cinderbound's old masters",
        "barrow-bound",
        [
            "a barrow-shackled dead",
            "a grave-cinder wight",
            "an ash-entombed lord",
        ],
        "The King Beneath the Ash",
    ),
    (
        "Gloamwright Deeps",
        "obsidian-dark",
        "shard-strewn glass",
        "the glass-galleries of the black artificers",
        "glass-wrights",
        [
            "a Gloamwright artisan",
            "an obsidian sentinel",
            "a shard-familiar",
        ],
        "Archwright Sethume of the Black Glass",
    ),
    (
        "Black Deserts",
        "mirror-flat",
        "glassed black sand",
        "a desert fused to a single sheet of glass",
        "glass-stalkers",
        ["a mirage-hunter", "a glass-skinned nomad", "a heat-wraith"],
        "The Mirror of Noon",
    ),
    (
        "Volcanoglass Reach",
        "razor-bright",
        "fresh volcanic glass",
        "spires of glass drawn straight from the fire",
        "glasswork-guardians",
        [
            "a glass-forged knight",
            "a molten-cored sentinel",
            "a shard-swarm",
        ],
        "The Glasswright Tyrant",
    ),
    (
        "Ashfall Wastes",
        "snowing-ash",
        "deep grey drift",
        "a plain buried under endless falling ash",
        "wastes-wanderers",
        [
            "an ash-drowned nomad",
            "a drift-lurker",
            "a grey-lung revenant",
        ],
        "The Grey Pilgrim",
    ),
    (
        "Stormheld Ascent",
        "wind-torn",
        "bare wind-scoured rock",
        "the first ledges of the sky-clans",
        "sky-clan outriders",
        [
            "a Stormheld skirmisher",
            "a cliff-lancer",
            "a thunder-scout",
        ],
        "Warlord Skarn of the Stormheld",
    ),
    (
        "Thunderspires",
        "storm-crowned",
        "lightning-fused stone",
        "spires that comb the lightning from the sky",
        "spire-riders",
        [
            "a storm-lancer",
            "a levinbolt caster",
            "a gale-borne raider",
        ],
        "Aethelmyr, the Sky-Queen",
    ),
    (
        "Cinder-Storms",
        "ash-blind",
        "spinning cinder-wrack",
        "the eye of a standing firestorm",
        "storm-born",
        [
            "a firestorm revenant",
            "a whirling ember-horror",
            "a cinder-cyclone wraith",
        ],
        "The Heart of the Firestorm",
    ),
    (
        "Hollowing",
        "sound-swallowing",
        "hollow ash-crust",
        "where the ash-crust rings hollow over a void",
        "the hollowed-out",
        [
            "a hollow-voiced revenant",
            "a silence-eater",
            "an echo-wraith",
        ],
        "The First Voice of the Choir",
    ),
    (
        "Choirhold Caverns",
        "hymn-haunted",
        "damp sunken ash",
        "the cavern-halls of the drowned-god cult",
        "Choir-cultists",
        [
            "a Hollow Choir chorister",
            "a drowned-god zealot",
            "a hymn-bound wraith",
        ],
        "The Choirmaster of the Hollow",
    ),
    (
        "Drowned Wound",
        "abyss-cold",
        "the wound's black silt",
        "the wound where the seas drained away",
        "wound-dwellers",
        [
            "a wound-crawling horror",
            "a drained-sea revenant",
            "a fathom-cold terror",
        ],
        "The Thing the Seas Fled",
    ),
    (
        "Unquenched Throne",
        "fire-and-ash",
        "throne-cinders of a dead age",
        "the burning throne of the Ashen King",
        "throne-guard",
        [
            "an ash-crowned knight",
            "an Unquenched zealot",
            "a cinder-forged guardian",
        ],
        "Kaethyr the Unquenched, Ashen King of Kaelmyr",
    ),
    (
        "Sundering Wound",
        "world-unmaking",
        "the floor beneath the world",
        "the deepest scar, where the world was first cut",
        "the unmade",
        [
            "a herald of the unmaking",
            "an unmade terror",
            "a singer-of-the-end",
        ],
        "Kaethyr Ascendant, Who Sang the God Awake",
    ),
];

/// Kaelmyr's paragraph prose: an ashland twist on `frontier_desc`, so the new
/// continent reads as burnt rather than drowned or wild. Weaves the tribe/place
/// flavour and hits the >=180-char, >=2-sentence room-prose bar.
fn kaelmyr_desc(adj: &str, ground: &str, feature: &str, creature: &str, idx: u32) -> String {
    const TERRAIN: [&str; 5] = [
        "You pick across {adj} ground where {ground} crunches and shifts beneath every wary step, and the ash-sky hangs low and starless overhead.",
        "The land here is broken and burnt, the {ground} pale and treacherous, and a hot wind carries the reek of old fire out of the {adj} dark.",
        "This {adj} stretch offers no green thing and no shade; only {ground} runs grey to a horizon smudged out by drifting ash.",
        "The way winds between leaning shelves of scorched rock, the {ground} banked deep in the hollows and still warm to the touch.",
        "Cinders sift down without end across this {adj} reach, and the {ground} whispers underfoot like something trying to speak.",
    ];
    const FEATURE: [&str; 5] = [
        "Ahead looms {feature}, half-lost in the smoke and older than any living memory of it.",
        "Off the trail stands {feature}, a landmark for the few tribes that still walk these ashes.",
        "The blackened bones of {feature} jut from the drift, from an age before the Sundering closed the sky.",
        "Beside the way rests {feature}, silent witness to whatever fire first burned this world.",
        "Through the haze you make out {feature}, leaning under the weight of uncounted ash-falls.",
    ];
    const ATMOS: [&str; 5] = [
        "Somewhere in the smoke {creature} call to one another, and the sound is not one that welcomes strangers.",
        "The air hangs thick with menace, for {creature} have left their marks on stone and ash alike.",
        "Nothing stirs but the falling cinders, yet you feel {creature} watching from beyond the reddened murk.",
        "A charred reek drifts on the hot wind; {creature} hunt these ashes, and they hunt without mercy.",
        "A ringing quiet holds the reach, the quiet of a place from which {creature} have driven all else into the fire.",
    ];
    let i = idx as usize;
    let t = TERRAIN[i % 5]
        .replace("{adj}", adj)
        .replace("{ground}", ground);
    let f = FEATURE[(i / 5) % 5].replace("{feature}", feature);
    let a = ATMOS[(i / 7 + i) % 5].replace("{creature}", creature);
    format!("{t} {f} {a}")
}

/// The Ashen King's ashland places, filling in the non-entrance, non-boss cells.
const KAELMYR_PLACES: [&str; 10] = [
    "Cinderway",
    "Emberhollow",
    "Ashcrossing",
    "Smoke-Overlook",
    "Pyre-Mark",
    "Slag-Descent",
    "Cinder-Reach",
    "Ember-Gauntlet",
    "Ash-Sanctum",
    "Smoke-Threshold",
];

/// Build Kaelmyr, the Ashen Reach: twenty zones of braided mazes and organic
/// calderas, each carved (never a grid), chained deepest-room -> next-entrance,
/// and hung off the deepest room of the Sundered Reaches (Yssgar's chamber).
#[allow(clippy::needless_range_loop, clippy::type_complexity)]
fn extend_kaelmyr(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (KAELMYR_W, KAELMYR_H);
    let n = w * h;
    let mut spawn_id: u32 = KAELMYR_SPAWN_ID_START;
    let mut prev_exit: Option<RoomId> = None;

    for (z, &(zname, adj, ground, feature, creature, mob_names, boss)) in
        KAELMYR_ZONES_DATA.iter().enumerate()
    {
        let zbase = KAELMYR_BASE + (z as u32) * KAELMYR_ZONE_STRIDE;
        // Kaelmyr picks up a full continent past the Reaches (which sat at 12+z);
        // its power curve is the deepest in the game.
        let tier = (z + 32) as i32;
        let mut rng = MazeRng::new(KAELMYR_SEED ^ (z as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

        // Carve as a braided maze or an organic cavern (with the connectivity
        // pass), reducing both to a common form: which cells are rooms, their
        // distance from the entrance, and their open exits. A too-sparse cavern
        // falls back to a maze so no zone comes out empty. No uniform grids here.
        let cavern_floor = if kaelmyr_zone_is_cavern(z) {
            let floor = carve_cavern(w, h, &mut rng);
            (floor.iter().filter(|f| **f).count() >= 24).then_some(floor)
        } else {
            None
        };
        let (entrance, reachable, dist, cell_exits): (
            usize,
            Vec<bool>,
            Vec<usize>,
            Vec<Vec<(Dir, usize)>>,
        ) = if let Some(floor) = cavern_floor {
            let entrance = (0..n).find(|&i| floor[i]).unwrap_or(0);
            let dist = cavern_distances(&floor, w, h, entrance);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    let (x, y) = (c % w, c / w);
                    let consider = |nx: i64, ny: i64, d: Dir, v: &mut Vec<(Dir, usize)>| {
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let nb = ny as usize * w + nx as usize;
                            if reachable[nb] {
                                v.push((d, nb));
                            }
                        }
                    };
                    consider(x as i64, y as i64 - 1, Dir::North, &mut v);
                    consider(x as i64 + 1, y as i64, Dir::East, &mut v);
                    consider(x as i64, y as i64 + 1, Dir::South, &mut v);
                    consider(x as i64 - 1, y as i64, Dir::West, &mut v);
                    v
                })
                .collect();
            (entrance, reachable, dist, exits)
        } else {
            let open = carve_maze(w, h, &mut rng);
            let dist = maze_distances(&open, w, h, 0);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    for d in 0..4 {
                        if open[c][d]
                            && let Some(nb) = maze_neighbor(c, d, w, h)
                        {
                            v.push((DIRS[d], nb));
                        }
                    }
                    v
                })
                .collect();
            (0, reachable, dist, exits)
        };

        // The zone boss waits in the cell farthest from the entrance.
        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let zone: &'static str = Box::leak(format!("The {zname}").into_boxed_str());

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = zbase + cell as u32;
            let is_entrance = cell == entrance;
            let is_boss = cell == deepest && cell != entrance;
            let degree = cell_exits[cell].len();

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, zbase + *nb as u32))
                .collect();

            let name: &'static str = if is_entrance {
                Box::leak(format!("{zname} - the Ash-Gate").into_boxed_str())
            } else if is_boss {
                Box::leak(format!("{zname} - the Ashen Heart").into_boxed_str())
            } else {
                Box::leak(format!("{zname} - {}", KAELMYR_PLACES[cell % 10]).into_boxed_str())
            };
            let desc: &'static str = Box::leak(
                kaelmyr_desc(adj, ground, feature, creature, cell as u32).into_boxed_str(),
            );

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone,
                    safe: is_entrance && z == 0, // only the ashen shore is a safe waystation
                    pvp: false,
                    exits,
                },
            );

            if is_entrance {
                continue;
            }

            // Behaviour by maze-role: dead-ends ambush, junctions swarm, corridors
            // patrol or cast; the deepest cell holds the boss.
            let depth = dist[cell] as i32;
            let stormland = z >= 12; // the Stormheld spires and beyond crackle
            let (mob_name, behavior, boss_mob, hp, dmg) = if is_boss {
                (
                    boss,
                    MobBehavior::Brute,
                    true,
                    3200 + tier * 260,
                    128 + tier * 6,
                )
            } else if degree == 1 {
                (
                    mob_names[0],
                    MobBehavior::Ambusher,
                    false,
                    1900 + tier * 70 + depth * 7,
                    116 + tier * 4 + depth,
                )
            } else if degree >= 3 {
                (
                    mob_names[1],
                    if rng.chance(50) {
                        MobBehavior::PackHunter
                    } else {
                        MobBehavior::Summoner
                    },
                    false,
                    2000 + tier * 80 + depth * 7,
                    118 + tier * 5 + depth,
                )
            } else {
                // Leave some corridors quiet so the reach breathes.
                if rng.chance(35) {
                    continue;
                }
                let behavior = match rng.below(4) {
                    0 => MobBehavior::Wanderer,
                    1 => MobBehavior::Patroller,
                    2 => MobBehavior::Hunter,
                    _ => MobBehavior::Caster(if stormland {
                        DamageType::Lightning
                    } else {
                        DamageType::Fire
                    }),
                };
                (
                    mob_names[2],
                    behavior,
                    false,
                    1900 + tier * 70 + depth * 7,
                    116 + tier * 4 + depth,
                )
            };
            let attack = match behavior {
                MobBehavior::Caster(school) => school,
                _ => DamageType::Physical,
            };
            // The boss wears the zone's weakness but never its resist:
            // prep is a pure reward on the fight players provision for.
            let theme = KAELMYR_ZONE_THEMES[z];
            let profile = if boss_mob {
                DamageProfile::new(attack, None, theme.weak())
            } else {
                DamageProfile::new(attack, theme.resist(), theme.weak())
            };
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: hp,
                damage: dmg,
                // XP continues past the Reaches: Kaelmyr is the longest grind in
                // the game, ending at the Ashen King Ascendant.
                xp: if boss_mob {
                    1400 + tier * 110
                } else {
                    420 + tier * 45 + depth * 6
                },
                respawn_secs: if boss_mob { 600 } else { 90 },
                loot: super::items::kaelmyr_loot(z),
                boss: boss_mob,
                profile,
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }

        // Chain this zone to the previous one: the prior boss room descends to
        // this zone's ash-gate, and back up again.
        let entrance_id = zbase + entrance as u32;
        if let Some(prev) = prev_exit {
            if let Some(r) = rooms.get_mut(&prev) {
                r.exits.insert(Dir::Down, entrance_id);
            }
            if let Some(r) = rooms.get_mut(&entrance_id) {
                r.exits.insert(Dir::Up, prev);
            }
        }
        prev_exit = Some(zbase + deepest as u32);
    }

    // Hang Kaelmyr off the deepest room of the Sundered Reaches - Yssgar's
    // drowned chamber - so the whole continent is reachable and gated behind the
    // Bane of Yssgar. Descend through the wound Yssgar left, and rise back.
    let entrance = KAELMYR_BASE;
    if let Some(yssgar_room) = kaelmyr_seagate_room(rooms, spawns) {
        if let Some(hub) = rooms.get_mut(&yssgar_room) {
            hub.exits.insert(Dir::Down, entrance);
        }
        if let Some(r) = rooms.get_mut(&entrance) {
            r.exits.insert(Dir::Up, yssgar_room);
        }
    }
}

/// The room Kaelmyr hangs off: the Reaches room where Yssgar, the Sundering
/// Deep, makes his home - the deepest boss chamber of the whole drowned realm.
/// Falls back to any Sundering Deep room, then the Reaches sea-gate, so the
/// continent is never orphaned even if the Reaches change shape.
fn kaelmyr_seagate_room(rooms: &HashMap<RoomId, Room>, spawns: &[MobSpawn]) -> Option<RoomId> {
    spawns
        .iter()
        .find(|s| s.name == "Yssgar, the Sundering Deep")
        .map(|s| s.home)
        .filter(|home| rooms.contains_key(home))
        .or_else(|| {
            rooms
                .values()
                .filter(|r| is_reaches_room(r.id) && r.zone == "The Sundering Deep")
                .max_by_key(|r| r.id)
                .map(|r| r.id)
        })
        .or_else(|| rooms.contains_key(&REACHES_BASE).then_some(REACHES_BASE))
}

// ==== The Sunderlakes ======================================================
//
// A large, peaceful water country (rooms 16000+) of flooded caverns, reed
// labyrinths, island-dotted meres and drowned valleys, hung off the Melvanala
// high lake by a normal walk. Where Kaelmyr is a burnt end-game grind, the
// Sunderlakes are mid-game friendly and serene: fewer and weaker mobs, whole
// zones with nothing worse than a territorial pike, and - above all - fish. It
// is the fishing country of Lateania: forty species (items 4600..4700) are
// caught here at resource nodes gated by the Fishing skill, the prized deep
// catches waiting for anglers who have trained the trade high enough to reach
// the deep meres and drowned trenches.
//
//   LORE. When the Sundering drained the seas into Yssgar's wound and raised
//   Kaelmyr burning from the seabed, the water it displaced had to go somewhere.
//   It came down the mountain valleys behind Melvanala as a slow, silver flood
//   that never wholly went away. A thousand meres and drowned dells were left
//   behind, and the highland folk - who had always fished the one great lake -
//   found a whole country of new water to work. They call it the Sunderlakes:
//   the lakes the Sundering made. It is quiet, and green, and very deep in
//   places, and the fishing has no equal in the world.
//
//   THROUGH-LINE. The zones wind outward and downward from the Anglers' Dock on
//   the Melvanala shore: through reed labyrinths and island meres, down into the
//   flooded caverns, and at last into the drowned valleys where whole villages
//   lie under the water and the biggest fish of all move slow in the dark.

pub const LAKES_BASE: RoomId = 16_000;
const LAKES_W: usize = 11;
const LAKES_H: usize = 8;
const LAKES_ZONES: usize = LAKES_ZONES_DATA.len();
/// Sunderlakes mob ids sit in a fresh band above Kaelmyr (960000+) and the
/// Archipelago (970000+), clear of both.
const LAKES_SPAWN_ID_START: u32 = 980_000;
const LAKES_SEED: u64 = 0x5A17_1A4E_5000_u64;
/// Each zone reserves this many room ids (a `LAKES_W`×`LAKES_H` cell field).
const LAKES_ZONE_STRIDE: u32 = (LAKES_W * LAKES_H) as u32;

/// Which Sunderlakes zones are carved as organic caverns (the flooded cavern
/// halls) rather than braided reed-mazes. The rest are mazes. Never a grid.
/// Fishing nodes are only seeded in the maze zones (every maze cell exists, so
/// their node home rooms are guaranteed real; cavern floors are sparse).
const fn lakes_zone_is_cavern(z: usize) -> bool {
    matches!(z, 2 | 5 | 8 | 11)
}

pub fn is_lakes_room(id: RoomId) -> bool {
    (LAKES_BASE..LAKES_BASE + LAKES_ZONES as u32 * LAKES_ZONE_STRIDE).contains(&id)
}

/// The world resist/weak pass (spec: CONTEXT.md, same-named section): one theme per
/// Sunderlakes zone, in `LAKES_ZONES_DATA` order. Regulars inherit the
/// theme's profile; the zone boss wears the theme's weakness but never its resist
/// (prep is a pure reward on the fight players provision for).
const LAKES_ZONE_THEMES: [ZoneTheme; LAKES_ZONES] = [
    ZoneTheme::Tidal,     // Anglers' Dock
    ZoneTheme::Beastwild, // Reed Labyrinth
    ZoneTheme::Drowned,   // Sunken Grotto
    ZoneTheme::Beastwild, // Isle Meres
    ZoneTheme::Verdant,   // Willow Drowns
    ZoneTheme::Drowned,   // Weeping Caverns
    ZoneTheme::Verdant,   // Lily Reaches
    ZoneTheme::Haunted,   // Drowned Orchard
    ZoneTheme::Beastwild, // Glasswater Deep
    ZoneTheme::Fae,       // Fenlight Marsh
    ZoneTheme::Fae,       // Mirror Meres
    ZoneTheme::Drowned,   // Fathom Caverns
    ZoneTheme::Haunted,   // Drowned Valley
    ZoneTheme::Beastwild, // Mere-Mother's Deep
];

/// Fourteen zones of the Sunderlakes: (zone, adjective, water noun, landmark,
/// creatures, three mob names, a notable/boss). Kept peaceful - the mob names
/// lean toward wildlife and lost things rather than horrors, and the notables
/// are lake-guardians more than tyrants. `lakes_desc` supplies the prose.
const LAKES_ZONES_DATA: [ZoneData; 14] = [
    (
        "Anglers' Dock",
        "sun-dappled",
        "clear shallow water",
        "the long weathered jetties of the lake-fishers",
        "dock-things",
        [
            "a tangled net-wraith",
            "a snapping mere-turtle",
            "a bank-vole swarm",
        ],
        "Old Grib, the Jetty-Keeper",
    ),
    (
        "Reed Labyrinth",
        "whispering",
        "reed-shadowed water",
        "a maze of head-high reeds that hides the sky",
        "reed-lurkers",
        [
            "a reed-stalking heron",
            "a marsh-adder",
            "a whispering reed-spirit",
        ],
        "the Heron-King of the Reeds",
    ),
    (
        "Sunken Grotto",
        "green-lit",
        "still cavern water",
        "a flooded cave whose roof drips like slow rain",
        "grotto-dwellers",
        [
            "a blind cave-newt",
            "a pale grotto-crab",
            "a dripping stone-lurker",
        ],
        "the Grotto Warden",
    ),
    (
        "Isle Meres",
        "island-dotted",
        "wide open mere-water",
        "a scatter of green islets on a mirror-still mere",
        "islet-wildlife",
        ["a territorial mere-pike", "an otter-pack", "a nesting swan"],
        "the Great Mere-Otter",
    ),
    (
        "Willow Drowns",
        "leaf-shadowed",
        "root-tangled water",
        "a drowned willow-wood, its branches trailing in the flood",
        "willow-haunts",
        ["a willow-wisp", "a drowned root-thing", "a bank-heron"],
        "the Weeping Willow-Mother",
    ),
    (
        "Weeping Caverns",
        "echoing",
        "black cavern pools",
        "a cave-hall where the walls seem to weep cold water",
        "cavern-drifters",
        [
            "a cave-eel",
            "a drifting jelly-thing",
            "a stone-cold lurker",
        ],
        "the Weeping Deep-Thing",
    ),
    (
        "Lily Reaches",
        "flower-strewn",
        "lily-choked shallows",
        "a broad slow reach carpeted white and gold with waterlilies",
        "lily-dwellers",
        [
            "a lily-frog chorus",
            "a snapping snapping-turtle",
            "a heron in the reeds",
        ],
        "the Lilypad Sovereign",
    ),
    (
        "Drowned Orchard",
        "blossom-drowned",
        "orchard-flooded water",
        "an orchard sunk to its crowns, blossom floating on the flood",
        "orchard-ghosts",
        [
            "a drowned orchard-keeper",
            "a blossom-wisp",
            "a windfall-eel",
        ],
        "the Orchard-Drowned Steward",
    ),
    (
        "Glasswater Deep",
        "crystal-clear",
        "impossibly clear deep water",
        "a deep so clear the bottom seems a stone's throw and is a hundred feet",
        "deep-dwellers",
        [
            "a glass-clear ray",
            "a deep-water sturgeon",
            "a cold-current lurker",
        ],
        "the Glasswater Leviathan",
    ),
    (
        "Fenlight Marsh",
        "will-o-lit",
        "lantern-lit fen-water",
        "a fen where cold lights drift low over the standing water",
        "marsh-lights",
        ["a will-o'-the-wisp", "a fen-adder", "a bog-lurker"],
        "the Fenlight Warden",
    ),
    (
        "Mirror Meres",
        "sky-holding",
        "mirror-flat water",
        "meres so still they hold the mountains upside down",
        "mirror-dwellers",
        [
            "a mirror-carp",
            "a reflected wraith",
            "a still-water lurker",
        ],
        "the Mirror-Mere Guardian",
    ),
    (
        "Fathom Caverns",
        "lightless",
        "fathomless black water",
        "flooded caverns that fall away into water no light has touched",
        "fathom-things",
        ["an abyss-anglerfish", "a fathom-eel", "a lightless drifter"],
        "the Fathom-King's Herald",
    ),
    (
        "Drowned Valley",
        "sorrow-still",
        "valley-deep flood-water",
        "a whole valley and its village lost beneath the silver flood",
        "valley-drowned",
        [
            "a drowned villager",
            "a flooded belfry-ghost",
            "a silt-wading lurker",
        ],
        "the Bell-Drowned Warden",
    ),
    (
        "Mere-Mother's Deep",
        "sacred-still",
        "the deepest sacred water",
        "the oldest, deepest mere, where the fen-folk say the first fish still swims",
        "the deep-sacred",
        [
            "a shrine-guardian eel",
            "a deep-water leviathan",
            "an ancient mere-thing",
        ],
        "the Mere-Mother, Eldest of the Deep",
    ),
];

const LAKES_PLACES: [&str; 10] = [
    "Shallows",
    "Reed-Bend",
    "Islet",
    "Backwater",
    "Ford",
    "Deep-Channel",
    "Fishing-Stand",
    "Lily-Bank",
    "Still-Pool",
    "Landing",
];

/// The Sunderlakes' paragraph prose: a serene, watery counterpart to
/// `frontier_desc` / `kaelmyr_desc`. Peaceful by default, hitting the
/// >=180-char, multi-sentence room-prose bar. Varied by the cell index.
fn lakes_desc(adj: &str, water: &str, feature: &str, creature: &str, idx: u32) -> String {
    const TERRAIN: [&str; 5] = [
        "You wade a {adj} stretch where {water} laps warm and slow against your legs, and dragonflies stitch the bright air above the surface.",
        "The way winds along a bank of {adj} country, {water} spreading green and quiet on every side under a wide and gentle sky.",
        "Here the flood opens into {adj} calm; {water} stretches out mirror-smooth, and the only sound is the plink of a rising fish.",
        "A punt-track threads this {adj} reach, the {water} broken only by lily-pads and the slow spreading rings where something fed.",
        "The land lies half-drowned and {adj} here, {water} standing between low green hummocks where herons fish the margins undisturbed.",
    ];
    const FEATURE: [&str; 5] = [
        "Ahead lies {feature}, reflected whole and unbroken in the still water.",
        "Off across the water stands {feature}, a landmark the lake-fishers steer by.",
        "The flood has half-swallowed {feature}, and the sight of it is strange and peaceful at once.",
        "Beside the channel rests {feature}, softened by weed and the long patient work of the water.",
        "Through the reeds you glimpse {feature}, quiet and green and older than the flood that drowned it.",
    ];
    const ATMOS: [&str; 5] = [
        "Somewhere out on the water {creature} go about their business, paying you no mind at all.",
        "The water is thick with life; {creature} move through the shallows, and the fishing here is famously fine.",
        "It is a good place to cast a line - {creature} rise all around, and the deep water keeps its own counsel.",
        "A heron lifts off unhurried, and {creature} slip away into the reeds; nothing here means you any harm.",
        "The quiet is broken only by {creature} and the slow music of moving water, and it soothes the traveller's heart.",
    ];
    let i = idx as usize;
    let t = TERRAIN[i % 5]
        .replace("{adj}", adj)
        .replace("{water}", water);
    let f = FEATURE[(i / 5) % 5].replace("{feature}", feature);
    let a = ATMOS[(i / 7 + i) % 5].replace("{creature}", creature);
    format!("{t} {f} {a}")
}

/// Build the Sunderlakes: fourteen zones of braided reed-mazes and flooded
/// caverns (rooms 16000+), each carved (never a grid), chained deepest-room ->
/// next-entrance, and hung off the Melvanala high lake by a normal walk. Kept
/// peaceful: mobs are fewer and weaker than Kaelmyr, whole corridors are left
/// serene, and the draw is the fishing (see `NODES` fishing spots at 16000+).
#[allow(clippy::needless_range_loop, clippy::type_complexity)]
fn extend_lakes(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (LAKES_W, LAKES_H);
    let n = w * h;
    let mut spawn_id: u32 = LAKES_SPAWN_ID_START;
    let mut prev_exit: Option<RoomId> = None;

    for (z, &(zname, adj, water, feature, creature, mob_names, boss)) in
        LAKES_ZONES_DATA.iter().enumerate()
    {
        let zbase = LAKES_BASE + (z as u32) * LAKES_ZONE_STRIDE;
        // A gentle mid-game power band that rises across the zones. Far below
        // Kaelmyr - the Sunderlakes are meant to be enjoyable, not a wall.
        let tier = z as i32;
        let mut rng = MazeRng::new(LAKES_SEED ^ (z as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

        // Carve as a braided reed-maze or a flooded cavern (with the
        // connectivity pass). A too-sparse cavern falls back to a maze so no
        // zone comes out empty. No uniform grids.
        let cavern_floor = if lakes_zone_is_cavern(z) {
            let floor = carve_cavern(w, h, &mut rng);
            (floor.iter().filter(|f| **f).count() >= 24).then_some(floor)
        } else {
            None
        };
        let (entrance, reachable, dist, cell_exits): (
            usize,
            Vec<bool>,
            Vec<usize>,
            Vec<Vec<(Dir, usize)>>,
        ) = if let Some(floor) = cavern_floor {
            let entrance = (0..n).find(|&i| floor[i]).unwrap_or(0);
            let dist = cavern_distances(&floor, w, h, entrance);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    let (x, y) = (c % w, c / w);
                    let consider = |nx: i64, ny: i64, d: Dir, v: &mut Vec<(Dir, usize)>| {
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let nb = ny as usize * w + nx as usize;
                            if reachable[nb] {
                                v.push((d, nb));
                            }
                        }
                    };
                    consider(x as i64, y as i64 - 1, Dir::North, &mut v);
                    consider(x as i64 + 1, y as i64, Dir::East, &mut v);
                    consider(x as i64, y as i64 + 1, Dir::South, &mut v);
                    consider(x as i64 - 1, y as i64, Dir::West, &mut v);
                    v
                })
                .collect();
            (entrance, reachable, dist, exits)
        } else {
            let open = carve_maze(w, h, &mut rng);
            let dist = maze_distances(&open, w, h, 0);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    for d in 0..4 {
                        if open[c][d]
                            && let Some(nb) = maze_neighbor(c, d, w, h)
                        {
                            v.push((DIRS[d], nb));
                        }
                    }
                    v
                })
                .collect();
            (0, reachable, dist, exits)
        };

        // The zone's notable waits in the cell farthest from the entrance.
        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let zone: &'static str = Box::leak(format!("The {zname}").into_boxed_str());

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = zbase + cell as u32;
            let is_entrance = cell == entrance;
            let is_boss = cell == deepest && cell != entrance;
            let degree = cell_exits[cell].len();

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, zbase + *nb as u32))
                .collect();

            let name: &'static str = if is_entrance {
                Box::leak(format!("{zname} - the Landing").into_boxed_str())
            } else if is_boss {
                Box::leak(format!("{zname} - the Deep Water").into_boxed_str())
            } else {
                Box::leak(format!("{zname} - {}", LAKES_PLACES[cell % 10]).into_boxed_str())
            };
            let desc: &'static str =
                Box::leak(lakes_desc(adj, water, feature, creature, cell as u32).into_boxed_str());

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone,
                    // The whole first zone is a safe angler's haven; deeper
                    // zones keep their entrance landings safe too, so the country
                    // reads as friendly resting-water between the fishing.
                    safe: is_entrance,
                    pvp: false,
                    exits,
                },
            );

            if is_entrance {
                continue;
            }

            let depth = dist[cell] as i32;
            // A peaceful country: the notable is a modest guardian, and only a
            // fraction of the other cells hold anything at all - and what they
            // hold is weak. Dead-ends may hide an ambusher, junctions a small
            // pack, corridors are mostly empty water.
            let (mob_name, behavior, boss_mob, hp, dmg): (&str, MobBehavior, bool, i32, i32) =
                if is_boss {
                    (
                        boss,
                        MobBehavior::Brute,
                        true,
                        420 + tier * 80,
                        22 + tier * 3,
                    )
                } else if degree == 1 {
                    // Half the dead-ends are simply quiet.
                    if rng.chance(50) {
                        continue;
                    }
                    (
                        mob_names[0],
                        MobBehavior::Ambusher,
                        false,
                        150 + tier * 22 + depth * 3,
                        12 + tier + depth / 2,
                    )
                } else if degree >= 3 {
                    if rng.chance(45) {
                        continue;
                    }
                    (
                        mob_names[1],
                        MobBehavior::PackHunter,
                        false,
                        160 + tier * 24 + depth * 3,
                        13 + tier + depth / 2,
                    )
                } else {
                    // Corridors are mostly serene open water.
                    if rng.chance(72) {
                        continue;
                    }
                    let behavior = match rng.below(3) {
                        0 => MobBehavior::Wanderer,
                        1 => MobBehavior::Patroller,
                        _ => MobBehavior::Skirmisher,
                    };
                    (
                        mob_names[2],
                        behavior,
                        false,
                        150 + tier * 22 + depth * 3,
                        11 + tier + depth / 2,
                    )
                };
            // The boss wears the zone's weakness but never its resist:
            // prep is a pure reward on the fight players provision for.
            let theme = LAKES_ZONE_THEMES[z];
            let profile = if boss_mob {
                DamageProfile::new(DamageType::Physical, None, theme.weak())
            } else {
                DamageProfile::new(DamageType::Physical, theme.resist(), theme.weak())
            };
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: hp,
                damage: dmg,
                xp: if boss_mob {
                    120 + tier * 30
                } else {
                    28 + tier * 8 + depth * 2
                },
                respawn_secs: if boss_mob { 240 } else { 60 },
                // Regular mobs drop from the zone's fish band; the zone's
                // notable also carries a shot at its own two Wildbound finds.
                loot: if boss_mob {
                    lakes_notable_loot(z)
                } else {
                    lakes_loot(z)
                },
                boss: boss_mob,
                profile,
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }

        // Chain this zone to the previous one: the prior deep-water room descends
        // to this zone's landing, and rises back.
        let entrance_id = zbase + entrance as u32;
        if let Some(prev) = prev_exit {
            if let Some(r) = rooms.get_mut(&prev) {
                r.exits.insert(Dir::Down, entrance_id);
            }
            if let Some(r) = rooms.get_mut(&entrance_id) {
                r.exits.insert(Dir::Up, prev);
            }
        }
        prev_exit = Some(zbase + deepest as u32);
    }

    // Hang the Sunderlakes off the Melvanala high lake by a normal walk exit, so
    // the whole country is reachable. The first landing (Anglers' Dock) is a safe
    // haven. Lightly gated or not at all - it is meant to be mid-game friendly.
    let entrance = LAKES_BASE;
    let portal = [Dir::South, Dir::East, Dir::West, Dir::North, Dir::Down]
        .into_iter()
        .find(|d| {
            rooms
                .get(&MELVANALA_SQUARE)
                .is_some_and(|r| !r.exits.contains_key(d))
        })
        .unwrap_or(Dir::South);
    if let Some(hub) = rooms.get_mut(&MELVANALA_SQUARE) {
        hub.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), MELVANALA_SQUARE);
    }
}

/// A small drop table for a Sunderlakes zone: the zone's own band of fish, so a
/// slain lake-notable may yield a catch. Fish resolve through `item`.
fn lakes_loot(z: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        (0..LAKES_ZONES)
            .map(|zone| {
                // Each zone's fish band (see `lakes_fish_for_zone`).
                lakes_fish_for_zone(zone).to_vec()
            })
            .collect()
    });
    tables[z.min(LAKES_ZONES - 1)].as_slice()
}

/// A Sunderlakes notable's loot: the zone's fish band plus its own two unique
/// Wildbound finds, so the zone's guardian has a real shot at gear a fish
/// stall would never sell.
fn lakes_notable_loot(z: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        (0..LAKES_ZONES)
            .map(|zone| {
                let mut v = lakes_fish_for_zone(zone);
                v.extend(super::items::sunderlakes_find_ids(zone));
                v
            })
            .collect()
    });
    tables[z.min(LAKES_ZONES - 1)].as_slice()
}

/// The fish species (item ids) associated with a Sunderlakes zone. The forty
/// fish are spread across the ten maze zones in order of prestige (four per
/// maze zone); cavern zones borrow the band of the nearest maze zone for their
/// loot. This same mapping seeds the fishing NODES (see `LAKES_FISH_NODES`).
fn lakes_fish_for_zone(z: usize) -> Vec<u32> {
    // Maze zones in ascending order carry the fish bands 0..10; a cavern zone
    // borrows the band of the maze zone just before it.
    let maze_rank = (0..=z).filter(|&zz| !lakes_zone_is_cavern(zz)).count();
    let band = maze_rank.saturating_sub(1).min(9);
    let base = super::items::FISH_BASE + (band as u32) * 4;
    (0..4).map(|i| base + i).collect()
}

// ---- Broceliande, the Greenwood (rooms 22000+) ---------------------------
//
//   Broceliande is a vast, verdant continent east of the Verdant Highlands: a
//   Dark-Age-of-Camelot country of deep-green oakwoods and steaming ferny
//   jungles, druid groves and briar mazes, standing stones and faerie rings,
//   moss-grown keeps and vine-choked ruins. Its through-line is the old celtic
//   dream of the enchanted wood: you enter at a safe woodward's holt on the
//   forest eaves and wind ever deeper and greener, past the druid circles and
//   the sleeping keeps, down into the jungle heart and the World-Oak at its
//   centre, where the Greenwood's oldest guardian still keeps its long watch.
//
//   Twenty zones of ~99 rooms each (~2000 rooms), every one carved as a braided
//   briar-maze (`carve_maze`) or an organic fern-cavern / grove-glade
//   (`carve_cavern`) - never a uniform grid. Zones chain deepest-room ->
//   next-entrance; mobs are behaviour-driven by maze-role (dead-ends ambush,
//   junctions swarm, corridors patrol/skirmish). Light-to-moderate gating: the
//   whole wood is reached by a normal walk off the Verdant Highlands (the
//   Faerie Hollow), and the first holt is a safe haven. It is a moderate
//   continent - tougher than the peaceful Sunderlakes but well below the
//   endgame Kaelmyr - and it is the home of the fifty tameable beasts of the
//   animal-taming trade (see `taming.rs`).

pub const BROCELIANDE_BASE: RoomId = 22_000;
const BROCELIANDE_W: usize = 11;
const BROCELIANDE_H: usize = 9;
const BROCELIANDE_ZONES: usize = BROCELIANDE_ZONES_DATA.len();
/// Broceliande mob ids sit in a fresh band above the Sunderlakes (980000+),
/// clear of every other region. Excluded from the endgame scaler (see
/// `tune_spawn_balance`) so the Greenwood stays a moderate, green country.
pub const BROCELIANDE_SPAWN_ID_START: u32 = 990_000;
const BROCELIANDE_SEED: u64 = 0xB70C_E11A_9DE0_u64;
/// Each zone reserves this many room ids (a `BROCELIANDE_W`×`BROCELIANDE_H`
/// cell field). Public so the taming system can place beasts within a zone.
pub const BROCELIANDE_ZONE_STRIDE: u32 = (BROCELIANDE_W * BROCELIANDE_H) as u32;
/// Number of Broceliande zones, public for beast placement.
pub const BROCELIANDE_ZONE_COUNT: usize = BROCELIANDE_ZONES;

/// Which Broceliande zones are carved as organic caverns/glades (fern grottoes,
/// grove-clearings, jungle sinks) rather than braided briar-mazes. The rest are
/// mazes. Never a uniform grid.
const fn broceliande_zone_is_cavern(z: usize) -> bool {
    matches!(z, 2 | 5 | 8 | 11 | 14 | 17)
}

pub fn is_broceliande_room(id: RoomId) -> bool {
    (BROCELIANDE_BASE..BROCELIANDE_BASE + BROCELIANDE_ZONES as u32 * BROCELIANDE_ZONE_STRIDE)
        .contains(&id)
}

/// Where a procedurally-generated room sits inside its own generator grid.
/// `region`/`zone` name the grid it belongs to, `(x, y)` is its cell within
/// that `zone_w` x `zone_h` grid, and `z` is the map level (0 surface, negative
/// underground). Rooms are numbered `base [+ zone*stride] + cell` and cells are
/// `(cell % w, cell / w)`, so this is a pure decode of the room id. Returns
/// `None` for hand-authored rooms (the capitals, roads, villages, housing,
/// archipelago) which have no grid and are laid out by walking exits instead.
/// The overhead world map uses this to place each zone as an exact,
/// collision-free block rather than flattening the whole world onto one plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionPlacement {
    pub region: &'static str,
    pub zone: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub zone_w: i32,
    pub zone_h: i32,
    /// How many zones this region chains together, so a room can say where it
    /// sits in the run ("zone 7 of 20") rather than only naming itself. A
    /// single-grid region is its own whole chain, so this is 1 there.
    pub zone_count: u32,
}

pub fn region_layout(id: RoomId) -> Option<RegionPlacement> {
    // Single-grid regions: `id = base + cell`.
    let single = |region, base: RoomId, w: usize, h: usize, z: i32| {
        (base..base + (w * h) as u32).contains(&id).then(|| {
            let cell = (id - base) as i32;
            RegionPlacement {
                region,
                zone: 0,
                x: cell % w as i32,
                y: cell / w as i32,
                z,
                zone_w: w as i32,
                zone_h: h as i32,
                zone_count: 1,
            }
        })
    };
    if let Some(p) = single("catacombs", CATACOMBS_BASE, CATACOMBS_W, CATACOMBS_H, -1) {
        return Some(p);
    }
    if let Some(p) = single("thornwood", THORNWOOD_BASE, THORNWOOD_W, THORNWOOD_H, 0) {
        return Some(p);
    }
    if let Some(p) = single("caverns", CAVERNS_BASE, CAVERNS_W, CAVERNS_H, -1) {
        return Some(p);
    }

    // Multi-zone regions: `id = base + zone*stride + cell`, stride = w*h.
    let multi = |region, base: RoomId, w: usize, h: usize, z: i32, zones: usize| {
        let stride = (w * h) as u32;
        let off = id - base;
        let zone = off / stride;
        let cell = (off % stride) as i32;
        RegionPlacement {
            region,
            zone,
            x: cell % w as i32,
            y: cell / w as i32,
            z,
            zone_w: w as i32,
            zone_h: h as i32,
            zone_count: zones as u32,
        }
    };
    if is_frontier_room(id) {
        return Some(multi(
            "frontier",
            FRONTIER_BASE,
            FRONTIER_W as usize,
            FRONTIER_H as usize,
            -1,
            FRONTIER_ZONES,
        ));
    }
    if is_reaches_room(id) {
        return Some(multi(
            "reaches",
            REACHES_BASE,
            REACHES_W,
            REACHES_H,
            0,
            REACHES_ZONES,
        ));
    }
    if is_kaelmyr_room(id) {
        return Some(multi(
            "kaelmyr",
            KAELMYR_BASE,
            KAELMYR_W,
            KAELMYR_H,
            0,
            KAELMYR_ZONES,
        ));
    }
    if is_lakes_room(id) {
        return Some(multi("lakes", LAKES_BASE, LAKES_W, LAKES_H, 0, LAKES_ZONES));
    }
    if is_broceliande_room(id) {
        return Some(multi(
            "broceliande",
            BROCELIANDE_BASE,
            BROCELIANDE_W,
            BROCELIANDE_H,
            0,
            BROCELIANDE_ZONES,
        ));
    }
    if is_aelunor_room(id) {
        return Some(multi(
            "aelunor",
            AELUNOR_BASE,
            AELUNOR_W,
            AELUNOR_H,
            0,
            AELUNOR_ZONES,
        ));
    }
    if is_wildbound_room(id) {
        return wildbound_layout(id);
    }
    None
}

/// The Wildbound Waste's map block: each biome is its own reserved `w` x `h`
/// field, with that biome's four gate-town rooms anchored on the carve's
/// entrance cell so the gate sits directly above the field room its South
/// exit really opens onto (pinned by `each_wildbound_gate_sits_directly_
/// above_the_field_cell_it_opens_onto`). The entrance is the first floor
/// cell in row-major order, so every cell before it is wall: the four town
/// cells (one row up and two rows up around the entrance's column) can never
/// land on a field room. Unlike the other chained regions the three biomes
/// are not one uniform grid, so this decodes by hand instead of going
/// through `multi`.
fn wildbound_layout(id: RoomId) -> Option<RegionPlacement> {
    let off = id - WILDBOUND_BASE;
    let zone = off / WILDBOUND_BIOME_STRIDE;
    let slot = off % WILDBOUND_BIOME_STRIDE;
    let biome = &WILDBOUND_BIOMES[zone as usize];
    let (w, h) = (biome.w as i32, biome.h as i32);
    let entrance = WILDBOUND_ENTRANCES[zone as usize] as i32;
    let (ex, ey) = (entrance % w, entrance / w);
    let (x, y) = match slot {
        0 => (ex, ey - 2),     // the square
        1 => (ex - 1, ey - 2), // the shelter, west of the square
        2 => (ex + 1, ey - 2), // the outfitter, east of the square
        3 => (ex, ey - 1),     // the gate, directly above the entrance cell
        4..=9 => return None,  // reserved, never built
        // The contested field: `field_base = base + 10`, one id per cell.
        _ => {
            let cell = slot as i32 - 10;
            if cell >= w * h {
                return None;
            }
            (cell % w, cell / w)
        }
    };
    Some(RegionPlacement {
        region: "wildbound",
        zone,
        x,
        y,
        z: 0,
        zone_w: w,
        zone_h: h + 2,
        zone_count: WILDBOUND_BIOMES.len() as u32,
    })
}

/// A room's map biome, for colouring the overhead world map. Derived from its
/// region, and for the mixed continents from whether its zone was carved as a
/// cavern (`*_zone_is_cavern`) rather than open ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    /// The safe home fields around Embergate.
    Heartland,
    /// Open overworld between the capitals.
    Plains,
    /// Capitals, city districts, portal villages, player housing.
    Urban,
    /// Greenwood: Broceliande and the Thornwood.
    Forest,
    /// The Sunderlakes and open water.
    Water,
    /// The Shattered Archipelago.
    Islands,
    /// Kaelmyr, the Ashen Reach: ash and volcanic ground.
    Ash,
    /// Underground: the catacombs, the Drowned Caverns, and any cavern zone.
    Cavern,
    /// The Frontier and the Sundered Reaches: broken, hostile ground.
    Badlands,
}

pub fn biome_of(id: RoomId) -> Biome {
    if let Some(p) = region_layout(id) {
        let cavern_zone = match p.region {
            "reaches" => reaches_zone_is_cavern(p.zone as usize),
            "kaelmyr" => kaelmyr_zone_is_cavern(p.zone as usize),
            "lakes" => lakes_zone_is_cavern(p.zone as usize),
            "broceliande" => broceliande_zone_is_cavern(p.zone as usize),
            _ => false,
        };
        if cavern_zone {
            return Biome::Cavern;
        }
        return match p.region {
            "catacombs" | "caverns" => Biome::Cavern,
            "thornwood" | "broceliande" | "aelunor" => Biome::Forest,
            "kaelmyr" => Biome::Ash,
            "lakes" => Biome::Water,
            "reaches" | "frontier" => Biome::Badlands,
            // The Waste's three biomes are three different lands: bramble
            // forest, the maze cavern past it, then burnt flats.
            "wildbound" => match p.zone {
                0 => Biome::Forest,
                1 => Biome::Cavern,
                _ => Biome::Badlands,
            },
            _ => Biome::Plains,
        };
    }
    // Hand-authored rooms, by id block.
    if super::archipelago::is_archipelago_room(id) {
        return Biome::Islands;
    }
    if super::archipelago::is_village_room(id) {
        return Biome::Urban;
    }
    if (super::housing::HOUSING_BASE..super::housing::HOUSING_BASE + 1000).contains(&id) {
        return Biome::Urban;
    }
    if (1..600).contains(&id) {
        return Biome::Heartland;
    }
    if (3000..3100).contains(&id) {
        return Biome::Urban;
    }
    Biome::Plains
}

/// The atlas entry (display name, danger tier) whose id range contains `id`.
/// Powers the region-level badge on the overhead map. Tiers are the same
/// vocabulary the text atlas uses: "safe / low", "wilds", "endgame", "brutal",
/// "deadly", etc.
pub fn region_atlas_entry(id: RoomId) -> Option<(&'static str, &'static str)> {
    REGIONS
        .iter()
        .find(|(_, lo, hi, _, _)| (*lo..*hi).contains(&id))
        .map(|&(name, _, _, tier, _)| (name, tier))
}

/// The world resist/weak pass (spec: CONTEXT.md, same-named section): one theme per
/// Broceliande zone, in `BROCELIANDE_ZONES_DATA` order. Regulars inherit the
/// theme's profile; the zone boss wears the theme's weakness but never its resist
/// (prep is a pure reward on the fight players provision for).
/// The Greenwood leans Fire-weak on purpose: burning the wood is the answer
/// the country teaches.
const BROCELIANDE_ZONE_THEMES: [ZoneTheme; BROCELIANDE_ZONES] = [
    ZoneTheme::Beastwild, // Woodward's Holt
    ZoneTheme::Verdant,   // Oakheart Grove
    ZoneTheme::Fungal,    // Fernlight Hollow
    ZoneTheme::Resonant,  // Druid's Circle
    ZoneTheme::Beastwild, // Briarmaze Thicket
    ZoneTheme::Fae,       // Whispering Fens
    ZoneTheme::Haunted,   // Verdant Ruins
    ZoneTheme::Fae,       // Moonshadow Glade
    ZoneTheme::Beastwild, // Steaming Jungle
    ZoneTheme::Verdant,   // Vine-Choked Deep
    ZoneTheme::Undead,    // Standing Kings
    ZoneTheme::Undead,    // Barrowgreen
    ZoneTheme::Fae,       // Faerie Reaches
    ZoneTheme::Beastwild, // Wyrmfern Hollows
    ZoneTheme::Haunted,   // Greenmantle Keep
    ZoneTheme::Verdant,   // Thornwyrd Maze
    ZoneTheme::Fae,       // Cernunmoor
    ZoneTheme::Fungal,    // Worldroot Deep
    ZoneTheme::Resonant,  // Greenmarch Heart
    ZoneTheme::Fae,       // World-Oak Crown
];

/// Twenty zones of Broceliande: (zone, adjective, greenery noun, a landmark
/// feature, the creatures that haunt it, three regular mob names, the zone
/// notable/boss). Celtic/arthurian tone throughout; `broceliande_desc` supplies
/// the paragraph prose. Zone names must NOT start with "The " (the builder does
/// not prepend it here, but keeps them clean for the leaked zone label).
const BROCELIANDE_ZONES_DATA: [ZoneData; 20] = [
    (
        "Woodward's Holt",
        "sun-dappled",
        "old green oakwood",
        "the mossy palisade of the woodwards who keep the forest eaves",
        "eaves-dwellers",
        [
            "a bristling forest-boar",
            "a green-eyed wildcat",
            "a briar-tangled poacher",
        ],
        "Aldwyn the Woodward-Reeve",
    ),
    (
        "Oakheart Grove",
        "cathedral-tall",
        "ancient oak columns",
        "a grove of oaks so old their crowns close out the sky",
        "grove-wardens",
        [
            "a moss-antlered stag",
            "a grove-adder",
            "a bark-skinned wood-wight",
        ],
        "the Oakheart Dryad",
    ),
    (
        "Fernlight Hollow",
        "green-lit",
        "waist-deep fern",
        "a sunken fern-grotto where the light falls green and thick as water",
        "hollow-things",
        [
            "a fern-lurking lynx",
            "a spore-drunk boar",
            "a pale hollow-stalker",
        ],
        "the Fernlight Warden",
    ),
    (
        "Druid's Circle",
        "stone-ringed",
        "grass cropped short by rites",
        "a great ring of moss-furred standing stones humming with old power",
        "circle-keepers",
        [
            "a mistletoe druid",
            "a stone-guardian hound",
            "a robed circle-acolyte",
        ],
        "the Archdruid of the Circle",
    ),
    (
        "Briarmaze Thicket",
        "thorn-walled",
        "impassable briar",
        "a labyrinth of thorn twice a man's height that shifts when unwatched",
        "briar-haunts",
        [
            "a thorn-crowned wolf",
            "a bramble-wight",
            "a lost knight-errant",
        ],
        "the Briar-Knight of the Thicket",
    ),
    (
        "Whispering Fens",
        "will-o-lit",
        "green standing water",
        "a fen where cold lights drift and the reeds whisper old names",
        "fen-lurkers",
        [
            "a fen-adder",
            "a bog-drowned reaver",
            "a marsh-lantern wisp",
        ],
        "the Drowned Green Man",
    ),
    (
        "Verdant Ruins",
        "vine-choked",
        "green-shrouded ruin",
        "a fallen keep swallowed whole by ivy, its halls floored with leaf-mould",
        "ruin-dwellers",
        [
            "an ivy-shrouded revenant",
            "a ruin-prowling panther",
            "a tomb-robber's ghost",
        ],
        "the Ivy-Crowned Castellan",
    ),
    (
        "Moonshadow Glade",
        "moon-silvered",
        "silver-lit sward",
        "a perfect round glade where the moon seems always to hang low and full",
        "glade-fae",
        [
            "a silver-pelt hare-king",
            "a moonshadow hound",
            "a glamour-weaving fae",
        ],
        "the Lady of the Moonlit Glade",
    ),
    (
        "Steaming Jungle",
        "steam-wreathed",
        "dripping green jungle",
        "a hot green tangle where steam rises off the leaf-litter in slow ghosts",
        "jungle-things",
        [
            "a jungle-drake",
            "a coiling constrictor",
            "a fever-mad huntsman",
        ],
        "the Jungle-Drake Matriarch",
    ),
    (
        "Vine-Choked Deep",
        "sun-starved",
        "black-green undergrowth",
        "a jungle deep so thick with vine that noon is a green midnight",
        "deep-lurkers",
        [
            "a strangler-vine horror",
            "a deep-jungle panther",
            "a vine-bound wanderer",
        ],
        "the Strangler-Vine Sovereign",
    ),
    (
        "Standing Kings",
        "storm-crowned",
        "windswept moorgrass",
        "a high heath crowned with monolith-kings that were old before the wood",
        "king-stone wraiths",
        [
            "a moor-wolf pack-leader",
            "a barrow-crowned wight",
            "a storm-called reaver",
        ],
        "the King in the Stone",
    ),
    (
        "Barrowgreen",
        "grave-still",
        "grass over old barrows",
        "a green field of burial-mounds where the dead of the wood were laid",
        "barrow-dead",
        [
            "a barrow-wight",
            "a grave-hound",
            "a mound-crowned revenant",
        ],
        "the Barrow-King of the Green",
    ),
    (
        "Faerie Reaches",
        "gold-hazed",
        "toadstool-ringed meadow",
        "a golden-lit country of faerie-rings where time itself runs thick and slow",
        "the fair folk",
        [
            "a redcap raider",
            "a will-o'-wisp",
            "a glamoured changeling",
        ],
        "the Erlking of the Reaches",
    ),
    (
        "Wyrmfern Hollows",
        "fern-drowned",
        "giant unfurling fern",
        "a jungle sink of tree-tall ferns where the great wyrms of the wood den",
        "wyrm-kin",
        [
            "a fern-wyrmling",
            "a hollow-denning drake",
            "a scale-hunter gone feral",
        ],
        "the Fern-Wyrm of the Hollows",
    ),
    (
        "Greenmantle Keep",
        "moss-mantled",
        "ivy-mantled stonework",
        "a keep the forest has taken for its own, moss for banners, roots for kings",
        "keep-haunts",
        [
            "a moss-mantled sentinel",
            "a keep-warden hound",
            "a green-armoured revenant",
        ],
        "the Green Warden of the Keep",
    ),
    (
        "Thornwyrd Maze",
        "blood-thorned",
        "wicked black thorn",
        "the deepest briar-labyrinth, its thorns dark and wet and hungry",
        "thornwyrd-things",
        [
            "a thorn-wyrm",
            "a bloodbriar stalker",
            "a maze-lost champion",
        ],
        "the Thornwyrd, the Maze-that-Hungers",
    ),
    (
        "Cernunmoor",
        "antler-shadowed",
        "wild heath under horn",
        "a wide wild heath overhung by the antler-shadow of the Horned One's presence",
        "the wild hunt",
        [
            "a hunt-hound of Cernunnos",
            "a horn-crowned stag-lord",
            "a spectral huntsman",
        ],
        "Cernunnos' Master of the Hunt",
    ),
    (
        "Worldroot Deep",
        "root-cavernous",
        "cavern-root and pale fungus",
        "a cavern-deep of the World-Oak's roots, floored with pale luminous fungus",
        "root-dwellers",
        [
            "a root-burrowing horror",
            "a cave-panther of the deep",
            "a fungus-riddled wanderer",
        ],
        "the Rootward of the Deep",
    ),
    (
        "Greenmarch Heart",
        "hush-fallen",
        "the wood's own green silence",
        "the still green heart of the march, where every path of the wood at last converges",
        "heart-guardians",
        [
            "a heart-oak treant",
            "a green-warden wyrm",
            "an old guardian of the march",
        ],
        "the Heart-Oak Elder",
    ),
    (
        "World-Oak Crown",
        "ageless",
        "the crown-roots of the World-Oak",
        "the crown of the World-Oak itself, older than the forest that grew from it",
        "the oldest green things",
        [
            "an ancient forest-drake",
            "a bark-armoured great treant",
            "a guardian of the first wood",
        ],
        "Broceliande, the Green Wyrm of the World-Oak",
    ),
];

const BROCELIANDE_PLACES: [&str; 10] = [
    "Green Ride",
    "Fern Path",
    "Oak Stand",
    "Briar Turn",
    "Moss Hollow",
    "Deer-Track",
    "Root Bend",
    "Ivy Ford",
    "Glade-Edge",
    "Thornway",
];

/// Broceliande's paragraph prose: a deep-green, celtic-arthurian counterpart to
/// `frontier_desc` / `lakes_desc`. Hits the >=180-char multi-sentence bar and
/// varies by the cell index so no two rooms read alike.
fn broceliande_desc(adj: &str, green: &str, feature: &str, creature: &str, idx: u32) -> String {
    const TERRAIN: [&str; 5] = [
        "You push through a {adj} stretch of {green}, where the light comes down in green coins through the canopy and the leaf-mould is soft and silent underfoot.",
        "The ride winds on beneath {adj} boughs, {green} closing green on every side until the wood itself seems to lean in and listen to your passing.",
        "Here the forest opens a little; {adj} {green} gives way to a hush of moss and fern, and old magic hangs in the still air like held breath.",
        "A deer-track threads this {adj} tangle of {green}, hoofprints in the black earth and the smell of sap and rot and slow green growing.",
        "The way climbs a {adj} bank of {green}, roots for a stair, and the whole wood breathes around you with a patience older than any kingdom.",
    ];
    const FEATURE: [&str; 5] = [
        "Ahead through the green stands {feature}, half-lost in leaf and shadow.",
        "Off among the trunks rises {feature}, a landmark the old woodwards steered by.",
        "The forest has all but swallowed {feature}, and the sight of it stops the breath.",
        "Beside the track waits {feature}, softened by moss and the long patient work of the wood.",
        "Through a gap in the briar you glimpse {feature}, green and still and older than the roads of men.",
    ];
    const ATMOS: [&str; 5] = [
        "Somewhere back in the deep green {creature} move unseen, and the wood watches you with a thousand quiet eyes.",
        "The undergrowth is thick with life; {creature} slip through the fern, and something far older stirs at the edge of hearing.",
        "It is a place of old power - {creature} keep their distance, and the standing stones of the druids are never truly far.",
        "A wood-pigeon claps up from the canopy, {creature} go still to watch you pass, and the green closes again behind your heels.",
        "The only sound is the drip of green water and {creature} far off, and the enchanted hush lies heavy on the traveller's heart.",
    ];
    let i = idx as usize;
    let t = TERRAIN[i % 5]
        .replace("{adj}", adj)
        .replace("{green}", green);
    let f = FEATURE[(i / 5) % 5].replace("{feature}", feature);
    let a = ATMOS[(i / 7 + i) % 5].replace("{creature}", creature);
    format!("{t} {f} {a}")
}

/// A small drop table for a Broceliande zone: representative gear from the
/// generated Frontier catalog for a matching tier, so a slain Greenwood
/// notable/mob yields real loot that resolves through `item`. Broceliande has
/// no gear catalog of its own; the reward here is the taming (see `taming.rs`).
fn broceliande_loot(z: usize) -> &'static [u32] {
    // Map the twenty zones onto a modest slice of the Frontier tiers so the wood
    // gives useful mid gear that rises with depth, without a bespoke catalog.
    let tier = (z / 2).min(super::items::FRONTIER_TIERS - 1);
    super::items::frontier_loot(tier)
}

/// A Greenwood notable's loot: the borrowed Frontier tier plus Broceliande's
/// own two uniquely named Wildbound finds for that zone.
fn broceliande_notable_loot(z: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        (0..BROCELIANDE_ZONES)
            .map(|zone| {
                let tier = (zone / 2).min(super::items::FRONTIER_TIERS - 1);
                let mut v = super::items::frontier_loot(tier).to_vec();
                v.extend(super::items::broceliande_find_ids(zone));
                v
            })
            .collect()
    });
    tables[z.min(BROCELIANDE_ZONES - 1)].as_slice()
}

/// Build Broceliande: twenty zones of braided briar-mazes and organic
/// fern-caverns (rooms 22000+), each carved (never a grid), chained
/// deepest-room -> next-entrance, and hung off the Verdant Highlands (the Faerie
/// Hollow) by a normal walk. A moderate green continent - the home of the fifty
/// tameable beasts, whose roaming spots are seeded in `taming.rs`.
#[allow(clippy::needless_range_loop, clippy::type_complexity)]
fn extend_broceliande(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (BROCELIANDE_W, BROCELIANDE_H);
    let n = w * h;
    let mut spawn_id: u32 = BROCELIANDE_SPAWN_ID_START;
    let mut prev_exit: Option<RoomId> = None;

    for (z, &(zname, adj, green, feature, creature, mob_names, boss)) in
        BROCELIANDE_ZONES_DATA.iter().enumerate()
    {
        let zbase = BROCELIANDE_BASE + (z as u32) * BROCELIANDE_ZONE_STRIDE;
        // A moderate power band that rises across the zones: above the peaceful
        // lakes, below the endgame continents. The deep jungle and the World-Oak
        // crown are a real challenge without being Kaelmyr.
        let tier = z as i32;
        let mut rng =
            MazeRng::new(BROCELIANDE_SEED ^ (z as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

        // Carve as a braided briar-maze or an organic fern-cavern (with the
        // connectivity pass). A too-sparse cavern falls back to a maze so no
        // zone comes out empty. No uniform grids.
        let cavern_floor = if broceliande_zone_is_cavern(z) {
            let floor = carve_cavern(w, h, &mut rng);
            (floor.iter().filter(|f| **f).count() >= 30).then_some(floor)
        } else {
            None
        };
        let (entrance, reachable, dist, cell_exits): (
            usize,
            Vec<bool>,
            Vec<usize>,
            Vec<Vec<(Dir, usize)>>,
        ) = if let Some(floor) = cavern_floor {
            let entrance = (0..n).find(|&i| floor[i]).unwrap_or(0);
            let dist = cavern_distances(&floor, w, h, entrance);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    let (x, y) = (c % w, c / w);
                    let consider = |nx: i64, ny: i64, d: Dir, v: &mut Vec<(Dir, usize)>| {
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let nb = ny as usize * w + nx as usize;
                            if reachable[nb] {
                                v.push((d, nb));
                            }
                        }
                    };
                    consider(x as i64, y as i64 - 1, Dir::North, &mut v);
                    consider(x as i64 + 1, y as i64, Dir::East, &mut v);
                    consider(x as i64, y as i64 + 1, Dir::South, &mut v);
                    consider(x as i64 - 1, y as i64, Dir::West, &mut v);
                    v
                })
                .collect();
            (entrance, reachable, dist, exits)
        } else {
            let open = carve_maze(w, h, &mut rng);
            let dist = maze_distances(&open, w, h, 0);
            let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
            let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                .map(|c| {
                    let mut v = Vec::new();
                    if !reachable[c] {
                        return v;
                    }
                    for d in 0..4 {
                        if open[c][d]
                            && let Some(nb) = maze_neighbor(c, d, w, h)
                        {
                            v.push((DIRS[d], nb));
                        }
                    }
                    v
                })
                .collect();
            (0, reachable, dist, exits)
        };

        // The zone's notable waits in the cell farthest from the entrance.
        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let zone: &'static str = Box::leak(zname.to_string().into_boxed_str());

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = zbase + cell as u32;
            let is_entrance = cell == entrance;
            let is_boss = cell == deepest && cell != entrance;
            let degree = cell_exits[cell].len();

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, zbase + *nb as u32))
                .collect();

            let name: &'static str = if is_entrance {
                Box::leak(format!("{zname} - the Forest Gate").into_boxed_str())
            } else if is_boss {
                Box::leak(format!("{zname} - the Green Heart").into_boxed_str())
            } else {
                Box::leak(format!("{zname} - {}", BROCELIANDE_PLACES[cell % 10]).into_boxed_str())
            };
            let desc: &'static str = Box::leak(
                broceliande_desc(adj, green, feature, creature, cell as u32).into_boxed_str(),
            );

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone,
                    // Every zone's entrance gate is a safe green haven, so the
                    // wood reads as a chain of woodward-holts between the deeps.
                    safe: is_entrance,
                    pvp: false,
                    exits,
                },
            );

            if is_entrance {
                continue;
            }

            let depth = dist[cell] as i32;
            // Behaviour-driven foes by maze-role: dead-ends ambush, junctions
            // swarm as packs, corridors patrol/skirmish. Moderate density - the
            // wood is alive but not wall-to-wall like the endgame.
            let (mob_name, behavior, boss_mob, hp, dmg): (&str, MobBehavior, bool, i32, i32) =
                if is_boss {
                    (
                        boss,
                        MobBehavior::Brute,
                        true,
                        600 + tier * 110,
                        30 + tier * 4,
                    )
                } else if degree == 1 {
                    if rng.chance(35) {
                        continue;
                    }
                    (
                        mob_names[0],
                        MobBehavior::Ambusher,
                        false,
                        200 + tier * 30 + depth * 4,
                        16 + tier + depth / 2,
                    )
                } else if degree >= 3 {
                    if rng.chance(35) {
                        continue;
                    }
                    (
                        mob_names[1],
                        MobBehavior::PackHunter,
                        false,
                        210 + tier * 32 + depth * 4,
                        17 + tier + depth / 2,
                    )
                } else {
                    if rng.chance(55) {
                        continue;
                    }
                    let behavior = match rng.below(3) {
                        0 => MobBehavior::Wanderer,
                        1 => MobBehavior::Patroller,
                        _ => MobBehavior::Skirmisher,
                    };
                    (
                        mob_names[2],
                        behavior,
                        false,
                        200 + tier * 30 + depth * 4,
                        15 + tier + depth / 2,
                    )
                };
            // The boss wears the zone's weakness but never its resist:
            // prep is a pure reward on the fight players provision for.
            let theme = BROCELIANDE_ZONE_THEMES[z];
            let profile = if boss_mob {
                DamageProfile::new(DamageType::Physical, None, theme.weak())
            } else {
                DamageProfile::new(DamageType::Physical, theme.resist(), theme.weak())
            };
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: hp,
                damage: dmg,
                xp: if boss_mob {
                    160 + tier * 34
                } else {
                    36 + tier * 9 + depth * 2
                },
                respawn_secs: if boss_mob { 260 } else { 62 },
                loot: if boss_mob {
                    broceliande_notable_loot(z)
                } else {
                    broceliande_loot(z)
                },
                boss: boss_mob,
                profile,
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }

        // Chain this zone to the previous one: the prior green-heart room
        // descends to this zone's forest gate, and rises back.
        let entrance_id = zbase + entrance as u32;
        if let Some(prev) = prev_exit {
            if let Some(r) = rooms.get_mut(&prev) {
                r.exits.insert(Dir::Down, entrance_id);
            }
            if let Some(r) = rooms.get_mut(&entrance_id) {
                r.exits.insert(Dir::Up, prev);
            }
        }
        prev_exit = Some(zbase + deepest as u32);
    }

    // Hang Broceliande off the Verdant Highlands (the Faerie Hollow, room 688)
    // by a normal walk exit, so the whole continent is reachable. The first
    // forest gate (Woodward's Holt) is a safe haven. Lightly gated - a green
    // country meant to be entered and explored.
    const BROCELIANDE_GATEWAY: RoomId = 688;
    let entrance = BROCELIANDE_BASE;
    let anchor = if rooms.contains_key(&BROCELIANDE_GATEWAY) {
        BROCELIANDE_GATEWAY
    } else {
        // Fall back to a real Verdant Highlands room if the hollow moved.
        rooms
            .keys()
            .copied()
            .find(|id| (680..692).contains(id))
            .unwrap_or(MELVANALA_SQUARE)
    };
    let portal = [Dir::North, Dir::South, Dir::East, Dir::West, Dir::Down]
        .into_iter()
        .find(|d| rooms.get(&anchor).is_some_and(|r| !r.exits.contains_key(d)))
        .unwrap_or(Dir::North);
    if let Some(hub) = rooms.get_mut(&anchor) {
        hub.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), anchor);
    }
}

// ---- Aelunor, the Faewood: a sprawling elven/fae forest (rooms 25000+) ----
//
// Twelve zones of organic, sprawling clearings - never a maze, never a grid
// (see `carve_cavern`; every single zone here is cavern-carved, deliberately
// unlike Broceliande's maze/cavern mix, so Aelunor always reads as glades and
// dells you wander between rather than corridors you solve). Home to the
// elves, high elves, druids, and fae of Lateania: some friendly (the
// villagers at every zone gate and the city below), most hostile (the
// hundred-creature roster below). Chained deepest-glade -> next-gate exactly
// like Broceliande, and hung off the Amber Savanna's terminal room by a
// normal walk east.

pub const AELUNOR_BASE: RoomId = 25_000;
const AELUNOR_W: usize = 9;
const AELUNOR_H: usize = 8;
const AELUNOR_ZONES: usize = AELUNOR_ZONES_DATA.len();
/// A fresh spawn-id band clear of every other region (Frontier/Reaches/
/// Kaelmyr/Lakes/Broceliande all sit in 900,000..1,000,000; Wildbound sits at
/// 1,500,000+). Falls into `tune_spawn_balance`'s default "gentle overworld"
/// bucket exactly like Wildbound does, since it matches none of the named
/// endgame bands - no special-casing needed.
const AELUNOR_SPAWN_ID_START: u32 = 1_600_000;
const AELUNOR_SEED: u64 = 0xAE1A_7702_u64;
/// Each zone reserves this many room ids (an `AELUNOR_W`x`AELUNOR_H` cell
/// field). Public so `taming.rs` can place the five Aelunor companions.
pub const AELUNOR_ZONE_STRIDE: u32 = (AELUNOR_W * AELUNOR_H) as u32;
pub const AELUNOR_ZONE_COUNT: usize = AELUNOR_ZONES;

pub fn is_aelunor_room(id: RoomId) -> bool {
    (AELUNOR_BASE..AELUNOR_BASE + AELUNOR_ZONES as u32 * AELUNOR_ZONE_STRIDE).contains(&id)
}

/// The five rarity tiers a regular Aelunor spawn can roll, from common
/// undergrowth to a once-in-a-visit find. Deliberately the same five words
/// `items::Rarity` already uses, so "this is the rarity system" reads as
/// literal, not just flavour - a Legendary spawn drops from a meaningfully
/// better loot tier than a Common one of the same base creature.
const AELUNOR_RARITY: [&str; 5] = ["", "Uncommon", "Rare", "Epic", "Legendary"];

/// The twenty base creatures of Aelunor's hostile roster, crossed with
/// `AELUNOR_RARITY` for a hundred named variants total (the same
/// base-name x affix-ladder shape already proven at Wildbound's 20x5 pool -
/// see `WILDBOUND_TIER_AFFIX`). Elves, high elves, druids, and fae gone
/// hostile: raiders, renegades, and things that were never on anyone's side.
const AELUNOR_CREATURES: [&str; 20] = [
    "Hollow-Elf Raider",
    "Grey Elf Outrider",
    "Faerie Trickster",
    "Wild Druid",
    "Thornbound Satyr",
    "Moss-Cloaked Stalker",
    "Pixie Swarm",
    "Bramble Warden",
    "Nightshade Nymph",
    "Dryad Handmaiden",
    "Faeling Marauder",
    "High Elf Renegade",
    "Thistlewitch Acolyte",
    "Antlered Stag-Knight",
    "Sylvan Revenant",
    "Gloomfae Assassin",
    "Wychwood Treant-Kin",
    "Starlit Mystic",
    "Feral Green Knight",
    "Wild Hunt Rider",
];

/// The world resist/weak pass (spec: CONTEXT.md, same-named section): one theme per
/// Aelunor glade, in `AELUNOR_ZONES_DATA` order. Regulars inherit the theme's
/// profile whatever affix they roll (the affix buys stats and loot, not a
/// different school game); the glade bosses keep their own authored profile
/// (Shadow, resisting Physical, weak to Holy - the region's own school game).
const AELUNOR_ZONE_THEMES: [ZoneTheme; AELUNOR_ZONES] = [
    ZoneTheme::Verdant,   // Silverleaf Eaves
    ZoneTheme::Haunted,   // the Whispering Boughs
    ZoneTheme::Verdant,   // Mossheart Glade
    ZoneTheme::Fae,       // the Sunfall Canopy
    ZoneTheme::Beastwild, // Thistledown Hollow
    ZoneTheme::Resonant,  // the Elder Ring
    ZoneTheme::Fae,       // Duskpetal Grove
    ZoneTheme::Tidal,     // the Starlit Fen
    ZoneTheme::Fungal,    // Wychroot Deeps
    ZoneTheme::Fae,       // the Faerie Loom
    ZoneTheme::Tidal,     // Moonwell Thicket
    ZoneTheme::Resonant,  // the Heartwood Sanctum
];

/// Twelve zones: (name, adjective, greenery noun, a landmark feature, the
/// creatures that haunt it, three "native" indices into `AELUNOR_CREATURES`
/// this zone favours, the zone's own named boss). Chained gate to gate, the
/// same shape as `BROCELIANDE_ZONES_DATA`. Zone names must NOT start with
/// "The " (the builder does not prepend it).
const AELUNOR_ZONES_DATA: [GladeData; 12] = [
    (
        "Silverleaf Eaves",
        "sun-dappled",
        "silver-barked birch",
        "a woven archway of living willow that never stops growing",
        "eaves-wardens",
        [0, 1, 6],
        "the Hollow-Elf Warlord",
    ),
    (
        "the Whispering Boughs",
        "wind-stirred",
        "tall whispering pine",
        "a ring of standing-stones humming faintly on the breeze",
        "bough-stalkers",
        [1, 2, 8],
        "Thistlewitch, the Bramble Queen",
    ),
    (
        "Mossheart Glade",
        "moss-thick",
        "moss-cloaked old oak",
        "a sunken hollow where the moss grows waist-deep and warm",
        "moss-kin",
        [5, 9, 16],
        "the Moss-Cloaked Ancient",
    ),
    (
        "the Sunfall Canopy",
        "gold-lit",
        "high sunfall canopy",
        "a broken shaft of light falling clean through the leaves onto an old altar",
        "canopy-runners",
        [3, 13, 19],
        "the Erlking's Huntsman",
    ),
    (
        "Thistledown Hollow",
        "thistle-choked",
        "wild thistledown bramble",
        "a drift of pale down that never quite settles",
        "hollow-fae",
        [2, 8, 12],
        "the Nightshade Nymph-Queen",
    ),
    (
        "the Elder Ring",
        "ring-marked",
        "an old fae-ring of toadstool and grass",
        "a perfect green circle the grass will not grow inside",
        "ring-wardens",
        [4, 7, 9],
        "the Ringmother of the Elder Circle",
    ),
    (
        "Duskpetal Grove",
        "dusk-shadowed",
        "dusk-petal blossom",
        "a grove of trees that only flower after dark",
        "duskpetal stalkers",
        [15, 16, 6],
        "the Gloomfae Reaper",
    ),
    (
        "the Starlit Fen",
        "star-mirrored",
        "reed and starlit water",
        "a still black mere that mirrors the sky too perfectly",
        "fen-wisps",
        [17, 8, 2],
        "the Starlit Seer-Queen",
    ),
    (
        "Wychroot Deeps",
        "root-choked",
        "gnarled wychroot",
        "a tangle of roots thick enough to walk on",
        "root-things",
        [16, 14, 5],
        "the Wychroot Revenant-Lord",
    ),
    (
        "the Faerie Loom",
        "thread-hung",
        "silver gossamer",
        "strands of cobweb-silk strung between the trees like a vast loom",
        "loom-fae",
        [2, 10, 6],
        "the Faerie Loomweaver",
    ),
    (
        "Moonwell Thicket",
        "moon-silvered",
        "pale moonwell birch",
        "a spring that only ever reflects the moon, whatever the hour",
        "moonwell wardens",
        [9, 17, 11],
        "the Moonwell Warden",
    ),
    (
        "the Heartwood Sanctum",
        "ancient",
        "the Heartwood itself, oldest tree in Aelunor",
        "the vast, living Heartwood, roots sunk to the world's own bones",
        "heartwood guardians",
        [11, 18, 19],
        "the Erlqueen, Heart of Aelunor",
    ),
];

/// Twelve places, one per zone, cycled by cell like `BROCELIANDE_PLACES`.
const AELUNOR_PLACES: [&str; 10] = [
    "the Glade Path",
    "a Sun-Break",
    "the Root Hollow",
    "a Fae Circle",
    "the Bramble Turn",
    "a Mossy Rise",
    "the Stillwater",
    "a Thicket Bend",
    "the Old Way",
    "a Quiet Dell",
];

/// Aelunor's regular-spawn loot: borrows the Frontier catalog exactly like
/// `broceliande_loot`. Depth is a **shallow** ladder (half a tier per zone,
/// the same slope Broceliande walks), and the rolled rarity is where the
/// reward actually lives - each affix step is worth three zones of depth, so
/// a Legendary spawn drops from a table a continent above its neighbours'.
/// This is the literal mechanism behind "different rarity, different drops",
/// and it is what makes the wood a lottery rather than a shortcut: the
/// jackpot is real (a Deep Heart Legendary reaches the catalog's Legendary
/// band) but you cannot farm it, because the affix is a rare roll at every
/// depth (see the rarity roll in `extend_aelunor`).
///
/// It must stay that way. Aelunor is entered by a plain walk off the Amber
/// Savanna with no title gate, and its mobs keep the gentle overworld
/// multipliers, so a *reliable* high tier here would hand out at ~660hp what
/// the Frontier guards at ~3280hp behind four Bane titles.
fn aelunor_loot(zone: usize, rarity: usize) -> &'static [u32] {
    let tier = (zone / 2 + rarity * 3).min(super::items::FRONTIER_TIERS - 1);
    super::items::frontier_loot(tier)
}

/// A named zone boss always drops, so it pays as though it were an Epic
/// spawn: the best table the wood offers reliably, still one affix step below
/// the Legendary roll that only luck produces.
fn aelunor_notable_loot(zone: usize) -> &'static [u32] {
    aelunor_loot(zone, 3)
}

/// Carve zone `z`'s glade floor. A pure function of the zone index (same
/// seed formula every call), factored out so the entrance a beast/city is
/// placed at (computed by external code, before or after `extend_aelunor`
/// runs) can never drift from the one `extend_aelunor` actually builds rooms
/// for. A too-sparse roll is re-rolled with a different stream rather than
/// falling back to a maze, so the "no maze here" promise never slips.
fn aelunor_carve_floor(z: usize) -> Vec<bool> {
    let (w, h) = (AELUNOR_W, AELUNOR_H);
    let mut rng = MazeRng::new(AELUNOR_SEED ^ (z as u64).wrapping_mul(0xA5A5_1234_5678_9ABCu64));
    let mut attempt = carve_cavern(w, h, &mut rng);
    let mut tries = 0;
    while attempt.iter().filter(|f| **f).count() < 24 && tries < 6 {
        attempt = carve_cavern(w, h, &mut rng);
        tries += 1;
    }
    attempt
}

/// Every zone's entrance room id (the "Wood-Gate"), the one cell every zone
/// is guaranteed to have reachable and safe. **Never assume offset 0 is the
/// entrance here** the way `taming::wild_beasts` does for Broceliande's
/// maze zones (where the maze carver's DFS always starts at cell 0): every
/// Aelunor zone is cavern-carved, and `carve_cavern` forces the whole grid
/// border - including cell 0 - to solid rock, so offset 0 is never even a
/// room. Computed once and cached.
pub(super) fn aelunor_entrances() -> &'static [RoomId] {
    static ENTRANCES: OnceLock<Vec<RoomId>> = OnceLock::new();
    ENTRANCES.get_or_init(|| {
        let n = AELUNOR_W * AELUNOR_H;
        (0..AELUNOR_ZONES)
            .map(|z| {
                let floor = aelunor_carve_floor(z);
                let cell = (0..n).find(|&i| floor[i]).unwrap_or(0);
                AELUNOR_BASE + z as u32 * AELUNOR_ZONE_STRIDE + cell as u32
            })
            .collect()
    })
}

/// Build Aelunor: twelve zones of organic forest glade (rooms 25000+), every
/// one cavern-carved (never a maze, never a grid), chained deepest-glade ->
/// next-gate, and hung off the Amber Savanna's terminal room. A moderate
/// green country, home of the hundred-creature Aelunor roster and the five
/// Aelunor companions (seeded in `taming.rs`), plus its own city, Silvael
/// (`extend_silvael`).
#[allow(clippy::needless_range_loop)]
fn extend_aelunor(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let (w, h) = (AELUNOR_W, AELUNOR_H);
    let n = w * h;
    let mut spawn_id: u32 = AELUNOR_SPAWN_ID_START;
    let mut prev_exit: Option<RoomId> = None;

    for (z, &(zname, adj, green, feature, creature, native, boss)) in
        AELUNOR_ZONES_DATA.iter().enumerate()
    {
        let zbase = AELUNOR_BASE + (z as u32) * AELUNOR_ZONE_STRIDE;
        // A separate stream from the carve's own rng (that one is fully
        // encapsulated in `aelunor_carve_floor` now), used only for mob
        // placement/rarity rolls below.
        let mut rng = MazeRng::new(
            AELUNOR_SEED.wrapping_mul(0xD1CE_u64) ^ (z as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );

        // Always an organic cavern glade - never a maze, never a grid. Uses
        // the same carve as `aelunor_entrances`, so the two can never
        // disagree about which cell is the entrance.
        let floor = aelunor_carve_floor(z);
        let entrance = (0..n).find(|&i| floor[i]).unwrap_or(0);
        let dist = cavern_distances(&floor, w, h, entrance);
        let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
        let cell_exits: Vec<Vec<(Dir, usize)>> = (0..n)
            .map(|c| {
                let mut v = Vec::new();
                if !reachable[c] {
                    return v;
                }
                let (x, y) = (c % w, c / w);
                let consider = |nx: i64, ny: i64, d: Dir, v: &mut Vec<(Dir, usize)>| {
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                        let nb = ny as usize * w + nx as usize;
                        if reachable[nb] {
                            v.push((d, nb));
                        }
                    }
                };
                consider(x as i64, y as i64 - 1, Dir::North, &mut v);
                consider(x as i64 + 1, y as i64, Dir::East, &mut v);
                consider(x as i64, y as i64 + 1, Dir::South, &mut v);
                consider(x as i64 - 1, y as i64, Dir::West, &mut v);
                v
            })
            .collect();

        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let zone: &'static str = Box::leak(zname.to_string().into_boxed_str());

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = zbase + cell as u32;
            let is_entrance = cell == entrance;
            let is_boss = cell == deepest && cell != entrance;

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, zbase + *nb as u32))
                .collect();

            let name: &'static str = if is_entrance {
                Box::leak(format!("{zname} - the Wood-Gate").into_boxed_str())
            } else if is_boss {
                Box::leak(format!("{zname} - the Deep Heart").into_boxed_str())
            } else {
                Box::leak(format!("{zname} - {}", AELUNOR_PLACES[cell % 10]).into_boxed_str())
            };
            let desc: &'static str = Box::leak(
                broceliande_desc(adj, green, feature, creature, cell as u32).into_boxed_str(),
            );

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone,
                    // Every zone's wood-gate is a safe haven, so Aelunor reads
                    // as a chain of gates between deepening wildwood.
                    safe: is_entrance,
                    pvp: false,
                    exits,
                },
            );

            if is_entrance {
                continue;
            }

            let depth = dist[cell] as i32;
            let tier = z as i32;
            if is_boss {
                let profile = DamageProfile::new(
                    DamageType::Shadow,
                    Some(DamageType::Physical),
                    Some(DamageType::Holy),
                );
                spawns.push(MobSpawn {
                    id: spawn_id,
                    name: boss,
                    home: id,
                    max_hp: 620 + tier * 120,
                    damage: 32 + tier * 4,
                    xp: 170 + tier * 36,
                    respawn_secs: 260,
                    loot: aelunor_notable_loot(z),
                    boss: true,
                    profile,
                });
                behaviors.insert(spawn_id, MobBehavior::Brute);
                spawn_id += 1;
                continue;
            }

            // Roughly a third of glade cells stay empty, so the wood breathes
            // rather than every clearing holding a fight.
            if rng.chance(34) {
                continue;
            }
            let base = AELUNOR_CREATURES[native[rng.below(3)]];
            // A lottery, not a depth ladder. The affix bands are fixed and
            // depth only nudges the roll, so a Legendary stays a rare find
            // wherever you are: ~1% at the eaves, ~5% in the Deep Heart.
            // A roll that climbed with depth instead (`below(20) + tier * 3`)
            // made the affix a second name for "how deep am I" - past zone 8
            // *every* spawn came up Legendary, and since the rarity picks the
            // drop table (`aelunor_loot`), that pointed a whole region of
            // ~660hp mobs at the Frontier catalog's top tier.
            let roll = rng.below(1000) as i32 + tier * 4;
            let rarity: usize = match roll {
                0..=549 => 0,
                550..=799 => 1,
                800..=929 => 2,
                930..=989 => 3,
                _ => 4,
            };
            let affix = AELUNOR_RARITY[rarity];
            let mob_name: &'static str = if affix.is_empty() {
                base
            } else {
                Box::leak(format!("{affix} {base}").into_boxed_str())
            };
            let behavior = match rng.below(3) {
                0 => MobBehavior::Wanderer,
                1 => MobBehavior::Skirmisher,
                _ => MobBehavior::Patroller,
            };
            // Now that the affix is a rare roll rather than a depth stamp, it
            // can buy a real fight instead of a slightly fatter common: the
            // premium is **quadratic** in the affix, so a Legendary spawn
            // lands at roughly twice its glade-mates' hp and reads as the
            // mini-boss it is. Deliberately flat across zones - the affix
            // jumps the drop table twelve tiers wherever it lands
            // (`aelunor_loot`), so the guard has to stand as far above the
            // local floor as the prize does, or a first-glade Legendary hands
            // a wanderer Epic-band gear off an ordinary fight.
            let elite = (rarity * rarity) as i32;
            let theme = AELUNOR_ZONE_THEMES[z];
            let profile = DamageProfile::new(DamageType::Physical, theme.resist(), theme.weak());
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: 190 + tier * 28 + depth * 4 + elite * 40,
                damage: 14 + tier + depth / 2 + elite * 3 / 2,
                xp: 32 + tier * 8 + depth * 2 + elite * 10,
                respawn_secs: 60,
                loot: aelunor_loot(z, rarity),
                boss: false,
                profile,
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }

        let entrance_id = zbase + entrance as u32;
        if let Some(prev) = prev_exit {
            if let Some(r) = rooms.get_mut(&prev) {
                r.exits.insert(Dir::Down, entrance_id);
            }
            if let Some(r) = rooms.get_mut(&entrance_id) {
                r.exits.insert(Dir::Up, prev);
            }
        }
        prev_exit = Some(zbase + deepest as u32);
    }

    // Hang Aelunor off the Amber Savanna's terminal room (its only free
    // direction: the wing chains east, so the last room never gained an east
    // neighbour) by a normal walk east. Lightly gated - a green country meant
    // to be entered and explored, same as Broceliande.
    let anchor = rooms
        .iter()
        .find(|(_, r)| r.name == "The Amber Savanna - The Pride's Reckoning")
        .map(|(&id, _)| id)
        .unwrap_or(MELVANALA_SQUARE);
    // Zone 0's real entrance, not `AELUNOR_BASE` (offset 0) - see
    // `aelunor_entrances`'s doc comment for why that would be a rock cell.
    let entrance = aelunor_entrances().first().copied().unwrap_or(AELUNOR_BASE);
    let portal = [Dir::East, Dir::North, Dir::South, Dir::West, Dir::Down]
        .into_iter()
        .find(|d| rooms.get(&anchor).is_some_and(|r| !r.exits.contains_key(d)))
        .unwrap_or(Dir::East);
    if let Some(hub) = rooms.get_mut(&anchor) {
        hub.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), anchor);
    }
}

/// Silvael, the Faewood's own city (rooms 26000+): a small, hand-wired haven
/// of elves, high elves, druids, and court fae. Every room here is safe -
/// "some friendly, some foe" plays out as the split between this city (the
/// friendly side) and the wood outside it, whose `AELUNOR_CREATURES` roster
/// reuses the same elf/druid/fae vocabulary for the hostile half.
pub const SILVAEL_BASE: RoomId = 26_000;
const SILVAEL_ROOM_COUNT: u32 = 8;

/// The direction a room should try next when chaining a fresh room onto it:
/// its first exit-free compass direction. Lets Silvael's inner wiring stay
/// correct no matter which direction `extend_aelunor` happened to splice the
/// city's own gate onto.
fn first_free_dir(rooms: &HashMap<RoomId, Room>, at: RoomId) -> Dir {
    [
        Dir::North,
        Dir::East,
        Dir::South,
        Dir::West,
        Dir::Up,
        Dir::Down,
    ]
    .into_iter()
    .find(|d| rooms.get(&at).is_some_and(|r| !r.exits.contains_key(d)))
    .unwrap_or(Dir::North)
}

/// Build Silvael and splice it onto the seam `extend_aelunor` used to hang
/// the wood off the Amber Savanna. That earlier splice walked the overworld
/// straight into the Faewood's first zone; this reopens that same link as
/// anchor -> Silvael's square -> the Wildwood Gate -> the wood, so the city
/// sits exactly where its story says it does: the threshold between the
/// King's roads and the Faewood proper. Never assumes the splice direction
/// was East - it re-derives it by finding whichever room actually links to
/// Aelunor's first zone entrance.
fn extend_silvael(rooms: &mut HashMap<RoomId, Room>) {
    let entrance = aelunor_entrances().first().copied().unwrap_or(AELUNOR_BASE);
    // Only the overworld side counts as the real anchor - the entrance cell
    // also has ordinary cavern-carved neighbours *within* Aelunor itself
    // (it's a normal reachable cell, not an island), and a search that
    // didn't exclude `is_aelunor_room` could match one of those instead,
    // depending on `HashMap` iteration order.
    let Some((anchor, dir)) = rooms.iter().find_map(|(&id, r)| {
        if is_aelunor_room(id) {
            return None;
        }
        r.exits
            .iter()
            .find(|&(_, &t)| t == entrance)
            .map(|(&d, _)| (id, d))
    }) else {
        return;
    };

    const ZONE: &str = "Silvael";
    let square = SILVAEL_BASE;
    let gate = SILVAEL_BASE + 1;
    let market = SILVAEL_BASE + 2;
    let larder = SILVAEL_BASE + 3;
    let moonwell = SILVAEL_BASE + 4;
    let circle = SILVAEL_BASE + 5;
    let terraces = SILVAEL_BASE + 6;
    let hollow = SILVAEL_BASE + 7;

    for (id, name, desc) in [
        (
            square,
            "Silvael - the Starlit Square",
            "Silvael rises straight out of the Faewood, with no wall to mark where \
             forest ends and city begins - only a ring of vast silver-barked trees \
             whose canopy has been coaxed, over centuries, into archways, stairs, \
             and whole hanging halls. Elf and high elf walk the square in equal \
             number, lantern-moths drift between the boughs where torches would \
             be anywhere else, and somewhere above a druid's low song keeps time \
             with the swaying leaves. The Wildwood breathes in cool and green from \
             one side of the square; a market, a moonwell, a stair of living wood, \
             and a quieter hollow open off the others.",
        ),
        (
            gate,
            "Silvael - the Wildwood Gate",
            "Silvael's living archways finally give out here, and the true Faewood \
             begins. The trees crowd closer, the lantern-moths thin to nothing, and \
             the last carved rail gives way to root and bramble underfoot. A pair \
             of high elf wardens keep this threshold, less to bar the way than to \
             mark it - nobody official has ever quite managed to say what waits \
             deeper in, only that it answers to older rules than the city's. The \
             square lies safe behind you.",
        ),
        (
            market,
            "Silvael - the Canopy Market",
            "Stalls hang from the branches on rope and pulley as often as they \
             stand on the ground, strung with pressed leaf-paper, woven charms, \
             and fae-work jewellery that shifts colour the moment nobody's looking \
             straight at it. Aelwen Songleaf, a high elf trader with a voice like \
             a struck bell, holds court at the finest stall and drives a harder \
             bargain than her smile suggests. Smaller vendors work the branches \
             above and below hers, trading in things that don't always translate \
             well to human coin.",
        ),
        (
            larder,
            "Silvael - the Green Larder",
            "A low, warm room built into the hollow of an ancient oak, its shelves \
             crowded with bundled herbs, jarred honey, and roots that smell of \
             nothing found outside the Faewood. Branwen Oakshadow, a druid with \
             moss for a beard, weighs out tinctures on a bone scale and never once \
             looks up from the work, though she always seems to know exactly who's \
             walked in. The Canopy Market lies back through the boughs.",
        ),
        (
            moonwell,
            "Silvael - the Moonwell",
            "A still, silver spring set into a hollow of root and stone, said to \
             reflect the moon whatever the actual hour above the canopy. Elves \
             kneel at its edge to wash the road from their faces, or simply to sit \
             and watch the water do something the sky above it isn't doing. The \
             old fae claim a wish spoken here on a true-dark night is heard, \
             though nobody in Silvael will confirm which nights those are. The \
             square lies close by.",
        ),
        (
            circle,
            "Silvael - the Druids' Circle",
            "A ring of standing stones stands here, worn smooth and hung with \
             willow-bark charms, where Silvael's druids keep their long watches \
             over the wood beyond the city. An elder druid tends the circle's low \
             fire without ever seeming to feed it, and the grass inside the ring \
             grows a shade greener than anywhere else in the city. The moonwell \
             glimmers back the way you came.",
        ),
        (
            terraces,
            "Silvael - the High Elm Terraces",
            "Tiered platforms climb the trunk of a single vast elm, linked by rope \
             bridges and stairs grown rather than built, where Silvael's high \
             elves keep their halls and their long, unhurried arguments about the \
             world beyond the wood. Shelves of bark-bound books line every \
             terrace, tended by an archivist who seems personally offended \
             whenever anyone actually asks to borrow one. The square lies below.",
        ),
        (
            hollow,
            "Silvael - the Beastkeeper's Hollow",
            "A quieter clearing behind the city proper, ringed with low dens and \
             roosts where a soft-spoken beastkeeper tends whatever the wood has \
             recently decided to trust to human hands. Bells and tame-charms hang \
             from every branch, and something with too many eyes watches you from \
             the shadows without ever quite showing itself. None of Silvael's fae \
             companions are sold here - the wood gives them, or it doesn't, same \
             as it always has. The square lies just beyond the trees.",
        ),
    ] {
        rooms.insert(id, room(id, name, ZONE, true, desc, &[]));
    }

    // Splice the city into the seam `extend_aelunor` used: overworld used to
    // walk straight from `anchor` into the wood; now it detours through
    // Silvael's square and its own Wildwood Gate first.
    if let Some(r) = rooms.get_mut(&anchor) {
        r.exits.insert(dir, square);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(dir.opposite(), gate);
    }
    if let Some(r) = rooms.get_mut(&square) {
        r.exits.insert(dir.opposite(), anchor);
        r.exits.insert(dir, gate);
    }
    if let Some(r) = rooms.get_mut(&gate) {
        r.exits.insert(dir.opposite(), square);
        r.exits.insert(dir, entrance);
    }

    // The square's remaining four compass directions (whichever they are)
    // fan out to the market, the moonwell, the terraces, and the hollow;
    // the market and the moonwell each chain one step further to the larder
    // and the circle.
    let spokes: Vec<Dir> = [
        Dir::North,
        Dir::East,
        Dir::South,
        Dir::West,
        Dir::Up,
        Dir::Down,
    ]
    .into_iter()
    .filter(|&d| d != dir && d != dir.opposite())
    .collect();
    link(rooms, square, spokes[0], market);
    link(rooms, square, spokes[1], moonwell);
    link(rooms, square, spokes[2], terraces);
    link(rooms, square, spokes[3], hollow);
    let d = first_free_dir(rooms, market);
    link(rooms, market, d, larder);
    let d = first_free_dir(rooms, moonwell);
    link(rooms, moonwell, d, circle);
}

// ---- The Wildbound Waste: a Felucca-style pvp continent (rooms 30000+) ----
//
// Three contested biomes - Duskmire Wood (forest), the Hollowdeep (dungeon),
// and the Scorched Flats (wasteland) - each a single large maze/cavern carve
// (never a uniform grid; see `carve_maze`/`carve_cavern`) whose regular mobs
// and one apex boss scale with BFS depth from the biome's edge. Three small
// safe towns, one gating each biome, are the only havens in the whole
// continent; every other room here is `pvp: true` (see `Room::pvp` and
// `svc::engage_player`) - adventurers can fight the mythical roster *or* each
// other. Chained gate -> field -> gate -> field -> gate -> field (deepening
// danger, same shape as Broceliande's zone chain) and hung off the Sahra
// Wastes' Sand-Wyrm's Maw (room 751) by a normal walk south.

pub const WILDBOUND_BASE: RoomId = 30_000;
const WILDBOUND_SPAWN_ID_START: u32 = 1_500_000;
const WILDBOUND_SEED: u64 = 0x5741_5354_4501_u64;
/// Room ids reserved per biome: four for the town plus the field carve. The
/// largest field (26x20 = 520 cells) starting at offset 10 leaves comfortable
/// headroom under this stride.
pub const WILDBOUND_BIOME_STRIDE: u32 = 700;
/// The Sahra Wastes' terminal room (see `extend_overworld`'s Sahra wing): its
/// `Dir::South` is never claimed there (the chain ends at this room), so the
/// Waste hangs off it cleanly without disturbing that wing.
const WILDBOUND_GATEWAY: RoomId = 751;

/// Whether `id` belongs to the Wildbound Waste (any of its three biomes,
/// gate town and contested field alike).
pub fn is_wildbound_room(id: RoomId) -> bool {
    (WILDBOUND_BASE..WILDBOUND_BASE + WILDBOUND_BIOMES.len() as u32 * WILDBOUND_BIOME_STRIDE)
        .contains(&id)
}

/// One biome's carved field, deterministic per biome index. Shared by
/// `extend_wildbound` (which builds the rooms from it) and the entrance table
/// below (which `wildbound_layout` reads), so the drawn gate town and the
/// gate's real exit can never disagree. `rng` is threaded through so a
/// caller's later draws see the same state the inline carve used to leave.
enum WildboundCarve {
    Cavern(Vec<bool>),
    Maze(Vec<Walls>),
}

fn wildbound_carve(b: usize, rng: &mut MazeRng) -> WildboundCarve {
    let biome = &WILDBOUND_BIOMES[b];
    let (w, h) = (biome.w, biome.h);
    if biome.cavern {
        let floor = carve_cavern(w, h, rng);
        if floor.iter().filter(|f| **f).count() >= 40 {
            return WildboundCarve::Cavern(floor);
        }
    }
    WildboundCarve::Maze(carve_maze(w, h, rng))
}

impl WildboundCarve {
    /// The field cell the biome's gate opens onto: the first floor cell in
    /// row-major order for a cavern, cell 0 for a maze. First-in-row-major
    /// is also what makes the town placement in `wildbound_layout`
    /// collision-free: every cell before the entrance is wall.
    fn entrance(&self) -> usize {
        match self {
            Self::Cavern(floor) => (0..floor.len()).find(|&i| floor[i]).unwrap_or(0),
            Self::Maze(_) => 0,
        }
    }
}

/// Entrance cell per biome, cached: `wildbound_layout` decodes ids in tight
/// loops and the carve behind the answer costs a full field each.
static WILDBOUND_ENTRANCES: LazyLock<[usize; 3]> = LazyLock::new(|| {
    std::array::from_fn(|b| {
        let mut rng = MazeRng::new(WILDBOUND_SEED ^ (b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        wildbound_carve(b, &mut rng).entrance()
    })
});

/// The five-tier power ladder shared by every biome's regular mobs, from the
/// biome's edge (Lesser) to its deep interior (Ancient) - one step short of
/// the biome's own named apex boss.
const WILDBOUND_TIER_AFFIX: [&str; 5] = ["Lesser", "", "Greater", "Elder", "Ancient"];

/// A closing clause appended to every contested-field room's description, so
/// the Waste reads as a distinct, dangerous place regardless of which prose
/// generator built the rest of the paragraph.
const WILDBOUND_PVP_NOTE: [&str; 4] = [
    " This deep in the Wildbound Waste no law but steel holds, and the next adventurer you meet may be foe as readily as friend.",
    " The old truce ends at the Waste's edge; here blade answers blade, and mercy is a coin few can afford to spend.",
    " No banner flies here to keep any peace - every stranger's hand may already be closing on a hilt.",
    " Word of the Waste travels slow and grim: those who enter contested ground and leave whole count themselves fortunate twice over.",
];

/// One of the Wildbound Waste's three biomes: everything needed to carve its
/// field, populate its mythical roster, and author its gate town.
struct WildboundBiome {
    zone: &'static str,
    w: usize,
    h: usize,
    /// True for an organic cellular-automata cavern; false for a braided maze.
    cavern: bool,
    adj: &'static str,
    ground: &'static str,
    feature: &'static str,
    creature_ambiance: &'static str,
    /// Which paragraph generator dresses this biome's rooms (see
    /// `broceliande_desc`/`frontier_desc`); both share this signature.
    desc_fn: fn(&str, &str, &str, &str, u32) -> String,
    places: [&'static str; 10],
    /// Twenty base creature names, crossed with `WILDBOUND_TIER_AFFIX`.
    creatures: [&'static str; 20],
    boss_name: &'static str,
    attack: DamageType,
    resist: Option<DamageType>,
    weak: Option<DamageType>,
    /// Pre-balance-scale (max_hp, damage) for each of the five tiers.
    tiers: [(i32, i32); 5],
    /// Pre-balance-scale (max_hp, damage) for the biome's apex boss.
    boss_stats: (i32, i32),
    /// Base offset into the Frontier loot catalog's twenty tiers (see
    /// `wildbound_loot`); each biome climbs five tiers from here.
    loot_base: usize,
    town_square_name: &'static str,
    town_square_desc: &'static str,
    town_shelter_name: &'static str,
    town_shelter_desc: &'static str,
    town_outfitter_name: &'static str,
    town_outfitter_desc: &'static str,
    town_gate_name: &'static str,
    town_gate_desc: &'static str,
}

const WILDBOUND_BIOMES: [WildboundBiome; 3] = [
    // ---- Duskmire Wood: a bramble-cavern forest, the shallow end of the
    // Waste. Levels run roughly 15-60, capped by its own apex.
    WildboundBiome {
        zone: "Duskmire Wood",
        w: 26,
        h: 20,
        cavern: true,
        adj: "bramble-choked",
        ground: "black thorn and rotting oak",
        feature: "a gallows-tree strung with old, swaying rope",
        creature_ambiance: "wraith-hounds",
        desc_fn: broceliande_desc,
        places: [
            "the Hanging Oak",
            "Widow's Clearing",
            "the Rot-Elm Stand",
            "Crowfoot Hollow",
            "the Gallows Path",
            "Blackthorn Break",
            "the Weeping Bower",
            "Ashleaf Corner",
            "the Sunken Grove",
            "Nightshade Row",
        ],
        creatures: [
            "Thornwolf",
            "Bramble Stalker",
            "Faehound",
            "Grovewisp",
            "Mosshide Troll",
            "Antlered Shade",
            "Weeping Wight",
            "Fen Harpy",
            "Bogsprite",
            "Nightjar Fury",
            "Elder Ent",
            "Duskmire Chimera",
            "Vinebound Horror",
            "Owlbear",
            "Marsh Basilisk",
            "Thicket Wraith",
            "Corpseflower Golem",
            "Stagheart Guardian",
            "Fungal Behemoth",
            "Wychelm Revenant",
        ],
        boss_name: "the Wychelm Sovereign",
        attack: DamageType::Poison,
        resist: Some(DamageType::Poison),
        weak: Some(DamageType::Fire),
        tiers: [(120, 10), (220, 16), (340, 22), (480, 28), (640, 34)],
        boss_stats: (1000, 45),
        loot_base: 0,
        town_square_name: "Last Watch - the Muster Square",
        town_square_desc: "Last Watch is less a town than a standing dare: a ring of timber palisade thrown up at the edge of civilised ground, where the King's law gives out and the Wildbound Waste begins. A muster bell hangs ready in a scorched frame at the square's heart, and the packed dirt underfoot is scuffed by boots that came back fewer than went out. Sellswords and the desperate share the fires here, sizing each other up as readily as any foe beyond the wall. A rough shelter stands west, a scavenger's outfitter east, and the log-gate south opens straight onto Duskmire Wood.",
        town_shelter_name: "Last Watch - the Ember Shelter",
        town_shelter_desc: "A long log hall serves Last Watch as barracks, infirmary, and the only truly safe place to close your eyes this side of the wall. Bedrolls line both walls, a banked fire smoulders in a stone pit, and someone has scratched a tally of names into a support beam, most crossed through. Nobody asks what happened to the others; everybody already knows. The square lies east.",
        town_outfitter_name: "Last Watch - the Scavenger's Stall",
        town_outfitter_desc: "A lean-to of salvaged planks and cannibalised cart-wheels serves as Last Watch's one trading post, its awning strung with grim trophies: fangs, claws, and stranger things pulled from the Wood. The scarred woman who runs it trades in whatever survivors carry out rather than coin most of the time, and she never asks where a fine ring came from. The square lies west.",
        town_gate_name: "Last Watch - the Log Gate",
        town_gate_desc: "The palisade breaks here for a gate of black, iron-bound logs, thrown wide day and night because nobody has ever needed to keep the Wood out - only to keep themselves in until they were ready. A watchman's brazier gutters overhead, more habit than help. Beyond the gate the bramble closes in at once, and the square lies safe behind you to the north.",
    },
    // ---- The Hollowdeep: a braided crypt-maze, the middle reach of the
    // Waste. Levels run roughly 40-70, capped by its own apex.
    WildboundBiome {
        zone: "the Hollowdeep",
        w: 22,
        h: 18,
        cavern: false,
        adj: "bone-choked",
        ground: "cracked ossuary tile",
        feature: "a rusted iron cage still holding a seated skeleton",
        creature_ambiance: "grave-wisps",
        desc_fn: frontier_desc,
        places: [
            "the Ossuary Vault",
            "Chain Landing",
            "the Weeping Wall",
            "Marrow Hall",
            "the Sealed Crypt",
            "Rust-Gate Corridor",
            "the Silent Choir",
            "Bonepile Junction",
            "the Drowned Stair",
            "Charnel Row",
        ],
        creatures: [
            "Hollow Wraith",
            "Barrow Lich",
            "Bone Chimera",
            "Crypt Gorgon",
            "Deepstalker",
            "Grave Hydra",
            "Sable Wyrmling",
            "Cinder Wisp",
            "Blackiron Golem",
            "Vault Cockatrice",
            "Manacled Horror",
            "Echo Banshee",
            "Tomb Basilisk",
            "the Warden of the Deep",
            "Shackled Behemoth",
            "Skeletal Manticore",
            "Voidtouched Revenant",
            "Gloomspawn",
            "Charnel Ooze",
            "Deathless Sentinel",
        ],
        boss_name: "the Deathless Warden",
        attack: DamageType::Shadow,
        resist: Some(DamageType::Shadow),
        weak: Some(DamageType::Holy),
        tiers: [(420, 26), (620, 34), (860, 42), (1140, 50), (1460, 58)],
        boss_stats: (2200, 68),
        loot_base: 7,
        town_square_name: "Barrowgate - the Sunken Square",
        town_square_desc: "Barrowgate is built into the mouth of the Hollowdeep itself, its houses sunk half into the hillside as though the crypt-country had already begun to claim them. The square is a bowl of packed grave-dirt around an old well nobody drinks from anymore, ringed by lean stone houses whose owners deal only with those who go below and, sometimes, come back. A shelter stands west, an outfitter east, and the crypt-gate south breathes cold air up from the Hollowdeep.",
        town_shelter_name: "Barrowgate - the Vigil House",
        town_shelter_desc: "Candles burn in every window of the Vigil House, day and night, kept lit by a standing rota of Barrowgate's residents against a dark that everyone agrees is closer here than it ought to be. Cots line the single long room, and a chalked board by the door lists names owed a vigil of their own. It is warm, close, and the one room in Barrowgate no one has ever reported hearing something knock from the other side of the wall. The square lies east.",
        town_outfitter_name: "Barrowgate - the Grave-Goods Exchange",
        town_outfitter_desc: "Shelves of reclaimed grave-goods line this narrow shop, sorted with a care that borders on reverence: rings, blades, and stranger relics pulled up from the Hollowdeep and cleaned of whatever they were buried in. The proprietor, a thin man who never quite meets your eyes, pays well and asks nothing. The square lies west.",
        town_gate_name: "Barrowgate - the Crypt Gate",
        town_gate_desc: "A stair of worn stone drops away here through a broken archway carved with names long since weathered unreadable, the last light of Barrowgate falling behind as the cold, grave-scented dark of the Hollowdeep rises to meet it. Nobody has ever bothered building an actual door. The square is safe behind you to the north.",
    },
    // ---- The Scorched Flats: a vast, sun-cracked wasteland cavern, the
    // Waste's deep end. Levels run roughly 65-100, ending at its own apex -
    // the single hardest fight in the Wildbound Waste.
    WildboundBiome {
        zone: "the Scorched Flats",
        w: 26,
        h: 20,
        cavern: true,
        adj: "sun-cracked",
        ground: "cracked white salt-pan",
        feature: "a colossus of fused black glass, half-sunk in the flat",
        creature_ambiance: "ash-wyrms",
        desc_fn: frontier_desc,
        places: [
            "the Salt Flat",
            "Cinder Row",
            "the Glass Crater",
            "Bonewhite Draw",
            "the Furnace Break",
            "Scorpion Wash",
            "the Blistered Reach",
            "Ember Gulch",
            "the Dust Maw",
            "Sunfall Ridge",
        ],
        creatures: [
            "Ashwyrm",
            "Cinderback Manticore",
            "Scorpion King",
            "Dune Wraith",
            "Emberhide Basilisk",
            "Bloodsand Harpy",
            "Withered Colossus",
            "Sunscorched Revenant",
            "Sandstorm Djinn",
            "Salt Golem",
            "Bleached Chimera",
            "Dust Behemoth",
            "Glasswing Wyvern",
            "Cracked-Earth Titan",
            "Locust Swarm-Lord",
            "Ashen Sphinx",
            "Marauder's Wraith",
            "Furnace Hound",
            "Scoured Gorgon",
            "the Cracked Sovereign",
        ],
        boss_name: "the Apex Sandwyrm",
        attack: DamageType::Fire,
        resist: Some(DamageType::Fire),
        weak: Some(DamageType::Frost),
        tiers: [(1200, 58), (1650, 68), (2150, 78), (2700, 88), (3300, 98)],
        boss_stats: (4200, 120),
        loot_base: 13,
        town_square_name: "Ashhold - the Scorched Square",
        town_square_desc: "Ashhold is a huddle of blackened stone at the true edge of the map, where the Wildbound Waste finally burns itself out into the Scorched Flats. Nothing grows here; the square is bare fused ground, and the folk who hold it - a harder breed than even Last Watch or Barrowgate turns out - trust nobody who hasn't already bled for the privilege. A shelter stands west, an outfitter east, and the ash-gate south is the last safe threshold before the Flats proper.",
        town_shelter_name: "Ashhold - the Cinder Hall",
        town_shelter_desc: "The Cinder Hall is dug half into the earth for the coolness of it, its low roof shored with salvaged black glass that catches what little light reaches this far into the Waste. Those who shelter here rarely talk about what drove them from wherever they started; the Flats have a way of erasing a person's history along with everything else. The square lies east.",
        town_outfitter_name: "Ashhold - the Glasswright's Stall",
        town_outfitter_desc: "A one-armed glasswright trades here in gear salvaged and reforged from whatever the Scorched Flats give up: fused-glass blades, ash-tempered armour, and trinkets pulled from things that used to be considerably larger and more dangerous. Prices are steep and non-negotiable, and the wares are, without exception, the genuine article. The square lies west.",
        town_gate_name: "Ashhold - the Ash Gate",
        town_gate_desc: "A last low arch of scorched stone marks where Ashhold ends and the true Scorched Flats begin, heat shimmering visibly through it even in the cold hours. No one has ever needed to be told twice what waits beyond. The square is safe behind you to the north.",
    },
];

/// The drop table for a Wildbound Waste tier: borrows the Frontier catalog
/// (which already spans early-endgame through the game's toughest numbers)
/// rather than authoring a bespoke item set, same shortcut `broceliande_loot`
/// takes.
///
/// Every table here is keyed to the biome's own `loot_base`, the apex boss
/// included: one affix ladder past its deepest regular, and never the
/// catalog's top tier (hence the `- 2` clamp, which holds however `loot_base`
/// is retuned later). The boss branch used to hand all three apexes
/// `FRONTIER_TIERS - 1`, which paid the ~1500hp Duskmire boss - walked to off
/// the Sahra Wastes, at gentle overworld multipliers, with no title anywhere
/// on the road, and dropping guaranteed (`svc::roll_loot` never rolls for a
/// boss) - exactly what the King Who Was Promised Nothing guards at ~11700hp
/// behind twenty Frontier zones and four Bane titles. The crown's table stays
/// the crown's.
fn wildbound_loot(loot_base: usize, tier: usize, boss: bool) -> &'static [u32] {
    let tier = if boss {
        loot_base + WILDBOUND_TIER_AFFIX.len()
    } else {
        loot_base + tier
    };
    super::items::frontier_loot(tier.min(super::items::FRONTIER_TIERS - 2))
}

/// Build the Wildbound Waste: three chained biomes (rooms 30000+), each a
/// carved contested field behind its own small safe town, hung off the Sahra
/// Wastes' Sand-Wyrm's Maw by a normal walk south.
#[allow(clippy::type_complexity)]
fn extend_wildbound(
    rooms: &mut HashMap<RoomId, Room>,
    spawns: &mut Vec<MobSpawn>,
    behaviors: &mut HashMap<u32, MobBehavior>,
) {
    let mut spawn_id: u32 = WILDBOUND_SPAWN_ID_START;
    // The chain's current tail: where the next town's square hangs. Starts at
    // the Waste's real-world gateway.
    let mut chain_from = WILDBOUND_GATEWAY;
    let mut chain_dir = Dir::South;

    for (b, biome) in WILDBOUND_BIOMES.iter().enumerate() {
        let base = WILDBOUND_BASE + (b as u32) * WILDBOUND_BIOME_STRIDE;
        let square_id = base;
        let shelter_id = base + 1;
        let outfitter_id = base + 2;
        let gate_id = base + 3;
        let field_base = base + 10;

        // --- The gate town: four small safe rooms, pvp: false throughout. ---
        rooms.insert(
            square_id,
            room(
                square_id,
                biome.town_square_name,
                biome.zone,
                true,
                biome.town_square_desc,
                &[
                    (Dir::West, shelter_id),
                    (Dir::East, outfitter_id),
                    (Dir::South, gate_id),
                ],
            ),
        );
        rooms.insert(
            shelter_id,
            room(
                shelter_id,
                biome.town_shelter_name,
                biome.zone,
                true,
                biome.town_shelter_desc,
                &[(Dir::East, square_id)],
            ),
        );
        rooms.insert(
            outfitter_id,
            room(
                outfitter_id,
                biome.town_outfitter_name,
                biome.zone,
                true,
                biome.town_outfitter_desc,
                &[(Dir::West, square_id)],
            ),
        );
        rooms.insert(
            gate_id,
            room(
                gate_id,
                biome.town_gate_name,
                biome.zone,
                true,
                biome.town_gate_desc,
                &[(Dir::North, square_id)],
            ),
        );
        link(rooms, chain_from, chain_dir, square_id);

        // --- Carve the contested field: a braided maze or an organic cavern
        // (with a density fallback to maze, exactly like Broceliande), never
        // a uniform grid. ---
        let (w, h) = (biome.w, biome.h);
        let n = w * h;
        let mut rng = MazeRng::new(WILDBOUND_SEED ^ (b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

        let carve = wildbound_carve(b, &mut rng);
        let entrance = carve.entrance();
        let (reachable, dist, cell_exits): (Vec<bool>, Vec<usize>, Vec<Vec<(Dir, usize)>>) =
            if let WildboundCarve::Cavern(floor) = &carve {
                let dist = cavern_distances(floor, w, h, entrance);
                let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
                let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                    .map(|c| {
                        let mut v = Vec::new();
                        if !reachable[c] {
                            return v;
                        }
                        let (x, y) = (c % w, c / w);
                        let consider = |nx: i64, ny: i64, d: Dir, v: &mut Vec<(Dir, usize)>| {
                            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                                let nb = ny as usize * w + nx as usize;
                                if reachable[nb] {
                                    v.push((d, nb));
                                }
                            }
                        };
                        consider(x as i64, y as i64 - 1, Dir::North, &mut v);
                        consider(x as i64 + 1, y as i64, Dir::East, &mut v);
                        consider(x as i64, y as i64 + 1, Dir::South, &mut v);
                        consider(x as i64 - 1, y as i64, Dir::West, &mut v);
                        v
                    })
                    .collect();
                (reachable, dist, exits)
            } else {
                let WildboundCarve::Maze(open) = &carve else {
                    unreachable!("a carve is either a cavern or a maze");
                };
                let dist = maze_distances(open, w, h, 0);
                let reachable: Vec<bool> = (0..n).map(|c| dist[c] != usize::MAX).collect();
                let exits: Vec<Vec<(Dir, usize)>> = (0..n)
                    .map(|c| {
                        let mut v = Vec::new();
                        if !reachable[c] {
                            return v;
                        }
                        for d in 0..4 {
                            if open[c][d]
                                && let Some(nb) = maze_neighbor(c, d, w, h)
                            {
                                v.push((DIRS[d], nb));
                            }
                        }
                        v
                    })
                    .collect();
                (reachable, dist, exits)
            };

        let deepest = (0..n)
            .filter(|&c| reachable[c])
            .max_by_key(|&c| dist[c])
            .unwrap_or(entrance);
        let max_depth = dist[deepest].max(1);

        for cell in 0..n {
            if !reachable[cell] {
                continue;
            }
            let id = field_base + cell as u32;
            let is_deepest = cell == deepest && cell != entrance;
            let degree = cell_exits[cell].len();

            let exits: HashMap<Dir, RoomId> = cell_exits[cell]
                .iter()
                .map(|(d, nb)| (*d, field_base + *nb as u32))
                .collect();

            let name: &'static str = if is_deepest {
                Box::leak(format!("{} - {}'s Lair", biome.zone, biome.boss_name).into_boxed_str())
            } else {
                Box::leak(format!("{} - {}", biome.zone, biome.places[cell % 10]).into_boxed_str())
            };
            let base_desc = (biome.desc_fn)(
                biome.adj,
                biome.ground,
                biome.feature,
                biome.creature_ambiance,
                cell as u32,
            );
            let desc: &'static str = Box::leak(
                format!(
                    "{base_desc}{}",
                    WILDBOUND_PVP_NOTE[cell % WILDBOUND_PVP_NOTE.len()]
                )
                .into_boxed_str(),
            );

            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone: biome.zone,
                    safe: false,
                    pvp: true,
                    exits,
                },
            );

            if cell == entrance {
                continue;
            }

            let depth = dist[cell];
            let tier = ((depth * 5) / max_depth).min(4);

            let (mob_name, hp, dmg, boss_mob): (&str, i32, i32, bool) = if is_deepest {
                (
                    biome.boss_name,
                    biome.boss_stats.0,
                    biome.boss_stats.1,
                    true,
                )
            } else if degree == 1 {
                if rng.chance(35) {
                    continue;
                }
                let (hp, dmg) = biome.tiers[tier];
                (
                    wildbound_named(biome.creatures[cell % 20], tier),
                    hp,
                    dmg,
                    false,
                )
            } else if degree >= 3 {
                if rng.chance(35) {
                    continue;
                }
                let (hp, dmg) = biome.tiers[tier];
                (
                    wildbound_named(biome.creatures[(cell + 7) % 20], tier),
                    hp,
                    dmg,
                    false,
                )
            } else {
                if rng.chance(55) {
                    continue;
                }
                let (hp, dmg) = biome.tiers[tier];
                (
                    wildbound_named(biome.creatures[(cell + 13) % 20], tier),
                    hp,
                    dmg,
                    false,
                )
            };
            let behavior = if boss_mob {
                MobBehavior::Brute
            } else if degree == 1 {
                MobBehavior::Ambusher
            } else if degree >= 3 {
                MobBehavior::PackHunter
            } else {
                match rng.below(3) {
                    0 => MobBehavior::Wanderer,
                    1 => MobBehavior::Patroller,
                    _ => MobBehavior::Skirmisher,
                }
            };
            let pre_power = hp + dmg * 4;
            spawns.push(MobSpawn {
                id: spawn_id,
                name: mob_name,
                home: id,
                max_hp: hp,
                damage: dmg,
                xp: if boss_mob {
                    pre_power / 3
                } else {
                    pre_power / 6
                },
                respawn_secs: if boss_mob { 300 } else { 55 },
                loot: wildbound_loot(biome.loot_base, tier, boss_mob),
                boss: boss_mob,
                profile: DamageProfile::new(biome.attack, biome.resist, biome.weak),
            });
            behaviors.insert(spawn_id, behavior);
            spawn_id += 1;
        }

        link(rooms, gate_id, Dir::South, field_base + entrance as u32);
        chain_from = field_base + deepest as u32;
        chain_dir = Dir::Down;
    }
}

/// Cross a base creature name with the shared tier ladder, e.g. tier 0
/// "Lesser Thornwolf", tier 1 (bare) "Thornwolf", tier 3 "Elder Thornwolf".
/// Leaked to `'static` once per call site, same as every other generated name
/// in this file (the world is built once at startup).
fn wildbound_named(creature: &str, tier: usize) -> &'static str {
    let affix = WILDBOUND_TIER_AFFIX[tier];
    Box::leak(
        if affix.is_empty() {
            creature.to_string()
        } else {
            format!("{affix} {creature}")
        }
        .into_boxed_str(),
    )
}

/// The world resist/weak pass (spec: CONTEXT.md, same-named section): one theme per
/// Frontier zone, in `FRONTIER_ZONES_DATA` order, derived from the zone's
/// flavor. Regulars inherit the theme's profile; the zone boss wears the theme's weakness but never its resist
/// (prep is a pure reward on the fight players provision for).
const FRONTIER_ZONE_THEMES: [ZoneTheme; FRONTIER_ZONES] = [
    ZoneTheme::Ashen,       // Ashen Wastes
    ZoneTheme::Tidal,       // Sunken Fens
    ZoneTheme::Fae,         // Glimmerwood
    ZoneTheme::Beastwild,   // Howling Steppe
    ZoneTheme::Sunscorched, // Cinder Barrens
    ZoneTheme::Tidal,       // Tideglass Coast
    ZoneTheme::Undead,      // Bonewhite Reach
    ZoneTheme::Haunted,     // Verdigris Ruins
    ZoneTheme::Storm,       // Stormspire Highlands
    ZoneTheme::Haunted,     // Umbral Depths
    ZoneTheme::Sunscorched, // Saltglass Desert
    ZoneTheme::Fungal,      // Fungal Hollow
    ZoneTheme::Crystal,     // Clockwork Ruins
    ZoneTheme::Beastwild,   // Bloodmarsh
    ZoneTheme::Resonant,    // Singing Canyon
    ZoneTheme::Frozen,      // Frostfang Tundra
    ZoneTheme::Crystal,     // Obsidian Flats
    ZoneTheme::Haunted,     // Driftbone Sea
    ZoneTheme::Sunscorched, // Emberfall Caldera
    ZoneTheme::Profane,     // Hollow Crown
];

/// Per-zone flavour: name, adjective, ground noun, a landmark feature, the
/// creatures that haunt it, three regular mob names, and the zone boss.
const FRONTIER_ZONES_DATA: [ZoneData; 20] = [
    (
        "Ashen Wastes",
        "ashen",
        "drifting cinders",
        "a toppled obelisk",
        "ash-wraiths",
        ["Cinder Jackal", "Ash Revenant", "Soot Brute"],
        "Pyremaw the Unquenched",
    ),
    (
        "Sunken Fens",
        "sodden",
        "black mire",
        "a drowned shrine",
        "fen-lurkers",
        ["Mire Crawler", "Bog Hag", "Drowned Thrall"],
        "Mother Mudgrim",
    ),
    (
        "Glimmerwood",
        "glimmering",
        "luminous moss",
        "a crystal-veined stump",
        "wisp-stalkers",
        ["Glimmer Moth", "Thornback Stag", "Lantern Shade"],
        "the Hollow King",
    ),
    (
        "Howling Steppe",
        "wind-scoured",
        "frost-burnt grass",
        "a leaning standing stone",
        "steppe-wolves",
        ["Gale Hound", "Steppe Reaver", "Frost Auroch"],
        "Skarn the Windbroken",
    ),
    (
        "Cinder Barrens",
        "blistered",
        "cracked slag",
        "a cold forge-chimney",
        "slag-born",
        ["Slag Hound", "Ember Golem", "Ash Marauder"],
        "Vulcaranth",
    ),
    (
        "Tideglass Coast",
        "salt-bitten",
        "ground shell and glass",
        "a half-sunk hull",
        "reef-stalkers",
        ["Brine Snapper", "Glasswing Gull", "Tide Revenant"],
        "the Drowned Captain",
    ),
    (
        "Bonewhite Reach",
        "bleached",
        "bone-dry chalk",
        "a colossal ribcage",
        "carrion-things",
        ["Chalk Crawler", "Bone Piper", "Marrow Fiend"],
        "Ossuary the Pale",
    ),
    (
        "Verdigris Ruins",
        "moss-eaten",
        "verdigris-stained flagstones",
        "a green-bronze colossus",
        "ruin-haunts",
        ["Patina Wraith", "Bronze Sentinel", "Vine Strangler"],
        "the Verdigris Warden",
    ),
    (
        "Stormspire Highlands",
        "thunder-struck",
        "shard-strewn scree",
        "a lightning-split spire",
        "storm-callers",
        ["Spark Roc", "Thunder Ram", "Storm Herald"],
        "Voltaryx",
    ),
    (
        "Umbral Depths",
        "lightless",
        "cold black stone",
        "a sealed vault door",
        "umbral horrors",
        ["Gloom Crawler", "Shadowmaw", "Void Acolyte"],
        "the Nameless Beneath",
    ),
    (
        "Saltglass Desert",
        "sun-cracked",
        "blinding white salt-flats",
        "a half-buried caravan",
        "glass-scorpions",
        ["Salt Wraith", "Mirage Stalker", "Dune Brute"],
        "Khepri the Sun-Drinker",
    ),
    (
        "Fungal Hollow",
        "spore-choked",
        "spongy mycelium",
        "a titan toadstool",
        "myconid swarms",
        ["Spore Hound", "Cap-Shrieker", "Rot Shambler"],
        "the Mycelial Mind",
    ),
    (
        "Clockwork Ruins",
        "rust-locked",
        "a cog-strewn floor",
        "a stalled great-engine",
        "clockwork sentinels",
        ["Cog Crawler", "Brass Automaton", "Spring-Loaded Horror"],
        "the Mainspring Tyrant",
    ),
    (
        "Bloodmarsh",
        "blood-warm",
        "iron-red bog",
        "a sunken altar",
        "leech-things",
        ["Bog Leech", "Crimson Stalker", "Bloodfly Swarm"],
        "the Sanguine Maw",
    ),
    (
        "Singing Canyon",
        "wind-carved",
        "ringing sandstone",
        "a wailing arch",
        "echo-hunters",
        ["Howl Bat", "Resonant Wraith", "Canyon Lurker"],
        "Diapason the Unending Note",
    ),
    (
        "Frostfang Tundra",
        "frost-locked",
        "blue-white permafrost",
        "a frozen mammoth",
        "ice-stalkers",
        ["Frost Wolf", "Rime Revenant", "Glacier Brute"],
        "Hoarfrost the Eternal Winter",
    ),
    (
        "Obsidian Flats",
        "glass-sharp",
        "black volcanic glass",
        "a shattered mirror-stair",
        "shardlings",
        ["Glass Hound", "Obsidian Wraith", "Razor Crawler"],
        "the Mirrorless King",
    ),
    (
        "Driftbone Sea",
        "wind-stripped",
        "dunes of grey driftbone",
        "a beached leviathan",
        "bone-pickers",
        ["Drift Crawler", "Marrow Gull", "Bone-Tide Revenant"],
        "the Ghost of Leviathan",
    ),
    (
        "Emberfall Caldera",
        "molten",
        "cooling lava-crust",
        "a sinking magma-temple",
        "flame-born",
        ["Magma Hound", "Ember Revenant", "Cinder Titan"],
        "Caldera the Heartfire",
    ),
    (
        "Hollow Crown",
        "god-haunted",
        "starless black marble",
        "the broken throne of a dead god",
        "crown-wights",
        ["Wight Sentinel", "Pale Regent", "Throne Shade"],
        "the King Who Was Promised Nothing",
    ),
];

/// Number of Frontier zones, and so the number of zone quests (slay each boss).
pub fn frontier_zone_count() -> usize {
    FRONTIER_ZONES_DATA.len()
}

/// The display name and boss name of Frontier zone `z`.
pub fn frontier_zone_info(z: usize) -> Option<(&'static str, &'static str)> {
    FRONTIER_ZONES_DATA.get(z).map(|d| (d.0, d.6))
}

/// The level Frontier zone `z` is pitched at: a straight line from the living
/// dark's exit (the three seals' crown level) to the deep target (the King's),
/// the two ends the generator is sloped between. Reward math (the zone-boss
/// bounty, the champion title) keys off this, never the level displayed over
/// the boss's head: that one reads by bite and moves with every retune of the
/// ladder, and a one-time payout must not.
pub fn frontier_zone_level(z: usize) -> i32 {
    let entry = crown_level("the Elder Dryad");
    let deep = crown_level("the King Who Was Promised Nothing");
    let last = (frontier_zone_count() - 1) as i32;
    entry + ((deep - entry) * z as i32) / last
}

/// The Frontier zone whose boss bears this name, if any, used to credit a
/// zone quest when its boss is slain.
pub fn frontier_zone_of_boss(name: &str) -> Option<usize> {
    FRONTIER_ZONES_DATA.iter().position(|d| d.6 == name)
}

const FRONTIER_PLACES: [&str; 10] = [
    "Approach",
    "Hollow",
    "Crossing",
    "Overlook",
    "Waymark",
    "Descent",
    "Reach",
    "Gauntlet",
    "Sanctum",
    "Threshold",
];

/// Compose a paragraph-length room description (>=180 chars, 3 sentences) from
/// per-zone flavour, varied by the cell index.
fn frontier_desc(adj: &str, ground: &str, feature: &str, creature: &str, idx: u32) -> String {
    const TERRAIN: [&str; 5] = [
        "The trail threads through {adj} country where {ground} shifts underfoot with every wary step.",
        "Broken ground rises and falls here, the {ground} pale and treacherous beneath a bruised sky.",
        "A cold wind scours this {adj} stretch, carrying grit that stings the eyes and rattles loose stone.",
        "The way narrows between leaning walls of rock, the {ground} drifted deep in the hollows.",
        "Open and exposed, this {adj} reach offers no shelter; the {ground} runs grey to the horizon.",
    ];
    const FEATURE: [&str; 5] = [
        "Nearby looms {feature}, weathered past recognition and half-claimed by the wilds.",
        "Off the path stands {feature}, a landmark for the few who pass this way and live.",
        "The bones of {feature} jut from the earth, older than any road that ever led here.",
        "Beside the trail rests {feature}, a silent witness to whatever fell upon this land.",
        "Through the murk you make out {feature}, leaning beneath the weight of long years.",
    ];
    const ATMOS: [&str; 5] = [
        "Somewhere out of sight {creature} call to one another, and the sound does not invite company.",
        "The air hangs heavy with menace, for {creature} have left their marks on stone and bark alike.",
        "Nothing moves but the wind, yet you sense {creature} watching from beyond the failing light.",
        "A foul reek drifts on the breeze; {creature} hunt these reaches, and they hunt well.",
        "A brittle quiet reigns, the quiet of a place from which {creature} have driven all else away.",
    ];
    let i = idx as usize;
    let t = TERRAIN[i % 5]
        .replace("{adj}", adj)
        .replace("{ground}", ground);
    let f = FEATURE[(i / 5) % 5].replace("{feature}", feature);
    let a = ATMOS[(i / 7 + i) % 5].replace("{creature}", creature);
    format!("{t} {f} {a}")
}

fn extend_frontier(rooms: &mut HashMap<RoomId, Room>, spawns: &mut Vec<MobSpawn>) {
    let per_zone = FRONTIER_W * FRONTIER_H;
    let mut spawn_id: u32 = FRONTIER_SPAWN_ID_START;

    // Pass 1: create every room and its mobs.
    for (z, &(zname, adj, ground, feature, creature, mob_names, boss)) in
        FRONTIER_ZONES_DATA.iter().enumerate()
    {
        let zbase = FRONTIER_BASE + (z as u32) * per_zone;
        let tier = z + 2; // the frontier sits beyond the base game's tiers
        for y in 0..FRONTIER_H {
            for x in 0..FRONTIER_W {
                let idx = y * FRONTIER_W + x;
                let id = zbase + idx;
                let is_entrance = z == 0 && idx == 0;
                let is_boss_room = idx == per_zone - 1;

                let zone: &'static str = Box::leak(format!("The {zname}").into_boxed_str());
                let name: &'static str = Box::leak(
                    format!("{zname} - {}", FRONTIER_PLACES[(idx as usize) % 10]).into_boxed_str(),
                );
                let desc: &'static str =
                    Box::leak(frontier_desc(adj, ground, feature, creature, idx).into_boxed_str());

                let mut exits: Vec<(Dir, RoomId)> = Vec::new();
                if x + 1 < FRONTIER_W {
                    exits.push((Dir::East, id + 1));
                }
                if x > 0 {
                    exits.push((Dir::West, id - 1));
                }
                if y + 1 < FRONTIER_H {
                    exits.push((Dir::South, id + FRONTIER_W));
                }
                if y > 0 {
                    exits.push((Dir::North, id - FRONTIER_W));
                }
                rooms.insert(
                    id,
                    Room {
                        id,
                        name,
                        desc,
                        zone,
                        safe: is_entrance,
                        pvp: false,
                        exits: exits.into_iter().collect(),
                    },
                );

                if is_entrance {
                    continue; // a safe waystation, no foes
                }
                if is_boss_room {
                    let ti = tier as i32;
                    // The boss wears the zone's weakness but never its
                    // resist: prep is a pure reward on the fight players
                    // provision for.
                    let theme = FRONTIER_ZONE_THEMES[z];
                    spawns.push(MobSpawn {
                        id: spawn_id,
                        name: boss,
                        home: id,
                        // Fielded as authored (the Frontier's band row is
                        // 1:1): a straight line from the entry target (a
                        // prepared L40 out of the living dark) to the deep
                        // target (the King's prepared L55), see `CROWNS`.
                        max_hp: 2280 + ti * 147,
                        damage: 56 + (ti * 57) / 20,
                        xp: 420 + ti * 95,
                        respawn_secs: 600,
                        loot: super::items::frontier_loot(z),
                        boss: true,
                        profile: DamageProfile::new(DamageType::Physical, None, theme.weak()),
                    });
                    spawn_id += 1;
                } else if idx.is_multiple_of(2) {
                    let ti = tier as i32;
                    let theme = FRONTIER_ZONE_THEMES[z];
                    spawns.push(MobSpawn {
                        id: spawn_id,
                        name: mob_names[(idx as usize) % 3],
                        home: id,
                        max_hp: 850 + ti * 55,
                        damage: 44 + (ti * 9) / 4,
                        xp: 95 + ti * 25,
                        respawn_secs: 90,
                        loot: super::items::frontier_loot(z),
                        boss: false,
                        profile: DamageProfile::new(
                            DamageType::Physical,
                            theme.resist(),
                            theme.weak(),
                        ),
                    });
                    spawn_id += 1;
                }
            }
        }
    }

    // Pass 2: chain each zone's last cell down into the next zone's first cell.
    for z in 0..FRONTIER_ZONES - 1 {
        let here = FRONTIER_BASE + (z as u32) * per_zone + (per_zone - 1);
        let there = FRONTIER_BASE + ((z as u32) + 1) * per_zone;
        if let Some(r) = rooms.get_mut(&here) {
            r.exits.insert(Dir::Down, there);
        }
        if let Some(r) = rooms.get_mut(&there) {
            r.exits.insert(Dir::Up, here);
        }
    }

    // Hang the whole frontier off Embergate's square (room 1) via a free
    // direction, so every frontier room is reachable from the start.
    let entrance = FRONTIER_BASE;
    let portal = [Dir::Down, Dir::Up]
        .into_iter()
        .find(|d| rooms.get(&1).is_some_and(|r| !r.exits.contains_key(d)))
        .unwrap_or(Dir::Down);
    if let Some(hub) = rooms.get_mut(&1) {
        hub.exits.insert(portal, entrance);
    }
    if let Some(r) = rooms.get_mut(&entrance) {
        r.exits.insert(portal.opposite(), 1);
    }
}

// ---- World extension wings (the path from 115 to 200 rooms) ---------------
//
// Each wing is a chain of rooms branching off an existing "anchor" room into a
// zone, linked head-to-tail. Links are wired in BOTH directions here, so a wing
// can never produce a one-way exit (the class of bug hand-authoring is prone
// to). Wing room ids start at 300 to stay clear of the base world.

/// One room in a wing: its name, description, and the direction that leads
/// DEEPER (to the next room in the chain). The return link is added automatically.
struct WingRoom {
    name: &'static str,
    desc: &'static str,
    /// Direction from this room to the next in the chain.
    onward: Dir,
}

/// Link two rooms reciprocally: `from` gets `dir` -> `to`, `to` gets the
/// opposite back to `from`. Wiring a direction that already leads somewhere
/// else is an authoring bug, and skipping it silently would sever the new
/// rooms with nothing to notice (a cut-off component still gets coordinates
/// of its own), so it panics instead. That refuses server startup:
/// `LateaniaService::new` runs `seed_world` synchronously in `main` before
/// the listener serves.
fn link(rooms: &mut HashMap<RoomId, Room>, from: RoomId, dir: Dir, to: RoomId) {
    if let Some(r) = rooms.get_mut(&from) {
        let prev = r.exits.insert(dir, to);
        assert!(
            prev.is_none_or(|p| p == to),
            "room {from} exit {dir:?} already leads to {prev:?}, cannot relink it to {to}"
        );
    }
    if let Some(r) = rooms.get_mut(&to) {
        let prev = r.exits.insert(dir.opposite(), from);
        assert!(
            prev.is_none_or(|p| p == from),
            "room {to} exit {:?} already leads to {prev:?}, cannot relink it to {from}",
            dir.opposite()
        );
    }
}

/// Append a chain of wing rooms to `rooms`, anchored to `anchor` via `entry`
/// (the direction from the anchor into the wing's first room). Returns the id of
/// the wing's last (deepest) room so callers can place a boss/mob there.
fn add_wing(
    rooms: &mut HashMap<RoomId, Room>,
    zone: &'static str,
    safe: bool,
    anchor: RoomId,
    entry: Dir,
    start_id: RoomId,
    chain: &[WingRoom],
) -> RoomId {
    let mut prev = anchor;
    let mut prev_dir = entry;
    let mut id = start_id;
    for wing in chain {
        rooms.insert(
            id,
            Room {
                id,
                name: wing.name,
                desc: wing.desc,
                zone,
                exits: HashMap::new(),
                safe,
                pvp: false,
            },
        );
        link(rooms, prev, prev_dir, id);
        prev = id;
        prev_dir = wing.onward;
        id += 1;
    }
    id - 1
}

fn wr(name: &'static str, desc: &'static str, onward: Dir) -> WingRoom {
    WingRoom { name, desc, onward }
}

fn extend_world(rooms: &mut HashMap<RoomId, Room>, spawns: &mut Vec<MobSpawn>) {
    let mut next_mob: u32 = 300;
    let mut mob = |spawns: &mut Vec<MobSpawn>,
                   name: &'static str,
                   home: RoomId,
                   hp: i32,
                   dmg: i32,
                   xp: i32,
                   boss: bool,
                   loot: &'static [u32],
                   profile: DamageProfile| {
        let id = next_mob;
        next_mob += 1;
        spawns.push(MobSpawn {
            id,
            name,
            home,
            max_hp: hp,
            damage: dmg,
            xp,
            respawn_secs: if boss { 320 } else { 55 },
            loot,
            boss,
            profile,
        });
    };

    fn p(at: DamageType, res: Option<DamageType>, weak: Option<DamageType>) -> DamageProfile {
        DamageProfile::new(at, res, weak)
    }
    use DamageType as D;

    // Each wing: (zone, anchor, entry dir, onward dir, id base, rooms). Id bases
    // are 30 apart so a wing can grow to 30 rooms without colliding. Mobs are
    // placed relative to the captured start/end ids, never hardcoded.

    // ---- Whisperwood: The Sunken Glade (12 rooms) -----------------------
    let start = 300;
    let last = add_wing(
        rooms,
        "Whisperwood",
        false,
        14,
        Dir::Down,
        start,
        &[
            wr(
                "Whisperwood - The Mushroom Stair",
                "Shelves of bracket-fungus climb a steep slope like a giant's staircase, soft and cold and faintly yielding underfoot, and a slow rain of spores drifts down through the lanternlight to settle on your shoulders. The deeper air tastes of loam and rot and something sweeter beneath. The stair leads down, and the standing-stone ring lies back up.",
                Dir::Down,
            ),
            wr(
                "Whisperwood - The Glowcap Grotto",
                "A hollow beneath a vast upturned root glimmers with luminous mushroom-caps in blue and green and palest gold, casting a drowned and dreamlike light across the soft loam. Moths the size of your hand drift between them on silent wings, and the silence has the held quality of a place that does not often see the living. The way leads on north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Toadstool Court",
                "Rings within rings of pale fungus carpet a still clearing, the old faerie-circles of song, and the longer you stand among them the more keenly you feel yourself watched by small patient things at ankle height. To step inside a ring is reckoned very bad luck, and the toadstools seem to lean inward as you pass. The path continues north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Weeping Willow",
                "A willow vast as a temple tower trails its long branches all the way to the wet ground, curtaining a hollow at its heart, and the wind moving through them makes a sound exactly and unmistakably like a woman weeping. You catch yourself listening for words in it, and almost find them. The way out lies north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Bog Causeway",
                "A path of half-sunk, slime-furred logs crosses a black bog that breathes slow bubbles of marsh-gas and a stench of rot and old death. The water between the logs is depthless and patient, and stepping wrong here would be a very quiet way indeed to vanish from the world. The treacherous causeway leads north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Drowned Oak",
                "A mighty oak has fallen full-length into the bog and rotted from within into a hollow tunnel, and the path runs straight through it, so that for a dozen paces you walk inside the dark damp ribcage of a dead green giant. Pale grubs the length of fingers glisten in the punky wood overhead. The tunnel lets out to the north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Witch's Hut",
                "A crooked hut leans at an impossible angle on foundations that look, in the wrong light, like the scaled feet of an enormous bird, its windows dark and its door standing ajar on a single slowly creaking hinge. Bundles of dried herbs and less wholesome things twist in the doorway, and nothing inside makes a sound. The path goes on north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Hag's Garden",
                "Behind the hut a walled garden grows things no honest garden ever should: pale swollen gourds with the half-formed suggestion of faces, vines that visibly flinch and recoil from your lantern's light, and beds of black flowers that turn to follow you. The soil here is too rich, and too dark, and you would rather not wonder why. The path leads north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Bone Orchard",
                "The trees of this orchard have grown around old bones over long slow years until trunk and skeleton are grown wholly into one, ribs and root indistinguishable in the gloom. The dark fruit they bear hangs heavy and glistening, and every instinct you own insists it is best left unpicked and untasted. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Moonwell",
                "A perfectly round well of old mortared stone brims to its very lip with water that glows a faint cold silver, and its surface reflects a full and brilliant moon that hangs nowhere in tonight's actual sky. To look too long into it is to feel the strong and dangerous urge to lean closer. The path continues north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Whispering Stones",
                "A ring of tall leaning stones, lichen-grey and older than the forest around them, mutters and murmurs softly among themselves in a language just below understanding, and falls utterly silent the very instant you turn your head to listen. The grass within the circle has never once been cut, yet grows no higher than your ankle. The glade lies on north.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Sunken Glade",
                "The trees draw back from a circle of green where a single shaft of moonlight falls, beautiful and far too quiet, where something has waited a very long time. The way back is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a will-o'-wisp",
        start + 1,
        26,
        6,
        24,
        false,
        COMMON_LOOT,
        p(D::Fire, None, Some(D::Frost)),
    );
    mob(
        spawns,
        "a giant glowcap spider",
        start + 5,
        34,
        7,
        30,
        false,
        COMMON_LOOT,
        p(D::Poison, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a bog-mire lurker",
        start + 8,
        40,
        8,
        36,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Hexcrone of the Glade",
        last,
        130,
        13,
        165,
        true,
        &[1006, 1110, 1111, 1201, 1302],
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );

    // ---- Duskhollow: The Barrow Deep (11 rooms) -------------------------
    let start = 330;
    let last = add_wing(
        rooms,
        "Duskhollow Caverns",
        false,
        37,
        Dir::West,
        start,
        &[
            wr(
                "Duskhollow - Behind the Sealed Door",
                "The great chained door gives at last onto a passage that no light has touched in centuries, the air beyond it dead and close and faintly, sickly sweet with the perfume of old decay. Dust lies undisturbed and ankle-deep, and your footprints are the first to mark it since the door was sealed. The passage runs west into the dark.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Gravewater Pool",
                "Black water fills a wide stone basin clear to the brim, utterly still, and pale shapes drift just beneath its skin, neither sunk nor surfaced, turning with a slowness that has nothing to do with any current. One of them, you are nearly certain, was facing the other way a moment ago. The passage continues west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Creeping Dark",
                "Your lantern-flame seems to shrink and gutter here for no draught you can find, and the dark presses in close enough to feel against the skin, a weight on the shoulders that is patient and almost, horribly, fond. It does not want to hurt you. It only wants you to stay. The way on lies west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Hall of Urns",
                "Thousands upon thousands of clay funerary urns line shelves that climb to an unseen ceiling, each one holding the forgotten ash of a forgotten life. Many have been broken open, and their grey contents lie scattered across the floor in trails that lead off into the dark, as though something went looking through them. The hall runs west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Mourner's Stair",
                "A long stair descends, its steps worn into a smooth central trough by the passage of countless centuries of grieving feet, down toward a cold that deepens with every footfall until your breath smokes white before you. Somewhere far below, water drips with the patience of an age. The stair leads down and on to the west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Catacomb Maze",
                "Passages branch and rejoin and double back among high walls of neatly stacked human bone, skull set upon skull, until direction itself loses all meaning and the maze seems to rearrange behind you. Only the faint cold draught breathing from somewhere ahead keeps your feet pointed true. Follow it west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Lamentation Hall",
                "A vast vaulted chamber catches the slightest sound you make and returns it warped and multiplied as a soft chorus of weeping, so that a single cleared throat becomes a hundred mourners, and you slowly lose the ability to tell your own echo from the grief of the listening dead. Best to move quietly. The chamber opens west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Gilded Tomb",
                "A single great tomb of beaten gold gleams warm and untouched amid all the surrounding rot, its heavy lid carved with the serene effigy of a sleeping king. The lid has been pushed askew from the inside, and the king it portrays is very plainly no longer at home within. The way on lies west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Guardian's Rest",
                "Stone sentinels line the final approach in two grim ranks, each clutching a real and rusted sword in its carved granite hands, and each, you slowly realize with a cold drop in the stomach, has taken exactly one heavy step down from its plinth toward the path. They wait now with the stillness of things that can afford to. The vault lies west.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Barrow King's Vault",
                "A burial chamber fit for a king who refused the grave: drifts of gold and grave-goods heaped glittering in the dark, weapons and crowns and the bones of buried servants. At its center stands a black throne, and upon it a crowned and withered thing, dry as old leather, slowly turns its head on a creaking neck to mark that someone has finally come. The only way out is back east.",
                Dir::West,
            ),
        ],
    );
    mob(
        spawns,
        "a tomb-rat swarm",
        start + 1,
        38,
        7,
        30,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a grave moth cloud",
        start + 3,
        44,
        8,
        38,
        false,
        COMMON_LOOT,
        p(D::Poison, None, Some(D::Holy)),
    );
    mob(
        spawns,
        "a shambling barrow-guard",
        start + 6,
        52,
        9,
        48,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "a clutch of bonepickers",
        start + 8,
        56,
        10,
        54,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Barrow King",
        last,
        190,
        17,
        235,
        true,
        &[1105, 1114, 1113, 1202, 1302],
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );

    // ---- Drowned Crypts: The Tidal Catacombs (11 rooms) -----------------
    let start = 360;
    let last = add_wing(
        rooms,
        "Drowned Crypts",
        false,
        54,
        Dir::South,
        start,
        &[
            wr(
                "Drowned Crypts - The Brine Stair",
                "Salt-crusted steps spiral steeply down into dark water that rises to meet you, cold as a drowned bell and tasting of deep brine and older death. The walls run with weeping rivulets, and far below the stair the black water waits without a ripple. The way down leads south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Coral Ossuary",
                "Bone and pale coral have grown into one another over drowned centuries until you cannot tell which parts were once the dead and which the patient sea made afterward. Skulls flower with coral horns, and ribcages cradle anemones that flinch closed as your light sweeps past. The flooded passage runs south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Kelp Forest",
                "Thick ropes of black kelp rise from the flooded dark and sway in slow unison though there is no current to move them, parting only reluctantly as you wade waist-deep through the cold. Now and then a strand brushes your leg and seems, for an instant, to tighten. The drowned forest gives way south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Sunken Chapel",
                "A small chapel stands fully submerged, its pews still ranked in drowned and silent rows beneath the surface, and upon the altar a single candle burns impossibly underwater, trailing a thin grey thread of smoke up through the green water to the unseen ceiling. Someone, or something, still keeps the vigil here. The flooded nave opens south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Pearl Vault",
                "Drowned treasure spills in glittering drifts from broken iron-bound chests, gold and pearl and gem heaped enough to ransom a kingdom, and every last piece of it is furred over with the same soft pale rot that fuzzes the bones between. To fill your pockets here would be to carry the grave home with you. The way leads south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Anemone Garden",
                "Things that might be flowers and might equally be mouths carpet the dripping walls from floor to ceiling, opening and closing in a slow, patient, breathing unison that follows you as you pass. A sweet rotten scent rises from them, and the nearest ones lean and turn to track your warmth. The chamber empties south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Siren's Landing",
                "A dry stone shelf lifts above the flood, and upon it stands a single weather-worn carved seat facing out over the black water, where something once sat through the long nights to sing passing ships down to their drowning. The seat is smooth with long use, and not quite cold. The shelf-path continues south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Black Trench",
                "The floor falls away without warning into a vast trench whose bottom the lantern-light never finds, only deepening blue going down to black, and from its depths a slow cold current breathes steadily up into your face like the exhalation of something enormous and asleep. A narrow ledge skirts the void. Follow it south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Bone Reef",
                "A reef built entirely from the bones of the drowned rises in pale ramparts and arches across the flooded cavern, the accumulated dead of a thousand wrecks knit together by coral and time. Pale eyeless things nest deep in its hollows, and they shift and click as your light crosses them. The way through lies south.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Leviathan's Maw",
                "A vast flooded cavern opens at the catacomb's end, dominated by the bleached rib-cage of something so enormous it should not fit in any sea the maps record, each rib an arch you could sail a boat beneath. In the green shadow beneath that cage of bone, a drowned horror uncoils and stirs toward the warmth of your coming. The only way back is north.",
                Dir::South,
            ),
        ],
    );
    mob(
        spawns,
        "a drowned acolyte",
        start + 1,
        58,
        11,
        60,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "a kelp-strangler",
        start + 3,
        64,
        12,
        66,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "a reef-thing",
        start + 6,
        70,
        13,
        72,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "a brine-bloated drowned",
        start + 8,
        74,
        13,
        76,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "the Tide-Drowned Leviathan",
        last,
        260,
        21,
        340,
        true,
        &[1008, 1115, 1204, 1302],
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );

    // ---- Emberpeak: The Deep Forge (11 rooms) ---------------------------
    let start = 390;
    let last = add_wing(
        rooms,
        "Emberpeak Mines",
        false,
        69,
        Dir::North,
        start,
        &[
            wr(
                "Emberpeak - The Cleared Drift",
                "Fresh rubble has been dragged aside to clear a way, the pick-marks still bright in the broken stone, and beyond it the old dwarven tunnels run on into a dry heat lit by a deep red glow from somewhere far ahead. The air smells of hot iron and char. The drift continues north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Ore Sorters",
                "Long conveyor troughs of cold black iron run the length of the hall, still holding their last sorted heaps of glittering raw ore exactly where the dwarven crews left them when they fled, untouched for an age. A single tin cup sits on the edge of a trough, as if its owner stepped away a moment ago. The tunnels run on north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Gem Cutters' Hall",
                "Rows of jewellers' workbenches stand abandoned mid-task, half-cut gems still clamped in their tiny vices, catching the distant forge-light and throwing it back like trapped and frightened sparks. Fine tools lie scattered as though dropped in a single shared instant of alarm. The hall opens north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Molten Channel",
                "A river of slow molten magma crosses the hall in a great hewn stone trough, glowing sullen orange and gold, and the air above it shimmers and warps hard enough to bend the very sight, so the far wall seems to swim and melt. The heat is a hand pressed flat against your face. A narrow span crosses it to the north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Bellows Engine",
                "A vast machine of cracked leather bellows and pitted iron fills the chamber and still wheezes faintly on, all on its own, breathing hot furnace-air into tunnels that no living hand has tended for centuries. Its slow rasping breath sounds disquietingly like that of a great sleeping beast. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Slag Cathedral",
                "Over a thousand years of discarded waste glass and cooled slag have been heaped and fused into soaring buttresses and arches, a vast cathedral built entirely by accident, its translucent walls catching the red glow and scattering it in a thousand sullen colors. It is grand, and unintended, and somehow holy. The nave runs north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Runesmith's Sanctum",
                "Walls dense with carved dwarven runes pulse and glow with a banked inner heat, the old work-songs and wardings of a vanished people, and at the heart of the sanctum a great forge of black iron broods over coals that have never once gone cold in all the centuries since its makers died. Something keeps it fed. The passage continues north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Ash Vault",
                "Knee-deep grey ash fills a sealed vault to which there is no other door, soft and undisturbed but for one thing: across its whole surface something has been writing, over and over and over in a child's clumsy hand, the same single dwarven word, which means sorry. The fresh strokes are still sharp. The way out lies north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Firewalk",
                "A narrow railless bridge of fire-blackened stone arches across a wide lake of slow-churning fire, and the span underfoot is warm enough to feel clearly through the soles of your boots, growing hotter toward the middle. Updrafts of furnace-air pluck at your clothes with every step. The bridge leads north.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Heart of the Forge",
                "The deepest forge of all opens here, hewn straight into a vein of living magma that lights the whole cavern the color of a wound and fills it with a roar of heat. As your shadow falls across the coals, a guardian of fused slag and molten fire, raised to keep this place against all comers, heaves itself ponderously upright to do exactly that. The only way out is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a coal-wretch",
        start + 1,
        80,
        14,
        84,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "a cinder-imp",
        start + 3,
        84,
        14,
        86,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "a runeforged sentry",
        start + 6,
        88,
        15,
        90,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Poison), Some(D::Frost)),
    );
    mob(
        spawns,
        "a slag golem",
        start + 8,
        94,
        16,
        96,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "the Forgeheart Guardian",
        last,
        340,
        27,
        460,
        true,
        &[1009, 1116, 1117, 1205, 1304],
        p(D::Fire, Some(D::Fire), Some(D::Frost)),
    );

    // ---- Frostspire: The Glacier's Heart (11 rooms) ---------------------
    let start = 420;
    let last = add_wing(
        rooms,
        "Frostspire Ascent",
        false,
        84,
        Dir::North,
        start,
        &[
            wr(
                "Frostspire - The Blue Descent",
                "A stair carved into the living glacier itself plunges down into translucent blue depths, the steps slick and glassy, the cold deepening with every careful footfall until it burns in the lungs. Shapes are frozen deep in the ice on either hand, too dim to name. The descent leads north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Frozen Falls",
                "A waterfall caught and frozen mid-plunge forms a vast curtain of clear ice three storeys high, glittering and motionless, and behind its warped glass something dim and slow shifts its weight from one side to the other. You tell yourself it is only the light. The way leads on north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Rime Galleries",
                "Glittering halls of rime-frost branch away in every direction, their walls so impossibly clear that you see straight into the frozen blue-black dark of the glacier's deep interior pressing close on all sides. The galleries echo your every breath back as a brittle whisper. The true way lies north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Mammoth Graveyard",
                "Tusked giants lie sprawled where the ice took them an age ago, mammoths and worse, each one perfectly kept and unblemished beneath the clear glacier, their great frozen eyes still open and somehow still seeming to follow your slow progress past. The cold here is the cold of held time. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Aurora Cavern",
                "Light from the unreachable surface filters down through uncounted fathoms of blue ice and breaks, somewhere far above, into slow drifting curtains of green and rose and violet that wash silently across the cavern floor like a captive aurora. It is the most beautiful thing you have seen in days, and the coldest. The way continues north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Frostbound Hoard",
                "A dragon's whole hoard lies sheathed entirely in a fathom of clear ice, every coin and crown and jewelled blade perfectly visible and utterly, mockingly unreachable, a fortune you could spend a lifetime failing to chip free. The ice is scored with the claw-marks of others who tried. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Silent Crevasse",
                "A crevasse splits the glacier so deep that the cold pouring up out of it stops your breath in your throat and frosts your lashes in an instant, and the silence down here is so complete that you can hear the slow heavy beat of your own labored heart. Nothing else moves. A ledge skirts the crack to the north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Wyrm's Spine",
                "The floor itself becomes the frozen length of some titanic serpent locked in the glacier, and you walk its spine scale after vast frozen scale for a full hundred paces, each one broad as a shield underfoot. You try very hard not to wonder where, ahead in the ice, its head must be. The spine leads north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Last Warmth",
                "A geothermal vent breathes warmth into one small chamber, just bearable after the killing cold of the galleries, and the huddle of frost-rimed bones around a long-dead campfire tells you that others found this refuge a little too late to be saved by it. Their packs lie unopened. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Glacier's Heart",
                "At the glacier's frozen core opens a chamber of impossible, luminous blue, and coiled at its center in what was meant to be eternal sleep lies an elder ice-wyrm, vast beyond the scale of the hoard it guards. The warmth of your blood has reached it at last, and it is waking now, slow and immense and very, very furious. The only way back is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a frost-bound wretch",
        start + 1,
        100,
        17,
        106,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "an ice-stalker",
        start + 3,
        104,
        18,
        110,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "a glacial revenant",
        start + 6,
        110,
        18,
        116,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "a hoarfrost wraith",
        start + 8,
        114,
        19,
        120,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Heart-of-Winter Wyrm",
        last,
        440,
        33,
        620,
        true,
        &[1007, 1117, 1205, 1304],
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );

    // ---- Sunken Citadel: The Forbidden Wing (10 rooms) ------------------
    let start = 450;
    let last = add_wing(
        rooms,
        "The Sunken Citadel",
        false,
        99,
        Dir::North,
        start,
        &[
            wr(
                "Citadel - The Sealed Wing",
                "This is a wing the citadel once tried to wall away from itself, the great brickwork seal still standing but bulging slowly outward, course by course, as though something on the far side has been pushing against it with infinite patience for a very long time. A draught of cold dead air leaks through the cracks. The wing runs north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Mirror Gallery",
                "Tall black mirrors line both walls of a long hall, and your reflection in them runs always a half-second late, lagging your steps, until you slowly come to understand with a crawling dread that it is not always troubling to copy what you do at all. Best not to stop and watch. The hall leads north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Forgotten Archive",
                "Shelves of iron-bound books stand toppled and burned the length of a great archive, and the drifts of ash on the floor still hold, impossibly intact, the shapes of words and diagrams that hurt the eye to almost-read and leave an ache behind them. Some knowledge was meant to burn. The archive opens north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Astronomer's Tower",
                "A ruined observatory stands open to a sky full of wrong and unfamiliar stars wheeling in patterns no living astronomer charted, and its great brass telescope sits aimed at one particular patch of starless darkness that seems, the longer you look, to be patiently aiming itself back at you. The dome groans in the wind. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Hall of Hands",
                "Ten thousand carved stone hands reach out from the walls of this hall, open and supplicant, and as you pass between them the nearest ones turn slowly, gently, almost tenderly, to follow your movement and reach a little further toward your warmth. None of them quite touches you. Not yet. The hall continues north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Drowned Laboratory",
                "Flooded laboratory benches hold the dust-furred apparatus of some forbidden study, retorts and coils of glass and bone, and the specimens floating in the rows of cloudy jars turn to track you as you wade past, watching with eyes that have no business still being wet and bright after all these centuries. The water laps at your knees. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Whispering Crypt",
                "The carved stone mouths that mutter throughout the citadel reach their loudest and most insistent here in this crypt, scores of them, all at last speaking the final word of the same enormous sentence the whole fortress has been pronouncing for an age. You feel the word in your teeth before you hear it. The crypt opens north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Throne of Echoes",
                "An empty black throne faces down a long hall built by clever ancient acoustics to carry a single seated voice forever and unfading to its furthest corner, and the still air here trembles faintly yet with the residue of the last command ever given from that seat. It has not finished echoing. The hall runs north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Vault of Saints",
                "The sarcophagi of the citadel's holy dead stand ranked in this vault, and every last one has been cracked open from within, the heavy lids shouldered aside by their occupants, who rose long ago to a sanctity gone sour and strange in the dark. The air is thick with cold incense and something fouler beneath. The vault leads north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Antechamber of the Heart",
                "The black stone of the walls turns subtly warm and almost soft to the touch here, yielding like cooling wax, and your lantern dims and shrinks against the dark as though something just ahead has begun, slowly and steadily, to drink the very light out of the air. Each step forward costs more will than the last. The way on lies north.",
                Dir::North,
            ),
            wr(
                "Citadel - The Sealed Heart",
                "This is the forbidden room at the citadel's very core, the thing the whole fortress was raised to cage, and as the last of your light gutters a being of folded shadow and cold starlight unfurls itself from the bound dark, dimension by impossible dimension, turning what passes for its attention upon the small warm intruder who unsealed its prison. The only way out is back south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a hollow archivist",
        start + 2,
        122,
        22,
        144,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "a mirror-wraith",
        start + 4,
        128,
        23,
        150,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "a grasping hand-swarm",
        start + 6,
        132,
        24,
        156,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Shadow), Some(D::Arcane)),
    );
    mob(
        spawns,
        "the Warden of the Sealed Heart",
        last,
        540,
        39,
        840,
        true,
        &[1109, 1118, 1202, 1304],
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );

    // ---- Obsidian Throne: The Infernal Depths (10 rooms) ----------------
    let start = 480;
    let last = add_wing(
        rooms,
        "The Obsidian Throne",
        false,
        109,
        Dir::South,
        start,
        &[
            wr(
                "Obsidian Throne - The Burning Descent",
                "A stair of black cooling lava, its treads still cracked with veins of dull orange fire, leads down into a heat so total it becomes almost a sound, a low and ceaseless roar that sits forever just at the edge of hearing. Sweat dries before it can fall. The descent leads south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Furnace of Sins",
                "Vast furnaces line a hall longer than a cathedral, and in each the damned are unmade and patiently remade, over and over, screaming on a single seamless loop ten thousand years long and showing no sign of nearing its end. The heat-haze bends their writhing shapes. The hall runs south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Chained Legion",
                "Rank upon serried rank of bound demons stand frozen at rigid attention, chained and waiting for a war-horn that has not yet sounded, and as you pass between them ten thousand burning eyes swivel in their stillness to track you the whole length of the hall. Not one of them so much as breathes. The way on lies south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Pact Chamber",
                "A round room of polished black glass holds the place where bargains were once struck with the throne itself, and the contracts still hang unsigned in the air, written in slow-burning light, turning gently, each one waiting with infinite patience for a desperate enough hand to take up the offered pen. You feel them sense your wants. The chamber opens south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The River of Fire",
                "A true river of liquid flame crosses the dark in a slow blinding flood, and at its near bank a tall ferryman of compacted ash stands waiting beside a boat of charred bone, one open and expectant hand held out for the toll that every soul must pay to cross. His price is rarely coin. The crossing lies south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Gallery of Torments",
                "A long gallery of alcoves runs into the dark, and each one holds a single damned soul fixed in its own eternal and inventively tailored agony, and each lifts its head as you pass to beg you, in a voice worn to a thread, for the one mercy of an end. You cannot give it, and they know, and still they ask. The gallery continues south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Brimstone Bridge",
                "A slender bridge of fused and blackened bone arches high over an abyss that glows the deep sullen red of a banked forge far below, exhaling a hot reek of sulphur that sears the throat with every breath. The bone underfoot is warm and faintly, horribly springy. The bridge crosses to the south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Hall of Broken Oaths",
                "Shattered contracts litter the floor of this hall ankle-deep in drifts of broken light, and the air hangs thick and cold with the lingering ghosts of every promise the throne was only ever glad to watch its bargainers break. They drift against you like cobwebs, whispering the terms you never read. The hall runs south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Weeping Pits",
                "Wide pits of black boiling tar bubble and sigh across the chamber floor, and each slow rising bubble briefly wears a stretched and silent face that mouths a single name, perhaps its own, perhaps yours, before it bursts and sinks back into the churning dark. The smell is of pitch and grief. The way on lies south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Antechamber of the Abyss",
                "The very substance of the realm thins here toward something far worse, the black glass underfoot going slowly translucent, then clear, opening onto a depthless void below that has no bottom, no floor, and no patience left for the warm thing walking above it. Vertigo claws at you. The last threshold lies south.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Abyssal Gate",
                "The infernal realm bottoms out at last before a colossal gate that opens onto pure and howling abyss, and before it stands a herald of Mal'gareth, wreathed in cold fire and older than the sin it serves, who will suffer no living soul to pass through in either direction while it still holds its post. It turns to bar your way. The only road back is north.",
                Dir::South,
            ),
        ],
    );
    mob(
        spawns,
        "a chained tormentor",
        start + 2,
        168,
        30,
        206,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "a tormented soul-husk",
        start + 4,
        174,
        31,
        212,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "an ash ferryman",
        start + 6,
        182,
        32,
        222,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Herald of Mal'gareth",
        last,
        620,
        43,
        1100,
        true,
        &[1009, 1119, 1205, 1401],
        p(D::Shadow, Some(D::Fire), Some(D::Holy)),
    );

    // ---- King's Road: The Bandit Trail (9 rooms, low-level detour) ------
    let start = 510;
    let last = add_wing(
        rooms,
        "King's Road",
        false,
        8,
        Dir::East,
        start,
        &[
            wr(
                "King's Road - The Poacher's Trail",
                "A narrow trail worn by furtive feet winds away east through the brush, and the careful eye picks out the glint of wire snares and the pale scar of deadfall triggers half-hidden in the undergrowth on either side. Someone does not want to be casually followed. The trail leads east; the road lies west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Hollow Tree",
                "A hollow oak stands big enough for a man to shelter inside, and it has plainly been used as exactly that: a ring of cold ashes, a heap of gnawed and cracked bones, and a stink of old habitation say clearly enough by whom, and how recently. The trail goes on east and back west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Abandoned Farmstead",
                "A burned-out farmstead slumps in a weed-choked clearing, its roof-beams fallen, its fields long gone to thistle and bramble, its well gone to still black water that smells of rot. Whoever worked this land did not leave it willingly. The trail continues east and west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Scarecrow Field",
                "Scarecrows of grey rags on crossed sticks lean at subtly wrong angles all across a dead and stubbled field, far more of them than any farmer would ever need, and a careful count leaves you uneasily certain there is one more of them now than there was when you first looked. None of them has a face. The trail runs east and west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Crossroads Gibbet",
                "An iron gibbet creaks slowly on its chain at a forgotten crossroads, swinging in a wind you cannot feel, its long-ago occupant flown now to a clatter of bone and a few greening rags. A weathered board names the crime, but the letters have run to rust. The ways lead east and west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Smuggler's Cellar",
                "A trapdoor sunk in the floor of a ruined roadside inn drops to a low cellar stacked with stolen goods, bolts of cloth and casks and crates, half of it gone to damp and mildew and all of it watched, you are quite sure, by unseen eyes from the further dark. Something down here is breathing. The trail continues east and west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Watchpost",
                "A half-built watchpost of lashed timber overlooks a bend in the trail, well-placed to spot anyone coming up from the road, and its lookout's three-legged stool still holds the warmth of someone who was sitting there a moment ago and is now, abruptly and ominously, nowhere in sight. The alarm has gone ahead of you. The trail runs east and west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Camp Approach",
                "The trees thin ahead toward the flicker of a great fire and the sound of rough laughter and the scrape of whetstones on steel, and the laughter falls silent, all at once, as you draw near. You are clearly expected, and just as clearly not at all welcome. The camp lies east; the trail back is west.",
                Dir::East,
            ),
            wr(
                "King's Road - The Bandit Camp",
                "A ring of tattered tents around a guttering fire marks the lair of the road's bandit crew, and their chief rises, hand on hilt, to greet the fool who found them. The way back is west.",
                Dir::East,
            ),
        ],
    );
    mob(
        spawns,
        "a feral poacher's hound",
        start + 1,
        26,
        5,
        22,
        false,
        COMMON_LOOT,
        DamageProfile::physical(),
    );
    mob(
        spawns,
        "a road cutthroat",
        start + 4,
        30,
        6,
        24,
        false,
        COMMON_LOOT,
        DamageProfile::physical(),
    );
    mob(
        spawns,
        "a crossbow bandit",
        start + 6,
        32,
        7,
        28,
        false,
        COMMON_LOOT,
        DamageProfile::physical(),
    );
    mob(
        spawns,
        "the Bandit Chief Garrote",
        last,
        110,
        12,
        130,
        true,
        &[1006, 1110, 1111, 1201, 1301],
        // Living flesh: the cheapest coat in the game (a tier-0 poison
        // vial) answers the game's first boss - the earliest prep lesson.
        DamageProfile::new(DamageType::Physical, None, Some(DamageType::Poison)),
    );
}

/// Common low-tier drop pool shared by wandering wing mobs.
// ---- Hearthward Close: the player-housing district (rooms 9000+) ----------
//
// A public courtyard off Embergate's Market Row, ringed with one home of each
// tier. The rooms are static and always present (so movement, visiting, and the
// snapshot all work unchanged); a deed merely records *ownership* in the service,
// and furniture is placed as runtime side-state. Anyone may walk in - the homes
// are shared-world, true to Ultima Online.
fn extend_housing(rooms: &mut HashMap<RoomId, Room>) {
    use super::housing::{HOUSING_BASE, TIERS, plot_base};

    const MARKET_ROW: RoomId = 3;
    // The five plot doors open off the close; south is the road back to market.
    let tier_dirs = [Dir::North, Dir::East, Dir::West, Dir::Up, Dir::Down];

    // The close itself, with a door to each home and the road back to market.
    let mut close_exits: Vec<(Dir, RoomId)> = vec![(Dir::South, MARKET_ROW)];
    for (i, _) in TIERS.iter().enumerate() {
        close_exits.push((tier_dirs[i], plot_base(i)));
    }
    rooms.insert(
        HOUSING_BASE,
        Room {
            id: HOUSING_BASE,
            name: "Hearthward Close",
            zone: "Hearthward Close",
            safe: true,
            pvp: false,
            desc: "A quiet cobbled court tucked behind Market Row, ringed with the doors of \
                   honest homes. A weathered housing clerk keeps a lectern of deeds by the \
                   gate, a wattle hut and a thatched cottage face each other across the \
                   stones, a longhouse fronts the lane, a broad stair climbs to a stone \
                   manor, and steps wind down to the foot of a slender wizard's tower. The \
                   road back to market runs south. These homes are open to all who call \
                   - knock, or simply walk in.",
            exits: close_exits.into_iter().collect(),
        },
    );
    // Open the close from Market Row.
    if let Some(m) = rooms.get_mut(&MARKET_ROW) {
        m.exits.insert(Dir::North, HOUSING_BASE);
    }

    for (i, t) in TIERS.iter().enumerate() {
        let base = plot_base(i);
        let n = t.rooms();
        for k in 0..n {
            let id = base + k as RoomId;
            let mut exits: Vec<(Dir, RoomId)> = Vec::new();
            // The entrance room links back out to the close. Interior rooms chain
            // North/South (not East/West) so this back-to-close direction can
            // never collide with the forward link and overwrite it - the Longhouse
            // door faces East, which was exactly the old chain direction, so its
            // way out was clobbered and anyone who entered was trapped.
            if k == 0 {
                exits.push((tier_dirs[i].opposite(), HOUSING_BASE));
            }
            // Link to the previous room (a stair where we cross to the upper floor).
            if k > 0 {
                let stair = k == t.ground;
                exits.push((
                    if stair { Dir::Down } else { Dir::North },
                    base + k as RoomId - 1,
                ));
            }
            // Link to the next room (a stair up at the floor boundary).
            if k + 1 < n {
                let stair = k + 1 == t.ground;
                exits.push((
                    if stair { Dir::Up } else { Dir::South },
                    base + k as RoomId + 1,
                ));
            }
            let upper = k >= t.ground;
            let role = house_room_role(t.label, k, upper, n);
            let name = leak(format!("{} - {}", t.label, role));
            let desc = leak(format!(
                "{} You are inside a home you may make your own. {}",
                house_room_desc(upper, k == 0),
                "Buy a deed at the close to claim it, then furnish it from the clerk's catalogue."
            ));
            rooms.insert(
                id,
                Room {
                    id,
                    name,
                    desc,
                    zone: t.label,
                    safe: true,
                    pvp: false,
                    exits: exits.into_iter().collect(),
                },
            );
        }
    }
}

/// A room's role label within a home, by floor position.
fn house_room_role(_tier: &str, k: usize, upper: bool, n: usize) -> &'static str {
    if n == 1 {
        return "Single Room";
    }
    if upper {
        return if k == n - 1 {
            "Upper Solar"
        } else {
            "Upper Landing"
        };
    }
    match k {
        0 => "Entrance Hall",
        1 => "Hearth Room",
        _ => "Back Room",
    }
}

/// Flavour for a home interior by floor.
fn house_room_desc(upper: bool, entrance: bool) -> &'static str {
    if upper {
        "Light falls through a high shuttered window onto bare boards that wait for a life to fill them."
    } else if entrance {
        "A swept threshold opens into quiet rooms, the air still and expectant, smelling faintly of new timber."
    } else {
        "A plain inner room stands empty and clean, its corners waiting for whatever you choose to put there."
    }
}

// ---- The overworld: the Greatroad and three capitals (rooms 600+) --------

/// The overworld: 100 rooms of new biomes radiating from Embergate's South Gate
/// down the Greatroad, plus the three capital cities - Tasmania (harbor),
/// Melvanala (mountain lake), and Matlatesh (desert) - each a safe haven with a
/// healing fountain and the builder's dedication plaque (see FEATURES). Built on
/// the same reciprocal add_wing spine as extend_world, so reachability and exit
/// reciprocity hold by construction. Mob ids start at 600 to clear all earlier
/// spawns; the three capital wings are safe and carry no mobs.
fn extend_overworld(rooms: &mut HashMap<RoomId, Room>, spawns: &mut Vec<MobSpawn>) {
    let mut next_mob: u32 = 600;
    let mut mob = |spawns: &mut Vec<MobSpawn>,
                   name: &'static str,
                   home: RoomId,
                   hp: i32,
                   dmg: i32,
                   xp: i32,
                   boss: bool,
                   loot: &'static [u32],
                   profile: DamageProfile| {
        let id = next_mob;
        next_mob += 1;
        spawns.push(MobSpawn {
            id,
            name,
            home,
            max_hp: hp,
            damage: dmg,
            xp,
            respawn_secs: if boss { 300 } else { 55 },
            loot,
            boss,
            profile,
        });
    };
    fn p(at: DamageType, res: Option<DamageType>, weak: Option<DamageType>) -> DamageProfile {
        DamageProfile::new(at, res, weak)
    }
    use DamageType as D;

    // ---- The Greatroad (9 rooms): the spine west from Embergate ---------
    add_wing(
        rooms,
        "The Greatroad",
        false,
        5,
        Dir::West,
        600,
        &[
            wr(
                "The Greatroad - The Westgate Mile",
                "Beyond Embergate's south gate the King's Road forks, and the Greatroad peels away west: a broad ribbon of old imperial flagstone, rutted by ten centuries of cartwheels and kept just clear enough of brigands to be called safe by optimists. Milestones march off into the haze, each chiselled with the league-count to cities you have only ever heard of in songs. The road runs on west, and Embergate lies back east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Toll Bridge",
                "A humpbacked stone bridge vaults a slow brown river, its toll-house long abandoned and its gate-arm rotted off the hinge. Beneath the span the water slides green and patient around the piers, and a heron stands one-legged among the reeds, wholly unimpressed by your passing. The road carries on west and east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Crossroads Shrine",
                "Here the Greatroad meets a northbound track, and at their meeting a weathered shrine to the road-god stands heaped with the small offerings of nervous travellers: copper coins, a child's shoe, a sprig of dried rosemary gone to dust. A painted board points north to the harbor-city of Tasmania, its lettering salt-faded but legible. The road runs west and east, and the northbound track climbs away toward the distant sea.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Poplar Avenue",
                "Tall poplars line the road in two unbroken ranks, planted by some forgotten governor to shade legions that no longer march, and the wind through their high leaves makes a dry, ceaseless, sea-like sighing. Their shadows fall in long bars across the worn stone, and between them the late light lies spilled like honey. The avenue runs west and east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Wayfarer's Rest",
                "A ruined coaching inn slumps at the roadside, half its roof fallen in, but one corner has been patched with hides and someone keeps a fire there for any soul benighted on the road. Tonight it stands empty, the embers banked low, a black kettle left hopefully on its hook above the coals. The road goes on west and east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Mountain Turn",
                "The land begins to heave upward, and a second track breaks away to the north, switchbacking toward the grey shoulders of the mountains and the lake-city of Melvanala hidden somewhere among them. The air here already tastes of cold stone and crushed pine. The Greatroad presses on west, the mountain track climbs north, and the way you came lies east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Locust Fields",
                "The road crosses a wide plain of abandoned grainfields gone to wild oats and the endless dry sawing of locusts, the husks of farmsteads standing roofless among them like the bones of a meal long since finished. A scarecrow leans at the verge, and you are nearly past before you notice it has turned its straw face to watch you go. The road runs west and east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Dust Reach",
                "The green drains out of the country by slow degrees until the road runs through a hard ochre land of thornscrub and heat-shimmer, the flagstones half-swallowed by blown grit. The west wind carries a fine hot sand that sings against your teeth and stings the eyes, and the horizon ahead has taken on the brassy glare of true desert. West and east.",
                Dir::West,
            ),
            wr(
                "The Greatroad - The Caravan Fork",
                "The Greatroad ends at a great fork worn into the desert's very edge, where the caravan roads diverge: one west into the gold furnace of the Sahra Wastes and the mud-walled city of Matlatesh, others scattering toward rumors of water and grass. A broken obelisk marks the place, its proud inscription scoured smooth and blank by a thousand years of sand. Tracks lead west, and the road home lies east.",
                Dir::West,
            ),
        ],
    );
    mob(
        spawns,
        "a road-worn brigand",
        601,
        30,
        6,
        26,
        false,
        COMMON_LOOT,
        p(D::Physical, None, None),
    );
    mob(
        spawns,
        "a dust-jackal",
        607,
        38,
        8,
        34,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Frost)),
    );
    mob(
        spawns,
        "a scarecrow that walks",
        606,
        46,
        9,
        44,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Poison), Some(D::Fire)),
    );

    // ---- Tasmania (7 rooms): the harbor capital (SAFE) ------------------
    add_wing(
        rooms,
        "Tasmania",
        true,
        602,
        Dir::North,
        620,
        &[
            wr(
                "Tasmania - Harborgate Square",
                "The northbound track ends at the sea-gate of Tasmania, and the city opens before you all at once: white-walled and red-roofed, tumbling down its hill to a harbor crowded with masts, loud with gulls and ship-chandlers and the bargaining of a hundred tongues. At the square's heart a great tiered fountain catches the sea-light, and a bronze plaque is set into the harbor wall beside it. A sealed boneyard stair drops down into the old catacombs, streets climb north into the city, and the Greatroad lies back south.",
                Dir::North,
            ),
            wr(
                "Tasmania - The Chandler's Row",
                "A steep cobbled street of ship-chandlers and net-menders, every doorway hung with coils of tarred rope, brass lanterns, and the clean iron smell of fish-hooks sold by the gross. Cats sun themselves on the warm stone and watch the wheeling gulls with the air of professionals reviewing amateurs. The street climbs north and drops back south to the square.",
                Dir::North,
            ),
            wr(
                "Tasmania - The Salt Market",
                "Under a vast patched awning the salt market roars: pyramids of white and grey and rose-pink salt, barrels of cured fish, ropes of garlic and dried chilies, and fishwives whose voices could strip the paint from a hull at forty paces. The air is a solid wall of brine and spice and frying oil. The way runs north and south.",
                Dir::North,
            ),
            wr(
                "Tasmania - The Cathedral of the Tide",
                "A great pale cathedral rises over the rooftops, its tall windows glazed with sea-green glass so that the light within swims and ripples as though the whole soaring nave lay drowned beneath the waves. Pilgrims come here to light slow candles for sailors who never made it home. The way climbs north, and the market lies south.",
                Dir::North,
            ),
            wr(
                "Tasmania - The Lighthouse Stair",
                "A long stair climbs the seaward cliff to the foot of the great lighthouse, whose patient lamp has not failed in three hundred years. From the windy landing the whole Sapphire Coast unrolls to the east, cliff and cove and the far white line of breaking surf. The city falls away north and south, and a cliff-path leads east along the coast.",
                Dir::North,
            ),
            wr(
                "Tasmania - The Governor's Terrace",
                "The topmost terrace of the city is given over to the governor's pale colonnaded palace and its gardens of wind-bent tamarisk, where the nobility take the evening air and pretend with great effort not to watch one another. The view to the north is nothing but open, gleaming sea. The terrace runs north and south.",
                Dir::North,
            ),
            wr(
                "Tasmania - The Watchtower Crown",
                "The city ends at its very highest point, an old watchtower crowning the hill, its beacon-pan long cold but still heaped and ready. From here Tasmania lies spread out below like a thing built of coral and chalk, and beyond it the sea simply goes on forever. The only way is back south.",
                Dir::North,
            ),
        ],
    );

    // ---- The Sapphire Coast (12 rooms): sea cliffs east of Tasmania -----
    let last = add_wing(
        rooms,
        "The Sapphire Coast",
        false,
        624,
        Dir::East,
        640,
        &[
            wr(
                "The Sapphire Coast - The Cliff Path",
                "A narrow path clings to the chalk cliff above a sheer drop where the sea breaks white on black rocks a hundred feet below, and the wind comes off the water hard enough to lean your whole weight against. Seabirds wheel and scream from their nests in the cliff-face, loudly resentful of the company. The path runs north, and Tasmania lies west.",
                Dir::North,
            ),
            wr(
                "The Sapphire Coast - The Smuggler's Cove",
                "A hidden cove opens at the foot of a treacherous goat-track, its shingle beach littered with the grey ribs of wrecked boats and, higher up the strand, the cold ashes and stacked kegs of folk who do their trading strictly by moonlight. The tide is out, and the sea-caves gape black and dripping. East and south.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Tidal Flats",
                "At low water a vast plain of rippled sand and mirror-bright pools stretches out toward a sea gone distant and small, and the cockle-pickers' baskets lie abandoned where their diggers fled from something none of them will name. The returning tide is only a rumor on the wind, for now. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Driftwood Henge",
                "Someone, or something, has hauled the bone-pale trunks of drowned trees upright into a rough circle on the strand, hung with fishing-floats of green glass and the small picked skulls of seabirds that turn and clack against one another in the wind. It is far older than it has any right to be. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Sea-Cave Mouth",
                "The cliff splits in a vast cave-mouth that breathes the sea in and out with a long, hollow, living groan, and far back in its dripping throat something pale shifts in water that has never once seen the sun. The whole tide-line is hung with weed like sodden green hair. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Coral Shelf",
                "The path crosses a wide shelf of dead white coral, sharp as smashed crockery underfoot, pocked everywhere with rock-pools where anemones the color of fresh bruises open and close with a slow and disconcerting intent. The sea sucks and clatters in the hollows below. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Wreck of the Cormorant",
                "A great galleon lies broken-backed across the rocks, her masts down and her hull stove wide open, and her gilded figurehead - a straining cormorant - still reaches seaward as though it might yet tear free and fly. Crabs the size of dinner-plates have claimed the captain's flooded cabin as their own. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Pearl Divers' Camp",
                "A shantytown of stilt-huts and drying-racks clings to a sheltered inlet where the pearl-divers worked, for the camp is silent now, the diving-stones still corded and waiting by the water's edge, the cook-fires gone long and utterly cold. Nothing moves but the flies. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Singing Sands",
                "A long crescent of fine white sand moans and booms underfoot with every step, a deep uncanny music that the coast-folk swear is the voices of the drowned singing up through the beach to call new company down. It raises the fine hairs on your arms. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Drowned Causeway",
                "A paved causeway runs arrow-straight out into the sea and simply vanishes beneath the waves, the road to some island the water swallowed an age ago; at the lowest tide its first stones glisten just clear, leading the eye and the foolish out toward the deeps. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Kraken's Reach",
                "The coast bends into a deep, still, oily bay where no birds fly and the water lies flat and black and waiting, and the rocks above the tideline are scored everywhere with great curving grooves that no storm ever cut. The air smells of cold salt and a very old fear. East and west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Tide-King's Grotto",
                "The path ends at last in a sea-grotto where the swell rushes in to fill a vast green-lit cavern, and upon a throne of barnacled rock something ancient and immense uncoils from the deep water to regard the small warm morsel that has wandered so far down its shore. The only way out is west.",
                Dir::East,
            ),
        ],
    );
    mob(
        spawns,
        "a cliff-nesting harpy",
        641,
        50,
        10,
        56,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Lightning)),
    );
    mob(
        spawns,
        "a shambling drowned sailor",
        644,
        58,
        11,
        64,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "a giant shore-crab",
        646,
        66,
        12,
        70,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "a singing-sand wraith",
        648,
        60,
        13,
        72,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Frost), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Tide-King of the Reach",
        last,
        300,
        22,
        380,
        true,
        &[1008, 1115, 1205, 1302],
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );

    // ---- Melvanala (7 rooms): the mountain-lake capital (SAFE) ----------
    add_wing(
        rooms,
        "Melvanala",
        true,
        605,
        Dir::North,
        660,
        &[
            wr(
                "Melvanala - The Lakeshore Square",
                "The mountain track climbs at last into Melvanala, a city of grey stone and blue slate terraced up the steeps above a vast and utterly still mountain lake. Woodsmoke and the sharp scent of pine-resin hang in the thin bright air, and at the heart of the lakeshore square a tiered fountain murmurs beside a bronze plaque set into the old retaining wall. A bramble path slips east beneath the old wood, stairs climb north into the city, and the Greatroad track falls away south.",
                Dir::North,
            ),
            wr(
                "Melvanala - The Coppersmith's Steps",
                "A stepped street rings all day long with the bright hammering of the coppersmiths, whose wares - kettles, braziers, bells, and prayer-wheels - hang gleaming from every lintel and turn the slanting evening light to running flame. The steps climb north and descend south to the square.",
                Dir::North,
            ),
            wr(
                "Melvanala - The Pilgrim's Stair",
                "A broad stone stair, worn into shallow troughs by the knees of countless generations, climbs between walls hung with sun-faded prayer-flags toward the high monastery above. Brass cylinders line the way, and the mountain wind spins them so they whisper their endless blessings to no one at all. North and south.",
                Dir::North,
            ),
            wr(
                "Melvanala - The Hanging Gardens",
                "Terrace upon terrace of mountain gardens cling to the slope, thick with alpine flowers and the drowsy hum of bees, fed by a clever lattice of stone channels that catch and share the snowmelt. From up here the whole city lies laid out below like a careful model of itself. North and south.",
                Dir::North,
            ),
            wr(
                "Melvanala - The Monastery Gate",
                "The pilgrim stair ends at the iron-bound gate of the high monastery, where saffron-robed monks keep a silence so deep it seems to carry an actual weight, and from the gatehouse the Verdant Highlands roll away green and gold and endless to the east. The city lies south, and a herders' path leads off east into the hills.",
                Dir::North,
            ),
            wr(
                "Melvanala - The Bell Tower",
                "A slender tower holds the great bronze bell of Melvanala, rung only three times a year, its single deep voice said to carry to every peak that can see the lake. From the high gallery the water lies far, far below, a held breath of perfect silver. North and south.",
                Dir::North,
            ),
            wr(
                "Melvanala - The Sky-Burial Ledge",
                "The city's highest place is a windswept stone ledge thrown open to the peaks and the patiently wheeling vultures, where the dead of Melvanala are given back up to the sky they loved. It is a place of fierce, cold, absolute beauty, and an even deeper peace. The only way is back south.",
                Dir::North,
            ),
        ],
    );

    // ---- The Verdant Highlands (12 rooms): green hills east of Melvanala
    let last = add_wing(
        rooms,
        "The Verdant Highlands",
        false,
        664,
        Dir::East,
        680,
        &[
            wr(
                "The Verdant Highlands - The Herders' Path",
                "A grassy path winds north through high rolling pasture, dotted with the small dark shapes of grazing yaks and the occasional stone cairn raised by herders to mark the way through the fog that rolls in without warning. Skylarks burst up singing from beneath your very boots. North, and Melvanala lies west.",
                Dir::North,
            ),
            wr(
                "The Verdant Highlands - The Gentian Meadow",
                "A meadow of deep-blue gentian and nodding white edelweiss spills down the hillside in a sweep of color so intense it looks painted, loud with bees and the click of grasshoppers in the warm grass. A lone shepherd's flute carries faintly from somewhere out of sight. North and south.",
                Dir::North,
            ),
            wr(
                "The Verdant Highlands - The Standing Stones",
                "A ring of moss-furred standing stones crowns a green hill, far older than any herder's memory, and the sheep will not graze within the circle no matter how rich the grass grows there. The wind drops oddly still as you step inside. North and south.",
                Dir::North,
            ),
            wr(
                "The Verdant Highlands - The Thundering Falls",
                "A river throws itself off a high green shelf in a white roar of spray, and the path crosses behind the falling water on a slick ledge where the whole world becomes noise and cold rainbow mist. The rock is treacherous and the drop is long. North and south.",
                Dir::North,
            ),
            wr(
                "The Verdant Highlands - The Heather Moor",
                "The grass gives way to a vast purple moor of springy heather and black peat-pools, stretching to every horizon under a sky full of racing cloud-shadow. Curlews call their lonely falling cry, and the wind never once stops moving over the open land. East and south.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Shepherd's Refuge",
                "A round drystone hut crouches in the lee of a tor, its turf roof grown thick with the same heather as the moor, a refuge built for herders caught out by the weather. Inside, a stack of cut peat and a tinderbox wait in patient readiness. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Eagle's Tor",
                "A great granite tor juts from the moor like a clenched fist, and from its summit a golden eagle launches on the updraft, while half a kingdom of green and grey and distant blue spreads out below your feet. The wind up here could carry a careless soul away. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Sunken Lane",
                "The path drops into a green-roofed lane so deep and so old that its banks rise twice a man's height on either hand, laced with the roots of unseen trees and floored with soft black mud. It is cool, and close, and very quiet down here. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Faerie Hollow",
                "A perfect green hollow opens in the hills, ringed with foxglove and toadstool, and the light within has a thick golden cast that makes time itself feel slow and uncertain. You have the strong sense of having interrupted something that has now gone still to watch. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Cattle Raid Ford",
                "A wide shallow river chatters over a stony ford, the crossing churned to mud by hooves and old violence, and a leaning standing-stone records some forgotten cattle-raid in worn spiral carvings. The water runs clear and bitterly cold. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Beast-Lord's Cairn",
                "The hills crowd close around a huge ancient burial cairn, its capstone fallen, its black mouth breathing out the smell of old fur and older blood. Bones gnawed white are scattered thick at the threshold, and not all of them are from sheep. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Antlered Throne",
                "The path ends in a high green amphitheatre walled by hills, where upon a throne of interlaced antler and weathered bone sits the great Beast-Lord of the highlands, vast and shaggy and crowned, rising now to the full towering height of its long-guarded solitude. The only way out is west.",
                Dir::East,
            ),
        ],
    );
    mob(
        spawns,
        "a moor wolf",
        681,
        54,
        11,
        60,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a highland reaver",
        684,
        60,
        12,
        66,
        false,
        COMMON_LOOT,
        p(D::Physical, None, None),
    );
    mob(
        spawns,
        "a cairn-bound revenant",
        690,
        70,
        13,
        78,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Beast-Lord of the Hills",
        last,
        320,
        24,
        420,
        true,
        &[1007, 1117, 1202, 1304],
        p(D::Physical, Some(D::Frost), Some(D::Fire)),
    );

    // ---- The Mistfen (9 rooms): drowned marsh south of the Highlands ----
    let last = add_wing(
        rooms,
        "The Mistfen",
        false,
        686,
        Dir::North,
        700,
        &[
            wr(
                "The Mistfen - The Sinking Path",
                "The firm highland turf rots away northward into a treacherous fen of black water and floating sedge, where a path of half-sunk logs offers the only footing and a cold white mist drinks the sound right out of the air. Something plops into the water just out of sight. North, and the hills lie south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Reed Labyrinth",
                "Walls of reed twice your height close in on every side, channels of still brown water branching and rejoining until the world shrinks to mud, mist, and the rustle of unseen things parting the stems ahead of you. Direction becomes a matter of faith. North and south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Drowned Village",
                "The peaked roofs of a sunken village break the surface of the fen, their windows full of black water, a church spire leaning at a drunken angle with its bell still hung and waiting. The mist hangs a single rope of woodsmoke that has no fire to come from. North and south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Will-o'-Wisp Mire",
                "Pale lights drift and bob across the deep mire, beautiful and patient, each one hovering just over the worst of the sucking mud, each one promising firm ground that is not there at all. They brighten, hopefully, as you draw near. North and south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Bog-Body Barrow",
                "A low island of slightly firmer peat holds an ancient barrow, and the black bog has kept its dead so perfectly that the faces pressing up through the surface still wear their final expressions of surprise. The peat sighs and shifts as if breathing. North and south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Leech-Black Pool",
                "The path skirts a pool so utterly black and still it might be a hole cut clean through the world, and the things that live in it - long, soft, and far too many - lift the surface in slow ripples that all turn, somehow, toward you. North and south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Hag's Causeway",
                "A causeway of mortared skulls, white and grinning, lifts the path above the deepest fen, and at its midpoint a wicker idol leans over the water, freshly garlanded by hands that did not love what they were appeasing. A fungal glow leaks from a sinkhole side-delving here. North, south, and down.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Sunken Cathedral",
                "A vast drowned cathedral rears from the mire, three-quarters swallowed, its remaining stained glass casting drowned and broken colors across the water, and from within comes the slow drip and the slower, deliberate sound of something very large turning over. North and south.",
                Dir::North,
            ),
            wr(
                "The Mistfen - The Marsh-Mother's Hollow",
                "The fen opens into a stagnant lagoon ringed by dead willows, and from its center, draped in weed and rising water, the Marsh-Mother lifts her ancient drowned head and opens arms enough to gather in the whole foolish world. The only way back is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a fen leech-swarm",
        701,
        50,
        10,
        54,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );
    mob(
        spawns,
        "a bog-body shambler",
        704,
        58,
        11,
        62,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Shadow), Some(D::Fire)),
    );
    mob(
        spawns,
        "a drowned bell-ringer",
        707,
        64,
        12,
        70,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Marsh-Mother",
        last,
        300,
        21,
        360,
        true,
        &[1109, 1118, 1204, 1302],
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );

    // ---- The Fungal Hollow (8 rooms): underdark beneath the Mistfen -----
    let last = add_wing(
        rooms,
        "The Fungal Hollow",
        false,
        705,
        Dir::Down,
        800,
        &[
            wr(
                "The Fungal Hollow - The Sinkhole Descent",
                "The Mistfen's sinkhole drops you into a warm and breathing dark, down a slope of soft pale mycelium that gives underfoot like flesh, into a world lit only by the cold blue glow of fungus. The mist and the marsh seal over far above. The hollow goes down, and the surface lies up.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Glowcap Forest",
                "A forest of luminous mushrooms taller than houses spreads in every direction, their caps shedding a soft drifting rain of spores that hangs glittering in the still air and settles cold on your skin. The silence has a texture, like standing inside a held breath. Up and down.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Spore Cloud Gallery",
                "The passage thickens with a dense floating fog of spores that catch the glow and turn the air to luminous soup, and breathing it leaves a strange sweet taste and the creeping certainty that the fungus is, very slowly, learning your shape. Up and down.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Myconid Ring",
                "A wide cavern floor is dimpled with a perfect ring of squat mushroom-folk, utterly still, their blunt faces all turned inward to a contemplation that has clearly been going on for centuries and does not welcome the interruption. Up and down.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Rot Pools",
                "Pools of bubbling digestive slime pock the cavern, hissing softly, dissolving the bones of the unlucky into a pale broth that the surrounding fungus drinks up through threadlike roots. The smell is sweet, and rich, and wrong. Up and down.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Crystal Vault",
                "The fungus thins where a vault of pale crystal takes over, every facet throwing back the blue glow until the chamber blazes like the inside of a star, and clusters of fungus-light pulse in slow patterns that almost, almost resolve into meaning. Up and down.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Spore-Lord's Antechamber",
                "The mycelium underfoot grows thick and propertarial, climbing the walls in pulsing ropes that all run inward and downward toward a single source, and the very air grows heavy with the sense of an enormous slow attention swinging round to face you. Up and down.",
                Dir::Down,
            ),
            wr(
                "The Fungal Hollow - The Heart-Spore",
                "The hollow bottoms out in a great domed chamber where the whole fungal world converges upon one vast pulsing fruiting-body, the Heart-Spore, which splits now along a hundred glowing seams to look upon the warm and breathing thing that has come down into its dark. The only way back is up.",
                Dir::Down,
            ),
        ],
    );
    mob(
        spawns,
        "a shrieker fungus",
        801,
        56,
        11,
        60,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );
    mob(
        spawns,
        "a spore-maddened thrall",
        803,
        62,
        12,
        66,
        false,
        COMMON_LOOT,
        p(D::Poison, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a myconid sovereign's guard",
        806,
        70,
        13,
        74,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Heart-Spore",
        last,
        310,
        22,
        400,
        true,
        &[1008, 1115, 1205, 1304],
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );

    // ---- Matlatesh (7 rooms): the desert capital (SAFE) -----------------
    add_wing(
        rooms,
        "Matlatesh",
        true,
        608,
        Dir::West,
        720,
        &[
            wr(
                "Matlatesh - The Oasis Square",
                "The caravan road climbs a last dune and Matlatesh stands revealed in the bowl of its oasis: a city of honey-colored mud-brick and palm shade, its wind-towers reaching up to catch the desert breeze, its streets cool and dim and smelling of cardamom and dust. A great tiered fountain spills at the square's heart, fed by the blessed spring, and a bronze plaque is set in the shaded wall beside it. A cistern stair descends toward drowned caverns, lanes run west into the city, and the desert road lies east.",
                Dir::West,
            ),
            wr(
                "Matlatesh - The Spice Souk",
                "A roofed bazaar runs deep into cool shadow, its stalls heaped with saffron and cumin and dried roses, with brass and carpets and caged singing-birds, and the haggling never stops nor rises above a confidential murmur. Shafts of dusty light fall from holes in the high roof. West and east.",
                Dir::West,
            ),
            wr(
                "Matlatesh - The Caravanserai",
                "A great arcaded courtyard gives rest to the desert caravans, ringed with stalls for camels and cool cells for their drivers, a fountain trickling at its center and the air thick with the patient grumble of beasts and the smell of dung-fires and mint tea. West and east.",
                Dir::West,
            ),
            wr(
                "Matlatesh - The Astronomer's College",
                "A domed college of pale stone houses the desert's famous star-readers, its courtyard floor inlaid with a vast brass map of a sky far clearer than any rain-country ever sees, its scholars arguing softly beneath an arch of mathematics. West and east.",
                Dir::West,
            ),
            wr(
                "Matlatesh - The Sultana's Water-Garden",
                "Behind high walls a miracle unfolds: a garden of running channels and quiet pools, of orange trees and jasmine and the impossible green that only the truly rich can wring from the desert, every drop of it accounted for and adored. West and east.",
                Dir::West,
            ),
            wr(
                "Matlatesh - The Potter's Quarter",
                "A warren of kilns and drying-yards where the city's red clay is thrown, fired, and painted, the lanes stacked head-high with jars and lamps and tiles, and every wall splashed with the bright glaze-spatter of a hundred years of work. West and east.",
                Dir::West,
            ),
            wr(
                "Matlatesh - The High Minaret",
                "The city's tallest minaret offers a dizzying climb to a balcony where the muezzin calls the hours, and from which the whole oasis lies green and small below while the Sahra Wastes run gold to every edge of the trembling world. The only way is back east.",
                Dir::West,
            ),
        ],
    );

    // ---- The Sahra Wastes (12 rooms): the deep desert south of Matlatesh
    let last = add_wing(
        rooms,
        "The Sahra Wastes",
        false,
        724,
        Dir::South,
        740,
        &[
            wr(
                "The Sahra Wastes - The Last Well",
                "South of the city walls the green ends with a single brick-ringed well, the last sure water before the Sahra Wastes proper, where camel-bones and prayer-rags mark the spot at which sensible travellers turn back. The dunes roll away gold and silent and enormous. South, and Matlatesh lies north.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Singing Dunes",
                "Mountainous dunes march to every horizon, and when the wind crests them they sing in a deep booming moan that you feel in your chest before you hear it, a sound like the desert mourning something vast and long-buried. Your footprints fill behind you as you walk. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Sun-Bleached Caravan",
                "A whole caravan lies preserved and abandoned in the lee of a dune, camels and crates and curl-toed slippers all sandblasted to the same pale gold, the traders sitting yet around a fire that went out a hundred years ago. Nothing has decayed; it has only dried. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Glass Crater",
                "A circle of desert has been fused to green glass, smooth and warm and cracked into a vast mosaic, the relic of some ancient fury fallen from the sky, and at its center the glass is darkest and the heat-shimmer hardest, hiding what lies beneath. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Bone Oasis",
                "A dead oasis: a dry stone basin ringed by the petrified stumps of palms, the water long gone, the place now only a graveyard where the desert's wanderers crawled to die in the memory of shade. The wind moves the sand like slow water. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Buried Colossus",
                "One vast stone hand and the crown of a serene carved face break the surface of the sand, all that shows of a buried colossus whose full size the dunes will never give up, gazing up forever at a sky that has long since forgotten it. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Scorpion Flats",
                "A hard, cracked pan of baked clay stretches between the dunes, and the ground itself seems to seethe, for it is carpeted with scorpions of every size, parting reluctantly before your boots and closing again behind. The heat here is a physical weight. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Mirage Lake",
                "A wide and shimmering lake lies dead ahead, blue and cool and crowded with palms, and it retreats exactly as fast as you advance, for it is no lake at all but the desert's cruelest lie told in light and heat to the thirsty. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Sandstorm Wall",
                "A wall of ochre cloud towers on the southern horizon and rolls steadily nearer, a sandstorm that will flay the skin from the bone of anything caught in the open, and the only shelter is the dark slot of a canyon ahead. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Tomb-Canyon",
                "A slot canyon cuts down through the bedrock, its walls honeycombed with the carved doorways of a thousand desert tombs, their seals broken, their dark mouths breathing out cool air and the dry whisper of disturbed dust. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Hall of the Dune-Kings",
                "The canyon opens into a pillared hall hewn from the living rock, lined with the seated stone statues of the old dune-kings, their painted eyes somehow still bright, watching the intruder come down the long aisle toward the dark at its end. North and south.",
                Dir::South,
            ),
            wr(
                "The Sahra Wastes - The Sand-Wyrm's Maw",
                "The hall ends above a vast funnel of softly sliding sand, and as your shadow falls across it the whole pit erupts, and the Sand-Wyrm of the Sahra rears its city-swallowing bulk into the light, ringed mouth wide, very glad you came. The only way back is north.",
                Dir::South,
            ),
        ],
    );
    mob(
        spawns,
        "a giant desert scorpion",
        746,
        56,
        12,
        64,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "a sun-dried husk",
        743,
        60,
        12,
        68,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "a tomb-canyon ghoul",
        749,
        68,
        13,
        76,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Sand-Wyrm of the Sahra",
        last,
        340,
        25,
        460,
        true,
        &[1009, 1116, 1119, 1205, 1401],
        p(D::Physical, Some(D::Fire), Some(D::Frost)),
    );

    // ---- The Amber Savanna (9 rooms): grassland east of the Sahra -------
    let last = add_wing(
        rooms,
        "The Amber Savanna",
        false,
        746,
        Dir::East,
        760,
        &[
            wr(
                "The Amber Savanna - The Grass Sea",
                "East of the deep desert the dunes give way to a rolling sea of amber grass, shoulder-high and whispering, broken only by the flat green crowns of solitary acacia trees standing like sentinels on the swells. The horizon is impossibly wide. East, and the Sahra lies west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Acacia Stand",
                "A loose grove of thorn-trees offers the only shade for miles, their crowns alive with weaver-birds and their trunks scored by the horns and claws of beasts that come to scratch. The grass beneath is cropped short and littered with old bones. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Watering Hole",
                "A muddy waterhole draws the life of the whole savanna to its banks in a wary, jostling truce, hoofprints and pawprints churned together in the mud, and just now the silence and the absolute stillness of the herd say a hunter is very close. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Migration Trail",
                "A broad trail beaten bare by the passage of countless hooves runs across the grassland, and the very ground trembles faintly with the memory or the approach of the great herds, the dust of their passing hanging gold and immense on the air. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Termite Cathedrals",
                "Spires of red mud rear twice the height of a man across the plain, the cathedrals of the termites, hard as fired brick and riddled within by a numberless industrious dark. Something larger has hollowed one out to make a lair. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Baobab of Bones",
                "A single colossal baobab stands alone, ancient beyond reckoning, its swollen trunk hollowed into a chamber and its branches hung with the bleached skulls of beasts and men alike, an oracle-tree, a charnel-tree, a place of old and bloody power. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Scorched Plain",
                "A wide swath of the savanna has burned recently to black stubble and white ash, still ticking with heat, the new green only just spearing up through the char, and the predators work the open ground here where nothing can hide. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Lion-Throne Kopje",
                "A pile of great sun-warmed boulders rises from the plain like a natural throne, and from its summit the savanna stretches gold to every edge of the sky, the perfect seat for the apex of all this teeming land to survey its domain. East and west.",
                Dir::East,
            ),
            wr(
                "The Amber Savanna - The Pride's Reckoning",
                "The grass opens into a trampled arena ringed by kopje-rock, and here the great Maned Terror of the savanna and its pride rise from the shade as one, unhurried and certain, to deal with the small upright thing that has walked so boldly into the open. The only way back is west.",
                Dir::East,
            ),
        ],
    );
    mob(
        spawns,
        "a savanna hyena",
        761,
        54,
        12,
        62,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a stampeding bull",
        763,
        64,
        13,
        70,
        false,
        COMMON_LOOT,
        p(D::Physical, None, None),
    );
    mob(
        spawns,
        "a baobab oracle-shade",
        765,
        66,
        13,
        74,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Maned Terror",
        last,
        320,
        24,
        430,
        true,
        &[1007, 1117, 1202, 1304],
        p(D::Physical, None, Some(D::Fire)),
    );

    // ---- The Skyreach Mesas (8 rooms): high red-rock country ------------
    let last = add_wing(
        rooms,
        "The Skyreach Mesas",
        false,
        765,
        Dir::North,
        780,
        &[
            wr(
                "The Skyreach Mesas - The Red Ascent",
                "North of the savanna the land buckles upward into towering mesas of banded red rock, and a switchback trail climbs the first of them through layers of stone laid down before the world had any names, the air thinning and cooling with every turn. North, and the grass lies south.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Hoodoo Forest",
                "A forest of slender rock spires, balanced impossibly with great boulders for caps, stands carved by ten thousand years of wind, and they cast long strange shadows that seem to shift and lean when you are not looking straight at them. North and south.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Cliff-Dwellings",
                "An entire abandoned city is built into the sheer face of the mesa, room stacked on room in the cool shade of an overhang, reached by ladders long since rotted away, its grindstones and painted pots all left mid-task an age ago. North and south.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Wind-Bridge",
                "A natural arch of red stone spans a dizzying gulf between two mesas, narrow and railless and humming faintly in the perpetual wind, with a fall on either hand long enough to leave a body time for serious reflection. North and south.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Thunderbird Eyrie",
                "The trail passes beneath a ledge heaped with an enormous nest of whole tree-trunks and sun-bleached bones, and the very rock is scorched in long forking patterns, for this is the eyrie of the thunderbird, and the sky above growls in warning. Up and south.",
                Dir::Up,
            ),
            wr(
                "The Skyreach Mesas - The Petroglyph Gallery",
                "A long sheltered wall is covered floor to unreachable ceiling in spiraling petroglyphs - suns, beasts, falling stars, and figures with too many arms - a history or a warning pecked into the rock by hands no one remembers. North and down.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Sky-Altar Approach",
                "The trail narrows toward the summit along a knife-edge of red stone, the world falling away on both sides into blue distance, the wind shoving at you with real intent, and ahead the flat crown of the highest mesa waits open to the whole roaring sky. North and south.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Roof of the World",
                "The trail tops out on the flat summit of the highest mesa, an altar-stone at its center and nothing above but sky, and as your shadow falls across the altar the Thunderbird stoops from the sun itself, vast and crackling, to defend the roof of the world. The only way down is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a cliff-stalking puma",
        781,
        58,
        13,
        66,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Lightning)),
    );
    mob(
        spawns,
        "a hoodoo rock-wight",
        784,
        66,
        13,
        72,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Poison), Some(D::Frost)),
    );
    mob(
        spawns,
        "a storm-touched roc",
        786,
        72,
        14,
        80,
        false,
        COMMON_LOOT,
        p(D::Lightning, Some(D::Lightning), Some(D::Frost)),
    );
    mob(
        spawns,
        "the Thunderbird",
        last,
        330,
        25,
        450,
        true,
        &[1008, 1118, 1205, 1304],
        p(D::Lightning, Some(D::Lightning), Some(D::Frost)),
    );
}

// ---- The loot tables the region generators draw from ---------------------

/// Common low-tier drop pool shared by wandering wing mobs.
const COMMON_LOOT: &[u32] = &[1000, 1100, 1103, 1300];
const CATACOMBS_COMMON_LOOT: &[u32] = &[1301, 1302, super::items::CATACOMBS_RELIC_ID];
const CATACOMBS_BOSS_LOOT: &[u32] = &[
    super::items::BONEWRIGHT_SCEPTER_ID,
    super::items::CRYPT_SAINT_COIF_ID,
    super::items::RELIQUARY_SIGIL_ID,
    1304,
    1305,
    super::items::CATACOMBS_RELIC_ID,
];
const THORNWOOD_COMMON_LOOT: &[u32] = &[1301, 1302, super::items::THORNWOOD_RELIC_ID];
const THORNWOOD_BOSS_LOOT: &[u32] = &[
    super::items::HEARTWOOD_THORNBLADE_ID,
    super::items::THORNHIDE_GRIPS_ID,
    super::items::HEART_TREE_CHARM_ID,
    1304,
    1305,
    super::items::THORNWOOD_RELIC_ID,
];
const CAVERNS_COMMON_LOOT: &[u32] = &[1301, 1302, super::items::CAVERNS_RELIC_ID];
const CAVERNS_BOSS_LOOT: &[u32] = &[
    super::items::ABYSSAL_HARPOON_ID,
    super::items::TIDEBLACK_CARAPACE_ID,
    super::items::DEEPCURRENT_BAND_ID,
    1304,
    1305,
    super::items::CAVERNS_RELIC_ID,
];

#[cfg(test)]
#[path = "world_test.rs"]
mod world_test;
