use late_core::models::aquarium_care::{AquariumCare, CARE_DAYS};
use late_core::models::chips::UserChips;
use late_core::models::marketplace::{
    AQUARIUM_MAX_FISH, AQUARIUM_MAX_PLANTS, AQUARIUM_PLANT_ITEM_KIND, AQUARIUM_SKU,
    purchase_durable_item_by_sku,
};
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::{AquariumService, CutOutcome, FEED_CHIP_BONUS, SproutFate};
use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::test_helpers::new_test_db;

const AQUARIUM_PRICE: i64 = 10_000;
const CLOWNFISH_PRICE: i64 = 1_000;

fn service(db: &late_core::db::Db) -> (AquariumService, broadcast::Receiver<ActivityEvent>) {
    let (tx, rx) = broadcast::channel::<ActivityEvent>(16);
    (AquariumService::new(db.clone(), tx), rx)
}

/// A tank with two clownfish swimming, bought the way a player buys them.
async fn stock_tank(db: &late_core::db::Db, user_id: Uuid) {
    let mut client = db.get().await.expect("db client");
    UserChips::admin_grant(&**client, user_id, AQUARIUM_PRICE + CLOWNFISH_PRICE * 2)
        .await
        .expect("fund chips");
    purchase_durable_item_by_sku(&mut client, user_id, AQUARIUM_SKU)
        .await
        .expect("aquarium purchase");
    // The tank came with a welcome fry; park it in inventory so only the
    // clownfish swim and every roll below lands on them.
    let welcome = AquariumCare::load(&**client, user_id)
        .await
        .expect("care")
        .expect("the purchase planted a care row")
        .fry_creature
        .expect("the purchase stamped a welcome fry");
    late_core::models::marketplace::adjust_aquarium_active_by_sku(
        &mut client,
        user_id,
        &format!("aquarium_fish_{welcome}"),
        -1,
    )
    .await
    .expect("park the welcome fry");
    for _ in 0..2 {
        purchase_durable_item_by_sku(&mut client, user_id, "aquarium_fish_clownfish")
            .await
            .expect("fish purchase");
    }
    late_core::models::marketplace::adjust_aquarium_active_by_sku(
        &mut client,
        user_id,
        "aquarium_fish_clownfish",
        2,
    )
    .await
    .expect("put the fish in the water");
}

/// Every plant the user owns, summed over the species: `(owned, in the
/// water)`. What a rooting sprout adds to, whichever plant it picked.
async fn plant_counts(db: &late_core::db::Db, user_id: Uuid) -> (i32, i32) {
    let client = db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT COALESCE(SUM(p.quantity), 0)::INT AS quantity,
                    COALESCE(SUM(p.active_quantity), 0)::INT AS active_quantity
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.item_kind = $2",
            &[&user_id, &AQUARIUM_PLANT_ITEM_KIND],
        )
        .await
        .expect("plant rows");
    (row.get("quantity"), row.get("active_quantity"))
}

async fn clownfish_counts(db: &late_core::db::Db, user_id: Uuid) -> (i32, i32) {
    sku_counts(db, user_id, "aquarium_fish_clownfish")
        .await
        .expect("clownfish row")
}

/// Owned and swimming counts of one SKU, `None` when the user has no row.
async fn sku_counts(db: &late_core::db::Db, user_id: Uuid, sku: &str) -> Option<(i32, i32)> {
    let client = db.get().await.expect("db client");
    client
        .query_opt(
            "SELECT p.quantity, p.active_quantity
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.sku = $2",
            &[&user_id, &sku],
        )
        .await
        .expect("purchase row")
        .map(|row| (row.get("quantity"), row.get("active_quantity")))
}

