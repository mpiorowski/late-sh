// Character classes for Lateania.
//
// Seven classes, each with a distinct resource, a passive class trait, a rich
// description, and a 50-level progression. Progression is formula-driven (data,
// not a hand-typed table) so balance lives in one place. Abilities unlock by
// level in abilities.rs.

/// The playable classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Warrior,
    Mage,
    Cleric,
    Rogue,
    Ranger,
    Druid,
    Necromancer,
}

/// The resource a class spends on abilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Rage,
    Mana,
    Energy,
    Focus,
    Spirit,
    Souls,
}

impl Resource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rage => "Rage",
            Self::Mana => "Mana",
            Self::Energy => "Energy",
            Self::Focus => "Focus",
            Self::Spirit => "Spirit",
            Self::Souls => "Souls",
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
    pub const ALL: [Class; 7] = [
        Class::Warrior,
        Class::Mage,
        Class::Cleric,
        Class::Rogue,
        Class::Ranger,
        Class::Druid,
        Class::Necromancer,
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
            Self::Druid => "Druid",
            Self::Necromancer => "Necromancer",
        }
    }

    /// The ability score that sharpens this class's strikes (its attack key).
    pub fn primary_score(self) -> super::stats::Score {
        use super::stats::Score;
        match self {
            Self::Warrior => Score::Strength,
            Self::Mage => Score::Intelligence,
            Self::Cleric => Score::Wisdom,
            Self::Rogue => Score::Dexterity,
            Self::Ranger => Score::Dexterity,
            Self::Druid => Score::Wisdom,
            Self::Necromancer => Score::Intelligence,
        }
    }

    pub fn resource(self) -> Resource {
        match self {
            Self::Warrior => Resource::Rage,
            Self::Mage => Resource::Mana,
            Self::Cleric => Resource::Mana,
            Self::Rogue => Resource::Energy,
            Self::Ranger => Resource::Focus,
            Self::Druid => Resource::Spirit,
            Self::Necromancer => Resource::Souls,
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
            Self::Druid => "Wild shapeshifter - nature's mercy and its teeth alike.",
            Self::Necromancer => "Master of death - drains the living, harvests the slain.",
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
            Self::Druid => {
                "The Druid keeps the old covenant with the wild, and the wild keeps it \
                back. They speak to root and storm and the slow green patience of growing \
                things, calling thorns from bare stone and rain from a clear sky, then \
                mending what the fight has torn as easily as breathing. Spirit is their \
                tether to the living world; while it holds, so do they. A Druid does not \
                so much win a battle as outlast the season of it - bending, never breaking, \
                until the land itself decides the matter."
            }
            Self::Necromancer => {
                "The Necromancer studies the one door everyone passes through, and has \
                learned to make it swing both ways. Where others see a corpse, they see \
                fuel; where others mourn, they harvest. Shadow answers their call, draining \
                the warmth from the living to feed their own cold endurance, and every foe \
                that falls before them yields up its Souls to be spent again. They are not \
                hated for cruelty so much as for candor - they simply refuse to pretend \
                that death is the end of anything useful."
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
            Self::Druid => "Nature's Renewal",
            Self::Necromancer => "Soul Harvest",
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
            Self::Druid => "The living world mends you: you regenerate health every few moments.",
            Self::Necromancer => {
                "Each foe you slay yields its life force, restoring health and Souls."
            }
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
            // Hybrid bruiser-healer: hardy and steady, like the Cleric but greener.
            Self::Druid => ClassStats {
                max_hp: 40 + l * 9,
                max_resource: 70 + l * 3,
                attack: 5 + (l * 3) / 2,
                resource_regen: 7,
            },
            // A caster a touch hardier than the Mage - undeath lends some grit.
            Self::Necromancer => ClassStats {
                max_hp: 32 + l * 8,
                max_resource: 60 + l * 4,
                attack: 5 + l * 2,
                resource_regen: 6,
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
            Self::Druid => "druid",
            Self::Necromancer => "necromancer",
        }
    }

    pub fn from_key(key: &str) -> Option<Class> {
        match key {
            "warrior" => Some(Self::Warrior),
            "mage" => Some(Self::Mage),
            "cleric" => Some(Self::Cleric),
            "rogue" => Some(Self::Rogue),
            "ranger" => Some(Self::Ranger),
            "druid" => Some(Self::Druid),
            "necromancer" => Some(Self::Necromancer),
            _ => None,
        }
    }
}

/// Total experience required to reach a given level. Smoothly rising curve so
/// early levels arrive quickly, then the climb past the first story bosses
/// stretches into a longer campaign.
pub fn xp_for_level(level: i32) -> i64 {
    if level <= 1 {
        return 0;
    }
    let l = level as i64;
    let d = l - 1;
    let base = 25 * d * d + (15 * d * d * d) / 10;
    if level <= 8 {
        base
    } else {
        let late = d - 7;
        base + 220 * late * late + 8 * late * late * late
    }
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
    fn xp_curve_slows_after_early_story_levels() {
        assert_eq!(xp_for_level(8), 25 * 7 * 7 + (15 * 7 * 7 * 7) / 10);
        assert!(xp_for_level(15) > 22_000);
        assert!(xp_for_level(30) > 240_000);
        assert!(xp_for_level(50) > 1_200_000);
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

    #[test]
    fn all_classes_round_trip_their_persistence_key_and_are_distinct() {
        assert_eq!(Class::ALL.len(), 7, "seven classes now");
        let mut keys = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for class in Class::ALL {
            // Stable persistence key survives a round trip.
            assert_eq!(Class::from_key(class.as_key()), Some(class));
            assert!(keys.insert(class.as_key()), "duplicate class key");
            assert!(names.insert(class.name()), "duplicate class name");
            // Every class has a non-empty tagline/description and a usable resource.
            assert!(!class.tagline().is_empty());
            assert!(!class.trait_name().is_empty());
            assert!(class.stats_at(1).max_resource > 0, "{:?}", class);
        }
        // The two newcomers landed with their intended identities.
        assert_eq!(Class::Druid.resource(), Resource::Spirit);
        assert_eq!(Class::Necromancer.resource(), Resource::Souls);
        assert_eq!(Class::from_key("druid"), Some(Class::Druid));
        assert_eq!(Class::from_key("necromancer"), Some(Class::Necromancer));
    }
}
