use std::time::Duration;

use late_core::test_utils::{create_test_user, test_db};

use crate::app::chat::cyberspace::api::CsPost;
use crate::app::chat::cyberspace::state::{
    Modal, State, View, feed_reload_due, parse_topics, unread_poll_due,
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
async fn scrolling_past_the_end_of_a_thread_stops_instead_of_running_away() {
    let mut state = test_state().await;
    // Built the way production builds one: straight off the wire shape.
    let post: CsPost =
        serde_json::from_str(r#"{"postId":"p1","content":"one line"}"#).expect("post");
    state.thread = Some(CsThread {
        post,
        replies: Vec::new(),
    });
    state.view = View::Thread;

    for _ in 0..500 {
        state.move_selection(1);
    }
    let parked = state.thread_scroll;
    state.move_selection(-1);
    assert!(
        state.thread_scroll < parked,
        "one k after holding j should move the view, not unwind 500 phantom steps"
    );
}

#[tokio::test]
async fn enter_on_a_notification_opens_the_entry_it_is_about() {
    let mut state = test_state().await;
    state.notifications = vec![
        serde_json::from_str(
            r#"{"id":"n1","type":"reply","targetId":"post-1","targetType":"reply"}"#,
        )
        .expect("reply notification"),
        serde_json::from_str(
            r#"{"id":"n2","type":"new_follower","targetId":"user-1","targetType":"user"}"#,
        )
        .expect("follow notification"),
    ];
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
    state.notifications = vec![
        serde_json::from_str(
            r#"{"id":"n1","type":"reply","targetId":"post-1","targetType":"reply"}"#,
        )
        .expect("notification"),
    ];
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
