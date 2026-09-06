use crate::models::chips::*;
use crate::test_utils::{create_test_user, test_db};
use std::collections::HashSet;
use std::future::poll_fn;
use std::time::Duration;
use tokio_postgres::{AsyncMessage, NoTls};
use uuid::Uuid;

/// Gifting must notify `chip_user_changed` for both parties, or their chip
/// counters go stale until the next leaderboard refresh.
#[tokio::test]
async fn transfer_gift_notifies_both_parties() {
    let test_db = test_db().await;
    let sender = create_test_user(&test_db.db, "gift-notify-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-notify-recipient").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");

    // Listen on a dedicated connection, started only after both chip rows
    // exist, so the only notifications observed come from the transfer.
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
    let listen_sql = format!("LISTEN {CHIP_USER_CHANGED_CHANNEL};");
    let listen = listener.batch_execute(&listen_sql);
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

    let tx = client.transaction().await.expect("gift transaction");
    UserChips::transfer_gift(&tx, sender.id, recipient.id, 300)
        .await
        .expect("gift succeeds")
        .expect("sender can afford gift");
    tx.commit().await.expect("gift commit");

    let mut notified: HashSet<Uuid> = HashSet::new();
    let expected: HashSet<Uuid> = [sender.id, recipient.id].into();
    while notified != expected {
        let message = tokio::time::timeout(
            Duration::from_secs(5),
            poll_fn(|cx| connection.poll_message(cx)),
        )
        .await
        .expect("chip_user_changed notifications for both gift parties")
        .expect("connection open")
        .expect("connection ok");
        if let AsyncMessage::Notification(notification) = message
            && notification.channel() == CHIP_USER_CHANGED_CHANNEL
        {
            let user_id: Uuid = notification.payload().parse().expect("uuid payload");
            notified.insert(user_id);
        }
    }
}

/// A gild pays the author exactly two thirds of the price and mints nothing
/// for the rest: the ledger pair for one message must sum to minus the burn,
/// which is the only record the burn has.
#[tokio::test]
async fn transfer_gild_burns_the_last_third() {
    use crate::models::chat_message_gild::GildTier;

    let test_db = test_db().await;
    let buyer = create_test_user(&test_db.db, "gild-ledger-buyer").await;
    let author = create_test_user(&test_db.db, "gild-ledger-author").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, buyer.id).await.expect("buyer");
    UserChips::ensure(&client, author.id).await.expect("author");
    // Silver costs more than a starting balance, and its share is the tier
    // where the 2/3 floor division actually rounds.
    let stake = 10_000;
    UserChips::admin_grant(&**client, buyer.id, stake)
        .await
        .expect("stake the buyer");

    let tier = GildTier::Silver;
    let message_id = Uuid::now_v7();
    let tx = client.transaction().await.expect("gild transaction");
    let (buyer_chips, author_chips) = UserChips::transfer_gild(
        &tx,
        buyer.id,
        author.id,
        tier.price(),
        tier.author_share(),
        message_id,
    )
    .await
    .expect("gild succeeds")
    .expect("buyer can afford the tier");
    tx.commit().await.expect("gild commit");

    assert_eq!(
        buyer_chips.balance,
        INITIAL_CHIP_BALANCE + stake - tier.price()
    );
    assert_eq!(
        author_chips.balance,
        INITIAL_CHIP_BALANCE + tier.author_share()
    );
    assert_eq!(tier.author_share(), 1_333);
    assert_eq!(tier.burn(), 667);

    let row = client
        .query_one(
            "SELECT COALESCE(SUM(delta), 0)::bigint AS total
             FROM chip_ledger
             WHERE source_ref = $1",
            &[&message_id.to_string()],
        )
        .await
        .expect("ledger sum");
    let total: i64 = row.get("total");
    assert_eq!(total, -tier.burn());
}

