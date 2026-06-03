// Character classes for Lateania.
//
// Five classes, each with a distinct resource, a passive class trait, a rich
// description, and a 50-level progression. Progression is formula-driven (data,
// not a hand-typed table) so balance lives in one place. Abilities unlock by
// level in abilities.rs.

/// The five playable classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Warrior,
    Mage,
    Cleric,
    Rogue,
    Ranger,
}

/// The resource a class spends on abilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Rage,
    Mana,
    Energy,
    Focus,
}

impl Resource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rage => "Rage",
            Self::Mana => "Mana",
            Self::Energy => "Energy",
            Self::Focus => "Focus",
        }
    }
}

/// Per-level stat shape for one class, computed from the level.
#[derive(Clone, Copy, Debug)]
pub struct ClassStats {
    pub max_hp: i32,
    pub max_resource: i32,
    pub attack: i32,
    /// Resource regained per world tick.
    pub resource_regen: i32,
}

impl Class {
    pub const ALL: [Class; 5] = [
        Class::Warrior,
        Class::Mage,
        Class::Cleric,
        Class::Rogue,
        Class::Ranger,
    ];

    /// The hard level ceiling. Reaching it is the long game.
    pub const MAX_LEVEL: i32 = 50;

    pub fn name(self) -> &'static str {
        match self {
            Self::Warrior => "Warrior",
            Self::Mage => "Mage",
            Self::Cleric => "Cleric",
            Self::Rogue => "Rogue",
            Self::Ranger => "Ranger",
        }
    }

    pub fn resource(self) -> Resource {
        match self {
            Self::Warrior => Resource::Rage,
            Self::Mage => Resource::Mana,
            Self::Cleric => Resource::Mana,
            Self::Rogue => Resource::Energy,
            Self::Ranger => Resource::Focus,
        }
    }

    /// A one-line role summary for the character sheet.
    pub fn tagline(self) -> &'static str {
        match self {
            Self::Warrior => "Frontline bulwark - trades blows and outlasts.",
            Self::Mage => "Glass-cannon spellcaster - immense burst, fragile frame.",
            Self::Cleric => "Holy battle-healer - sustains, smites the undead.",
            Self::Rogue => "Lethal duelist - stealth, poison, and sudden death.",
            Self::Ranger => "Patient hunter - ranged pressure and field-craft.",
        }
    }

    /// The flavorful long description shown when choosing or inspecting a class.
    pub fn description(self) -> &'static str {
        match self {
            Self::Warrior => {
                "Where the line breaks, the Warrior stands. Clad in iron and \
                certainty, they read a battle in the rhythm of falling blows and answer it \
                with their own. Rage is their fuel: it does not pool while they rest but \
                kindles in the fight itself, every wound taken and given stoking it higher \
                until they end the matter with a single, ruinous stroke. Warriors do not \
                dazzle. They endure, and what they endure, they outlive."
            }
            Self::Mage => {
                "The Mage holds the oldest and most dangerous bargain: power \
                without armor, knowledge without mercy. They unmake the world in syllables, \
                calling fire that clings, frost that locks the joints, and lightning that \
                forgets nothing it touches. Mana is their well, deep but not bottomless, and \
                a Mage caught between spells is a candle in a gale. Strike first, strike \
                hardest, and never let the enemy close the distance."
            }
            Self::Cleric => {
                "The Cleric carries the Dawn into dark places. Theirs is the \
                hardest road: to mend and to smite with the same hand, to stand in the ruin \
                and refuse to let a companion fall. Holy fire answers the wicked and \
                searing light judges the undead, while a whispered prayer knits torn flesh \
                whole. A party with a Cleric is a party that comes home; a Cleric alone is \
                a quiet, patient kind of unkillable."
            }
            Self::Rogue => {
                "The Rogue settles fights before they are fairly begun. They \
                trade plate for shadow and brawn for precision, finding the gap in the \
                guard, the vein that will not close, the breath of inattention that ends a \
                life. Energy floods back swiftly, rewarding the quick and the cruel with \
                flurry after flurry. A Rogue who is seen has already made a mistake; a Rogue \
                who is not will open you from hip to throat and be gone."
            }
            Self::Ranger => {
                "The Ranger belongs to the long marches and the patient kill. \
                Bow in hand and the wilds at their back, they wear the enemy down from a \
                distance no blade can answer, layering venom and volley and the cold \
                wisdom of a hundred camps. Focus is their discipline, spent on shots that \
                never waste and traps that never miss. Give a Ranger room and time, and the \
                fight is already lost - the quarry simply has not been told yet."
            }
        }
    }

    /// The passive class trait: a defining, always-on edge.
    pub fn trait_name(self) -> &'static str {
        match self {
            Self::Warrior => "Unbreakable",
            Self::Mage => "Arcane Mastery",
            Self::Cleric => "Light of the Dawn",
            Self::Rogue => "Opportunist",
            Self::Ranger => "Hunter's Instinct",
        }
    }

    pub fn trait_desc(self) -> &'static str {
        match self {
            Self::Warrior => {
                "The first killing blow each fight is survived at 1 HP instead of falling."
            }
            Self::Mage => "Every offensive spell strikes for extra arcane damage.",
            Self::Cleric => "All healing is amplified, and the undead take added holy damage.",
            Self::Rogue => "The opening strike of a fight always lands as a critical hit.",
            Self::Ranger => "Strikes against a wounded foe (below half health) hit harder.",
        }
    }

    /// Full stat block at a given level. Linear-plus-curve growth keeps all five
    /// classes climbing meaningfully to level 50.
    pub fn stats_at(self, level: i32) -> ClassStats {
        let lvl = level.clamp(1, Self::MAX_LEVEL);
        let l = lvl - 1; // levels gained past 1
        match self {
            Self::Warrior => ClassStats {
                max_hp: 48 + l * 12,
                max_resource: 100,
                attack: 6 + l * 2,
                resource_regen: 6,
            },
            Self::Mage => ClassStats {
                max_hp: 30 + l * 7,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 7,
            },
            Self::Cleric => ClassStats {
                max_hp: 38 + l * 9,
                max_resource: 55 + l * 4,
                attack: 5 + (l * 3) / 2,
                resource_regen: 6,
            },
            Self::Rogue => ClassStats {
                max_hp: 34 + l * 8,
                max_resource: 100,
                attack: 6 + l * 2,
                resource_regen: 12,
            },
            Self::Ranger => ClassStats {
                max_hp: 36 + l * 8,
                max_resource: 80 + l * 2,
                attack: 6 + l * 2,
                resource_regen: 9,
            },
        }
    }

    pub fn from_index(i: usize) -> Class {
        Self::ALL[i % Self::ALL.len()]
    }

    /// Stable lowercase key for persistence (never reorder these strings).
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Warrior => "warrior",
            Self::Mage => "mage",
            Self::Cleric => "cleric",
            Self::Rogue => "rogue",
            Self::Ranger => "ranger",
        }
    }

    pub fn from_key(key: &str) -> Option<Class> {
        match key {
            "warrior" => Some(Self::Warrior),
            "mage" => Some(Self::Mage),
            "cleric" => Some(Self::Cleric),
            "rogue" => Some(Self::Rogue),
            "ranger" => Some(Self::Ranger),
            _ => None,
        }
    }
}

