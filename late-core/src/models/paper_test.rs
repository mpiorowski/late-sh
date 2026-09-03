use chrono::{Duration, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::{PaperEdition, PaperRoomEdition, PaperSectionKind, PaperSectionRow, PaperStatus};
use crate::db::Db;
use crate::models::chat_message::{ChatMessage, ChatMessageParams};
use crate::models::chat_room::ChatRoom;
use crate::test_utils::{create_test_user, test_db};

const EDITION: NaiveDate = match NaiveDate::from_ymd_opt(2026, 9, 3) {
    Some(date) => date,
    None => unreachable!(),
};

/// The edition's window: the whole of the day before it.
fn window() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let ceiling = EDITION.and_hms_opt(0, 0, 0).unwrap().and_utc();
    (ceiling - Duration::days(1), ceiling)
}

/// `count` human messages in `room`, stamped inside the edition's window.
async fn say(client: &Client, db: &Db, room: &ChatRoom, author: &str, count: usize) {
    let user = create_test_user(db, author).await;
    let (floor, _) = window();
    for index in 0..count {
        let message = ChatMessage::create(
            client,
            ChatMessageParams {
                room_id: room.id,
                user_id: user.id,
                body: format!("line {index}"),
            },
        )
        .await
        .expect("create message");
        client
            .execute(
                "UPDATE chat_messages SET created = $2 WHERE id = $1",
                &[
                    &message.id,
                    &(floor + Duration::hours(1) + Duration::seconds(index as i64)),
                ],
            )
            .await
            .expect("backdate message");
    }
}

#[tokio::test]
async fn candidates_count_the_window_and_skip_settled_rooms() {
    let test_db = test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (floor, ceiling) = window();
    let stale_before = Utc::now() - Duration::minutes(15);

    let busy = ChatRoom::get_or_create_public_room(&client, "busy")
        .await
        .expect("room");
    say(&client, &db, &busy, "alice", 4).await;
    say(&client, &db, &busy, "bob", 2).await;
    // Today's message is outside the window and must not count.
    let today = create_test_user(&db, "carol").await;
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: busy.id,
            user_id: today.id,
            body: "this morning".to_string(),
        },
    )
    .await
    .expect("create message");
    let quiet = ChatRoom::get_or_create_public_room(&client, "quiet")
        .await
        .expect("room");
    say(&client, &db, &quiet, "dave", 1).await;
    // A private room never reaches the paper, however busy.
    let owner = create_test_user(&db, "owner").await;
    let private = ChatRoom::create_private_room(&client, "secret", owner.id)
        .await
        .expect("private room");
    say(&client, &db, &private, "erin", 9).await;

    let candidates =
        PaperRoomEdition::list_candidates(&client, EDITION, floor, ceiling, stale_before)
            .await
            .expect("candidates");
    let by_id: Vec<(Uuid, &str, i64, i64)> = candidates
        .iter()
        .map(|c| (c.room_id, c.label.as_str(), c.message_count, c.author_count))
        .collect();
    assert_eq!(
        by_id,
        vec![(busy.id, "busy", 6, 2), (quiet.id, "quiet", 1, 1)],
        "{candidates:?}"
    );

    // Settling a room takes it out of the next sweep: quiet for good, a
    // fresh claim while it holds, and a stale claim only until reclaimed.
    assert!(
        PaperRoomEdition::mark_quiet(&client, quiet.id, EDITION, 1, 1)
            .await
            .expect("quiet")
    );
    assert!(
        PaperRoomEdition::claim_printing(&client, busy.id, EDITION, 6, 2, stale_before)
            .await
            .expect("claim")
    );
    let remaining =
        PaperRoomEdition::list_candidates(&client, EDITION, floor, ceiling, stale_before)
            .await
            .expect("candidates");
    assert!(remaining.is_empty(), "{remaining:?}");
    let reclaimable = PaperRoomEdition::list_candidates(
        &client,
        EDITION,
        floor,
        ceiling,
        Utc::now() + Duration::hours(1),
    )
    .await
    .expect("candidates");
    assert_eq!(
        reclaimable.iter().map(|c| c.room_id).collect::<Vec<_>>(),
        vec![busy.id]
    );
}

