use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    irc_history::{HistoryReader, ReadError, Reference, Selection},
    room_ban::{RoomBan, RoomBanParams},
};
use crate::test_utils::{create_test_user, test_db};

async fn message(
    client: &tokio_postgres::Client,
    room: Uuid,
    author: Uuid,
    at: DateTime<Utc>,
    body: &str,
) -> ChatMessage {
    let msg = ChatMessage::create(
        client,
        ChatMessageParams {
            room_id: room,
            user_id: author,
            body: body.into(),
        },
    )
    .await
    .unwrap();
    client
        .execute(
            "UPDATE chat_messages SET created = $2 WHERE id = $1",
            &[&msg.id, &at],
        )
        .await
        .unwrap();
    ChatMessage::get(client, msg.id).await.unwrap().unwrap()
}

#[tokio::test]
async fn irc_history_paginates_ties_ranges_and_around_without_skips() {
    let db = test_db().await;
    let client = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "history-reader").await;
    let room = ChatRoom::get_or_create_public_room(&client, "history")
        .await
        .unwrap();
    let at: DateTime<Utc> = "2026-10-13T10:00:00.000Z".parse().unwrap();
    let mut stored = Vec::new();
    for i in 0..7 {
        stored.push(
            message(
                &client,
                room.id,
                user.id,
                at + Duration::microseconds(123),
                &format!("row {i}"),
            )
            .await,
        );
    }
    stored.sort_by_key(|m| (m.created, m.id));
    let joined = [room.id];
    let reader = HistoryReader {
        client: &client,
        viewer: user.id,
        joined_channels: &joined,
        ignored_authors: &[],
    };
    let ids = |page: super::irc_history::Page<super::chat_message::IrcHistoryMessage>| {
        page.items
            .into_iter()
            .map(|m| m.message.id)
            .collect::<Vec<_>>()
    };
    let reference = |i: usize| Reference::MessageId(stored[i].id);
    let expected =
        |range: std::ops::Range<usize>| stored[range].iter().map(|m| m.id).collect::<Vec<_>>();
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Latest(None), 3)
            .await
            .unwrap()),
        expected(4..7)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Before(reference(4)), 3)
            .await
            .unwrap()),
        expected(1..4)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::After(reference(1)), 3)
            .await
            .unwrap()),
        expected(2..5)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Latest(Some(reference(1))), 2)
            .await
            .unwrap()),
        expected(5..7)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Between(reference(0), reference(6)), 2)
            .await
            .unwrap()),
        expected(1..3)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Between(reference(6), reference(0)), 2)
            .await
            .unwrap()),
        expected(4..6)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Around(reference(3)), 5)
            .await
            .unwrap()),
        expected(1..6)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Around(reference(0)), 5)
            .await
            .unwrap()),
        expected(0..5)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Around(reference(6)), 5)
            .await
            .unwrap()),
        expected(2..7)
    );
    assert_eq!(
        ids(reader
            .messages(room.id, Selection::Around(reference(3)), 1)
            .await
            .unwrap()),
        expected(3..4)
    );
    assert!(
        reader
            .messages(room.id, Selection::Before(Reference::Timestamp(at)), 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        reader
            .messages(room.id, Selection::After(Reference::Timestamp(at)), 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        reader
            .messages(
                room.id,
                Selection::Before(Reference::Timestamp(at + Duration::milliseconds(1))),
                10
            )
            .await
            .unwrap()
            .items
            .len(),
        7
    );
    assert!(
        reader
            .messages(room.id, Selection::Latest(None), 7)
            .await
            .unwrap()
            .exhausted
    );
    assert!(
        !reader
            .messages(room.id, Selection::Latest(None), 6)
            .await
            .unwrap()
            .exhausted
    );
}

#[tokio::test]
async fn irc_history_checks_access_and_anchor_scope_on_every_read() {
    let db = test_db().await;
    let client = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "reader").await;
    let owner = create_test_user(&db.db, "owner").await;
    let public = ChatRoom::get_or_create_public_room(&client, "shared")
        .await
        .unwrap();
    let private = ChatRoom::create_private_room(&client, "shared", owner.id)
        .await
        .unwrap();
    let at = Utc::now();
    let pub_msg = message(&client, public.id, owner.id, at, "public").await;
    let priv_msg = message(&client, private.id, owner.id, at, "private").await;
    let joined = [public.id, private.id];
    let reader = HistoryReader {
        client: &client,
        viewer: user.id,
        joined_channels: &joined,
        ignored_authors: &[],
    };
    assert!(matches!(
        reader
            .messages(private.id, Selection::Latest(None), 10)
            .await,
        Err(ReadError::InvalidTarget)
    ));
    ChatRoomMember::join(&client, private.id, user.id)
        .await
        .unwrap();
    assert_eq!(
        reader
            .messages(private.id, Selection::Latest(None), 10)
            .await
            .unwrap()
            .items[0]
            .message
            .id,
        priv_msg.id
    );
    assert!(matches!(
        reader
            .messages(
                public.id,
                Selection::Before(Reference::MessageId(priv_msg.id)),
                10
            )
            .await,
        Err(ReadError::InvalidReference)
    ));
    assert_eq!(
        reader
            .messages(public.id, Selection::Latest(None), 10)
            .await
            .unwrap()
            .items[0]
            .message
            .id,
        pub_msg.id
    );
    ChatRoomMember::leave(&client, private.id, user.id)
        .await
        .unwrap();
    assert!(matches!(
        reader
            .messages(private.id, Selection::Latest(None), 10)
            .await,
        Err(ReadError::InvalidTarget)
    ));
    RoomBan::create(
        &client,
        RoomBanParams {
            room_id: public.id,
            target_user_id: user.id,
            actor_user_id: owner.id,
            reason: "test".into(),
            expires_at: None,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        reader
            .messages(public.id, Selection::Latest(None), 10)
            .await,
        Err(ReadError::InvalidTarget)
    ));
    let targets = reader
        .targets(at - Duration::days(1), at + Duration::days(1), 10)
        .await
        .unwrap();
    assert!(targets.items.is_empty());
    let unjoined = HistoryReader {
        joined_channels: &[],
        ..reader
    };
    assert!(matches!(
        unjoined
            .messages(public.id, Selection::Latest(None), 10)
            .await,
        Err(ReadError::InvalidTarget)
    ));
}

