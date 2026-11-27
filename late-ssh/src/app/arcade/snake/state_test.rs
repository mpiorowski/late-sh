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

/// The cells no rock may spawn on, worked out from the head alone: the square
/// two cells out on every side, and the four cells straight ahead.
fn cells_kept_clear(head: &Position, dir: Direction) -> Vec<(i64, i64)> {
    let (head_x, head_y) = (i64::from(head.x), i64::from(head.y));
    let mut clear = Vec::new();
    for dx in -2..=2 {
        for dy in -2..=2 {
            clear.push((head_x + dx, head_y + dy));
        }
    }
    let (step_x, step_y) = match dir {
        Direction::Up => (0, -1),
        Direction::Down => (0, 1),
        Direction::Right => (1, 0),
        Direction::Left => (-1, 0),
    };
    for step in 1..=4 {
        clear.push((head_x + step_x * step, head_y + step_y * step));
    }
    clear
}

#[test]
fn rock_spawns_keep_clear_of_the_head_and_forward_runway() {
    for dir in [
        Direction::Right,
        Direction::Left,
        Direction::Down,
        Direction::Up,
    ] {
        let mut field = Field::new_empty(22, 58);
        let mut cobra = Cobra::new(&field, 1);
        cobra.head_dir = dir;
        let head = cobra.body.last().expect("cobra head").clone();
        let clear = cells_kept_clear(&head, dir);
        for _ in 0..50 {
            field.things.clear();
            field.gen_things(10, &cobra);
            for thing in &field.things {
                if matches!(thing.kind, ThingKind::Rock) {
                    assert!(
                        !cobra.collide(&thing.position),
                        "rock spawned on cobra body at {:?}",
                        thing.position
                    );
                    let cell = (i64::from(thing.position.x), i64::from(thing.position.y));
                    assert!(
                        !clear.contains(&cell),
                        "rock spawned inside head halo or forward runway at {:?} facing {:?}",
                        thing.position,
                        dir
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn eating_the_last_food_advances_the_level_on_that_step() {
    let mut game = saved_game(0, false);
    game.level = 1;
    let mut state = restored(game);
    let head = state.cobra.body.last().expect("cobra head").clone();
    state.cobra.head_dir = Direction::Right;
    state.field.things = vec![ThingOnScreen::from_kind_at_pos(
        ThingKind::Food,
        Position {
            x: head.x + 1,
            y: head.y,
        },
    )];
    state.stutter_left = 0;

    assert!(state.tick());
    assert_eq!(state.level.number, 2);
    assert_eq!(state.field.food_left(), 4);
    assert_eq!(state.score, 210);
}

/// A turn still waiting in the queue would steer the cobra, and skip the
/// stutter, on the first tick of the new level.
#[test]
fn level_reset_drops_buffered_inputs() {
    let mut state = restored(saved_game(0, false));
    state.cobra.head_dir = Direction::Right;
    state.input_queue = vec![Direction::Left, Direction::Up];
    state.stutter_left = 3;

    state.reset_level_without_persist(false);

    assert!(!state.tick());
    assert_eq!(state.stutter_left, 2);
    assert_eq!(state.cobra.head_dir, Direction::Right);
}
