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

/// Steps of the road per UTC day (`TURNS_PER_DAY`): a ration buys one.
pub const RATIONS_PER_DAY: i32 = 10;
/// Signal (health) per level; max signal is a flat multiple.
pub const SIGNAL_PER_LEVEL: i32 = 10;
/// Bits a fresh runner holds (`START_GOLD`).
pub const START_BITS: i64 = 50;
/// Exp kept when the signal drops (`EXP_KEEP_ON_DEATH`). LoGD keeps 90%;
/// 65% is what makes a few drops on the way up cost the reckless runner
/// a week (`sim_test.rs`), with the wall and patch priced as they are
/// (`BALANCE.md`).
pub const EXP_KEEP_ON_DEATH: f64 = 0.65;
/// Levels 1 to 15.
pub const MAX_LEVEL: i32 = 15;
/// What the armorer pays for the piece you hand back, as a percentage of
/// its price on the wall (`weapons.php`: 75%). Rounded down.
pub const TRADE_IN_PERCENT: i64 = 75;
/// A win that leaves the signal at or under this is a near miss, and the
/// wire says so (GAME.md, "shaped luck": the moments people retell).
pub const NEAR_MISS_SIGNAL: i32 = 3;

/// Exp needed to advance from level `i + 1` (`lib/experience.php`, the
/// base curve; `scaled` adds the marks, GAME.md "The stat block"). The
/// last entry is not a level: it is the exp at the top that makes the Old
/// Signal hear you ([`exp_to_seek`]).
pub const EXP_TO_ADVANCE: [i64; 15] = [
    100, 400, 1002, 1912, 3140, 4707, 6641, 8985, 11795, 15143, 19121, 23840, 29437, 36071, 43930,
];

/// The attack and defense a mark adds, capped: the second climb is a bit
/// faster, and a veteran never outgrows the room (GAME.md, "Marks: the
/// reset").
pub const MARK_BONUS_CAP: i32 = 5;

/// Exp needed to leave `level` with `marks`. Past the top there is no next
/// level; the Old Signal is the way up.
pub fn exp_to_advance(level: i32, marks: i32) -> Option<i64> {
    if !(1..MAX_LEVEL).contains(&level) {
        return None;
    }
    Some(scaled(level, marks))
}

/// Exp at the top of the ladder that brings the Old Signal to the screen
/// instead of a glyph, with `marks`.
pub fn exp_to_seek(marks: i32) -> i64 {
    scaled(MAX_LEVEL, marks)
}

/// LoGD's dragon-kill scaling, transcribed (`lib/experience.php`, the same
/// formula as the door's `exp_to_advance`): every threshold climbs by a
/// quarter of the level times a hundred per mark, rounded.
fn scaled(level: i32, marks: i32) -> i64 {
    let base = EXP_TO_ADVANCE[(level - 1) as usize] as f64;
    let scale = (f64::from(marks) / 4.0) * f64::from(level) * 100.0;
    (base + scale).round() as i64
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
    /// What it does, turn after turn, round and round: the whole of its
    /// mind, shown a turn ahead. A `Heavy` always follows a `Charge`.
    pub pattern: &'static [Intent],
}

/// What a glyph means to do with its turn, shown before you play yours
/// (GAME.md, "The round": block or race is the whole question, and it has
/// to read at a glance).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Intent {
    /// Hits for its hit.
    Hit,
    /// Gathers itself: nothing lands this turn, the next is a `Heavy`.
    Charge,
    /// Hits for twice its hit.
    Heavy,
    /// Floods the deck: [`NOISE_CARDS`] static cards, no damage, and no
    /// block stops it.
    Noise,
}