#[tokio::test]
async fn feeding_pays_the_daily_chips_once_and_only_a_tank_owner() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "aquarium-svc-feed").await;
    let (svc, mut rx) = service(&test_db.db);
    let boot = svc.bootstrap(user.id).await.expect("bootstrap");
    assert_eq!(boot.care, None, "no tank, no clock");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    // No tank: nothing to feed, nothing paid, no clock started.
    svc.feed(user.id).await.expect("feed without a tank");
    let unpaid = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(unpaid, before, "a user without a tank earns nothing");
    assert_eq!(
        AquariumCare::load(&**client, user.id).await.expect("care"),
        None,
        "feeding without a tank starts no clock"
    );
    assert!(rx.try_recv().is_err(), "and announces nothing");

    stock_tank(&test_db.db, user.id).await;
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    svc.feed(user.id).await.expect("first feed");
    svc.feed(user.id).await.expect("second feed");

    let after = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(after - before, FEED_CHIP_BONUS);
    let care = AquariumCare::load(&**client, user.id)
        .await
        .expect("care")
        .expect("care row");
    assert_eq!(care.last_fed.date_naive(), chrono::Utc::now().date_naive());
    assert_eq!(care.streak, 1);

    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert_eq!(event.user_id, Some(user.id));
    assert!(matches!(event.kind, ActivityKind::AquariumFed));
    assert!(
        rx.try_recv().is_err(),
        "the same-day second feed announces nothing"
    );
}

#[tokio::test]
async fn the_fourteenth_straight_feed_hatches_a_fry() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-svc-fry").await;
    stock_tank(&test_db.db, user.id).await;
    let (svc, mut rx) = service(&test_db.db);
    let client = test_db.db.get().await.expect("db client");
    // Thirteen days fed, the last one yesterday: the care row is the table
    // under test, so it is set up directly.
    svc.feed(user.id).await.expect("seed the row");
    client
        .execute(
            "UPDATE user_aquarium_care
             SET last_fed = current_timestamp - interval '1 day', streak = $2
             WHERE user_id = $1",
            &[&user.id, &(CARE_DAYS as i32 - 1)],
        )
        .await
        .expect("rewind a day");

    svc.feed(user.id).await.expect("fourteenth feed");

    assert_eq!(clownfish_counts(&test_db.db, user.id).await, (3, 3));
    let care = AquariumCare::load(&**client, user.id)
        .await
        .expect("care")
        .expect("care row");
    assert_eq!(care.streak, 14);
    assert_eq!(care.fry_creature.as_deref(), Some("clownfish"));
    assert_eq!(care.fry_born, Some(chrono::Utc::now().date_naive()));

    let mut kinds = Vec::new();
    for _ in 0..3 {
        let event = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("activity in time")
            .expect("activity event");
        kinds.push(event.kind);
    }
    assert!(matches!(kinds[0], ActivityKind::AquariumFed));
    assert!(matches!(kinds[1], ActivityKind::AquariumFed));
    assert!(matches!(
        &kinds[2],
        ActivityKind::AquariumFryHatched { creature, swimming: true } if creature == "clownfish"
    ));
}

#[tokio::test]
async fn fourteen_unfed_days_starve_one_fish_at_login_and_only_once() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-svc-starve").await;
    stock_tank(&test_db.db, user.id).await;
    let (svc, mut rx) = service(&test_db.db);
    let client = test_db.db.get().await.expect("db client");

    // The first connect of a tank owner who never fed starts the clock.
    let boot = svc.bootstrap(user.id).await.expect("first bootstrap");
    assert!(boot.lost.is_empty());
    let care = boot.care.expect("the owner has a clock now");
    assert_eq!(care.deaths_settled, 0);

    // Two weeks pass without a meal.
    client
        .execute(
            "UPDATE user_aquarium_care
             SET last_fed = current_timestamp - interval '14 days'
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("rewind two weeks");

    let boot = svc.bootstrap(user.id).await.expect("second bootstrap");
    assert_eq!(boot.lost, vec!["clownfish".to_string()]);
    assert_eq!(boot.care.expect("care").deaths_settled, 1);
    assert_eq!(clownfish_counts(&test_db.db, user.id).await, (1, 1));
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert!(matches!(
        &event.kind,
        ActivityKind::AquariumFishLost { creature } if creature == "clownfish"
    ));

    // The same day again, another device: settled already, nothing dies.
    let boot = svc.bootstrap(user.id).await.expect("third bootstrap");
    assert!(boot.lost.is_empty());
    assert_eq!(clownfish_counts(&test_db.db, user.id).await, (1, 1));
    assert!(rx.try_recv().is_err());

    // A meal closes the account: the next death is fourteen days away.
    svc.feed(user.id).await.expect("feed");
    let care = AquariumCare::load(&**client, user.id)
        .await
        .expect("care")
        .expect("care row");
    assert_eq!(care.deaths_settled, 0);
}

