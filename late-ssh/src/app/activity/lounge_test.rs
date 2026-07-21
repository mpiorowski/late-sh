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
    use late_core::models::username_effect::{GlowColor, UsernameEffect};

    let mut recent = HashMap::new();
    let user = Uuid::now_v7();
    let ember = ActivityEvent::username_effect_applied(
        user,
        "mira",
        UsernameEffect::Glow(GlowColor::Ember),
    );
    assert!(!is_repeat(&mut recent, &ember));
    // Rebuying the same look inside the window stays quiet...
    assert!(is_repeat(&mut recent, &ember));
    // ...but a new color or a new style visibly changed the name, so it
    // announces again.
    let sky =
        ActivityEvent::username_effect_applied(user, "mira", UsernameEffect::Glow(GlowColor::Sky));
    assert!(!is_repeat(&mut recent, &sky));
    let shimmer = ActivityEvent::username_effect_applied(user, "mira", UsernameEffect::Shimmer);
    assert!(!is_repeat(&mut recent, &shimmer));
}