pub const FOES: [FoeKind; 15] = [
    FoeKind {
        name: "flicker",
        portrait: ["  ▘  ", " ▗▖▝ ", "  ▖  "],
        arrives: "a flicker sputters out of the static. it is barely there.",
        pattern: &[Intent::Hit],
    },
    FoeKind {
        name: "hiss",
        portrait: [" ░ ░ ", "░▒░▒░", " ░ ░ "],
        arrives: "the noise thickens into a hiss. it has edges.",
        pattern: &[Intent::Hit, Intent::Hit, Intent::Noise],
    },
    FoeKind {
        name: "drift",
        portrait: ["  ▚  ", " ▞▚▞ ", "  ▞  "],
        arrives: "a drift slides sideways out of the picture and stops in front of you.",
        pattern: &[Intent::Hit, Intent::Charge, Intent::Heavy],
    },
    FoeKind {
        name: "stray signal",
        portrait: [" ┼ ┼ ", "▐▘ ▝▌", " ┼─┼ "],
        arrives: "a stray signal, still carrying somebody's voice. it turns toward you.",
        pattern: &[Intent::Charge, Intent::Heavy, Intent::Hit, Intent::Hit],
    },
    FoeKind {
        name: "crackle",
        portrait: [" ╪ ╪ ", "▐▚▞▚▌", " ╫ ╫ "],
        arrives: "the screen crackles and something climbs down off it.",
        pattern: &[Intent::Hit, Intent::Noise, Intent::Charge, Intent::Heavy],
    },
    FoeKind {
        name: "ghost frame",
        portrait: [" ░▒░ ", "▐◌ ◌▌", " ▒░▒ "],
        arrives: "a ghost frame: one picture, held too long, burned in. it remembers you.",
        pattern: &[Intent::Noise, Intent::Hit, Intent::Hit],
    },
    FoeKind {
        name: "howler",
        portrait: ["▚▞▚▞▚", "▐▓▒▓▌", "▞▚▞▚▞"],
        arrives: "a howler. the whole street hears it before it sees it.",
        pattern: &[Intent::Charge, Intent::Heavy],
    },
    FoeKind {
        name: "dead pixels",
        portrait: ["▖▗▖▗▖", "▝▘▝▘▝", "▖▗▖▗▖"],
        arrives: "dead pixels, a swarm of them, moving like one thing.",
        pattern: &[Intent::Hit, Intent::Noise, Intent::Hit, Intent::Hit],
    },
    FoeKind {
        name: "carrier",
        portrait: [" ╫╫╫ ", "▐╪═╪▌", " ▟▓▙ "],
        arrives: "a carrier steps out of the static wearing a coat. it is not a runner.",
        pattern: &[Intent::Hit, Intent::Charge, Intent::Heavy, Intent::Hit],
    },
    FoeKind {
        name: "blackout",
        portrait: [" ███ ", "▐█ █▌", " ███ "],
        arrives: "a blackout. where it stands there is no light at all.",
        pattern: &[Intent::Charge, Intent::Heavy, Intent::Hit, Intent::Noise],
    },
    FoeKind {
        name: "feedback",
        portrait: ["╬╬╬╬╬", "▐▞▚▞▌", "╬╬╬╬╬"],
        arrives: "feedback, screaming at itself, and now at you.",
        pattern: &[
            Intent::Hit,
            Intent::Charge,
            Intent::Heavy,
            Intent::Charge,
            Intent::Heavy,
        ],
    },
    FoeKind {
        name: "white noise",
        portrait: ["▒▓▒▓▒", "▓░▓░▓", "▒▓▒▓▒"],
        arrives: "white noise, all of it, walking.",
        pattern: &[
            Intent::Noise,
            Intent::Hit,
            Intent::Hit,
            Intent::Noise,
            Intent::Hit,
        ],
    },
    FoeKind {
        name: "test pattern",
        portrait: ["▐▌▐▌▐", "▓░▓░▓", "▌▐▌▐▌"],
        arrives: "the test pattern comes on. it has never done that with somebody standing here.",
        pattern: &[
            Intent::Hit,
            Intent::Charge,
            Intent::Heavy,
            Intent::Noise,
            Intent::Hit,
        ],
    },
    FoeKind {
        name: "burnout",
        portrait: [" ▓█▓ ", "█▐░▌█", " ▓█▓ "],
        arrives: "a burnout, what is left when a channel runs too hot. it is still hot.",
        pattern: &[
            Intent::Charge,
            Intent::Heavy,
            Intent::Charge,
            Intent::Heavy,
            Intent::Hit,
        ],
    },
    FoeKind {
        name: "interference",
        portrait: ["╪╫╪╫╪", "▐┼╬┼▌", "╫╪╫╪╫"],
        arrives: "interference from somewhere below. this is the top of it.",
        pattern: &[Intent::Hit, Intent::Charge, Intent::Heavy, Intent::Noise],
    },
];