/// Both reward scales hang off the one tier enum; these are the user-facing
/// numbers documented in the Hub guide and CONTEXT.md, so a change here is a
/// product decision, not a refactor.
#[test]
fn difficulty_tiers() {
    assert_eq!(
        Difficulty::ALL,
        &[Difficulty::Easy, Difficulty::Medium, Difficulty::Hard]
    );
    assert_eq!(
        Difficulty::ALL.iter().map(|d| d.key()).collect::<Vec<_>>(),
        ["easy", "medium", "hard"]
    );
    assert_eq!(
        Difficulty::ALL
            .iter()
            .map(|d| d.chips())
            .collect::<Vec<_>>(),
        [100, 250, 500]
    );
    assert_eq!(
        Difficulty::ALL
            .iter()
            .map(|d| d.points())
            .collect::<Vec<_>>(),
        [1, 3, 5]
    );
}

#[test]
fn constants() {
    assert_eq!(CHIP_FLOOR, 100);
    assert_eq!(INITIAL_CHIP_BALANCE, 1_000);
}

/// The earnings exclusion list is derived from the roster, in roster order;
/// only the non-earning moves may appear, and every reason must stay unique
/// so a new variant cannot silently alias an existing ledger reason.
#[test]
fn earning_exclusions_and_reason_uniqueness() {
    // One rule: everything counts, on both sides, except the two house
    // tables and gifts (decided 2026-09-06). So neither folding a poker
    // table to one seat nor funnelling gifts can buy a place; gilds, the
    // pot, spends, Super Snake, the bonsai drip, and the stipend all count.
    // Admin grants are not here because they never reach the ledger.
    assert_eq!(
        ChipMove::excluded_earning_reasons(),
        vec![
            "chip_credit",
            "chip_debit",
            "blackjack_bet",
            "blackjack_payout",
            "poker_bet",
            "poker_payout",
            "floor_restore",
            "chip_gift_sent",
            "chip_gift_received",
        ]
    );
    let reasons: HashSet<&str> = ChipMove::ALL.iter().map(|mv| mv.reason()).collect();
    assert_eq!(reasons.len(), ChipMove::ALL.len());
}

/// `ensure` is the only way a chips row is born, and the stipend it starts
/// with is a ledger row like any other: a user's ledger sums to their
/// balance from the first login. A second call neither pays nor writes.
#[tokio::test]
async fn ensure_writes_the_stipend_once() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chips-stipend").await;
    let client = test_db.db.get().await.expect("db client");

    let first = UserChips::ensure(&client, user.id).await.expect("first");
    let second = UserChips::ensure(&client, user.id).await.expect("second");
    assert_eq!(first.balance, INITIAL_CHIP_BALANCE);
    assert_eq!(second.balance, INITIAL_CHIP_BALANCE);

    let rows = client
        .query(
            "SELECT delta, reason, source_kind, source_ref
             FROM chip_ledger
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("ledger rows");
    assert_eq!(rows.len(), 1, "one stipend row, however many logins");
    assert_eq!(rows[0].get::<_, i64>("delta"), INITIAL_CHIP_BALANCE);
    assert_eq!(
        rows[0].get::<_, &str>("reason"),
        ChipMove::InitialBalance.reason()
    );
    assert_eq!(
        rows[0].get::<_, &str>("source_kind"),
        ChipMove::InitialBalance.source_kind()
    );
    assert_eq!(rows[0].get::<_, &str>("source_ref"), user.id.to_string());
}

/// A gift's two rows each name the other party, so either side of the
/// ledger says who the chips went to or came from.
#[tokio::test]
async fn transfer_gift_rows_name_the_counterparty() {
    let test_db = test_db().await;
    let sender = create_test_user(&test_db.db, "gift-ref-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-ref-recipient").await;
    let mut client = test_db.db.get().await.expect("db client");

    // Neither has logged in: the transfer ensures both rows itself.
    let tx = client.transaction().await.expect("gift transaction");
    UserChips::transfer_gift(&tx, sender.id, recipient.id, 300)
        .await
        .expect("gift succeeds")
        .expect("sender can afford gift");
    tx.commit().await.expect("gift commit");

    let rows = client
        .query(
            "SELECT user_id, delta, reason, source_ref
             FROM chip_ledger
             WHERE reason IN ($1, $2)
             ORDER BY delta",
            &[
                &ChipMove::GiftSent.reason(),
                &ChipMove::GiftReceived.reason(),
            ],
        )
        .await
        .expect("gift rows");
    let rows: Vec<(Uuid, i64, String, String)> = rows
        .into_iter()
        .map(|row| {
            (
                row.get("user_id"),
                row.get("delta"),
                row.get("reason"),
                row.get("source_ref"),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                sender.id,
                -300,
                "chip_gift_sent".to_string(),
                recipient.id.to_string()
            ),
            (
                recipient.id,
                300,
                "chip_gift_received".to_string(),
                sender.id.to_string()
            ),
        ]
    );
}

