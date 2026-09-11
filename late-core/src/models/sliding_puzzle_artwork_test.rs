use super::sliding_puzzle_artwork::{Artwork, Submission};
use crate::{
    models::{
        chat_message_reaction::ChatMessageReaction, chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
    },
    test_utils::{create_test_user, test_db},
};
use chrono::{Duration, Utc};

fn submission(hash: char) -> Submission {
    Submission {
        source_url: "https://example.com/source.png".into(),
        image_url: "https://files.example.com/review.png".into(),
        sha256: hash.to_string().repeat(64),
        title: "Night Garden".into(),
    }
}

#[tokio::test]
async fn only_staff_thumbs_up_approves_and_reaction_changes_do_not_revoke() {
    let db = test_db().await;
    let artist = create_test_user(&db.db, "puzzle-artist").await;
    let regular = create_test_user(&db.db, "puzzle-viewer").await;
    let moderator = create_test_user(&db.db, "puzzle-reviewer").await;
    let admin = create_test_user(&db.db, "puzzle-admin").await;
    let mut client = db.db.get().await.unwrap();
    let room = ChatRoom::find_public_non_dm_by_slug(&client, "puzzle-art")
        .await
        .unwrap()
        .unwrap();
    for id in [artist.id, regular.id, moderator.id, admin.id] {
        ChatRoomMember::join(&client, room.id, id).await.unwrap();
    }
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&moderator.id],
        )
        .await
        .unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .unwrap();
    let tx = client.transaction().await.unwrap();
    let message = Artwork::submit(&tx, room.id, artist.id, &submission('a'))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    ChatMessageReaction::toggle(&client, message.id, regular.id, "👍")
        .await
        .unwrap();
    ChatMessageReaction::toggle(&client, message.id, moderator.id, "👎")
        .await
        .unwrap();
    assert!(client.query_one("SELECT approved_at IS NULL AS pending FROM sliding_puzzle_artworks WHERE source_message_id = $1", &[&message.id]).await.unwrap().get::<_, bool>("pending"));
    let other = db.db.get().await.unwrap();
    let (a, b) = tokio::join!(
        ChatMessageReaction::toggle(&client, message.id, moderator.id, "👍🏽"),
        ChatMessageReaction::toggle(&other, message.id, admin.id, "👍"),
    );
    a.unwrap();
    b.unwrap();
    assert!(
        Artwork::approval_for_message(&client, message.id, regular.id)
            .await
            .unwrap()
            .is_none()
    );
    for reviewer_id in [moderator.id, admin.id] {
        assert_eq!(
            Artwork::approval_for_message(&client, message.id, reviewer_id)
                .await
                .unwrap()
                .unwrap()
                .0,
            "Night Garden"
        );
    }
    let row = client.query_one("SELECT approved_by, available_from FROM sliding_puzzle_artworks WHERE source_message_id = $1", &[&message.id]).await.unwrap();
    assert!([moderator.id, admin.id].contains(&row.get("approved_by")));
    assert_eq!(
        row.get::<_, chrono::NaiveDate>("available_from"),
        Utc::now().date_naive() + Duration::days(1)
    );
    ChatMessageReaction::toggle(&client, message.id, admin.id, "👍")
        .await
        .unwrap();
    ChatMessageReaction::toggle(&client, message.id, moderator.id, "👎")
        .await
        .unwrap();
    assert!(client.query_one("SELECT approved_at IS NOT NULL AS approved FROM sliding_puzzle_artworks WHERE source_message_id = $1", &[&message.id]).await.unwrap().get::<_, bool>("approved"));
}