/// The Old Signal: the thing broadcasting under the static, the one fight
/// that is not a glyph of your level. It carries no bits and no exp:
/// putting it down resets the runner, so a purse would be gone before it
/// was counted.
pub const OLD_SIGNAL: FoeKind = FoeKind {
    name: "Old Signal",
    portrait: ["▗╬╬╬▖", "▐█▬█▌", "▟╬█╬▙"],
    arrives: "the static parts. under it something has been broadcasting since before the city. it is the Old Signal, and it heard you.",
    pattern: &[
        Intent::Hit,
        Intent::Noise,
        Intent::Charge,
        Intent::Heavy,
        Intent::Hit,
        Intent::Charge,
        Intent::Heavy,
    ],
};

/// LoGD's dragon is 45 attack, 25 defense, 300 hit points (`dragon.php`,
/// the door's `DRAGON_*`), numbers for its exchange loop. In the round the
/// Old Signal is tuned on its own (`fight/BALANCE.md`): from the top of the
/// wall a first kill lands about one try in two on the `Auto` key and nine
/// in ten for a runner reading the hand, whatever was drafted, and a glass
/// makes the key's try near sure. No dice but the shuffle means the odds
/// move in steps: ten more signal is one more hit to land, and takes ten
/// points or more off the key's chances. The arena's contract holds the
/// band for every build; move these with it.
pub const OLD_SIGNAL_TIER: FoeTier = FoeTier {
    signal: 290,
    attack: 39,
    defense: 22,
    bits: 0,
    exp: 0,
};

/// The line the kill that crosses [`exp_to_seek`] prints under itself.
pub const HEARD_LINE: &str =
    "below the static something has heard you. the next glyph on the road will be it.";

/// The kill, before the reset's own line (GAME.md, "Marks: the reset").
pub const SLAIN_LINE: &str =
    "the Old Signal comes apart, and for a moment every screen in the city goes quiet.";

/// The line under a kill the month's payout already went to (GAME.md,
/// "Economy rules": a mark pays once a month; the amount and the window
/// live in the `deadchannel_old_signal_slain` reward template).
pub const OLD_SIGNAL_PAID_THIS_MONTH_LINE: &str =
    "the house paid for a broadcast this month already. the mark is yours all the same.";

/// The line under a kill whose payout failed to land; the service logs it.
pub const OLD_SIGNAL_TILL_JAMMED_LINE: &str =
    "the house's till is jammed and pays nothing tonight. the mark is yours all the same.";

/// Titles by marks, one per rung (LoGD's dragon-kill titles, neutral
/// names; placeholder copy, design review). `title(0)` is none; past the
/// last rung the last one holds.
pub const TITLES: [&str; 5] = ["heard", "tuned", "carrier", "broadcast", "old voice"];

pub fn title(marks: i32) -> Option<&'static str> {
    match marks {
        i32::MIN..=0 => None,
        marks => Some(TITLES[((marks - 1) as usize).min(TITLES.len() - 1)]),
    }
}

/// What a glyph pays over LoGD's table, in bits and in exp, as
/// percentages. LoGD paced a season of ten forest fights a day; the climb
/// here is two to three weeks to the first mark on five fights a day
/// (GAME.md, "The daily ration loop"; `road.rs`), so a kill pays over five
/// times LoGD's exp and `sim_test.rs` holds that window. The bits are a
/// separate knob on purpose: exp sets the pace, bits set how far the purse
/// reaches at the armorer and at patch (`fight/BALANCE.md`).
pub const PAY_BITS_PERCENT: i64 = 400;
pub const PAY_EXP_PERCENT: i64 = 540;

/// The armorer's prices as a percentage of the city's ladder
/// (`city/data.rs::COST_LADDER`, LoGD's). The knob that decides how the kit
/// tracks the level: `fight/BALANCE.md`, "The kit follows the level".
pub const PRICE_PERCENT: i64 = 225;

/// What patch charges per missing point of signal, per level, as a
/// percentage of a bit. A bit a point: the road mends nothing, so patch
/// (or the roll) is how the signal comes back (`fight/BALANCE.md`).
pub const PATCH_PERCENT: i64 = 100;

/// [`PATCH_PERCENT`] as the panel says it, so the copy moves with the knob.
pub fn patch_rate() -> String {
    match PATCH_PERCENT {
        50 => "half a bit".to_string(),
        100 => "a bit".to_string(),
        percent => format!("{percent}% of a bit"),
    }
}

