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
    use late_core::models::username_effect::{
        GlowColor, USERNAME_EFFECT_DURATION_SECS, USERNAME_EFFECT_MONTH_DURATION_SECS,
        UsernameEffect,
    };

    let event = ActivityEvent::username_effect_applied(
        Uuid::nil(),
        "user",
        UsernameEffect::Glow(GlowColor::Gold),
        USERNAME_EFFECT_DURATION_SECS,
    );

    assert!(lounge_includes(&event));
    assert_eq!(event.action, "is glowing (24h)");

    // The month tier announces its own window rather than the day tier's.
    let month = ActivityEvent::username_effect_applied(
        Uuid::nil(),
        "user",
        UsernameEffect::Shimmer,
        USERNAME_EFFECT_MONTH_DURATION_SECS,
    );

    assert!(lounge_includes(&month));
    assert_eq!(month.action, "is shimmering (30d)");
}

#[test]
fn lounge_includes_stream_viewers() {
    let event = ActivityEvent::watching_stream(Uuid::nil(), "bob", "mat".to_string());

    assert!(lounge_includes(&event));
    assert_eq!(event.action, "is watching mat's stream");
}
