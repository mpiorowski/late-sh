use serde_json::{Value, json};

/// `shop_consumable_effects.effect_kind` for the user-scoped 24h username
/// effects (Name Glow / Name Gradient / Name Shimmer). One active effect per
/// user: activating any username effect deactivates the previous one.
pub const USERNAME_EFFECT_KIND: &str = "username_effect";

pub const USERNAME_GLOW_SKU: &str = "username_glow_day";
pub const USERNAME_GRADIENT_SKU: &str = "username_gradient_day";
pub const USERNAME_SHIMMER_SKU: &str = "username_shimmer_day";

/// Default effect duration when an item payload omits `duration_secs`.
pub const USERNAME_EFFECT_DURATION_SECS: i64 = 86_400;

/// The buyer-picked color for the Name Glow effect. RGB values live in
/// `late-ssh` (theme territory); this enum only names the choice so the
/// purchase payload round-trips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlowColor {
    Ember,
    Gold,
    Lime,
    Aqua,
    Sky,
    Orchid,
}

impl GlowColor {
    pub const ALL: [Self; 6] = [
        Self::Ember,
        Self::Gold,
        Self::Lime,
        Self::Aqua,
        Self::Sky,
        Self::Orchid,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Ember => "ember",
            Self::Gold => "gold",
            Self::Lime => "lime",
            Self::Aqua => "aqua",
            Self::Sky => "sky",
            Self::Orchid => "orchid",
        }
    }

    pub fn parse_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.slug() == slug)
    }
}

/// The buyer-picked color pair for the Name Gradient effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GradientPair {
    Sunset,
    Ocean,
    Dusk,
    Forest,
    Candy,
    Flare,
}

impl GradientPair {
    pub const ALL: [Self; 6] = [
        Self::Sunset,
        Self::Ocean,
        Self::Dusk,
        Self::Forest,
        Self::Candy,
        Self::Flare,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Sunset => "sunset",
            Self::Ocean => "ocean",
            Self::Dusk => "dusk",
            Self::Forest => "forest",
            Self::Candy => "candy",
            Self::Flare => "flare",
        }
    }

    pub fn parse_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|pair| pair.slug() == slug)
    }
}

/// A purchased 24h username effect, as persisted in the effect row payload.
/// Glow paints the name one bright color, Gradient fades it between a preset
/// pair, Shimmer animates through the glow palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UsernameEffect {
    Glow(GlowColor),
    Gradient(GradientPair),
    Shimmer,
}

impl UsernameEffect {
    /// The item-payload `variant` key this choice belongs to; the purchase
    /// path rejects a choice whose variant does not match the bought item.
    pub fn variant_key(self) -> &'static str {
        match self {
            Self::Glow(_) => "glow",
            Self::Gradient(_) => "gradient",
            Self::Shimmer => "shimmer",
        }
    }

    /// Stable identity string, e.g. `glow:ember` — used for activity
    /// repeat-throttle keys.
    pub fn slug(self) -> String {
        match self {
            Self::Glow(color) => format!("glow:{}", color.slug()),
            Self::Gradient(pair) => format!("gradient:{}", pair.slug()),
            Self::Shimmer => "shimmer".to_string(),
        }
    }

    /// The `shop_consumable_effects.payload` for this choice.
    pub fn to_payload(self) -> Value {
        match self {
            Self::Glow(color) => json!({"variant": "glow", "color": color.slug()}),
            Self::Gradient(pair) => json!({"variant": "gradient", "color": pair.slug()}),
            Self::Shimmer => json!({"variant": "shimmer"}),
        }
    }

    /// Parse an effect row payload; `None` on unknown variant/color so
    /// readers can warn and skip rather than fail.
    pub fn from_payload(payload: &Value) -> Option<Self> {
        let variant = payload.get("variant")?.as_str()?;
        let color = payload.get("color").and_then(Value::as_str);
        match variant {
            "glow" => Some(Self::Glow(GlowColor::parse_slug(color?)?)),
            "gradient" => Some(Self::Gradient(GradientPair::parse_slug(color?)?)),
            "shimmer" => Some(Self::Shimmer),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_effects() -> Vec<UsernameEffect> {
        let mut effects: Vec<UsernameEffect> = GlowColor::ALL
            .into_iter()
            .map(UsernameEffect::Glow)
            .collect();
        effects.extend(GradientPair::ALL.into_iter().map(UsernameEffect::Gradient));
        effects.push(UsernameEffect::Shimmer);
        effects
    }

    #[test]
    fn glow_and_gradient_slugs_round_trip() {
        for color in GlowColor::ALL {
            assert_eq!(GlowColor::parse_slug(color.slug()), Some(color));
        }
        for pair in GradientPair::ALL {
            assert_eq!(GradientPair::parse_slug(pair.slug()), Some(pair));
        }
        assert_eq!(GlowColor::parse_slug("mauve"), None);
        assert_eq!(GradientPair::parse_slug("void"), None);
    }

    #[test]
    fn payload_round_trips_for_every_effect() {
        for effect in all_effects() {
            let payload = effect.to_payload();
            assert_eq!(UsernameEffect::from_payload(&payload), Some(effect));
            assert_eq!(
                payload.get("variant").and_then(Value::as_str),
                Some(effect.variant_key())
            );
        }
    }

    #[test]
    fn from_payload_rejects_unknown_or_incomplete() {
        assert_eq!(
            UsernameEffect::from_payload(&json!({"variant": "sparkle"})),
            None
        );
        assert_eq!(
            UsernameEffect::from_payload(&json!({"variant": "glow", "color": "mauve"})),
            None
        );
        assert_eq!(
            UsernameEffect::from_payload(&json!({"variant": "glow"})),
            None
        );
        assert_eq!(UsernameEffect::from_payload(&json!({})), None);
    }

    #[test]
    fn slugs_are_unique_across_all_effects() {
        let mut seen = std::collections::HashSet::new();
        for effect in all_effects() {
            assert!(
                seen.insert(effect.slug()),
                "duplicate slug {}",
                effect.slug()
            );
        }
    }
}
