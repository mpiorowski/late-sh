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
                supports_reconnect_on_drain: false,
                reconnect_reason: None,
            },
        )
        .await;
        assert_eq!(config.splash_piece.is_some(), expected, "{settings}");
    }
}

#[tokio::test]
async fn shared_bootstrap_reads_the_disclaimer_of_this_device() {
    use late_core::models::user_ssh_key::UserSshKey;

    let test_db = new_test_db().await;
    let state = test_app_state(test_db.db.clone(), test_config(test_db.db.config().clone()));
    publish_test_splash(&state).await;
    let mut user = create_test_user(&test_db.db, "bootstrap-device-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    for fingerprint in ["SHA256:home", "SHA256:work", "SHA256:fresh"] {
        UserSshKey::ensure(&client, user.id, fingerprint)
            .await
            .expect("key");
    }
    UserSshKey::set_artboard_disclaimer(&client, user.id, "SHA256:home", false)
        .await
        .expect("home off");
    UserSshKey::set_artboard_disclaimer(&client, user.id, "SHA256:work", true)
        .await
        .expect("work on");

    // The device's own choice wins either way; a key with none follows the
    // account, and so does a keyless session.
    for (account, fingerprint, stored, splash) in [
        (true, Some("SHA256:home"), Some(false), true),
        (false, Some("SHA256:work"), Some(true), false),
        (true, Some("SHA256:fresh"), None, false),
        (false, Some("SHA256:fresh"), None, true),
        (false, None, None, true),
    ] {
        user.settings = serde_json::json!({ "artboard_disclaimer": account });
        let config = build_session_config(
            &state,
            SessionBootstrapInputs {
                user: user.clone(),
                is_new_user: false,
                cols: 80,
                rows: 24,
                term: "xterm-256color".to_string(),
                session_token: "device-disclaimer-bootstrap-test".to_string(),
                session_rx: None,
                activity_feed_rx: None,
                key_fingerprint: fingerprint.map(str::to_string),
                supports_reconnect_on_drain: false,
                reconnect_reason: None,
            },
        )
        .await;
        assert_eq!(config.key_artboard_disclaimer, stored, "{fingerprint:?}");
        assert_eq!(
            config.splash_piece.is_some(),
            splash,
            "account={account} key={fingerprint:?}"
        );
    }
}
