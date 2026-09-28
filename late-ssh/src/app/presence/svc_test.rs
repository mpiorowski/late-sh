use late_core::models::presence::{ClubhouseStand, PresenceRecord, Spot, StreetStand};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::PresenceService;
use crate::app::presence::session::PresenceSession;
use crate::pg_listener::{PgListener, Signal};
use crate::test_helpers::{new_test_db, wait_until};

/// Two replicas as two presence services on one database: what one
/// session publishes on replica A reaches a session on replica B over real
/// `pg_notify`, and a logout takes it off B's list.
#[tokio::test]
async fn a_session_on_one_replica_moves_and_leaves_on_the_other() {
    let test_db = new_test_db().await;
    let mut listener = PgListener::new();
    let replica_a = PresenceService::start(
        test_db.db.clone(),
        listener.subscribe(PresenceService::CHANNELS),
    );
    let replica_b = PresenceService::start(
        test_db.db.clone(),
        listener.subscribe(PresenceService::CHANNELS),
    );
    // Act only once the LISTEN is live, so the first flush is heard and
    // the test does not ride on a heartbeat.
    let mut probe = listener.subscribe(PresenceService::CHANNELS);
    let _task = listener.start(test_db.db.config().clone());
    let resync = timeout(Duration::from_secs(10), probe.recv())
        .await
        .expect("the listener comes up");
    assert_eq!(resync, Some(Signal::Resync));

    let mut on_a = PresenceSession::new(&replica_a, Uuid::now_v7());
    let mut on_b = PresenceSession::new(&replica_b, Uuid::now_v7());
    let mut ann = PresenceRecord {
        session_id: on_a.session_id(),
        user_id: Uuid::now_v7(),
        username: "ann".to_string(),
        clubhouse: ClubhouseStand {
            spot: Spot::Seat { index: 3 },
            since_ms: 1_000,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: None,
        street: None,
    };

    on_a.publish(ann.clone());
    sees(&mut on_b, vec![ann.clone()], "ann sits down on replica b").await;

    ann.street = Some(StreetStand {
        x: 11,
        y: 5,
        present: true,
        moved_at_ms: 2_000,
    });
    on_a.publish(ann.clone());
    sees(&mut on_b, vec![ann.clone()], "ann's descent reaches replica b").await;

    drop(on_a);
    sees(&mut on_b, Vec::new(), "ann's logout clears replica b").await;
}

/// Wait until `session`'s copy of presence is exactly `records`.
async fn sees(session: &mut PresenceSession, records: Vec<PresenceRecord>, label: &str) {
    wait_until(
        || {
            session.refresh();
            let hit = *session.records == records;
            async move { hit }
        },
        label,
    )
    .await;
}
