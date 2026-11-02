use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::{chat_room::ChatRoom, notification::NotificationView};
use uuid::Uuid;

use super::{UnreadRow, unread_rows};

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 14, 12, minute, 0)
        .single()
        .expect("valid time")
}

fn dm(user_a: Uuid, user_b: Uuid) -> ChatRoom {
    ChatRoom {
        id: Uuid::now_v7(),
        created: at(0),
        updated: at(0),
        kind: "dm".to_string(),
        visibility: "private".to_string(),
        auto_join: false,
        permanent: false,
        slug: None,
        language_code: None,
        dm_user_a: Some(user_a),
        dm_user_b: Some(user_b),
        topic: None,
        rules: None,
        created_by: None,
    }
}

fn mention(
    viewer: Uuid,
    actor: (Uuid, &str),
    room_id: Uuid,
    room_slug: Option<&str>,
    preview: &str,
    created: DateTime<Utc>,
    read_at: Option<DateTime<Utc>>,
) -> NotificationView {
    NotificationView {
        id: Uuid::now_v7(),
        created,
        user_id: viewer,
        actor_id: actor.0,
        message_id: Uuid::now_v7(),
        room_id,
        actor_username: actor.1.to_string(),
        room_slug: room_slug.map(str::to_string),
        message_preview: preview.to_string(),
        read_at,
    }
}

#[test]
fn the_unread_rows_list_dms_by_count_then_mentions_newest_first() {
    let viewer = Uuid::now_v7();
    let (alice, bob, carol, dave) = (
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
    );
    let with_alice = dm(viewer, alice);
    let with_bob = dm(bob, viewer);
    let with_carol = dm(viewer, carol);
    let with_dave = dm(viewer, dave);
    let unread_counts = HashMap::from([
        (with_alice.id, 1),
        (with_bob.id, 3),
        (with_carol.id, 5),
        (with_dave.id, 0),
    ]);
    let usernames = HashMap::from([
        (alice, "alice".to_string()),
        (bob, "bob".to_string()),
        (carol, "carol".to_string()),
        (dave, "dave".to_string()),
    ]);
    // Carol is ignored: her DM stays hidden however much it holds.
    let ignored = HashSet::from([carol]);
    let lounge = Uuid::now_v7();
    let read = mention(
        viewer,
        (alice, "alice"),
        lounge,
        Some("lounge"),
        "  hi\n  there",
        at(1),
        Some(at(2)),
    );
    let fresh = mention(viewer, (bob, "bob"), with_bob.id, None, "ping", at(5), None);
    let rooms = vec![
        (with_alice.clone(), Vec::new()),
        (with_bob.clone(), Vec::new()),
        (with_carol, Vec::new()),
        (with_dave, Vec::new()),
    ];

    let rows = unread_rows(
        viewer,
        &rooms,
        &unread_counts,
        &usernames,
        &ignored,
        &[read.clone(), fresh.clone()],
    );

    assert_eq!(
        rows,
        vec![
            UnreadRow::Dm {
                room_id: with_bob.id,
                peer: "bob".to_string(),
                unread: 3,
            },
            UnreadRow::Dm {
                room_id: with_alice.id,
                peer: "alice".to_string(),
                unread: 1,
            },
            UnreadRow::Mention {
                room_id: with_bob.id,
                message_id: fresh.message_id,
                actor: "bob".to_string(),
                room: None,
                preview: "ping".to_string(),
                at: at(5),
                unread: true,
            },
            UnreadRow::Mention {
                room_id: lounge,
                message_id: read.message_id,
                actor: "alice".to_string(),
                room: Some("lounge".to_string()),
                preview: "hi there".to_string(),
                at: at(1),
                unread: false,
            },
        ]
    );
}
