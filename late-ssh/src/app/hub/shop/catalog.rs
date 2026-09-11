use late_core::models::{
    marketplace::{
        AQUARIUM_CONSUMABLE_ITEM_KIND, AQUARIUM_FISH_ITEM_KIND, AQUARIUM_PLANT_ITEM_KIND,
        AQUARIUM_SKU, BONSAI_CONSUMABLE_ITEM_KIND, CHAT_CONSUMABLE_ITEM_KIND,
        COMPANION_CONSUMABLE_ITEM_KIND, PET_COMPANION_SKU, USERNAME_EFFECT_ITEM_KIND,
    },
    rental::TITLE_RENTAL_ITEM_KIND,
};

use super::svc::ShopCatalogItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShopCategory {
    Companions,
    Fish,
    Plants,
    Chat,
    Badges,
    Flags,
    Ultimates,
}

impl ShopCategory {
    /// Tab order. The name-adjacent tabs lead (Chat, then the badge and flag
    /// rentals it stacks with), then the unlocks, the tank's two kinds of
    /// stock, and the burn tier.
    pub(crate) const ALL: [Self; 7] = [
        Self::Chat,
        Self::Badges,
        Self::Flags,
        Self::Companions,
        Self::Fish,
        Self::Plants,
        Self::Ultimates,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Companions => "Companions",
            Self::Fish => "Fish",
            Self::Plants => "Plants",
            Self::Chat => "Chat",
            Self::Badges => "Badges",
            Self::Flags => "Flags",
            Self::Ultimates => "Ultimates",
        }
    }

    pub(crate) fn matches_item(self, item: &ShopCatalogItem) -> bool {
        match self {
            // Everything you keep alive, in one tab: the pet, the bonsai
            // shield, and the tank with its shield. Section rows split the
            // three; the tank's stock has tabs of its own.
            Self::Companions => {
                item.item_kind == "feature_unlock"
                    || item.item_kind == COMPANION_CONSUMABLE_ITEM_KIND
                    || item.item_kind == BONSAI_CONSUMABLE_ITEM_KIND
                    || item.item_kind == AQUARIUM_CONSUMABLE_ITEM_KIND
            }
            // The fish, led by the fry the tank comes with; the plants, led
            // by the sprout on the floor. Two kinds, two caps, two clocks.
            Self::Fish => item.item_kind == AQUARIUM_FISH_ITEM_KIND,
            Self::Plants => item.item_kind == AQUARIUM_PLANT_ITEM_KIND,
            Self::Chat => {
                item.item_kind == CHAT_CONSUMABLE_ITEM_KIND
                    || item.item_kind == USERNAME_EFFECT_ITEM_KIND
                    || item.item_kind == TITLE_RENTAL_ITEM_KIND
            }
            Self::Badges => item.is_chat_badge() && !item.is_flag_badge(),
            Self::Flags => item.is_flag_badge(),
            // The two dearest things the shop sells share a tab: the burn
            // milestones and the spells. Section rows split them in the list.
            Self::Ultimates => item.is_ultimate_spell() || item.is_milestone_badge(),
        }
    }
}

pub(crate) fn is_pet_companion_sku(sku: &str) -> bool {
    sku == PET_COMPANION_SKU
}

pub(crate) fn is_aquarium_sku(sku: &str) -> bool {
    sku == AQUARIUM_SKU
}
