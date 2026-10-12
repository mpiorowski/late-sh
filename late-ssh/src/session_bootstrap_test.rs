use super::*;
use crate::test_helpers::{new_test_db, publish_test_splash, test_app_state, test_config};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn shared_bootstrap_shows_gallery_splash_only_after_always_view() {
    let test_db = new_test_db().await;
    let state = test_app_state(test_db.db.clone(), test_config(test_db.db.config().clone()));
    publish_test_splash(&state).await;
    let mut user = create_test_user(&test_db.db, "bootstrap-splash-viewer").await;
    // A newcomer has no setting yet: the disclaimer is on and the door is
    // the cup until they choose Always View.
    for (settings, expected) in [
        (serde_json::json!({}), false),
        (serde_json::json!({"artboard_disclaimer": true}), false),
        (serde_json::json!({"artboard_disclaimer": false}), true),
    ] {
        user.settings = settings.clone();
        let config = build_session_config(
            &state,
            SessionBootstrapInputs {
                user: user.clone(),
                is_new_user: false,
                cols: 80,
                rows: 24,
                term: "xterm-256color".to_string(),
                session_token: "splash-bootstrap-test".to_string(),
                session_rx: None,
                activity_feed_rx: None,
                key_fingerprint: None,
            },
        )
        .await;
        assert_eq!(config.splash_piece.is_some(), expected, "{settings}");
    }
}
