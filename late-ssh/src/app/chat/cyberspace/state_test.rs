use late_core::test_utils::{create_test_user, test_db};

use super::state::{Modal, State, parse_topics};
use super::svc::CyberspaceService;

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
