// Combat companions for Lateania.
//
// A pet is a creature an adventurer buys from a capital Stable. It travels with
// its owner (it lives on `PlayerState`, so it is always in the same room),
// fights the owner's target each combat round, can be downed when its owner is
// struck, and grows stronger as it is fed (loyalty). Only the species table and
// the growth maths live here; the world wiring (buying, feeding, combat) is in
// `svc.rs`.

/// Where a species comes from. Both kinds share the same runtime `Pet`, so a
/// tamed beast fights, is fed, and persists exactly like a bought one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetSource {
    /// Sold at every capital Stable. `rung` is the Animal Taming level whose
    /// wild beasts it is worth, placing it on the one companion ladder the
    /// tameables climb (see `PetSpecies::rung`).
    Stable { price: i64, rung: i32 },
    /// Tamed in the wild, needing this Animal Taming level.
    Wild { tame_level: i32 },
}

/// A companion species: the fixed template a live `Pet` grows from.
#[derive(Clone, Copy, Debug)]
pub struct PetSpecies {
    /// Stable persistence key (never reorder/rename).
    pub key: &'static str,
    pub name: &'static str,
    /// A short glyph shown beside the pet in panels.
    pub glyph: &'static str,
    /// Level-1 health and per-round attack, before loyalty growth.
    pub base_hp: i32,
    pub base_attack: i32,
    pub desc: &'static str,
    pub source: PetSource,
    /// This species' own auto-skill unlock ladder. Every pre-Aelunor species
    /// points at the shared `taming::PET_SKILLS` ladder (unchanged behaviour);
    /// the five Aelunor companions each carry their own distinct ladder, so
    /// "different pets, different spells" is real per-species data, not just
    /// re-skinned flavour text on the same five abilities.
    pub skills: &'static [super::taming::PetSkill],
}

