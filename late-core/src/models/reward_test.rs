use crate::models::leaderboard::DailyPuzzle;
use crate::models::reward::*;
use crate::test_utils::test_db;

#[test]
fn daily_puzzle_reward_key_uses_typed_game_and_normalized_difficulty() {
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzle::Solitaire, "draw-3"),
        "solitaire_daily_draw_3_win"
    );
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzle::LeWord, "daily"),
        "le_word_daily_daily_win"
    );
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzle::RubiksCube, "daily"),
        "rubiks_cube_daily_daily_win"
    );
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzle::SlidingPuzzle, "hard"),
        "sliding_puzzle_daily_hard_win"
    );
}

/// The reward rows are migration data the compiler cannot see: every board
/// the roster says a daily pays needs an active template, or the win pays
/// nothing, and the template must pay what the lobby card and the guide
/// show, which they read from the tier.
#[tokio::test]
async fn every_daily_tier_has_an_active_template_paying_its_tier() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    for puzzle in DailyPuzzle::ALL {
        for (difficulty_key, tier) in puzzle.tiers().list() {
            let key = daily_puzzle_reward_key(*puzzle, difficulty_key);
            let template = RewardTemplate::get_active_by_key(&**client, &key)
                .await
                .unwrap_or_else(|error| panic!("{puzzle:?} {difficulty_key}: {error:#}"));
            assert_eq!(
                template.reward_chips,
                tier.chips(),
                "{key} pays what the {puzzle:?} card shows"
            );
            assert_eq!(template.claim_policy, REWARD_CLAIM_POLICY_UTC_DAY, "{key}");
        }
    }
}
