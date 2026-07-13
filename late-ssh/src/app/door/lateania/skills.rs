// Gathering skills for Lateania: the first pillar of the crafting economy.
//
// Five gathering trades - Woodcutting, Mining, Fishing, Foraging, Skinning -
// each levelled 1..=50 on its own XP curve that steepens every tier, so the
// late materials are a genuine grind. A player's skill xp lives on PlayerState
// (a map of skill -> total xp) and persists; the level is a pure function of xp.
//
// Resource NODES (trees, ore veins, fishing spots, herb/skinning patches) live
// in `world.rs` and each belongs to one skill; harvesting a node grants its raw
// MATERIAL item (see `items::material_id`) plus skill xp, gated behind a
// per-node minimum skill level.

use std::fmt;

/// A gathering trade. The order here is the order shown on the character sheet
/// and, via `index`, the layout of the raw-material item ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GatherSkill {
    Woodcutting,
    Mining,
    Fishing,
    Foraging,
    Skinning,
}

impl GatherSkill {
    pub const ALL: [GatherSkill; 5] = [
        GatherSkill::Woodcutting,
        GatherSkill::Mining,
        GatherSkill::Fishing,
        GatherSkill::Foraging,
        GatherSkill::Skinning,
    ];

    /// Stable index used to lay out raw-material item ids (see `items`). Never
    /// change once shipped, or persisted materials would point at the wrong item.
    pub const fn index(self) -> u32 {
        match self {
            Self::Woodcutting => 0,
            Self::Mining => 1,
            Self::Fishing => 2,
            Self::Foraging => 3,
            Self::Skinning => 4,
        }
    }

    /// Stable key for persistence (never change once shipped).
    pub fn key(self) -> &'static str {
        match self {
            Self::Woodcutting => "woodcutting",
            Self::Mining => "mining",
            Self::Fishing => "fishing",
            Self::Foraging => "foraging",
            Self::Skinning => "skinning",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    /// Display name for panels and log lines.
    pub fn label(self) -> &'static str {
        match self {
            Self::Woodcutting => "Woodcutting",
            Self::Mining => "Mining",
            Self::Fishing => "Fishing",
            Self::Foraging => "Foraging",
            Self::Skinning => "Skinning",
        }
    }

    /// The working verb for the harvest log line: "You chop ...", "You mine ...".
    pub fn verb(self) -> &'static str {
        match self {
            Self::Woodcutting => "chop",
            Self::Mining => "mine",
            Self::Fishing => "fish",
            Self::Foraging => "forage",
            Self::Skinning => "skin",
        }
    }
}

impl fmt::Display for GatherSkill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A crafting trade - the maker's side of the economy. Each turns gathered raw
/// materials (and refined intermediates) into usable, sellable goods, and levels
/// 1..=50 on the very same curve as the gathering skills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CraftSkill {
    Smithing,
    Woodworking,
    Leatherworking,
    Alchemy,
    Cooking,
}

impl CraftSkill {
    pub const ALL: [CraftSkill; 5] = [
        CraftSkill::Smithing,
        CraftSkill::Woodworking,
        CraftSkill::Leatherworking,
        CraftSkill::Alchemy,
        CraftSkill::Cooking,
    ];

    /// Stable index used to lay out crafted-item ids (see `items`). Never change.
    pub const fn index(self) -> u32 {
        match self {
            Self::Smithing => 0,
            Self::Woodworking => 1,
            Self::Leatherworking => 2,
            Self::Alchemy => 3,
            Self::Cooking => 4,
        }
    }

    /// Stable key for persistence (never change once shipped).
    pub fn key(self) -> &'static str {
        match self {
            Self::Smithing => "smithing",
            Self::Woodworking => "woodworking",
            Self::Leatherworking => "leatherworking",
            Self::Alchemy => "alchemy",
            Self::Cooking => "cooking",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Smithing => "Smithing",
            Self::Woodworking => "Woodworking",
            Self::Leatherworking => "Leatherworking",
            Self::Alchemy => "Alchemy",
            Self::Cooking => "Cooking",
        }
    }

    /// The making verb for the craft log line: "You forge ...", "You brew ...".
    pub fn verb(self) -> &'static str {
        match self {
            Self::Smithing => "forge",
            Self::Woodworking => "craft",
            Self::Leatherworking => "tan",
            Self::Alchemy => "brew",
            Self::Cooking => "cook",
        }
    }

    /// The station a crafter works at (feature name / panel wording).
    pub fn station(self) -> &'static str {
        match self {
            Self::Smithing => "forge",
            Self::Woodworking => "workbench",
            Self::Leatherworking => "tannery",
            Self::Alchemy => "alchemy lab",
            Self::Cooking => "cooking fire",
        }
    }
}

