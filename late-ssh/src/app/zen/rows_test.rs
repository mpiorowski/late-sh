use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::{
    article::{Article, ArticleFeedItem},
    chat_room::ChatRoom,
    notification::NotificationView,
    rss_entry::{RssEntry, RssEntryView},
};
use uuid::Uuid;

use super::{Headline, InboxRow, headlines, inbox_rows};

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
fn the_inbox_lists_unread_dms_by_count_then_mentions_newest_first() {
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
    let fresh = mention(
        viewer,
        (bob, "bob"),
        with_bob.id,
        None,
        "ping",
        at(5),
        None,
    );
    let rooms = vec![
        (with_alice.clone(), Vec::new()),
        (with_bob.clone(), Vec::new()),
        (with_carol, Vec::new()),
        (with_dave, Vec::new()),
    ];

    let rows = inbox_rows(
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
            InboxRow::Dm {
                room_id: with_bob.id,
                peer: "bob".to_string(),
                unread: 3,
            },
            InboxRow::Dm {
                room_id: with_alice.id,
                peer: "alice".to_string(),
                unread: 1,
            },
            InboxRow::Mention {
                room_id: with_bob.id,
                message_id: fresh.message_id,
                actor: "bob".to_string(),
                room: None,
                preview: "ping".to_string(),
                at: at(5),
                unread: true,
            },
            InboxRow::Mention {
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

fn rss(url: &str, title: &str, feed: &str, published: Option<DateTime<Utc>>) -> RssEntryView {
    RssEntryView {
        entry: RssEntry {
            id: Uuid::now_v7(),
            created: at(20),
            updated: at(20),
            feed_id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            guid: url.to_string(),
            url: url.to_string(),
            title: title.to_string(),
            summary: String::new(),
            published_at: published,
            shared_at: None,
            dismissed_at: None,
        },
        feed_title: feed.to_string(),
        feed_url: format!("{url}/feed"),
    }
}

#[test]
fn headlines_merge_news_and_rss_newest_first_and_a_shared_entry_lists_once() {
    let article = ArticleFeedItem {
        article: Article {
            id: Uuid::now_v7(),
            created: at(10),
            updated: at(10),
            user_id: Uuid::now_v7(),
            url: "https://a.example/one".to_string(),
            title: "Big news".to_string(),
            summary: String::new(),
            ascii_art: String::new(),
        },
        author_username: "mira".to_string(),
    };
    let entries = vec![
        // Shared to News already: the article speaks for it.
        rss("https://a.example/one", "Big news", "a feed", Some(at(9))),
        // No title and no publish date: the link and the fetch time.
        rss("https://b.example/two", "", "b feed", None),
        rss("https://c.example/three", "Old\n  story", "c feed", Some(at(2))),
    ];

    assert_eq!(
        headlines(&[article], &entries),
        vec![
            Headline {
                title: "https://b.example/two".to_string(),
                source: "b feed".to_string(),
                url: "https://b.example/two".to_string(),
                at: at(20),
            },
            Headline {
                title: "Big news".to_string(),
                source: "news · mira".to_string(),
                url: "https://a.example/one".to_string(),
                at: at(10),
            },
            Headline {
                title: "Old story".to_string(),
                source: "c feed".to_string(),
                url: "https://c.example/three".to_string(),
                at: at(2),
            },
        ]
    );
}
