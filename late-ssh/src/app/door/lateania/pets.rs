// Combat companions for Lateania.
//
// A pet is a creature an adventurer buys from a capital Stable. It travels with
// its owner (it lives on `PlayerState`, so it is always in the same room),
// fights the owner's target each combat round, can be downed when its owner is
// struck, and grows stronger as it is fed (loyalty). Only the species table and
// the growth maths live here; the world wiring (buying, feeding, combat) is in
// `svc.rs`.

/// A companion species: the fixed template a live `Pet` grows from. A species is
/// either **bought** at a Stable (`tame_level == 0`, a positive `price`) or
/// **tamed** in the wild (`tame_level > 0`, the required Animal Taming level; the
/// `price` is unused). Both kinds share the same runtime `Pet`, so a tamed beast
/// fights, is fed, and persists exactly like a bought one.
#[derive(Clone, Copy, Debug)]
pub struct PetSpecies {
    /// Stable persistence key (never reorder/rename).
    pub key: &'static str,
    pub name: &'static str,
    /// A short glyph shown beside the pet in panels.
    pub glyph: &'static str,
    /// Purchase price in gold (for buyable Stable species; 0 for tameables).
    pub price: i64,
    /// Level-1 health and per-round attack, before loyalty growth.
    pub base_hp: i32,
    pub base_attack: i32,
    pub desc: &'static str,
    /// Animal Taming level required to tame this beast in the wild. `0` marks a
    /// buyable Stable companion (not tameable). Rises across the fifty wild
    /// beasts so taming gets harder and harder.
    pub tame_level: i32,
}

/// The companions sold across the capital Stables. Ordered cheapest first.
pub const PET_SPECIES: &[PetSpecies] = &[
    PetSpecies {
        key: "war_hound",
        name: "War Hound",
        glyph: "\u{1F415}",
        price: 120,
        base_hp: 40,
        base_attack: 6,
        desc: "A loyal hound bred for the shield-wall - eager, brave, and quick to the throat of your foe.",
        tame_level: 0,
    },
    PetSpecies {
        key: "dire_wolf",
        name: "Dire Wolf",
        glyph: "\u{1F43A}",
        price: 320,
        base_hp: 64,
        base_attack: 10,
        desc: "A grey hunter of the deep wood, all sinew and patience, that brings down quarry far above its weight.",
        tame_level: 0,
    },
    PetSpecies {
        key: "moor_hawk",
        name: "Moor Hawk",
        glyph: "\u{1F985}",
        price: 280,
        base_hp: 30,
        base_attack: 14,
        desc: "A swift raptor that stoops from above in a blur of talons - fragile, but its strikes bite deep.",
        tame_level: 0,
    },
    PetSpecies {
        key: "cave_bear",
        name: "Cave Bear",
        glyph: "\u{1F43B}",
        price: 640,
        base_hp: 120,
        base_attack: 12,
        desc: "A mountain of fur and muscle from the frostline caverns; slow to rouse, ruinous once it does.",
        tame_level: 0,
    },
    PetSpecies {
        key: "emberdrake",
        name: "Emberdrake",
        glyph: "\u{1F432}",
        price: 1200,
        base_hp: 90,
        base_attack: 20,
        desc: "A hatchling wyrm with coals for eyes - rare, prized, and worth every coin to those who can afford it.",
        tame_level: 0,
    },
];

impl PetSpecies {
    /// True for a wild beast tamed via the Animal Taming trade (not a Stable buy).
    pub fn is_tameable(&self) -> bool {
        self.tame_level > 0
    }
}

/// Look up a species by its stable persistence key, across both the buyable
/// Stable companions and the fifty tameable wild beasts of Broceliande. Used by
/// persistence, so a saved pet of either kind reloads correctly.
pub fn pet_species_by_key(key: &str) -> Option<&'static PetSpecies> {
    PET_SPECIES
        .iter()
        .chain(super::taming::TAMEABLE.iter())
        .find(|s| s.key == key)
}

