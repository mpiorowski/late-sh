/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The balance
 * tables and notification prose below are transcribed from `script/room.js`
 * and `script/outside.js`. See LICENSING.md and NOTICE. */

//! Canonical A Dark Room balance data: the closed resource/building/job sets,
//! their costs and yields, and the timing constants the sim steps against.
//!
//! Everything here is a pure table. The rules acting on it live in `model`,
//! the clock that advances it lives in `sim`, and our own pacing layer (which
//! is *not* upstream data, and so is not covered by the MPL) lives in `pace`.

use serde::{Deserialize, Serialize};

/// The display name of the door. Kept in one place: the port ships under
/// upstream's title only while that is settled with its author, and renaming
/// is meant to be a one-line change.
pub const TITLE: &str = "A Dark Room";

/// Everything a player can hold. Upstream keys `stores` by free-form string;
/// a closed set means an unknown resource cannot be represented at all, and
/// adding one breaks every match that has to care.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Wood,
    Fur,
    Meat,
    Scales,
    Teeth,
    Cloth,
    Charm,
    Bait,
    Leather,
    CuredMeat,
    Iron,
    Coal,
    Sulphur,
    Steel,
    Medicine,
    Bullets,
    EnergyCell,
    AlienAlloy,
    Torch,
    Hypo,
    Stim,
    Glowstone,
    BoneSpear,
    IronSword,
    SteelSword,
    Rifle,
    LaserRifle,
    Bolas,
    Grenade,
    Bayonet,
    PlasmaRifle,
    EnergyBlade,
    Disruptor,
    Waterskin,
    Cask,
    WaterTank,
    FluidRecycler,
    Rucksack,
    Wagon,
    Convoy,
    CargoDrone,
    LeatherArmour,
    IronArmour,
    SteelArmour,
    KineticArmour,
    Compass,
    /// Taken off the immortal wanderer on the battleship's command deck. Held,
    /// never spent: what it changes is the ending.
    FleetBeacon,
    HypoBlueprint,
    KineticArmourBlueprint,
    DisruptorBlueprint,
    PlasmaRifleBlueprint,
    StimBlueprint,
    GlowstoneBlueprint,
}

impl Resource {
    /// Every resource, in the order the stores panel lists them.
    pub const ALL: [Resource; 53] = [
        Resource::Wood,
        Resource::Fur,
        Resource::Meat,
        Resource::Scales,
        Resource::Teeth,
        Resource::Cloth,
        Resource::Charm,
        Resource::Bait,
        Resource::Leather,
        Resource::CuredMeat,
        Resource::Iron,
        Resource::Coal,
        Resource::Sulphur,
        Resource::Steel,
        Resource::Medicine,
        Resource::Bullets,
        Resource::EnergyCell,
        Resource::AlienAlloy,
        Resource::Torch,
        Resource::Hypo,
        Resource::Stim,
        Resource::Glowstone,
        Resource::BoneSpear,
        Resource::IronSword,
        Resource::SteelSword,
        Resource::Rifle,
        Resource::LaserRifle,
        Resource::Bolas,
        Resource::Grenade,
        Resource::Bayonet,
        Resource::PlasmaRifle,
        Resource::EnergyBlade,
        Resource::Disruptor,
        Resource::Waterskin,
        Resource::Cask,
        Resource::WaterTank,
        Resource::FluidRecycler,
        Resource::Rucksack,
        Resource::Wagon,
        Resource::Convoy,
        Resource::CargoDrone,
        Resource::LeatherArmour,
        Resource::IronArmour,
        Resource::SteelArmour,
        Resource::KineticArmour,
        Resource::Compass,
        Resource::FleetBeacon,
        Resource::HypoBlueprint,
        Resource::KineticArmourBlueprint,
        Resource::DisruptorBlueprint,
        Resource::PlasmaRifleBlueprint,
        Resource::StimBlueprint,
        Resource::GlowstoneBlueprint,
    ];

