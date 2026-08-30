use std::future::poll_fn;
use std::time::Duration;

use tokio_postgres::{AsyncMessage, NoTls};

use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_gild::{
            CHAT_MESSAGE_GILDED_CHANNEL, ChatMessageGild, ChatMessageGildSummary, GildCounts,
            GildPlacement, GildTier, listen_for_gild_changes, parse_gilded_payload,
        },
        chat_room::ChatRoom,
    },
    test_utils::{create_test_user, test_db},
};

#[test]
fn gilded_payload_round_trips() {
    let message_id = uuid::Uuid::now_v7();
    let room_id = uuid::Uuid::now_v7();
    assert_eq!(
        parse_gilded_payload(&format!("{message_id}:{room_id}")),
        Some((message_id, room_id))
    );
    assert_eq!(parse_gilded_payload("nonsense"), None);
    assert_eq!(parse_gilded_payload(&format!("{message_id}:nope")), None);
}

/// The marker only reaches a second replica over Postgres, so the gild
/// transaction must actually emit on the channel, and only on commit.
/// A placement the test expects to land (a first gild or a raise); anything
/// else is the test's failure, named.
async fn place(
    tx: &tokio_postgres::Transaction<'_>,
    message_id: uuid::Uuid,
    author_id: uuid::Uuid,
    buyer_id: uuid::Uuid,
    tier: GildTier,
) -> ChatMessageGild {
    match ChatMessageGild::place_in_tx(tx, message_id, author_id, buyer_id, tier)
        .await
        .expect("place")
    {
        GildPlacement::Placed(gild) | GildPlacement::Upgraded { gild, .. } => gild,
        refused @ (GildPlacement::SameTier | GildPlacement::HeldHigher(_)) => {
            panic!("expected the gild to land, got {refused:?}")
        }
    }
}

#[tokio::test]
async fn gilding_notifies_every_replica() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let author = create_test_user(&test_db.db, "gild-notify-author").await;
    let buyer = create_test_user(&test_db.db, "gild-notify-buyer").await;
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "notify me".to_string(),
        },
    )
    .await
    .expect("message");

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
    let listen = listen_for_gild_changes(&listener);
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

    // A rolled-back gild must tell nobody, so this transaction is dropped
    // without committing before the one that counts.
    let rolled_back = client.transaction().await.expect("tx");
    place(
        &rolled_back,
        message.id,
        author.id,
        buyer.id,
        GildTier::Bronze,
    )
    .await;
    ChatMessageGild::notify_gilded(&rolled_back, message.id, room.id)
        .await
        .expect("notify");
    drop(rolled_back);

    let tx = client.transaction().await.expect("tx");
    place(&tx, message.id, author.id, buyer.id, GildTier::Gold).await;
    ChatMessageGild::notify_gilded(&tx, message.id, room.id)
        .await
        .expect("notify");
    tx.commit().await.expect("commit");

    let mut payloads = Vec::new();
    while payloads.is_empty() {
        let notification = tokio::time::timeout(
            Duration::from_secs(5),
            poll_fn(|cx| connection.poll_message(cx)),
        )
        .await
        .expect("gild notification")
        .expect("connection open")
        .expect("connection ok");
        if let AsyncMessage::Notification(notification) = notification
            && notification.channel() == CHAT_MESSAGE_GILDED_CHANNEL
        {
            payloads.push(notification.payload().to_string());
        }
    }
    assert_eq!(payloads.len(), 1, "only the committed gild notifies");
    assert_eq!(
        parse_gilded_payload(&payloads[0]),
        Some((message.id, room.id))
    );
}

/// The prices, the split, and the markers are the product. They are quoted in
/// SHOP.md and in the help copy, so a change here is a decision, not a
/// refactor.
#[test]
fn tier_roster() {
    assert_eq!(
        GildTier::ALL,
        &[GildTier::Bronze, GildTier::Silver, GildTier::Gold]
    );
    assert_eq!(
        GildTier::ALL.iter().map(|t| t.price()).collect::<Vec<_>>(),
        [500, 2_000, 10_000]
    );
    assert_eq!(
        GildTier::ALL
            .iter()
            .map(|t| t.author_share())
            .collect::<Vec<_>>(),
        [333, 1_333, 6_666]
    );
    assert_eq!(
        GildTier::ALL.iter().map(|t| t.burn()).collect::<Vec<_>>(),
        [167, 667, 3_334]
    );
    assert_eq!(
        GildTier::ALL.iter().map(|t| t.marker()).collect::<Vec<_>>(),
        ["◆", "◆◆", "◆◆◆"]
    );
    for tier in GildTier::ALL {
        assert_eq!(GildTier::from_rank(tier.rank()), Some(*tier));
        assert_eq!(tier.author_share() + tier.burn(), tier.price());
    }
    assert_eq!(GildTier::from_rank(0), None);
    assert_eq!(GildTier::from_rank(4), None);
    // "Highest tier the message holds" reads this ordering.
    assert!(GildTier::Gold > GildTier::Silver && GildTier::Silver > GildTier::Bronze);
}