/// Every number a balance pass turns, in one value, so the sim and the
/// arena can play the same machine under a candidate set and compare it
/// with the live one in a single run (`fight/BALANCE.md`). The live game
/// plays [`RULES`], built from the constants beside it; nothing in
/// production builds another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub pay_bits_percent: i64,
    pub pay_exp_percent: i64,
    pub price_percent: i64,
    pub trade_in_percent: i64,
    pub patch_percent: i64,
    pub exp_keep_on_death: f64,
    pub crystal_drop_one_in: u32,
    pub bright_signal_percent: i32,
    pub bright_edge_percent: u32,
    pub bright_bits_times: i64,
    pub cart_crystals: i32,
    /// A glyph's signal as a percentage of the creature table's: how many
    /// turns a fight lasts.
    pub foe_signal_percent: i32,
    /// What a glyph's hit lands for, as a percentage of its attack less a
    /// quarter of your defense.
    pub hit_percent: u32,
    /// What a block card holds, as a percentage of your defense.
    pub block_percent: u32,
    /// What a cache on the road holds, as a percentage of what the glyph
    /// of your level pays.
    pub cache_percent: i64,
    /// The Old Signal's numbers.
    pub old_signal: FoeTier,
    /// The drafted cards' numbers, each a share of a strike or a block
    /// (`cards.rs`; a card's rule line says the same number in words).
    pub jab_percent: i32,
    pub siphon_mend_percent: i32,
    pub bulwark_percent: i32,
    pub burn_energy: u8,
    pub ground_percent: i32,
    pub sever_percent: i32,
}

/// The rules the game is played under.
pub const RULES: Rules = Rules {
    pay_bits_percent: PAY_BITS_PERCENT,
    pay_exp_percent: PAY_EXP_PERCENT,
    price_percent: PRICE_PERCENT,
    trade_in_percent: TRADE_IN_PERCENT,
    patch_percent: PATCH_PERCENT,
    exp_keep_on_death: EXP_KEEP_ON_DEATH,
    crystal_drop_one_in: CRYSTAL_DROP_ONE_IN,
    bright_signal_percent: BRIGHT_SIGNAL_PERCENT,
    bright_edge_percent: BRIGHT_EDGE_PERCENT,
    bright_bits_times: BRIGHT_BITS_TIMES,
    cart_crystals: CART_CRYSTALS,
    foe_signal_percent: FOE_SIGNAL_PERCENT,
    hit_percent: HIT_PERCENT,
    block_percent: BLOCK_PERCENT,
    cache_percent: CACHE_PERCENT,
    old_signal: OLD_SIGNAL_TIER,
    jab_percent: JAB_PERCENT,
    siphon_mend_percent: SIPHON_MEND_PERCENT,
    bulwark_percent: BULWARK_PERCENT,
    burn_energy: BURN_ENERGY,
    ground_percent: GROUND_PERCENT,
    sever_percent: SEVER_PERCENT,
};

impl Rules {
    /// The glyph that answers a runner of `level`: one kind and one tier
    /// per level, clamped to the table, its pay scaled.
    pub fn foe(&self, level: i32) -> (usize, &'static FoeKind, FoeTier) {
        let index = (level.clamp(1, MAX_LEVEL) - 1) as usize;
        let tier = FOE_TIERS[index];
        (
            index,
            &FOES[index],
            FoeTier {
                signal: (tier.signal * self.foe_signal_percent / 100).max(1),
                bits: tier.bits * self.pay_bits_percent / 100,
                exp: tier.exp * self.pay_exp_percent / 100,
                ..tier
            },
        )
    }

    /// What a strike card lands for: your attack less half the glyph's
    /// defense, never under one. The weapon tier is in the attack, so the
    /// armorer's wall is the strike's ladder.
    pub fn strike(&self, attack: u32, foe_defense: u32) -> i32 {
        (attack as i32 - (foe_defense / 2) as i32).max(1)
    }

    /// What a surge lands for: two strikes and half of a third, for two
    /// energy and one card.
    pub fn surge(&self, strike: i32) -> i32 {
        strike * 2 + (strike + 1) / 2
    }

    /// What a block card holds: a share of your defense, never under one.
    /// The armor tier is in the defense.
    pub fn block(&self, defense: u32) -> i32 {
        ((defense * self.block_percent / 100) as i32).max(1)
    }

    /// What a glyph's hit lands for before your block: its attack less a
    /// quarter of your defense, scaled, never under one. A heavy lands for
    /// twice this.
    pub fn hit(&self, foe_attack: u32, defense: u32) -> i32 {
        ((foe_attack.saturating_sub(defense / 4) * self.hit_percent / 100) as i32).max(1)
    }

