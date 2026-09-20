use chrono::{NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::{
    models::{
        aquarium_care::{
            AquariumCare, CARE_DAYS, SPROUT_DAYS, SPROUT_EVERY_DAYS, deaths_due, dry_days,
            fish_weight, hatches_fry, pick_by_weight, sprout_rooted, streak_continues_from,
        },
        aquarium_shield::AquariumShield,
        marketplace::FishStock,
    },
    test_utils::{create_test_user, test_db},
};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
}

fn shield(from: u32, to: u32) -> AquariumShield {
    AquariumShield {
        starts_at: Utc.with_ymd_and_hms(2026, 9, from, 12, 0, 0).unwrap(),
        ends_at: Utc.with_ymd_and_hms(2026, 9, to, 12, 0, 0).unwrap(),
    }
}

#[test]
fn dry_days_count_every_unfed_day_no_shield_covered() {
    // Fed on the 1st, looking on the 15th: fourteen unfed days, one death.
    assert_eq!(dry_days(day(1), day(15), &[]), 14);
    assert_eq!(deaths_due(14), 1);
    assert_eq!(deaths_due(13), 0);
    assert_eq!(deaths_due(28), 2);
    // Fed today or in the future: nothing on the clock.
    assert_eq!(dry_days(day(15), day(15), &[]), 0);

    // A shield over the 5th through the 10th excuses those six days, and it
    // still counts after it has lapsed: the 15th is past its end.
    assert_eq!(dry_days(day(1), day(15), &[shield(5, 10)]), 8);

    // A second shield bought after the first lapsed is its own window. Both
    // excuse their days: without the union, buying the second one would
    // put the first one's six days back on the clock and cost a fish.
    let both = [shield(13, 27), shield(5, 10)];
    assert_eq!(dry_days(day(1), day(30), &both), 8);
    assert_eq!(deaths_due(dry_days(day(1), day(30), &both)), 0);
    assert_eq!(
        deaths_due(dry_days(day(1), day(30), &[shield(13, 27)])),
        1,
        "the newest window alone would starve a fish"
    );
}

#[test]
fn the_streak_continues_across_the_days_a_shield_covered() {
    // No shield: the last meal must have been yesterday.
    assert_eq!(streak_continues_from(day(15), &[]), day(14));
    // A shield over the 10th through the 14th: a meal on the 9th still
    // continues the streak on the 15th, the minded days neither grow nor
    // break it.
    assert_eq!(streak_continues_from(day(15), &[shield(10, 14)]), day(9));
    // Two windows back to back are walked through as one.
    assert_eq!(
        streak_continues_from(day(15), &[shield(12, 14), shield(8, 11)]),
        day(7)
    );
    // A shield that does not reach yesterday changes nothing.
    assert_eq!(streak_continues_from(day(15), &[shield(5, 10)]), day(14));
}

#[test]
fn a_fry_hatches_every_fourteen_straight_days() {
    assert!(!hatches_fry(0));
    assert!(!hatches_fry(13));
    assert!(hatches_fry(CARE_DAYS as i32));
    assert!(!hatches_fry(15));
    assert!(hatches_fry(28));
}

fn stock(name: &str, price_chips: i64, active_quantity: i32) -> FishStock {
    FishStock {
        item_id: Uuid::now_v7(),
        creature: name.to_string(),
        name: name.to_string(),
        price_chips,
        quantity: active_quantity,
        active_quantity,
    }
}

#[test]
fn cheap_fish_weigh_more_than_dear_ones() {
    assert_eq!(fish_weight(1_000), 10);
    assert_eq!(fish_weight(2_500), 4);
    assert_eq!(fish_weight(5_000), 2);
    assert_eq!(fish_weight(10_000), 1);
    // Anything dearer than the reference still weighs one, never zero.
    assert_eq!(fish_weight(50_000), 1);

    // Two clownfish (20), two squigs (8), two Bigberts (2): thirty tickets.
    let tank = vec![
        stock("clownfish", 1_000, 2),
        stock("squigs", 2_500, 2),
        stock("bigbert", 10_000, 2),
    ];
    let picks: Vec<&str> = (0..30)
        .map(|roll| pick_by_weight(&tank, roll).unwrap().creature.as_str())
        .collect();
    assert_eq!(picks.iter().filter(|c| **c == "clownfish").count(), 20);
    assert_eq!(picks.iter().filter(|c| **c == "squigs").count(), 8);
    assert_eq!(picks.iter().filter(|c| **c == "bigbert").count(), 2);
    // The roll wraps, so any random number lands on a fish.
    assert_eq!(pick_by_weight(&tank, 30).unwrap().creature, "clownfish");

    // Inventory fish are not in the water: no tickets.
    let shelf = vec![stock("clownfish", 1_000, 0)];
    assert!(pick_by_weight(&shelf, 7).is_none());
    assert!(pick_by_weight(&[], 7).is_none());
}