impl fmt::Display for CraftSkill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Skill level cap - the same 50 the class levels use, so "level 1 to 50" reads
/// consistently across the game.
pub const SKILL_MAX_LEVEL: i32 = 50;

/// Total xp required to *reach* a given skill level. Level 1 is free; each tier
/// costs more than the last, and a cubic term that only bites past level 10
/// makes the back half of every trade the real work (harder and harder).
pub fn xp_for_skill_level(level: i32) -> i64 {
    if level <= 1 {
        return 0;
    }
    let d = (level - 1) as i64;
    let base = 30 * d * d;
    let late = (level - 10).max(0) as i64;
    base + 10 * late * late * late
}

/// The skill level a given total xp corresponds to (1..=SKILL_MAX_LEVEL).
pub fn skill_level_for_xp(xp: i64) -> i32 {
    let mut level = 1;
    while level < SKILL_MAX_LEVEL && xp >= xp_for_skill_level(level + 1) {
        level += 1;
    }
    level
}

/// Progress within the current level: (xp into this level, xp needed for the
/// next). At the cap the second value is 0 ("maxed").
pub fn skill_progress(xp: i64) -> (i64, i64) {
    let level = skill_level_for_xp(xp);
    if level >= SKILL_MAX_LEVEL {
        return (0, 0);
    }
    let floor = xp_for_skill_level(level);
    let next = xp_for_skill_level(level + 1);
    (xp - floor, next - floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in GatherSkill::ALL {
            assert!(seen.insert(s.key()), "duplicate skill key {}", s.key());
            assert_eq!(GatherSkill::from_key(s.key()), Some(s));
        }
        assert_eq!(GatherSkill::from_key("nonsense"), None);
    }

    #[test]
    fn indices_are_unique_and_dense() {
        let mut idx: Vec<u32> = GatherSkill::ALL.iter().map(|s| s.index()).collect();
        idx.sort_unstable();
        assert_eq!(idx, vec![0, 1, 2, 3, 4]);
        let mut cidx: Vec<u32> = CraftSkill::ALL.iter().map(|s| s.index()).collect();
        cidx.sort_unstable();
        assert_eq!(cidx, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn craft_keys_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in CraftSkill::ALL {
            assert!(seen.insert(s.key()), "duplicate craft key {}", s.key());
            assert_eq!(CraftSkill::from_key(s.key()), Some(s));
        }
        assert_eq!(CraftSkill::from_key("nonsense"), None);
    }

    #[test]
    fn level_one_is_free_and_curve_is_strictly_increasing() {
        assert_eq!(xp_for_skill_level(1), 0);
        assert_eq!(xp_for_skill_level(0), 0);
        for level in 2..=SKILL_MAX_LEVEL {
            assert!(
                xp_for_skill_level(level) > xp_for_skill_level(level - 1),
                "curve must rise at level {level}"
            );
        }
    }

    #[test]
    fn curve_steepens_late() {
        // The cost of a mid-game level must exceed the cost of an early one, and
        // a late level must cost far more still (the "harder and harder" shape).
        let early = xp_for_skill_level(5) - xp_for_skill_level(4);
        let mid = xp_for_skill_level(20) - xp_for_skill_level(19);
        let late = xp_for_skill_level(50) - xp_for_skill_level(49);
        assert!(mid > early);
        assert!(late > mid * 3);
    }

    #[test]
    fn level_for_xp_inverts_the_curve_and_caps() {
        for level in 1..=SKILL_MAX_LEVEL {
            assert_eq!(skill_level_for_xp(xp_for_skill_level(level)), level);
        }
        // One short of a threshold stays on the lower level.
        assert_eq!(
            skill_level_for_xp(xp_for_skill_level(10) - 1),
            9,
            "just under the level-10 threshold is still level 9"
        );
        // Absurd xp still caps at the max level.
        assert_eq!(skill_level_for_xp(i64::MAX / 2), SKILL_MAX_LEVEL);
    }

    #[test]
    fn progress_stays_within_the_level_band() {
        let xp = xp_for_skill_level(7) + 5;
        let (into, need) = skill_progress(xp);
        assert_eq!(into, 5);
        assert_eq!(need, xp_for_skill_level(8) - xp_for_skill_level(7));
        // At the cap there is no "next".
        assert_eq!(skill_progress(xp_for_skill_level(SKILL_MAX_LEVEL)), (0, 0));
    }
}
