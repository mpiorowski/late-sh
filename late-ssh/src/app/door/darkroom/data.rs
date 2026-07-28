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
}

impl Resource {
    /// Every resource, in the order the stores panel lists them.
    pub const ALL: [Resource; 10] = [
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
        }
    }
}

/// What the builder can put up. Upstream's `Craftables` of `type: 'building'`,
/// cut to the village act: the workshop tier and beyond needs the wasteland to
/// supply its materials, so it is not in this port yet.
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
}

impl Building {
    pub const ALL: [Building; 7] = [
        Building::Trap,
        Building::Cart,
        Building::Hut,
        Building::Lodge,
        Building::TradingPost,
        Building::Tannery,
        Building::Smokehouse,
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
            | Building::Smokehouse => Some(1),
        }
    }

    /// Build cost given how many already stand (traps and huts get pricier).
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
        }
    }

    /// The line the builder says when this first becomes available.
    pub fn available_msg(self) -> &'static str {
        match self {
            Building::Trap => {
                "builder says she can make traps to catch any creatures might still be alive out there"
            }
            Building::Cart => "builder says she can make a cart for carrying wood",
            Building::Hut => "builder says there are more wanderers. says they'll work, too.",
            Building::Lodge => "villagers could help hunt, given the means",
            Building::TradingPost => "a trading post would make commerce easier",
            Building::Tannery => {
                "builder says leather could be useful. says the villagers could make it."
            }
            Building::Smokehouse => {
                "should cure the meat, or it'll spoil. builder says she can fix something up."
            }
        }
    }

    /// The line when one goes up.
    pub fn build_msg(self) -> &'static str {
        match self {
            Building::Trap => "more traps to catch more creatures",
            Building::Cart => "the rickety cart will carry more wood from the forest",
            Building::Hut => "builder puts up a hut, out in the forest. says word will get around.",
            Building::Lodge => "the hunting lodge stands in the forest, a ways out of town",
            Building::TradingPost => {
                "now the nomads have a place to set up shop, they might stick around a while"
            }
            Building::Tannery => "tannery goes up quick, on the edge of the village",
            Building::Smokehouse => "builder finishes the smokehouse. she looks hungry.",
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
            | Building::Smokehouse => None,
        }
    }

    /// Jobs this building opens up (upstream `Outside.checkWorker`'s job map).
    pub fn unlocks_jobs(self) -> &'static [Job] {
        match self {
            Building::Lodge => &[Job::Hunter, Job::Trapper],
            Building::Tannery => &[Job::Tanner],
            Building::Smokehouse => &[Job::Charcutier],
            Building::Trap | Building::Cart | Building::Hut | Building::TradingPost => &[],
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
}

impl Job {
    pub const ALL: [Job; 4] = [Job::Hunter, Job::Trapper, Job::Tanner, Job::Charcutier];

    pub fn label(self) -> &'static str {
        match self {
            Job::Hunter => "hunter",
            Job::Trapper => "trapper",
            Job::Tanner => "tanner",
            Job::Charcutier => "charcutier",
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
        }
    }
}

/// What an unassigned villager brings in per income tick.
pub const GATHERER_YIELD: (Resource, f64) = (Resource::Wood, 1.0);

/// What the builder brings in once she is helping.
pub const BUILDER_YIELD: (Resource, f64) = (Resource::Wood, 2.0);

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
