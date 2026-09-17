use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;
use tokio::time::{Duration, Instant, sleep};

use super::BonsaiSession;
use crate::app::activity::event::ActivityEvent;
use crate::app::bonsai::state::{BonsaiAction, BonsaiState};
use crate::app::bonsai::svc::BonsaiService;
use crate::test_helpers::new_test_db;

async fn tick_until(
    session: &mut BonsaiSession,
    label: &str,
    done: impl Fn(&BonsaiSession) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        session.tick();
        if done(session) {
            return;
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for: {label}");
}

// Two sessions of one account, both opened before anyone watered. The
// second one's mirror follows the first one's watering, and its own `w`
// is refused by the stored row rather than grown from a stale copy.
#[tokio::test]
async fn a_second_session_follows_the_first_ones_watering() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-session-pair").await;
    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    let mut first = BonsaiSession::new(
        user.id,
        svc.clone(),
        BonsaiState::view_only(tree.clone(), None),
    );
    let mut second = BonsaiSession::new(user.id, svc.clone(), BonsaiState::view_only(tree, None));
    let today = BonsaiService::today();

    first.request(BonsaiAction::Water);
    tick_until(&mut first, "the first session's answer", |session| {
        session.tree.last_watered == Some(today)
    })
    .await;
    assert!(
        first
            .tree
            .message
            .as_deref()
            .is_some_and(|message| message.starts_with("Watered (+"))
    );

    // No LISTEN connection in tests: deliver the notice the commit sent.
    svc.publish_change(&user.id.to_string());
    tick_until(&mut second, "the second session's reload", |session| {
        session.tree.last_watered == Some(today)
    })
    .await;
    assert_eq!(second.tree.revision, first.tree.revision);
    assert_eq!(second.tree.vigor, first.tree.vigor);
    assert_eq!(
        second.tree.graph.branches.len(),
        first.tree.graph.branches.len()
    );

    second.request(BonsaiAction::Water);
    tick_until(&mut second, "the second session's refusal", |session| {
        session.tree.message.as_deref() == Some("Already watered today")
    })
    .await;
    assert_eq!(second.tree.revision, first.tree.revision);
}

// A change notice for somebody else's tree costs this session nothing.
#[tokio::test]
async fn another_users_change_does_not_reload() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-session-other").await;
    let (tx, _rx) = broadcast::channel::<ActivityEvent>(16);
    let svc = BonsaiService::new(test_db.db.clone(), tx);
    let tree = svc.ensure_tree(user.id).await.expect("ensure tree");
    let mut session = BonsaiSession::new(user.id, svc.clone(), BonsaiState::view_only(tree, None));

    svc.publish_change(&uuid::Uuid::now_v7().to_string());

    assert!(!session.own_tree_changed());
}
