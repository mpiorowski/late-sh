use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;
use crate::app::activity::filter::*;

#[test]
fn dashboard_filter_includes_public_activity() {
    let event = ActivityEvent::joined(Uuid::nil(), "user");

    assert!(ActivityFilter::dashboard().includes(&event));
}

#[test]
fn lounge_includes_rented_badges_and_titles() {
    use late_core::models::rental::{RENTAL_DAY_SECS, RENTAL_MONTH_SECS};

    let badge = ActivityEvent::badge_rented(Uuid::nil(), "mira", "🐱", RENTAL_DAY_SECS);
    assert!(lounge_includes(&badge));
    assert_eq!(badge.action, "rented 🐱 (24h)");

    let title =
        ActivityEvent::title_applied(Uuid::nil(), "mira", "the night clerk", RENTAL_MONTH_SECS);
    assert!(lounge_includes(&title));
    assert_eq!(title.action, "is now the night clerk (30d)");
    // Feed bodies never carry an @.
    assert!(!badge.action.contains('@') && !title.action.contains('@'));
}

#[test]
fn lounge_includes_username_effects() {
    use late_core::models::rental::{RENTAL_DAY_SECS, RENTAL_MONTH_SECS};
    use late_core::models::username_effect::{GlowColor, UsernameEffect};

    let event = ActivityEvent::username_effect_applied(
        Uuid::nil(),
        "user",
        UsernameEffect::Glow(GlowColor::Gold),
        RENTAL_DAY_SECS,
    );

    assert!(lounge_includes(&event));
    assert_eq!(event.action, "is glowing (24h)");

    // The month tier announces its own window rather than the day tier's.
    let month = ActivityEvent::username_effect_applied(
        Uuid::nil(),
        "user",
        UsernameEffect::Shimmer,
        RENTAL_MONTH_SECS,
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

/// The threshold line ships, it names only the author, it points at the room,
/// and it carries no `@` (feed bodies run through the mention pipeline).
#[test]
fn lounge_includes_the_gild_threshold_line() {
    use late_core::models::chat_message_gild::GILD_FEED_THRESHOLD;

    let event = ActivityEvent::message_gilded(
        Uuid::nil(),
        "mira",
        Uuid::nil(),
        GILD_FEED_THRESHOLD,
        Some("lounge".to_string()),
    );
    assert!(lounge_includes(&event));
    assert_eq!(event.action, "got a message gilded 3 times in #lounge");
    assert!(!event.action.contains('@'));

    // A room with no slug still posts; it just cannot say where.
    let roomless = ActivityEvent::message_gilded(Uuid::nil(), "mira", Uuid::nil(), 3, None);
    assert_eq!(roomless.action, "got a message gilded 3 times");
}