#[tokio::test]
async fn duplicate_submissions_edits_and_deletion_preserve_review_identity() {
    let db = test_db().await;
    let artist = create_test_user(&db.db, "puzzle-identity").await;
    let mut client = db.db.get().await.unwrap();
    let room = ChatRoom::find_public_non_dm_by_slug(&client, "puzzle-art")
        .await
        .unwrap()
        .unwrap();
    ChatRoomMember::join(&client, room.id, artist.id)
        .await
        .unwrap();
    let tx = client.transaction().await.unwrap();
    let message = Artwork::submit(&tx, room.id, artist.id, &submission('b'))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let tx = client.transaction().await.unwrap();
    assert!(
        Artwork::submit(&tx, room.id, artist.id, &submission('b'))
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(
        client
            .execute(
                "UPDATE chat_messages SET body = 'swapped image' WHERE id = $1",
                &[&message.id]
            )
            .await
            .is_err()
    );
    client
        .execute("DELETE FROM chat_messages WHERE id = $1", &[&message.id])
        .await
        .unwrap();
    assert!(
        !client
            .query_one(
                "SELECT active FROM sliding_puzzle_artworks WHERE sha256 = $1",
                &[&submission('b').sha256]
            )
            .await
            .unwrap()
            .get::<_, bool>("active")
    );
    let tx = client.transaction().await.unwrap();
    assert!(
        Artwork::submit(&tx, room.id, artist.id, &submission('b'))
            .await
            .is_ok()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn daily_assignment_is_shared_stable_and_rotates_through_approved_artwork() {
    let db = test_db().await;
    let today = Utc::now().date_naive();
    let mut first = db.db.get().await.unwrap();
    let mut second = db.db.get().await.unwrap();
    let (a, b) = tokio::join!(
        async {
            let tx = first.transaction().await.unwrap();
            let art = Artwork::assign_daily(&tx, today).await.unwrap();
            tx.commit().await.unwrap();
            art
        },
        async {
            let tx = second.transaction().await.unwrap();
            let art = Artwork::assign_daily(&tx, today).await.unwrap();
            tx.commit().await.unwrap();
            art
        }
    );
    assert_eq!(a.id, b.id);
    let pending: uuid::Uuid = first.query_one(
        "INSERT INTO sliding_puzzle_artworks (image_url, sha256, title, credit) VALUES ('https://files.example.com/new.png', $1, 'New image', 'artist') RETURNING id", &[&"c".repeat(64)],
    ).await.unwrap().get("id");
    let mut seen = std::collections::HashSet::from([a.id]);
    for day in 1..3 {
        let tx = first.transaction().await.unwrap();
        let art = Artwork::assign_daily(&tx, today + Duration::days(day))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_ne!(art.id, pending);
        assert!(seen.insert(art.id));
    }
    first.execute("UPDATE sliding_puzzle_artworks SET approved_at = now(), available_from = $2 WHERE id = $1", &[&pending, &(today + Duration::days(3))]).await.unwrap();
    let tx = first.transaction().await.unwrap();
    let art = Artwork::assign_daily(&tx, today + Duration::days(3))
        .await
        .unwrap();
    assert_eq!(art.id, pending);
    tx.commit().await.unwrap();
    first
        .execute(
            "UPDATE sliding_puzzle_artworks SET active = false WHERE id = $1",
            &[&pending],
        )
        .await
        .unwrap();
    let tx = first.transaction().await.unwrap();
    assert_eq!(
        Artwork::assign_daily(&tx, today + Duration::days(3))
            .await
            .unwrap()
            .id,
        pending
    );
    assert_ne!(
        Artwork::assign_daily(&tx, today + Duration::days(4))
            .await
            .unwrap()
            .id,
        pending
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn old_boards_requested_after_midnight_do_not_repeat_neighboring_assignments() {
    let db = test_db().await;
    let mut client = db.db.get().await.unwrap();
    let date = Utc::now().date_naive();
    let mut chosen = std::collections::HashMap::new();
    for offset in [1, 3, 2, 0] {
        let tx = client.transaction().await.unwrap();
        let art = Artwork::assign_daily(&tx, date + Duration::days(offset))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        chosen.insert(offset, art.id);
    }
    for offset in 0..3 {
        assert_ne!(chosen[&offset], chosen[&(offset + 1)]);
    }
}
