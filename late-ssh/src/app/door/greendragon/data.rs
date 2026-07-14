//! Balance data for the Green Dragon door.
//!
//! Two different things live here, with two different provenances:
//!
//! 1. **Numeric balance tables** — the cost/power ladders, per-level creature
//!    stat blocks, the experience curve, master/dragon stats. These are
//!    transcribed from the established LoGD balance (the DragonPrime-lineage
//!    seed `jimlunsford/lotgd@master`). Game mechanics and the numbers that
//!    express them are not copyrightable, so transcribing them keeps the game
//!    feeling authentic instead of re-tuned, with no licensing entanglement.
//!
//! 2. **Flavor text** — creature names, master names, and gear names. These are
//!    *original to late.sh*, written fresh. We deliberately do **not** reuse the
//!    seed's names: that seed is CC BY-NC-SA, whose NonCommercial + ShareAlike
//!    terms conflict with shipping inside late.sh. Names are the copyrightable
//!    layer, so ours are our own and carry no obligation.
//!
//! Numeric source files (all `jimlunsford/lotgd@master`):
//! - cost ladder / creature / master stat seeds: `lib/installer/installer_sqlstatements.php`
//! - experience curve + dragonkill scaling: `lib/experience.php`
//! - combat formula: `lib/battle-skills.php` (`rolldamage`)
//! - dragon stats / gating: `dragon.php`, `lib/forest.php`

/// Maximum character level in the base game (`maxlevel` default). Reaching it
/// requires beating the level-14 master and unlocks the Green Dragon.
pub const MAX_LEVEL: u8 = 15;

/// The shared weapon/armor cost ladder. Every cosmetic weapon/armor "type" in
/// LoGD uses this identical ladder; the tier (1..=15) is the only thing that
/// matters for balance. `COST_LADDER[tier - 1]` is the buy price in gold for a
/// weapon/armor of that tier; the item's power (weapon damage / armor defense)
/// equals the tier itself.
///
/// Buying applies a 75% trade-in on the currently equipped item's cost.
pub const COST_LADDER: [u32; 15] = [
    48, 225, 585, 990, 1575, 2250, 2790, 3420, 4230, 5040, 5850, 6840, 8010, 9000, 10350,
];

/// Trade-in fraction credited from the currently equipped item's cost when
/// upgrading (LoGD: `cost - 0.75 * current_value`).
pub const TRADE_IN_FRACTION: f32 = 0.75;

/// One forest creature's combat stats. In LoGD every creature of a given level
/// shares the same stats; the name + weapon are pure flavor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureTier {
    pub hp: u32,
    pub attack: u32,
    pub defense: u32,
    pub gold: u32,
    pub exp: u32,
}

/// Per-level creature stat blocks for forest levels 1..=16, indexed by
/// `level - 1`. (LoGD levels 17-18 are degenerate easter-egg "Loneliness"
/// entries and are intentionally omitted.)
pub const CREATURES: [CreatureTier; 16] = [
    CreatureTier {
        hp: 10,
        attack: 1,
        defense: 1,
        gold: 36,
        exp: 14,
    },
    CreatureTier {
        hp: 21,
        attack: 3,
        defense: 3,
        gold: 97,
        exp: 24,
    },
    CreatureTier {
        hp: 32,
        attack: 5,
        defense: 4,
        gold: 148,
        exp: 34,
    },
    CreatureTier {
        hp: 43,
        attack: 7,
        defense: 6,
        gold: 162,
        exp: 45,
    },
    CreatureTier {
        hp: 53,
        attack: 9,
        defense: 7,
        gold: 199,
        exp: 55,
    },
    CreatureTier {
        hp: 64,
        attack: 11,
        defense: 8,
        gold: 233,
        exp: 66,
    },
    CreatureTier {
        hp: 74,
        attack: 13,
        defense: 10,
        gold: 268,
        exp: 77,
    },
    CreatureTier {
        hp: 84,
        attack: 15,
        defense: 11,
        gold: 302,
        exp: 89,
    },
    CreatureTier {
        hp: 94,
        attack: 17,
        defense: 13,
        gold: 336,
        exp: 101,
    },
    CreatureTier {
        hp: 105,
        attack: 19,
        defense: 14,
        gold: 369,
        exp: 114,
    },
    CreatureTier {
        hp: 115,
        attack: 21,
        defense: 15,
        gold: 402,
        exp: 127,
    },
    CreatureTier {
        hp: 125,
        attack: 23,
        defense: 17,
        gold: 435,
        exp: 141,
    },
    CreatureTier {
        hp: 135,
        attack: 25,
        defense: 18,
        gold: 467,
        exp: 156,
    },
    CreatureTier {
        hp: 145,
        attack: 27,
        defense: 20,
        gold: 499,
        exp: 172,
    },
    CreatureTier {
        hp: 155,
        attack: 29,
        defense: 21,
        gold: 531,
        exp: 189,
    },
    CreatureTier {
        hp: 166,
        attack: 31,
        defense: 22,
        gold: 563,
        exp: 207,
    },
];

