//! Service integration tests for the crown against a real ephemeral DB.
//!
//! The transaction is where every rule of the crown lands:
//! the ladder charges what it says, a refusal never touches the ledger, and
//! two racing takes each settle to one debit, at successive prices.

use late_core::models::{
    chips::{CHIP_FLOOR, ChipMove, INITIAL_CHIP_BALANCE, UserChips},
    crown::{CROWN_MIN_PRICE, CrownReign},
};
use late_core::test_utils::{create_test_user, roll_crown_reigns_back_a_month};
use uuid::Uuid;

use crate::app::crown::svc::{
    CrownBid, CrownError, CrownEvent, CrownHolder, CrownRefusal, CrownService, CrownStatus,
    CrownStatusHolder, CrownWearers,
};
use crate::test_helpers::new_test_db;

/// The ladder starts at 500 and every rung is 1.5x the last, rounded up:
/// 500 / 750 / 1,125 / 1,688 / 2,532 / 3,798. Six takes need six wallets big
/// enough to pay their rung.
const LADDER: [i64; 6] = [500, 750, 1_125, 1_688, 2_532, 3_798];

/// Every ledger row this user has for the crown, most recent first.
async fn crown_debits(client: &tokio_postgres::Client, user_id: Uuid) -> Vec<i64> {
    client
        .query(
            "SELECT delta FROM chip_ledger
             WHERE user_id = $1 AND reason = $2
             ORDER BY created_at DESC",
            &[&user_id, &ChipMove::CrownTaken.reason()],
        )
        .await
        .expect("crown ledger rows")
        .into_iter()
        .map(|row| row.get::<_, i64>("delta"))
        .collect()
}

async fn balance(client: &tokio_postgres::Client, user_id: Uuid) -> i64 {
    UserChips::find(client, user_id)
        .await
        .expect("chips")
        .map(|chips| chips.balance)
        .unwrap_or(INITIAL_CHIP_BALANCE)
}

/// The one line `/crown` prints, in its two shapes. It is the only place
/// the price is quoted to a player, so the wording is pinned here.
#[test]
fn the_status_line_reads_the_two_shapes_the_crown_has() {
    let vacant = CrownStatus {
        holder: None,
        price: CROWN_MIN_PRICE,
    };
    assert_eq!(
        vacant.line(),
        "The crown is vacant. /crown take claims it for 500 chips, /crown take N bids more."
    );

    let theirs = CrownStatus {
        holder: Some(CrownStatusHolder {
            username: "mira".to_string(),
            is_you: false,
            held_for_secs: 12 * 60,
        }),
        price: 25_313,
    };
    assert_eq!(
        theirs.line(),
        "mira has worn the crown for 12m. /crown take costs 25,313 chips, /crown take N bids more."
    );

    let yours = CrownStatus {
        holder: Some(CrownStatusHolder {
            username: "mira".to_string(),
            is_you: true,
            held_for_secs: 3 * 3_600 + 12 * 60,
        }),
        price: 37_970,
    };
    assert_eq!(
        yours.line(),
        "You have worn the crown for 3h12m. /crown take costs 37,970 chips, /crown take N bids more."
    );
}

/// Six takes from a vacant crown walk the ladder exactly, and every chip is
/// burned: each take is one debit with no credit anywhere, so the ledger's
/// total for the whole contest is minus the sum of the rungs.
#[tokio::test]
async fn six_takes_walk_the_ladder_and_burn_every_chip() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());

    let mut takers = Vec::new();
    for (index, price) in LADDER.iter().enumerate() {
        let user = create_test_user(&test_db.db, &format!("crown-ladder-{index}")).await;
        UserChips::admin_grant(&**client, user.id, price + CHIP_FLOOR)
            .await
            .expect("stake the taker");
        takers.push(user);
    }

    let mut paid = Vec::new();
    for taker in &takers {
        let outcome = service
            .take(taker.id, &taker.username, CrownBid::AtPrice)
            .await
            .expect("take the crown");
        paid.push(outcome.price);
    }
    assert_eq!(paid, LADDER);

    // One debit each, for exactly the rung they paid, and nothing credited
    // back: the burn is the absence of a matching credit reason.
    for (taker, price) in takers.iter().zip(LADDER) {
        assert_eq!(crown_debits(&client, taker.id).await, vec![-price]);
    }
    let reign = CrownReign::find_open(&client)
        .await
        .expect("find open")
        .expect("a reign is open");
    assert_eq!(reign.holder_user_id, takers[5].id);
    assert_eq!(reign.paid_chips, 3_798);
}

