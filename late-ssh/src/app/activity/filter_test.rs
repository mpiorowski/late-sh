use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;
use crate::app::activity::filter::*;

#[test]
fn dashboard_filter_includes_public_activity() {
    let event = ActivityEvent::joined(Uuid::nil(), "user");

    assert!(ActivityFilter::dashboard().includes(&event));
}

#[test]
fn lounge_includes_username_effects() {
    use late_core::models::username_effect::{GlowColor, UsernameEffect};

    let event = ActivityEvent::username_effect_applied(
        Uuid::nil(),
        "user",
        UsernameEffect::Glow(GlowColor::Gold),
    );

    assert!(lounge_includes(&event));
    assert_eq!(event.action, "is glowing (24h)");
}
