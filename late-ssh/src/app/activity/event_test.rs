use uuid::Uuid;

use super::event::ActivityEvent;

// Feed actions become #lounge chat bodies, and the send path runs the
// mention pipeline on every body: a `@name` smuggled through a free-text
// title would mint a real mention notification from a system-authored line.
#[test]
fn went_live_strips_mentions_from_the_title() {
    let event = ActivityEvent::went_live(
        Uuid::now_v7(),
        "mat",
        Some("come hang @alice @bob".to_string()),
    );
    assert_eq!(event.action, "is live: come hang alice bob");

    let event = ActivityEvent::went_live(Uuid::now_v7(), "mat", Some("@@@".to_string()));
    assert_eq!(event.action, "is live");

    let event = ActivityEvent::went_live(Uuid::now_v7(), "mat", None);
    assert_eq!(event.action, "is live");
}

#[test]
fn cyberspace_posted_strips_mentions_from_the_title() {
    let event =
        ActivityEvent::cyberspace_posted(Uuid::now_v7(), "mat", Some("ping @alice".to_string()));
    assert_eq!(event.action, "published \"ping alice\" on cyberspace");
}
