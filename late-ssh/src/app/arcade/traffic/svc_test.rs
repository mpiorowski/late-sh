use late_core::models::traffic::{HighScore, TrackScore};

use crate::test_helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn aggregate_high_score_is_sum_of_per_track_bests() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "traffic-aggregate").await;
    let mut client = test_db.db.get().await.expect("db client");

    // First track: total is just this track.
    let total = HighScore::update_track_score_if_higher(&mut client, user.id, "alpha", 700)
        .await
        .expect("submit alpha");
    assert_eq!(total, 700);

    // Second track: aggregate is the sum of both bests.
    let total = HighScore::update_track_score_if_higher(&mut client, user.id, "beta", 250)
        .await
        .expect("submit beta");
    assert_eq!(total, 950);

    // A lower score on an existing track is ignored (GREATEST), aggregate holds.
    let total = HighScore::update_track_score_if_higher(&mut client, user.id, "alpha", 400)
        .await
        .expect("submit lower alpha");
    assert_eq!(total, 950);

    // A higher score on an existing track raises both the track best and total.
    let total = HighScore::update_track_score_if_higher(&mut client, user.id, "alpha", 900)
        .await
        .expect("submit higher alpha");
    assert_eq!(total, 1150);

    // Per-track bests reflect the GREATEST-kept values.
    let mut scores = TrackScore::list_for_user(&client, user.id)
        .await
        .expect("list track scores");
    scores.sort_by(|a, b| a.track_key.cmp(&b.track_key));
    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].track_key, "alpha");
    assert_eq!(scores[0].score, 900);
    assert_eq!(scores[1].track_key, "beta");
    assert_eq!(scores[1].score, 250);

    // Aggregate row mirrors the latest total.
    let hs = HighScore::find_by_user_id(&client, user.id)
        .await
        .expect("load high score")
        .expect("existing high score");
    assert_eq!(hs.score, 1150);
}

#[tokio::test]
async fn record_score_event_writes_a_traffic_row() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "traffic-score-event").await;
    let client = test_db.db.get().await.expect("db client");

    HighScore::record_score_event(&client, user.id, 1150)
        .await
        .expect("record score event");

    let rows = client
        .query(
            "SELECT score FROM game_score_events WHERE user_id = $1 AND game = 'traffic'",
            &[&user.id],
        )
        .await
        .expect("load score events");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, i32>("score"), 1150);
}
