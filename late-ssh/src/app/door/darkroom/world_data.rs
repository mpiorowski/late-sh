/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The tables and
 * prose below are transcribed from `script/world.js` and `script/path.js`.
 * See LICENSING.md and NOTICE. */

//! The wasteland's balance data: the tile set, the landmark table, the weapon
//! table, the outfitting weights and capacities, and every constant the world
//! rules step against. Pure tables, like `data`; the rules live in `world`.

use serde::{Deserialize, Serialize};

use super::data::{Perk, Resource};

/// Half the map's width. The grid is `RADIUS * 2 + 1` square (61x61).
pub const RADIUS: i32 = 30;

/// The village sits at the exact centre, and every trip starts and ends there.
pub const VILLAGE_POS: (i32, i32) = (RADIUS, RADIUS);

/// How strongly a tile copies its neighbours during generation.
pub const STICKINESS: f64 = 0.5;

/// How far the wanderer's lantern reaches (doubled by the scout perk).
pub const LIGHT_RADIUS: i32 = 2;

pub const BASE_WATER: i64 = 10;
pub const BASE_HEALTH: i64 = 10;
pub const BASE_HIT_CHANCE: f64 = 0.8;
pub const MOVES_PER_FOOD: u32 = 2;
pub const MOVES_PER_WATER: u32 = 1;
pub const MEAT_HEAL: i64 = 8;
pub const MEDS_HEAL: i64 = 20;
/// At least three moves between fights.
pub const FIGHT_DELAY: u32 = 3;
pub const FIGHT_CHANCE: f64 = 0.20;
/// Seconds before another expedition may set out after a death.
pub const DEATH_COOLDOWN: u32 = 120;
/// Seconds of cooldown on the eat and medicine buttons in a fight.
pub const EAT_COOLDOWN: f64 = 5.0;
pub const MEDS_COOLDOWN: f64 = 7.0;
/// Seconds a bolas keeps an enemy tangled.
pub const STUN_DURATION: f64 = 4.0;
/// Seconds before the leave button is live after a fight.
pub const LEAVE_COOLDOWN: f64 = 1.0;

/// Terrain probabilities. These three must sum to one.
pub const FOREST_PROB: f64 = 0.15;
pub const FIELD_PROB: f64 = 0.35;
pub const BARRENS_PROB: f64 = 0.5;

/// What a square of the map holds. Upstream keys the grid by a single
/// character; the glyphs are kept as the save format, but the rules match on
/// this closed set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tile {
    Village,
    IronMine,
    CoalMine,
    SulphurMine,
    Forest,
    Field,
    Barrens,
    Road,
    House,
    Cave,
    Town,
    City,
    Outpost,
    Ship,
    Borehole,
    Battlefield,
    Swamp,
}

impl Tile {
    pub const ALL: [Tile; 17] = [
        Tile::Village,
        Tile::IronMine,
        Tile::CoalMine,
        Tile::SulphurMine,
        Tile::Forest,
        Tile::Field,
        Tile::Barrens,
        Tile::Road,
        Tile::House,
        Tile::Cave,
        Tile::Town,
        Tile::City,
        Tile::Outpost,
        Tile::Ship,
        Tile::Borehole,
        Tile::Battlefield,
        Tile::Swamp,
    ];

    pub fn glyph(self) -> char {
        match self {
            Tile::Village => 'A',
            Tile::IronMine => 'I',
            Tile::CoalMine => 'C',
            Tile::SulphurMine => 'S',
            Tile::Forest => ';',
            Tile::Field => ',',
            Tile::Barrens => '.',
            Tile::Road => '#',
            Tile::House => 'H',
            Tile::Cave => 'V',
            Tile::Town => 'O',
            Tile::City => 'Y',
            Tile::Outpost => 'P',
            Tile::Ship => 'W',
            Tile::Borehole => 'B',
            Tile::Battlefield => 'F',
            Tile::Swamp => 'M',
        }
    }

