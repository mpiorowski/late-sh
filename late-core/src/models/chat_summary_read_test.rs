use chrono::{Duration, SubsecRound, Utc};

/// Postgres stores `timestamptz` to the microsecond. Production watermarks
/// are a message's own `created`, so they have already been through that
/// truncation; only a test minting its own instant has to do it by hand.
fn stamp(ago: Duration) -> chrono::DateTime<Utc> {
    (Utc::now() - ago).trunc_subsecs(6)
}

use crate::{
    models::{
        chat_room::ChatRoom,
        chat_summary_read::ChatSummaryRead,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

async fn reader(client: &tokio_postgres::Client, name: &str) -> User {
    User::create(
        client,
        UserParams {
            fingerprint: format!("summary-read-{name}"),
            username: name.to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user")
}

#[tokio::test]
async fn a_reader_with_no_summary_yet_has_no_watermark() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let user = reader(&client, "swm1").await;

    // The first catch-up has to be distinguishable from a stale one: `None`
    // is what sends the caller to the first-catch-up window instead of
    // reaching back forever.
    assert_eq!(
        ChatSummaryRead::summarized_through(&client, user.id, room.id)
            .await
            .expect("read watermark"),
        None
    );
}

#[tokio::test]
async fn the_watermark_carries_forward_and_never_backwards() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let user = reader(&client, "swm2").await;

    let noon = stamp(Duration::hours(2));
    ChatSummaryRead::advance(&client, user.id, room.id, noon)
        .await
        .expect("first advance");
    assert_eq!(
        ChatSummaryRead::summarized_through(&client, user.id, room.id)
            .await
            .expect("read watermark"),
        Some(noon)
    );

    let later = noon + Duration::hours(1);
    ChatSummaryRead::advance(&client, user.id, room.id, later)
        .await
        .expect("second advance");
    assert_eq!(
        ChatSummaryRead::summarized_through(&client, user.id, room.id)
            .await
            .expect("read watermark"),
        Some(later)
    );

    // A narrow explicit window landing on already-summarized ground must not
    // rewind the cursor: that would hand the reader the same bullets twice.
    ChatSummaryRead::advance(&client, user.id, room.id, noon)
        .await
        .expect("backwards advance");
    assert_eq!(
        ChatSummaryRead::summarized_through(&client, user.id, room.id)
            .await
            .expect("read watermark"),
        Some(later)
    );
}

#[tokio::test]
async fn watermarks_are_scoped_to_the_reader_and_the_room() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let other = ChatRoom::get_or_create_public_room(&client, "summary-scope")
        .await
        .expect("public room");
    let mine = reader(&client, "swm3").await;
    let theirs = reader(&client, "swm4").await;

    let through = stamp(Duration::minutes(30));
    ChatSummaryRead::advance(&client, mine.id, lounge.id, through)
        .await
        .expect("advance");

    // Being told about #lounge says nothing about another room, and nothing
    // about another reader.
    assert_eq!(
        ChatSummaryRead::summarized_through(&client, mine.id, other.id)
            .await
            .expect("read watermark"),
        None
    );
    assert_eq!(
        ChatSummaryRead::summarized_through(&client, theirs.id, lounge.id)
            .await
            .expect("read watermark"),
        None
    );
}
