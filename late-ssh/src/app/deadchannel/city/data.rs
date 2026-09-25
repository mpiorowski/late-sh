//! The city's copy and catalogs: what the shops show before they trade.
//! Build-time content in the announcer's voice (lowercase, noir, every noun
//! passing the screenshot test). Numbers are the LoGD numbers GAME.md
//! reuses 1:1; the names are the draft ladder from GAME.md, "Gear".
//!
//! The Green Dragon door keeps its own copy of the price ladder
//! (`door/greendragon/data.rs`); this one is the game's, so the door can
//! stay untouched and the game can drift with a stated reason.

use super::map::Landmark;

/// Buy price in bits for a weapon or armor of tier `i + 1`; power equals
/// the tier. LoGD's ladder, transcribed.
pub const COST_LADDER: [u32; 15] = [
    48, 225, 585, 990, 1575, 2250, 2790, 3420, 4230, 5040, 5850, 6840, 8010, 9000, 10350,
];

/// Weapon names by tier (GAME.md draft ladder): street junk up to things
/// made of the medium itself.
pub const WEAPONS: [&str; 15] = [
    "bent antenna",
    "box cutter",
    "tire iron",
    "rebar club",
    "stun baton",
    "nail gun",
    "cable whip",
    "arc torch",
    "static knife",
    "flicker blade",
    "feedback saw",
    "dead-air saber",
    "burnout lance",
    "channel breaker",
    "the last broadcast",
];

/// Armor names by tier.
pub const ARMOR: [&str; 15] = [
    "thrift coat",
    "rain shell",
    "padded jacket",
    "riot vest",
    "foil-lined coat",
    "kevlar weave",
    "faraday coat",
    "lead apron",
    "static cloak",
    "shielded rig",
    "ghost weave",
    "dead-channel mantle",
    "blackout plate",
    "white-noise shell",
    "the test pattern",
];

/// A band: the class a runner picks on the first descent.
pub struct Band {
    pub name: &'static str,
    /// One line, the register.
    pub register: &'static str,
    /// The four moves, unlocked by band skill. Draft names; each band
    /// reads as one voice.
    pub moves: [&'static str; 4],
}

pub const BANDS: [Band; 3] = [
    Band {
        name: "tuner",
        register: "works the signal itself: mends, siphons, summons a hand of the medium",
        moves: ["retune", "siphon", "carrier", "clear channel"],
    },
    Band {
        name: "jammer",
        register: "noise and corruption: summoned static, curses that cut a foe's attack, wither",
        moves: ["hiss", "jam", "wither", "blackout"],
    },
    Band {
        name: "ghost",
        register: "unseen: poison, the hidden strike, the backstab",
        moves: ["smear", "from behind", "unseen", "vanish"],
    },
];

/// A notice pinned to the board: a standing order, fulfilled through the
/// ambient loop (GAME.md, "Quests at 30 users"). None are posted yet.
pub struct Notice {
    pub text: &'static str,
    pub reward: &'static str,
}

pub const NOTICES: [Notice; 4] = [
    Notice {
        text: "put down five flickers this week",
        reward: "?? bits",
    },
    Notice {
        text: "survive the broadcast tonight",
        reward: "?? bits",
    },
    Notice {
        text: "bring the reader a howler's tooth",
        reward: "?? bits",
    },
    Notice {
        text: "find out what the lockers are humming",
        reward: "a key",
    },
];

/// What the bar pours. Flavor; the real drinks are upstairs, for chips.
pub const DRINKS: [&str; 4] = [
    "static on ice",
    "dead air, neat",
    "test pattern (comes with the bars)",
    "the last broadcast (ask)",
];

/// Prices the tailor will charge once the rack exists (GAME.md placeholders,
/// design review pending). Chips, burned whole.
pub const TAILOR_PRICES: [(&str, &str); 3] = [
    ("starter piece or starter mark", "free"),
    ("common piece", "2,000 chips"),
    ("rare piece", "10,000 chips"),
];

/// What the street says when you press Enter at a landmark that is not a
/// shop. A pool per landmark; the state picks one by tick.
pub fn lines(landmark: Landmark) -> &'static [&'static str] {
    match landmark {
        Landmark::Noodles => &[
            "the cook doesn't look up. \"two bowls or none.\"",
            "steam. broth. somebody's runner left a bowl half finished.",
            "\"you're the one from the wire,\" the cook says. \"eat.\"",
        ],
        Landmark::Umbrellas => &[
            "it never stops. buy one anyway.",
            "the handles glow so you can find each other in the rain.",
            "\"cyan or magenta. nobody buys the black ones.\"",
        ],
        Landmark::Blades => &[
            "what the armorer won't sell.",
            "\"no receipts. no names.\" the vendor grins at your mark.",
            "the tire iron is 585 bits here too. the street has one price.",
        ],
        Landmark::Reader => &[
            "she reads the static. it has said your name a lot lately.",
            "\"come back when the clock stops glitching,\" she says, and does not explain.",
            "the tent smells of ozone. she does not look at you. she looks behind you.",
        ],
        Landmark::Stairs => &[
            "lower levels. the stair goes down further than the map.",
            "the lights below are somebody else's street.",
            "not yet.",
        ],
        Landmark::Armorer
        | Landmark::Tailor
        | Landmark::Lockers
        | Landmark::Bands
        | Landmark::Bar
        | Landmark::Repairs
        | Landmark::Board
        | Landmark::Bits
        | Landmark::Screen
        | Landmark::Wire
        | Landmark::Ledge => &[],
    }
}

/// The popover title for a landmark: its name as the street knows it.
pub fn title(landmark: Landmark) -> &'static str {
    match landmark {
        Landmark::Armorer => "the armorer",
        Landmark::Tailor => "the tailor",
        Landmark::Lockers => "the lockers",
        Landmark::Bands => "bands",
        Landmark::Bar => "dead air",
        Landmark::Screen => "the screen",
        Landmark::Repairs => "patch",
        Landmark::Board => "the board",
        Landmark::Bits => "the bits machine",
        Landmark::Noodles => "the noodle cart",
        Landmark::Umbrellas => "the umbrella stall",
        Landmark::Blades => "the blade cart",
        Landmark::Reader => "the reader",
        Landmark::Stairs => "the stairs down",
        Landmark::Wire => "the wire",
        Landmark::Ledge => "the ledge",
    }
}

/// The one-line pitch under the popover title.
pub fn pitch(landmark: Landmark) -> &'static str {
    match landmark {
        Landmark::Armorer => "weapons and armor, fifteen tiers, bits only",
        Landmark::Tailor => "hoods, eyes, coats, marks: the look. chips only",
        Landmark::Lockers => "the stash. what you leave here survives a dropped signal",
        Landmark::Bands => "tuner, jammer, ghost: the choice is made once",
        Landmark::Bar => "the signal is warm in here",
        Landmark::Screen => "tuned to a dead channel. the glyphs come out of it. a ration a step",
        Landmark::Repairs => "repairs, when there is something to repair",
        Landmark::Board => "standing orders. nothing posted yet",
        Landmark::Bits => "it hums. it has never once paid out",
        Landmark::Noodles => "two bowls or none",
        Landmark::Umbrellas => "it never stops",
        Landmark::Blades => "what the armorer won't sell",
        Landmark::Reader => "she reads the static",
        Landmark::Stairs => "the way down is not open",
        Landmark::Wire => "back up to #deadchannel",
        Landmark::Ledge => "the lower city, all the way down",
    }
}
