use super::*;

#[test]
fn daily_puzzle_reward_game_accepts_only_daily_puzzle_games() {
    assert_eq!(
        daily_puzzle_reward_game(ActivityGame::LeWord),
        Some(DailyPuzzleRewardGame::LeWord)
    );
    assert_eq!(
        daily_puzzle_reward_game(ActivityGame::Minesweeper),
        Some(DailyPuzzleRewardGame::Minesweeper)
    );
    assert_eq!(
        daily_puzzle_reward_game(ActivityGame::Sudoku),
        Some(DailyPuzzleRewardGame::Sudoku)
    );
    assert_eq!(
        daily_puzzle_reward_game(ActivityGame::RubiksCube),
        Some(DailyPuzzleRewardGame::RubiksCube)
    );
    assert_eq!(daily_puzzle_reward_game(ActivityGame::Lateris), None);
    assert_eq!(daily_puzzle_reward_game(ActivityGame::Blackjack), None);
}
