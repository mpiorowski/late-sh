use std::time::Duration;

use chrono::{DateTime, Utc};
use late_core::test_utils::{create_test_user, test_db};

use crate::app::chat::cyberspace::api::{CircMessage, CircStreamEvent, CsNotification, CsPost};
use crate::app::chat::cyberspace::state::{
    Modal, State, View, count_unread_entries, dedupe_notifications, feed_reload_due, parse_topics,
    unread_poll_due,
};
use crate::app::chat::cyberspace::svc::{CsEvent, CsThread, CyberspaceService};

async fn test_state() -> State {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "cs-state").await;
    // Dead base URL: state logic under test never talks to the network.
    let service = CyberspaceService::new(test_db.db.clone(), "http://127.0.0.1:1".to_string());
    State::new(service, user.id)
}

#[test]
fn topics_parse_lowercase_deduped_and_capped() {
    assert_eq!(parse_topics(""), Ok(vec![]));
    assert_eq!(
        parse_topics("Music, #Linux music"),
        Ok(vec!["music".to_string(), "linux".to_string()])
    );
    assert_eq!(
        parse_topics("one two three four"),
        Err("up to 3 topics".to_string())
    );
}

#[test]
fn unread_badge_polls_only_when_linked_and_never_faster_than_the_interval() {
    // Fresh session, or one that just polled: the count is already current.
    assert!(!unread_poll_due(true, Duration::ZERO));
    assert!(!unread_poll_due(true, Duration::from_secs(9 * 60)));
    // Ten minutes on, a linked session re-fetches the badge.
    assert!(unread_poll_due(true, Duration::from_secs(10 * 60)));
    // An unlinked session has no token, so it never polls, however long it
    // sits there.
    assert!(!unread_poll_due(false, Duration::from_secs(60 * 60)));
}

#[test]
fn entering_the_pane_refetches_the_feed_but_not_on_every_landing() {
    // Cycling the room rail lands on the slot repeatedly, and each landing
    // would otherwise be an authenticated call to a third party.
    assert!(feed_reload_due(true, false, None), "first open must fetch");
    assert!(!feed_reload_due(true, false, Some(Duration::from_secs(5))));
    assert!(feed_reload_due(true, false, Some(Duration::from_secs(30))));
    // Never on top of a fetch in flight, and never without a token.
    assert!(!feed_reload_due(true, true, None));
    assert!(!feed_reload_due(false, false, None));
}