    /// Lowercase label, exactly as upstream prints it.
    pub fn label(self) -> &'static str {
        match self {
            Resource::Wood => "wood",
            Resource::Fur => "fur",
            Resource::Meat => "meat",
            Resource::Scales => "scales",
            Resource::Teeth => "teeth",
            Resource::Cloth => "cloth",
            Resource::Charm => "charm",
            Resource::Bait => "bait",
            Resource::Leather => "leather",
            Resource::CuredMeat => "cured meat",
            Resource::Iron => "iron",
            Resource::Coal => "coal",
            Resource::Sulphur => "sulphur",
            Resource::Steel => "steel",
            Resource::Medicine => "medicine",
            Resource::Bullets => "bullets",
            Resource::EnergyCell => "energy cell",
            Resource::AlienAlloy => "alien alloy",
            Resource::Torch => "torch",
            Resource::Hypo => "hypo",
            Resource::Stim => "stim",
            Resource::Glowstone => "glow stone",
            Resource::BoneSpear => "bone spear",
            Resource::IronSword => "iron sword",
            Resource::SteelSword => "steel sword",
            Resource::Rifle => "rifle",
            Resource::LaserRifle => "laser rifle",
            Resource::Bolas => "bolas",
            Resource::Grenade => "grenade",
            Resource::Bayonet => "bayonet",
            Resource::PlasmaRifle => "plasma rifle",
            Resource::EnergyBlade => "energy blade",
            Resource::Disruptor => "disruptor",
            Resource::Waterskin => "waterskin",
            Resource::Cask => "cask",
            Resource::WaterTank => "water tank",
            Resource::FluidRecycler => "fluid recycler",
            Resource::Rucksack => "rucksack",
            Resource::Wagon => "wagon",
            Resource::Convoy => "convoy",
            Resource::CargoDrone => "cargo drone",
            Resource::LeatherArmour => "l armour",
            Resource::IronArmour => "i armour",
            Resource::SteelArmour => "s armour",
            Resource::KineticArmour => "kinetic armour",
            Resource::Compass => "compass",
            Resource::FleetBeacon => "fleet beacon",
            Resource::HypoBlueprint => "hypo blueprint",
            Resource::KineticArmourBlueprint => "kinetic armour blueprint",
            Resource::DisruptorBlueprint => "disruptor blueprint",
            Resource::PlasmaRifleBlueprint => "plasma rifle blueprint",
            Resource::StimBlueprint => "stim blueprint",
            Resource::GlowstoneBlueprint => "glow stone blueprint",
        }
    }

    /// Which bucket this belongs to, from the `type` field upstream hangs off
    /// its `Craftables`/`TradeGoods`/`MiscItems` entries.
    pub fn kind(self) -> ResourceKind {
        match self {
            Resource::Wood
            | Resource::Meat
            | Resource::Cloth
            | Resource::Charm
            | Resource::Bait
            | Resource::Leather
            | Resource::CuredMeat
            | Resource::Fur
            | Resource::Sulphur => ResourceKind::Basic,
            Resource::Scales
            | Resource::Teeth
            | Resource::Iron
            | Resource::Coal
            | Resource::Steel
            | Resource::Medicine
            | Resource::Bullets
            | Resource::EnergyCell
            | Resource::AlienAlloy => ResourceKind::Good,
            Resource::Torch | Resource::Hypo | Resource::Stim | Resource::Glowstone => {
                ResourceKind::Tool
            }
            Resource::BoneSpear
            | Resource::IronSword
            | Resource::SteelSword
            | Resource::Rifle
            | Resource::LaserRifle
            | Resource::Bolas
            | Resource::Grenade
            | Resource::Bayonet
            | Resource::PlasmaRifle
            | Resource::EnergyBlade
            | Resource::Disruptor => ResourceKind::Weapon,
            Resource::Waterskin
            | Resource::Cask
            | Resource::WaterTank
            | Resource::Rucksack
            | Resource::Wagon
            | Resource::Convoy
            | Resource::LeatherArmour
            | Resource::IronArmour
            | Resource::SteelArmour
            | Resource::KineticArmour
            | Resource::FluidRecycler
            | Resource::CargoDrone => ResourceKind::Upgrade,
            Resource::Compass
            | Resource::FleetBeacon
            | Resource::HypoBlueprint
            | Resource::KineticArmourBlueprint
            | Resource::DisruptorBlueprint
            | Resource::PlasmaRifleBlueprint
            | Resource::StimBlueprint
            | Resource::GlowstoneBlueprint => ResourceKind::Special,
        }
    }
}

/// Upstream's `type` field, which decides where a store is listed and whether
/// the workshop is needed to make it. The keys upstream leaves untyped (wood,
/// fur, the trap drops) fall into its default bucket, which is `Basic` here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ResourceKind {
    Basic,
    Good,
    Tool,
    Weapon,
    Upgrade,
    Special,
}

impl ResourceKind {
    /// Upstream `Room.needsWorkshop`: the finer things need the tools.
    pub fn needs_workshop(self) -> bool {
        match self {
            ResourceKind::Tool | ResourceKind::Weapon | ResourceKind::Upgrade => true,
            ResourceKind::Basic | ResourceKind::Good | ResourceKind::Special => false,
        }
    }
}

/// Everything that can stand in the village. Upstream's `Craftables` of
/// `type: 'building'`, plus the three mines, which are never offered by the
/// builder: the world hands those over when their setpiece is cleared.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Building {
    Trap,
    Cart,
    Hut,
    Lodge,
    TradingPost,
    Tannery,
    Smokehouse,
    Workshop,
    Steelworks,
    Armoury,
    IronMine,
    CoalMine,
    SulphurMine,
}

