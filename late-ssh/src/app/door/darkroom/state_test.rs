use chrono::Utc;
use late_core::models::darkroom_save::DarkroomSave;
use late_core::test_utils::create_test_user;
use uuid::Uuid;

use super::model::Game;
use super::space::{Flight, Space};
use super::state::{Ending, EndingBeat, Row, State};
use super::svc::DarkroomService;
use crate::app::activity::publisher::ActivityPublisher;
use crate::app::games::chips::svc::ChipService;
use crate::test_helpers::{new_test_db, wait_until};

fn darkroom_service(db: &late_core::db::Db) -> DarkroomService {
    let (activity_tx, _rx) = crate::app::activity::channel::new(64);
    DarkroomService::new(
        ActivityPublisher::new(db.clone(), activity_tx),
        ChipService::new(db.clone()),
        db.clone(),
    )
}

/// Drive the state the way the render loop does, rather than through
/// `wait_until`, which cannot hold the mutable borrow across its async
/// closure.
async fn load_game(state: &mut State) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while state.game().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the darkroom save to load"
        );
        state.tick();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn saved(db: &late_core::db::Db, user_id: Uuid) -> bool {
    let client = db.get().await.expect("db client");
    DarkroomSave::load(&client, user_id)
        .await
        .expect("load save")
        .is_some()
}

async fn badge_count(db: &late_core::db::Db, user_id: Uuid, category: &str) -> i64 {
    let client = db.get().await.expect("db client");
    client
        .query_one(
            "SELECT COUNT(*)::bigint AS n FROM profile_awards
             WHERE user_id = $1 AND category = $2",
            &[&user_id, &category],
        )
        .await
        .expect("badge count")
        .get("n")
}

async fn payout_total(db: &late_core::db::Db, user_id: Uuid) -> i64 {
    let client = db.get().await.expect("db client");
    client
        .query_one(
            "SELECT COALESCE(SUM(amount), 0)::bigint AS total
             FROM game_payout_claims
             WHERE user_id = $1 AND game = 'darkroom'",
            &[&user_id],
        )
        .await
        .expect("payout total")
        .get("total")
}

/// The load channel's sender is dropped as soon as the loader has sent, so a
/// receiver that gates on `has_changed()` sees `Err` and never picks the game
/// up — the session sits on "the dark is quiet..." forever. `tick` must read
/// the value instead.
#[tokio::test]
async fn a_session_picks_up_its_save_and_offers_the_fire() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-load").await;
    let svc = darkroom_service(&test_db.db);

    let mut state = State::new(svc, user.id, Utc::now());
    assert!(state.game().is_none(), "the load starts in flight");
    load_game(&mut state).await;

    // A fresh save is a dead fire in a dark room, so the only thing to do is
    // light it.
    assert_eq!(state.rows(), vec![Row::LightFire, Row::Leave]);
}

/// The one ending: the epitaph takes the screen, the account keeps the badge
/// and the chips, and the save is deleted so the next visit starts over. The
/// last part is what leaving must not undo.
#[tokio::test]
async fn winning_the_ascent_wipes_the_save_and_pays_the_badge() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-ending").await;
    let svc = darkroom_service(&test_db.db);

    let mut state = State::new(svc, user.id, Utc::now());
    load_game(&mut state).await;
    // Lighting the fire is the first thing that writes a save, so there is
    // something for the ending to wipe.
    state.select();
    let db = test_db.db.clone();
    wait_until(|| async { saved(&db, user.id).await }, "save written").await;

    // The ship is through the debris cloud.
    let mut flight = Space::new(1, 1);
    flight.outcome = Some(Flight::Won);
    state.flight = Some(flight);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    state.tick();

    assert!(state.flight.is_none(), "the flight is over");
    let ending = state.ending.as_ref().expect("the ending is up");
    assert!(!ending.done(), "the epitaph arrives a beat at a time");
    assert_eq!(
        ending.beats().last(),
        Some(&EndingBeat::Prompt),
        "the last thing it says is the way out"
    );

    let db = test_db.db.clone();
    wait_until(|| async { !saved(&db, user.id).await }, "save wiped").await;
    let db = test_db.db.clone();
    wait_until(
        || async { badge_count(&db, user.id, "darkroom_escape").await == 1 },
        "escape badge granted",
    )
    .await;
    assert_eq!(
        payout_total(&test_db.db, user.id).await,
        10_000,
        "the escape pays once per account"
    );

    // Stepping out of the door must not write the finished run back over the
    // wipe: the room is dark again, and it stays that way.
    state.save_on_leave();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !saved(&test_db.db, user.id).await,
        "leaving after the ending resurrected the save"
    );
}

/// The epitaph reveals itself one beat at a time, and a key press skips the
/// wait: an ending nobody can hurry is an ending people kill their terminal on.
#[test]
fn the_ending_reveals_a_beat_at_a_time_until_a_key_skips_it() {
    let mut ending = Ending::for_run(&Game::new());

    assert_eq!(ending.revealed_count(), 1, "it opens on the first line");
    assert!(!ending.done());

    ending.reveal_all();
    assert_eq!(ending.revealed_count(), ending.beats().len());
    assert!(ending.done(), "a skipped reveal is a finished one");
}
