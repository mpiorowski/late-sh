//! Service integration tests for the pot against a real ephemeral DB.
//!
//! The transaction is where every acceptance rule in SHOP.md phase 5 lands:
//! the cap refuses uncharged, the ledger shows the 20% gap, two sweepers
//! settle one payout, and there is always exactly one open pot afterwards.

use chrono::{Duration, Utc};
use late_core::models::{
    chips::{CHIP_FLOOR, ChipMove, INITIAL_CHIP_BALANCE, UserChips},
    pot::{POT_MAX_TICKETS_PER_DAY, POT_TICKET_PRICE, Pot, PotStatus, next_draw_at},
};
use late_core::test_utils::create_test_user;
use uuid::Uuid;

use crate::app::pot::svc::{PotError, PotRefusal, PotService, PotSettlement, PotStatus as Status};
use crate::test_helpers::new_test_db;

/// Every pot ledger row this user has, most recent first.
async fn pot_ledger(client: &tokio_postgres::Client, user_id: Uuid) -> Vec<i64> {
    client
        .query(
            "SELECT delta FROM chip_ledger
             WHERE user_id = $1 AND reason IN ($2, $3)
             ORDER BY created_at DESC",
            &[
                &user_id,
                &ChipMove::PotTicket.reason(),
                &ChipMove::PotWon.reason(),
            ],
        )
        .await
        .expect("pot ledger rows")
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

/// Move the open pot's draw into the past, so the next sweep settles it. The
/// service always schedules from `next_draw_at`; only a test may fake the
/// hour, and only through here.
async fn make_the_pot_due(client: &tokio_postgres::Client) {
    let updated = client
        .execute(
            "UPDATE pots SET draws_at = current_timestamp - interval '1 minute'
             WHERE status = 'open'",
            &[],
        )
        .await
        .expect("make the pot due");
    assert_eq!(updated, 1, "exactly one pot must be open");
}

/// The one line `/pot` prints, in its shapes. It is the only place the pot is
/// quoted to a player, so the wording is pinned here.
#[test]
fn the_status_line_reads_the_shapes_the_pot_has() {
    let holding = Status {
        size: 84_200,
        ticket_count: 842,
        my_tickets: 5,
        room_today: 5,
        ticket_price: POT_TICKET_PRICE,
        draws_in_secs: Some(4 * 86_400 + 12 * 3_600),
    };
    assert_eq!(
        holding.line(),
        "Pot 84,200 on 842 tickets, you hold 5 (500 chips), 5 more today, draws in 4d12h. /pot buy N at 100 each."
    );

    let empty_handed = Status {
        my_tickets: 0,
        room_today: POT_MAX_TICKETS_PER_DAY,
        ..holding
    };
    assert_eq!(
        empty_handed.line(),
        "Pot 84,200 on 842 tickets, you hold none, 10 more today, draws in 4d12h. /pot buy N at 100 each."
    );

    let capped_for_today = Status {
        my_tickets: 24,
        room_today: 0,
        ..holding
    };
    assert_eq!(
        capped_for_today.line(),
        "Pot 84,200 on 842 tickets, you hold 24 (2,400 chips), none more today, draws in 4d12h. /pot buy N at 100 each."
    );

    let before_the_first_sweep = Status {
        size: 0,
        ticket_count: 0,
        my_tickets: 0,
        room_today: 0,
        ticket_price: 0,
        draws_in_secs: None,
    };
    assert_eq!(before_the_first_sweep.line(), "The pot has not opened yet.");
}

/// The first sweep opens a pot, and every session sees it: the snapshot is
/// what the panel and `/pot` both read.
#[tokio::test]
async fn the_first_sweep_opens_a_pot() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());
    let watcher = create_test_user(&test_db.db, "pot-first-watcher").await;

    assert!(service.settle_due().await.expect("sweep").is_none());
    service.refresh().await.expect("refresh");

    let pot = Pot::find_open(&**client)
        .await
        .expect("find open")
        .expect("a pot is open");
    assert_eq!(pot.status, PotStatus::Open);
    assert_eq!(pot.ticket_price, POT_TICKET_PRICE);
    assert_eq!(pot.draws_at, next_draw_at(pot.opens_at));

    let status = service.status_for(watcher.id);
    assert_eq!(status.size, 0);
    assert_eq!(status.ticket_count, 0);
    assert_eq!(status.my_tickets, 0);
    assert!(status.draws_in_secs.is_some());
}