impl Building {
    pub const ALL: [Building; 13] = [
        Building::Trap,
        Building::Cart,
        Building::Hut,
        Building::Lodge,
        Building::TradingPost,
        Building::Tannery,
        Building::Smokehouse,
        Building::Workshop,
        Building::Steelworks,
        Building::Armoury,
        Building::IronMine,
        Building::CoalMine,
        Building::SulphurMine,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Building::Trap => "trap",
            Building::Cart => "cart",
            Building::Hut => "hut",
            Building::Lodge => "lodge",
            Building::TradingPost => "trading post",
            Building::Tannery => "tannery",
            Building::Smokehouse => "smokehouse",
            Building::Workshop => "workshop",
            Building::Steelworks => "steelworks",
            Building::Armoury => "armoury",
            Building::IronMine => "iron mine",
            Building::CoalMine => "coal mine",
            Building::SulphurMine => "sulphur mine",
        }
    }

    /// Whether the builder offers this at all. The mines are the exception:
    /// they only ever arrive from the wasteland, so they carry no cost and no
    /// build line, and never appear as a build row.
    pub fn builder_built(self) -> bool {
        match self {
            Building::Trap
            | Building::Cart
            | Building::Hut
            | Building::Lodge
            | Building::TradingPost
            | Building::Tannery
            | Building::Smokehouse
            | Building::Workshop
            | Building::Steelworks
            | Building::Armoury => true,
            Building::IronMine | Building::CoalMine | Building::SulphurMine => false,
        }
    }

    /// How many of this building may stand. `None` means unbounded.
    pub fn maximum(self) -> Option<u32> {
        match self {
            Building::Trap => Some(10),
            Building::Hut => Some(20),
            Building::Cart
            | Building::Lodge
            | Building::TradingPost
            | Building::Tannery
            | Building::Smokehouse
            | Building::Workshop
            | Building::Steelworks
            | Building::Armoury
            | Building::IronMine
            | Building::CoalMine
            | Building::SulphurMine => Some(1),
        }
    }

    /// Build cost given how many already stand (traps and huts get pricier).
    /// The mines have none: they are not built, they are found.
    pub fn cost(self, built: u32) -> Vec<(Resource, i64)> {
        let n = i64::from(built);
        match self {
            Building::Trap => vec![(Resource::Wood, 10 + n * 10)],
            Building::Cart => vec![(Resource::Wood, 30)],
            Building::Hut => vec![(Resource::Wood, 100 + n * 50)],
            Building::Lodge => vec![
                (Resource::Wood, 200),
                (Resource::Fur, 10),
                (Resource::Meat, 5),
            ],
            Building::TradingPost => vec![(Resource::Wood, 400), (Resource::Fur, 100)],
            Building::Tannery => vec![(Resource::Wood, 500), (Resource::Fur, 50)],
            Building::Smokehouse => vec![(Resource::Wood, 600), (Resource::Meat, 50)],
            Building::Workshop => vec![
                (Resource::Wood, 800),
                (Resource::Leather, 100),
                (Resource::Scales, 10),
            ],
            Building::Steelworks => vec![
                (Resource::Wood, 1500),
                (Resource::Iron, 100),
                (Resource::Coal, 100),
            ],
            Building::Armoury => vec![
                (Resource::Wood, 3000),
                (Resource::Steel, 100),
                (Resource::Sulphur, 50),
            ],
            Building::IronMine | Building::CoalMine | Building::SulphurMine => Vec::new(),
        }
    }

    /// The line the builder says when this first becomes available.
    pub fn available_msg(self) -> Option<&'static str> {
        match self {
            Building::Trap => Some(
                "builder says she can make traps to catch any creatures might still be alive out there",
            ),
            Building::Cart => Some("builder says she can make a cart for carrying wood"),
            Building::Hut => Some("builder says there are more wanderers. says they'll work, too."),
            Building::Lodge => Some("villagers could help hunt, given the means"),
            Building::TradingPost => Some("a trading post would make commerce easier"),
            Building::Tannery => {
                Some("builder says leather could be useful. says the villagers could make it.")
            }
            Building::Smokehouse => {
                Some("should cure the meat, or it'll spoil. builder says she can fix something up.")
            }
            Building::Workshop => {
                Some("builder says she could make finer things, if she had the tools")
            }
            Building::Steelworks => {
                Some("builder says the villagers could make steel, given the tools")
            }
            Building::Armoury => {
                Some("builder says it'd be useful to have a steady source of bullets")
            }
            Building::IronMine | Building::CoalMine | Building::SulphurMine => None,
        }
    }

    /// The line when one goes up.
    pub fn build_msg(self) -> Option<&'static str> {
        match self {
            Building::Trap => Some("more traps to catch more creatures"),
            Building::Cart => Some("the rickety cart will carry more wood from the forest"),
            Building::Hut => {
                Some("builder puts up a hut, out in the forest. says word will get around.")
            }
            Building::Lodge => Some("the hunting lodge stands in the forest, a ways out of town"),
            Building::TradingPost => {
                Some("now the nomads have a place to set up shop, they might stick around a while")
            }
            Building::Tannery => Some("tannery goes up quick, on the edge of the village"),
            Building::Smokehouse => Some("builder finishes the smokehouse. she looks hungry."),
            Building::Workshop => Some("workshop's finally ready. builder's excited to get to it"),
            Building::Steelworks => {
                Some("a haze falls over the village as the steelworks fires up")
            }
            Building::Armoury => Some("armoury's done, welcoming back the weapons of the past."),
            Building::IronMine | Building::CoalMine | Building::SulphurMine => None,
        }
    }

    /// The line when the maximum is already standing.
    pub fn max_msg(self) -> Option<&'static str> {
        match self {
            Building::Trap => Some("more traps won't help now"),
            Building::Hut => Some("no more room for huts."),
            Building::Cart
            | Building::Lodge
            | Building::TradingPost
            | Building::Tannery
            | Building::Smokehouse
            | Building::Workshop
            | Building::Steelworks
            | Building::Armoury
            | Building::IronMine
            | Building::CoalMine
            | Building::SulphurMine => None,
        }
    }

    /// Jobs this building opens up (upstream `Outside.checkWorker`'s job map).
    pub fn unlocks_jobs(self) -> &'static [Job] {
        match self {
            Building::Lodge => &[Job::Hunter, Job::Trapper],
            Building::Tannery => &[Job::Tanner],
            Building::Smokehouse => &[Job::Charcutier],
            Building::IronMine => &[Job::IronMiner],
            Building::CoalMine => &[Job::CoalMiner],
            Building::SulphurMine => &[Job::SulphurMiner],
            Building::Steelworks => &[Job::Steelworker],
            Building::Armoury => &[Job::Armourer],
            Building::Trap
            | Building::Cart
            | Building::Hut
            | Building::TradingPost
            | Building::Workshop => &[],
        }
    }
}