#[tokio::test]
async fn irc_history_filters_before_limit_and_discovers_existing_dms() {
    let db = test_db().await;
    let client = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "reader").await;
    let ignored = create_test_user(&db.db, "ignored").await;
    let system = create_test_user(&db.db, "system").await;
    client
        .execute(
            "UPDATE users SET settings = '{\"system\":true}' WHERE id = $1",
            &[&system.id],
        )
        .await
        .unwrap();
    let room = ChatRoom::get_or_create_public_room(&client, "history")
        .await
        .unwrap();
    let dm = ChatRoom::get_or_create_dm(&client, user.id, ignored.id)
        .await
        .unwrap();
    ChatRoomMember::join(&client, dm.id, user.id).await.unwrap();
    ChatRoomMember::join(&client, dm.id, ignored.id)
        .await
        .unwrap();
    let at: DateTime<Utc> = "2026-10-13T10:00:00.000Z".parse().unwrap();
    let old = message(&client, room.id, user.id, at, "old").await;
    message(
        &client,
        room.id,
        system.id,
        at + Duration::seconds(1),
        "activity",
    )
    .await;
    message(
        &client,
        room.id,
        ignored.id,
        at + Duration::seconds(2),
        "ignored",
    )
    .await;
    let incoming = message(
        &client,
        dm.id,
        ignored.id,
        at + Duration::seconds(3),
        "incoming",
    )
    .await;
    message(
        &client,
        dm.id,
        user.id,
        at + Duration::seconds(4),
        "outgoing",
    )
    .await;
    ChatMessage::edit_by_author(&client, old.id, user.id, "edited")
        .await
        .unwrap();
    let deleted = message(
        &client,
        room.id,
        user.id,
        at + Duration::seconds(5),
        "deleted",
    )
    .await;
    ChatMessage::delete_by_author(&client, deleted.id, user.id)
        .await
        .unwrap();
    let joined = [room.id];
    let ignored_ids = [ignored.id];
    let reader = HistoryReader {
        client: &client,
        viewer: user.id,
        joined_channels: &joined,
        ignored_authors: &ignored_ids,
    };
    let page = reader
        .messages(room.id, Selection::Latest(None), 2)
        .await
        .unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|m| m.message.body.as_str())
            .collect::<Vec<_>>(),
        ["edited", "activity"]
    );
    assert_eq!(page.items[0].message.created, at);
    assert!(page.exhausted);
    let page = reader
        .messages(dm.id, Selection::Latest(None), 10)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].message.id, incoming.id);
    let targets = reader
        .targets(at - Duration::seconds(1), at + Duration::seconds(10), 10)
        .await
        .unwrap();
    assert_eq!(
        targets.items.iter().map(|t| t.room.id).collect::<Vec<_>>(),
        [room.id, dm.id]
    );
    let targets = reader
        .targets(at + Duration::seconds(10), at - Duration::seconds(1), 1)
        .await
        .unwrap();
    assert_eq!(targets.items[0].room.id, dm.id);
    assert!(!targets.exhausted);
    // A target with a newer last message is not selected by an older message.
    let targets = reader
        .targets(at + Duration::seconds(2), at + Duration::seconds(4), 10)
        .await
        .unwrap();
    assert!(targets.items.is_empty());
    let last_read: Option<DateTime<Utc>> = client
        .query_one(
            "SELECT last_read_at FROM chat_room_members WHERE room_id = $1 AND user_id = $2",
            &[&dm.id, &user.id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(last_read.is_none());
    ChatRoomMember::leave(&client, dm.id, user.id)
        .await
        .unwrap();
    assert!(matches!(
        reader.messages(dm.id, Selection::Latest(None), 10).await,
        Err(ReadError::InvalidTarget)
    ));
}

#[tokio::test]
async fn irc_history_excludes_game_and_hidden_rooms_even_if_joined() {
    let db = test_db().await;
    let client = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "history-viewer").await;
    let game = ChatRoom::get_or_create_game_room(
        &client,
        super::game_room::GameKind::Blackjack,
        "history-table",
    )
    .await
    .unwrap();
    let hidden = ChatRoom::ensure_nightcap(&client).await.unwrap();
    let at: DateTime<Utc> = "2026-10-13T10:00:00.000Z".parse().unwrap();
    for room in [&game, &hidden] {
        ChatRoomMember::join(&client, room.id, user.id)
            .await
            .unwrap();
        message(&client, room.id, user.id, at, "not IRC history").await;
    }
    let joined = [game.id, hidden.id];
    let reader = HistoryReader {
        client: &client,
        viewer: user.id,
        joined_channels: &joined,
        ignored_authors: &[],
    };
    for room in [&game, &hidden] {
        assert!(matches!(
            reader.messages(room.id, Selection::Latest(None), 10).await,
            Err(ReadError::InvalidTarget)
        ));
        assert!(
            reader
                .targets(at - Duration::days(1), at + Duration::days(1), 10)
                .await
                .unwrap()
                .items
                .is_empty()
        );
    }
}