    /// The tile a saved glyph stands for. An unknown glyph is barrens: a save
    /// written by a future version must never strand a session.
    pub fn from_glyph(glyph: char) -> Tile {
        Tile::ALL
            .into_iter()
            .find(|tile| tile.glyph() == glyph)
            .unwrap_or(Tile::Barrens)
    }

    /// Whether a landmark may be dropped here (upstream `isTerrain`).
    pub fn is_terrain(self) -> bool {
        matches!(self, Tile::Forest | Tile::Field | Tile::Barrens)
    }

    /// The odds of generating this tile, for the three that generation draws.
    pub fn terrain_prob(self) -> f64 {
        match self {
            Tile::Forest => FOREST_PROB,
            Tile::Field => FIELD_PROB,
            Tile::Barrens => BARRENS_PROB,
            _ => 0.0,
        }
    }

    /// The setpiece this tile starts, and how many are scattered where.
    pub fn landmark(self) -> Option<Landmark> {
        match self {
            Tile::Outpost => Some(Landmark {
                num: 0,
                min_radius: 0,
                max_radius: 0,
                scene: "outpost",
                label: "An Outpost",
            }),
            Tile::IronMine => Some(Landmark {
                num: 1,
                min_radius: 5,
                max_radius: 5,
                scene: "ironmine",
                label: "Iron Mine",
            }),
            Tile::CoalMine => Some(Landmark {
                num: 1,
                min_radius: 10,
                max_radius: 10,
                scene: "coalmine",
                label: "Coal Mine",
            }),
            Tile::SulphurMine => Some(Landmark {
                num: 1,
                min_radius: 20,
                max_radius: 20,
                scene: "sulphurmine",
                label: "Sulphur Mine",
            }),
            Tile::House => Some(Landmark {
                num: 10,
                min_radius: 0,
                max_radius: 45,
                scene: "house",
                label: "An Old House",
            }),
            Tile::Cave => Some(Landmark {
                num: 5,
                min_radius: 3,
                max_radius: 10,
                scene: "cave",
                label: "A Damp Cave",
            }),
            Tile::Town => Some(Landmark {
                num: 10,
                min_radius: 10,
                max_radius: 20,
                scene: "town",
                label: "An Abandoned Town",
            }),
            Tile::City => Some(Landmark {
                num: 20,
                min_radius: 20,
                max_radius: 45,
                scene: "city",
                label: "A Ruined City",
            }),
            Tile::Ship => Some(Landmark {
                num: 1,
                min_radius: 28,
                max_radius: 28,
                scene: "ship",
                label: "A Crashed Starship",
            }),
            Tile::Borehole => Some(Landmark {
                num: 10,
                min_radius: 15,
                max_radius: 45,
                scene: "borehole",
                label: "A Borehole",
            }),
            Tile::Battlefield => Some(Landmark {
                num: 5,
                min_radius: 18,
                max_radius: 45,
                scene: "battlefield",
                label: "A Battlefield",
            }),
            Tile::Swamp => Some(Landmark {
                num: 1,
                min_radius: 15,
                max_radius: 45,
                scene: "swamp",
                label: "A Murky Swamp",
            }),
            Tile::Village | Tile::Forest | Tile::Field | Tile::Barrens | Tile::Road => None,
        }
    }
}

/// A landmark's placement rules and the setpiece it runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Landmark {
    /// How many to scatter at generation. The outpost has none: outposts are
    /// made by clearing dungeons.
    pub num: u32,
    pub min_radius: i32,
    pub max_radius: i32,
    pub scene: &'static str,
    pub label: &'static str,
}

/// How a weapon is swung, which decides which perks touch it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeaponKind {
    Unarmed,
    Melee,
    Ranged,
}

/// What a hit does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Damage {
    Hits(i64),
    /// The bolas: no damage, but the enemy stops swinging for a while.
    Stun,
}

