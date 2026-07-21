use crate::models::reward::*;

#[test]
fn daily_puzzle_reward_key_uses_typed_game_and_normalized_difficulty() {
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzleRewardGame::Solitaire, "draw-3"),
        "solitaire_daily_draw_3_win"
    );
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzleRewardGame::LeWord, "daily"),
        "le_word_daily_daily_win"
    );
    assert_eq!(
        daily_puzzle_reward_key(DailyPuzzleRewardGame::RubiksCube, "daily"),
        "rubiks_cube_daily_daily_win"
    );
}