    /// `percent` of `n`, rounded up: how a drafted card's number comes
    /// off a strike or a block.
    pub fn share(&self, n: i32, percent: i32) -> i32 {
        (n * percent + 99) / 100
    }

    /// The bits in a cache on the road for a runner of `level`.
    pub fn cache(&self, level: i32) -> i64 {
        self.foe(level).2.bits * self.cache_percent / 100
    }

    /// The glyph a level below a runner of `level`, its pay cut to
    /// [`LOWER_PAY_PERCENT`]. `None` at level 1: there is nothing below
    /// the flicker.
    pub fn lower_foe(&self, level: i32) -> Option<(usize, &'static FoeKind, FoeTier)> {
        match level {
            i32::MIN..=1 => None,
            level => {
                let (index, kind, tier) = self.foe(level - 1);
                Some((
                    index,
                    kind,
                    FoeTier {
                        bits: tier.bits * LOWER_PAY_PERCENT / 100,
                        exp: tier.exp * LOWER_PAY_PERCENT / 100,
                        ..tier
                    },
                ))
            }
        }
    }

    /// The bright glyph that answers a runner of `level`: the kind of the
    /// level, its tier lifted, its bits multiplied, its exp plain.
    pub fn bright_foe(&self, level: i32) -> (usize, &'static FoeKind, FoeTier) {
        let (index, kind, tier) = self.foe(level);
        (
            index,
            kind,
            FoeTier {
                signal: tier.signal * self.bright_signal_percent / 100,
                attack: (tier.attack * self.bright_edge_percent).div_ceil(100),
                defense: (tier.defense * self.bright_edge_percent).div_ceil(100),
                bits: tier.bits * self.bright_bits_times,
                exp: tier.exp,
            },
        )
    }

    /// The price on the armorer's wall for `tier` (1 to 15).
    pub fn price(&self, tier: i32) -> i64 {
        i64::from(crate::app::deadchannel::city::data::COST_LADDER[(tier - 1) as usize])
            * self.price_percent
            / 100
    }

    /// What the armorer pays for a carried `tier`; nothing for bare hands.
    pub fn trade_in(&self, tier: i32) -> i64 {
        match tier {
            0 => 0,
            tier => self.price(tier) * self.trade_in_percent / 100,
        }
    }
}

/// The live glyph of `level` ([`Rules::foe`] under [`RULES`]).
pub fn foe_for_level(level: i32) -> (usize, &'static FoeKind, FoeTier) {
    RULES.foe(level)
}

/// What a glyph a level down pays, as a percentage of its own pay, bits
/// and exp alike. LoGD's slumming pays the lower creature in full; here the
/// step down is the way out of a fight you cannot win, not a cheaper farm,
/// so it pays half. Rounded down.
pub const LOWER_PAY_PERCENT: i64 = 50;

/// The live glyph a level below ([`Rules::lower_foe`] under [`RULES`]).
pub fn lower_foe_for_level(level: i32) -> Option<(usize, &'static FoeKind, FoeTier)> {
    RULES.lower_foe(level)
}

/// One fair kill in this many leaves a crystal in the static: LoGD's
/// forest gem, the small chance of something special. A step down never
/// leaves one (the way out is not a farm), and a bright glyph always does.
pub const CRYSTAL_DROP_ONE_IN: u32 = 12;

/// What a bright glyph has over the glyph of its level: half as much
/// signal again, 15% more attack and defense (rounded up), and twice the
/// bits. The exp is the plain glyph's: the bright one is for the crystal
/// and the purse, and the climb's pace stays the rations' (GAME.md, "The
/// daily ration loop"). `arena_test.rs` holds how it reads from a kit
/// level with the runner.
pub const BRIGHT_SIGNAL_PERCENT: i32 = 150;
pub const BRIGHT_EDGE_PERCENT: u32 = 115;
pub const BRIGHT_BITS_TIMES: i64 = 2;

/// The live bright glyph ([`Rules::bright_foe`] under [`RULES`]).
pub fn bright_foe_for_level(level: i32) -> (usize, &'static FoeKind, FoeTier) {
    RULES.bright_foe(level)
}

/// A glyph's signal over the creature table's. The table is LoGD's and
/// was paced for its exchange loop; this is the round's length
/// (`fight/BALANCE.md`).
pub const FOE_SIGNAL_PERCENT: i32 = 100;

/// A glyph's hit, as a percentage of its attack less a quarter of your
/// defense.
pub const HIT_PERCENT: u32 = 185;