/// Every ledger row says what it paid for: an empty ref is refused before
/// anything is written, and the retired shared table reasons cannot be
/// written at all.
#[tokio::test]
async fn apply_refuses_empty_refs_and_retired_reasons() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chips-refused").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user.id).await.expect("chips");

    let empty = UserChips::apply(&**client, user.id, ChipMove::SongQueued, 100, "").await;
    assert!(empty.is_err(), "an empty source_ref is not a source");

    for retired in [ChipMove::LegacyTableCredit, ChipMove::LegacyTableDebit] {
        let written = UserChips::apply(&**client, user.id, retired, 100, "old-table").await;
        assert!(written.is_err(), "{retired:?} is retired");
    }

    let balance = UserChips::find(&client, user.id)
        .await
        .expect("find")
        .expect("row")
        .balance;
    assert_eq!(balance, INITIAL_CHIP_BALANCE, "nothing moved");
}

/// The admin grant is the one balance change with no ledger row, by
/// decision: the ledger records what players did.
#[tokio::test]
async fn admin_grant_moves_the_balance_and_writes_nothing() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chips-granted").await;
    let client = test_db.db.get().await.expect("db client");

    let chips = UserChips::admin_grant(&**client, user.id, 5_000)
        .await
        .expect("grant");
    assert_eq!(chips.balance, INITIAL_CHIP_BALANCE + 5_000);

    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint AS rows, COALESCE(SUM(delta), 0)::bigint AS total
             FROM chip_ledger
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("ledger");
    assert_eq!(row.get::<_, i64>("rows"), 1, "only the stipend is written");
    assert_eq!(row.get::<_, i64>("total"), INITIAL_CHIP_BALANCE);
}

/// Two first touches of the same user at once: one transaction holds the
/// stipend insert open, a second `ensure` arrives and waits on it, and must
/// still come back with the row once the first commits. A fallback that
/// reads the statement's pre-wait snapshot sees no row and fails the caller
/// (a login, a gift, a gild) for a user who does have chips.
#[tokio::test]
async fn ensure_survives_a_concurrent_first_insert() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chips-ensure-race").await;
    let mut holder = test_db.db.get().await.expect("holder client");
    let tx = holder.transaction().await.expect("holder tx");
    UserChips::ensure_in(&*tx, user.id)
        .await
        .expect("first insert, uncommitted");

    let db = test_db.db.clone();
    let user_id = user.id;
    let waiter = tokio::spawn(async move {
        let client = db.get().await.expect("waiter client");
        UserChips::ensure(&client, user_id).await
    });

    // Wait until the second ensure is parked on the first's uncommitted row.
    let probe = test_db.db.get().await.expect("probe client");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let blocked: i64 = probe
            .query_one(
                "SELECT COUNT(*) FROM pg_stat_activity
                 WHERE datname = current_database()
                   AND wait_event_type = 'Lock'
                   AND query LIKE '%user_chips%'",
                &[],
            )
            .await
            .expect("probe pg_stat_activity")
            .get(0);
        if blocked > 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the second ensure never blocked on the first"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tx.commit().await.expect("commit first insert");

    let second = waiter
        .await
        .expect("waiter task")
        .expect("the second ensure returns the row the first committed");
    assert_eq!(second.balance, INITIAL_CHIP_BALANCE);

    let stipend_rows: i64 = probe
        .query_one(
            "SELECT COUNT(*) FROM chip_ledger WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("ledger rows")
        .get(0);
    assert_eq!(stipend_rows, 1, "one stipend row, however many first touches");
}
