use std::time::Duration;

use late_core::models::marketplace::{THEMATRIX_ULTIMATE_SKU, WONDERLAND_ULTIMATE_SKU};

use super::effects::{THEMATRIX_TOTAL_MS, UltimateEffectKind};

pub(super) const WONDERLAND_ID: &str = "wonderland";
pub(super) const THEMATRIX_ID: &str = "thematrix";
pub(super) const ULTIMATE_SPELL_DURATION_MS: u64 = 10_000;
pub(crate) const ULTIMATE_CAST_COOLDOWN: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum UltimateKind {
    Wonderland,
    Thematrix,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UltimateSpell {
    pub kind: UltimateKind,
    pub id: &'static str,
    pub sku: &'static str,
    pub name: &'static str,
    pub duration_ms: u64,
    pub effect_kind: UltimateEffectKind,
}

pub(super) const ULTIMATE_SPELLS: &[UltimateSpell] = &[
    UltimateSpell {
        kind: UltimateKind::Wonderland,
        id: WONDERLAND_ID,
        sku: WONDERLAND_ULTIMATE_SKU,
        name: "Wonderland",
        duration_ms: ULTIMATE_SPELL_DURATION_MS,
        effect_kind: UltimateEffectKind::Wonderland,
    },
    UltimateSpell {
        kind: UltimateKind::Thematrix,
        id: THEMATRIX_ID,
        sku: THEMATRIX_ULTIMATE_SKU,
        name: "The Matrix",
        duration_ms: THEMATRIX_TOTAL_MS,
        effect_kind: UltimateEffectKind::Thematrix,
    },
];

impl UltimateKind {
    pub(crate) fn manifest(self) -> &'static UltimateSpell {
        ULTIMATE_SPELLS
            .iter()
            .find(|spell| spell.kind == self)
            .expect("ultimate kind must have manifest entry")
    }

    pub(crate) fn id(self) -> &'static str {
        self.manifest().id
    }

    pub(crate) fn name(self) -> &'static str {
        self.manifest().name
    }

    pub(crate) fn duration_ms(self) -> u64 {
        self.manifest().duration_ms
    }

    pub(crate) fn effect_kind(self) -> UltimateEffectKind {
        self.manifest().effect_kind
    }

    pub(crate) fn from_sku(sku: &str) -> Option<Self> {
        ULTIMATE_SPELLS
            .iter()
            .find(|spell| spell.sku == sku)
            .map(|spell| spell.kind)
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        ULTIMATE_SPELLS
            .iter()
            .find(|spell| spell.id == id)
            .map(|spell| spell.kind)
    }
}
