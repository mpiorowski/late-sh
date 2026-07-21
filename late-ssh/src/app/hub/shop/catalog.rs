use late_core::models::marketplace::{
    AQUARIUM_FISH_ITEM_KIND, AQUARIUM_SKU, CHAT_BADGE_SLOT, CHAT_CONSUMABLE_ITEM_KIND,
    CHAT_FLAG_SLOT, COMPANION_CONSUMABLE_ITEM_KIND, PET_COMPANION_SKU, USERNAME_EFFECT_ITEM_KIND,
};

use super::svc::ShopCatalogItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShopCategory {
    Companions,
    Chat,
    Aquarium,
    Badges,
    Flags,
    Ultimates,
}

impl ShopCategory {
    pub(crate) const ALL: [Self; 6] = [
        Self::Chat,
        Self::Companions,
        Self::Aquarium,
        Self::Badges,
        Self::Flags,
        Self::Ultimates,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Companions => "Companions",
            Self::Chat => "Chat",
            Self::Aquarium => "Aquarium",
            Self::Badges => "Badges",
            Self::Flags => "Flags",
            Self::Ultimates => "Ultimates",
        }
    }

    pub(crate) fn matches_item(self, item: &ShopCatalogItem) -> bool {
        match self {
            Self::Companions => {
                item.item_kind == "feature_unlock"
                    || item.item_kind == COMPANION_CONSUMABLE_ITEM_KIND
            }
            Self::Chat => {
                item.item_kind == CHAT_CONSUMABLE_ITEM_KIND
                    || item.item_kind == USERNAME_EFFECT_ITEM_KIND
            }
            Self::Aquarium => item.item_kind == AQUARIUM_FISH_ITEM_KIND,
            Self::Badges => item.is_chat_badge() && !item.is_flag_badge(),
            Self::Flags => item.is_flag_badge(),
            Self::Ultimates => item.is_ultimate_spell(),
        }
    }
}

pub(crate) fn is_pet_companion_sku(sku: &str) -> bool {
    sku == PET_COMPANION_SKU
}

pub(crate) fn is_aquarium_sku(sku: &str) -> bool {
    sku == AQUARIUM_SKU
}

pub(crate) fn is_chat_badge_slot(slot: Option<&str>) -> bool {
    matches!(slot, Some(CHAT_BADGE_SLOT | CHAT_FLAG_SLOT))
}
