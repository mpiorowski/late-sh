use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::StreetService;
use crate::app::deadchannel::street::session::StreetSession;
use crate::app::deadchannel::street::state::{StreetRunner, StreetView};
use crate::pg_listener::{PgListener, Signal};
use crate::test_helpers::{new_test_db, wait_until};

/// Two replicas as two street services on one database: what one
/// session does on replica A reaches a session on replica B over real
/// `pg_notify`, and a logout takes the runner off B's street.
#[tokio::test]
async fn a_runner_on_one_replica_walks_dims_and_leaves_on_the_other() {
    let test_db = new_test_db().await;
    let mut listener = PgListener::new();
    let replica_a = StreetService::start(
        test_db.db.clone(),
        listener.subscribe(StreetService::CHANNELS),
    );
    let replica_b = StreetService::start(
        test_db.db.clone(),
        listener.subscribe(StreetService::CHANNELS),
    );
    // Act only once the LISTEN is live, so the first flush is heard and
    // the test does not ride on a heartbeat.
    let mut probe = listener.subscribe(StreetService::CHANNELS);
    let _task = listener.start(test_db.db.config().clone());
    let resync = timeout(Duration::from_secs(10), probe.recv())
        .await
        .expect("the listener comes up");
    assert_eq!(resync, Some(Signal::Resync));

    let (ann, bob) = (Uuid::now_v7(), Uuid::now_v7());
    let mut on_a = StreetSession::new(&replica_a, ann);
    let mut on_b = StreetSession::new(&replica_b, bob);
    // Before the descent there is nothing to say.
    on_a.sync(10, 5, true);
    on_a.descend();
    on_a.sync(10, 5, true);
    on_a.sync(11, 5, true);
    sees(
        &mut on_b,
        [(ann, StreetRunner { x: 11, y: 5, present: true })].into(),
        "ann's step reaches replica b",
    )
    .await;

    on_a.sync(11, 5, false);
    sees(
        &mut on_b,
        [(ann, StreetRunner { x: 11, y: 5, present: false })].into(),
        "ann looking away dims her on replica b",
    )
    .await;

    drop(on_a);
    sees(
        &mut on_b,
        StreetView::new(),
        "ann's logout clears replica b's street",
    )
    .await;
}

/// Wait until `session`'s copy of the street is exactly `view`.
async fn sees(session: &mut StreetSession, view: StreetView, label: &str) {
    wait_until(
        || {
            session.refresh();
            let hit = *session.view == view;
            async move { hit }
        },
        label,
    )
    .await;
}