#[tokio::test]
async fn feed_day_runs_the_streak_and_closes_the_starvation_account() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-care-streak").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();
    let tomorrow = today.succ_opt().unwrap();
    let later = tomorrow.succ_opt().unwrap().succ_opt().unwrap();

    assert_eq!(AquariumCare::load(&**client, user.id).await.unwrap(), None);

    // First meal ever: streak one. The same day again: no witness.
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, today, today.pred_opt().unwrap())
            .await
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, today, today.pred_opt().unwrap())
            .await
            .unwrap(),
        None
    );
    // The row stamps `last_fed` with the clock, so "tomorrow" is the day
    // after that stamp: the streak continues.
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, tomorrow, today)
            .await
            .unwrap(),
        Some(2)
    );
    // Two days skipped: the streak restarts.
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, later, later.pred_opt().unwrap())
            .await
            .unwrap(),
        Some(1)
    );
    // The skipped days were minded by a shield: the anchor reaches back to
    // the stamp and the streak continues instead of restarting.
    let minded = later.succ_opt().unwrap();
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, minded, today)
            .await
            .unwrap(),
        Some(2)
    );

    // A settled death is forgotten by the next meal.
    AquariumCare::settle_deaths(&**client, user.id, 1)
        .await
        .unwrap();
    assert_eq!(
        AquariumCare::load(&**client, user.id)
            .await
            .unwrap()
            .unwrap()
            .deaths_settled,
        1
    );
    let far = later.succ_opt().unwrap();
    AquariumCare::feed_day(&**client, user.id, far, later)
        .await
        .unwrap();
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(care.deaths_settled, 0);
    assert_eq!(care.fry_creature, None);

    AquariumCare::set_fry(&**client, user.id, "clownfish", today)
        .await
        .unwrap();
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(care.fry_creature.as_deref(), Some("clownfish"));
    assert_eq!(care.fry_born, Some(today));
}

#[tokio::test]
async fn ensure_starts_the_clock_yesterday_and_keeps_an_existing_row() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-care-ensure").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();

    AquariumCare::ensure(&**client, user.id).await.unwrap();
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(care.last_fed.date_naive(), today.pred_opt().unwrap());
    assert_eq!(care.streak, 0);

    AquariumCare::feed_day(&**client, user.id, today, today.pred_opt().unwrap())
        .await
        .unwrap();
    AquariumCare::ensure(&**client, user.id).await.unwrap();
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        care.last_fed.date_naive(),
        today,
        "ensure never rewinds a fed tank"
    );
    assert_eq!(care.streak, 1);
}

#[test]
fn a_sprout_can_be_cut_for_a_week_and_then_it_is_a_plant() {
    assert!(!sprout_rooted(day(1), day(1)));
    assert!(
        !sprout_rooted(day(1), day(7)),
        "the seventh day: still a sprout"
    );
    assert!(sprout_rooted(day(1), day(8)), "the day after: rooted");
    assert_eq!(SPROUT_DAYS, 7);
    assert_eq!(SPROUT_EVERY_DAYS, 14);
}

#[tokio::test]
async fn the_sprout_clock_raises_one_every_two_weeks_and_cut_or_root_clears_it() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-care-sprout").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();
    AquariumCare::ensure(&**client, user.id).await.unwrap();

    // A fresh row books the first sprout two weeks out: nothing today.
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(care.sprout_born, None);
    assert_eq!(care.next_sprout, today + chrono::Days::new(14));
    assert!(
        !AquariumCare::sprout_up(&**client, user.id, today)
            .await
            .unwrap()
    );

    // Two weeks later: up it comes, and the next one is booked.
    let due = today + chrono::Days::new(14);
    assert!(
        AquariumCare::sprout_up(&**client, user.id, due)
            .await
            .unwrap()
    );
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(care.sprout_born, Some(due));
    assert_eq!(care.next_sprout, due + chrono::Days::new(14));
    // One at a time: a second is not raised while this one stands, even
    // when the next is due.
    assert!(
        !AquariumCare::sprout_up(&**client, user.id, due + chrono::Days::new(30))
            .await
            .unwrap()
    );

    // Too young to root, young enough to cut.
    let sixth = due + chrono::Days::new(6);
    assert!(
        !AquariumCare::root_sprout(&**client, user.id, sixth)
            .await
            .unwrap()
    );
    assert!(
        AquariumCare::cut_sprout(&**client, user.id, sixth)
            .await
            .unwrap()
    );
    assert!(
        !AquariumCare::cut_sprout(&**client, user.id, sixth)
            .await
            .unwrap(),
        "nothing left to cut"
    );

    // The next one, left alone for a week, roots; a late cut finds a plant.
    let second = due + chrono::Days::new(14);
    assert!(
        AquariumCare::sprout_up(&**client, user.id, second)
            .await
            .unwrap()
    );
    let rooted_on = second + chrono::Days::new(7);
    assert!(
        !AquariumCare::cut_sprout(&**client, user.id, rooted_on)
            .await
            .unwrap()
    );
    assert!(
        AquariumCare::root_sprout(&**client, user.id, rooted_on)
            .await
            .unwrap()
    );
    assert_eq!(
        AquariumCare::load(&**client, user.id)
            .await
            .unwrap()
            .unwrap()
            .sprout_born,
        None
    );
}
