use std::future::poll_fn;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use tokio_postgres::{AsyncMessage, Client, NoTls};

use crate::{
    models::crown::{
        CROWN_CHANGED_CHANNEL, CROWN_MIN_PRICE, CrownChange, CrownReign, crown_month, next_price,
        previous_crown_month,
    },
    test_utils::{create_test_user, test_db},
};

async fn wait_for_blocker(probe: &Client, waiter_pid: i32, blocker_pid: i32) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let blocked: bool = probe
                .query_one(
                    "SELECT $1::int = ANY(pg_blocking_pids($2))",
                    &[&blocker_pid, &waiter_pid],
                )
                .await
                .expect("probe lock wait")
                .get(0);
            if blocked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the second take blocks on the first");
}

/// The ladder is the product: the crown is never priced by us, only by
/// whoever last paid for it. These six rungs are the decided ladder
/// (2026-08-26), so a change here is a decision rather than a refactor.
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

/// Two takes racing for a vacant crown must land in order. The advisory lock
/// in `lock_open` is what serializes them, so the second transaction reads
/// the reign the first one opened instead of inserting beside it.
#[tokio::test]
async fn a_second_take_waits_and_sees_the_reign_the_first_one_opened() {
    let test_db = test_db().await;
    let mut first_client = test_db.db.get().await.expect("db client");
    let mut second_client = test_db.db.get().await.expect("db client");
    let first_taker = create_test_user(&test_db.db, "crown-race-first").await;
    let second_taker = create_test_user(&test_db.db, "crown-race-second").await;
    let probe = test_db.db.get().await.expect("probe client");
    let first_pid = first_client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .expect("first pid")
        .get(0);
    let second_pid = second_client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .expect("second pid")
        .get(0);

    let first = first_client.transaction().await.expect("tx");
    let (held, taken_at) = CrownReign::lock_open(&first).await.expect("lock");
    assert_eq!(held, None);
    let opened = CrownReign::open_in_tx(&first, first_taker.id, CROWN_MIN_PRICE, taken_at)
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

    wait_for_blocker(&probe, second_pid, first_pid).await;
    let before_release: DateTime<Utc> = first
        .query_one("SELECT clock_timestamp()", &[])
        .await
        .expect("clock before release")
        .get(0);
    first.commit().await.expect("commit");

    let (held, taken_at) = tokio::time::timeout(Duration::from_secs(15), second_lock)
        .await
        .expect("second take unblocks")
        .expect("second take task");
    let held = held.expect("the second take sees the first one's reign");
    assert_eq!(held.id, opened.id);
    assert_eq!(held.holder_user_id, first_taker.id);
    assert_eq!(held.paid_chips, CROWN_MIN_PRICE);
    assert_ne!(held.holder_user_id, second_taker.id);
    assert!(
        taken_at >= before_release,
        "timestamp follows the advisory-lock wait"
    );
}

/// The row lock can also wait after the advisory lock has been acquired.
#[tokio::test]
async fn the_take_timestamp_follows_the_row_lock_wait() {
    let test_db = test_db().await;
    let holder = create_test_user(&test_db.db, "crown-row-holder").await;
    let mut first_client = test_db.db.get().await.expect("first client");
    let mut second_client = test_db.db.get().await.expect("second client");
    let probe = test_db.db.get().await.expect("probe client");
    let first_pid = first_client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .expect("first pid")
        .get(0);
    let second_pid = second_client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .expect("second pid")
        .get(0);

    let setup = first_client.transaction().await.expect("setup tx");
    let (_, taken_at) = CrownReign::lock_open(&setup).await.expect("lock");
    let opened = CrownReign::open_in_tx(&setup, holder.id, CROWN_MIN_PRICE, taken_at)
        .await
        .expect("open");
    setup.commit().await.expect("commit setup");

    // Hold only the row lock, so the waiter gets past the advisory lock.
    let first = first_client.transaction().await.expect("holder tx");
    first
        .query_one(
            "SELECT id FROM crown_reigns WHERE id = $1 FOR UPDATE",
            &[&opened.id],
        )
        .await
        .expect("row lock");
    let second_lock = tokio::spawn(async move {
        let second = second_client.transaction().await.expect("waiter tx");
        let locked = CrownReign::lock_open(&second).await.expect("lock");
        second.commit().await.expect("commit waiter");
        locked
    });
    wait_for_blocker(&probe, second_pid, first_pid).await;
    let before_release: DateTime<Utc> = first
        .query_one("SELECT clock_timestamp()", &[])
        .await
        .expect("clock before release")
        .get(0);
    first.commit().await.expect("release row lock");
    let (held, taken_at) = tokio::time::timeout(Duration::from_secs(15), second_lock)
        .await
        .expect("row lock unblocks")
        .expect("waiter task");
    assert_eq!(held.expect("open reign").id, opened.id);
    assert!(
        taken_at >= before_release,
        "timestamp follows the row-lock wait"
    );
}

