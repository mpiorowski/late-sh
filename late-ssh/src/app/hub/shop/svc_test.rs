use std::time::{Duration, Instant};

use super::{CUSTOM_TITLE_SCREEN_COOLDOWN, custom_title_precheck};

#[test]
fn a_buyer_who_cannot_afford_the_tier_is_refused_before_any_screen() {
    let now = Instant::now();
    assert_eq!(
        custom_title_precheck(1_999, 2_000, "Custom Title", None, now),
        Some("Need 2000 chips for Custom Title".to_string())
    );
    // Exactly the price is enough: the purchase's own rule is `balance < price`.
    assert_eq!(
        custom_title_precheck(2_000, 2_000, "Custom Title", None, now),
        None
    );
}

#[test]
fn a_second_screen_inside_the_cooldown_is_refused_and_says_how_long() {
    let now = Instant::now();
    let three_seconds_ago = now - Duration::from_secs(3);
    let refusal =
        custom_title_precheck(60_000, 2_000, "Custom Title", Some(three_seconds_ago), now);
    let expected_wait = (CUSTOM_TITLE_SCREEN_COOLDOWN - Duration::from_secs(3)).as_secs();
    assert_eq!(
        refusal,
        Some(format!(
            "Wait {expected_wait}s before screening another title"
        ))
    );

    let past_cooldown = now - CUSTOM_TITLE_SCREEN_COOLDOWN;
    assert_eq!(
        custom_title_precheck(60_000, 2_000, "Custom Title", Some(past_cooldown), now),
        None
    );
}

#[test]
fn the_balance_gate_comes_before_the_cooldown_gate() {
    // A broke buyer hears about the chips, not the clock: the clock only
    // matters once a screen could actually lead to a purchase.
    let now = Instant::now();
    let just_now = now - Duration::from_secs(1);
    assert_eq!(
        custom_title_precheck(100, 2_000, "Custom Title", Some(just_now), now),
        Some("Need 2000 chips for Custom Title".to_string())
    );
}
