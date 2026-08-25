use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_gild::{ChatMessageGild, ChatMessageGildSummary, GildCounts, GildTier},
        chat_room::ChatRoom,
    },
    test_utils::{create_test_user, test_db},
};

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
        [500, 5_000, 50_000]
    );
    assert_eq!(
        GildTier::ALL
            .iter()
            .map(|t| t.author_share())
            .collect::<Vec<_>>(),
        [333, 3_333, 33_333]
    );
    assert_eq!(
        GildTier::ALL.iter().map(|t| t.burn()).collect::<Vec<_>>(),
        [167, 1_667, 16_667]
    );
    assert_eq!(
        GildTier::ALL.iter().map(|t| t.marker()).collect::<Vec<_>>(),
        ["$", "$$", "$$$"]
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

/// A message's marker names the best tier anyone bought and how many gilds it
/// holds in total, and the same buyer may stack one of each tier but never
/// the same tier twice.
#[tokio::test]
async fn stacking_tiers_and_the_duplicate_refusal() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let author = create_test_user(&test_db.db, "gild-stack-author").await;
    let buyer = create_test_user(&test_db.db, "gild-stack-buyer").await;
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
    let bronze =
        ChatMessageGild::insert_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Bronze)
            .await
            .expect("insert bronze")
            .expect("first bronze lands");
    assert_eq!(bronze.chips, GildTier::Bronze.price());
    assert_eq!(bronze.tier, GildTier::Bronze);
    assert_eq!(
        ChatMessageGild::count_for_message(&tx, message.id)
            .await
            .expect("count"),
        1
    );

    let repeat =
        ChatMessageGild::insert_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Bronze)
            .await
            .expect("insert repeat");
    assert!(repeat.is_none(), "the same tier never lands twice");

    ChatMessageGild::insert_in_tx(&tx, message.id, author.id, buyer.id, GildTier::Gold)
        .await
        .expect("insert gold")
        .expect("a second tier lands");
    tx.commit().await.expect("commit");

    let summary = ChatMessageGild::summary_for_message(&client, message.id)
        .await
        .expect("summary")
        .expect("gilded message has a marker");
    assert_eq!(
        summary,
        ChatMessageGildSummary {
            top_tier: GildTier::Gold,
            count: 2,
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
        ChatMessageGild::insert_in_tx(&tx, message.id, author.id, author.id, GildTier::Bronze)
            .await;
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
        ChatMessageGild::insert_in_tx(&tx, ours.id, author.id, buyer, tier)
            .await
            .expect("insert")
            .expect("lands");
    }
    ChatMessageGild::insert_in_tx(&tx, theirs.id, other.id, buyer_a.id, GildTier::Silver)
        .await
        .expect("insert")
        .expect("lands");
    tx.commit().await.expect("commit");

    let counts = ChatMessageGild::counts_for_author(&client, author.id)
        .await
        .expect("counts");
    assert_eq!(
        counts,
        GildCounts {
            bronze: 2,
            silver: 0,
            gold: 1,
        }
    );
    assert_eq!(counts.total(), 3);
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
    ChatMessageGild::insert_in_tx(&tx, ids[0], author.id, buyer.id, GildTier::Silver)
        .await
        .expect("insert")
        .expect("lands");
    ChatMessageGild::insert_in_tx(&tx, ids[2], author.id, buyer.id, GildTier::Bronze)
        .await
        .expect("insert")
        .expect("lands");
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
