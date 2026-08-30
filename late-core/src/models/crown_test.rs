use std::future::poll_fn;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use tokio_postgres::{AsyncMessage, NoTls};

use crate::{
    models::crown::{
        CROWN_CHANGED_CHANNEL, CROWN_MIN_PRICE, CrownChange, CrownReign, crown_month,
        listen_for_crown_changes, next_price,
    },
    test_utils::{create_test_user, test_db},
};

/// The ladder is the product: the crown is never priced by us, only by
/// whoever last paid for it. SHOP.md's fixed-numbers table quotes these six
/// rungs, so a change here is a decision rather than a refactor.
#[test]
fn the_price_ladder_climbs_from_a_vacant_crown() {
    let mut prices = Vec::new();
    let mut paid = None;
    for _ in 0..6 {
        let price = next_price(paid);
        prices.push(price);
        paid = Some(price);
    }
    assert_eq!(prices, vec![500, 750, 1_125, 1_688, 2_532, 3_798]);
    assert_eq!(prices[0], CROWN_MIN_PRICE);
}

/// A holder who paid less than the floor (only reachable if the minimum ever
/// rises) still costs the floor to unseat, and an absurd price cannot wrap
/// the ladder back down into affordable territory.
#[test]
fn the_ladder_never_drops_below_the_minimum() {
    assert_eq!(next_price(Some(1)), CROWN_MIN_PRICE);
    assert_eq!(next_price(Some(i64::MAX)), i64::MAX / 2 + 1);
}

/// The month boundary is enforced at read, with no sweeper: a reign left open
/// across the rollover stops counting the moment the month does, which is
/// what leaves the crown vacant at the minimum on the first.
#[test]
fn a_reign_stops_counting_when_its_month_does() {
    let taken_at = Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap();
    let reign = CrownReign {
        id: uuid::Uuid::now_v7(),
        month: crown_month(taken_at),
        holder_user_id: uuid::Uuid::now_v7(),
        paid_chips: 25_000,
        taken_at,
        ended_at: None,
    };

    let same_month = Utc.with_ymd_and_hms(2026, 7, 31, 23, 59, 59).unwrap();
    assert!(reign.is_current(same_month));

    let next_month = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
    assert!(!reign.is_current(next_month));
    assert_eq!(next_price(None), CROWN_MIN_PRICE);

    let closed = CrownReign {
        ended_at: Some(same_month),
        ..reign
    };
    assert!(!closed.is_current(same_month));
}

/// Two takes racing for a vacant crown must not both land. The advisory lock
/// in `lock_open` is what serializes them, so the second transaction reads
/// the reign the first one opened instead of inserting beside it.
#[tokio::test]
async fn a_second_take_waits_and_sees_the_reign_the_first_one_opened() {
    let test_db = test_db().await;
    let mut first_client = test_db.db.get().await.expect("db client");
    let mut second_client = test_db.db.get().await.expect("db client");
    let first_taker = create_test_user(&test_db.db, "crown-race-first").await;
    let second_taker = create_test_user(&test_db.db, "crown-race-second").await;

    let first = first_client.transaction().await.expect("tx");
    assert_eq!(CrownReign::lock_open(&first).await.expect("lock"), None);
    let opened = CrownReign::open_in_tx(&first, first_taker.id, CROWN_MIN_PRICE)
        .await
        .expect("open");

    // The first transaction already holds the advisory lock, so the second
    // blocks inside `lock_open` until the commit below, whatever order the
    // two tasks are scheduled in.
    let second_lock = tokio::spawn(async move {
        let second = second_client.transaction().await.expect("tx");
        let held = CrownReign::lock_open(&second).await.expect("lock");
        second.commit().await.expect("commit");
        held
    });

    first.commit().await.expect("commit");

    let held = tokio::time::timeout(Duration::from_secs(5), second_lock)
        .await
        .expect("second take unblocks")
        .expect("second take task");
    let held = held.expect("the second take sees the first one's reign");
    assert_eq!(held.id, opened.id);
    assert_eq!(held.holder_user_id, first_taker.id);
    assert_eq!(held.paid_chips, CROWN_MIN_PRICE);
    assert_ne!(held.holder_user_id, second_taker.id);
}

