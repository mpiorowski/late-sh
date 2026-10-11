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
    Chat,
    Badges,
    Flags,
    Ultimates,
}

impl ShopCategory {
    /// Tab order. The name-adjacent tabs lead (Chat, then the badge and flag
    /// rentals it stacks with), the unlocks and the burn tier follow.
    pub(crate) const ALL: [Self; 5] = [
        Self::Chat,
        Self::Badges,
        Self::Flags,
        Self::Companions,
        Self::Ultimates,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Companions => "Companions",
            Self::Chat => "Chat",
            Self::Badges => "Badges",
            Self::Flags => "Flags",
            Self::Ultimates => "Ultimates",
        }
    }

    pub(crate) fn matches_item(self, item: &ShopCatalogItem) -> bool {
        match self {
            // Everything you keep alive, in one tab: the pet, the bonsai
            // shield, and the tank with its shield, its plants, and its
            // fish. `CompanionSection` splits and orders them.
            Self::Companions => {
                item.item_kind == "feature_unlock"
                    || item.item_kind == COMPANION_CONSUMABLE_ITEM_KIND
                    || item.item_kind == BONSAI_CONSUMABLE_ITEM_KIND
                    || item.item_kind == AQUARIUM_CONSUMABLE_ITEM_KIND
                    || item.item_kind == AQUARIUM_FISH_ITEM_KIND
                    || item.item_kind == AQUARIUM_PLANT_ITEM_KIND
            }
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

/// The Companions tab's section rows, in the order the tab lists them:
/// the list is sorted by this (`ShopState::visible_items`, stable, so
/// catalog order holds inside a section) and labelled by it
/// (`shop::ui::item_list_rows`). The tank's own growth (the fry and the
/// sprout, listed and never sold) sits between the tank and what it sells,
/// the plants before the fish.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum CompanionSection {
    Pet,
    Bonsai,
    Aquarium,
    Growing,
    Plants,
    Fish,
}

impl CompanionSection {
    pub(crate) fn of(item: &ShopCatalogItem) -> Self {
        if item.is_pet_companion() {
            Self::Pet
        } else if item.is_bonsai_decay_shield() {
            Self::Bonsai
        } else if item.is_welcome_fish() || item.is_sprout() {
            Self::Growing
        } else if item.is_aquarium_plant() {
            Self::Plants
        } else if item.is_aquarium_fish() {
            Self::Fish
        } else {
            Self::Aquarium
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pet => "Pet",
            Self::Bonsai => "Bonsai",
            Self::Aquarium => "Aquarium",
            Self::Growing => "Growing",
            Self::Plants => "Plants",
            Self::Fish => "Fish",
        }
    }
}

pub(crate) fn is_pet_companion_sku(sku: &str) -> bool {
    sku == PET_COMPANION_SKU
}

pub(crate) fn is_aquarium_sku(sku: &str) -> bool {
    sku == AQUARIUM_SKU
}
