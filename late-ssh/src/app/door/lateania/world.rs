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

use super::damage::{DamageProfile, DamageType};

/// Compass (with diagonals and vertical) directions a player can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    South,
    East,
    West,
    Northeast,
    Northwest,
    Southeast,
    Southwest,
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
            Self::Northeast => "northeast",
            Self::Northwest => "northwest",
            Self::Southeast => "southeast",
            Self::Southwest => "southwest",
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
            Self::Northeast => "ne",
            Self::Northwest => "nw",
            Self::Southeast => "se",
            Self::Southwest => "sw",
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
            Self::Northeast => Self::Southwest,
            Self::Southwest => Self::Northeast,
            Self::Northwest => Self::Southeast,
            Self::Southeast => Self::Northwest,
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
            Self::Northeast => (1, -1),
            Self::Northwest => (-1, -1),
            Self::Southeast => (1, 1),
            Self::Southwest => (-1, 1),
            Self::Up | Self::Down => return None,
        })
    }
}

pub type RoomId = u32;

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
    /// A displayed level, derived from the mob's vitality and bite so it scales
    /// naturally across the whole roster without authoring a level per spawn.
    pub fn level(&self) -> i32 {
        ((self.max_hp + self.damage * 4) / 14).clamp(1, 60)
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

/// The immutable world: every room plus the mob roster.
#[derive(Clone, Debug)]
pub struct World {
    pub rooms: HashMap<RoomId, Room>,
    pub spawns: Vec<MobSpawn>,
    pub start_room: RoomId,
}

impl World {
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }

    /// Build an overhead minimap centred on `current`, spanning `hr` rooms east
    /// and west and `vr` rooms north and south. Visited rooms are drawn solid;
    /// an unvisited room one step from a drawn room becomes a faint frontier
    /// marker so the player can see where there is still to explore. Up/down
    /// exits can't be placed on a flat plane, so they're reported as flags.
    pub fn minimap(
        &self,
        current: RoomId,
        previous: Option<RoomId>,
        visited: &HashSet<RoomId>,
        hr: i32,
        vr: i32,
    ) -> MiniMap {
        // 1. Lay visited rooms onto an integer grid by walking exits out from the
        //    current room. BFS, so the shortest path to each room wins any clash
        //    that the world's non-Euclidean geometry might otherwise create.
        let mut coords: HashMap<RoomId, (i32, i32)> = HashMap::new();
        coords.insert(current, (0, 0));
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
                if !visited.contains(&dest) || coords.contains_key(&dest) {
                    continue;
                }
                coords.insert(dest, (nx, ny));
                queue.push_back(dest);
            }
        }

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
                draw_connector(&mut grid[(r + nr) / 2][(c + nc) / 2], dx, dy);
                // A corridor leaving the visited set points at somewhere new.
                if !coords.contains_key(&dest) && grid[nr][nc] == MapCell::Empty {
                    grid[nr][nc] = MapCell::Frontier;
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
    /// A `/` corridor (northeast/southwest).
    ConnSlash,
    /// A `\` corridor (northwest/southeast).
    ConnBack,
    /// Where two diagonal corridors cross (`X`).
    ConnCross,
    /// Highlighted connector from the previous room to the current room.
    TrailH,
    TrailV,
    TrailSlash,
    TrailBack,
    TrailCross,
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

/// Lay a corridor glyph into a connector cell, merging crossing diagonals into
/// an `X`. Room cells and matching prior corridors are left untouched.
fn draw_connector(cell: &mut MapCell, dx: i32, dy: i32) {
    let drawn = if dx == 0 {
        MapCell::ConnV
    } else if dy == 0 {
        MapCell::ConnH
    } else if dx == dy {
        MapCell::ConnBack
    } else {
        MapCell::ConnSlash
    };
    *cell = match (*cell, drawn) {
        (MapCell::Empty, glyph) => glyph,
        (MapCell::ConnSlash, MapCell::ConnBack) | (MapCell::ConnBack, MapCell::ConnSlash) => {
            MapCell::ConnCross
        }
        (existing, _) => existing,
    };
}

fn draw_trail_connector(cell: &mut MapCell, dx: i32, dy: i32) {
    let drawn = if dx == 0 {
        MapCell::TrailV
    } else if dy == 0 {
        MapCell::TrailH
    } else if dx == dy {
        MapCell::TrailBack
    } else {
        MapCell::TrailSlash
    };
    *cell = match (*cell, drawn) {
        (_, glyph @ (MapCell::TrailH | MapCell::TrailV)) => glyph,
        (MapCell::TrailSlash, MapCell::TrailBack)
        | (MapCell::TrailBack, MapCell::TrailSlash)
        | (MapCell::ConnSlash, MapCell::TrailBack)
        | (MapCell::ConnBack, MapCell::TrailSlash)
        | (MapCell::ConnCross, _) => MapCell::TrailCross,
        (_, glyph) => glyph,
    };
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

/// What kind of lookable thing a feature is. Fountains restore vitals in a safe
/// capital; the rest are pure description revealed on look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureKind {
    Scenery,
    Fountain,
    Plaque,
    Vista,
}

impl FeatureKind {
    /// Short tag shown beside the feature in the Examine panel.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Scenery => "",
            Self::Fountain => "fountain",
            Self::Plaque => "plaque",
            Self::Vista => "vista",
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

/// Every lookable feature in the world, keyed to the room it stands in.
pub const FEATURES: &[Feature] = &[
    // ---- Embergate (the town square: recall point + safe haven) ---------
    feat(
        1,
        "the town well",
        FeatureKind::Fountain,
        EMBERGATE_WELL_DESC,
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
];

pub fn features_at(room: RoomId) -> Vec<&'static Feature> {
    FEATURES.iter().filter(|f| f.room == room).collect()
}

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
];

pub fn critters_at(room: RoomId) -> Vec<&'static CritterSpawn> {
    WILDLIFE.iter().filter(|c| c.home == room).collect()
}

