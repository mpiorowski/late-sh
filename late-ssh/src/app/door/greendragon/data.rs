//! Canonical Legend of the Green Dragon (LoGD) balance data, transcribed from
//! the DragonPrime-lineage default install seed (`jimlunsford/lotgd@master`).
//!
//! These are *data tables*, not gameplay code: the cost/power ladders, the
//! per-level creature stat blocks, the experience curve, and the named level
//! masters. Game mechanics are not copyrightable and these numeric tables are
//! the established LoGD balance; we transcribe them verbatim so the game feels
//! authentic instead of re-tuned. Flavor text (creature/master names) comes
//! from the same open-source seed.
//!
//! Source files (all `jimlunsford/lotgd@master`):
//! - weapons/armor/creatures/masters seed: `lib/installer/installer_sqlstatements.php`
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
    CreatureTier { hp: 10, attack: 1, defense: 1, gold: 36, exp: 14 },
    CreatureTier { hp: 21, attack: 3, defense: 3, gold: 97, exp: 24 },
    CreatureTier { hp: 32, attack: 5, defense: 4, gold: 148, exp: 34 },
    CreatureTier { hp: 43, attack: 7, defense: 6, gold: 162, exp: 45 },
    CreatureTier { hp: 53, attack: 9, defense: 7, gold: 198, exp: 55 },
    CreatureTier { hp: 64, attack: 11, defense: 8, gold: 234, exp: 66 },
    CreatureTier { hp: 74, attack: 13, defense: 10, gold: 268, exp: 77 },
    CreatureTier { hp: 84, attack: 15, defense: 11, gold: 302, exp: 89 },
    CreatureTier { hp: 94, attack: 17, defense: 13, gold: 336, exp: 101 },
    CreatureTier { hp: 105, attack: 19, defense: 14, gold: 369, exp: 114 },
    CreatureTier { hp: 115, attack: 21, defense: 15, gold: 402, exp: 127 },
    CreatureTier { hp: 125, attack: 23, defense: 17, gold: 435, exp: 141 },
    CreatureTier { hp: 135, attack: 25, defense: 18, gold: 467, exp: 156 },
    CreatureTier { hp: 145, attack: 27, defense: 20, gold: 499, exp: 172 },
    CreatureTier { hp: 155, attack: 29, defense: 21, gold: 531, exp: 189 },
    CreatureTier { hp: 166, attack: 31, defense: 22, gold: 563, exp: 207 },
];

/// Look up the creature stat block for a forest level, clamped to 1..=16.
pub fn creature_tier(level: u8) -> CreatureTier {
    let idx = (level.clamp(1, 16) - 1) as usize;
    CREATURES[idx]
}

/// Flavor (name, weapon) pairs per forest level 1..=16, indexed by `level - 1`.
/// Stats always come from [`CREATURES`]; this list only varies the prose. Pulled
/// from the LoGD seed examples; more canon names can be appended per level.
pub const CREATURE_NAMES: [&[(&str, &str)]; 16] = [
    &[("Thieving Kender", "Whirling Hoopak"), ("Baby Unicorn", "Blunt Horn")],
    &[("Pygmy Marmoset", "Pieces of Treebark")],
    &[("Amazon", "Bow and Arrow")],
    &[("Angry Mob", "Torches"), ("Polar Bear", "Terrible Claws")],
    &[("Mature Unicorn", "Powerful Horn")],
    &[("Magic Mushroom", "Swirling Colors")],
    &[("Moe", "Two Knives")],
    &[("Daughter of the Devil", "Sinfully Good Looks")],
    &[("Old Hag", "Red Red Rose")],
    &[("Garden Gnome", "Painful Tackiness")],
    &[("Bluebird of Happiness", "Uplifting Melody")],
    &[("Magic Mirror", "Flattering Remarks")],
    &[("Cerberus", "Three Drooling Maws"), ("Giant", "Smashing Club")],
    &[("Samurai Master", "Daisho")],
    &[("Evil Wizard", "Tormented Souls")],
    &[("Darkness", "Self-induced Terror")],
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
/// from level N to N+1; beating Yoresh (14) unlocks level 15 and the Dragon.
/// Master stats are derived (see [`master_stats`]): attack = defense = 2*level,
/// hp = 11*level (level 1 = 12 by the seed).
pub const MASTERS: [Master; 14] = [
    Master { name: "Mireraband", weapon: "Small Dagger" },
    Master { name: "Fie", weapon: "Short Sword" },
    Master { name: "Glynyc", weapon: "Hugely Spiked Mace" },
    Master { name: "Guth", weapon: "Spiked Club" },
    Master { name: "Unélith", weapon: "Thought Control" },
    Master { name: "Adwares", weapon: "Dwarven Battle Axe" },
    Master { name: "Gerrard", weapon: "Battle Bow" },
    Master { name: "Ceiloth", weapon: "Orkos Broadsword" },
    Master { name: "Dwiredan", weapon: "Twin Swords" },
    Master { name: "Sensei Noetha", weapon: "Martial Arts Skills" },
    Master { name: "Celith", weapon: "Throwing Halos" },
    Master { name: "Gadriel the Elven Ranger", weapon: "Elven Long Bow" },
    Master { name: "Adoawyr", weapon: "Gargantuan Broad Sword" },
    Master { name: "Yoresh", weapon: "Death Touch" },
];

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
}