/// Look up the creature stat block for a forest level, clamped to 1..=16.
pub fn creature_tier(level: u8) -> CreatureTier {
    let idx = (level.clamp(1, 16) - 1) as usize;
    CREATURES[idx]
}

/// Flavor (name, weapon) pairs per forest level 1..=16, indexed by `level - 1`.
/// Stats always come from [`CREATURES`]; this list only varies the prose. These
/// names are original to late.sh (see the module note on licensing); more can be
/// appended per level without touching the stat tables.
pub const CREATURE_NAMES: [&[(&str, &str)]; 16] = [
    &[
        ("Mangy Goblin", "Chipped Cleaver"),
        ("Field Rat Swarm", "Gnashing Teeth"),
        ("Ditch Imp", "Bent Spoon"),
        ("Sourwing Bat", "Needle Fangs"),
    ],
    &[
        ("Bog Lurcher", "Slick Tendrils"),
        ("Fen Toad, Grown Wrong", "Barbed Tongue"),
        ("Stray Cur Pack", "Ragged Bites"),
        ("Hollow Stump Sprite", "Splinter Darts"),
    ],
    &[
        ("Bandit Scout", "Worn Crossbow"),
        ("Molting Harpy", "Filthy Talons"),
        ("Ridge Wolf", "Worrying Jaws"),
        ("Tinker Gone Feral", "Sharpened Trowel"),
    ],
    &[
        ("Snow Troll", "Frostbitten Fists"),
        ("Torch-lit Mob", "Pitchforks"),
        ("Barrow Rat King", "Crown of Teeth"),
        ("Creekbed Naiad", "Drowning Grip"),
    ],
    &[
        ("Thornback Boar", "Goring Tusks"),
        ("Deserter Sergeant", "Stolen Halberd"),
        ("Weeping Willow, Awake", "Lashing Boughs"),
        ("Chalk Gargoyle", "Grinding Knuckles"),
    ],
    &[
        ("Spore Wraith", "Choking Cloud"),
        ("Adder Queen", "Venom Spray"),
        ("Charred Scarecrow", "Smoldering Grasp"),
        ("Gully Ogre", "Fence-post Club"),
    ],
    &[
        ("Gravel Golem", "Crushing Slam"),
        ("Poacher-King", "Barbed Snares"),
        ("Wisp-eaten Knight", "Rusted Flail"),
        ("Cave Mantis", "Scything Arms"),
    ],
    &[
        ("Veiled Temptress", "Beguiling Whisper"),
        ("Bone Collector", "Sack of Hooks"),
        ("Stone-eyed Basilisk", "Petrifying Stare"),
        ("Moor Hag", "Knotted Sinew"),
    ],
    &[
        ("Marsh Crone", "Hexed Nettles"),
        ("Feathered Serpent", "Diving Strike"),
        ("Grave-mold Shambler", "Rotting Embrace"),
        ("Twin-tailed Lynx", "Razor Pounce"),
    ],
    &[
        ("Clockwork Sentry", "Whirring Blades"),
        ("Embermaw Salamander", "Gout of Flame"),
        ("The Pale Auctioneer", "Binding Contract"),
        ("River Troll Matron", "Millstone Fists"),
    ],
    &[
        ("Gloomfinch Flock", "Razor Feathers"),
        ("Headless Duelist", "Remembered Saber"),
        ("Frost Revenant", "Icicle Spear"),
        ("Burrowing Horror", "Grasping Mandibles"),
    ],
    &[
        ("Mirror Shade", "Stolen Face"),
        ("Storm-called Djinn", "Bottled Thunder"),
        ("Widow of the Ford", "Wet Silk Shroud"),
        ("Ironwood Treant", "Heartwood Hammer"),
    ],
    &[
        ("Three-Headed Hound", "Snapping Maws"),
        ("Hill Giant", "Uprooted Oak"),
        ("The Toll Reaper", "Ferryman's Scythe"),
        ("Obsidian Wyrmling", "Glass-edged Tail"),
    ],
    &[
        ("Ronin of Ash", "Twin Embers"),
        ("Chimera of the Vale", "Threefold Fury"),
        ("Sunken Bell Spirit", "Deafening Toll"),
        ("Warlord's Ghost", "Phantom Warhorn"),
    ],
    &[
        ("Hollow Archmage", "Unspoken Word"),
        ("Elder Manticore", "Volley of Spines"),
        ("The Starving Saint", "Beatific Hunger"),
        ("Nightmare Courser", "Trampling Dark"),
    ],
    &[
        ("The Long Dark", "Creeping Dread"),
        ("Herald of the Dragon", "Green-fire Brand"),
        ("Mountain That Walks", "Avalanche Fist"),
        ("The Unlit Lighthouse", "Sweeping Shadow"),
    ],
];

/// Experience required to advance *from* the indexed level to the next, for
/// levels 1..=15 (index `level - 1`). Level 15 is the cap; its entry is the
/// threshold LoGD still stores but no normal advance occurs past it.
///
/// LoGD additionally scales each threshold by dragon kills:
/// `round(base + (dragonkills / 4) * level * 100)`. See [`exp_to_advance`].
pub const EXP_TO_ADVANCE: [u64; 15] = [
    100, 400, 1002, 1912, 3140, 4707, 6641, 8985, 11795, 15143, 19121, 23840, 29437, 36071, 43930,
];