/// Transaction start order need not match lock acquisition order. The older
/// transaction must be able to replace a reign opened by the newer one.
#[tokio::test]
async fn an_older_transaction_can_take_a_newer_reign() {
    let test_db = test_db().await;
    let holder = create_test_user(&test_db.db, "crown-newer-holder").await;
    let challenger = create_test_user(&test_db.db, "crown-older-challenger").await;
    let mut older_client = test_db.db.get().await.expect("older client");
    let mut newer_client = test_db.db.get().await.expect("newer client");

    // Establish the older transaction's timestamp before the other take,
    // but deliberately defer its lock acquisition until after that take.
    let older = older_client.transaction().await.expect("older tx");
    let started_at: DateTime<Utc> = older
        .query_one("SELECT current_timestamp", &[])
        .await
        .expect("transaction timestamp")
        .get(0);
    let newer = newer_client.transaction().await.expect("newer tx");
    let (held, taken_at) = CrownReign::lock_open(&newer).await.expect("lock");
    assert!(held.is_none());
    let first = CrownReign::open_in_tx(&newer, holder.id, CROWN_MIN_PRICE, taken_at)
        .await
        .expect("first reign");
    newer.commit().await.expect("commit first take");
    assert!(first.taken_at > started_at, "transaction order is reversed");

    let (held, taken_at) = CrownReign::lock_open(&older)
        .await
        .expect("older transaction locks after the newer one");
    let held = held.expect("first reign");
    assert_eq!(held.id, first.id);
    CrownReign::close_in_tx(&older, held.id, taken_at)
        .await
        .expect("an older transaction can close a newer reign");
    let second = CrownReign::open_in_tx(
        &older,
        challenger.id,
        next_price(Some(held.paid_chips)),
        taken_at,
    )
    .await
    .expect("replacement reign");
    older.commit().await.expect("commit second take");

    let closed = CrownReign::from(
        newer_client
            .query_one("SELECT * FROM crown_reigns WHERE id = $1", &[&first.id])
            .await
            .expect("closed reign"),
    );
    assert_eq!(closed.ended_at, Some(second.taken_at));
    assert!(second.taken_at > first.taken_at);
    let open_count: i64 = newer_client
        .query_one(
            "SELECT count(*) FROM crown_reigns WHERE ended_at IS NULL",
            &[],
        )
        .await
        .expect("one open reign")
        .get(0);
    assert_eq!(open_count, 1);
    let deposed = CrownReign::deposed_for_reigns(&newer_client, &[first.id, second.id])
        .await
        .expect("predecessors");
    assert_eq!(deposed.len(), 1);
    assert_eq!(deposed.get(&second.id), Some(&holder.id));
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
    let (_, taken_at) = CrownReign::lock_open(&tx).await.expect("lock");
    let first = CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE, taken_at)
        .await
        .expect("open");
    let beside = CrownReign::open_in_tx(
        &tx,
        challenger.id,
        next_price(Some(first.paid_chips)),
        taken_at,
    )
    .await;
    assert!(
        beside.is_err(),
        "a second open reign must be rejected by the table"
    );
    drop(tx);

    let tx = client.transaction().await.expect("tx");
    let (_, taken_at) = CrownReign::lock_open(&tx).await.expect("lock");
    let first = CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE, taken_at)
        .await
        .expect("open");
    let (_, taken_at) = CrownReign::lock_open(&tx).await.expect("lock replacement");
    CrownReign::close_in_tx(&tx, first.id, taken_at)
        .await
        .expect("close");
    let second = CrownReign::open_in_tx(
        &tx,
        challenger.id,
        next_price(Some(first.paid_chips)),
        taken_at,
    )
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
    assert_eq!(open.month, crown_month(taken_at));
}

/// A take's UTC month and both reign boundaries use the supplied timestamp,
/// even when the transaction and the session timezone say something else.
#[tokio::test]
async fn a_take_at_the_month_boundary_uses_one_utc_timestamp() {
    let test_db = test_db().await;
    let holder = create_test_user(&test_db.db, "crown-month-holder").await;
    let challenger = create_test_user(&test_db.db, "crown-month-challenger").await;
    let mut client = test_db.db.get().await.expect("db client");
    let before = Utc.with_ymd_and_hms(2026, 7, 31, 23, 59, 59).unwrap()
        + chrono::Duration::microseconds(999_999);
    let after = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();

    let tx = client.transaction().await.expect("tx");
    tx.batch_execute("SET LOCAL TIME ZONE 'Pacific/Honolulu'")
        .await
        .expect("non-UTC session timezone");
    CrownReign::lock_open(&tx).await.expect("lock");
    // Supply fixed boundary times instead of waiting for a real rollover.
    let first = CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE, before)
        .await
        .expect("last take of July");
    assert_eq!(first.taken_at, before);
    assert_eq!(
        first.month,
        chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap()
    );
    assert!(first.is_current(before));
    assert!(!first.is_current(after));

    CrownReign::close_in_tx(&tx, first.id, after)
        .await
        .expect("close July reign");
    let second = CrownReign::open_in_tx(&tx, challenger.id, CROWN_MIN_PRICE, after)
        .await
        .expect("first take of August");
    tx.commit().await.expect("commit");
    let closed = CrownReign::from(
        client
            .query_one("SELECT * FROM crown_reigns WHERE id = $1", &[&first.id])
            .await
            .expect("closed reign"),
    );
    assert_eq!(closed.ended_at, Some(after));
    assert_eq!(second.taken_at, after);
    assert_eq!(second.month, after.date_naive());
    assert!(second.is_current(after));
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
    let listen_statement = format!("LISTEN {CROWN_CHANGED_CHANNEL};");
    let listen = listener.batch_execute(&listen_statement);
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
    let (_, taken_at) = CrownReign::lock_open(&rolled_back).await.expect("lock");
    CrownReign::open_in_tx(&rolled_back, holder.id, CROWN_MIN_PRICE, taken_at)
        .await
        .expect("open");
    CrownReign::notify_changed(&rolled_back, &change)
        .await
        .expect("notify");
    drop(rolled_back);

    let tx = client.transaction().await.expect("tx");
    let (_, taken_at) = CrownReign::lock_open(&tx).await.expect("lock");
    CrownReign::open_in_tx(&tx, holder.id, CROWN_MIN_PRICE, taken_at)
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

