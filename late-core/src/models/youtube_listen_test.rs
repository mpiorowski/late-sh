use chrono::{Duration, Utc};

use crate::{
    models::youtube_listen::YoutubeListen,
    test_utils::{create_test_user, test_db},
};

/// A listener is written once per day: the second roll call is a no-op.
/// A day's count holds each account once, and another day's rows stay out.
#[tokio::test]
async fn a_days_youtube_listeners_count_each_account_once() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let alice = create_test_user(&test_db.db, "youtube-alice").await;
    let bob = create_test_user(&test_db.db, "youtube-bob").await;
    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);

    assert!(
        YoutubeListen::record(&client, alice.id, today)
            .await
            .expect("record")
    );
    assert!(
        !YoutubeListen::record(&client, alice.id, today)
            .await
            .expect("record again"),
        "the same listener on the same day is a no-op"
    );
    for (user, day) in [(bob.id, today), (bob.id, yesterday)] {
        assert!(
            YoutubeListen::record(&client, user, day)
                .await
                .expect("record")
        );
    }

    let on = async |day| {
        YoutubeListen::listeners_on(&client, day)
            .await
            .expect("listeners")
    };
    assert_eq!(on(today).await, 2);
    assert_eq!(on(yesterday).await, 1);
    assert_eq!(on(today - Duration::days(2)).await, 0);
}
