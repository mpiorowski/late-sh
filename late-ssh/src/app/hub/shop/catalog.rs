use late_core::models::marketplace::CAT_COMPANION_SKU;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShopCategory {
    Companions,
}

impl ShopCategory {
    pub const ALL: [Self; 1] = [Self::Companions];

    pub fn label(self) -> &'static str {
        match self {
            Self::Companions => "Companions",
        }
    }

    pub fn matches_kind(self, item_kind: &str) -> bool {
        match self {
            Self::Companions => item_kind == "feature_unlock",
        }
    }
}

pub fn is_cat_companion_sku(sku: &str) -> bool {
    sku == CAT_COMPANION_SKU
}