/// The companions sold across the capital Stables, cheapest and weakest first.
/// Each one's `rung` is the Animal Taming level whose wild beasts it matches,
/// so gold buys a real mid-game pet while the top of the ladder stays wild.
pub const PET_SPECIES: &[PetSpecies] = &[
    PetSpecies {
        key: "war_hound",
        name: "War Hound",
        glyph: "\u{1F415}",
        base_hp: 40,
        base_attack: 6,
        desc: "A loyal hound bred for the shield-wall - eager, brave, and quick to the throat of your foe.",
        source: PetSource::Stable {
            price: 120,
            rung: 4,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "dire_wolf",
        name: "Dire Wolf",
        glyph: "\u{1F43A}",
        base_hp: 64,
        base_attack: 10,
        desc: "A grey hunter of the deep wood, all sinew and patience, that brings down quarry far above its weight.",
        source: PetSource::Stable {
            price: 320,
            rung: 9,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "moor_hawk",
        name: "Moor Hawk",
        glyph: "\u{1F985}",
        base_hp: 52,
        base_attack: 14,
        desc: "A swift raptor that stoops from above in a blur of talons - light in the bone, but its strikes bite deep.",
        source: PetSource::Stable {
            price: 650,
            rung: 12,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "cave_bear",
        name: "Cave Bear",
        glyph: "\u{1F43B}",
        base_hp: 120,
        base_attack: 13,
        desc: "A mountain of fur and muscle from the frostline caverns; slow to rouse, ruinous once it does.",
        source: PetSource::Stable {
            price: 1100,
            rung: 16,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "sabre_cat",
        name: "Sabre Cat",
        glyph: "\u{1F405}",
        base_hp: 104,
        base_attack: 18,
        desc: "A striped sabre-toothed cat from the southern scrub, trained to the leash and never quite to the hand.",
        source: PetSource::Stable {
            price: 2000,
            rung: 20,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "emberdrake",
        name: "Emberdrake",
        glyph: "\u{1F432}",
        base_hp: 116,
        base_attack: 20,
        desc: "A hatchling wyrm with coals for eyes - rare, prized, and worth every coin to those who can afford it.",
        source: PetSource::Stable {
            price: 3200,
            rung: 26,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "iron_rhino",
        name: "Ironhide Rhino",
        glyph: "\u{1F98F}",
        base_hp: 170,
        base_attack: 17,
        desc: "An armoured desert rhino shod in iron plates; it takes the blows meant for you and answers with the horn.",
        source: PetSource::Stable {
            price: 5000,
            rung: 30,
        },
        skills: super::taming::PET_SKILLS,
    },
    PetSpecies {
        key: "basilisk",
        name: "Stormhide Basilisk",
        glyph: "\u{1F98E}",
        base_hp: 160,
        base_attack: 23,
        desc: "A storm-scaled basilisk hatched in the Matlatesh beast-pits; the finest beast any Stable sells, and priced like it.",
        source: PetSource::Stable {
            price: 8000,
            rung: 35,
        },
        skills: super::taming::PET_SKILLS,
    },
];

impl PetSpecies {
    /// True for a wild beast tamed via the Animal Taming trade (not a Stable buy).
    pub fn is_tameable(&self) -> bool {
        match self.source {
            PetSource::Stable { .. } => false,
            PetSource::Wild { .. } => true,
        }
    }

    /// Where this species sits on the one companion ladder: the Animal Taming
    /// level a wild beast needs, or the level a Stable pet is priced to match.
    pub fn rung(&self) -> i32 {
        match self.source {
            PetSource::Stable { rung, .. } => rung,
            PetSource::Wild { tame_level } => tame_level,
        }
    }

    /// Gold to buy this species at a Stable. None for a wild beast, which is
    /// tamed and never sold.
    pub fn price(&self) -> Option<i64> {
        match self.source {
            PetSource::Stable { price, .. } => Some(price),
            PetSource::Wild { .. } => None,
        }
    }

    /// The Animal Taming level a wild beast needs. Only ever asked of a beast
    /// roaming the wild (`taming::beast_species`), never of a Stable pet.
    pub fn tame_level(&self) -> i32 {
        match self.source {
            PetSource::Wild { tame_level } => tame_level,
            PetSource::Stable { .. } => panic!("{} is sold at a Stable, never tamed", self.key),
        }
    }
}

/// Look up a species by its stable persistence key, across both the buyable
/// Stable companions and the fifty tameable wild beasts of Broceliande. Used by
/// persistence, so a saved pet of either kind reloads correctly.
pub fn pet_species_by_key(key: &str) -> Option<&'static PetSpecies> {
    PET_SPECIES
        .iter()
        .chain(super::taming::TAMEABLE.iter())
        .chain(super::taming::AELUNOR_TAMEABLE.iter())
        .find(|s| s.key == key)
}

/// Loyalty earned per feeding, and the loyalty needed for each level beyond the
/// first. A pet caps at `PET_MAX_LEVEL`.
pub const FEED_LOYALTY: i64 = 25;
pub const LOYALTY_PER_LEVEL: i64 = 100;
pub const PET_MAX_LEVEL: i32 = 10;
/// Meals a real (UTC) day that raise loyalty. Past them a feed still mends the
/// pet but it grows no fonder, so raising a companion to the cap takes nine
/// days of care rather than one purse emptied at the Stable.
pub const MEALS_PER_DAY: u32 = 4;

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

    /// The companion's own bite: the species base plus an eighth of it (at
    /// least 1) per loyalty level gained. What actually lands is this plus a share of the
    /// owner's attack rating (`svc::PET_COEF_PCT`), so a pet scales with the
    /// build it fights beside instead of being a fixed lump that dwarfs a
    /// level-30 character and fades by 100. Loyalty used to add a quarter per
    /// level (3.25x at cap); that flat growth was the lump.
    pub fn attack(&self) -> i32 {
        let base = self.species.base_attack;
        let step = (base / 8).max(1);
        (base + step * (self.level() - 1)).max(1)
    }

    /// Loyalty progress toward the next level, as a 0-100 percentage (100 at cap).
    pub fn loyalty_pct(&self) -> i32 {
        if self.level() >= PET_MAX_LEVEL {
            return 100;
        }
        ((self.loyalty_xp % LOYALTY_PER_LEVEL) * 100 / LOYALTY_PER_LEVEL) as i32
    }

    /// Tend the pet without a meal: revive it and heal it to full.
    pub fn mend(&mut self) {
        self.downed = false;
        self.hp = self.max_hp();
    }

    /// Feed the pet: add loyalty, then mend it (at the new level's health).
    /// Returns true if the meal leveled the pet up.
    pub fn feed(&mut self) -> bool {
        let before = self.level();
        self.loyalty_xp += FEED_LOYALTY;
        self.mend();
        self.level() > before
    }
}

/// The companions a player owns but is not leading: they rest at home and can
/// be called out at any capital Stable. A tame or a purchase never throws a
/// companion away any more; it lands here instead. Holds at most one pet of a
/// species, and never the species at the owner's heel, so every species a
/// player owns exists exactly once across the two.
#[derive(Clone, Debug, Default)]
pub struct Kennel {
    resting: Vec<Pet>,
}

/// Where a newly won companion went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adopted {
    /// There was no companion at the owner's heel, so it took the place.
    AtHeel,
    /// The heel was taken, so it went home to the kennel.
    Kenneled,
    /// The owner already keeps this species; nothing changed.
    AlreadyOwned,
}

/// What buying a companion at the Stable did.
#[derive(Clone, Copy, Debug)]
pub enum Bought {
    /// It is at the heel now; the companion it replaced, if any, went home.
    AtHeel {
        sent_home: Option<&'static PetSpecies>,
    },
    /// The owner already keeps this species; nothing changed.
    AlreadyOwned,
}

/// What calling a kennelled companion out to the owner's heel did.
#[derive(Clone, Copy, Debug)]
pub enum CalledOut {
    /// `called` is at the heel now; the companion it replaced, if any, went home.
    Swapped {
        called: &'static PetSpecies,
        sent_home: Option<&'static PetSpecies>,
    },
    /// No companion of that species rests in the kennel.
    NotKenneled,
}

impl Kennel {
    /// The resting companions, strongest rung first.
    pub fn resting(&self) -> &[Pet] {
        &self.resting
    }

    /// Whether the owner keeps `key` anywhere: at the heel or in the kennel.
    pub fn owns(&self, active: Option<&Pet>, key: &str) -> bool {
        active.is_some_and(|p| p.species.key == key)
            || self.resting.iter().any(|p| p.species.key == key)
    }

    /// Admit a companion that is not at the heel (a tame when the heel is
    /// taken, the old companion when a new one steps up, a reloaded save).
    /// Returns false, changing nothing, when the species is already owned.
    pub fn admit(&mut self, active: Option<&Pet>, pet: Pet) -> bool {
        if self.owns(active, pet.species.key) {
            return false;
        }
        self.resting.push(pet);
        self.resting
            .sort_by_key(|p| (std::cmp::Reverse(p.species.rung()), p.species.key));
        true
    }

    /// A companion won in the wild: to the heel if it is free, else home.
    pub fn adopt_tamed(&mut self, active: &mut Option<Pet>, species: &'static PetSpecies) -> Adopted {
        if self.owns(active.as_ref(), species.key) {
            return Adopted::AlreadyOwned;
        }
        match active {
            None => {
                *active = Some(Pet::new(species, 0));
                Adopted::AtHeel
            }
            Some(_) => {
                self.admit(active.as_ref(), Pet::new(species, 0));
                Adopted::Kenneled
            }
        }
    }

    /// A companion bought at the Stable steps straight to the heel; the one it
    /// replaces goes home rather than back to the wild.
    pub fn adopt_bought(&mut self, active: &mut Option<Pet>, species: &'static PetSpecies) -> Bought {
        if self.owns(active.as_ref(), species.key) {
            return Bought::AlreadyOwned;
        }
        let bought = Pet::new(species, 0);
        let sent_home = active.replace(bought);
        if let Some(old) = sent_home {
            self.admit(Some(&bought), old);
        }
        Bought::AtHeel {
            sent_home: sent_home.map(|p| p.species),
        }
    }

    /// Swap the kennelled `key` in for whatever is at the heel.
    pub fn call_out(&mut self, active: &mut Option<Pet>, key: &str) -> CalledOut {
        let Some(at) = self.resting.iter().position(|p| p.species.key == key) else {
            return CalledOut::NotKenneled;
        };
        let called = self.resting.remove(at);
        let sent_home = active.replace(called);
        if let Some(old) = sent_home {
            self.admit(Some(&called), old);
        }
        CalledOut::Swapped {
            called: called.species,
            sent_home: sent_home.map(|p| p.species),
        }
    }
}