/// One slot per buyer per message, and it only ever goes up: a raise
/// rewrites the row (tier and what was paid last), the same tier and a lower
/// tier are refusals, and the marker counts buyers, not purchases.
#[tokio::test]
async fn a_buyers_gild_only_ever_goes_up() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let author = create_test_user(&test_db.db, "gild-up-author").await;
    let buyer = create_test_user(&test_db.db, "gild-up-buyer").await;
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "worth paying for".to_string(),
        },
    )
    .await
    .expect("message");

    let tx = client.transaction().await.expect("tx");
    let silver =
        match ChatMessageGild::place_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Silver)
            .await
            .expect("place silver")
        {
            GildPlacement::Placed(gild) => gild,
            other => panic!("a first gild is placed, got {other:?}"),
        };
    assert_eq!(silver.tier, GildTier::Silver);
    assert_eq!(silver.chips, GildTier::Silver.price());

    let repeat =
        ChatMessageGild::place_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Silver)
            .await
            .expect("place repeat");
    assert!(matches!(repeat, GildPlacement::SameTier), "{repeat:?}");

    let lower =
        ChatMessageGild::place_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Bronze)
            .await
            .expect("place lower");
    assert!(
        matches!(lower, GildPlacement::HeldHigher(GildTier::Silver)),
        "{lower:?}"
    );

    let gold =
        match ChatMessageGild::place_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Gold)
            .await
            .expect("place gold")
        {
            GildPlacement::Upgraded { from, gild } => {
                assert_eq!(from, GildTier::Silver);
                gild
            }
            other => panic!("a higher tier raises the row, got {other:?}"),
        };
    assert_eq!(gold.id, silver.id, "the raise rewrites the same row");
    assert_eq!(gold.tier, GildTier::Gold);
    assert_eq!(gold.chips, GildTier::Gold.price());
    assert_eq!(
        ChatMessageGild::count_for_message(&tx, message.id)
            .await
            .expect("count"),
        1,
        "one buyer is one gild however many times they raise it"
    );
    tx.commit().await.expect("commit");

    let summary = ChatMessageGild::summary_for_message(&client, message.id)
        .await
        .expect("summary")
        .expect("gilded message has a marker");
    assert_eq!(
        summary,
        ChatMessageGildSummary {
            top_tier: GildTier::Gold,
            count: 1,
        }
    );
}

/// The no-self-gild rule is a table constraint, not a service promise: even a
/// direct insert cannot mark your own message.
#[tokio::test]
async fn self_gild_is_refused_by_the_table() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let author = create_test_user(&test_db.db, "gild-self-author").await;
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "gilding myself".to_string(),
        },
    )
    .await
    .expect("message");

    let tx = client.transaction().await.expect("tx");
    let result =
        ChatMessageGild::place_in_tx(&tx, message.id, author.id, author.id, GildTier::Bronze).await;
    assert!(result.is_err(), "the check constraint refuses a self-gild");
}

/// The profile count is scoped to its owner in the query, and an ungilded
/// author reads as zeroes rather than as an absent row.
#[tokio::test]
async fn counts_are_scoped_to_the_author() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let author = create_test_user(&test_db.db, "gild-count-author").await;
    let other = create_test_user(&test_db.db, "gild-count-other").await;
    let buyer_a = create_test_user(&test_db.db, "gild-count-buyer-a").await;
    let buyer_b = create_test_user(&test_db.db, "gild-count-buyer-b").await;

    let ours = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "ours".to_string(),
        },
    )
    .await
    .expect("message");
    let theirs = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: other.id,
            body: "theirs".to_string(),
        },
    )
    .await
    .expect("message");

    let tx = client.transaction().await.expect("tx");
    for (buyer, tier) in [
        (buyer_a.id, GildTier::Bronze),
        (buyer_b.id, GildTier::Bronze),
        (buyer_a.id, GildTier::Gold),
    ] {
        place(&tx, ours.id, author.id, buyer, tier).await;
    }
    place(&tx, theirs.id, other.id, buyer_a.id, GildTier::Silver).await;
    tx.commit().await.expect("commit");

    let listed = ChatMessageGild::list_for_message(&client, ours.id)
        .await
        .expect("list");
    assert_eq!(
        listed
            .iter()
            .map(|gild| (gild.user_id, gild.tier))
            .collect::<Vec<_>>(),
        [(buyer_a.id, GildTier::Gold), (buyer_b.id, GildTier::Bronze)],
        "best tier first, one row per buyer"
    );

    let counts = ChatMessageGild::counts_for_author(&client, author.id)
        .await
        .expect("counts");
    assert_eq!(
        counts,
        GildCounts {
            bronze: 1,
            silver: 0,
            gold: 1,
        },
        "buyer_a's raise moved their gild from bronze to gold"
    );
    assert_eq!(counts.total(), 2);
    assert_eq!(counts.get(GildTier::Gold), 1);

    let none = create_test_user(&test_db.db, "gild-count-nobody").await;
    let empty = ChatMessageGild::counts_for_author(&client, none.id)
        .await
        .expect("counts");
    assert!(empty.is_empty());
}

/// The marker query is one pass per page: ungilded messages are absent, and
/// the summary is per message rather than per room.
#[tokio::test]
async fn summaries_cover_a_page_of_messages() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let author = create_test_user(&test_db.db, "gild-page-author").await;
    let buyer = create_test_user(&test_db.db, "gild-page-buyer").await;

    let mut ids = Vec::new();
    for body in ["one", "two", "three"] {
        let message = ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: author.id,
                body: body.to_string(),
            },
        )
        .await
        .expect("message");
        ids.push(message.id);
    }

    let tx = client.transaction().await.expect("tx");
    place(&tx, ids[0], author.id, buyer.id, GildTier::Silver).await;
    place(&tx, ids[2], author.id, buyer.id, GildTier::Bronze).await;
    tx.commit().await.expect("commit");

    let summaries = ChatMessageGild::list_summaries_for_messages(&client, &ids)
        .await
        .expect("summaries");
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[&ids[0]].top_tier, GildTier::Silver);
    assert_eq!(summaries[&ids[2]].top_tier, GildTier::Bronze);
    assert!(!summaries.contains_key(&ids[1]));

    assert!(
        ChatMessageGild::list_summaries_for_messages(&client, &[])
            .await
            .expect("empty page")
            .is_empty()
    );
}