/// Taking a crown you already wear costs nothing and changes nothing.
#[tokio::test]
async fn a_self_take_is_refused_uncharged() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let holder = create_test_user(&test_db.db, "crown-self-holder").await;
    UserChips::admin_grant(&**client, holder.id, 100_000)
        .await
        .expect("stake");

    service
        .take(holder.id, &holder.username, CrownBid::AtPrice)
        .await
        .expect("first take");
    let after_first = balance(&client, holder.id).await;

    match service
        .take(holder.id, &holder.username, CrownBid::AtPrice)
        .await
    {
        Err(CrownError::Refused(CrownRefusal::AlreadyYours)) => {}
        other => panic!("expected a self-take refusal, got {other:?}"),
    }

    assert_eq!(balance(&client, holder.id).await, after_first);
    assert_eq!(
        crown_debits(&client, holder.id).await,
        vec![-CROWN_MIN_PRICE]
    );
    // And the refusal left the reign exactly where it was.
    let reign = CrownReign::find_open(&client)
        .await
        .expect("find open")
        .expect("a reign is open");
    assert_eq!(reign.holder_user_id, holder.id);
}

/// A take that would drop the buyer below the chip floor is refused, and the
/// whole transaction rolls back with it: no reign, no ledger row, and the
/// crown stays where it was.
#[tokio::test]
async fn a_take_the_buyer_cannot_afford_changes_nothing() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let pauper = create_test_user(&test_db.db, "crown-pauper").await;
    UserChips::ensure(&client, pauper.id).await.expect("chips");
    // A fresh account can afford a vacant crown, so bet the stake away
    // first: what is left is the floor, and the floor cannot pay.
    UserChips::apply(
        &**client,
        pauper.id,
        ChipMove::BlackjackBet,
        INITIAL_CHIP_BALANCE - CHIP_FLOOR,
        "test-round",
    )
    .await
    .expect("lose the stake");
    let before = balance(&client, pauper.id).await;
    assert!(
        before < CROWN_MIN_PRICE + CHIP_FLOOR,
        "the fixture must be too poor"
    );

    match service
        .take(pauper.id, &pauper.username, CrownBid::AtPrice)
        .await
    {
        Err(CrownError::Refused(CrownRefusal::InsufficientChips { price })) => {
            assert_eq!(price, CROWN_MIN_PRICE);
        }
        other => panic!("expected an affordability refusal, got {other:?}"),
    }

    assert_eq!(balance(&client, pauper.id).await, before);
    assert!(crown_debits(&client, pauper.id).await.is_empty());
    assert_eq!(
        CrownReign::find_open(&client).await.expect("find open"),
        None,
        "a refused take must not leave a reign behind"
    );
}

