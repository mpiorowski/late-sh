use std::future::poll_fn;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use tokio_postgres::{AsyncMessage, NoTls};
use uuid::Uuid;

use crate::{
    models::pot::{
        POT_CHANGED_CHANNEL, POT_MAX_TICKETS_PER_DAY, POT_TICKET_PRICE, Pot, PotChange, PotDraw,
        PotStatus, PotTicket, PotTicketHolder, draw_from_seed, listen_for_pot_changes,
        next_draw_at, payout_for,
    },
    test_utils::{create_test_user, test_db},
};

fn holder(user_id: Uuid, tickets: i64) -> PotTicketHolder {
    PotTicketHolder {
        user_id,
        tickets,
        bought_today: tickets,
    }
}

/// The whole-state assertion for the draw: fixed tickets plus a fixed seed
/// give one exact result, every field of it. SHOP.md's payout rule (80% to
/// the winner, the fifth burned) is what the last two fields pin.
#[test]
fn a_seeded_draw_settles_to_one_exact_result() {
    // Ordered by user id, the way `PotTicket::holders` returns them, so the
    // walk below is a function of the tickets alone.
    let a = Uuid::parse_str("00000000-0000-7000-8000-00000000000a").expect("uuid");
    let b = Uuid::parse_str("00000000-0000-7000-8000-00000000000b").expect("uuid");
    let c = Uuid::parse_str("00000000-0000-7000-8000-00000000000c").expect("uuid");
    let holders = [holder(a, 3), holder(b, 40), holder(c, 7)];

    assert_eq!(
        draw_from_seed(&holders, POT_TICKET_PRICE, 42),
        Some(PotDraw {
            winner_user_id: b,
            winner_tickets: 40,
            total_tickets: 50,
            size: 5_000,
            payout_chips: 4_000,
        })
    );

    // The same tickets under a different seed can land elsewhere; what may
    // never change is the arithmetic around the winner.
    let other = draw_from_seed(&holders, POT_TICKET_PRICE, 7).expect("a winner");
    assert_eq!(other.total_tickets, 50);
    assert_eq!(other.size, 5_000);
    assert_eq!(other.payout_chips, 4_000);
    assert!([a, b, c].contains(&other.winner_user_id));
}

/// Weighting is the whole fairness claim: over many seeds each holder is
/// drawn in proportion to their tickets, and a holder with none is never
/// drawn at all.
#[test]
fn the_draw_is_weighted_by_tickets() {
    let big = Uuid::parse_str("00000000-0000-7000-8000-0000000000b1").expect("uuid");
    let small = Uuid::parse_str("00000000-0000-7000-8000-0000000000c1").expect("uuid");
    let holders = [holder(big, 49), holder(small, 1)];
    let mut big_wins = 0;
    for seed in 1..=1_000u64 {
        let draw = draw_from_seed(&holders, POT_TICKET_PRICE, seed).expect("a winner");
        if draw.winner_user_id == big {
            big_wins += 1;
        }
    }
    // 49 of 50 tickets. A fair walk lands well above 900 of 1,000; this is a
    // regression guard on the walk, not a statistics exam.
    assert!(
        big_wins > 900,
        "the 49-ticket holder won only {big_wins} of 1,000 draws"
    );
}

/// Nobody bought in: there is no winner to invent, which is what the rolled
/// status exists for.
#[test]
fn an_empty_pot_draws_nobody() {
    assert_eq!(draw_from_seed(&[], POT_TICKET_PRICE, 1), None);
}

/// The burn is the gap between what came in and what went out, and it is
/// always a floor: an odd pot never pays out a chip more than 80%.
#[test]
fn the_payout_floors_at_four_fifths() {
    assert_eq!(payout_for(0), 0);
    assert_eq!(payout_for(100), 80);
    assert_eq!(payout_for(84_200), 67_360);
    // 4/5 of 300 is 240 exactly; 4/5 of 900 is 720. The interesting case is
    // a size that is not a multiple of five.
    assert_eq!(payout_for(999), 799);
}

