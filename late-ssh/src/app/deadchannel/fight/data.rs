//! The fight's numbers and its fauna. Numbers are LoGD's, transcribed
//! (the same provenance note as `door/greendragon/data.rs`: balance tables
//! are mechanics, the names are ours); GAME.md reuses them 1:1 and any
//! deviation states its reason here. The door keeps its own copy so it
//! stays untouched and the game can drift.
//!
//! The fauna is original: glyphs, creatures made of the characters the
//! terminal renders (GAME.md, "Theme"). Every name passes the screenshot
//! test, every portrait is three rows of five cells in the runner's own
//! portrait format, built from the glyph alphabet and the static shades,
//! so a foe and a runner face each other in one register.

/// Fights per UTC day (`TURNS_PER_DAY`).
pub const RATIONS_PER_DAY: i32 = 10;
/// Signal (health) per level; max signal is a flat multiple.
pub const SIGNAL_PER_LEVEL: i32 = 10;
/// Bits a fresh runner holds (`START_GOLD`).
pub const START_BITS: i64 = 50;
/// Exp kept when the signal drops (`EXP_KEEP_ON_DEATH`).
pub const EXP_KEEP_ON_DEATH: f64 = 0.90;
/// Levels 1 to 15.
pub const MAX_LEVEL: i32 = 15;
/// What the armorer pays for the piece you hand back, as a percentage of
/// its price on the wall (`weapons.php`: 75%). Rounded down.
pub const TRADE_IN_PERCENT: i64 = 75;
/// Odds a run gets away, out of `RUN_ODDS_OUT_OF`. LoGD's forest run is a
/// coin toss that the foe answers with a free strike when it fails; two
/// in three here because a fight on the street is short and a failed run
/// that costs the fight anyway reads as a rigged door.
pub const RUN_ODDS: u32 = 2;
pub const RUN_ODDS_OUT_OF: u32 = 3;

/// A win that leaves the signal at or under this is a near miss, and the
/// wire says so (GAME.md, "shaped luck": the moments people retell).
pub const NEAR_MISS_SIGNAL: i32 = 3;

/// Exp needed to advance from level `i + 1` (`lib/experience.php`, the
/// base curve; marks scale it later, GAME.md "The stat block").
pub const EXP_TO_ADVANCE: [i64; 15] = [
    100, 400, 1002, 1912, 3140, 4707, 6641, 8985, 11795, 15143, 19121, 23840, 29437, 36071, 43930,
];

/// Exp needed to leave `level`. Past the top there is no next level; the
/// Old Signal is the way up, and it is not here yet.
pub fn exp_to_advance(level: i32) -> Option<i64> {
    if !(1..MAX_LEVEL).contains(&level) {
        return None;
    }
    Some(EXP_TO_ADVANCE[(level - 1) as usize])
}

/// A glyph's stat block: LoGD's per-level creature seeds, every creature
/// of a level sharing the same numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoeTier {
    pub signal: i32,
    pub attack: u32,
    pub defense: u32,
    pub bits: i64,
    pub exp: i64,
}

pub const FOE_TIERS: [FoeTier; 15] = [
    FoeTier {
        signal: 10,
        attack: 1,
        defense: 1,
        bits: 36,
        exp: 14,
    },
    FoeTier {
        signal: 21,
        attack: 3,
        defense: 3,
        bits: 97,
        exp: 24,
    },
    FoeTier {
        signal: 32,
        attack: 5,
        defense: 4,
        bits: 148,
        exp: 34,
    },
    FoeTier {
        signal: 43,
        attack: 7,
        defense: 6,
        bits: 162,
        exp: 45,
    },
    FoeTier {
        signal: 53,
        attack: 9,
        defense: 7,
        bits: 199,
        exp: 55,
    },
    FoeTier {
        signal: 64,
        attack: 11,
        defense: 8,
        bits: 233,
        exp: 66,
    },
    FoeTier {
        signal: 74,
        attack: 13,
        defense: 10,
        bits: 268,
        exp: 77,
    },
    FoeTier {
        signal: 84,
        attack: 15,
        defense: 11,
        bits: 302,
        exp: 89,
    },
    FoeTier {
        signal: 94,
        attack: 17,
        defense: 13,
        bits: 336,
        exp: 101,
    },
    FoeTier {
        signal: 105,
        attack: 19,
        defense: 14,
        bits: 369,
        exp: 114,
    },
    FoeTier {
        signal: 115,
        attack: 21,
        defense: 15,
        bits: 402,
        exp: 127,
    },
    FoeTier {
        signal: 125,
        attack: 23,
        defense: 17,
        bits: 435,
        exp: 141,
    },
    FoeTier {
        signal: 135,
        attack: 25,
        defense: 18,
        bits: 467,
        exp: 156,
    },
    FoeTier {
        signal: 145,
        attack: 27,
        defense: 20,
        bits: 499,
        exp: 172,
    },
    FoeTier {
        signal: 155,
        attack: 29,
        defense: 21,
        bits: 531,
        exp: 189,
    },
];

