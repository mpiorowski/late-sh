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

/// A cobra on an open field, laid out tail first, already heading `dir`.
fn cobra_on_field(body: &[(u32, u32)], dir: Direction) -> (Cobra, Field) {
    let field = Field::new_empty(10, 10);
    let mut cobra = Cobra::new(&field, 1);
    cobra.body = body.iter().map(|&(x, y)| Position { x, y }).collect();
    cobra.head_dir = dir;
    (cobra, field)
}

fn cells(cobra: &Cobra) -> Vec<(u32, u32)> {
    cobra.body.iter().map(|part| (part.x, part.y)).collect()
}

/// A cobra curled into a ring chases its own tail: the head steps into the
/// cell the tail leaves on that same step, which is free, not a collision.
/// The run used to end there with nothing in the way (the tail is drawn
/// under the head once the head lands on it).
#[test]
fn the_head_may_follow_the_tail_into_the_cell_it_leaves() {
    let ring = [(5, 2), (4, 2), (4, 3), (4, 4), (5, 4), (5, 3)];
    let (mut cobra, mut field) = cobra_on_field(&ring, Direction::Up);

    let effect = cobra.move_cobra(&mut field);

    assert!(effect.is_none(), "an empty cell has no effect");
    assert!(matches!(cobra.state, CobraState::Alive));
    assert_eq!(
        cells(&cobra),
        vec![(4, 2), (4, 3), (4, 4), (5, 4), (5, 3), (5, 2)]
    );
}

/// Any other part of the body is still in the way.
#[test]
fn the_head_running_into_the_body_ends_the_run() {
    let ring = [(5, 2), (4, 2), (4, 3), (4, 4), (5, 4), (5, 3)];
    let (mut cobra, mut field) = cobra_on_field(&ring, Direction::Left);

    let effect = cobra.move_cobra(&mut field);

    assert!(matches!(effect, Some(CobraEffect::Blow)));
    assert!(matches!(cobra.state, CobraState::Dead));
}
