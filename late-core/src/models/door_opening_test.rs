use chrono::{Duration, Utc};

use crate::{
    models::{
        door_opening::{DoorCard, DoorOpening},
        leaderboard::{DayTally, DoorGame, WeekGame, day_tallies},
    },
    test_utils::{create_test_user, test_db},
};

/// Every opening is its own row, the same member again included; the
/// day's tallies count a card's openings inside the UTC day and leave
/// another day's out; every card's key reads back as the card, and the
/// roguelikes' keys are the leaderboard's.
#[tokio::test]
async fn each_opening_is_counted_on_its_day() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let alice = create_test_user(&test_db.db, "door-alice").await;
    let bob = create_test_user(&test_db.db, "door-bob").await;
    let now = Utc::now();
    let today = now.date_naive();

    for (user, card, at) in [
        (alice.id, DoorCard::Nethack, now),
        (alice.id, DoorCard::Nethack, now),
        (bob.id, DoorCard::Nethack, now),
        (bob.id, DoorCard::Dcss, now),
        (bob.id, DoorCard::Lateania, now),
        (alice.id, DoorCard::Brogue, now - Duration::days(1)),
    ] {
        DoorOpening::record(&client, user, card, at)
            .await
            .expect("record");
    }

    let rows: i64 = client
        .query_one(
            "SELECT count(*) FROM door_openings WHERE user_id = $1 OR user_id = $2",
            &[&alice.id, &bob.id],
        )
        .await
        .expect("count")
        .get(0);
    assert_eq!(rows, 6, "every opening is a row of its own");

    let doors: Vec<DayTally> = day_tallies(&client, today)
        .await
        .expect("day")
        .into_iter()
        .filter(|tally| matches!(tally.game, WeekGame::Door(_)))
        .collect();
    let tally = |game, plays, won| DayTally { game, plays, won };
    assert_eq!(
        doors,
        vec![
            tally(WeekGame::Door(DoorGame::Dcss), 1, 0),
            tally(WeekGame::Door(DoorGame::Nethack), 3, 0),
        ],
        "the roguelikes' openings, in roster order; Lateania is not a door of the shelf and yesterday's Brogue is not today's"
    );

    for card in DoorCard::ALL {
        assert_eq!(DoorCard::from_key(card.key()), Some(card));
    }
    for game in DoorGame::ALL {
        assert_eq!(game.card().key(), game.key());
    }
}
