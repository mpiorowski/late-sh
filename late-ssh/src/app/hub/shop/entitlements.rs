use std::collections::HashSet;

use super::catalog::is_cat_companion_sku;

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

    pub fn has_cat_companion(&self) -> bool {
        self.owned_skus.iter().any(|sku| is_cat_companion_sku(sku))
    }
}
