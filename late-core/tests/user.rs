use late_core::models::user::{User, UserParams};
use late_core::test_utils::{TestDb, test_db};
use serde_json::json;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

async fn setup_db() -> (deadpool_postgres::Client, TestDb) {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    client
        .execute(
            "CREATE TEMP TABLE users (
            id uuid primary key default uuidv7(),
            created timestamptz not null default current_timestamp,
            updated timestamptz not null default current_timestamp,
            last_seen timestamptz not null default current_timestamp,
            is_admin boolean not null default false,
            fingerprint text not null,
            username text not null default '',
            settings jsonb not null default '{}',
            unique (fingerprint)
        )",
            &[],
        )
        .await
        .expect("failed to create temp users table");

    (client, test_db)
}

#[tokio::test]
async fn user_fingerprint_lookup() {
    let (client, _test_db) = setup_db().await;

    let fingerprint = "fp-test-123";

    let created = User::create(
        &client,
        UserParams {
            fingerprint: fingerprint.to_string(),
            username: "test_user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("failed to create user");

    let found = User::find_by_fingerprint(&client, fingerprint)
        .await
        .expect("lookup failed")
        .expect("user not found");

    assert_eq!(found.id, created.id);
    assert_eq!(found.fingerprint, fingerprint);
}

#[tokio::test]
async fn user_last_seen_updates_without_touching_updated() {
    let (client, _test_db) = setup_db().await;

    let mut user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-presence".to_string(),
            username: "presence_user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("failed to create user");

    let initial_updated = user.updated;
    let initial_last_seen = user.last_seen;

    sleep(Duration::from_millis(50)).await;

    user.update_last_seen(&client)
        .await
        .expect("failed to update last_seen");

    let fresh = User::get(&client, user.id)
        .await
        .expect("get failed")
        .unwrap();

    assert!(
        fresh.last_seen > initial_last_seen,
        "last_seen should have increased"
    );
    assert_eq!(
        fresh.updated, initial_updated,
        "updated should NOT have changed when only updating presence"
    );
}

#[tokio::test]
async fn user_update_modifies_updated_timestamp() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-edit".to_string(),
            username: "edit_user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("failed to create user");

    let initial_updated = user.updated;

    sleep(Duration::from_millis(50)).await;

    let updated_user = User::update(
        &client,
        user.id,
        UserParams {
            fingerprint: "fp-edit".to_string(),
            username: "edited_user".to_string(),
            settings: serde_json::json!({"theme": "dark"}),
        },
    )
    .await
    .expect("failed to update user");

    assert!(
        updated_user.updated > initial_updated,
        "updated timestamp SHOULD have increased after profile edit"
    );
    assert_eq!(updated_user.username, "edited_user");
}

#[tokio::test]
async fn ignored_usernames_are_normalized_sorted_and_deduped() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-read".to_string(),
            username: "ignore_read_user".to_string(),
            settings: json!({
                "ignored_usernames": [" Bob ", "alice", "ALICE", "", "charlie"]
            }),
        },
    )
    .await
    .expect("failed to create user");

    let ignored = User::ignored_usernames(&client, user.id)
        .await
        .expect("read ignored usernames");
    assert_eq!(ignored, vec!["alice", "bob", "charlie"]);
}

#[tokio::test]
async fn add_ignored_username_preserves_other_settings() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-add".to_string(),
            username: "ignore_add_user".to_string(),
            settings: json!({"theme": "dark"}),
        },
    )
    .await
    .expect("failed to create user");

    let update = User::add_ignored_username(&client, user.id, " Alice ")
        .await
        .expect("add ignored username");
    match update {
        late_core::models::user::IgnoreListMutation::Added {
            username,
            ignored_usernames,
        } => {
            assert_eq!(username, "alice");
            assert_eq!(ignored_usernames, vec!["alice"]);
        }
        other => panic!("expected Added mutation, got {other:?}"),
    }

    let refreshed = User::get(&client, user.id)
        .await
        .expect("get user")
        .expect("user");
    assert_eq!(refreshed.settings["theme"], json!("dark"));
    assert_eq!(refreshed.settings["ignored_usernames"], json!(["alice"]));
}

#[tokio::test]
async fn add_ignored_username_reports_already_present_without_duplication() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-dup".to_string(),
            username: "ignore_dup_user".to_string(),
            settings: json!({"ignored_usernames": ["alice"]}),
        },
    )
    .await
    .expect("failed to create user");

    let update = User::add_ignored_username(&client, user.id, "@ALICE")
        .await
        .expect("re-add ignored username");
    match update {
        late_core::models::user::IgnoreListMutation::AlreadyPresent {
            username,
            ignored_usernames,
        } => {
            assert_eq!(username, "alice");
            assert_eq!(ignored_usernames, vec!["alice"]);
        }
        other => panic!("expected AlreadyPresent mutation, got {other:?}"),
    }

    let ignored = User::ignored_usernames(&client, user.id)
        .await
        .expect("read ignored usernames");
    assert_eq!(ignored, vec!["alice"]);
}

#[tokio::test]
async fn remove_ignored_username_updates_settings() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-remove".to_string(),
            username: "ignore_remove_user".to_string(),
            settings: json!({"ignored_usernames": ["alice", "bob"]}),
        },
    )
    .await
    .expect("failed to create user");

    let update = User::remove_ignored_username(&client, user.id, "bob")
        .await
        .expect("remove ignored username");
    match update {
        late_core::models::user::IgnoreListMutation::Removed {
            username,
            ignored_usernames,
        } => {
            assert_eq!(username, "bob");
            assert_eq!(ignored_usernames, vec!["alice"]);
        }
        other => panic!("expected Removed mutation, got {other:?}"),
    }

    let refreshed = User::get(&client, user.id)
        .await
        .expect("get user")
        .expect("user");
    assert_eq!(refreshed.settings["ignored_usernames"], json!(["alice"]));
}

#[tokio::test]
async fn remove_ignored_username_reports_missing_entry() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-missing".to_string(),
            username: "ignore_missing_user".to_string(),
            settings: json!({"ignored_usernames": ["alice"]}),
        },
    )
    .await
    .expect("failed to create user");

    let update = User::remove_ignored_username(&client, user.id, "bob")
        .await
        .expect("remove missing ignored username");
    match update {
        late_core::models::user::IgnoreListMutation::Missing {
            username,
            ignored_usernames,
        } => {
            assert_eq!(username, "bob");
            assert_eq!(ignored_usernames, vec!["alice"]);
        }
        other => panic!("expected Missing mutation, got {other:?}"),
    }
}

#[tokio::test]
async fn ignored_usernames_require_existing_user() {
    let (client, _test_db) = setup_db().await;
    let missing_user_id = Uuid::now_v7();

    let err = User::ignored_usernames(&client, missing_user_id)
        .await
        .expect_err("expected missing user error");
    assert!(err.to_string().contains("User not found"));
}