/// At the UTC month rollover the crown empties: the price drops back to the
/// minimum however much the outgoing holder paid, the stale reign is closed
/// on the way past, and the new one belongs to the new month.
#[tokio::test]
async fn the_month_rollover_empties_the_crown_at_the_minimum() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let outgoing = create_test_user(&test_db.db, "crown-rollover-outgoing").await;
    let incoming = create_test_user(&test_db.db, "crown-rollover-incoming").await;
    for user in [&outgoing, &incoming] {
        UserChips::admin_grant(&**client, user.id, 100_000)
            .await
            .expect("stake");
    }

    service
        .take(outgoing.id, &outgoing.username, CrownBid::AtPrice)
        .await
        .expect("first take");
    let dear = service
        .take(incoming.id, &incoming.username, CrownBid::AtPrice)
        .await
        .expect("second take");
    assert_eq!(dear.price, 750);

    roll_crown_reigns_back_a_month(&client).await;

    // Last month's holder is nobody now, so this is a vacant claim at the
    // minimum, not a 1.5x takeover, and #lounge is told nobody was deposed.
    let fresh = service
        .take(outgoing.id, &outgoing.username, CrownBid::AtPrice)
        .await
        .expect("take after rollover");
    assert_eq!(fresh.price, CROWN_MIN_PRICE);
    assert!(fresh.from.is_none(), "a vacant crown deposes nobody");

    let reign = CrownReign::find_open(&client)
        .await
        .expect("find open")
        .expect("a reign is open");
    assert_eq!(reign.id, fresh.reign_id);
    assert_eq!(reign.holder_user_id, outgoing.id);
    assert_eq!(
        reign.month,
        late_core::models::crown::crown_month(chrono::Utc::now())
    );
}

/// Two takes racing for the same vacant crown both land, in some order: the
/// advisory lock serializes them, so the second reads the reign the first
/// opened and pays the next rung rather than colliding on the unique index
/// or paying the vacant price twice. There is no hold, so nobody is refused;
/// the ladder is the only throttle.
#[tokio::test]
async fn two_concurrent_takes_both_land_and_walk_the_ladder() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let first = create_test_user(&test_db.db, "crown-concurrent-first").await;
    let second = create_test_user(&test_db.db, "crown-concurrent-second").await;
    for user in [&first, &second] {
        UserChips::admin_grant(&**client, user.id, 100_000)
            .await
            .expect("stake");
    }

    let left = tokio::spawn({
        let service = service.clone();
        let id = first.id;
        async move { service.take(id, "racer", CrownBid::AtPrice).await }
    });
    let right = tokio::spawn({
        let service = service.clone();
        let id = second.id;
        async move { service.take(id, "racer", CrownBid::AtPrice).await }
    });
    let outcomes = [left.await.expect("task"), right.await.expect("task")];

    let mut prices: Vec<i64> = outcomes
        .iter()
        .map(|outcome| match outcome {
            Ok(outcome) => outcome.price,
            Err(error) => panic!("both racing takes must land: {error:?}"),
        })
        .collect();
    prices.sort_unstable();
    assert_eq!(
        prices,
        vec![CROWN_MIN_PRICE, 750],
        "vacant, then the next rung"
    );

    let mut paid: Vec<i64> = [
        crown_debits(&client, first.id).await,
        crown_debits(&client, second.id).await,
    ]
    .concat();
    paid.sort_unstable();
    assert_eq!(paid, vec![-750, -CROWN_MIN_PRICE], "one debit each");

    let reigns: Vec<CrownReign> = client
        .query("SELECT * FROM crown_reigns ORDER BY taken_at", &[])
        .await
        .expect("reigns in takeover order")
        .into_iter()
        .map(CrownReign::from)
        .collect();
    assert_eq!(reigns.len(), 2, "two reigns, one closed by the other");
    assert_eq!(reigns[0].paid_chips, CROWN_MIN_PRICE);
    assert!(reigns[1].taken_at > reigns[0].taken_at);
    assert_eq!(reigns[0].ended_at, Some(reigns[1].taken_at));
    assert_eq!(reigns[1].ended_at, None);
    let open = CrownReign::find_open(&client)
        .await
        .expect("find open")
        .expect("a reign is open");
    assert_eq!(open.paid_chips, 750, "the second take holds the crown");
    assert_eq!(open.id, reigns[1].id);
}