/// The table, not the service, is what guarantees one crown: an insert beside
/// a live reign is rejected outright, and closing the old one is what makes
/// room for the new.
#[tokio::test]
async fn only_one_reign_is_ever_open() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let holder = create_test_user(&test_db.db, "crown-single-holder").await;
    let challenger = create_test_user(&test_db.db, "crown-single-challenger").await;

    let tx = client.transaction().await.expect("tx");
    let first = CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE)
        .await
        .expect("open");
    let beside =
        CrownReign::open_in_tx(&tx, challenger.id, next_price(Some(first.paid_chips))).await;
    assert!(
        beside.is_err(),
        "a second open reign must be rejected by the table"
    );
    drop(tx);

    let tx = client.transaction().await.expect("tx");
    let first = CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE)
        .await
        .expect("open");
    CrownReign::close_in_tx(&tx, first.id).await.expect("close");
    let second = CrownReign::open_in_tx(&tx, challenger.id, next_price(Some(first.paid_chips)))
        .await
        .expect("open after close");
    tx.commit().await.expect("commit");

    let client = test_db.db.get().await.expect("db client");
    let open = CrownReign::find_open(&client)
        .await
        .expect("find open")
        .expect("a reign is open");
    assert_eq!(open.id, second.id);
    assert_eq!(open.holder_user_id, challenger.id);
    assert_eq!(open.paid_chips, 750);
    // Stamped by the database, from the clock `taken_at` comes from.
    assert_eq!(open.month, crown_month(open.taken_at));
    assert_eq!(open.month, crown_month(Utc::now()));
}

/// The glyph only reaches a second replica over Postgres, so the take
/// transaction must emit on the channel, and only on commit.
#[tokio::test]
async fn taking_the_crown_notifies_every_replica() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let holder = create_test_user(&test_db.db, "crown-notify-holder").await;
    let change = CrownChange {
        taker_username: "crown-notify-holder".to_string(),
        price: CROWN_MIN_PRICE,
        deposed_user_id: None,
    };

    let cfg = test_db.db.config();
    let mut listener_config = tokio_postgres::Config::new();
    listener_config
        .host(&cfg.host)
        .port(cfg.port)
        .user(&cfg.user)
        .password(&cfg.password)
        .dbname(&cfg.dbname);
    let (listener, mut connection) = listener_config
        .connect(NoTls)
        .await
        .expect("listener connection");
    let listen = listen_for_crown_changes(&listener);
    tokio::pin!(listen);
    let mut listen_done = false;
    while !listen_done {
        tokio::select! {
            result = &mut listen, if !listen_done => {
                result.expect("listen");
                listen_done = true;
            }
            message = poll_fn(|cx| connection.poll_message(cx)) => {
                let _ = message.expect("connection open").expect("connection ok");
            }
        }
    }

    // A rolled-back take must tell nobody, so this transaction is dropped
    // without committing before the one that counts.
    let rolled_back = client.transaction().await.expect("tx");
    CrownReign::open_in_tx(&rolled_back, holder.id, CROWN_MIN_PRICE)
        .await
        .expect("open");
    CrownReign::notify_changed(&rolled_back, &change)
        .await
        .expect("notify");
    drop(rolled_back);

    let tx = client.transaction().await.expect("tx");
    CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE)
        .await
        .expect("open");
    CrownReign::notify_changed(&tx, &change)
        .await
        .expect("notify");
    tx.commit().await.expect("commit");

    let mut seen = 0usize;
    while seen == 0 {
        let notification = tokio::time::timeout(
            Duration::from_secs(5),
            poll_fn(|cx| connection.poll_message(cx)),
        )
        .await
        .expect("crown notification")
        .expect("connection open")
        .expect("connection ok");
        if let AsyncMessage::Notification(notification) = notification
            && notification.channel() == CROWN_CHANGED_CHANNEL
        {
            seen += 1;
            assert_eq!(
                CrownChange::parse(notification.payload()).expect("payload parses"),
                change,
                "the payload is what the deposed holder's replica needs"
            );
        }
    }
    assert_eq!(seen, 1, "only the committed take notifies");
}
