use super::*;

#[test]
fn normalize_duration_accepts_configured_options() {
    for duration_secs in POLL_DURATION_OPTIONS_SECS {
        assert_eq!(
            normalize_duration_secs(duration_secs).unwrap(),
            duration_secs
        );
    }
}

#[test]
fn normalize_duration_rejects_unconfigured_values() {
    assert!(normalize_duration_secs(5 * 60).is_err());
    assert!(normalize_duration_secs(40 * 60).is_err());
}

/// A poll has to carry its author's name out of the query: the strip says who
/// started it, and the only other way to get the name is a second round trip
/// per rendered frame.
#[tokio::test]
async fn active_polls_carry_the_author_username() {
    let test_db = crate::test_utils::test_db().await;
    let author = crate::test_utils::create_test_user(&test_db.db, "poll-author").await;
    let mut client = test_db.db.get().await.expect("db client");
    let lounge = crate::models::chat_room::ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    crate::models::chat_room_member::ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join lounge");

    let created = create_poll(
        &mut client,
        CreateChatPoll {
            user_id: author.id,
            room_id: lounge.id,
            question: "Which editor wins?".to_string(),
            options: vec!["vim".to_string(), "emacs".to_string()],
            duration_secs: POLL_DURATION_OPTIONS_SECS[0],
        },
    )
    .await
    .expect("create poll");
    assert_eq!(created.author_username.as_deref(), Some(&*author.username));

    let mut active = list_active_polls_for_rooms(&client, author.id, &[lounge.id])
        .await
        .expect("list active polls");
    let listed = active.remove(&lounge.id).expect("poll for the lounge");
    assert_eq!(listed.author_username.as_deref(), Some(&*author.username));
}

/// The closing sweep reads the poll back through its own claim query, so that
/// one needs the author too, or the name is only ever right while the poll is
/// still open.
#[tokio::test]
async fn claiming_an_expired_poll_keeps_the_author_username() {
    let test_db = crate::test_utils::test_db().await;
    let author = crate::test_utils::create_test_user(&test_db.db, "poll-closer").await;
    let mut client = test_db.db.get().await.expect("db client");
    let lounge = crate::models::chat_room::ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    crate::models::chat_room_member::ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join lounge");

    let created = create_poll(
        &mut client,
        CreateChatPoll {
            user_id: author.id,
            room_id: lounge.id,
            question: "Which editor wins?".to_string(),
            options: vec!["vim".to_string(), "emacs".to_string()],
            duration_secs: POLL_DURATION_OPTIONS_SECS[0],
        },
    )
    .await
    .expect("create poll");

    // Age the whole window out rather than sleeping: the claim only fires once
    // `ends_at` has passed, and the table checks `ends_at > starts_at`.
    client
        .execute(
            "UPDATE chat_polls
             SET starts_at = current_timestamp - interval '2 minutes',
                 ends_at = current_timestamp - interval '1 minute'
             WHERE id = $1",
            &[&created.poll.id],
        )
        .await
        .expect("expire poll");

    let claimed = claim_expired_poll(&client, created.poll.id)
        .await
        .expect("claim expired poll")
        .expect("expired poll was claimable");

    assert_eq!(claimed.author_username.as_deref(), Some(&*author.username));
}