/// A buy writes one ledger row for the whole buy, and the pot's size is its
/// tickets: no stored total, no house wallet.
#[tokio::test]
async fn a_buy_debits_once_and_grows_the_pot() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());
    let buyer = create_test_user(&test_db.db, "pot-buyer").await;
    UserChips::apply(&**client, buyer.id, ChipMove::Credit, 10_000, None)
        .await
        .expect("stake the buyer");
    let before = balance(&client, buyer.id).await;

    service.settle_due().await.expect("open the first pot");
    let outcome = service.buy(buyer.id, 3).await.expect("buy");
    assert_eq!(outcome.tickets, 3);
    assert_eq!(outcome.held, 3);
    assert_eq!(outcome.price, 3 * POT_TICKET_PRICE);
    assert_eq!(outcome.size, 3 * POT_TICKET_PRICE);

    // One row for the buy, not one per ticket.
    assert_eq!(pot_ledger(&client, buyer.id).await, vec![-300]);
    assert_eq!(balance(&client, buyer.id).await, before - 300);

    // A second buy stacks on the holding.
    let outcome = service.buy(buyer.id, 2).await.expect("buy again");
    assert_eq!(outcome.held, 5);
    service.refresh().await.expect("refresh");
    let status = service.status_for(buyer.id);
    assert_eq!(status.my_tickets, 5);
    assert_eq!(status.ticket_count, 5);
    assert_eq!(status.size, 500);
}

/// Today's cap is a refusal, not an error, and a refused buy costs nothing:
/// no ticket row and no ledger row.
#[tokio::test]
async fn a_buy_past_the_daily_cap_is_refused_uncharged() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());
    let buyer = create_test_user(&test_db.db, "pot-cap").await;
    UserChips::apply(&**client, buyer.id, ChipMove::Credit, 100_000, None)
        .await
        .expect("stake the buyer");

    service.settle_due().await.expect("open the first pot");
    service
        .buy(buyer.id, POT_MAX_TICKETS_PER_DAY)
        .await
        .expect("buy to the cap");
    let after_the_cap = balance(&client, buyer.id).await;

    match service.buy(buyer.id, 1).await {
        Err(PotError::Refused(PotRefusal::CapReached { bought_today })) => {
            assert_eq!(bought_today, POT_MAX_TICKETS_PER_DAY);
        }
        other => panic!("expected a cap refusal, got {other:?}"),
    }
    assert_eq!(balance(&client, buyer.id).await, after_the_cap);
    assert_eq!(
        pot_ledger(&client, buyer.id).await,
        vec![-POT_MAX_TICKETS_PER_DAY * POT_TICKET_PRICE],
        "the refused buy wrote no ledger row"
    );
}

/// A buy that would drop the buyer below the chip floor is refused, and the
/// whole transaction rolls back with it: no ticket, no ledger row.
#[tokio::test]
async fn a_buy_the_player_cannot_afford_changes_nothing() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());
    let pauper = create_test_user(&test_db.db, "pot-pauper").await;
    UserChips::ensure(&client, pauper.id).await.expect("chips");
    UserChips::apply(
        &**client,
        pauper.id,
        ChipMove::Bet,
        INITIAL_CHIP_BALANCE - CHIP_FLOOR,
        None,
    )
    .await
    .expect("lose the stake");

    service.settle_due().await.expect("open the first pot");
    match service.buy(pauper.id, 5).await {
        Err(PotError::Refused(PotRefusal::InsufficientChips { price })) => {
            assert_eq!(price, 5 * POT_TICKET_PRICE);
        }
        other => panic!("expected an affordability refusal, got {other:?}"),
    }
    assert_eq!(balance(&client, pauper.id).await, CHIP_FLOOR);
    assert!(pot_ledger(&client, pauper.id).await.is_empty());

    let pot = Pot::find_open(&**client)
        .await
        .expect("find open")
        .expect("a pot is open");
    let tickets = client
        .query("SELECT id FROM pot_tickets WHERE pot_id = $1", &[&pot.id])
        .await
        .expect("tickets");
    assert!(tickets.is_empty(), "a refused buy leaves no ticket behind");
}

