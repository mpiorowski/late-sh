use chrono::{Duration, NaiveDate, Utc};
use uuid::Uuid;

use super::{Ambush, AmbushOutcome, NewAmbush};
use crate::test_utils::{create_test_user, test_db};

/// Each test gets a day of its own, so the day's caps never collide
/// across tests sharing a database.
fn fresh_day() -> NaiveDate {
    let offset = (Uuid::now_v7().as_u128() % 20_000) as i64;
    NaiveDate::from_ymd_opt(2100, 1, 1).unwrap() + Duration::days(offset)
}

fn crossing(day: NaiveDate, attacker: Uuid, defender: Uuid, step: i32) -> NewAmbush {
    NewAmbush {
        id: Uuid::now_v7(),
        day,
        attacker_user_id: attacker,
        defender_user_id: defender,
        attacker_username: "tom".to_string(),
        defender_username: "mira".to_string(),
        step: Some(step),
        stake: 340,
        answer_to: None,
        started_at: Utc::now(),
    }
}

#[tokio::test]
async fn a_sleeper_is_crossed_once_a_day_and_a_pair_crosses_once() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    let tom = create_test_user(&test_db.db, "ambush-tom").await.id;
    let mira = create_test_user(&test_db.db, "ambush-mira").await.id;
    let dax = create_test_user(&test_db.db, "ambush-dax").await.id;
    let day = fresh_day();

    let first = Ambush::insert(client, &crossing(day, tom, mira, 6))
        .await
        .expect("insert")
        .expect("the first crossing lands");
    assert_eq!(first.stake, 340);
    assert_eq!(first.step, Some(6));
    assert!(!first.is_answer());
    assert!(first.outcome.is_none());

    // Mira was crossed today: dax cannot cross her too.
    assert!(
        Ambush::insert(client, &crossing(day, dax, mira, 6))
            .await
            .expect("insert")
            .is_none()
    );
    // Mira cannot cross tom back today either: the pair crossed.
    assert!(
        Ambush::insert(client, &crossing(day, mira, tom, 6))
            .await
            .expect("insert")
            .is_none()
    );
    // Dax crossing tom is a different pair and a different sleeper.
    assert!(
        Ambush::insert(client, &crossing(day, dax, tom, 6))
            .await
            .expect("insert")
            .is_some()
    );
    // Tomorrow, mira is on the road again.
    assert!(
        Ambush::insert(client, &crossing(day + Duration::days(1), dax, mira, 6))
            .await
            .expect("insert")
            .is_some()
    );

    let today = Ambush::crossings_of_day(client, day).await.expect("read");
    assert_eq!(today.len(), 2);
}

#[tokio::test]
async fn a_won_crossing_is_owed_one_answer_until_it_is_given() {
    let test_db = test_db().await;
    let pooled = test_db.db.get().await.expect("db client");
    let client: &tokio_postgres::Client = &pooled;
    let tom = create_test_user(&test_db.db, "answer-tom").await.id;
    let mira = create_test_user(&test_db.db, "answer-mira").await.id;
    let day = fresh_day();
    let since = Utc::now() - Duration::hours(1);

    let first = Ambush::insert(client, &crossing(day, tom, mira, 4))
        .await
        .expect("insert")
        .expect("lands");
    // Open: nothing to answer, nothing to read.
    assert!(
        Ambush::answerable(client, mira, since)
            .await
            .expect("read")
            .is_none()
    );
    assert!(
        Ambush::latest_on(client, mira, since)
            .await
            .expect("read")
            .is_none()
    );

    let now = Utc::now();
    assert!(
        Ambush::end(client, first.id, AmbushOutcome::Won, 340, now)
            .await
            .expect("end")
    );
    // Ended once.
    assert!(
        !Ambush::end(client, first.id, AmbushOutcome::Lost, 0, now)
            .await
            .expect("end")
    );
    let ended = Ambush::get(client, first.id)
        .await
        .expect("read")
        .expect("the row");
    assert_eq!(ended.outcome, Some(AmbushOutcome::Won));
    assert_eq!(ended.bits_moved, 340);

    // Mira is owed an answer, and the news on her row is this crossing.
    let owed = Ambush::answerable(client, mira, since)
        .await
        .expect("read")
        .expect("owed");
    assert_eq!(owed.id, first.id);
    assert_eq!(
        Ambush::latest_on(client, mira, since)
            .await
            .expect("read")
            .map(|row| row.id),
        Some(first.id)
    );
    // Tom is owed nothing.
    assert!(
        Ambush::answerable(client, tom, since)
            .await
            .expect("read")
            .is_none()
    );

    let answer = Ambush::insert(client, &crossing_answer(day, mira, tom, first.id))
        .await
        .expect("insert")
        .expect("the answer lands on the same day the pair crossed");
    assert!(answer.is_answer());
    // One answer per crossing.
    assert!(
        Ambush::insert(client, &crossing_answer(day, mira, tom, first.id))
            .await
            .expect("insert")
            .is_none()
    );
    // Answered: nothing owed any more.
    assert!(
        Ambush::answerable(client, mira, since)
            .await
            .expect("read")
            .is_none()
    );
    // A crossing that the attacker lost owes nothing.
    let day2 = day + Duration::days(1);
    let lost = Ambush::insert(client, &crossing(day2, tom, mira, 2))
        .await
        .expect("insert")
        .expect("lands");
    assert!(
        Ambush::end(client, lost.id, AmbushOutcome::Lost, -50, Utc::now())
            .await
            .expect("end")
    );
    assert!(
        Ambush::answerable(client, mira, since)
            .await
            .expect("read")
            .is_none()
    );
}

fn crossing_answer(day: NaiveDate, answerer: Uuid, attacker: Uuid, crossing: Uuid) -> NewAmbush {
    NewAmbush {
        id: Uuid::now_v7(),
        day,
        attacker_user_id: answerer,
        defender_user_id: attacker,
        attacker_username: "mira".to_string(),
        defender_username: "tom".to_string(),
        step: None,
        stake: 120,
        answer_to: Some(crossing),
        started_at: Utc::now(),
    }
}