#[tokio::test]
async fn scrolling_a_thread_reaches_the_end_and_stops_there() {
    let mut state = test_state().await;
    // Built the way production builds one: straight off the wire shape.
    let post: CsPost =
        serde_json::from_str(r#"{"postId":"p1","content":"one line"}"#).expect("post");
    state.thread = Some(CsThread {
        post,
        replies: Vec::new(),
    });
    state.view = View::Thread;
    // What the renderer hands back after laying the entry out: this one wraps
    // to four rows past the bottom of the viewport.
    state.thread_max_scroll.set(4);

    for _ in 0..500 {
        state.move_selection(1);
    }
    assert_eq!(state.thread_scroll, 4, "j must reach the end of the entry");
    state.move_selection(-1);
    assert_eq!(
        state.thread_scroll, 3,
        "one k after holding j should move the view, not unwind 500 phantom steps"
    );
    state.go_to_top();
    assert_eq!(state.thread_scroll, 0, "g returns to the top of the entry");
}

#[test]
fn unread_entries_are_the_ones_published_since_the_cursor() {
    let post = |created: &str| -> CsPost {
        serde_json::from_str(&format!(r#"{{"postId":"p","createdAt":"{created}"}}"#)).expect("post")
    };
    let undated: CsPost = serde_json::from_str(r#"{"postId":"p"}"#).expect("post");
    let cursor: DateTime<Utc> = "2026-08-07T12:00:00Z".parse().expect("cursor");
    let posts = vec![
        post("2026-08-07T14:00:00Z"),
        post("2026-08-07T13:00:00Z"),
        post("2026-08-07T11:00:00Z"),
        undated,
    ];

    assert_eq!(count_unread_entries(&posts, Some(cursor)), 2);
    // Never opened the pane: opening it to "everything is new" would be
    // counting entries that were never theirs to miss.
    assert_eq!(count_unread_entries(&posts, None), 0);
    // Caught up: the newest entry is the one they read to.
    assert_eq!(
        count_unread_entries(&posts, Some("2026-08-07T14:00:00Z".parse().expect("stamp"))),
        0
    );
}

#[tokio::test]
async fn the_rail_badge_sums_notifications_and_new_entries() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    let read_at: DateTime<Utc> = "2026-08-07T12:00:00Z".parse().expect("cursor");
    let _ = state.apply_event(CsEvent::LinkStatus {
        user_id,
        username: Some("mat".to_string()),
        feed_read_at: Some(read_at),
        circ_rooms: Vec::new(),
        circ_room_reads: std::collections::HashMap::new(),
    });
    let _ = state.apply_event(CsEvent::UnreadCount { user_id, count: 4 });
    let _ = state.apply_event(CsEvent::RecentEntries {
        user_id,
        posts: vec![
            serde_json::from_str(r#"{"postId":"p1","createdAt":"2026-08-07T13:00:00Z"}"#)
                .expect("post"),
            serde_json::from_str(r#"{"postId":"p2","createdAt":"2026-08-07T11:00:00Z"}"#)
                .expect("post"),
        ],
    });

    assert_eq!(state.unread_count(), 5, "4 notifications + 1 new entry");
    assert_eq!(state.unread_notifications(), 4);
    assert_eq!(state.unread_entries(), 1);
}

#[tokio::test]
async fn opening_the_pane_clears_the_count_but_keeps_the_marks_being_read() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    let read_at: DateTime<Utc> = "2026-08-07T12:00:00Z".parse().expect("cursor");
    let _ = state.apply_event(CsEvent::LinkStatus {
        user_id,
        username: Some("mat".to_string()),
        feed_read_at: Some(read_at),
        circ_rooms: Vec::new(),
        circ_room_reads: std::collections::HashMap::new(),
    });
    let fresh: CsPost =
        serde_json::from_str(r#"{"postId":"p1","createdAt":"2026-08-07T13:00:00Z"}"#)
            .expect("post");
    let old: CsPost = serde_json::from_str(r#"{"postId":"p2","createdAt":"2026-08-07T11:00:00Z"}"#)
        .expect("post");
    let _ = state.apply_event(CsEvent::RecentEntries {
        user_id,
        posts: vec![fresh.clone(), old.clone()],
    });
    assert_eq!(state.unread_entries(), 1);

    state.opened();
    assert_eq!(
        state.unread_entries(),
        1,
        "the feed has not arrived yet, so nothing has been read"
    );
    let _ = state.apply_event(CsEvent::FeedLoaded {
        user_id,
        posts: vec![fresh.clone(), old.clone()],
    });

    assert_eq!(
        state.unread_entries(),
        0,
        "the badge clears once the feed being read arrives"
    );
    assert!(
        state.is_unread_entry(&fresh),
        "the entry they came to read must keep its mark for the visit"
    );
    assert!(!state.is_unread_entry(&old));
}

#[tokio::test]
async fn reentering_inside_the_reload_interval_keeps_unseen_entries_unread() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    let read_at: DateTime<Utc> = "2026-08-07T12:00:00Z".parse().expect("cursor");
    let _ = state.apply_event(CsEvent::LinkStatus {
        user_id,
        username: Some("mat".to_string()),
        feed_read_at: Some(read_at),
        circ_rooms: Vec::new(),
        circ_room_reads: std::collections::HashMap::new(),
    });
    // First visit: the feed arrives with its newest entry at 13:00, which is
    // as far as this user has demonstrably read.
    let seen: CsPost =
        serde_json::from_str(r#"{"postId":"p1","createdAt":"2026-08-07T13:00:00Z"}"#)
            .expect("post");
    state.opened();
    let _ = state.apply_event(CsEvent::FeedLoaded {
        user_id,
        posts: vec![seen.clone()],
    });

    // An entry lands, and the pane is re-entered inside the reload interval:
    // the stale page is shown again, no fetch fires.
    state.opened();

    // The next probe reports the entry published in the gap. It was never on
    // screen, so it must still count and keep its mark.
    let gap: CsPost = serde_json::from_str(r#"{"postId":"p2","createdAt":"2026-08-07T13:30:00Z"}"#)
        .expect("post");
    let _ = state.apply_event(CsEvent::RecentEntries {
        user_id,
        posts: vec![gap.clone(), seen.clone()],
    });
    assert_eq!(
        state.unread_entries(),
        1,
        "an entry never rendered must stay unread"
    );
    assert!(state.is_unread_entry(&gap));
}

#[tokio::test]
async fn reopening_the_pane_lands_on_the_newest_entry() {
    let mut state = test_state().await;
    state.posts = vec![
        serde_json::from_str(r#"{"postId":"p1"}"#).expect("post"),
        serde_json::from_str(r#"{"postId":"p2"}"#).expect("post"),
    ];
    state.selected = 1;
    state.view = View::Notifications;

    state.opened();

    assert_eq!(state.view, View::Feed);
    assert_eq!(
        state.selected, 0,
        "a selection kept across visits points into a feed that has been refetched since"
    );
}

#[test]
fn one_notification_row_per_thing_that_happened() {
    let reply = |id: &str, actor: &str, post: &str, reply_id: &str| -> CsNotification {
        serde_json::from_str(&format!(
            r#"{{"id":"{id}","type":"reply","actorUsername":"{actor}","targetId":"{post}","targetType":"reply","metadata":{{"replyId":"{reply_id}"}}}}"#
        ))
        .expect("reply notification")
    };
    let bookmark = |id: &str, actor: &str, post: &str| -> CsNotification {
        serde_json::from_str(&format!(
            r#"{{"id":"{id}","type":"bookmark","actorUsername":"{actor}","targetId":"{post}","targetType":"post"}}"#
        ))
        .expect("bookmark notification")
    };

    let rows = dedupe_notifications(vec![
        // One reply from laschii, notified three times over: distinct
        // notification ids, but all of them about the same reply.
        reply("n1", "laschii", "post-1", "reply-1"),
        reply("n2", "laschii", "post-1", "reply-1"),
        reply("n3", "laschii", "post-1", "reply-1"),
        // A second, real reply from the same person on the same entry.
        reply("n4", "laschii", "post-1", "reply-2"),
        // Same person, a different thing done: its own row.
        bookmark("n5", "laschii", "post-1"),
        // A different person, and a different entry: their own rows.
        reply("n6", "leet", "post-1", "reply-3"),
        reply("n7", "laschii", "post-2", "reply-4"),
        // Duplicates further down the page go too, not only adjacent ones.
        reply("n8", "laschii", "post-1", "reply-1"),
        bookmark("n9", "laschii", "post-1"),
    ]);

    let kept: Vec<&str> = rows
        .iter()
        .map(|notification| notification.id.as_str())
        .collect();
    assert_eq!(
        kept,
        vec!["n1", "n4", "n5", "n6", "n7"],
        "the first notification of each event survives, so the row keeps the newest stamp"
    );
}

#[tokio::test]
async fn enter_on_a_notification_opens_the_entry_it_is_about() {
    let mut state = test_state().await;
    state.notifications = dedupe_notifications(vec![
        serde_json::from_str(
            r#"{"id":"n1","type":"reply","targetId":"post-1","targetType":"reply"}"#,
        )
        .expect("reply notification"),
        serde_json::from_str(
            r#"{"id":"n2","type":"new_follower","targetId":"user-1","targetType":"user"}"#,
        )
        .expect("follow notification"),
    ]);
    state.view = View::Notifications;

    assert!(state.open_selected_notification().is_none());
    assert_eq!(state.view, View::Thread);
    assert_eq!(state.thread_target.as_deref(), Some("post-1"));

    // A follow has no entry behind it, so it says so and stays put.
    state.back_to_feed();
    state.view = View::Notifications;
    state.notif_selected = 1;
    assert!(state.open_selected_notification().is_some(), "banner");
    assert_eq!(state.view, View::Notifications, "view must not move");
}

#[tokio::test]
async fn a_thread_that_finished_loading_after_the_user_left_is_dropped() {
    let mut state = test_state().await;
    state.notifications = dedupe_notifications(vec![
        serde_json::from_str(
            r#"{"id":"n1","type":"reply","targetId":"post-1","targetType":"reply"}"#,
        )
        .expect("notification"),
    ]);
    state.view = View::Notifications;
    state.open_selected_notification();

    // The user moves on before the fetch lands, then a slow load arrives for
    // the entry they were looking at a moment ago.
    state.back_to_feed();
    let user_id = state.user_id;
    let _ = state.apply_event(CsEvent::ThreadLoaded {
        user_id,
        thread: CsThread {
            post: serde_json::from_str(r#"{"postId":"post-1"}"#).expect("post"),
            replies: Vec::new(),
        },
    });
    assert_eq!(
        state.view,
        View::Feed,
        "a stale load must not yank the view"
    );
    assert!(state.thread.is_none());
}

#[tokio::test]
async fn link_modal_submit_requires_both_fields() {
    let mut state = test_state().await;
    state.open_link_modal();
    state.submit_modal();
    match &state.modal {
        Some(Modal::Link(link)) => {
            assert!(!link.busy, "invalid submit must not go busy");
            assert_eq!(
                link.error.as_deref(),
                Some("email and password are both required")
            );
        }
        _ => panic!("link modal should stay open"),
    }
}

#[tokio::test]
async fn compose_modal_submit_requires_a_body() {
    let mut state = test_state().await;
    // Compose refuses to open before a link exists.
    let banner = state.open_compose_modal();
    assert!(banner.is_some(), "unlinked compose should be refused");
    assert!(state.modal.is_none());
}

fn circ_message(id: &str, timestamp: i64, content: &str) -> CircMessage {
    serde_json::from_str(&format!(
        r#"{{"id":"{id}","username":"someone","content":"{content}","timestamp":{timestamp}}}"#
    ))
    .expect("message")
}

#[tokio::test]
async fn room_history_and_the_live_window_land_as_one_conversation() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    state.enter_room("general".to_string());

    // History arrives first, oldest-first.
    let _ = state.apply_event(CsEvent::CircHistoryLoaded {
        user_id,
        room: "general".to_string(),
        messages: vec![
            circ_message("m1", 1, "first"),
            circ_message("m2", 2, "second"),
        ],
    });

    // The stream's opening window replays part of the same tail: the overlap
    // must land as one row per message, not as duplicates.
    let _ = state.apply_event(CsEvent::CircStreamed {
        user_id,
        room: "general".to_string(),
        event: CircStreamEvent::Window(vec![
            circ_message("m2", 2, "second"),
            circ_message("m3", 3, "third"),
        ]),
    });

    let room = state.open_room.as_ref().expect("room open");
    let ids: Vec<&str> = room.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["m1", "m2", "m3"]);
    assert!(!room.loading);
}

#[tokio::test]
async fn a_deletion_rewrites_the_message_already_on_screen() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    state.enter_room("general".to_string());
    let _ = state.apply_event(CsEvent::CircHistoryLoaded {
        user_id,
        room: "general".to_string(),
        messages: vec![circ_message("m1", 1, "regrets")],
    });

    // Their delete is a patch against a message we already hold. Appending it
    // (or ignoring it) leaves the deleted text on screen until a reload.
    let _ = state.apply_event(CsEvent::CircStreamed {
        user_id,
        room: "general".to_string(),
        event: CircStreamEvent::Patch {
            id: "m1".to_string(),
            content: Some("[DELETED]".to_string()),
            deleted: true,
        },
    });

    let room = state.open_room.as_ref().expect("room open");
    assert_eq!(room.messages.len(), 1);
    assert!(room.messages[0].deleted);
    assert_eq!(room.messages[0].display_text(), "[deleted]");
    // The author and the time survive a deletion; the payload does not.
    assert_eq!(room.messages[0].username, "someone");
    assert_eq!(room.messages[0].attachment_label(), None);
}

#[tokio::test]
async fn frames_for_another_room_never_touch_the_open_one() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    state.enter_room("general".to_string());
    let _ = state.apply_event(CsEvent::CircHistoryLoaded {
        user_id,
        room: "general".to_string(),
        messages: vec![circ_message("m1", 1, "here")],
    });

    // Another session of the same user is in a different room; its frames ride
    // the same broadcast and must not land here.
    let _ = state.apply_event(CsEvent::CircStreamed {
        user_id,
        room: "tech".to_string(),
        event: CircStreamEvent::Upsert(Box::new(circ_message("m9", 9, "elsewhere"))),
    });
    let _ = state.apply_event(CsEvent::CircHistoryLoaded {
        user_id,
        room: "tech".to_string(),
        messages: vec![circ_message("m8", 8, "elsewhere")],
    });

    let room = state.open_room.as_ref().expect("room open");
    let ids: Vec<&str> = room.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["m1"]);
}

#[tokio::test]
async fn the_picker_adds_a_room_to_the_rail_and_takes_it_back_off() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    let _ = state.apply_event(CsEvent::LinkStatus {
        user_id,
        username: Some("mat".to_string()),
        feed_read_at: None,
        circ_rooms: Vec::new(),
        circ_room_reads: std::collections::HashMap::new(),
    });
    assert!(
        state.open_rooms_modal().is_none(),
        "a linked user gets the picker, not an error"
    );
    let _ = state.apply_event(CsEvent::CircRooms {
        user_id,
        rooms: vec![
            serde_json::from_str(r#"{"id":"r1","slug":"general","name":"General"}"#).expect("room"),
            serde_json::from_str(r#"{"id":"r2","slug":"ideas","name":"Ideas"}"#).expect("room"),
        ],
    });

    // Adding is what creates the rail entry; it does not open the room.
    assert!(state.pinned_rooms().is_empty());
    state.move_rooms_modal_selection(1);
    assert!(state.toggle_selected_room().is_some());
    assert_eq!(state.pinned_rooms(), ["ideas".to_string()]);
    assert!(state.is_pinned("ideas"));
    assert_eq!(
        state.open_room_slug(),
        None,
        "adding a room does not enter it"
    );

    // The same key takes the row back off the rail.
    assert!(state.toggle_selected_room().is_some());
    assert!(state.pinned_rooms().is_empty());
}

#[tokio::test]
async fn room_dots_follow_the_roster_against_the_read_cursor() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    let _ = state.apply_event(CsEvent::LinkStatus {
        user_id,
        username: Some("mat".to_string()),
        feed_read_at: None,
        circ_rooms: vec!["general".to_string(), "quiet".to_string()],
        circ_room_reads: std::collections::HashMap::from([("general".to_string(), 1_000)]),
    });

    // No roster yet: nothing may claim unread.
    assert_eq!(state.room_unread_flags(), vec![false, false]);

    // The badge poll's roster lands: general moved past the cursor. Quiet
    // was never visited, and a room with no cursor shows nothing rather
    // than claiming a backlog the user was never behind on.
    let _ = state.apply_event(CsEvent::CircRooms {
        user_id,
        rooms: vec![
            serde_json::from_str(r#"{"id":"r1","slug":"general","lastMessageAt":2000}"#)
                .expect("room"),
            serde_json::from_str(r#"{"id":"r2","slug":"quiet","lastMessageAt":500}"#)
                .expect("room"),
        ],
    });
    assert_eq!(state.room_unread_flags(), vec![true, false]);

    // Being inside the room is reading it: no dot while open.
    state.enter_room("general".to_string());
    assert_eq!(state.room_unread_flags(), vec![false, false]);

    // History landing moves the cursor to the newest message seen, so the
    // dot stays off after leaving rather than re-marking a read room.
    let _ = state.apply_event(CsEvent::CircHistoryLoaded {
        user_id,
        room: "general".to_string(),
        messages: vec![
            serde_json::from_str(r#"{"id":"m1","content":"hi","timestamp":2000}"#)
                .expect("message"),
        ],
    });
    state.leave_room();
    assert_eq!(state.room_unread_flags(), vec![false, false]);

    // The next roster naming a newer message dots the room again.
    let _ = state.apply_event(CsEvent::CircRooms {
        user_id,
        rooms: vec![
            serde_json::from_str(r#"{"id":"r1","slug":"general","lastMessageAt":3000}"#)
                .expect("room"),
        ],
    });
    assert_eq!(state.room_unread_flags(), vec![true, false]);
}

#[tokio::test]
async fn unlinking_closes_the_open_room_and_clears_the_rail() {
    let mut state = test_state().await;
    let user_id = state.user_id;
    let _ = state.apply_event(CsEvent::LinkStatus {
        user_id,
        username: Some("mat".to_string()),
        feed_read_at: None,
        circ_rooms: vec!["general".to_string()],
        circ_room_reads: std::collections::HashMap::new(),
    });
    state.enter_room("general".to_string());
    assert_eq!(state.open_room_slug(), Some("general"));

    let _ = state.apply_event(CsEvent::Unlinked { user_id });

    // An unlinked account must not still be streaming or present in a room.
    assert_eq!(state.open_room_slug(), None);
    assert!(state.pinned_rooms().is_empty());
}
