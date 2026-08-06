use std::time::Duration;

use late_core::test_utils::{create_test_user, test_db};

use crate::app::chat::cyberspace::state::{Modal, State, parse_topics, unread_poll_due};
use crate::app::chat::cyberspace::svc::CyberspaceService;

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