/// Everything that can be swung at something. Upstream's `World.Weapons`,
/// minus the fabricator gear this port cuts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weapon {
    Fists,
    BoneSpear,
    IronSword,
    SteelSword,
    Bayonet,
    Rifle,
    LaserRifle,
    Grenade,
    Bolas,
}

impl Weapon {
    pub const ALL: [Weapon; 9] = [
        Weapon::Fists,
        Weapon::BoneSpear,
        Weapon::IronSword,
        Weapon::SteelSword,
        Weapon::Bayonet,
        Weapon::Rifle,
        Weapon::LaserRifle,
        Weapon::Grenade,
        Weapon::Bolas,
    ];

    /// What the attack row says you are doing.
    pub fn verb(self) -> &'static str {
        match self {
            Weapon::Fists => "punch",
            Weapon::BoneSpear => "stab",
            Weapon::IronSword => "swing",
            Weapon::SteelSword => "slash",
            Weapon::Bayonet => "thrust",
            Weapon::Rifle => "shoot",
            Weapon::LaserRifle => "blast",
            Weapon::Grenade => "lob",
            Weapon::Bolas => "tangle",
        }
    }

    pub fn kind(self) -> WeaponKind {
        match self {
            Weapon::Fists => WeaponKind::Unarmed,
            Weapon::BoneSpear | Weapon::IronSword | Weapon::SteelSword | Weapon::Bayonet => {
                WeaponKind::Melee
            }
            Weapon::Rifle | Weapon::LaserRifle | Weapon::Grenade | Weapon::Bolas => {
                WeaponKind::Ranged
            }
        }
    }

    pub fn damage(self) -> Damage {
        match self {
            Weapon::Fists => Damage::Hits(1),
            Weapon::BoneSpear => Damage::Hits(2),
            Weapon::IronSword => Damage::Hits(4),
            Weapon::SteelSword => Damage::Hits(6),
            Weapon::Bayonet => Damage::Hits(8),
            Weapon::Rifle => Damage::Hits(5),
            Weapon::LaserRifle => Damage::Hits(8),
            Weapon::Grenade => Damage::Hits(15),
            Weapon::Bolas => Damage::Stun,
        }
    }

    /// Seconds before this can be used again.
    pub fn cooldown(self) -> f64 {
        match self {
            Weapon::Fists
            | Weapon::BoneSpear
            | Weapon::IronSword
            | Weapon::SteelSword
            | Weapon::Bayonet => 2.0,
            Weapon::Rifle | Weapon::LaserRifle => 1.0,
            Weapon::Grenade => 5.0,
            Weapon::Bolas => 15.0,
        }
    }

    /// What each use spends out of the pack.
    pub fn cost(self) -> Option<(Resource, i64)> {
        match self {
            Weapon::Rifle => Some((Resource::Bullets, 1)),
            Weapon::LaserRifle => Some((Resource::EnergyCell, 1)),
            Weapon::Grenade => Some((Resource::Grenade, 1)),
            Weapon::Bolas => Some((Resource::Bolas, 1)),
            Weapon::Fists
            | Weapon::BoneSpear
            | Weapon::IronSword
            | Weapon::SteelSword
            | Weapon::Bayonet => None,
        }
    }

    /// What must be in the pack to swing it at all. Fists always are.
    pub fn item(self) -> Option<Resource> {
        match self {
            Weapon::Fists => None,
            Weapon::BoneSpear => Some(Resource::BoneSpear),
            Weapon::IronSword => Some(Resource::IronSword),
            Weapon::SteelSword => Some(Resource::SteelSword),
            Weapon::Bayonet => Some(Resource::Bayonet),
            Weapon::Rifle => Some(Resource::Rifle),
            Weapon::LaserRifle => Some(Resource::LaserRifle),
            Weapon::Grenade => Some(Resource::Grenade),
            Weapon::Bolas => Some(Resource::Bolas),
        }
    }

    /// The weapon a carried item is, if it is one.
    pub fn of(item: Resource) -> Option<Weapon> {
        Weapon::ALL
            .into_iter()
            .find(|weapon| weapon.item() == Some(item))
    }
}