/// Stable global index of a critter (its position in `WILDLIFE`), used to key
/// per-world hunt cooldowns.
pub fn critter_index(c: &CritterSpawn) -> Option<usize> {
    WILDLIFE.iter().position(|w| std::ptr::eq(w, c))
}

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
             board leans by the well, thick with bounties and lost-cat pleas alike. \
             The Gilded Flagon glows north, the temple west, Market Row east, and the \
             South Gate and open road lie south.",
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
             last of those rarer than the others. The square lies south.",
            &[(Dir::South, 1)],
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
             grave, patient mercy fills the air. The square lies east.",
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
             crypt runs south.",
            &[(Dir::Up, 50), (Dir::South, 52)],
        ),
        room(
            52,
            "Drowned Crypts - The Sarcophagus Row",
            "Drowned Crypts",
            false,
            "Stone coffins line both walls, their lids carved with the serene faces \
             of the long-dead. Several lids lie aside in the water. The faces beneath \
             are no longer serene. Ways lead north, south, and east.",
            &[(Dir::North, 51), (Dir::South, 53), (Dir::East, 54)],
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
             that you feel in your teeth more than hear. North and south.",
            &[(Dir::North, 57), (Dir::South, 60)],
        ),
        room(
            60,
            "Drowned Crypts - The Sunken Crossing",
            "Drowned Crypts",
            false,
            "Submerged steps lead up onto a broad landing where three flooded halls \
             converge, their arches reflected in the still water until you cannot \
             tell stone from its double. Ways lead north, south, and east.",
            &[(Dir::North, 59), (Dir::South, 61), (Dir::East, 62)],
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
            "The flood drains away up a stair cut from raw red stone, and the air \
             changes utterly: drier, sharper, carrying the faraway tang of smoke and \
             hot metal. Something deep in the rock is awake and burning. North \
             returns to the crypts; up climbs toward the heat.",
            &[(Dir::North, 64), (Dir::Up, 66)],
        ),
        // ---- Emberpeak Mines (fire & dwarven ruin, tier 5-6) ------------
        room(
            66,
            "Emberpeak Mines - The Cinder Gate",
            "Emberpeak Mines",
            false,
            "You climb into a hewn hall where the very walls hold a sullen red \
             warmth, and runes carved by long-vanished dwarves still glow faintly in \
             the heat. Down leads back to the cold crypts; the mines open north.",
            &[(Dir::Down, 65), (Dir::North, 67)],
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
             ground glass. Down returns to the warm dark; the ascent climbs north.",
            &[(Dir::Down, 80), (Dir::North, 82)],
        ),
        room(
            82,
            "Frostspire Ascent - The Wind-Carved Pass",
            "Frostspire Ascent",
            false,
            "The path threads a pass where the wind has sculpted the ice into a \
             gallery of blades and figures, frozen courtiers bowing eternally to a \
             gale that never tires of them. Ways lead south, north, and east.",
            &[(Dir::South, 81), (Dir::North, 83), (Dir::East, 84)],
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
            &[(Dir::South, 94), (Dir::Up, 96)],
        ),
        // ---- The Sunken Citadel (megadungeon, tier 7-8) -----------------
        room(
            96,
            "The Sunken Citadel - The Hall of Entry",
            "The Sunken Citadel",
            false,
            "You pass from ice into a hall of black stone so vast the lantern cannot \
             find its roof, and the cold here is not winter's cold but something \
             older and more deliberate. The gate is down and behind; the citadel \
             opens north.",
            &[(Dir::Down, 95), (Dir::North, 97)],
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
            loot: &[1006, 1201, 1301],
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
            loot: &[1105, 1202, 1302],
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
            loot: &[1008, 1204, 1302],
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
            loot: &[1009, 1205, 1304],
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
            loot: &[1007, 1205, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // ---- The Sunken Citadel (tier 7-8) ------------------------------
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
                Some(DamageType::Physical),
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
                Some(DamageType::Physical),
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
            loot: &[1109, 1202, 1304],
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
        // Final boss
        MobSpawn {
            id: 73,
            name: "the Archdemon Mal'gareth",
            home: 110,
            max_hp: 800,
            damage: 48,
            xp: 1500,
            respawn_secs: 600,
            loot: &[1009, 1205, 1401],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
    ];

    let mut rooms: HashMap<RoomId, Room> = rooms.into_iter().map(|r| (r.id, r)).collect();
    let mut spawns = spawns;

    // Append the deeper-exploration wings (rooms 300+), reciprocal by construction.
    extend_world(&mut rooms, &mut spawns);

    // Append the overworld: 100 rooms of new biomes and the three capital cities
    // (rooms 600+), reachable from Embergate's South Gate.
    extend_overworld(&mut rooms, &mut spawns);

    // Append the Frontier: 500 procedurally-composed rooms across ten themed
    // zones (rooms 2000+), hung off Embergate and populated with the 40-type
    // frontier roster and generated loot.
    extend_frontier(&mut rooms, &mut spawns);

    World {
        rooms,
        spawns,
        start_room: 1,
    }
}

// ---- The Frontier (procedural expansion) --------------------------------
//
// Ten themed zones, each a 10x5 grid of 50 rooms, chained one below the next and
// hung off Embergate's square. Rooms, names and descriptions are composed
// deterministically from per-zone flavour and leaked to 'static (the world is
// built once at startup). Each zone fields three regular mob types and a boss
// (40 types total); loot is the generated frontier catalog for that tier.

const FRONTIER_BASE: RoomId = 2000;
const FRONTIER_W: u32 = 10;
const FRONTIER_H: u32 = 5;
const FRONTIER_ZONES: usize = FRONTIER_ZONES_DATA.len();

/// Per-zone flavour: name, adjective, ground noun, a landmark feature, the
/// creatures that haunt it, three regular mob names, and the zone boss.
#[allow(clippy::type_complexity)]
const FRONTIER_ZONES_DATA: [(&str, &str, &str, &str, &str, [&str; 3], &str); 20] = [
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
        "The Hollow Crown",
        "god-haunted",
        "starless black marble",
        "the broken throne of a dead god",
        "crown-wights",
        ["Wight Sentinel", "Pale Regent", "Throne Shade"],
        "the King Who Was Promised Nothing",
    ),
];

