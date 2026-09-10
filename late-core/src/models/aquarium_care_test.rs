use chrono::{NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::{
    models::{
        aquarium_care::{
            AquariumCare, CARE_DAYS, deaths_due, dry_days, fish_weight, hatches_fry,
            pick_by_weight,
        },
        aquarium_shield::AquariumShield,
        marketplace::FishStock,
    },
    test_utils::{create_test_user, test_db},
};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
}

#[test]
fn dry_days_count_every_unfed_day_the_shield_did_not_cover() {
    // Fed on the 1st, looking on the 15th: fourteen unfed days, one death.
    assert_eq!(dry_days(day(1), day(15), None), 14);
    assert_eq!(deaths_due(14), 1);
    assert_eq!(deaths_due(13), 0);
    assert_eq!(deaths_due(28), 2);
    // Fed today or in the future: nothing on the clock.
    assert_eq!(dry_days(day(15), day(15), None), 0);

    // A shield over the 5th through the 10th excuses those six days, and it
    // still counts after it has lapsed: the 15th is past its end.
    let shield = AquariumShield {
        starts_at: Utc.with_ymd_and_hms(2026, 9, 5, 12, 0, 0).unwrap(),
        ends_at: Utc.with_ymd_and_hms(2026, 9, 10, 12, 0, 0).unwrap(),
    };
    assert_eq!(dry_days(day(1), day(15), Some(&shield)), 8);
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
        AquariumCare::feed_day(&**client, user.id, today).await.unwrap(),
        Some(1)
    );
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, today).await.unwrap(),
        None
    );
    // The row stamps `last_fed` with the clock, so "tomorrow" is the day
    // after that stamp: the streak continues.
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, tomorrow)
            .await
            .unwrap(),
        Some(2)
    );
    // Two days skipped: the streak restarts.
    assert_eq!(
        AquariumCare::feed_day(&**client, user.id, later).await.unwrap(),
        Some(1)
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
    AquariumCare::feed_day(&**client, user.id, far).await.unwrap();
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

    AquariumCare::feed_day(&**client, user.id, today).await.unwrap();
    AquariumCare::ensure(&**client, user.id).await.unwrap();
    let care = AquariumCare::load(&**client, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(care.last_fed.date_naive(), today, "ensure never rewinds a fed tank");
    assert_eq!(care.streak, 1);
}
