use super::*;

#[test]
fn daily_results_dedupe_per_match_not_per_game() {
    let mut recent = HashMap::new();
    let winner = Uuid::now_v7();
    let first = ActivityEvent::daily_win(winner, "mira", "Chess", Uuid::now_v7());
    assert!(!is_repeat(&mut recent, &first));
    // A re-emit of the same finished match stays deduped...
    assert!(is_repeat(&mut recent, &first));
    // ...but a second match the same winner finishes at the same game is
    // its own line: the contract is one announcement per match.
    let second = ActivityEvent::daily_win(winner, "mira", "Chess", Uuid::now_v7());
    assert!(!is_repeat(&mut recent, &second));
}

#[test]
fn repeat_window_drops_same_shape_and_keeps_distinct() {
    let mut recent = HashMap::new();
    let sit = ActivityEvent::sat_down(
        Uuid::nil(),
        "mira",
        crate::app::activity::event::ActivityGame::Poker,
    );
    assert!(!is_repeat(&mut recent, &sit));
    assert!(is_repeat(&mut recent, &sit));

    let other_game = ActivityEvent::sat_down(
        Uuid::nil(),
        "mira",
        crate::app::activity::event::ActivityGame::Chess,
    );
    assert!(!is_repeat(&mut recent, &other_game));

    let other_user = ActivityEvent::sat_down(
        Uuid::now_v7(),
        "someone-else",
        crate::app::activity::event::ActivityGame::Poker,
    );
    assert!(!is_repeat(&mut recent, &other_user));
}

#[test]
fn username_effect_repeat_keys_on_full_style_slug() {
    use late_core::models::rental::RENTAL_DAY_SECS;
    use late_core::models::username_effect::{GlowColor, UsernameEffect};

    let mut recent = HashMap::new();
    let user = Uuid::now_v7();
    let ember = ActivityEvent::username_effect_applied(
        user,
        "mira",
        UsernameEffect::Glow(GlowColor::Ember),
        RENTAL_DAY_SECS,
    );
    assert!(!is_repeat(&mut recent, &ember));
    // Rebuying the same look inside the window stays quiet...
    assert!(is_repeat(&mut recent, &ember));
    // ...but a new color or a new style visibly changed the name, so it
    // announces again.
    let sky = ActivityEvent::username_effect_applied(
        user,
        "mira",
        UsernameEffect::Glow(GlowColor::Sky),
        RENTAL_DAY_SECS,
    );
    assert!(!is_repeat(&mut recent, &sky));
    let shimmer = ActivityEvent::username_effect_applied(
        user,
        "mira",
        UsernameEffect::Shimmer,
        RENTAL_DAY_SECS,
    );
    assert!(!is_repeat(&mut recent, &shimmer));
}

/// A whale who climbs two rungs in one sitting gets two lines: the rungs are
/// distinct purchases and each is its own six-figure story. Keying on the
/// buyer alone would have swallowed the second.
#[test]
fn burn_milestones_key_on_the_rung_not_the_buyer() {
    let mut recent = HashMap::new();
    let buyer = Uuid::now_v7();
    let wick = ActivityEvent::burn_milestone(buyer, "mira", "Wick", "\u{1F56F}\u{FE0F}", 50_000);
    let furnace = ActivityEvent::burn_milestone(buyer, "mira", "Furnace", "\u{1F30B}", 500_000);

    assert!(!is_repeat(&mut recent, &wick));
    assert!(!is_repeat(&mut recent, &furnace));
    assert!(is_repeat(&mut recent, &wick));
}

/// The line is the receipt: it names the price in full, with separators, and
/// shows the glyph everyone is about to see beside the name.
#[test]
fn a_burn_milestone_line_quotes_the_price_and_the_glyph() {
    let event = ActivityEvent::burn_milestone(Uuid::now_v7(), "mira", "Fuse", "\u{1F9E8}", 150_000);
    assert_eq!(event.action, "burned 150,000 chips for the Fuse \u{1F9E8}");
    assert!(!event.action.contains('@'));
}
