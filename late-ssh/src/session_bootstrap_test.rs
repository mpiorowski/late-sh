use super::*;
use crate::test_helpers::{new_test_db, publish_test_splash, test_app_state, test_config};
use late_core::models::user::ArtSplashMode;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn shared_bootstrap_applies_saved_splash_mode_after_authentication() {
    let test_db = new_test_db().await;
    let state = test_app_state(test_db.db.clone(), test_config(test_db.db.config().clone()));
    let piece = publish_test_splash(&state).await;
    let mut user = create_test_user(&test_db.db, "bootstrap-splash-viewer").await;
    let client = test_db.db.get().await.unwrap();
    // The canvas was cached before this classification committed.
    client
        .execute(
            "UPDATE artboard_pieces SET owner_marked_nsfw = true WHERE id = $1",
            &[&piece],
        )
        .await
        .unwrap();
    for (mode, expected) in [
        (ArtSplashMode::Sfw, false),
        (ArtSplashMode::Always, true),
        (ArtSplashMode::Never, false),
    ] {
        user.settings = serde_json::json!({"art_splash_mode": mode.as_str()});
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
                supports_reconnect_on_drain: false,
                reconnect_reason: None,
            },
        )
        .await;
        assert_eq!(config.splash_piece.is_some(), expected, "{mode:?}");
    }
}