/// The draw is Monday 21:00 UTC, and "next" is strictly after now, so a pot
/// settling at its own draw hour schedules the following Monday rather than
/// itself. 2026-08-31 is a Monday.
#[test]
fn the_next_draw_is_the_next_monday_twenty_one_hundred_utc() {
    let thursday = Utc.with_ymd_and_hms(2026, 8, 27, 9, 0, 0).unwrap();
    assert_eq!(
        next_draw_at(thursday),
        Utc.with_ymd_and_hms(2026, 8, 31, 21, 0, 0).unwrap()
    );

    let monday_morning = Utc.with_ymd_and_hms(2026, 8, 31, 9, 0, 0).unwrap();
    assert_eq!(
        next_draw_at(monday_morning),
        Utc.with_ymd_and_hms(2026, 8, 31, 21, 0, 0).unwrap(),
        "a Monday before the hour draws that evening"
    );

    let on_the_hour = Utc.with_ymd_and_hms(2026, 8, 31, 21, 0, 0).unwrap();
    assert_eq!(
        next_draw_at(on_the_hour),
        Utc.with_ymd_and_hms(2026, 9, 7, 21, 0, 0).unwrap(),
        "exactly on the hour is the following week"
    );

    let monday_late = Utc.with_ymd_and_hms(2026, 8, 31, 23, 30, 0).unwrap();
    assert_eq!(
        next_draw_at(monday_late),
        Utc.with_ymd_and_hms(2026, 9, 7, 21, 0, 0).unwrap()
    );

    let sunday_late = Utc.with_ymd_and_hms(2026, 8, 30, 23, 30, 0).unwrap();
    assert_eq!(
        next_draw_at(sunday_late),
        Utc.with_ymd_and_hms(2026, 8, 31, 21, 0, 0).unwrap()
    );
}

/// The cap is enforced by the insert itself: a buy that would take a player
/// past today's cap writes no row, so the caller can roll back an uncharged
/// refusal. It counts today's tickets only: yesterday's rows are held, not
/// spent against today.
#[tokio::test]
async fn the_daily_cap_is_enforced_by_the_insert() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let buyer = create_test_user(&test_db.db, "pot-cap-buyer").await;
    let other = create_test_user(&test_db.db, "pot-cap-other").await;

    let tx = client.transaction().await.expect("tx");
    let pot = Pot::open_in_tx(&tx, next_draw_at(Utc::now()), POT_TICKET_PRICE)
        .await
        .expect("open");
    let held = PotTicket::buy_in_tx(
        &tx,
        pot.id,
        buyer.id,
        POT_MAX_TICKETS_PER_DAY - 1,
        POT_MAX_TICKETS_PER_DAY,
    )
    .await
    .expect("buy");
    assert_eq!(held, Some(POT_MAX_TICKETS_PER_DAY - 1));

    // One over the cap: no row, and the holding is untouched.
    let refused = PotTicket::buy_in_tx(&tx, pot.id, buyer.id, 2, POT_MAX_TICKETS_PER_DAY)
        .await
        .expect("buy");
    assert_eq!(refused, None);
    assert_eq!(
        PotTicket::user_total(&*tx, pot.id, buyer.id)
            .await
            .expect("total"),
        POT_MAX_TICKETS_PER_DAY - 1
    );

    // Exactly to the cap goes through, and the cap is per player: someone
    // else's tickets are not counted against it.
    assert_eq!(
        PotTicket::buy_in_tx(&tx, pot.id, buyer.id, 1, POT_MAX_TICKETS_PER_DAY)
            .await
            .expect("buy"),
        Some(POT_MAX_TICKETS_PER_DAY)
    );
    assert_eq!(
        PotTicket::buy_in_tx(&tx, pot.id, other.id, 5, POT_MAX_TICKETS_PER_DAY)
            .await
            .expect("buy"),
        Some(5)
    );
    assert_eq!(
        PotTicket::holders(&*tx, pot.id)
            .await
            .expect("holders")
            .len(),
        2
    );

    // A new day: yesterday's tickets are still held but no longer count, so
    // the same player can buy the whole cap again. Only a test may move a
    // ticket's clock, and only against the table this test is exercising.
    tx.execute(
        "UPDATE pot_tickets SET created = created - interval '1 day'
         WHERE pot_id = $1 AND user_id = $2",
        &[&pot.id, &buyer.id],
    )
    .await
    .expect("age the tickets");
    assert_eq!(
        PotTicket::user_total_today(&*tx, pot.id, buyer.id)
            .await
            .expect("today"),
        0
    );
    let aged = PotTicket::holders(&*tx, pot.id)
        .await
        .expect("holders")
        .into_iter()
        .find(|holder| holder.user_id == buyer.id)
        .expect("the buyer still holds");
    assert_eq!(
        (aged.tickets, aged.bought_today),
        (POT_MAX_TICKETS_PER_DAY, 0),
        "the holding is the whole pot, today's part is what the cap counts"
    );
    assert_eq!(
        PotTicket::buy_in_tx(
            &tx,
            pot.id,
            buyer.id,
            POT_MAX_TICKETS_PER_DAY,
            POT_MAX_TICKETS_PER_DAY
        )
        .await
        .expect("buy"),
        Some(2 * POT_MAX_TICKETS_PER_DAY),
        "the holding is the whole week; the cap is today"
    );
    tx.commit().await.expect("commit");
}