/// Loyalty earned per feeding, and the loyalty needed for each level beyond the
/// first. A pet caps at `PET_MAX_LEVEL`.
pub const FEED_LOYALTY: i64 = 25;
pub const LOYALTY_PER_LEVEL: i64 = 100;
pub const PET_MAX_LEVEL: i32 = 10;

/// A live companion owned by a player. Loyalty (and thus level) persists; the
/// current `hp`/`downed` are runtime-only and reset to full on reload.
#[derive(Clone, Copy, Debug)]
pub struct Pet {
    pub species: &'static PetSpecies,
    /// Total loyalty earned by feeding; drives the level via a pure function.
    pub loyalty_xp: i64,
    pub hp: i32,
    /// True once the pet is beaten down; it cannot fight until fed/revived.
    pub downed: bool,
}

impl Pet {
    /// A freshly bought (or reloaded) companion at full health.
    pub fn new(species: &'static PetSpecies, loyalty_xp: i64) -> Self {
        let mut pet = Self {
            species,
            loyalty_xp: loyalty_xp.max(0),
            hp: 0,
            downed: false,
        };
        pet.hp = pet.max_hp();
        pet
    }

    /// Level grows one step per `LOYALTY_PER_LEVEL` of loyalty, capped.
    pub fn level(&self) -> i32 {
        (1 + (self.loyalty_xp / LOYALTY_PER_LEVEL) as i32).clamp(1, PET_MAX_LEVEL)
    }

    /// Max health: the base pool plus a quarter of it per level gained.
    pub fn max_hp(&self) -> i32 {
        let base = self.species.base_hp;
        (base + base * (self.level() - 1) / 4).max(1)
    }

    /// Per-round attack: the base bite plus a quarter of it per level gained.
    pub fn attack(&self) -> i32 {
        let base = self.species.base_attack;
        (base + base * (self.level() - 1) / 4).max(1)
    }

    /// Loyalty progress toward the next level, as a 0-100 percentage (100 at cap).
    pub fn loyalty_pct(&self) -> i32 {
        if self.level() >= PET_MAX_LEVEL {
            return 100;
        }
        ((self.loyalty_xp % LOYALTY_PER_LEVEL) * 100 / LOYALTY_PER_LEVEL) as i32
    }

    /// Feed the pet: revive it, heal to full, and add loyalty. Returns true if a
    /// feeding actually leveled the pet up.
    pub fn feed(&mut self) -> bool {
        let before = self.level();
        self.loyalty_xp += FEED_LOYALTY;
        self.downed = false;
        self.hp = self.max_hp();
        self.level() > before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_keys_are_unique_and_round_trip() {
        for s in PET_SPECIES {
            assert_eq!(pet_species_by_key(s.key).map(|x| x.key), Some(s.key));
        }
        let mut keys: Vec<&str> = PET_SPECIES.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), PET_SPECIES.len(), "species keys are unique");
    }

    #[test]
    fn feeding_grows_loyalty_health_and_attack() {
        let species = pet_species_by_key("war_hound").unwrap();
        let mut pet = Pet::new(species, 0);
        assert_eq!(pet.level(), 1);
        let hp1 = pet.max_hp();
        let atk1 = pet.attack();
        // Four feedings = LOYALTY_PER_LEVEL of loyalty = one level.
        for _ in 0..(LOYALTY_PER_LEVEL / FEED_LOYALTY) {
            pet.feed();
        }
        assert_eq!(pet.level(), 2, "a full bar of loyalty levels the pet");
        assert!(pet.max_hp() > hp1, "leveling raises max HP");
        assert!(pet.attack() > atk1, "leveling raises attack");
        assert_eq!(pet.hp, pet.max_hp(), "feeding heals to full");
    }

    #[test]
    fn level_and_health_are_capped() {
        let species = pet_species_by_key("emberdrake").unwrap();
        let pet = Pet::new(species, LOYALTY_PER_LEVEL * 1000);
        assert_eq!(pet.level(), PET_MAX_LEVEL);
        assert_eq!(pet.loyalty_pct(), 100);
    }
}