/// A ledger row's ref is a reign id; the reign taken right before it names
/// who lost the crown. The first reign ever took it from nobody.
#[tokio::test]
async fn deposed_holders_resolve_from_the_reign_before() {
    let test_db = test_db().await;
    let first = create_test_user(&test_db.db, "crown-first").await;
    let second = create_test_user(&test_db.db, "crown-second").await;
    let mut client = test_db.db.get().await.expect("db client");

    let tx = client.transaction().await.expect("tx");
    let (_, taken_at) = CrownReign::lock_open(&tx).await.expect("lock");
    let first_reign = CrownReign::open_in_tx(&tx, first.id, CROWN_MIN_PRICE, taken_at)
        .await
        .expect("first reign");
    tx.commit().await.expect("commit");
    let tx = client.transaction().await.expect("tx");
    let (_, taken_at) = CrownReign::lock_open(&tx).await.expect("lock");
    CrownReign::close_in_tx(&tx, first_reign.id, taken_at)
        .await
        .expect("close");
    let second_reign =
        CrownReign::open_in_tx(&tx, second.id, next_price(Some(CROWN_MIN_PRICE)), taken_at)
            .await
            .expect("second reign");
    tx.commit().await.expect("commit");

    let deposed = CrownReign::deposed_for_reigns(&client, &[first_reign.id, second_reign.id])
        .await
        .expect("deposed");
    assert_eq!(deposed.len(), 1);
    assert_eq!(deposed.get(&second_reign.id), Some(&first.id));
}

/// The laureate is whoever wore the crown when the month ended: the last
/// reign of that month, whether it was closed by a later take or left open
/// across the rollover. Only the two newest months come back, newest first,
/// one reign each.
#[tokio::test]
async fn the_last_reign_of_each_recent_month_is_who_wore_it_at_the_end() {
    let test_db = test_db().await;
    let june = create_test_user(&test_db.db, "crown-months-june").await;
    let july_early = create_test_user(&test_db.db, "crown-months-july-early").await;
    let july_late = create_test_user(&test_db.db, "crown-months-july-late").await;
    let august = create_test_user(&test_db.db, "crown-months-august").await;
    let mut client = test_db.db.get().await.expect("db client");

    let takes = [
        (june.id, Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap()),
        (july_early.id, Utc.with_ymd_and_hms(2026, 7, 2, 12, 0, 0).unwrap()),
        (july_late.id, Utc.with_ymd_and_hms(2026, 7, 30, 12, 0, 0).unwrap()),
        (august.id, Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap()),
    ];
    let mut open: Option<CrownReign> = None;
    for (holder, taken_at) in takes {
        let tx = client.transaction().await.expect("tx");
        CrownReign::lock_open(&tx).await.expect("lock");
        if let Some(previous) = &open {
            CrownReign::close_in_tx(&tx, previous.id, taken_at)
                .await
                .expect("close");
        }
        open = Some(
            CrownReign::open_in_tx(&tx, holder, CROWN_MIN_PRICE, taken_at)
                .await
                .expect("open"),
        );
        tx.commit().await.expect("commit");
    }

    let last = CrownReign::last_of_recent_months(&client)
        .await
        .expect("last reigns");
    let summary: Vec<(chrono::NaiveDate, uuid::Uuid)> = last
        .iter()
        .map(|reign| (reign.month, reign.holder_user_id))
        .collect();
    assert_eq!(
        summary,
        vec![
            (chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(), august.id),
            (chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(), july_late.id),
        ]
    );
}

/// The laureate's month is the calendar month before now, across a year
/// boundary too.
#[test]
fn the_previous_crown_month_steps_back_one_calendar_month() {
    assert_eq!(
        previous_crown_month(Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap()),
        chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap()
    );
    assert_eq!(
        previous_crown_month(Utc.with_ymd_and_hms(2027, 1, 31, 23, 59, 59).unwrap()),
        chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()
    );
}