/// Total experience required to reach a given level. Smoothly rising curve so
/// the climb to 50 is a real journey: ~50 xp for level 2, tens of thousands by 50.
pub fn xp_for_level(level: i32) -> i64 {
    if level <= 1 {
        return 0;
    }
    let l = level as i64;
    // Cubic-ish curve: 25*(l-1)^2 + 15*(l-1)^3/10, tuned for a long grind.
    let d = l - 1;
    25 * d * d + (15 * d * d * d) / 10
}

/// The level a given total xp corresponds to (1..=MAX_LEVEL).
pub fn level_for_xp(xp: i64) -> i32 {
    let mut level = 1;
    while level < Class::MAX_LEVEL && xp >= xp_for_level(level + 1) {
        level += 1;
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifty_levels_are_reachable_and_capped() {
        // Enough xp for any conceivable grind still caps at MAX_LEVEL.
        assert_eq!(level_for_xp(i64::MAX / 2), Class::MAX_LEVEL);
        assert_eq!(level_for_xp(0), 1);
    }

    #[test]
    fn xp_curve_is_strictly_increasing() {
        for l in 2..=Class::MAX_LEVEL {
            assert!(
                xp_for_level(l) > xp_for_level(l - 1),
                "xp curve must rise at level {l}"
            );
        }
    }

    #[test]
    fn level_and_xp_round_trip() {
        for l in 1..=Class::MAX_LEVEL {
            let xp = xp_for_level(l);
            assert_eq!(level_for_xp(xp), l, "xp boundary for level {l}");
        }
    }

    #[test]
    fn every_class_grows_hp_to_fifty() {
        for class in Class::ALL {
            let lo = class.stats_at(1).max_hp;
            let hi = class.stats_at(50).max_hp;
            assert!(hi > lo * 3, "{:?} should grow substantially by 50", class);
        }
    }
}