/// Experience needed to advance from `level` to `level + 1`, including LoGD's
/// dragonkill scaling. Levels at/above [`MAX_LEVEL`] reuse the level-15 base.
pub fn exp_to_advance(level: u8, dragon_kills: u32) -> u64 {
    let idx = (level.clamp(1, MAX_LEVEL) - 1) as usize;
    let base = EXP_TO_ADVANCE[idx];
    let scale = (dragon_kills as f64 / 4.0) * level as f64 * 100.0;
    (base as f64 + scale).round() as u64
}

/// A level master fought at Bluspring's Warrior Training to advance a level.
#[derive(Clone, Copy, Debug)]
pub struct Master {
    pub name: &'static str,
    pub weapon: &'static str,
}

/// The 14 named masters, indexed by `level - 1`. You fight master N to advance
/// from level N to N+1; beating the level-14 master unlocks level 15 and the
/// Dragon. Names are original to late.sh; stats are derived (see
/// [`master_stats`]): attack = defense = 2*level, hp = 11*level (level 1 = 12).
pub const MASTERS: [Master; 14] = [
    Master {
        name: "Sergeant Brann",
        weapon: "Drill Baton",
    },
    Master {
        name: "Mistress Veil",
        weapon: "Quick Rapier",
    },
    Master {
        name: "Old Garrick",
        weapon: "Notched Maul",
    },
    Master {
        name: "Bram the Bear",
        weapon: "Studded Club",
    },
    Master {
        name: "Seer Anwyn",
        weapon: "Silent Will",
    },
    Master {
        name: "Thane Korl",
        weapon: "Dwarf-forged Axe",
    },
    Master {
        name: "Ranger Esk",
        weapon: "Yew Longbow",
    },
    Master {
        name: "Sir Aldric",
        weapon: "Broadsword",
    },
    Master {
        name: "The Twin Mara",
        weapon: "Paired Blades",
    },
    Master {
        name: "Master Sojin",
        weapon: "Open Palm",
    },
    Master {
        name: "Halcyon",
        weapon: "Ringed Chakram",
    },
    Master {
        name: "Wardren the Grey",
        weapon: "Elder Bow",
    },
    Master {
        name: "Goliath Vorne",
        weapon: "Greatsword",
    },
    Master {
        name: "Veotha the Last",
        weapon: "Severing Touch",
    },
];

/// Original weapon names by tier 1..=15, indexed by `tier - 1`. Purely cosmetic:
/// every tier shares the one [`COST_LADDER`]/power ladder, so the name carries no
/// mechanical weight. Tier 0 (unarmed) is rendered separately by
/// [`weapon_name`]. These names are late.sh's own.
pub const WEAPON_NAMES: [&str; 15] = [
    "Rusted Knife",
    "Worn Shortsword",
    "Iron Hatchet",
    "Oak Cudgel",
    "Bronze Mace",
    "Steel Saber",
    "Forester's Axe",
    "Knight's Longsword",
    "Warhammer",
    "Duskblade",
    "Serrated Glaive",
    "Moonsteel Sword",
    "Obsidian Greataxe",
    "Stormpike",
    "Dragonbane",
];

/// Original armor names by tier 1..=15, indexed by `tier - 1`. Cosmetic, like
/// [`WEAPON_NAMES`]; tier 0 (unarmored) is rendered separately by [`armor_name`].
pub const ARMOR_NAMES: [&str; 15] = [
    "Padded Cloth",
    "Boiled Leather",
    "Studded Hide",
    "Ringmail",
    "Chainmail",
    "Scale Vest",
    "Brigandine",
    "Banded Plate",
    "Half Plate",
    "Knight's Plate",
    "Tempered Cuirass",
    "Moonsteel Plate",
    "Obsidian Harness",
    "Stormguard Plate",
    "Dragonscale",
];

/// Display name for an equipped weapon tier (0 = unarmed), clamped to range.
pub fn weapon_name(tier: u8) -> &'static str {
    match tier {
        0 => "Fists",
        t => WEAPON_NAMES[(t.min(MAX_LEVEL) - 1) as usize],
    }
}

/// Display name for an equipped armor tier (0 = unarmored), clamped to range.
pub fn armor_name(tier: u8) -> &'static str {
    match tier {
        0 => "None",
        t => ARMOR_NAMES[(t.min(MAX_LEVEL) - 1) as usize],
    }
}

/// Combat stats (attack, defense, hp) for the master at `level` (1..=14).
pub fn master_stats(level: u8) -> (u32, u32, u32) {
    let l = level.clamp(1, 14) as u32;
    let hp = if l == 1 { 12 } else { 11 * l };
    (2 * l, 2 * l, hp)
}