#[tokio::test]
async fn a_sprout_comes_up_every_two_weeks_and_roots_unless_it_is_cut() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-svc-sprout").await;
    let (svc, mut rx) = service(&test_db.db);
    let client = test_db.db.get().await.expect("db client");

    // No tank: nothing to cut, no clock.
    assert_eq!(
        svc.cut(user.id).await.expect("cut without a tank"),
        CutOutcome::NoTank
    );
    stock_tank(&test_db.db, user.id).await;

    // The purchase planted the first sprout: the first connect finds it up
    // already (nothing new to announce) with the next booked two weeks out.
    let today = chrono::Utc::now().date_naive();
    let boot = svc.bootstrap(user.id).await.expect("first bootstrap");
    assert!(!boot.sprouted, "up since the purchase, not raised now");
    assert_eq!(boot.rooted, None);
    let care = boot.care.expect("care");
    assert_eq!(care.sprout_born, Some(today));
    assert_eq!(care.next_sprout, today + chrono::Days::new(14));
    assert!(rx.try_recv().is_err(), "nothing to announce yet");

    // Cut it: gone, announced once, and a second press finds nothing.
    assert_eq!(svc.cut(user.id).await.expect("cut"), CutOutcome::Cut);
    assert_eq!(
        svc.cut(user.id).await.expect("cut again"),
        CutOutcome::NothingToCut
    );
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert!(matches!(event.kind, ActivityKind::AquariumSproutCut));
    assert!(rx.try_recv().is_err());
    assert_eq!(
        plant_counts(&test_db.db, user.id).await,
        (0, 0),
        "a cut sprout grows nothing"
    );

    // Two weeks pass: the next connect raises the next one.
    client
        .execute(
            "UPDATE user_aquarium_care SET next_sprout = current_date WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("bring the sprout forward");
    let boot = svc.bootstrap(user.id).await.expect("second bootstrap");
    assert!(boot.sprouted);
    assert_eq!(boot.rooted, None);
    assert_eq!(boot.care.expect("care").sprout_born, Some(today));
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert!(matches!(event.kind, ActivityKind::AquariumSprouted { born } if born == today));

    // The next one, left alone for a week: it roots as one of the
    // catalog's plants, in the water, and a late cut finds a plant, not a
    // sprout. The fish are untouched: a plant takes a plant place.
    client
        .execute(
            "UPDATE user_aquarium_care
             SET sprout_born = current_date - 7, next_sprout = current_date + 7
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("an old sprout");
    assert_eq!(
        svc.cut(user.id).await.expect("late cut"),
        CutOutcome::NothingToCut
    );
    let boot = svc.bootstrap(user.id).await.expect("third bootstrap");
    let Some(SproutFate::Rooted { creature, swimming }) = boot.rooted else {
        panic!("the sprout rooted, got {:?}", boot.rooted);
    };
    assert!(swimming);
    assert!(
        ["seatuft", "wigglewort"].contains(&creature.as_str()),
        "roots as a catalog plant, got {creature}"
    );
    assert!(!boot.sprouted, "the next is still a week away");
    assert_eq!(boot.care.expect("care").sprout_born, None);
    assert_eq!(plant_counts(&test_db.db, user.id).await, (1, 1));
    assert_eq!(
        sku_counts(&test_db.db, user.id, &format!("aquarium_plant_{creature}")).await,
        Some((1, 1))
    );
    assert_eq!(clownfish_counts(&test_db.db, user.id).await, (2, 2));
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert!(matches!(
        &event.kind,
        ActivityKind::AquariumSproutRooted { creature: rooted, swimming: true } if *rooted == creature
    ));

    // Rooting again on the same day finds nothing: settled once.
    let boot = svc.bootstrap(user.id).await.expect("fourth bootstrap");
    assert_eq!(boot.rooted, None);
    assert_eq!(plant_counts(&test_db.db, user.id).await, (1, 1));
}

/// The owned caps stop what the tank grows on its own: at twenty plants a
/// sprout left alone withers instead of rooting, and at twenty fish the
/// fourteenth feed pays its chips but hatches nothing.
#[tokio::test]
async fn at_twenty_owned_a_sprout_withers_and_no_fry_is_born() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-svc-owned-cap").await;
    stock_tank(&test_db.db, user.id).await;
    let (svc, mut rx) = service(&test_db.db);
    let mut client = test_db.db.get().await.expect("db client");
    let seatuft_price = 1_000;
    UserChips::admin_grant(
        &**client,
        user.id,
        seatuft_price * AQUARIUM_MAX_PLANTS as i64
            + CLOWNFISH_PRICE * (AQUARIUM_MAX_FISH as i64 - 3),
    )
    .await
    .expect("fund chips");
    for _ in 0..AQUARIUM_MAX_PLANTS {
        purchase_durable_item_by_sku(&mut client, user.id, "aquarium_plant_seatuft")
            .await
            .expect("plant purchase");
    }
    // The fry and two clownfish are three; seventeen more make twenty.
    for _ in 0..AQUARIUM_MAX_FISH - 3 {
        purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_clownfish")
            .await
            .expect("fish purchase");
    }
    assert_eq!(plant_counts(&test_db.db, user.id).await, (20, 0));
    assert_eq!(clownfish_counts(&test_db.db, user.id).await, (19, 2));

    // A sprout past its week finds no room and withers.
    client
        .execute(
            "UPDATE user_aquarium_care
             SET sprout_born = current_date - 7, next_sprout = current_date + 7
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("an old sprout");
    let boot = svc.bootstrap(user.id).await.expect("bootstrap");
    assert_eq!(boot.rooted, Some(SproutFate::Withered));
    assert_eq!(
        boot.care.expect("care").sprout_born,
        None,
        "the floor is bare"
    );
    assert_eq!(plant_counts(&test_db.db, user.id).await, (20, 0));
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("activity in time")
        .expect("activity event");
    assert!(matches!(event.kind, ActivityKind::AquariumSproutWithered));

    // The fourteenth straight feed: chips paid, streak counted, no fry.
    svc.feed(user.id).await.expect("seed the row");
    client
        .execute(
            "UPDATE user_aquarium_care
             SET last_fed = current_timestamp - interval '1 day', streak = $2
             WHERE user_id = $1",
            &[&user.id, &(CARE_DAYS as i32 - 1)],
        )
        .await
        .expect("rewind a day");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    svc.feed(user.id).await.expect("fourteenth feed");
    let care = AquariumCare::load(&**client, user.id)
        .await
        .expect("care")
        .expect("care row");
    assert_eq!(care.streak, 14);
    assert_eq!(
        care.fry_creature.as_deref(),
        Some("fry"),
        "still the welcome fry"
    );
    assert_eq!(clownfish_counts(&test_db.db, user.id).await, (19, 2));
    assert_eq!(
        UserChips::ensure(&client, user.id)
            .await
            .expect("chips")
            .balance,
        before + FEED_CHIP_BONUS
    );
    for _ in 0..2 {
        let event = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("activity in time")
            .expect("activity event");
        assert!(matches!(event.kind, ActivityKind::AquariumFed));
    }
    assert!(
        timeout(Duration::from_millis(200), rx.recv())
            .await
            .is_err(),
        "no hatch event follows"
    );
}