/// The glyph and the deposed holder's banner both cross replicas over the
/// `crown_changed` notify, so a service that only listens (a second replica)
/// must end up with the new holder in its watch and a `Deposed` event for the
/// old one, without ever having run the take itself.
#[tokio::test]
async fn a_listening_replica_learns_the_holder_and_tells_the_deposed() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let seller = CrownService::new(test_db.db.clone());
    let other_replica = CrownService::new(test_db.db.clone());
    let mut pg_listener = crate::pg_listener::PgListener::new();
    let _worker = other_replica.start_notify_worker(pg_listener.subscribe(CrownService::CHANNELS));
    let _listener = pg_listener.start(test_db.db.config().clone());
    let mut wearers_rx = other_replica.subscribe_wearers();
    let mut events_rx = other_replica.subscribe_events();

    let first = create_test_user(&test_db.db, "crown-replica-first").await;
    let second = create_test_user(&test_db.db, "crown-replica-second").await;
    for user in [&first, &second] {
        UserChips::admin_grant(&**client, user.id, 100_000)
            .await
            .expect("stake");
    }

    // Whether the LISTEN is live before or after this take, the listener's
    // seed read lands the holder; once it has, the LISTEN is live for the
    // takeover below.
    seller
        .take(first.id, &first.username, CrownBid::AtPrice)
        .await
        .expect("first take");
    let seeded = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let holder = wearers_rx.borrow_and_update().holder(chrono::Utc::now());
            if holder == Some(first.id) {
                return;
            }
            wearers_rx.changed().await.expect("wearers watch open");
        }
    })
    .await;
    seeded.expect("the listening replica seeds the first holder");

    let taken = seller
        .take(second.id, &second.username, CrownBid::AtPrice)
        .await
        .expect("second take");

    let deposed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match events_rx.recv().await.expect("events open") {
                CrownEvent::Deposed {
                    user_id,
                    taker_username,
                    price,
                } => return (user_id, taker_username, price),
                CrownEvent::Status { .. }
                | CrownEvent::Taken { .. }
                | CrownEvent::Failed { .. } => {}
            }
        }
    })
    .await
    .expect("the deposed holder is told on the other replica");
    assert_eq!(deposed, (first.id, second.username.clone(), taken.price));

    let holder = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let wearers = *wearers_rx.borrow_and_update();
            if wearers.holder(chrono::Utc::now()) == Some(second.id) {
                return wearers.open;
            }
            wearers_rx.changed().await.expect("wearers watch open");
        }
    })
    .await
    .expect("the glyph moves on the other replica");
    assert_eq!(
        holder,
        Some(CrownHolder {
            user_id: second.id,
            month: late_core::models::crown::crown_month(chrono::Utc::now()),
        })
    );
}

/// A bid is the price or anything above it, charged in full. A bare take
/// pays the price and nothing more.
#[test]
fn a_bid_charges_itself_at_or_above_the_price_and_nothing_below() {
    assert_eq!(CrownBid::AtPrice.charge(750), Ok(750));
    assert_eq!(CrownBid::Offer(750).charge(750), Ok(750));
    assert_eq!(CrownBid::Offer(10_000).charge(750), Ok(10_000));
    assert_eq!(
        CrownBid::Offer(749).charge(750),
        Err(CrownRefusal::BidTooLow {
            offer: 749,
            price: 750
        })
    );
    assert_eq!(
        CrownRefusal::BidTooLow {
            offer: 1_000,
            price: 1_125
        }
        .message(),
        "A 1,000-chip bid is under the crown's 1,125-chip price"
    );
}