/// The Green Dragon's base combat stats (`dragon.php`). LoGD scales these up by
/// the player's spent dragon points; the base is the level-15 challenge.
pub const DRAGON_ATTACK: u32 = 45;
pub const DRAGON_DEFENSE: u32 = 25;
pub const DRAGON_HP: u32 = 300;

/// The death realm's overlord NPC. Original name — upstream's `deathoverlord`
/// setting defaults to "Ramius", which is theirs.
pub const DEATH_OVERLORD: &str = "Morvane";

/// The taunt pool appended to death news items (upstream's `taunts` table +
/// `lib/taunt.php`, picked uniformly at random). **All lines original to
/// late.sh** — the seed's ~26 taunts are theirs.
pub const TAUNTS: [&str; 15] = [
    "\"The forest keeps what it kills,\" say the old folk, nodding.",
    "The village children are already re-enacting it with sticks.",
    "A bard has begun a ballad about it. It is not flattering.",
    "The crows held a moment of silence, then a feast.",
    "The gravedigger measures by eye these days. Practice.",
    "Somewhere, a master shakes their head and pockets the tuition.",
    "\"Could've been me,\" mutters a farmer, comfortably alive.",
    "The armory has already re-stocked the departed's size.",
    "Duskmere's obituary column grows another line longer.",
    "The worms send their regards, and their thanks.",
    "An empty stool at the tavern is filled before it cools.",
    "The healer notes, dryly, that prevention was cheaper.",
    "Wagers were settled at the gate before the body was cold.",
    "The dragon, informed, was heard to yawn.",
    "A moment of silence was proposed, and voted down.",
];

/// Pick one random death taunt.
pub fn taunt(rng: &mut impl rand::Rng) -> &'static str {
    TAUNTS[rng.gen_range(0..TAUNTS.len())]
}

/// Creature names with larcenous habits: while one of these stands in a
/// fight and the player carries a heavy purse, it tries — once per fight —
/// to cut it (see `state`'s purse-cut roll). **An original late.sh
/// mechanic**: stock LoGD 1.1.2 ships no mid-fight steal (verified against
/// the source: no creature-ai script implements one), so the numbers here
/// are ours, not a port.
pub const BANDIT_CREATURES: [&str; 5] = [
    "Bandit Scout",
    "Deserter Sergeant",
    "Poacher-King",
    "The Pale Auctioneer",
    "The Toll Reaper",
];

/// Battle-end flavor for a slain forest creature (the upstream `creatures`
/// table carries per-creature win/lose lines; ours is an original shared
/// pool, drawn at random when the last foe falls).
pub const FOE_DYING_LINES: [&str; 10] = [
    "It ends with a surprised little sound, and then silence.",
    "Whatever drove it snaps like a dry twig.",
    "It sinks down as if it had only ever wanted to rest.",
    "The forest exhales; one terror fewer under its boughs.",
    "Its weapon outlives it, quivering in the dirt.",
    "It backs away two steps, both of them too late.",
    "You wipe your blade on the moss. The moss objects less.",
    "Something small watches from the ferns, and starts to sing.",
    "It curses you in a tongue you're glad not to know.",
    "The quiet afterward is its own kind of loot.",
];

/// Battle-end flavor for a forest creature that wins (drawn at random when
/// the player falls to one). Original pool, same rationale as
/// [`FOE_DYING_LINES`].
pub const FOE_GLOATING_LINES: [&str; 10] = [
    "The last thing you hear is it going through your pockets.",
    "It doesn't even stay to watch you finish falling.",
    "Your weapon lands somewhere in the leaves, unhurried.",
    "It steps over you like a root in the path.",
    "Above you, the crows change their plans for the evening.",
    "It takes a souvenir. You'd rather not know which.",
    "The ferns close over the spot as if you'd never stood there.",
    "It hums something tuneless as it ambles away.",
    "Your last thought is that the healer warned you. Twice.",
    "It salutes you, almost respectfully. Almost.",
];

/// One random dying line for a slain creature.
pub fn foe_dying_line(rng: &mut impl rand::Rng) -> &'static str {
    FOE_DYING_LINES[rng.gen_range(0..FOE_DYING_LINES.len())]
}

/// One random gloat for a creature that has just slain the player.
pub fn foe_gloating_line(rng: &mut impl rand::Rng) -> &'static str {
    FOE_GLOATING_LINES[rng.gen_range(0..FOE_GLOATING_LINES.len())]
}

// --- phase-3 building NPCs (all names original to late.sh) -------------------

/// The inn (upstream's setting defaults name it; ours is our own).
pub const INN_NAME: &str = "The Sleeping Stag";
/// The barkeep who takes bribes and stocks the potion shelf (Cedrik-analog).
pub const BARKEEP: &str = "Hobb";
/// The bard whose song is a nightly gamble (Seth-analog). Doubles as the
/// romance partner for first-style characters, exactly as upstream's bard
/// doubles for its.
pub const BARD: &str = "Alder";
/// The barmaid, the romance partner for second-style characters
/// (Violet-analog).
pub const BARMAID: &str = "Wren";
/// The ostler who runs the stables.
pub const OSTLER: &str = "Fenwick";
/// The one-eyed gambler at the Crooked Wheel (the "old man").
pub const GAMBLER: &str = "the one-eyed gambler";
/// The bounty broker sulking in the inn's darkest booth (the Dag
/// Durnick-analog; upstream's name is theirs, this one is ours).
pub const BOUNTY_BROKER: &str = "Varn";
/// The clan registrar behind the lobby's polished desk (the
/// Karissa-analog; upstream's name is theirs, this one is ours).
pub const CLAN_REGISTRAR: &str = "Maren";

