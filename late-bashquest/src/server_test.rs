use super::*;

// The local `Server` struct shadows the trait name, so bring the trait into
// scope anonymously just for `new_client`.
use russh::server::Server as _;

use crate::identity::derive_client_key;

const SECRET: &str = "test-bashquest-secret";

fn test_config() -> Config {
    Config {
        bin: "/usr/local/bin/bashquest.sh".to_string(),
        data_dir: "/var/lib/late-bashquest".to_string(),
        secret: SECRET.to_string(),
        listen_addr: "127.0.0.1".to_string(),
        port: 2330,
        idle_timeout: 3600,
    }
}

fn handler() -> ClientHandler {
    Server::new(&test_config()).new_client(None)
}

/// A client presenting the shared-secret-derived key is accepted, and the
/// playname it authenticated with is recorded. `shell_request` reads that
/// same field to decide whether the session may start a child, so an accepted
/// auth that left it empty would authenticate the player and then refuse to
/// launch the game.
#[tokio::test]
async fn accepted_auth_records_the_playname_the_shell_launches_with() {
    let mut handler = handler();
    let key = derive_client_key(SECRET).public_key().clone();

    let auth = handler
        .auth_publickey("mateu", &key)
        .await
        .expect("auth must not error");

    assert!(matches!(auth, Auth::Accept));
    assert_eq!(handler.playname.as_deref(), Some("mateu"));
}

/// A client with the wrong secret is rejected and leaves no identity behind,
/// so a shell request on that session has nothing to launch with.
#[tokio::test]
async fn rejected_auth_records_no_playname() {
    let mut handler = handler();
    let key = derive_client_key("a-different-secret").public_key().clone();

    let auth = handler
        .auth_publickey("mateu", &key)
        .await
        .expect("auth must not error");

    assert!(matches!(auth, Auth::Reject { .. }));
    assert_eq!(handler.playname, None);
}

/// The username is sanitized before it is stored, so what reaches the child as
/// `BASHQUEST_AUTOLOGIN` is never the raw wire value.
#[tokio::test]
async fn hostile_username_is_sanitized_before_it_is_stored() {
    let mut handler = handler();
    let key = derive_client_key(SECRET).public_key().clone();

    handler
        .auth_publickey("../../etc/passwd", &key)
        .await
        .expect("auth must not error");

    assert_eq!(handler.playname.as_deref(), Some("etcpasswd"));
}
