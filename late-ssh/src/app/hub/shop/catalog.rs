use late_core::models::marketplace::{
    AQUARIUM_FISH_ITEM_KIND, AQUARIUM_SKU, CAT_COMPANION_SKU, CHAT_BADGE_SLOT,
};

use super::svc::ShopCatalogItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShopCategory {
    Companions,
    Aquarium,
    Badges,
    Ultimates,
}

impl ShopCategory {
    pub const ALL: [Self; 4] = [
        Self::Companions,
        Self::Aquarium,
        Self::Badges,
        Self::Ultimates,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Companions => "Companions",
            Self::Aquarium => "Aquarium",
            Self::Badges => "Badges",
            Self::Ultimates => "Ultimates",
        }
    }

    pub fn matches_item(self, item: &ShopCatalogItem) -> bool {
        match self {
            Self::Companions => item.item_kind == "feature_unlock" && !is_aquarium_sku(&item.sku),
            Self::Aquarium => {
                is_aquarium_sku(&item.sku) || item.item_kind == AQUARIUM_FISH_ITEM_KIND
            }
            Self::Badges => item.is_chat_badge(),
            Self::Ultimates => item.is_ultimate_spell(),
        }
    }
}

pub fn is_cat_companion_sku(sku: &str) -> bool {
    sku == CAT_COMPANION_SKU
}

pub fn is_aquarium_sku(sku: &str) -> bool {
    sku == AQUARIUM_SKU
}

pub fn is_chat_badge_slot(slot: Option<&str>) -> bool {
    slot == Some(CHAT_BADGE_SLOT)
}