/// A villager's assignment. Gatherers are not in this set: upstream derives
/// them as "population minus everyone assigned", so an unassigned villager
/// gathers by definition and cannot be double-counted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    Hunter,
    Trapper,
    Tanner,
    Charcutier,
    IronMiner,
    CoalMiner,
    SulphurMiner,
    Steelworker,
    Armourer,
}

impl Job {
    /// In upstream `_INCOME` order, which is the order the workers list uses.
    pub const ALL: [Job; 9] = [
        Job::Hunter,
        Job::Trapper,
        Job::Tanner,
        Job::Charcutier,
        Job::IronMiner,
        Job::CoalMiner,
        Job::SulphurMiner,
        Job::Steelworker,
        Job::Armourer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Job::Hunter => "hunter",
            Job::Trapper => "trapper",
            Job::Tanner => "tanner",
            Job::Charcutier => "charcutier",
            Job::IronMiner => "iron miner",
            Job::CoalMiner => "coal miner",
            Job::SulphurMiner => "sulphur miner",
            Job::Steelworker => "steelworker",
            Job::Armourer => "armourer",
        }
    }

    /// What one worker of this job moves per income tick. Negative entries are
    /// consumed; upstream skips the whole payout when any input would go
    /// negative, so a starved trade simply stalls instead of going into debt.
    pub fn yields(self) -> &'static [(Resource, f64)] {
        match self {
            Job::Hunter => &[(Resource::Fur, 0.5), (Resource::Meat, 0.5)],
            Job::Trapper => &[(Resource::Meat, -1.0), (Resource::Bait, 1.0)],
            Job::Tanner => &[(Resource::Fur, -5.0), (Resource::Leather, 1.0)],
            Job::Charcutier => &[
                (Resource::Meat, -5.0),
                (Resource::Wood, -5.0),
                (Resource::CuredMeat, 1.0),
            ],
            Job::IronMiner => &[(Resource::CuredMeat, -1.0), (Resource::Iron, 1.0)],
            Job::CoalMiner => &[(Resource::CuredMeat, -1.0), (Resource::Coal, 1.0)],
            Job::SulphurMiner => &[(Resource::CuredMeat, -1.0), (Resource::Sulphur, 1.0)],
            Job::Steelworker => &[
                (Resource::Iron, -1.0),
                (Resource::Coal, -1.0),
                (Resource::Steel, 1.0),
            ],
            Job::Armourer => &[
                (Resource::Steel, -1.0),
                (Resource::Sulphur, -1.0),
                (Resource::Bullets, 1.0),
            ],
        }
    }
}

/// One thing the workshop can make. Upstream keeps these in the same
/// `Craftables` table as the buildings, keyed by `type`; the buildings live on
/// [`Building`] here, because only they carry an escalating cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Craftable {
    pub item: Resource,
    /// `None` where upstream leaves `maximum` undefined: torches and weapons
    /// stack, upgrades are one apiece.
    pub maximum: Option<u32>,
    pub cost: &'static [(Resource, i64)],
    /// The line when one is made. Upstream gives craftables no `availableMsg`,
    /// so a new craft row simply appears without comment.
    pub build_msg: &'static str,
}

