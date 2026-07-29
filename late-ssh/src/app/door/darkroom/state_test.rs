use chrono::Utc;
use late_core::test_utils::create_test_user;

use super::state::{Row, State};
use super::svc::DarkroomService;
use crate::test_helpers::new_test_db;

/// The load channel's sender is dropped as soon as the loader has sent, so a
/// receiver that gates on `has_changed()` sees `Err` and never picks the game
/// up — the session sits on "the dark is quiet..." forever. `tick` must read
/// the value instead.
#[tokio::test]
async fn a_session_picks_up_its_save_and_offers_the_fire() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-load").await;
    let svc = DarkroomService::new(test_db.db.clone());

    let mut state = State::new(svc, user.id, Utc::now());
    assert!(state.game().is_none(), "the load starts in flight");

    // Poll the way the render loop does, rather than through `wait_until`,
    // which cannot hold the mutable borrow across its async closure.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while state.game().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the darkroom save to load"
        );
        state.tick();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // A fresh save is a dead fire in a dark room, so the only thing to do is
    // light it.
    assert_eq!(state.rows(), vec![Row::LightFire, Row::Leave]);
}
