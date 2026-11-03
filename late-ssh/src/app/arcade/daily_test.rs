use late_core::models::leaderboard::DailyPuzzle;

use super::activity_game;

/// The two maps between the rosters are written out by hand on both
/// sides; a daily that publishes as one game and is paid as another would
/// win and never be paid.
#[test]
fn every_daily_round_trips_through_its_activity_game() {
    for puzzle in DailyPuzzle::ALL {
        assert_eq!(activity_game(*puzzle).daily_puzzle(), Some(*puzzle));
        assert_eq!(activity_game(*puzzle).key(), puzzle.key());
    }
}
