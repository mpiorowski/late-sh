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

/// The earnings exclusion list is derived from the roster; only the two
/// non-earning moves may appear, and every reason must stay unique so a new
/// variant cannot silently alias an existing ledger reason.
#[test]
fn earning_exclusions_and_reason_uniqueness() {
    assert_eq!(
        ChipMove::excluded_earning_reasons(),
        vec!["floor_restore", "shop_purchase"]
    );
    let reasons: HashSet<&str> = ChipMove::ALL.iter().map(|mv| mv.reason()).collect();
    assert_eq!(reasons.len(), ChipMove::ALL.len());
}