/// The six ways a haunt goes wrong (`case_haunt3.php` rolls one of six
/// failure vignettes; the news carries the botch either way). **All lines
/// original to late.sh** — `{name}` is replaced with the target's name.
pub const HAUNT_FUMBLES: [&str; 6] = [
    "You rear up over {name}'s bed, terrible and vast - and they roll over, dead asleep, and miss the whole performance.",
    "You begin the wail you practiced, but {name}'s dog starts howling along, and the effect is entirely lost.",
    "You sweep toward {name} in a rush of grave-cold air, snag on the bedpost, and dissipate with an embarrassed pop.",
    "{name} opens one eye, mutters \"not tonight,\" and pulls the blanket over their head. You drift off, deflated.",
    "You loom over {name} in glorious dread - then catch your own reflection in the window and flee shrieking.",
    "{name} sits bolt upright, stares straight through you, and asks if you could haunt the tax collector instead.",
];

/// One random haunt-fumble vignette, with the target's name folded in.
pub fn haunt_fumble(rng: &mut impl rand::Rng, name: &str) -> String {
    HAUNT_FUMBLES[rng.gen_range(0..HAUNT_FUMBLES.len())].replace("{name}", name)
}

/// The romance partner for an address style: first-style characters court
/// the barmaid, second-style ones the bard (upstream keys this off `sex`;
/// unchosen characters render first-style everywhere).
pub fn partner(style: super::model::AddressStyle) -> &'static str {
    match style {
        super::model::AddressStyle::Second => BARD,
        _ => BARMAID,
    }
}

/// One of the inn's drinks (`modules/drinks.php` + its installer seed). A
/// field-for-field transcription of the stock drink rows: when both branch
/// weights are set, one `e_rand(1, hp+turn)` roll picks the HP or the turn
/// effect; the `always_*` flags fire both unconditionally. `hp_percent`
/// nonzero means the HP delta is `round(maxhp * pct/100)`, else it rolls
/// `e_rand(hp_min, hp_max)`. HP results floor at 1 (and ride over max as an
/// overheal, exactly upstream); turn results floor at 0. Names original.
#[derive(Clone, Copy, Debug)]
pub struct Drink {
    pub name: &'static str,
    /// Price is `level * cost_per_level`.
    pub cost_per_level: u64,
    /// Drunkenness added per glass.
    pub drunkenness: u32,
    /// Hard liquor: capped per day (`hardlimit` 3).
    pub hard: bool,
    /// Branch weights: `e_rand(1, hp+turn) <= hp` takes the HP branch.
    pub hp_chance: u32,
    pub turn_chance: u32,
    /// Fire the HP/turn effects unconditionally instead of branching.
    pub always_both: bool,
    /// Percent-of-max HP delta when nonzero (`round(maxhp*pct/100)`).
    pub hp_percent: u32,
    /// Otherwise the HP delta rolls this inclusive range (signed).
    pub hp_range: (i32, i32),
    /// The turn delta's inclusive range (signed).
    pub turn_range: (i32, i32),
    pub buff_name: &'static str,
    pub buff_rounds: u32,
    pub atk_mod: f32,
    pub def_mod: f32,
    pub dmg_mod: f32,
    pub damage_shield: f32,
    pub wearoff: &'static str,
}

/// The three stock drinks (the installer's ale / habanero martini / mule
/// analogs): costs 10/15/25 per level, +33/+50/+50 drunkenness, and the exact
/// stock effect rolls and buffs. Names and prose original.
pub const DRINKS: [Drink; 3] = [
    Drink {
        name: "House Brew",
        cost_per_level: 10,
        drunkenness: 33,
        hard: false,
        hp_chance: 2,
        turn_chance: 1,
        always_both: false,
        hp_percent: 10,
        hp_range: (0, 0),
        turn_range: (1, 1),
        buff_name: "A Warm Buzz",
        buff_rounds: 10,
        atk_mod: 1.25,
        def_mod: 1.0,
        dmg_mod: 1.0,
        damage_shield: 0.0,
        wearoff: "The warm buzz fades from your arms.",
    },
    Drink {
        name: "Fire Shot",
        cost_per_level: 15,
        drunkenness: 50,
        hard: true,
        hp_chance: 0,
        turn_chance: 0,
        always_both: true,
        hp_percent: 0,
        hp_range: (-5, 15),
        turn_range: (-1, 1),
        buff_name: "Firehands",
        buff_rounds: 12,
        atk_mod: 1.1,
        def_mod: 0.9,
        dmg_mod: 1.5,
        damage_shield: 0.0,
        wearoff: "The fire in your hands gutters out.",
    },
    Drink {
        name: "Black Cask",
        cost_per_level: 25,
        drunkenness: 50,
        hard: true,
        hp_chance: 2,
        turn_chance: 3,
        always_both: false,
        hp_percent: 0,
        hp_range: (-10, -1),
        turn_range: (1, 3),
        buff_name: "Caskskin",
        buff_rounds: 15,
        atk_mod: 1.0,
        def_mod: 1.0,
        dmg_mod: 1.3,
        damage_shield: 1.3,
        wearoff: "Your cask-hardened skin softens again.",
    },
];