#[tokio::test]
async fn a_room_claim_is_won_once_reclaimed_when_stale_and_finished_by_its_holder() {
    let test_db = test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "claims")
        .await
        .expect("room");
    let member = create_test_user(&db, "member").await;
    crate::models::chat_room_member::ChatRoomMember::join(&client, room.id, member.id)
        .await
        .expect("join");
    let stale_before = Utc::now() - Duration::minutes(15);

    assert!(
        PaperRoomEdition::claim_printing(&client, room.id, EDITION, 7, 3, stale_before)
            .await
            .expect("claim")
    );
    // The second replica loses while the claim is fresh.
    assert!(
        !PaperRoomEdition::claim_printing(&client, room.id, EDITION, 7, 3, stale_before)
            .await
            .expect("claim")
    );
    // A crashed printer's claim is taken over once it is old enough.
    assert!(
        PaperRoomEdition::claim_printing(
            &client,
            room.id,
            EDITION,
            7,
            3,
            Utc::now() + Duration::hours(1)
        )
        .await
        .expect("reclaim")
    );

    PaperRoomEdition::finish(&client, room.id, EDITION, "- a page")
        .await
        .expect("finish");
    // Finished is final: a late finisher has nothing to write into, and a
    // release after the fact does not take the page away.
    assert!(
        PaperRoomEdition::finish(&client, room.id, EDITION, "- another")
            .await
            .is_err()
    );
    PaperRoomEdition::release(&client, room.id, EDITION)
        .await
        .expect("release");

    let edition = PaperEdition::load(&client, EDITION).await.expect("load");
    assert!(edition.has_print());
    let page = &edition.rooms[0];
    assert_eq!(
        (
            page.room_id,
            page.label.as_str(),
            page.status,
            page.message_count,
            page.author_count,
            page.member_count,
            page.text.as_deref(),
        ),
        (
            room.id,
            "claims",
            PaperStatus::Ready,
            7,
            3,
            1,
            Some("- a page")
        )
    );

    // A release while printing does drop the claim, so the next sweep
    // can retry.
    let retry = ChatRoom::get_or_create_public_room(&client, "retry")
        .await
        .expect("room");
    assert!(
        PaperRoomEdition::claim_printing(&client, retry.id, EDITION, 5, 1, stale_before)
            .await
            .expect("claim")
    );
    PaperRoomEdition::release(&client, retry.id, EDITION)
        .await
        .expect("release");
    assert!(
        PaperRoomEdition::claim_printing(&client, retry.id, EDITION, 5, 1, stale_before)
            .await
            .expect("claim again")
    );
}

#[tokio::test]
async fn sections_settle_as_ready_or_quiet_and_load_with_the_edition() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let stale_before = Utc::now() - Duration::minutes(15);

    assert!(
        PaperSectionRow::is_unsettled(&client, EDITION, PaperSectionKind::Reading, stale_before)
            .await
            .expect("unsettled")
    );
    assert!(
        PaperSectionRow::claim_printing(&client, EDITION, PaperSectionKind::Reading, stale_before)
            .await
            .expect("claim")
    );
    assert!(
        !PaperSectionRow::is_unsettled(&client, EDITION, PaperSectionKind::Reading, stale_before)
            .await
            .expect("held")
    );
    // A print that found nothing to say settles quiet under the same claim.
    PaperSectionRow::finish(&client, EDITION, PaperSectionKind::Reading, None)
        .await
        .expect("finish quiet");

    assert!(
        PaperSectionRow::claim_printing(&client, EDITION, PaperSectionKind::Outside, stale_before)
            .await
            .expect("claim")
    );
    PaperSectionRow::finish(
        &client,
        EDITION,
        PaperSectionKind::Outside,
        Some("- the world"),
    )
    .await
    .expect("finish ready");
    assert!(
        !PaperSectionRow::mark_quiet(&client, EDITION, PaperSectionKind::Outside)
            .await
            .expect("quiet after ready is a no-op")
    );

    let edition = PaperEdition::load(&client, EDITION).await.expect("load");
    assert!(edition.rooms.is_empty());
    assert_eq!(
        edition
            .sections
            .iter()
            .map(|s| (s.section, s.status, s.text.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (
                PaperSectionKind::Outside,
                PaperStatus::Ready,
                Some("- the world")
            ),
            (PaperSectionKind::Reading, PaperStatus::Quiet, None),
        ]
    );
    assert!(edition.has_print());
}

#[tokio::test]
async fn recent_ready_sections_come_back_newest_first_and_only_from_earlier_editions() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let stale_before = Utc::now() - Duration::minutes(15);
    let day = |offset: i64| EDITION + Duration::days(offset);

    for (offset, text) in [
        (-3, Some("- three days ago")),
        (-2, None),
        (-1, Some("- yesterday")),
        (0, Some("- today")),
    ] {
        assert!(
            PaperSectionRow::claim_printing(
                &client,
                day(offset),
                PaperSectionKind::Outside,
                stale_before
            )
            .await
            .expect("claim")
        );
        PaperSectionRow::finish(&client, day(offset), PaperSectionKind::Outside, text)
            .await
            .expect("finish");
    }
    // A different section on the same day is not this section's memory.
    assert!(
        PaperSectionRow::claim_printing(&client, day(-1), PaperSectionKind::Reading, stale_before)
            .await
            .expect("claim")
    );
    PaperSectionRow::finish(
        &client,
        day(-1),
        PaperSectionKind::Reading,
        Some("- a share"),
    )
    .await
    .expect("finish");

    let recent = PaperSectionRow::list_recent_ready(&client, PaperSectionKind::Outside, EDITION, 5)
        .await
        .expect("recent");
    assert_eq!(
        recent,
        vec![
            (day(-1), "- yesterday".to_string()),
            (day(-3), "- three days ago".to_string()),
        ]
    );
    let capped = PaperSectionRow::list_recent_ready(&client, PaperSectionKind::Outside, EDITION, 1)
        .await
        .expect("recent");
    assert_eq!(capped, vec![(day(-1), "- yesterday".to_string())]);
}
