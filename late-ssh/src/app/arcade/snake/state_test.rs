use chrono::Utc;
use late_core::db::{Db, DbConfig};
use late_core::models::snake::Game;
use uuid::Uuid;

use super::*;

fn saved_game(score: i32, is_game_over: bool) -> Game {
    let now = Utc::now();
    Game {
        id: Uuid::now_v7(),
        created: now,
        updated: now,
        user_id: Uuid::now_v7(),
        score,
        level: 3,
        lives: if is_game_over { 0 } else { 1 },
        is_game_over,
    }
}

fn restored(game: Game) -> State {
    let svc = SnakeService::new(Db::new(&DbConfig::default()).expect("test db pool"));
    State::restore(game.user_id, svc, 0, 25, 60, game)
}

/// A finished game recorded its final score when it ended. Submitting it a
/// second time on the next reset published a fresh score event today, which
/// completed score-based daily quests for a game the player never played.
///
/// The submission itself is a `tokio::spawn`, so a re-submission here (no
/// runtime) panics rather than quietly passing.
#[test]
fn restored_finished_game_does_not_resubmit_its_final_score() {
    let mut state = restored(saved_game(11_000, true));

    state.submit_final_score();

    assert_eq!(state.score, 11_000, "the finished board is still on screen");
}