/// A kind of glyph: its name and its face. One per level; the name is
/// fiction, the tier is the balance.
#[derive(Debug, PartialEq, Eq)]
pub struct FoeKind {
    pub name: &'static str,
    /// Three rows of five cells, the runner portrait's format.
    pub portrait: [&'static str; 3],
    /// How it arrives, in the announcer's voice.
    pub arrives: &'static str,
}

pub const FOES: [FoeKind; 15] = [
    FoeKind {
        name: "flicker",
        portrait: ["  ▘  ", " ▗▖▝ ", "  ▖  "],
        arrives: "a flicker sputters out of the static. it is barely there.",
    },
    FoeKind {
        name: "hiss",
        portrait: [" ░ ░ ", "░▒░▒░", " ░ ░ "],
        arrives: "the noise thickens into a hiss. it has edges.",
    },
    FoeKind {
        name: "drift",
        portrait: ["  ▚  ", " ▞▚▞ ", "  ▞  "],
        arrives: "a drift slides sideways out of the picture and stops in front of you.",
    },
    FoeKind {
        name: "stray signal",
        portrait: [" ┼ ┼ ", "▐▘ ▝▌", " ┼─┼ "],
        arrives: "a stray signal, still carrying somebody's voice. it turns toward you.",
    },
    FoeKind {
        name: "crackle",
        portrait: [" ╪ ╪ ", "▐▚▞▚▌", " ╫ ╫ "],
        arrives: "the screen crackles and something climbs down off it.",
    },
    FoeKind {
        name: "ghost frame",
        portrait: [" ░▒░ ", "▐◌ ◌▌", " ▒░▒ "],
        arrives: "a ghost frame: one picture, held too long, burned in. it remembers you.",
    },
    FoeKind {
        name: "howler",
        portrait: ["▚▞▚▞▚", "▐▓▒▓▌", "▞▚▞▚▞"],
        arrives: "a howler. the whole street hears it before it sees it.",
    },
    FoeKind {
        name: "dead pixels",
        portrait: ["▖▗▖▗▖", "▝▘▝▘▝", "▖▗▖▗▖"],
        arrives: "dead pixels, a swarm of them, moving like one thing.",
    },
    FoeKind {
        name: "carrier",
        portrait: [" ╫╫╫ ", "▐╪═╪▌", " ▟▓▙ "],
        arrives: "a carrier steps out of the static wearing a coat. it is not a runner.",
    },
    FoeKind {
        name: "blackout",
        portrait: [" ███ ", "▐█ █▌", " ███ "],
        arrives: "a blackout. where it stands there is no light at all.",
    },
    FoeKind {
        name: "feedback",
        portrait: ["╬╬╬╬╬", "▐▞▚▞▌", "╬╬╬╬╬"],
        arrives: "feedback, screaming at itself, and now at you.",
    },
    FoeKind {
        name: "white noise",
        portrait: ["▒▓▒▓▒", "▓░▓░▓", "▒▓▒▓▒"],
        arrives: "white noise, all of it, walking.",
    },
    FoeKind {
        name: "test pattern",
        portrait: ["▐▌▐▌▐", "▓░▓░▓", "▌▐▌▐▌"],
        arrives: "the test pattern comes on. it has never done that with somebody standing here.",
    },
    FoeKind {
        name: "burnout",
        portrait: [" ▓█▓ ", "█▐░▌█", " ▓█▓ "],
        arrives: "a burnout, what is left when a channel runs too hot. it is still hot.",
    },
    FoeKind {
        name: "interference",
        portrait: ["╪╫╪╫╪", "▐┼╬┼▌", "╫╪╫╪╫"],
        arrives: "interference from somewhere below. this is the top of it.",
    },
];

/// The glyph that answers a runner of `level`: one kind and one tier per
/// level, clamped to the table.
pub fn foe_for_level(level: i32) -> (usize, &'static FoeKind, FoeTier) {
    let index = (level.clamp(1, MAX_LEVEL) - 1) as usize;
    (index, &FOES[index], FOE_TIERS[index])
}

/// The bits the street takes when the signal drops: everything on hand.
pub const DROP_LINES: [&str; 3] = [
    "your signal drops. the street goes quiet around you, then goes on without you.",
    "the picture tears and does not come back. your signal drops.",
    "you are static. the channel closes over you. your signal drops.",
];

pub const KILL_LINES: [&str; 3] = [
    "it scatters into the static.",
    "it comes apart into the noise it was made of.",
    "the screen swallows it back.",
];

pub const RUN_LINES: [&str; 2] = [
    "you get away. the static closes behind you.",
    "you back out of the picture. it does not follow.",
];

pub const RUN_FAILED_LINES: [&str; 2] = [
    "you turn and it is already there.",
    "no way out of the frame. it catches you turning.",
];
