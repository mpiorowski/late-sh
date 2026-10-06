use crate::app::lobby::house::poker::settings::*;

#[test]
fn invalid_small_blind_falls_back_to_default() {
    let settings = PokerTableSettings::from_json(&serde_json::json!({
        "pace": "standard",
        "small_blind": 999
    }));

    assert_eq!(settings.small_blind(), 10);
    assert_eq!(settings.big_blind(), 20);
    assert_eq!(settings.starting_stack(), 1_000);
}

#[test]
fn invalid_values_fall_back_independently() {
    let settings = PokerTableSettings::from_json(&serde_json::json!({
        "pace": "typo",
        "small_blind": 50,
        "starting_stack": 123
    }));

    assert_eq!(settings.pace, PokerPace::Standard);
    assert_eq!(settings.small_blind(), 50);
    assert_eq!(settings.big_blind(), 100);
    assert_eq!(settings.starting_stack(), 1_000);
}

#[test]
fn labels_include_stack_and_blinds() {
    let settings = PokerTableSettings {
        pace: PokerPace::Quick,
        small_blind: 50,
        starting_stack: 5_000,
    };

    assert_eq!(settings.stake_label(), "5000 stack");
    assert_eq!(settings.blind_label(), "50/100 blinds");
    assert_eq!(settings.action_timeout_secs(), 20);
    assert_eq!(
        settings.meta_label(),
        "5000 stack · 50/100 blinds · 20s/turn"
    );
}
