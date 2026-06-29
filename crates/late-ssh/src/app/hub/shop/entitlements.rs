use std::collections::HashSet;

use late_core::models::marketplace::{DYNAMIC_BONSAI_SKU, WONDERLAND_ULTIMATE_SKU};

use super::catalog::{is_aquarium_sku, is_pet_companion_sku};

#[derive(Clone, Debug, Default)]
pub struct ShopEntitlements {
    owned_skus: HashSet<String>,
}

impl ShopEntitlements {
    pub fn from_owned_skus(owned_skus: impl IntoIterator<Item = String>) -> Self {
        Self {
            owned_skus: owned_skus.into_iter().collect(),
        }
    }

    pub fn owns(&self, sku: &str) -> bool {
        self.owned_skus.contains(sku)
    }

    pub fn has_pet_companion(&self) -> bool {
        self.owned_skus.iter().any(|sku| is_pet_companion_sku(sku))
    }

    pub fn has_aquarium(&self) -> bool {
        self.owned_skus.iter().any(|sku| is_aquarium_sku(sku))
    }

    pub fn has_dynamic_bonsai(&self) -> bool {
        self.owns(DYNAMIC_BONSAI_SKU)
    }

    pub fn has_wonderland_ultimate(&self) -> bool {
        self.owns(WONDERLAND_ULTIMATE_SKU)
    }
}