/// Everything the workshop makes, in upstream's `Craftables` order. A `static`
/// rather than a `const` so a row can hold a `&'static Craftable` and no code
/// path has to look one up by name.
pub static CRAFTABLES: [Craftable; 14] = [
    Craftable {
        item: Resource::Torch,
        maximum: None,
        cost: &[(Resource::Wood, 1), (Resource::Cloth, 1)],
        build_msg: "a torch to keep the dark away",
    },
    Craftable {
        item: Resource::Waterskin,
        maximum: Some(1),
        cost: &[(Resource::Leather, 50)],
        build_msg: "this waterskin'll hold a bit of water, at least",
    },
    Craftable {
        item: Resource::Cask,
        maximum: Some(1),
        cost: &[(Resource::Leather, 100), (Resource::Iron, 20)],
        build_msg: "the cask holds enough water for longer expeditions",
    },
    Craftable {
        item: Resource::WaterTank,
        maximum: Some(1),
        cost: &[(Resource::Iron, 100), (Resource::Steel, 50)],
        build_msg: "never go thirsty again",
    },
    Craftable {
        item: Resource::BoneSpear,
        maximum: None,
        cost: &[(Resource::Wood, 100), (Resource::Teeth, 5)],
        build_msg: "this spear's not elegant, but it's pretty good at stabbing",
    },
    Craftable {
        item: Resource::Rucksack,
        maximum: Some(1),
        cost: &[(Resource::Leather, 200)],
        build_msg: "carrying more means longer expeditions to the wilds",
    },
    Craftable {
        item: Resource::Wagon,
        maximum: Some(1),
        cost: &[(Resource::Wood, 500), (Resource::Iron, 100)],
        build_msg: "the wagon can carry a lot of supplies",
    },
    Craftable {
        item: Resource::Convoy,
        maximum: Some(1),
        cost: &[
            (Resource::Wood, 1000),
            (Resource::Iron, 200),
            (Resource::Steel, 100),
        ],
        build_msg: "the convoy can haul mostly everything",
    },
    Craftable {
        item: Resource::LeatherArmour,
        maximum: Some(1),
        cost: &[(Resource::Leather, 200), (Resource::Scales, 20)],
        build_msg: "leather's not strong. better than rags, though.",
    },
    Craftable {
        item: Resource::IronArmour,
        maximum: Some(1),
        cost: &[(Resource::Leather, 200), (Resource::Iron, 100)],
        build_msg: "iron's stronger than leather",
    },
    Craftable {
        item: Resource::SteelArmour,
        maximum: Some(1),
        cost: &[(Resource::Leather, 200), (Resource::Steel, 100)],
        build_msg: "steel's stronger than iron",
    },
    Craftable {
        item: Resource::IronSword,
        maximum: None,
        cost: &[
            (Resource::Wood, 200),
            (Resource::Leather, 50),
            (Resource::Iron, 20),
        ],
        build_msg: "sword is sharp. good protection out in the wilds.",
    },
    Craftable {
        item: Resource::SteelSword,
        maximum: None,
        cost: &[
            (Resource::Wood, 500),
            (Resource::Leather, 100),
            (Resource::Steel, 20),
        ],
        build_msg: "the steel is strong, and the blade true.",
    },
    Craftable {
        item: Resource::Rifle,
        maximum: None,
        cost: &[
            (Resource::Wood, 200),
            (Resource::Steel, 50),
            (Resource::Sulphur, 50),
        ],
        build_msg: "black powder and bullets, like the old days.",
    },
];

// ---------------------------------------------------------------------------
// The fabricator, salvaged out of the ravaged battleship
// ---------------------------------------------------------------------------

/// A fabricator recipe that has to be found before it can be made. Upstream
/// keeps these as free-form `character.blueprints` keys redeemed out of the
/// pack on the way home; a closed set means a save can never hold a blueprint
/// for something the fabricator does not build.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blueprint {
    Hypo,
    KineticArmour,
    Disruptor,
    PlasmaRifle,
    Stim,
    Glowstone,
}

impl Blueprint {
    pub const ALL: [Blueprint; 6] = [
        Blueprint::Hypo,
        Blueprint::KineticArmour,
        Blueprint::Disruptor,
        Blueprint::PlasmaRifle,
        Blueprint::Stim,
        Blueprint::Glowstone,
    ];

    /// The loot item that redeems into this blueprint. Upstream drops these as
    /// ordinary pack items and converts them in `World.redeemBlueprints`.
    pub fn token(self) -> Resource {
        match self {
            Blueprint::Hypo => Resource::HypoBlueprint,
            Blueprint::KineticArmour => Resource::KineticArmourBlueprint,
            Blueprint::Disruptor => Resource::DisruptorBlueprint,
            Blueprint::PlasmaRifle => Resource::PlasmaRifleBlueprint,
            Blueprint::Stim => Resource::StimBlueprint,
            Blueprint::Glowstone => Resource::GlowstoneBlueprint,
        }
    }

    /// What holding it teaches the fabricator to make.
    pub fn item(self) -> Resource {
        match self {
            Blueprint::Hypo => Resource::Hypo,
            Blueprint::KineticArmour => Resource::KineticArmour,
            Blueprint::Disruptor => Resource::Disruptor,
            Blueprint::PlasmaRifle => Resource::PlasmaRifle,
            Blueprint::Stim => Resource::Stim,
            Blueprint::Glowstone => Resource::Glowstone,
        }
    }

    pub fn label(self) -> &'static str {
        self.item().label()
    }
}

/// One thing the fabricator makes. Upstream's `Fabricator.Craftables`: the
/// same whole-refusal shape as the workshop, but paid for in alien alloy and
/// gated on a blueprint rather than on a building.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fabricable {
    pub item: Resource,
    /// `None` where upstream leaves `maximum` undefined: weapons and tools
    /// stack, upgrades are one apiece.
    pub maximum: Option<u32>,
    pub cost: &'static [(Resource, i64)],
    /// How many one press makes (upstream's `quantity`, five hypos a go).
    pub quantity: i64,
    /// The blueprint that has to be found first, where upstream sets
    /// `blueprintRequired`. Three recipes need none: the fabricator arrives
    /// already knowing them.
    pub blueprint: Option<Blueprint>,
    pub build_msg: &'static str,
}