/// The flirt ladder's seven rungs, in order (labels original; thresholds live
/// in `model::FLIRT_LADDER`).
pub const FLIRT_RUNGS: [&str; 7] = [
    "Catch their eye across the room",
    "Pay a shy compliment",
    "Share a whispered joke",
    "A kiss on the cheek",
    "A long, lingering kiss",
    "An evening upstairs",
    "Ask for their hand",
];

/// Idle-chat flavor from the barmaid, bucketed by `charm + e_rand(-1,1)`
/// (the upstream chat switch: <=0, 1-3, 4-6, 7-9, 10-12, 13-15, 16-18, 19+).
/// All lines original.
pub const CHAT_BARMAID: [&str; 8] = [
    "Wren wipes the bar and somehow never quite reaches your end of it.",
    "Wren nods along politely, eyes drifting to the door behind you.",
    "Wren laughs once at your story, mostly out of professional courtesy.",
    "Wren leans on the bar and asks how the forest is treating you.",
    "Wren pours you the good measure without being asked.",
    "Wren's laugh at your joke turns every head at the bar.",
    "Wren ignores three waving customers to keep talking with you.",
    "Wren blushes when you catch her already looking at you.",
];

/// Idle-chat flavor from the bard, same buckets. All lines original.
pub const CHAT_BARD: [&str; 8] = [
    "Alder tunes his lute with great focus the moment you sit down.",
    "Alder hums politely at your story without missing a chord.",
    "Alder grants your joke one raised eyebrow and half a smile.",
    "Alder sets the lute aside and asks what you've seen out there.",
    "Alder works your name into the chorus, just quietly.",
    "Alder plays the next song to your corner of the room.",
    "Alder loses his place in the verse when you smile.",
    "Alder writes a new verse on the spot; it is unmistakably about you.",
];

/// A stable mount (`stables.php` + the mounts seed). Numbers are upstream's
/// stock three exactly (gems 6/10/16, +1/+2/+3 daily fights, 20/40/60 buffed
/// rounds, attack x1.2); names original.
#[derive(Clone, Copy, Debug)]
pub struct Mount {
    pub name: &'static str,
    pub cost_gems: u64,
    /// Extra forest fights each new day (`mountforestfights`).
    pub forest_fights: u32,
    /// Mounted combat rounds per day (the mount buff's `rounds`).
    pub buff_rounds: u32,
}

/// The stock stable, cheapest first. 1-based `Character::mount` indexes this.
pub const MOUNTS: [Mount; 3] = [
    Mount {
        name: "Moor Pony",
        cost_gems: 6,
        forest_fights: 1,
        buff_rounds: 20,
    },
    Mount {
        name: "Dun Courser",
        cost_gems: 10,
        forest_fights: 2,
        buff_rounds: 40,
    },
    Mount {
        name: "Black Destrier",
        cost_gems: 16,
        forest_fights: 3,
        buff_rounds: 60,
    },
];

/// Attack multiplier while riding (`mountbuff` `atkmod`, all stock mounts).
pub const MOUNT_ATK_MOD: f32 = 1.2;

/// A mercenary for hire (`mercenarycamp.php` + the companions seed). Stats
/// are `base + per_level * buyer_level`, baked at purchase; names original.
#[derive(Clone, Copy, Debug)]
pub struct Mercenary {
    pub name: &'static str,
    pub cost_gold: u64,
    pub cost_gems: u64,
    pub attack: (u32, u32),
    pub defense: (u32, u32),
    pub hp: (u32, u32),
    pub ability: super::combat::CompanionAbility,
    pub dying_text: &'static str,
}

/// The two stock hires (upstream's javelin man and healer, 573g+4gems and
/// 1000g+3gems).
pub const MERCENARIES: [Mercenary; 2] = [
    Mercenary {
        name: "Skarn the Pikeman",
        cost_gold: 573,
        cost_gems: 4,
        attack: (5, 2),
        defense: (1, 2),
        hp: (20, 20),
        ability: super::combat::CompanionAbility::Fight,
        dying_text: "Skarn drops his pike and crumples without a sound.",
    },
    Mercenary {
        name: "Elsbet the Field-Medic",
        cost_gold: 1000,
        cost_gems: 3,
        attack: (1, 1),
        defense: (5, 5),
        hp: (15, 10),
        ability: super::combat::CompanionAbility::Heal(2),
        dying_text: "Elsbet's satchel spills open as she falls, bandages unspooling.",
    },
];