/// Two replicas sweeping the same pot must produce one payout. The status
/// guard is what decides it: the second UPDATE matches nothing.
#[tokio::test]
async fn only_one_sweeper_settles_a_pot() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let winner = create_test_user(&test_db.db, "pot-settle-winner").await;

    let tx = client.transaction().await.expect("tx");
    let pot = Pot::open_in_tx(&tx, next_draw_at(Utc::now()), POT_TICKET_PRICE)
        .await
        .expect("open");
    PotTicket::buy_in_tx(&tx, pot.id, winner.id, 3, POT_MAX_TICKETS_PER_DAY)
        .await
        .expect("buy");
    let draw = PotDraw {
        winner_user_id: winner.id,
        winner_tickets: 3,
        total_tickets: 3,
        size: 300,
        payout_chips: 240,
    };
    let settled = Pot::settle_drawn_in_tx(&tx, pot.id, &draw)
        .await
        .expect("settle")
        .expect("the first sweeper wins the row");
    assert_eq!(settled.status, PotStatus::Drawn);
    assert_eq!(settled.winner_user_id, Some(winner.id));
    assert_eq!(settled.payout_chips, Some(240));
    assert_eq!(settled.ticket_count, Some(3));

    // The second sweeper's guarded UPDATE matches nothing, whether it tries
    // to pay or to roll.
    assert!(
        Pot::settle_drawn_in_tx(&tx, pot.id, &draw)
            .await
            .expect("settle again")
            .is_none()
    );
    assert!(
        Pot::settle_rolled_in_tx(&tx, pot.id)
            .await
            .expect("roll")
            .is_none()
    );
    tx.commit().await.expect("commit");

    // And the next pot can open beside the settled one: "at most one open"
    // is what the index guarantees, not "at most one row".
    let tx = client.transaction().await.expect("tx");
    assert_eq!(Pot::lock_open(&tx).await.expect("lock"), None);
    Pot::open_in_tx(&tx, next_draw_at(Utc::now()), POT_TICKET_PRICE)
        .await
        .expect("open the next pot");
    tx.commit().await.expect("commit");
}

/// The table, not the service, is what guarantees one open pot.
#[tokio::test]
async fn only_one_pot_is_ever_open() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");

    let tx = client.transaction().await.expect("tx");
    Pot::open_in_tx(&tx, next_draw_at(Utc::now()), POT_TICKET_PRICE)
        .await
        .expect("open");
    let beside = Pot::open_in_tx(&tx, next_draw_at(Utc::now()), POT_TICKET_PRICE).await;
    assert!(
        beside.is_err(),
        "a second open pot must be rejected by the table"
    );
}

/// The winner banner only reaches a second replica over Postgres, so the
/// draw transaction must emit on the channel, and only on commit.
#[tokio::test]
async fn a_draw_notifies_every_replica() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let winner = create_test_user(&test_db.db, "pot-notify-winner").await;
    let change = PotChange::Drawn {
        winner_user_id: winner.id,
        payout_chips: 4_000,
        winner_tickets: 40,
        total_tickets: 50,
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
    let listen = listen_for_pot_changes(&listener);
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

    // A rolled-back draw must tell nobody.
    let rolled_back = client.transaction().await.expect("tx");
    Pot::notify_changed(&rolled_back, &change)
        .await
        .expect("notify");
    drop(rolled_back);

    let tx = client.transaction().await.expect("tx");
    Pot::notify_changed(&tx, &change).await.expect("notify");
    tx.commit().await.expect("commit");

    let mut seen = 0usize;
    while seen == 0 {
        let notification = tokio::time::timeout(
            Duration::from_secs(5),
            poll_fn(|cx| connection.poll_message(cx)),
        )
        .await
        .expect("pot notification")
        .expect("connection open")
        .expect("connection ok");
        if let AsyncMessage::Notification(notification) = notification
            && notification.channel() == POT_CHANGED_CHANNEL
        {
            seen += 1;
            assert_eq!(
                PotChange::parse(notification.payload()).expect("payload parses"),
                change,
                "the payload is what the winner's replica needs"
            );
        }
    }
    assert_eq!(seen, 1, "only the committed draw notifies");
}