/// Everything the fabricator makes, in upstream's `Fabricator.Craftables`
/// order.
pub static FABRICABLES: [Fabricable; 9] = [
    Fabricable {
        item: Resource::EnergyBlade,
        maximum: None,
        cost: &[(Resource::AlienAlloy, 1)],
        quantity: 1,
        blueprint: None,
        build_msg: "the blade hums, charged particles sparking and fizzing.",
    },
    Fabricable {
        item: Resource::FluidRecycler,
        maximum: Some(1),
        cost: &[(Resource::AlienAlloy, 2)],
        quantity: 1,
        blueprint: None,
        build_msg: "water out, water in. waste not, want not.",
    },
    Fabricable {
        item: Resource::CargoDrone,
        maximum: Some(1),
        cost: &[(Resource::AlienAlloy, 2)],
        quantity: 1,
        blueprint: None,
        build_msg: "the workhorse of the wanderer fleet.",
    },
    Fabricable {
        item: Resource::KineticArmour,
        maximum: Some(1),
        cost: &[(Resource::AlienAlloy, 2)],
        quantity: 1,
        blueprint: Some(Blueprint::KineticArmour),
        build_msg: "wanderer soldiers succeed by subverting the enemy's rage.",
    },
    Fabricable {
        item: Resource::Disruptor,
        maximum: None,
        cost: &[(Resource::AlienAlloy, 1)],
        quantity: 1,
        blueprint: Some(Blueprint::Disruptor),
        build_msg: "somtimes it is best not to fight.",
    },
    Fabricable {
        item: Resource::Hypo,
        maximum: None,
        cost: &[(Resource::AlienAlloy, 1)],
        quantity: 5,
        blueprint: Some(Blueprint::Hypo),
        build_msg: "a handful of hypos. life in a vial.",
    },
    Fabricable {
        item: Resource::Stim,
        maximum: None,
        cost: &[(Resource::AlienAlloy, 1)],
        quantity: 1,
        blueprint: Some(Blueprint::Stim),
        build_msg: "sometimes it is best to fight without restraint.",
    },
    Fabricable {
        item: Resource::PlasmaRifle,
        maximum: None,
        cost: &[(Resource::AlienAlloy, 1)],
        quantity: 1,
        blueprint: Some(Blueprint::PlasmaRifle),
        build_msg: "the peak of wanderer weapons technology, sleek and deadly.",
    },
    Fabricable {
        item: Resource::Glowstone,
        maximum: None,
        cost: &[(Resource::AlienAlloy, 1)],
        quantity: 1,
        blueprint: Some(Blueprint::Glowstone),
        build_msg: "a smooth, perfect sphere. its light is inextinguishable.",
    },
];

/// One thing the trading post sells. Upstream's `TradeGoods` entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TradeGood {
    pub good: Resource,
    /// Only the compass is limited (`maximum: 1`); the rest stack.
    pub maximum: Option<u32>,
    pub cost: &'static [(Resource, i64)],
}

/// Everything the nomads sell, in upstream's `TradeGoods` order.
pub static TRADE_GOODS: [TradeGood; 13] = [
    TradeGood {
        good: Resource::Scales,
        maximum: None,
        cost: &[(Resource::Fur, 150)],
    },
    TradeGood {
        good: Resource::Teeth,
        maximum: None,
        cost: &[(Resource::Fur, 300)],
    },
    TradeGood {
        good: Resource::Iron,
        maximum: None,
        cost: &[(Resource::Fur, 150), (Resource::Scales, 50)],
    },
    TradeGood {
        good: Resource::Coal,
        maximum: None,
        cost: &[(Resource::Fur, 200), (Resource::Teeth, 50)],
    },
    TradeGood {
        good: Resource::Steel,
        maximum: None,
        cost: &[
            (Resource::Fur, 300),
            (Resource::Scales, 50),
            (Resource::Teeth, 50),
        ],
    },
    TradeGood {
        good: Resource::Medicine,
        maximum: None,
        cost: &[(Resource::Scales, 50), (Resource::Teeth, 30)],
    },
    TradeGood {
        good: Resource::Bullets,
        maximum: None,
        cost: &[(Resource::Scales, 10)],
    },
    TradeGood {
        good: Resource::EnergyCell,
        maximum: None,
        cost: &[(Resource::Scales, 10), (Resource::Teeth, 10)],
    },
    TradeGood {
        good: Resource::Bolas,
        maximum: None,
        cost: &[(Resource::Teeth, 10)],
    },
    TradeGood {
        good: Resource::Grenade,
        maximum: None,
        cost: &[(Resource::Scales, 100), (Resource::Teeth, 50)],
    },
    TradeGood {
        good: Resource::Bayonet,
        maximum: None,
        cost: &[(Resource::Scales, 500), (Resource::Teeth, 250)],
    },
    TradeGood {
        good: Resource::AlienAlloy,
        maximum: None,
        cost: &[
            (Resource::Fur, 1500),
            (Resource::Scales, 750),
            (Resource::Teeth, 300),
        ],
    },
    TradeGood {
        good: Resource::Compass,
        maximum: Some(1),
        cost: &[
            (Resource::Fur, 400),
            (Resource::Scales, 20),
            (Resource::Teeth, 10),
        ],
    },
];

