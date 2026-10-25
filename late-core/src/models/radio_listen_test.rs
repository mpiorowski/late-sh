use chrono::{Duration, Utc};

use crate::{
    models::radio_listen::{RadioListen, StationListeners},
    radio::RadioStation,
    test_utils::{create_test_user, test_db},
};

/// A listener is written once per station per day: the second roll call
/// is a no-op. A day's stations come back by listener count, a tie by
/// key, under the limit, and another day's rows stay out.
#[tokio::test]
async fn a_days_stations_count_each_listener_once() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let alice = create_test_user(&test_db.db, "radio-alice").await;
    let bob = create_test_user(&test_db.db, "radio-bob").await;
    let carol = create_test_user(&test_db.db, "radio-carol").await;
    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);
    let nightride = RadioStation::from_key("nightride").expect("nightride");
    let chillsynth = RadioStation::from_key("chillsynth").expect("chillsynth");
    let datawave = RadioStation::from_key("datawave").expect("datawave");

    assert!(
        RadioListen::record(&client, alice.id, nightride, today)
            .await
            .expect("record")
    );
    assert!(
        !RadioListen::record(&client, alice.id, nightride, today)
            .await
            .expect("record again"),
        "the same listener on the same day is a no-op"
    );
    for (user, station) in [
        (bob.id, nightride),
        (bob.id, chillsynth),
        (carol.id, datawave),
    ] {
        assert!(
            RadioListen::record(&client, user, station, today)
                .await
                .expect("record")
        );
    }
    assert!(
        RadioListen::record(&client, carol.id, chillsynth, yesterday)
            .await
            .expect("record yesterday")
    );

    let listeners = |station: &str, listeners| StationListeners {
        station: station.to_string(),
        listeners,
    };
    assert_eq!(
        RadioListen::top_stations_on(&client, today, 3)
            .await
            .expect("today"),
        vec![
            listeners("nightride", 2),
            listeners("chillsynth", 1),
            listeners("datawave", 1),
        ]
    );
    assert_eq!(
        RadioListen::top_stations_on(&client, today, 1)
            .await
            .expect("top one"),
        vec![listeners("nightride", 2)]
    );
    assert_eq!(
        RadioListen::top_stations_on(&client, yesterday, 3)
            .await
            .expect("yesterday"),
        vec![listeners("chillsynth", 1)]
    );
    assert!(
        RadioListen::top_stations_on(&client, today - Duration::days(2), 3)
            .await
            .expect("a quiet day")
            .is_empty()
    );
}