/// An overbid is the tactic: the taker pays the whole bid, the reign records
/// it, and the next rung climbs from the bid rather than from the price. A
/// rival who bids under that rung is refused and charged nothing.
#[tokio::test]
async fn an_overbid_sets_the_next_rung_and_an_underbid_is_refused_uncharged() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let bidder = create_test_user(&test_db.db, "crown-bid-high").await;
    let rival = create_test_user(&test_db.db, "crown-bid-rival").await;
    for user in [&bidder, &rival] {
        UserChips::admin_grant(&**client, user.id, 100_000)
            .await
            .expect("stake");
    }

    let outcome = service
        .take(bidder.id, &bidder.username, CrownBid::Offer(10_000))
        .await
        .expect("an overbid on a vacant crown lands");
    assert_eq!(outcome.price, 10_000);
    assert_eq!(crown_debits(&client, bidder.id).await, vec![-10_000]);

    let status = service.status(rival.id).await.expect("status");
    assert_eq!(status.price, 15_000, "the next rung is 1.5x the bid");

    let rival_before = balance(&client, rival.id).await;
    match service
        .take(rival.id, &rival.username, CrownBid::Offer(14_999))
        .await
    {
        Err(CrownError::Refused(CrownRefusal::BidTooLow { offer, price })) => {
            assert_eq!((offer, price), (14_999, 15_000));
        }
        other => panic!("expected an underbid refusal, got {other:?}"),
    }
    assert_eq!(balance(&client, rival.id).await, rival_before);
    assert!(crown_debits(&client, rival.id).await.is_empty());
    let reign = CrownReign::find_open(&client)
        .await
        .expect("find open")
        .expect("a reign is open");
    assert_eq!(reign.holder_user_id, bidder.id);
    assert_eq!(reign.paid_chips, 10_000);

    let taken = service
        .take(rival.id, &rival.username, CrownBid::AtPrice)
        .await
        .expect("a bare take pays the rung");
    assert_eq!(taken.price, 15_000);
}

/// The holder and the laureate both resolve from `now`, so the rollover
/// moves them with no refresh: at midnight the live holder stops counting
/// and becomes last month's laureate in the same read.
#[test]
fn the_rollover_turns_the_last_holder_into_the_laureate() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let august = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
    let july_winner = Uuid::now_v7();
    let august_holder = Uuid::now_v7();
    let wearers = CrownWearers {
        open: Some(CrownHolder {
            user_id: august_holder,
            month: august,
        }),
        month_ends: [
            Some(CrownHolder {
                user_id: august_holder,
                month: august,
            }),
            Some(CrownHolder {
                user_id: july_winner,
                month: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            }),
        ],
    };

    let late_august = Utc.with_ymd_and_hms(2026, 8, 31, 23, 59, 59).unwrap();
    assert_eq!(wearers.holder(late_august), Some(august_holder));
    assert_eq!(wearers.laureate(late_august), Some(july_winner));

    let first_of_september = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
    assert_eq!(wearers.holder(first_of_september), None);
    assert_eq!(wearers.laureate(first_of_september), Some(august_holder));

    // A month nobody took the crown in leaves the month after it without a
    // laureate, however recent the last reign was.
    let october = Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap();
    assert_eq!(wearers.laureate(october), None);
    assert_eq!(
        CrownWearers::default().laureate(first_of_september),
        None,
        "a crown nobody ever took has no laureate"
    );
}

/// The published wearers come from the table: after a take and a rollover,
/// a fresh read names last month's last holder as the laureate and nobody
/// as the holder.
#[tokio::test]
async fn refreshed_wearers_name_last_months_last_holder() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let first = create_test_user(&test_db.db, "crown-wearers-first").await;
    let last = create_test_user(&test_db.db, "crown-wearers-last").await;
    for user in [&first, &last] {
        UserChips::admin_grant(&**client, user.id, 100_000)
            .await
            .expect("stake");
    }
    service
        .take(first.id, &first.username, CrownBid::AtPrice)
        .await
        .expect("first take");
    service
        .take(last.id, &last.username, CrownBid::AtPrice)
        .await
        .expect("second take");
    roll_crown_reigns_back_a_month(&client).await;

    let rx = service.subscribe_wearers();
    service.refresh_wearers().await.expect("refresh");
    let wearers = *rx.borrow();
    let now = chrono::Utc::now();
    assert_eq!(wearers.holder(now), None, "last month's reign is stale");
    assert_eq!(wearers.laureate(now), Some(last.id));
}