/// What the wanderer has learned, from `engine.js` `Engine.Perks`. Perks are
/// permanent, and every one of them changes a number in the wasteland rather
/// than unlocking anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Perk {
    Boxer,
    MartialArtist,
    UnarmedMaster,
    Barbarian,
    SlowMetabolism,
    DesertRat,
    Evasive,
    Precise,
    Scout,
    Stealthy,
    Gastronome,
}

impl Perk {
    pub const ALL: [Perk; 11] = [
        Perk::Boxer,
        Perk::MartialArtist,
        Perk::UnarmedMaster,
        Perk::Barbarian,
        Perk::SlowMetabolism,
        Perk::DesertRat,
        Perk::Evasive,
        Perk::Precise,
        Perk::Scout,
        Perk::Stealthy,
        Perk::Gastronome,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Perk::Boxer => "boxer",
            Perk::MartialArtist => "martial artist",
            Perk::UnarmedMaster => "unarmed master",
            Perk::Barbarian => "barbarian",
            Perk::SlowMetabolism => "slow metabolism",
            Perk::DesertRat => "desert rat",
            Perk::Evasive => "evasive",
            Perk::Precise => "precise",
            Perk::Scout => "scout",
            Perk::Stealthy => "stealthy",
            Perk::Gastronome => "gastronome",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Perk::Boxer => "punches do more damage",
            Perk::MartialArtist => "punches do even more damage.",
            Perk::UnarmedMaster => "punch twice as fast, and with even more force",
            Perk::Barbarian => "melee weapons deal more damage",
            Perk::SlowMetabolism => "go twice as far without eating",
            Perk::DesertRat => "go twice as far without drinking",
            Perk::Evasive => "dodge attacks more effectively",
            Perk::Precise => "land blows more often",
            Perk::Scout => "see farther",
            Perk::Stealthy => "better avoid conflict in the wild",
            Perk::Gastronome => "restore more health when eating",
        }
    }

    /// The line printed the moment it is learned.
    pub fn notify(self) -> &'static str {
        match self {
            Perk::Boxer => "learned to throw punches with purpose",
            Perk::MartialArtist => "learned to fight quite effectively without weapons",
            Perk::UnarmedMaster => "learned to strike faster without weapons",
            Perk::Barbarian => "learned to swing weapons with force",
            Perk::SlowMetabolism => "learned how to ignore the hunger",
            Perk::DesertRat => "learned to love the dry air",
            Perk::Evasive => "learned to be where they're not",
            Perk::Precise => "learned to predict their movement",
            Perk::Scout => "learned to look ahead",
            Perk::Stealthy => "learned how not to be seen",
            Perk::Gastronome => "learned to make the most of food",
        }
    }
}

/// What an unassigned villager brings in per income tick.
pub const GATHERER_YIELD: (Resource, f64) = (Resource::Wood, 1.0);

/// What the builder brings in once she is helping.
pub const BUILDER_YIELD: (Resource, f64) = (Resource::Wood, 2.0);

/// What the thieves take per income tick while they are at it
/// (`state_manager.js` `startThieves`).
pub const THIEF_SKIM: &[(Resource, i64)] = &[
    (Resource::Wood, -10),
    (Resource::Fur, -5),
    (Resource::Meat, -5),
];

/// How hot the fire is burning.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fire {
    #[default]
    Dead,
    Smoldering,
    Flickering,
    Burning,
    Roaring,
}

impl Fire {
    pub fn text(self) -> &'static str {
        match self {
            Fire::Dead => "dead",
            Fire::Smoldering => "smoldering",
            Fire::Flickering => "flickering",
            Fire::Burning => "burning",
            Fire::Roaring => "roaring",
        }
    }

    pub fn value(self) -> u8 {
        match self {
            Fire::Dead => 0,
            Fire::Smoldering => 1,
            Fire::Flickering => 2,
            Fire::Burning => 3,
            Fire::Roaring => 4,
        }
    }

    pub fn from_value(value: u8) -> Fire {
        match value {
            0 => Fire::Dead,
            1 => Fire::Smoldering,
            2 => Fire::Flickering,
            3 => Fire::Burning,
            _ => Fire::Roaring,
        }
    }

    /// One step up, saturating at roaring.
    pub fn stoked(self) -> Fire {
        Fire::from_value(self.value().saturating_add(1).min(4))
    }

    /// One step down, saturating at dead.
    pub fn cooled(self) -> Fire {
        Fire::from_value(self.value().saturating_sub(1))
    }
}

/// How warm the room is.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Temperature {
    #[default]
    Freezing,
    Cold,
    Mild,
    Warm,
    Hot,
}

impl Temperature {
    pub fn text(self) -> &'static str {
        match self {
            Temperature::Freezing => "freezing",
            Temperature::Cold => "cold",
            Temperature::Mild => "mild",
            Temperature::Warm => "warm",
            Temperature::Hot => "hot",
        }
    }

    pub fn value(self) -> u8 {
        match self {
            Temperature::Freezing => 0,
            Temperature::Cold => 1,
            Temperature::Mild => 2,
            Temperature::Warm => 3,
            Temperature::Hot => 4,
        }
    }

    pub fn from_value(value: u8) -> Temperature {
        match value {
            0 => Temperature::Freezing,
            1 => Temperature::Cold,
            2 => Temperature::Mild,
            3 => Temperature::Warm,
            _ => Temperature::Hot,
        }
    }
}

