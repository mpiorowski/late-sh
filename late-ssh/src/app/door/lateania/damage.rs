// Damage types and the resistance system for Lateania.
//
// Every offensive ability and every mob attack carries a DamageType. Mobs have a
// resistance profile - the types they shrug off and the types that flay them -
// so element choice is a real tactical lever rather than flavor. Damage resolves
// through a single multiplier in the combat runtime.

/// The schools of damage. Physical is the plain weapon/auto-attack school;
/// the rest are elemental or divine and key off mob weaknesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageType {
    Physical,
    Fire,
    Frost,
    Holy,
    Shadow,
    Poison,
    Arcane,
    Lightning,
}

impl DamageType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::Fire => "fire",
            Self::Frost => "frost",
            Self::Holy => "holy",
            Self::Shadow => "shadow",
            Self::Poison => "poison",
            Self::Arcane => "arcane",
            Self::Lightning => "lightning",
        }
    }

    /// A short colored-word tag for combat log flavor.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Physical => "strikes",
            Self::Fire => "burns",
            Self::Frost => "freezes",
            Self::Holy => "sears",
            Self::Shadow => "withers",
            Self::Poison => "poisons",
            Self::Arcane => "blasts",
            Self::Lightning => "shocks",
        }
    }
}

/// How a mob responds to each damage type. Resist halves; Weak adds 50%.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defense {
    Resist,
    Normal,
    Weak,
}

impl Defense {
    /// Damage multiplier in percent (50 = half, 100 = normal, 150 = +50%).
    pub fn multiplier_pct(self) -> i32 {
        match self {
            Self::Resist => 50,
            Self::Normal => 100,
            Self::Weak => 150,
        }
    }
}

/// The themed vocabulary of the world resist/weak pass (THUNDERSMITH.md
/// section 13). Every generated zone names one theme, and the zone's regular
/// mobs inherit the theme's profile; bosses keep their own. The set is closed
/// and the mapping exhaustive, so the whole pass is auditable as data: a theme
/// can never place Physical in either slot (a zone-wide Physical resist would
/// tax the seven Physical-locked classes with no counterplay, and nothing is
/// ever weak to Physical), it always carries a weakness (weak-forward: the
/// right school is a reward, walls are events), and resists exist on a
/// minority of themes only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneTheme {
    /// Burnt, magma-born country: shrugs off Fire, fears Frost.
    Ashen,
    /// Sun-cracked heat without the fire-born flesh: fears Frost.
    Sunscorched,
    /// Ice-locked country: shrugs off Frost, fears Fire.
    Frozen,
    /// Green, growing, burnable country: fears Fire.
    Verdant,
    /// Open water and wet ground, a conductor: fears Lightning.
    Tidal,
    /// The cold drowned deep: shrugs off Frost, fears Lightning.
    Drowned,
    /// Storm-born country: shrugs off Lightning, unravels to Arcane.
    Storm,
    /// Song, echo, and standing wards: unravels to Arcane.
    Resonant,
    /// The walking dead: shrug off Shadow, wither under Holy.
    Undead,
    /// Ghosts and hauntings without barrow-flesh: wither under Holy.
    Haunted,
    /// God-cults and the profaned divine: shrug off Holy, fear Shadow.
    Profane,
    /// Glamour and fae light: the dark undoes it, fears Shadow.
    Fae,
    /// Living beasts and vermin: flesh that poison kills, fears Poison.
    Beastwild,
    /// Spore, rot, and venom: shrugs off Poison, burns well, fears Fire.
    Fungal,
    /// Bloodless made things: shrug off Poison, overload under Lightning.
    Construct,
    /// Glass, shard, and crystal: shatters under Lightning.
    Crystal,
}

impl ZoneTheme {
    /// The school this theme's regulars resist, if any. Never Physical.
    pub const fn resist(self) -> Option<DamageType> {
        match self {
            Self::Ashen => Some(DamageType::Fire),
            Self::Sunscorched => None,
            Self::Frozen => Some(DamageType::Frost),
            Self::Verdant => None,
            Self::Tidal => None,
            Self::Drowned => Some(DamageType::Frost),
            Self::Storm => Some(DamageType::Lightning),
            Self::Resonant => None,
            Self::Undead => Some(DamageType::Shadow),
            Self::Haunted => None,
            Self::Profane => Some(DamageType::Holy),
            Self::Fae => None,
            Self::Beastwild => None,
            Self::Fungal => Some(DamageType::Poison),
            Self::Construct => Some(DamageType::Poison),
            Self::Crystal => None,
        }
    }

    /// The school this theme's regulars are weak to. Always present
    /// (weak-forward), never Physical.
    pub const fn weak(self) -> Option<DamageType> {
        match self {
            Self::Ashen => Some(DamageType::Frost),
            Self::Sunscorched => Some(DamageType::Frost),
            Self::Frozen => Some(DamageType::Fire),
            Self::Verdant => Some(DamageType::Fire),
            Self::Tidal => Some(DamageType::Lightning),
            Self::Drowned => Some(DamageType::Lightning),
            Self::Storm => Some(DamageType::Arcane),
            Self::Resonant => Some(DamageType::Arcane),
            Self::Undead => Some(DamageType::Holy),
            Self::Haunted => Some(DamageType::Holy),
            Self::Profane => Some(DamageType::Shadow),
            Self::Fae => Some(DamageType::Shadow),
            Self::Beastwild => Some(DamageType::Poison),
            Self::Fungal => Some(DamageType::Fire),
            Self::Construct => Some(DamageType::Lightning),
            Self::Crystal => Some(DamageType::Lightning),
        }
    }

    /// Every theme, for census tests over the vocabulary itself.
    pub const ALL: [ZoneTheme; 16] = [
        Self::Ashen,
        Self::Sunscorched,
        Self::Frozen,
        Self::Verdant,
        Self::Tidal,
        Self::Drowned,
        Self::Storm,
        Self::Resonant,
        Self::Undead,
        Self::Haunted,
        Self::Profane,
        Self::Fae,
        Self::Beastwild,
        Self::Fungal,
        Self::Construct,
        Self::Crystal,
    ];
}

/// A mob's full damage profile: the type it deals, plus up to one resisted and
/// one weak school. Built as data on each MobSpawn.
#[derive(Clone, Copy, Debug)]
pub struct DamageProfile {
    /// The damage type this mob's own attacks deal.
    pub attack_type: DamageType,
    /// The school this mob resists (takes half), if any.
    pub resist: Option<DamageType>,
    /// The school this mob is weak to (takes +50%), if any.
    pub weak: Option<DamageType>,
}

impl DamageProfile {
    pub const fn new(
        attack_type: DamageType,
        resist: Option<DamageType>,
        weak: Option<DamageType>,
    ) -> Self {
        Self {
            attack_type,
            resist,
            weak,
        }
    }

    /// Plain physical bruiser with no elemental quirks.
    pub const fn physical() -> Self {
        Self::new(DamageType::Physical, None, None)
    }

    pub fn defense_against(&self, incoming: DamageType) -> Defense {
        if self.weak == Some(incoming) {
            Defense::Weak
        } else if self.resist == Some(incoming) {
            Defense::Resist
        } else {
            Defense::Normal
        }
    }

    /// Resolve incoming damage of a school against this profile.
    pub fn apply(&self, raw: i32, incoming: DamageType) -> (i32, Defense) {
        let def = self.defense_against(incoming);
        let scaled = (raw * def.multiplier_pct() / 100).max(1);
        (scaled, def)
    }
}
