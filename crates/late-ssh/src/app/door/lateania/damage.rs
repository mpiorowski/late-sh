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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weakness_amplifies_and_resist_reduces() {
        let undead = DamageProfile::new(
            DamageType::Shadow,
            Some(DamageType::Shadow),
            Some(DamageType::Holy),
        );
        let (holy, def_h) = undead.apply(100, DamageType::Holy);
        let (shadow, def_s) = undead.apply(100, DamageType::Shadow);
        let (phys, def_p) = undead.apply(100, DamageType::Physical);
        assert_eq!((holy, def_h), (150, Defense::Weak));
        assert_eq!((shadow, def_s), (50, Defense::Resist));
        assert_eq!((phys, def_p), (100, Defense::Normal));
    }

    #[test]
    fn damage_never_drops_below_one() {
        let p = DamageProfile::new(DamageType::Fire, Some(DamageType::Fire), None);
        let (dmg, _) = p.apply(1, DamageType::Fire);
        assert!(dmg >= 1);
    }
}
