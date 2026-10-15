use chrono::{Duration, NaiveDate, Utc};
use uuid::Uuid;

use super::{
    HitResult, LoungeDay, LoungeDice, Spawn, SpawnHit, SpawnKind, SpawnOutcome, Window,
    parse_spawn_payload,
};
use crate::test_utils::test_db;

const DICE: LoungeDice = LoungeDice {
    flicker_minutes: [120, 600, 1000],
    howler_minute: 777,
};

fn hit(username: &str) -> SpawnHit {
    SpawnHit {
        user_id: Uuid::now_v7(),
        username: username.to_string(),
        at_ms: 1_000,
    }
}

/// Each test gets a day of its own, so the day rows never collide across
/// tests sharing a database.
fn fresh_day() -> NaiveDate {
    let offset = (Uuid::now_v7().as_u128() % 20_000) as i64;
    NaiveDate::from_ymd_opt(2100, 1, 1).unwrap() + Duration::days(offset)
}

#[test]
fn a_window_is_eight_hours_of_the_day() {
    assert_eq!(Window::of_minute(0), Window::Early);
    assert_eq!(Window::of_minute(479), Window::Early);
    assert_eq!(Window::of_minute(480), Window::Mid);
    assert_eq!(Window::of_minute(959), Window::Mid);
    assert_eq!(Window::of_minute(960), Window::Late);
    assert_eq!(Window::of_minute(1439), Window::Late);
    assert_eq!(Window::Late.first_minute(), 960);
}

#[test]
fn a_spawn_survives_the_wire() {
    let spawn = Spawn {
        id: Uuid::now_v7(),
        kind: SpawnKind::Howler,
        hits_needed: 3,
        hits: vec![hit("tom")],
        spawned_at_ms: 1,
        expires_at_ms: 2,
        ended_at_ms: None,
        outcome: None,
    };
    assert_eq!(parse_spawn_payload(&spawn.to_payload()), Some(spawn));
    assert_eq!(parse_spawn_payload("{\"not\": \"a spawn\"}"), None);
}

#[tokio::test]
async fn the_first_touch_rolls_the_day_and_every_other_reads_it() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    let day = fresh_day();

    let rolled = LoungeDay::ensure(client, day, DICE).await.expect("ensure");
    assert_eq!(rolled.flicker_minutes, DICE.flicker_minutes);
    assert_eq!(rolled.howler_minute, DICE.howler_minute);
    assert_eq!(rolled.flickers_spent, [false; 3]);
    assert!(!rolled.howler_spent);

    let other_dice = LoungeDice {
        flicker_minutes: [1, 481, 961],
        howler_minute: 5,
    };
    let again = LoungeDay::ensure(client, day, other_dice)
        .await
        .expect("ensure again");
    assert_eq!(again, rolled);
}

#[tokio::test]
async fn a_flicker_is_spent_once_and_the_gap_holds() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    let day = fresh_day();
    LoungeDay::ensure(client, day, DICE).await.expect("ensure");
    let now = Utc::now();
    let gap = Duration::minutes(45);

    assert!(
        LoungeDay::claim_flicker(client, day, Window::Mid, now, now - gap)
            .await
            .expect("claim")
    );
    // The same window again: spent.
    assert!(
        !LoungeDay::claim_flicker(client, day, Window::Mid, now, now - gap)
            .await
            .expect("claim again")
    );
    // Another window a minute later: inside the gap.
    let soon = now + Duration::minutes(1);
    assert!(
        !LoungeDay::claim_flicker(client, day, Window::Late, soon, soon - gap)
            .await
            .expect("claim inside the gap")
    );
    assert!(
        !LoungeDay::claim_howler(client, day, soon, soon - gap)
            .await
            .expect("howler inside the gap")
    );
    // Past the gap: both go.
    let later = now + gap;
    assert!(
        LoungeDay::claim_flicker(client, day, Window::Late, later, later - gap)
            .await
            .expect("claim past the gap")
    );
    let latest = later + gap;
    assert!(
        LoungeDay::claim_howler(client, day, latest, latest - gap)
            .await
            .expect("howler past the gap")
    );
    let row = LoungeDay::ensure(client, day, DICE).await.expect("read");
    assert_eq!(row.flickers_spent, [false, true, true]);
    assert!(row.howler_spent);
    assert_eq!(row.last_spawn_at, Some(latest));
}

