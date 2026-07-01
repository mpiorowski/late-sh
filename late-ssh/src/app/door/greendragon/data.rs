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
        gold: 198,
        exp: 55,
    },
    CreatureTier {
        hp: 64,
        attack: 11,
        defense: 8,
        gold: 234,
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
    ],
    &[("Bog Lurcher", "Slick Tendrils")],
    &[("Bandit Scout", "Worn Crossbow")],
    &[
        ("Snow Troll", "Frostbitten Fists"),
        ("Torch-lit Mob", "Pitchforks"),
    ],
    &[("Thornback Boar", "Goring Tusks")],
    &[("Spore Wraith", "Choking Cloud")],
    &[("Gravel Golem", "Crushing Slam")],
    &[("Veiled Temptress", "Beguiling Whisper")],
    &[("Marsh Crone", "Hexed Nettles")],
    &[("Clockwork Sentry", "Whirring Blades")],
    &[("Gloomfinch Flock", "Razor Feathers")],
    &[("Mirror Shade", "Stolen Face")],
    &[
        ("Three-Headed Hound", "Snapping Maws"),
        ("Hill Giant", "Uprooted Oak"),
    ],
    &[("Ronin of Ash", "Twin Embers")],
    &[("Hollow Archmage", "Unspoken Word")],
    &[("The Long Dark", "Creeping Dread")],
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

// --- Mercenary Camp ---------------------------------------------------------

/// Most companions you can lead into the forest at once (LoGD's `maxcompanions`
/// setting, which both `apply_companion` and this camp gate on). A server
/// setting upstream rather than a fixed balance number, so this is our chosen
/// band size; summoned skeletons and hired mercenaries share it.
pub const MAX_COMPANIONS: usize = 4;

/// Multiplier in the companion heal price (LoGD `mercenarycamp.php`:
/// `round(log(level+1) * (pointstoheal + 10) * 1.33)`).
pub const COMPANION_HEAL_FACTOR: f64 = 1.33;

/// A companion hireable at the Mercenary Camp (LoGD `mercenarycamp.php` reading
/// the `companions` table). The **mechanic** is transcribed 1=1: a gold/gem
/// price, a dragon-kill gate (`companioncostdks`), and stats that scale with
/// your level at hire (`attack = attack + attackperlevel*level`, likewise for
/// defense and max HP). The **roster, names, and flavor are original to
/// late.sh** — the stock `companions` table is a separately-seeded, non-core
/// list, like our creature/weapon names, so there is no canonical seed to copy.
#[derive(Clone, Copy, Debug)]
pub struct Mercenary {
    pub name: &'static str,
    /// One-line pitch shown in the camp.
    pub blurb: &'static str,
    pub cost_gold: u64,
    pub cost_gems: u64,
    /// Dragon kills required before this one will follow you.
    pub cost_dks: u32,
    pub base_attack: u32,
    pub attack_per_level: u32,
    pub base_defense: u32,
    pub defense_per_level: u32,
    pub base_hp: u32,
    pub hp_per_level: u32,
    /// Logged the round this companion falls.
    pub dying_text: &'static str,
}

/// The camp's roster, cheapest first. Later entries gate behind dragon kills and
/// gems, so the band you can field grows with your investment.
pub const MERCENARIES: [Mercenary; 5] = [
    Mercenary {
        name: "Stray Cur",
        blurb: "A half-wild dog that took a liking to you. More teeth than sense.",
        cost_gold: 250,
        cost_gems: 0,
        cost_dks: 0,
        base_attack: 1,
        attack_per_level: 1,
        base_defense: 0,
        defense_per_level: 1,
        base_hp: 5,
        hp_per_level: 4,
        dying_text: "The stray yelps once and bolts into the brush.",
    },
    Mercenary {
        name: "Copper Sellsword",
        blurb: "A road-worn blade for hire who works cheap and asks nothing.",
        cost_gold: 2_000,
        cost_gems: 0,
        cost_dks: 0,
        base_attack: 2,
        attack_per_level: 1,
        base_defense: 1,
        defense_per_level: 1,
        base_hp: 10,
        hp_per_level: 8,
        dying_text: "The sellsword crumples with a curse on his lips.",
    },
    Mercenary {
        name: "Hedge Warden",
        blurb: "A grim forester who has buried more men than she can name.",
        cost_gold: 8_000,
        cost_gems: 0,
        cost_dks: 1,
        base_attack: 3,
        attack_per_level: 2,
        base_defense: 2,
        defense_per_level: 1,
        base_hp: 15,
        hp_per_level: 10,
        dying_text: "The warden falls without a sound, as she lived.",
    },
    Mercenary {
        name: "Ironbound Reaver",
        blurb: "A plate-clad killer who charges gems and earns every one.",
        cost_gold: 20_000,
        cost_gems: 1,
        cost_dks: 3,
        base_attack: 5,
        attack_per_level: 2,
        base_defense: 4,
        defense_per_level: 2,
        base_hp: 25,
        hp_per_level: 12,
        dying_text: "The reaver's armor buckles and he goes down hard.",
    },
    Mercenary {
        name: "Ashen Revenant",
        blurb: "Something that used to be a champion, bound to your banner by gems and dread.",
        cost_gold: 0,
        cost_gems: 5,
        cost_dks: 7,
        base_attack: 8,
        attack_per_level: 3,
        base_defense: 6,
        defense_per_level: 2,
        base_hp: 40,
        hp_per_level: 15,
        dying_text: "The revenant collapses into a drift of cold ash.",
    },
];

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