/// A block card, as a percentage of your defense. Under a hit on purpose:
/// one block never stops one hit whole, so blocking is a choice and not a
/// reflex (GAME.md, "The round").
pub const BLOCK_PERCENT: u32 = 60;

/// The drafted cards (`cards.rs`), each as a percentage of the strike or
/// the block it is built on, rounded up. The card's rule line and the
/// guide say the same number in words: move them together.
///
/// A jab, of a strike.
pub const JAB_PERCENT: i32 = 75;
/// What a siphon mends, of a strike.
pub const SIPHON_MEND_PERCENT: i32 = 50;
/// A bulwark, of a block.
pub const BULWARK_PERCENT: i32 = 250;
/// Energy a burn hands back before any static feeds it; each static card
/// it burns is one more.
pub const BURN_ENERGY: u8 = 2;
/// What a ground adds for each static card in the hand, of a strike.
pub const GROUND_PERCENT: i32 = 100;
/// A sever once the glyph is at half its signal or less, of a strike.
pub const SEVER_PERCENT: i32 = 250;

/// Static cards a `Noise` turn puts into the deck.
pub const NOISE_CARDS: usize = 2;

/// What a cache on the road holds, as a percentage of what the glyph of
/// your level pays.
pub const CACHE_PERCENT: i64 = 50;

/// What a glass at Dead Air costs, in crystals. One glass a day.
pub const DRINK_CRYSTALS: i32 = 1;

/// What the day's glass adds to attack (static on ice) or defense (dead
/// air, neat) at `level`: a point, and one more every four levels.
pub fn drink_edge(level: i32) -> i32 {
    1 + level / 4
}

/// What the test pattern adds to the signal's max per level, until the
/// roll: a fifth of [`SIGNAL_PER_LEVEL`].
pub const DRINK_SIGNAL_PER_LEVEL: i32 = 2;

/// What the blade shop asks for the next tier up from the one a slot
/// carries, in crystals and nothing else.
pub const CART_CRYSTALS: i32 = 3;

/// The locker's cut of every deposit, a percentage of what goes in,
/// rounded up so no deposit is free. LoGD's bank takes nothing and pays
/// interest; the locker pays none and takes this. Withdrawals are free.
pub const LOCKER_FEE_PERCENT: i64 = 10;

/// What the bits machine lends a runner, per level: the cap a loan fills
/// the debt up to. A level-2 runner can borrow the two tier-1 pieces.
pub const LOAN_PER_LEVEL: i64 = 50;

/// The machine's fee on every loan, a percentage of what it hands over,
/// rounded up, added to the debt once when it lends. The debt never grows
/// after that: a day roll is any touch on the row (a connect is one), so a
/// daily rate would bill a runner for days they never fought.
pub const LOAN_FEE_PERCENT: i64 = 10;

/// The share of every glyph's bits the machine takes toward the debt
/// before the rest reaches your hand, a percentage, rounded down.
pub const GARNISH_PERCENT: i64 = 50;

/// `amount` times `percent` over a hundred, rounded up: the locker's cut
/// and the machine's fee never round away to nothing.
pub fn percent_up(amount: i64, percent: i64) -> i64 {
    (amount * percent + 99) / 100
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

/// A crystal left by a kill, under the kill's own line.
pub const CRYSTAL_LINE: &str = "something is left in the static where it stood. a crystal.";

/// Static into the deck, under the hit that landed. It says where the
/// card came from and what to do about it every time: a dead card nobody
/// can account for reads as a bug.
pub const STATIC_LINE: &str = "that hit left a static card in your deck. play it to throw it out.";

/// A draft owed, under the kill that reached its level and under a pick
/// that leaves another owed.
pub const DRAFT_LINE: &str = "a new card is waiting for you on the road.";

/// The bright glyph's arrival, under the glyph's own line.
pub const BRIGHT_LINE: &str = "this one is bright. it burns harder, and it is carrying something.";

/// The lower glyph's arrival, under its own line: the step down is said.
pub const STEPPED_DOWN_LINE: &str = "you went looking for something smaller. it pays like it.";

/// A rest on the road: the static shaken out of the deck.
pub const CLEAR_LINE: &str =
    "a dry doorway out of the rain. you shake the static out of your deck.";

/// A cache on the road, ahead of what it held.
pub const CACHE_LINES: [&str; 3] = [
    "a locker somebody never came back for.",
    "a coat on a railing, the pockets still heavy.",
    "a dead terminal with its tray hanging open.",
];