#[tokio::test]
async fn three_hits_from_three_runners_put_a_howler_down_and_one_pay_claim_wins() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    let now = Utc::now();
    let spawn = Spawn::insert(
        client,
        Uuid::now_v7(),
        SpawnKind::Howler,
        3,
        now,
        now + Duration::minutes(3),
    )
    .await
    .expect("insert");
    assert!(spawn.is_live());
    assert_eq!(spawn.hits_left(), 3);
    assert_eq!(
        Spawn::live(client).await.expect("live").as_ref(),
        Some(&spawn)
    );

    let tom = hit("tom");
    let HitResult::Landed(after_tom) = Spawn::hit(client, spawn.id, &tom, now)
        .await
        .expect("tom hits")
    else {
        panic!("tom's hit lands");
    };
    assert_eq!(after_tom.hits, vec![tom.clone()]);
    assert_eq!(after_tom.hits_left(), 2);
    assert!(after_tom.is_live());

    // One hit per runner per spawn.
    assert_eq!(
        Spawn::hit(client, spawn.id, &tom, now)
            .await
            .expect("tom again"),
        HitResult::AlreadyHit
    );

    let dax = hit("dax");
    let HitResult::Landed(after_dax) = Spawn::hit(client, spawn.id, &dax, now)
        .await
        .expect("dax hits")
    else {
        panic!("dax's hit lands");
    };
    assert!(after_dax.is_live());

    let mira = hit("mira");
    let HitResult::Landed(down) = Spawn::hit(client, spawn.id, &mira, now)
        .await
        .expect("mira hits")
    else {
        panic!("mira's hit lands");
    };
    assert_eq!(down.outcome, Some(SpawnOutcome::Slain));
    assert_eq!(down.ended_at_ms, Some(now.timestamp_millis()));
    assert_eq!(down.hits.len(), 3);
    assert!(down.hit_by(mira.user_id));

    // A fourth runner finds nothing to hit.
    assert_eq!(
        Spawn::hit(client, spawn.id, &hit("late"), now)
            .await
            .expect("late hit"),
        HitResult::Gone
    );
    assert_eq!(Spawn::live(client).await.expect("live"), None);

    // Exactly one caller pays.
    assert!(
        Spawn::claim_pay(client, spawn.id, now)
            .await
            .expect("claim")
            .is_some()
    );
    assert!(
        Spawn::claim_pay(client, spawn.id, now)
            .await
            .expect("claim again")
            .is_none()
    );
    assert!(
        Spawn::unpaid_slain(client, now)
            .await
            .expect("unpaid")
            .iter()
            .all(|unpaid| unpaid.id != spawn.id)
    );
}

#[tokio::test]
async fn one_live_spawn_at_a_time_and_a_slipped_one_is_ended_once() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    let now = Utc::now();
    let spawn = Spawn::insert(
        client,
        Uuid::now_v7(),
        SpawnKind::Flicker,
        1,
        now,
        now + Duration::seconds(90),
    )
    .await
    .expect("insert");

    assert!(
        Spawn::insert(
            client,
            Uuid::now_v7(),
            SpawnKind::Flicker,
            1,
            now,
            now + Duration::seconds(90),
        )
        .await
        .is_err(),
        "a second live spawn is refused"
    );

    // Not yet past its time: nothing ends.
    assert!(
        Spawn::end_expired(client, now + Duration::seconds(89))
            .await
            .expect("end early")
            .is_empty()
    );
    // A hit past its time misses.
    let late = now + Duration::seconds(90);
    assert_eq!(
        Spawn::hit(client, spawn.id, &hit("tom"), late)
            .await
            .expect("late hit"),
        HitResult::Gone
    );
    let ended = Spawn::end_expired(client, late).await.expect("end");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].id, spawn.id);
    assert_eq!(ended[0].outcome, Some(SpawnOutcome::Slipped));
    assert!(
        Spawn::end_expired(client, late)
            .await
            .expect("end again")
            .is_empty()
    );
    // A slipped spawn is never pay.
    assert!(
        Spawn::claim_pay(client, spawn.id, late)
            .await
            .expect("claim")
            .is_none()
    );
    // And the way is clear for the next one.
    Spawn::insert(
        client,
        Uuid::now_v7(),
        SpawnKind::Flicker,
        1,
        late,
        late + Duration::seconds(90),
    )
    .await
    .expect("the next spawn");
}

#[tokio::test]
async fn a_new_day_starts_its_gap_from_the_newest_spawn() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    // A spawn late on one day; the next day's row has to know about it,
    // or a flicker rolled for 00:05 lands five minutes after it.
    let day = fresh_day();
    let late = day.and_hms_opt(23, 50, 0).unwrap().and_utc();
    Spawn::insert(
        client,
        Uuid::now_v7(),
        SpawnKind::Flicker,
        1,
        late,
        late + Duration::seconds(90),
    )
    .await
    .expect("insert");
    Spawn::end_expired(client, late + Duration::minutes(2))
        .await
        .expect("end");

    let tomorrow = LoungeDay::ensure(client, day + Duration::days(1), DICE)
        .await
        .expect("ensure tomorrow");
    assert_eq!(tomorrow.last_spawn_at, Some(late));
    let soon = late + Duration::minutes(5);
    assert!(
        !LoungeDay::claim_flicker(
            client,
            tomorrow.day,
            Window::Early,
            soon,
            soon - Duration::minutes(45)
        )
        .await
        .expect("claim inside the gap")
    );
}