/// The payout math, end to end: the winner receives four fifths of what the
/// tickets paid in, and the ledger's total across everyone is the burn.
#[tokio::test]
async fn the_draw_pays_four_fifths_and_burns_the_rest() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());

    let mut buyers = Vec::new();
    for index in 0..3 {
        let buyer = create_test_user(&test_db.db, &format!("pot-draw-{index}")).await;
        UserChips::apply(&**client, buyer.id, ChipMove::Credit, 10_000, None)
            .await
            .expect("stake");
        buyers.push(buyer);
    }

    service.settle_due().await.expect("open the first pot");
    for buyer in &buyers {
        service.buy(buyer.id, 10).await.expect("buy");
    }
    let size = 30 * POT_TICKET_PRICE;

    make_the_pot_due(&client).await;
    let settlement = service
        .settle_due()
        .await
        .expect("draw")
        .expect("the due pot settles");
    let PotSettlement::Drawn { pot_id, draw } = settlement else {
        panic!("a pot with tickets in it must draw, not roll");
    };
    assert_eq!(draw.total_tickets, 30);
    assert_eq!(draw.winner_tickets, 10);
    assert_eq!(draw.size, size);
    assert_eq!(draw.payout_chips, size * 4 / 5);
    assert!(buyers.iter().any(|buyer| buyer.id == draw.winner_user_id));

    // The winner's own rows: three hundred out for the tickets, the payout
    // back in. Everyone else only paid.
    let mut ledger_total = 0;
    for buyer in &buyers {
        let rows = pot_ledger(&client, buyer.id).await;
        ledger_total += rows.iter().sum::<i64>();
        match buyer.id == draw.winner_user_id {
            true => assert_eq!(rows, vec![draw.payout_chips, -1_000]),
            false => assert_eq!(rows, vec![-1_000]),
        }
    }
    assert_eq!(
        ledger_total,
        draw.payout_chips - size,
        "the gap between what went in and what came out is the burn"
    );

    // The settled row is the record of what happened, and the next pot is
    // already open behind it.
    let settled = client
        .query_one("SELECT * FROM pots WHERE id = $1", &[&pot_id])
        .await
        .expect("the settled pot");
    assert_eq!(settled.get::<_, String>("status"), "drawn");
    assert_eq!(settled.get::<_, i64>("ticket_count"), 30);
    assert_eq!(settled.get::<_, i64>("payout_chips"), draw.payout_chips);
    let open = Pot::find_open(&**client)
        .await
        .expect("find open")
        .expect("the next pot is open");
    assert_ne!(open.id, pot_id);
    assert!(open.draws_at > Utc::now() + Duration::hours(1));
}