/// The Deepfolk-only hire (`racedwarf.php`'s bear: 600g+4gems, defend-only).
pub const DEEPFOLK_BEAR: Mercenary = Mercenary {
    name: "Crag Bear",
    cost_gold: 600,
    cost_gems: 4,
    attack: (1, 2),
    defense: (5, 2),
    hp: (25, 25),
    ability: super::combat::CompanionAbility::Defend,
    dying_text: "The crag bear takes one blow too many and lumbers off into the trees.",
};

/// The dragon-kill title ladder (`titles` table + `lib/titles.php`): rows of
/// `(dk_threshold, first-style title, second-style title)`. Selection takes
/// the highest threshold at or below the character's kills, picking randomly
/// among rows that share it (upstream supports several per threshold). The
/// two columns are keyed by [`super::model::AddressStyle`] where upstream
/// keys male/female. One rung per kill 0..=31, exactly upstream's 32-row
/// seed — the tier count and thresholds are mechanics; **all title strings
/// are original to late.sh** (the upstream Farmboy-to-God ladder is theirs).
pub const TITLES: &[(u32, &str, &str)] = &[
    (0, "Mudfoot", "Mudlark"),
    (1, "Wyrmscarred", "Wyrmscarred"),
    (2, "Cinderhand", "Cinderhand"),
    (3, "Scalebreaker", "Scalebreaker"),
    (4, "Greenbane", "Greenbane"),
    (5, "Wyrmreaper", "Wyrmreaper"),
    (6, "Fangtaker", "Fangtaker"),
    (7, "Ashlord", "Ashlady"),
    (8, "Emberknight", "Emberknight"),
    (9, "Scorchwarden", "Scorchwarden"),
    (10, "Dragonlord", "Dragonlady"),
    (11, "Hoardtaker", "Hoardtaker"),
    (12, "Cavemaster", "Cavemistress"),
    (13, "Flamebroker", "Flamebroker"),
    (14, "Smokeherald", "Smokeherald"),
    (15, "Doomscale", "Doomscale"),
    (16, "Pyre Prince", "Pyre Princess"),
    (17, "Cinder King", "Cinder Queen"),
    (18, "Wyrm Emperor", "Wyrm Empress"),
    (19, "Duskhallowed", "Duskhallowed"),
    (20, "Wrath of Duskmere", "Wrath of Duskmere"),
    (21, "Stormhallowed", "Stormhallowed"),
    (22, "Skyrender", "Skyrender"),
    (23, "Starclad", "Starclad"),
    (24, "Moonshadowed", "Moonshadowed"),
    (25, "Sunforged", "Sunforged"),
    (26, "Worldwalker", "Worldwalker"),
    (27, "Fateweaver", "Fateweaver"),
    (28, "Godling", "Godling"),
    (29, "Titanheart", "Titanheart"),
    (30, "Worldscourge", "Worldscourge"),
    (31, "The Deathless", "The Deathless"),
];

/// Pick the `(first-style, second-style)` title pair for `dragon_kills`: the
/// rows at the highest threshold not exceeding the kill count, chosen at
/// random among ties (`get_dk_title`). The ladder always has a threshold-0
/// row, so this never comes up empty.
pub fn dk_title_pair(dragon_kills: u32, rng: &mut impl rand::Rng) -> (&'static str, &'static str) {
    let threshold = TITLES
        .iter()
        .filter(|(dk, _, _)| *dk <= dragon_kills)
        .map(|(dk, _, _)| *dk)
        .max()
        .unwrap_or(0);
    let rows: Vec<_> = TITLES
        .iter()
        .filter(|(dk, _, _)| *dk == threshold)
        .collect();
    let (_, a, b) = rows[rng.gen_range(0..rows.len())];
    (a, b)
}

/// Original (name, weapon) flavor for the graveyard's tormentable souls.
/// Upstream flags its entire forest roster `graveyard=1` and overrides every
/// stat at spawn (`case_battle_search.php`), so the pool is pure flavor; ours
/// is a dedicated dead-realm cast. Stats come from [`graveyard_creature_stats`].
pub const GRAVEYARD_CREATURES: [(&str, &str); 10] = [
    ("Restless Shade", "Cold Whisper"),
    ("Grave-bound Wisp", "Flickering Chill"),
    ("The Hollow Mourner", "Endless Keening"),
    ("Chainrattle Spirit", "Dragging Fetters"),
    ("Candlewax Phantom", "Guttering Flame"),
    ("The Unburied Duelist", "Remembered Grudge"),
    ("Sexton's Regret", "Rusted Spade"),
    ("Weeping Reliquary", "Saint's Splinters"),
    ("The Toll-less Ferryman", "Empty Palm"),
    ("Mausoleum Draft", "Creeping Numbness"),
];