/// Number of Frontier zones — and so the number of zone quests (slay each boss).
pub fn frontier_zone_count() -> usize {
    FRONTIER_ZONES_DATA.len()
}

/// The display name and boss name of Frontier zone `z`.
pub fn frontier_zone_info(z: usize) -> Option<(&'static str, &'static str)> {
    FRONTIER_ZONES_DATA.get(z).map(|d| (d.0, d.6))
}

/// The Frontier zone whose boss bears this name, if any — used to credit a
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
    let mut spawn_id: u32 = 900_000;

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
                        exits: exits.into_iter().collect(),
                    },
                );

                if is_entrance {
                    continue; // a safe waystation, no foes
                }
                if is_boss_room {
                    let ti = tier as i32;
                    spawns.push(MobSpawn {
                        id: spawn_id,
                        name: boss,
                        home: id,
                        max_hp: 120 + ti * 60,
                        damage: 8 + ti * 3,
                        xp: 200 + ti * 80,
                        respawn_secs: 600,
                        loot: super::items::frontier_loot(z),
                        boss: true,
                        profile: DamageProfile::new(DamageType::Physical, None, None),
                    });
                    spawn_id += 1;
                } else if idx.is_multiple_of(2) {
                    let ti = tier as i32;
                    spawns.push(MobSpawn {
                        id: spawn_id,
                        name: mob_names[(idx as usize) % 3],
                        home: id,
                        max_hp: 30 + ti * 15,
                        damage: 4 + ti * 2,
                        xp: 25 + ti * 12,
                        respawn_secs: 90,
                        loot: super::items::frontier_loot(z),
                        boss: false,
                        profile: DamageProfile::new(DamageType::Physical, None, None),
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
    let portal = [
        Dir::Down,
        Dir::Up,
        Dir::Northeast,
        Dir::Northwest,
        Dir::Southeast,
        Dir::Southwest,
    ]
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
/// opposite back to `from`. Never overwrites an existing exit.
fn link(rooms: &mut HashMap<RoomId, Room>, from: RoomId, dir: Dir, to: RoomId) {
    if let Some(r) = rooms.get_mut(&from) {
        r.exits.entry(dir).or_insert(to);
    }
    if let Some(r) = rooms.get_mut(&to) {
        r.exits.entry(dir.opposite()).or_insert(from);
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
        Dir::North,
        start,
        &[
            wr(
                "Whisperwood - The Mushroom Stair",
                "Shelves of bracket-fungus climb a steep slope like a giant's staircase, soft and cold and faintly yielding underfoot, and a slow rain of spores drifts down through the lanternlight to settle on your shoulders. The deeper air tastes of loam and rot and something sweeter beneath. The stair leads north, and the standing-stone ring lies back south.",
                Dir::North,
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
        &[1006, 1201, 1302],
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
        &[1105, 1202, 1302],
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
        &[1008, 1204, 1302],
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
        p(D::Fire, Some(D::Physical), Some(D::Frost)),
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
        &[1009, 1205, 1304],
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
        p(D::Frost, Some(D::Physical), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Heart-of-Winter Wyrm",
        last,
        440,
        33,
        620,
        true,
        &[1007, 1205, 1304],
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
        p(D::Shadow, Some(D::Physical), Some(D::Holy)),
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
        p(D::Shadow, Some(D::Physical), Some(D::Holy)),
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
        p(D::Physical, Some(D::Physical), Some(D::Arcane)),
    );
    mob(
        spawns,
        "the Warden of the Sealed Heart",
        last,
        540,
        39,
        840,
        true,
        &[1109, 1202, 1304],
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
        &[1009, 1205, 1401],
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
        &[1006, 1201, 1301],
        DamageProfile::physical(),
    );
}

/// Common low-tier drop pool shared by wandering wing mobs.
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
        p(D::Physical, Some(D::Physical), Some(D::Fire)),
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
                "The northbound track ends at the sea-gate of Tasmania, and the city opens before you all at once: white-walled and red-roofed, tumbling down its hill to a harbor crowded with masts, loud with gulls and ship-chandlers and the bargaining of a hundred tongues. At the square's heart a great tiered fountain catches the sea-light, and a bronze plaque is set into the harbor wall beside it. Streets climb north into the city, and the Greatroad lies back south.",
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
                "A narrow path clings to the chalk cliff above a sheer drop where the sea breaks white on black rocks a hundred feet below, and the wind comes off the water hard enough to lean your whole weight against. Seabirds wheel and scream from their nests in the cliff-face, loudly resentful of the company. The path runs east, and Tasmania lies west.",
                Dir::East,
            ),
            wr(
                "The Sapphire Coast - The Smuggler's Cove",
                "A hidden cove opens at the foot of a treacherous goat-track, its shingle beach littered with the grey ribs of wrecked boats and, higher up the strand, the cold ashes and stacked kegs of folk who do their trading strictly by moonlight. The tide is out, and the sea-caves gape black and dripping. East and west.",
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
        p(D::Physical, Some(D::Physical), Some(D::Lightning)),
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
        &[1008, 1205, 1302],
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
                "The mountain track climbs at last into Melvanala, a city of grey stone and blue slate terraced up the steeps above a vast and utterly still mountain lake. Woodsmoke and the sharp scent of pine-resin hang in the thin bright air, and at the heart of the lakeshore square a tiered fountain murmurs beside a bronze plaque set into the old retaining wall. Stairs climb north into the city, and the Greatroad track falls away south.",
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
                "A grassy path winds east through high rolling pasture, dotted with the small dark shapes of grazing yaks and the occasional stone cairn raised by herders to mark the way through the fog that rolls in without warning. Skylarks burst up singing from beneath your very boots. East, and Melvanala lies west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Gentian Meadow",
                "A meadow of deep-blue gentian and nodding white edelweiss spills down the hillside in a sweep of color so intense it looks painted, loud with bees and the click of grasshoppers in the warm grass. A lone shepherd's flute carries faintly from somewhere out of sight. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Standing Stones",
                "A ring of moss-furred standing stones crowns a green hill, far older than any herder's memory, and the sheep will not graze within the circle no matter how rich the grass grows there. The wind drops oddly still as you step inside. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Thundering Falls",
                "A river throws itself off a high green shelf in a white roar of spray, and the path crosses behind the falling water on a slick ledge where the whole world becomes noise and cold rainbow mist. The rock is treacherous and the drop is long. East and west.",
                Dir::East,
            ),
            wr(
                "The Verdant Highlands - The Heather Moor",
                "The grass gives way to a vast purple moor of springy heather and black peat-pools, stretching to every horizon under a sky full of racing cloud-shadow. Curlews call their lonely falling cry, and the wind never once stops moving over the open land. East and west.",
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
        &[1007, 1202, 1304],
        p(D::Physical, Some(D::Frost), Some(D::Fire)),
    );

    // ---- The Mistfen (9 rooms): drowned marsh south of the Highlands ----
    let last = add_wing(
        rooms,
        "The Mistfen",
        false,
        686,
        Dir::South,
        700,
        &[
            wr(
                "The Mistfen - The Sinking Path",
                "The firm highland turf rots away southward into a treacherous fen of black water and floating sedge, where a path of half-sunk logs offers the only footing and a cold white mist drinks the sound right out of the air. Something plops into the water just out of sight. South, and the hills lie north.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Reed Labyrinth",
                "Walls of reed twice your height close in on every side, channels of still brown water branching and rejoining until the world shrinks to mud, mist, and the rustle of unseen things parting the stems ahead of you. Direction becomes a matter of faith. North and south.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Drowned Village",
                "The peaked roofs of a sunken village break the surface of the fen, their windows full of black water, a church spire leaning at a drunken angle with its bell still hung and waiting. The mist hangs a single rope of woodsmoke that has no fire to come from. North and south.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Will-o'-Wisp Mire",
                "Pale lights drift and bob across the deep mire, beautiful and patient, each one hovering just over the worst of the sucking mud, each one promising firm ground that is not there at all. They brighten, hopefully, as you draw near. North and south.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Bog-Body Barrow",
                "A low island of slightly firmer peat holds an ancient barrow, and the black bog has kept its dead so perfectly that the faces pressing up through the surface still wear their final expressions of surprise. The peat sighs and shifts as if breathing. North and south.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Leech-Black Pool",
                "The path skirts a pool so utterly black and still it might be a hole cut clean through the world, and the things that live in it - long, soft, and far too many - lift the surface in slow ripples that all turn, somehow, toward you. North and south.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Hag's Causeway",
                "A causeway of mortared skulls, white and grinning, lifts the path above the deepest fen, and at its midpoint a wicker idol leans over the water, freshly garlanded by hands that did not love what they were appeasing. A way leads down through a sinkhole here. North, south, and down.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Sunken Cathedral",
                "A vast drowned cathedral rears from the mire, three-quarters swallowed, its remaining stained glass casting drowned and broken colors across the water, and from within comes the slow drip and the slower, deliberate sound of something very large turning over. North and south.",
                Dir::South,
            ),
            wr(
                "The Mistfen - The Marsh-Mother's Hollow",
                "The fen opens into a stagnant lagoon ringed by dead willows, and from its center, draped in weed and rising water, the Marsh-Mother lifts her ancient drowned head and opens arms enough to gather in the whole foolish world. The only way back is north.",
                Dir::South,
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
        &[1109, 1204, 1302],
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
        p(D::Poison, Some(D::Physical), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Heart-Spore",
        last,
        310,
        22,
        400,
        true,
        &[1008, 1205, 1304],
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
                "The caravan road climbs a last dune and Matlatesh stands revealed in the bowl of its oasis: a city of honey-colored mud-brick and palm shade, its wind-towers reaching up to catch the desert breeze, its streets cool and dim and smelling of cardamom and dust. A great tiered fountain spills at the square's heart, fed by the blessed spring, and a bronze plaque is set in the shaded wall beside it. Lanes run west into the city, and the desert road lies east.",
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
        &[1009, 1205, 1401],
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
        p(D::Shadow, Some(D::Physical), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Maned Terror",
        last,
        320,
        24,
        430,
        true,
        &[1007, 1202, 1304],
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
                "The trail passes beneath a ledge heaped with an enormous nest of whole tree-trunks and sun-bleached bones, and the very rock is scorched in long forking patterns, for this is the eyrie of the thunderbird, and the sky to the north growls in warning. North and south.",
                Dir::North,
            ),
            wr(
                "The Skyreach Mesas - The Petroglyph Gallery",
                "A long sheltered wall is covered floor to unreachable ceiling in spiraling petroglyphs - suns, beasts, falling stars, and figures with too many arms - a history or a warning pecked into the rock by hands no one remembers. North and south.",
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
        p(D::Physical, Some(D::Physical), Some(D::Frost)),
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
        &[1008, 1205, 1304],
        p(D::Lightning, Some(D::Lightning), Some(D::Frost)),
    );
}

/// Common low-tier drop pool shared by wandering wing mobs.
const COMMON_LOOT: &[u32] = &[1000, 1100, 1103, 1300];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exit_resolves_to_a_real_room() {
        let world = seed_world();
        for room in world.rooms.values() {
            for (dir, target) in &room.exits {
                assert!(
                    world.rooms.contains_key(target),
                    "room {} ({}) has a {} exit to missing room {}",
                    room.id,
                    room.name,
                    dir.label(),
                    target
                );
            }
        }
    }

    #[test]
    fn exits_are_reciprocal_where_expected() {
        // Embergate square (1) <-> south gate (5): going south then north returns.
        let world = seed_world();
        let square = world.room(1).expect("square exists");
        let gate_id = square.exits.get(&Dir::South).copied().expect("south exit");
        let gate = world.room(gate_id).expect("gate exists");
        assert_eq!(gate.exits.get(&Dir::North).copied(), Some(1));
    }

    #[test]
    fn start_room_exists_and_is_safe() {
        let world = seed_world();
        let start = world.room(world.start_room).expect("start room exists");
        assert!(start.safe, "players should spawn in a safe room");
    }

    #[test]
    fn world_has_expected_size_and_every_mob_homes_to_a_real_room() {
        let world = seed_world();
        // 198 base + extension rooms, the 100 overworld rooms, and the 1000
        // procedural Frontier rooms (20 zones × 50, rooms 2000+).
        assert_eq!(world.rooms.len(), 1298, "expected 1298 rooms");
        for spawn in &world.spawns {
            assert!(
                world.rooms.contains_key(&spawn.home),
                "mob {} ({}) homes to missing room {}",
                spawn.id,
                spawn.name,
                spawn.home
            );
        }
    }

    #[test]
    fn there_are_at_least_fifty_distinct_enemy_types() {
        let world = seed_world();
        let mut names: Vec<&str> = world.spawns.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert!(
            names.len() >= 50,
            "expected 50+ distinct enemy types, found {}",
            names.len()
        );
    }

    #[test]
    fn mob_spawn_ids_are_unique() {
        let world = seed_world();
        let mut ids: Vec<u32> = world.spawns.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(count, ids.len(), "duplicate mob spawn id");
    }

    #[test]
    fn every_boss_has_a_guaranteed_loot_table() {
        let world = seed_world();
        let bosses: Vec<_> = world.spawns.iter().filter(|s| s.boss).collect();
        assert!(bosses.len() >= 7, "expected at least 7 zone bosses");
        for boss in bosses {
            assert!(!boss.loot.is_empty(), "boss {} has no loot", boss.name);
            for id in boss.loot {
                assert!(
                    crate::app::door::lateania::items::item(*id).is_some(),
                    "boss {} drops missing item {}",
                    boss.name,
                    id
                );
            }
        }
    }

    #[test]
    fn all_mob_loot_references_real_items() {
        let world = seed_world();
        for spawn in &world.spawns {
            for id in spawn.loot {
                assert!(
                    crate::app::door::lateania::items::item(*id).is_some(),
                    "mob {} drops missing item {}",
                    spawn.name,
                    id
                );
            }
        }
    }

    #[test]
    fn every_room_reachable_from_start() {
        let world = seed_world();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![world.start_room];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(room) = world.room(id) {
                for target in room.exits.values() {
                    stack.push(*target);
                }
            }
        }
        assert_eq!(
            seen.len(),
            world.rooms.len(),
            "some rooms are unreachable from the start room"
        );
    }

    #[test]
    fn overworld_adds_one_hundred_new_rooms() {
        let world = seed_world();
        // The overworld occupies ids 600..2000; the Frontier starts at 2000.
        let new_rooms = world
            .rooms
            .keys()
            .filter(|id| (600..2000).contains(*id))
            .count();
        assert_eq!(
            new_rooms, 100,
            "expected exactly 100 new overworld rooms (600-1999)"
        );
    }

    #[test]
    fn every_room_has_a_paragraph_description() {
        // "A paragraph of detail" - every authored room reads as real prose, not
        // a stub. The bar is a minimum length plus more than one sentence.
        const MIN_CHARS: usize = 180;
        let world = seed_world();
        let mut short: Vec<(RoomId, usize)> = world
            .rooms
            .values()
            .filter(|r| {
                let len = r.desc.chars().count();
                let sentences = r.desc.matches(['.', '!', '?']).count();
                len < MIN_CHARS || sentences < 2
            })
            .map(|r| (r.id, r.desc.chars().count()))
            .collect();
        short.sort_unstable();
        assert!(
            short.is_empty(),
            "{} room(s) lack a paragraph-length description: {:?}",
            short.len(),
            short
        );
    }

    #[test]
    fn frontier_quests_map_each_boss_back_to_its_zone() {
        assert_eq!(frontier_zone_count(), 20);
        for z in 0..frontier_zone_count() {
            let (_zname, boss) = frontier_zone_info(z).expect("zone exists");
            assert_eq!(
                frontier_zone_of_boss(boss),
                Some(z),
                "boss {boss} should credit zone {z}"
            );
        }
        assert_eq!(frontier_zone_of_boss("not a boss"), None);
    }

    #[test]
    fn town_and_capitals_have_wildlife() {
        assert!(!critters_at(1).is_empty(), "the town square has wildlife");
        assert!(
            critters_at(1)
                .iter()
                .any(|c| matches!(c.kind, CritterKind::Boon(_))),
            "a boon creature lives in the town square"
        );
        assert!(
            WILDLIFE.iter().any(|c| c.kind == CritterKind::Game),
            "small game lives out in the wilds"
        );
    }

    #[test]
    fn town_square_has_a_recall_fountain() {
        // The recall destination carries a healing fountain, and room 1 is safe
        // so the fountain actually restores vitals.
        assert!(
            features_at(1)
                .iter()
                .any(|f| f.kind == FeatureKind::Fountain),
            "the town square needs a fountain"
        );
        assert!(seed_world().room(1).expect("town square exists").safe);
    }

    #[test]
    fn every_capital_has_a_fountain_and_a_plaque() {
        let world = seed_world();
        for square in [TASMANIA_SQUARE, MELVANALA_SQUARE, MATLATESH_SQUARE] {
            let room = world.room(square).expect("capital square exists");
            assert!(room.safe, "capital {square} must be a safe haven");
            let feats = features_at(square);
            assert!(
                feats.iter().any(|f| f.kind == FeatureKind::Fountain),
                "capital {square} has no healing fountain"
            );
            assert!(
                feats.iter().any(|f| f.kind == FeatureKind::Plaque),
                "capital {square} has no dedication plaque"
            );
        }
    }

    #[test]
    fn every_feature_lives_in_a_real_room() {
        let world = seed_world();
        for feature in FEATURES {
            assert!(
                world.rooms.contains_key(&feature.room),
                "feature {:?} references missing room {}",
                feature.name,
                feature.room
            );
        }
    }

    #[test]
    fn minimap_centres_on_the_player_and_reveals_frontiers() {
        let world = seed_world();
        let start = world.start_room;
        // Only the start room is visited: it sits dead centre, and at least one
        // unexplored exit shows up as a frontier marker.
        let visited = HashSet::from([start]);
        let map = world.minimap(start, None, &visited, 3, 2);
        let centre = (map.grid.len() / 2, map.grid[0].len() / 2);
        assert_eq!(map.grid[centre.0][centre.1], MapCell::Current);
        let frontiers = map
            .grid
            .iter()
            .flatten()
            .filter(|c| **c == MapCell::Frontier)
            .count();
        assert!(
            frontiers >= 1,
            "the start room should reveal somewhere to go"
        );
    }

    #[test]
    fn minimap_draws_a_corridor_between_visited_rooms() {
        let world = seed_world();
        let start = world.start_room;
        let neighbour = world
            .room(start)
            .unwrap()
            .exits
            .iter()
            .filter(|(dir, _)| dir.delta_2d().is_some())
            .map(|(_, dest)| *dest)
            .next()
            .expect("start has a planar exit");
        let visited = HashSet::from([start, neighbour]);
        let map = world.minimap(start, None, &visited, 3, 2);
        let visited_cells = map
            .grid
            .iter()
            .flatten()
            .filter(|c| **c == MapCell::Visited)
            .count();
        assert!(visited_cells >= 1, "the visited neighbour should be drawn");
        let corridors = map
            .grid
            .iter()
            .flatten()
            .filter(|c| {
                matches!(
                    **c,
                    MapCell::ConnH | MapCell::ConnV | MapCell::ConnSlash | MapCell::ConnBack
                )
            })
            .count();
        assert!(corridors >= 1, "a corridor should join the two rooms");
    }

    #[test]
    fn minimap_marks_previous_room_and_trail() {
        let world = seed_world();
        let start = world.start_room;
        let previous = world
            .room(start)
            .unwrap()
            .exits
            .iter()
            .filter(|(dir, _)| dir.delta_2d().is_some())
            .map(|(_, dest)| *dest)
            .next()
            .expect("start has a planar exit");
        let visited = HashSet::from([start, previous]);

        let map = world.minimap(start, Some(previous), &visited, 3, 2);

        assert!(
            map.grid.iter().flatten().any(|c| *c == MapCell::Previous),
            "the room just left should be marked"
        );
        assert!(
            map.grid.iter().flatten().any(|c| matches!(
                *c,
                MapCell::TrailH
                    | MapCell::TrailV
                    | MapCell::TrailSlash
                    | MapCell::TrailBack
                    | MapCell::TrailCross
            )),
            "the route from previous room to current room should be highlighted"
        );
    }
}