/// What a sprung trap can hold, as cumulative roll thresholds over `[0, 1)`.
pub const TRAP_DROPS: [(f64, Resource, &str); 6] = [
    (0.5, Resource::Fur, "scraps of fur"),
    (0.75, Resource::Meat, "bits of meat"),
    (0.85, Resource::Scales, "strange scales"),
    (0.93, Resource::Teeth, "scattered teeth"),
    (0.995, Resource::Cloth, "tattered cloth"),
    (1.0, Resource::Charm, "a crudely made charm"),
];

// ---------------------------------------------------------------------------
// Timing. Upstream expresses these in milliseconds against wall-clock timers;
// the port steps a whole-second sim, so they are seconds here.
// ---------------------------------------------------------------------------

/// Seconds after a stoke before the fire drops a level (`_FIRE_COOL_DELAY`).
pub const FIRE_COOL_DELAY: u32 = 5 * 60;

/// Seconds between room temperature steps (`_ROOM_WARM_DELAY`).
pub const ROOM_WARM_DELAY: u32 = 30;

/// Seconds between builder state steps (`_BUILDER_STATE_DELAY`).
pub const BUILDER_STATE_DELAY: u32 = 30;

/// Seconds from the stranger arriving to the wood running out
/// (`_NEED_WOOD_DELAY`).
pub const NEED_WOOD_DELAY: u32 = 15;

/// Cooldown on the light/stoke buttons (`_STOKE_COOLDOWN`).
pub const STOKE_COOLDOWN: u32 = 10;

/// Cooldown on gathering wood (`_GATHER_DELAY`).
pub const GATHER_DELAY: u32 = 60;

/// Cooldown on checking the traps (`_TRAPS_DELAY`).
pub const TRAPS_DELAY: u32 = 90;

/// Seconds between income payouts (every worker's `delay`).
pub const INCOME_DELAY: u32 = 10;

/// Villagers one hut sleeps (`_HUT_ROOM`).
pub const HUT_ROOM: u32 = 4;

/// The window new arrivals are drawn from (`_POP_DELAY`), in minutes. Upstream
/// draws `floor(random * (high - low)) + low`, which lands on exactly 0.5, 1.5
/// or 2.5 minutes; the sim reproduces that draw rather than smoothing it into
/// a uniform range.
pub const POP_DELAY_MINUTES: (f64, f64) = (0.5, 3.0);

/// Wood a bare gathering trip brings back, and the same trip with the cart.
pub const GATHER_WOOD: i64 = 10;
pub const GATHER_WOOD_CART: i64 = 50;

/// Wood it takes to light a dead fire, and to stoke a live one.
pub const LIGHT_FIRE_COST: i64 = 5;
pub const STOKE_FIRE_COST: i64 = 1;

/// What the forest holds the moment the wood runs out.
pub const UNLOCK_FOREST_WOOD: i64 = 4;

// ---------------------------------------------------------------------------
// Notification prose, verbatim from upstream. Kept here (under the MPL) rather
// than at the call sites in `state.rs`, which is an FSL file.
// ---------------------------------------------------------------------------

/// `Room.lightFire` on short wood.
pub const MSG_NOT_ENOUGH_WOOD: &str = "not enough wood to get the fire going";

/// `Room.stokeFire` on empty stores.
pub const MSG_WOOD_RUN_OUT: &str = "the wood has run out";

/// `Room.onFireChange`, the moment the fire first draws the stranger.
pub const MSG_FIRE_SPILLS: &str =
    "the light from the fire spills from the windows, out into the dark";

/// `Room.build` while the room is Cold or worse.
pub const MSG_BUILDER_SHIVERS: &str = "builder just shivers";

/// `Outside.gatherWood`.
pub const MSG_GATHER_WOOD: &str = "dry brush and dead branches litter the forest floor";

/// `Outside.onArrival`, printed once on the first visit outside.
pub const MSG_SEEN_FOREST: &str = "the sky is grey and the wind blows relentlessly";

/// The legends over the three groups of room buttons (`updateBuildButtons`).
pub const SECTION_BUILD: &str = "build:";
pub const SECTION_CRAFT: &str = "craft:";
pub const SECTION_BUY: &str = "buy:";

/// The fabricator's panel: its title, its button legend, and the legend over
/// the list of blueprints found so far (upstream `Fabricator.init`).
pub const FABRICATOR_TITLE: &str = "A Whirring Fabricator";
pub const SECTION_FABRICATE: &str = "fabricate:";
pub const SECTION_BLUEPRINTS: &str = "blueprints";

/// What the village says when the strange device comes home from the ravaged
/// battleship (upstream `World.goHome`).
pub const MSG_FABRICATOR_FOUND: &str = "builder knows the strange device when she sees it. takes it for herself real quick. doesn't ask where it came from.";

/// The fabricator's one arrival line, said the first time it is looked at.
pub const MSG_FABRICATOR_SEEN: &str =
    "the familiar hum of wanderer machinery coming to life. finally, real tools.";

/// What a blueprint carried home is worth (upstream `World.redeemBlueprints`).
pub const MSG_BLUEPRINTS_REDEEMED: &str =
    "blueprints feed into the fabricator data port. possibilities grow.";