/// A graveyard shade's combat stats (attack, defense, hp) for a player of
/// `level` (`lib/graveyard/case_battle_search.php`). Every stat derives from
/// the *player's* level; the seed row is overridden entirely:
/// `shift = -1` under level 5, `attack = 9 + shift + (int)((level-1)*1.5)`,
/// `defense = attack * 0.7` ("make graveyard creatures easier"),
/// `hp = level*5 + 50`. Upstream keeps the defense as a PHP float; our
/// integer combatant rounds it.
pub fn graveyard_creature_stats(level: u8) -> (u32, u32, u32) {
    let level = level.clamp(1, MAX_LEVEL) as i32;
    let shift = if level < 5 { -1 } else { 0 };
    let base = 9 + shift + ((level - 1) * 3) / 2; // (int)((level-1) * 1.5)
    let attack = base as u32;
    let defense = (base as f64 * 0.7).round() as u32;
    let hp = level as u32 * 5 + 50;
    (attack, defense, hp)
}

/// The inclusive favor payout range a tormented shade offers on victory (its
/// "exp" slot upstream): `e_rand(10 + round(level/3), 20 + round(level/3))`.
pub fn graveyard_favor_range(level: u8) -> (u32, u32) {
    let bump = (level as f64 / 3.0).round() as u32;
    (10 + bump, 20 + bump)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_ladder_is_monotonic() {
        assert!(COST_LADDER.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(COST_LADDER.len(), MAX_LEVEL as usize);
    }

    #[test]
    fn creature_tier_clamps() {
        assert_eq!(creature_tier(0), CREATURES[0]);
        assert_eq!(creature_tier(1), CREATURES[0]);
        assert_eq!(creature_tier(16), CREATURES[15]);
        assert_eq!(creature_tier(99), CREATURES[15]);
    }

    #[test]
    fn exp_scales_with_dragon_kills() {
        assert_eq!(exp_to_advance(1, 0), 100);
        // base 100 + (4/4)*1*100 = 200
        assert_eq!(exp_to_advance(1, 4), 200);
        assert_eq!(exp_to_advance(15, 0), 43930);
    }

    #[test]
    fn master_stats_follow_seed() {
        assert_eq!(master_stats(1), (2, 2, 12));
        assert_eq!(master_stats(14), (28, 28, 154));
        assert_eq!(MASTERS.len(), 14);
    }

    #[test]
    fn every_creature_level_has_at_least_one_name() {
        assert!(CREATURE_NAMES.iter().all(|names| !names.is_empty()));
        assert_eq!(CREATURE_NAMES.len(), CREATURES.len());
    }

    #[test]
    fn graveyard_shades_scale_off_the_player_level() {
        // Level 1: shift -1, base 8; defense round(8*0.7) = 6; hp 55.
        assert_eq!(graveyard_creature_stats(1), (8, 6, 55));
        // Level 4: shift -1, base 9 - 1 + (int)(4.5) = 12; def round(8.4) = 8.
        assert_eq!(graveyard_creature_stats(4), (12, 8, 70));
        // Level 15: no shift, base 9 + 21 = 30; def round(21.0) = 21.
        assert_eq!(graveyard_creature_stats(15), (30, 21, 125));
        // Favor payout range: 10..20 plus round(level/3).
        assert_eq!(graveyard_favor_range(1), (10, 20));
        assert_eq!(graveyard_favor_range(5), (12, 22));
        assert!(!GRAVEYARD_CREATURES.is_empty());
    }

    #[test]
    fn title_ladder_picks_the_highest_earned_threshold() {
        use rand::{SeedableRng, rngs::StdRng};
        let mut rng = StdRng::seed_from_u64(1);
        // Fresh characters get the threshold-0 pair.
        assert_eq!(dk_title_pair(0, &mut rng), ("Mudfoot", "Mudlark"));
        // Exact thresholds and the open top end (past 31 the top rung holds).
        assert_eq!(dk_title_pair(6, &mut rng).0, "Fangtaker");
        assert_eq!(dk_title_pair(10, &mut rng), ("Dragonlord", "Dragonlady"));
        assert_eq!(dk_title_pair(99, &mut rng).0, "The Deathless");
        // One rung per kill 0..=31, upstream's 32-row seed.
        assert_eq!(TITLES.len(), 32);
        assert!(
            TITLES
                .iter()
                .enumerate()
                .all(|(i, (dk, _, _))| *dk == i as u32)
        );
    }

    #[test]
    fn gear_name_tables_cover_every_tier() {
        assert_eq!(WEAPON_NAMES.len(), MAX_LEVEL as usize);
        assert_eq!(ARMOR_NAMES.len(), MAX_LEVEL as usize);
        // Tier 0 is the unarmed/unarmored sentinel.
        assert_eq!(weapon_name(0), "Fists");
        assert_eq!(armor_name(0), "None");
        // Tiers map to their table entry and clamp past the cap.
        assert_eq!(weapon_name(1), WEAPON_NAMES[0]);
        assert_eq!(weapon_name(MAX_LEVEL), WEAPON_NAMES[MAX_LEVEL as usize - 1]);
        assert_eq!(weapon_name(99), WEAPON_NAMES[MAX_LEVEL as usize - 1]);
        assert_eq!(armor_name(99), ARMOR_NAMES[MAX_LEVEL as usize - 1]);
    }
}
