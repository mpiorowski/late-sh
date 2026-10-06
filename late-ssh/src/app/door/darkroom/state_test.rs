use chrono::Utc;
use late_core::models::darkroom_save::DarkroomSave;
use late_core::test_utils::create_test_user;
use uuid::Uuid;

use super::data::Resource;
use super::event::{self, Active};
use super::model::{Expedition, Game, View};
use super::space::{Flight, Space};
use super::state::{Ending, EndingBeat, Escape, Row, State};
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
        15_000,
        "the escape pays for the run that got out"
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
    let mut ending = Ending::for_run(&Game::new(false), Escape::Plain);

    assert_eq!(ending.revealed_count(), 1, "it opens on the first line");
    assert!(!ending.done());

    ending.reveal_all();
    assert_eq!(ending.revealed_count(), ending.beats().len());
    assert!(ending.done(), "a skipped reveal is a finished one");
}

/// Flying out holding the fleet beacon is a different ending, with different
/// words and a different badge. The two must never be confused: they are
/// separate payouts at separate prices, and an account can hold both.
#[test]
fn the_beacon_ending_says_something_else_and_pays_its_own_badge() {
    let plain = Ending::for_run(&Game::new(false), Escape::Plain);
    let beacon = Ending::for_run(&Game::new(false), Escape::WithBeacon);

    let opens_with = |ending: &Ending| match ending.beats().first() {
        Some(EndingBeat::Prose(line)) => (*line).to_string(),
        other => panic!("an ending opens on prose, got {other:?}"),
    };
    assert_eq!(opens_with(&plain), super::space::ENDING[0]);
    assert_eq!(opens_with(&beacon), super::space::BEACON_ENDING[0]);

    assert!(
        beacon
            .beats()
            .contains(&EndingBeat::Award(Escape::WithBeacon)),
        "the beacon run has to award the beacon badge"
    );
    assert!(plain.beats().contains(&EndingBeat::Award(Escape::Plain)));
    assert_eq!(Escape::Plain.award_category(), "darkroom_escape");
    assert_eq!(Escape::WithBeacon.award_category(), "darkroom_beacon");

    // And #lounge tells the two apart: flying out is the whole story for one
    // of them and only half of it for the other.
    assert_eq!(Escape::Plain.feed_detail(), None);
    assert_eq!(
        Escape::WithBeacon.feed_detail(),
        Some("followed the fleet beacon home")
    );

    // And they say different prices, because they pay different ones.
    assert_eq!(
        Escape::Plain.reward_line(),
        "15,000 chips, every run that gets out"
    );
    assert_eq!(
        Escape::WithBeacon.reward_line(),
        "20,000 chips, every run that gets out"
    );
}

/// The run id is what makes the ending's payout repeatable: it has to survive
/// a save/load, differ between runs, and appear on a blob written before the
/// field existed rather than reading as a nil id shared by every old save.
#[test]
fn a_run_carries_an_id_that_survives_a_save_and_never_repeats() {
    let first = Game::new(false);
    let second = Game::new(false);
    assert_ne!(first.run_id, second.run_id, "two runs, two ids");

    let round_tripped = super::persist::from_json(&super::persist::to_json(&first));
    assert_eq!(round_tripped.run_id, first.run_id, "a save keeps its run");

    // A blob written before the field existed: it comes back with an id of
    // its own, not a nil one.
    let mut blob = super::persist::to_json(&first);
    blob["game"]
        .as_object_mut()
        .expect("the game object")
        .remove("run_id");
    let upgraded = super::persist::from_json(&blob);
    assert_ne!(upgraded.run_id, uuid::Uuid::nil());
    assert_ne!(upgraded.run_id, first.run_id);
}

/// Swinging a weapon has to leave the cursor on that weapon.
///
/// Upstream is a page of buttons that do not move: using one greys it out for
/// its cooldown and leaves it exactly where it was. This used to snap the
/// cursor back to the top of the list on every press, so a fight with more
/// than one weapon meant scrolling back down after every single blow.
#[tokio::test]
async fn attacking_leaves_the_cursor_on_the_weapon_you_swung() {
    use super::world_data::Weapon;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-fight-cursor").await;
    let svc = darkroom_service(&test_db.db);

    let mut state = State::new(svc, user.id, Utc::now());
    load_game(&mut state).await;

    // Out in the wasteland carrying three weapons, so the row under the
    // cursor is not the first one.
    let mut trip = Expedition {
        hp: 500,
        water: 10,
        ..Expedition::default()
    };
    trip.add(Resource::BoneSpear, 1);
    trip.add(Resource::IronSword, 1);
    trip.add(Resource::SteelSword, 1);
    state.game_mut().expect("loaded").expedition = Some(trip);
    state.view = View::World;

    // The immortal wanderer: 500 health, so it survives being hit and the
    // fight stays on the same screen.
    let command = super::scenes_executioner::by_key("executioner-command").expect("the boss");
    let scene = command.scene("6").expect("the fight");
    state.event = Some(Active::resume(command, scene, 500));

    let steel = Row::Event(event::Row::Attack(Weapon::SteelSword));
    let index = state
        .rows()
        .iter()
        .position(|row| *row == steel)
        .expect("the steel sword is one of the rows");
    assert!(
        index > 0,
        "the test needs a row that is not already the first"
    );
    state.cursor = index;

    state.select();

    assert_eq!(
        state.selected(),
        steel,
        "the cursor jumped off the weapon that was just swung"
    );
    assert!(
        matches!(
            state.event.as_ref().map(|a| &a.phase),
            Some(event::Phase::Fighting(_))
        ),
        "the boss should still be standing, so the rows have not changed"
    );
}

/// The other half of the same rule: a press that genuinely replaces the rows
/// does put the cursor back at the top, because staying on index 2 of a list
/// that is now something else entirely is worse than starting over.
#[tokio::test]
async fn walking_into_a_new_scene_puts_the_cursor_back_at_the_top() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-scene-cursor").await;
    let svc = darkroom_service(&test_db.db);

    let mut state = State::new(svc, user.id, Utc::now());
    load_game(&mut state).await;
    state.game_mut().expect("loaded").expedition = Some(Expedition {
        hp: 100,
        water: 10,
        ..Expedition::default()
    });
    state.view = View::World;

    // The elevator bank, with a button per deck.
    let antechamber =
        super::scenes_executioner::by_key("executioner-antechamber").expect("the antechamber");
    let start = antechamber.scene("start").expect("the bank of elevators");
    state.event = Some(Active::resume(antechamber, start, 0));

    // Take the third elevator rather than the first.
    state.cursor = 2;
    assert_eq!(state.selected(), Row::Event(event::Row::Button(2)));

    state.select();

    assert_eq!(
        state.event.as_ref().map(|active| active.event.key),
        Some("executioner-martial"),
        "the third elevator opens the martial wing"
    );
    assert_eq!(
        state.cursor, 0,
        "a new wing is a new list of rows, so the cursor starts over"
    );
}
