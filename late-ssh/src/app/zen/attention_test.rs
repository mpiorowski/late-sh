use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::{chat_room::ChatRoom, notification::NotificationView};
use uuid::Uuid;

use super::{Inputs, Item, Match, items, unread_rows};

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
        message_body: preview.to_string(),
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
        "answered",
        at(1),
        Some(at(2)),
    );
    let older = mention(
        viewer,
        (alice, "alice"),
        lounge,
        Some("lounge"),
        "hi",
        at(3),
        None,
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
        &[read, older.clone(), fresh.clone()],
    );

    // The read mention answered itself: it never comes back as a row.
    assert_eq!(
        rows,
        vec![
            Item::Dm {
                room_id: with_bob.id,
                peer: "bob".to_string(),
                unread: 3,
            },
            Item::Dm {
                room_id: with_alice.id,
                peer: "alice".to_string(),
                unread: 1,
            },
            Item::Mention {
                room_id: with_bob.id,
                message_id: fresh.message_id,
                actor: "bob".to_string(),
                room: None,
            },
            Item::Mention {
                room_id: lounge,
                message_id: older.message_id,
                actor: "alice".to_string(),
                room: Some("lounge".to_string()),
            },
        ]
    );
}

fn quiet() -> Inputs {
    Inputs {
        my_moves: Vec::new(),
        results: Vec::new(),
        unread: Vec::new(),
        tank_due: false,
        tree_due: false,
        pet_due: false,
    }
}

#[test]
fn nothing_waiting_is_an_empty_list_so_the_guest_stays_down() {
    assert!(items(quiet()).is_empty());
}

#[test]
fn the_list_puts_your_moves_first_then_results_people_and_the_companions() {
    let dm = Item::Dm {
        room_id: Uuid::nil(),
        peer: "ana".to_string(),
        unread: 3,
    };
    let mention = Item::Mention {
        room_id: Uuid::nil(),
        message_id: Uuid::nil(),
        actor: "bo".to_string(),
        room: Some("lounge".to_string()),
    };
    let listed = items(Inputs {
        my_moves: vec![Match {
            game: "chess",
            opponent: "mat".to_string(),
        }],
        results: vec![Match {
            game: "battleship",
            opponent: "lee".to_string(),
        }],
        unread: vec![dm.clone(), mention.clone()],
        tank_due: true,
        tree_due: false,
        pet_due: true,
    });
    assert_eq!(
        listed,
        vec![
            Item::YourMove {
                game: "chess",
                opponent: "mat".to_string(),
            },
            Item::Result {
                game: "battleship",
                opponent: "lee".to_string(),
            },
            dm,
            mention,
            Item::FeedTank,
            Item::PetPet,
        ]
    );
    let words: Vec<String> = listed.iter().map(Item::label).collect();
    assert_eq!(
        words,
        vec![
            "your move: chess vs mat",
            "result in: battleship vs lee",
            "@ana: 3 unread",
            "@bo mentioned you in #lounge",
            "the tank is unfed today",
            "the pet is unpetted today",
        ]
    );
    let acts: Vec<&str> = listed.iter().map(Item::act).collect();
    assert_eq!(acts, vec!["play", "look", "read", "read", "feed", "pet"]);
}

#[test]
fn a_single_unread_dm_and_a_nameless_room_read_in_words() {
    let dm = Item::Dm {
        room_id: Uuid::nil(),
        peer: "ana".to_string(),
        unread: 1,
    };
    assert_eq!(dm.label(), "@ana: one unread");
    let mention = Item::Mention {
        room_id: Uuid::nil(),
        message_id: Uuid::nil(),
        actor: "bo".to_string(),
        room: None,
    };
    assert_eq!(mention.label(), "@bo mentioned you");
    assert_eq!(Item::WaterTree.label(), "the tree is unwatered today");
    assert_eq!(Item::WaterTree.act(), "water");
}
