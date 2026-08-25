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
