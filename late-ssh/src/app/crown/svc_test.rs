//! Service integration tests for the crown against a real ephemeral DB.
//!
//! The transaction is where every acceptance rule in SHOP.md phase 3 lands:
//! the ladder charges what it says, a refusal never touches the ledger, and
//! two racing takes settle to one debit.

use late_core::models::{
    chips::{CHIP_FLOOR, ChipMove, INITIAL_CHIP_BALANCE, UserChips},
    crown::{CROWN_MIN_PRICE, CrownReign},
};
use late_core::test_utils::{create_test_user, expire_crown_hold, roll_crown_reigns_back_a_month};
use uuid::Uuid;

use crate::app::crown::svc::{
    CrownError, CrownRefusal, CrownService, CrownStatus, CrownStatusHolder,
};
use crate::test_helpers::new_test_db;

/// The ladder starts at 5,000 and every rung is 1.5x the last: 5,000 /
/// 7,500 / 11,250 / 16,875 / 25,313 / 37,970. Six takes need six wallets big
/// enough to pay their rung.
const LADDER: [i64; 6] = [5_000, 7_500, 11_250, 16_875, 25_313, 37_970];

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

/// The one line `/crown` prints, in its three shapes. It is the only place
/// the price is quoted to a player, so the wording is pinned here.
#[test]
fn the_status_line_reads_the_three_shapes_the_crown_has() {
    let vacant = CrownStatus {
        holder: None,
        price: CROWN_MIN_PRICE,
    };
    assert_eq!(
        vacant.line(),
        "The crown is vacant. /crown take claims it for 5,000 chips."
    );

    let held = CrownStatus {
        holder: Some(CrownStatusHolder {
            username: "mira".to_string(),
            is_you: false,
            held_for_secs: 12 * 60,
            hold_remaining_secs: 18 * 60,
        }),
        price: 25_313,
    };
    assert_eq!(
        held.line(),
        "mira has worn the crown for 12m. Takeable in 18m for 25,313 chips."
    );

    let takeable = CrownStatus {
        holder: Some(CrownStatusHolder {
            username: "mira".to_string(),
            is_you: true,
            held_for_secs: 3 * 3_600 + 12 * 60,
            hold_remaining_secs: 0,
        }),
        price: 37_970,
    };
    assert_eq!(
        takeable.line(),
        "You have worn the crown for 3h12m. /crown take costs 37,970 chips."
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
        UserChips::apply(
            &**client,
            user.id,
            ChipMove::Credit,
            price + CHIP_FLOOR,
            None,
        )
        .await
        .expect("stake the taker");
        takers.push(user);
    }

    let mut paid = Vec::new();
    for (index, taker) in takers.iter().enumerate() {
        if index > 0 {
            // Each rung is a real takeover, so the previous reign has to be
            // out of its hold window first.
            expire_crown_hold(&client).await;
        }
        let outcome = service.take(taker.id).await.expect("take the crown");
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
    assert_eq!(reign.paid_chips, 37_970);
}

/// Both no-op takes cost nothing: taking a crown you already wear, and
/// taking one whose reign is still inside its 30 minute hold.
#[tokio::test]
async fn a_self_take_and_a_held_reign_are_refused_uncharged() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let holder = create_test_user(&test_db.db, "crown-hold-holder").await;
    let challenger = create_test_user(&test_db.db, "crown-hold-challenger").await;
    for user in [&holder, &challenger] {
        UserChips::apply(&**client, user.id, ChipMove::Credit, 100_000, None)
            .await
            .expect("stake");
    }

    service.take(holder.id).await.expect("first take");
    let after_first = balance(&client, holder.id).await;

    match service.take(holder.id).await {
        Err(CrownError::Refused(CrownRefusal::AlreadyYours)) => {}
        other => panic!("expected a self-take refusal, got {other:?}"),
    }
    match service.take(challenger.id).await {
        Err(CrownError::Refused(CrownRefusal::Held { remaining_secs })) => {
            assert!(remaining_secs > 0, "a held reign must say how long is left");
        }
        other => panic!("expected a hold refusal, got {other:?}"),
    }

    assert_eq!(balance(&client, holder.id).await, after_first);
    assert_eq!(
        crown_debits(&client, holder.id).await,
        vec![-CROWN_MIN_PRICE]
    );
    assert!(crown_debits(&client, challenger.id).await.is_empty());
    // And the refusals left the reign exactly where it was.
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
    let before = balance(&client, pauper.id).await;
    assert!(before < CROWN_MIN_PRICE, "the fixture must be too poor");

    match service.take(pauper.id).await {
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
        UserChips::apply(&**client, user.id, ChipMove::Credit, 100_000, None)
            .await
            .expect("stake");
    }

    service.take(outgoing.id).await.expect("first take");
    expire_crown_hold(&client).await;
    let dear = service.take(incoming.id).await.expect("second take");
    assert_eq!(dear.price, 7_500);

    roll_crown_reigns_back_a_month(&client).await;

    // Last month's holder is nobody now, so this is a vacant claim at the
    // minimum, not a 1.5x takeover, and #lounge is told nobody was deposed.
    let fresh = service
        .take(outgoing.id)
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

/// Two takes racing for the same vacant crown settle to exactly one debit
/// and one reign: the loser is told the crown is held, and pays nothing.
#[tokio::test]
async fn two_concurrent_takes_settle_to_one_debit() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = CrownService::new(test_db.db.clone());
    let first = create_test_user(&test_db.db, "crown-concurrent-first").await;
    let second = create_test_user(&test_db.db, "crown-concurrent-second").await;
    for user in [&first, &second] {
        UserChips::apply(&**client, user.id, ChipMove::Credit, 100_000, None)
            .await
            .expect("stake");
    }

    let left = tokio::spawn({
        let service = service.clone();
        let id = first.id;
        async move { service.take(id).await }
    });
    let right = tokio::spawn({
        let service = service.clone();
        let id = second.id;
        async move { service.take(id).await }
    });
    let outcomes = [left.await.expect("task"), right.await.expect("task")];

    let winners: Vec<_> = outcomes.iter().filter(|outcome| outcome.is_ok()).collect();
    assert_eq!(winners.len(), 1, "exactly one take may land: {outcomes:?}");
    for outcome in &outcomes {
        if let Err(CrownError::Failed(error)) = outcome {
            panic!("a losing take must be a refusal, not a failure: {error:?}");
        }
    }

    let debits = [
        crown_debits(&client, first.id).await,
        crown_debits(&client, second.id).await,
    ];
    let paid: Vec<i64> = debits.iter().flatten().copied().collect();
    assert_eq!(paid, vec![-CROWN_MIN_PRICE], "one debit, at the minimum");

    let reigns: i64 = client
        .query_one("SELECT COUNT(*)::bigint AS count FROM crown_reigns", &[])
        .await
        .expect("count reigns")
        .get("count");
    assert_eq!(reigns, 1, "one reign, however many takes raced for it");
}