/// What a unit of something weighs in the pack. Everything not listed here
/// weighs one (upstream `Path.Weight`).
pub fn weight(item: Resource) -> f64 {
    match item {
        Resource::BoneSpear => 2.0,
        Resource::IronSword => 3.0,
        Resource::SteelSword | Resource::Rifle | Resource::LaserRifle => 5.0,
        Resource::Bullets => 0.1,
        Resource::EnergyCell => 0.2,
        Resource::Bolas => 0.5,
        _ => 1.0,
    }
}

/// Everything that can go in the pack, in the order the path screen lists it:
/// upstream's craftables and weapons, plus the handful of extras `path.js`
/// merges in.
pub static CARRYABLE: [Resource; 15] = [
    Resource::CuredMeat,
    Resource::Bullets,
    Resource::Medicine,
    Resource::EnergyCell,
    Resource::Charm,
    Resource::AlienAlloy,
    Resource::Torch,
    Resource::BoneSpear,
    Resource::IronSword,
    Resource::SteelSword,
    Resource::Rifle,
    Resource::LaserRifle,
    Resource::Bolas,
    Resource::Grenade,
    Resource::Bayonet,
];

/// The armour tiers, best first, with the health they add.
pub static ARMOUR: [(Resource, i64, &str); 3] = [
    (Resource::SteelArmour, 35, "steel"),
    (Resource::IronArmour, 15, "iron"),
    (Resource::LeatherArmour, 5, "leather"),
];

/// The water tiers, best first, with the water they add.
pub static WATERSKINS: [(Resource, i64); 3] = [
    (Resource::WaterTank, 50),
    (Resource::Cask, 20),
    (Resource::Waterskin, 10),
];

/// The pack tiers, best first, with the space they add over the base ten.
pub static PACKS: [(Resource, f64); 3] = [
    (Resource::Convoy, 60.0),
    (Resource::Wagon, 30.0),
    (Resource::Rucksack, 10.0),
];

/// Base pack capacity with nothing but pockets.
pub const DEFAULT_BAG_SPACE: f64 = 10.0;

/// The line for walking from one kind of ground onto another.
pub fn narrate_move(from: Tile, to: Tile) -> Option<&'static str> {
    match (from, to) {
        (Tile::Forest, Tile::Field) => {
            Some("the trees yield to dry grass. the yellowed brush rustles in the wind.")
        }
        (Tile::Forest, Tile::Barrens) => {
            Some("the trees are gone. parched earth and blowing dust are poor replacements.")
        }
        (Tile::Field, Tile::Forest) => Some(
            "trees loom on the horizon. grasses gradually yield to a forest floor of dry branches and fallen leaves.",
        ),
        (Tile::Field, Tile::Barrens) => Some("the grasses thin. soon, only dust remains."),
        (Tile::Barrens, Tile::Field) => {
            Some("the barrens break at a sea of dying grass, swaying in the arid breeze.")
        }
        (Tile::Barrens, Tile::Forest) => Some(
            "a wall of gnarled trees rises from the dust. their branches twist into a skeletal canopy overhead.",
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The wasteland's own prose.
// ---------------------------------------------------------------------------

pub const MSG_DANGER: &str = "dangerous to be this far from the village without proper protection";
pub const MSG_SAFER: &str = "safer here";
pub const MSG_MEAT_OUT: &str = "the meat has run out";
pub const MSG_STARVING: &str = "starvation sets in";
pub const MSG_WATER_OUT: &str = "there is no more water";
pub const MSG_THIRST: &str = "the thirst becomes unbearable";
pub const MSG_WATER_REPLENISHED: &str = "water replenished";
pub const MSG_WORLD_FADES: &str = "the world fades";

/// The perk a long enough hunger or thirst eventually teaches.
pub const STARVED_PERK_AT: u32 = 10;
pub const STARVED_PERK: Perk = Perk::SlowMetabolism;
pub const DEHYDRATED_PERK: Perk = Perk::DesertRat;