/// Two replicas sweeping the same due pot: one pays, the other finds the
/// successor and does nothing. The winner is credited exactly once.
#[tokio::test]
async fn two_sweepers_produce_one_payout() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let first = PotService::new(test_db.db.clone());
    let second = PotService::new(test_db.db.clone());
    let buyer = create_test_user(&test_db.db, "pot-race-buyer").await;
    UserChips::apply(&**client, buyer.id, ChipMove::Credit, 10_000, None)
        .await
        .expect("stake");

    first.settle_due().await.expect("open the first pot");
    first.buy(buyer.id, 4).await.expect("buy");
    make_the_pot_due(&client).await;

    let (a, b) = tokio::join!(first.settle_due(), second.settle_due());
    let settled: Vec<_> = [a.expect("first sweep"), b.expect("second sweep")]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(settled.len(), 1, "exactly one sweeper settles the pot");

    assert_eq!(
        pot_ledger(&client, buyer.id).await,
        vec![320, -400],
        "the winner is credited once"
    );
    let pots = client
        .query("SELECT status FROM pots ORDER BY opens_at", &[])
        .await
        .expect("pots");
    let statuses: Vec<String> = pots.iter().map(|row| row.get("status")).collect();
    assert_eq!(statuses, vec!["drawn".to_string(), "open".to_string()]);
}

/// Nobody bought in: the pot rolls, nothing is paid, and the next one opens
/// behind it, so `/pot` never has to answer "there isn't one".
#[tokio::test]
async fn an_empty_pot_rolls_and_the_next_one_opens() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());

    service.settle_due().await.expect("open the first pot");
    make_the_pot_due(&client).await;
    let settlement = service
        .settle_due()
        .await
        .expect("roll")
        .expect("the due pot settles");
    let PotSettlement::Rolled { pot_id } = settlement else {
        panic!("an empty pot must roll, not draw");
    };

    let rolled = client
        .query_one("SELECT * FROM pots WHERE id = $1", &[&pot_id])
        .await
        .expect("the rolled pot");
    assert_eq!(rolled.get::<_, String>("status"), "rolled");
    assert_eq!(rolled.get::<_, i64>("payout_chips"), 0);
    assert!(rolled.get::<_, Option<Uuid>>("winner_user_id").is_none());

    let open = Pot::find_open(&**client)
        .await
        .expect("find open")
        .expect("the next pot is open");
    assert_ne!(open.id, pot_id);
}

/// A pot that is not due yet is left alone: the sweeper runs every minute and
/// must not draw the pot 23 hours early.
#[tokio::test]
async fn a_pot_that_is_not_due_is_left_alone() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());

    service.settle_due().await.expect("open the first pot");
    let opened = Pot::find_open(&**client)
        .await
        .expect("find open")
        .expect("a pot is open");

    assert!(service.settle_due().await.expect("sweep").is_none());
    let still_open = Pot::find_open(&**client)
        .await
        .expect("find open")
        .expect("the same pot is still open");
    assert_eq!(still_open.id, opened.id);
}

/// One session's snapshot never carries another's holding out to it: the map
/// behind the pot is private, and `tickets_for` is the only door.
#[tokio::test]
async fn a_session_only_reads_its_own_holding() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = PotService::new(test_db.db.clone());
    let mine = create_test_user(&test_db.db, "pot-mine").await;
    let theirs = create_test_user(&test_db.db, "pot-theirs").await;
    for user in [&mine, &theirs] {
        UserChips::apply(&**client, user.id, ChipMove::Credit, 10_000, None)
            .await
            .expect("stake");
    }

    service.settle_due().await.expect("open the first pot");
    service.buy(mine.id, 2).await.expect("buy");
    service.buy(theirs.id, 7).await.expect("buy");
    service.refresh().await.expect("refresh");

    assert_eq!(service.status_for(mine.id).my_tickets, 2);
    assert_eq!(service.status_for(theirs.id).my_tickets, 7);
    // Today's room is the cap less what each bought today, each their own.
    assert_eq!(
        service.status_for(mine.id).room_today,
        POT_MAX_TICKETS_PER_DAY - 2
    );
    assert_eq!(
        service.status_for(theirs.id).room_today,
        POT_MAX_TICKETS_PER_DAY - 7
    );
    // The public half is the same for both: the field is not a secret, the
    // per-player breakdown is.
    assert_eq!(service.status_for(mine.id).ticket_count, 9);
    assert_eq!(service.status_for(theirs.id).ticket_count, 9);
}
