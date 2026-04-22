use late_core::models::user::{PromptProfileContext, User, UserParams};
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
async fn ignored_user_ids_are_parsed_sorted_and_deduped() {
    let (client, _test_db) = setup_db().await;

    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();
    let charlie = Uuid::now_v7();
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-read".to_string(),
            username: "ignore_read_user".to_string(),
            settings: json!({
                "ignored_user_ids": [
                    bob.to_string(),
                    alice.to_string(),
                    alice.to_string(),
                    "",
                    charlie.to_string(),
                    "not-a-uuid",
                ]
            }),
        },
    )
    .await
    .expect("failed to create user");

    let mut expected = vec![alice, bob, charlie];
    expected.sort();
    let ignored = User::ignored_user_ids(&client, user.id)
        .await
        .expect("read ignored user ids");
    assert_eq!(ignored, expected);
}

#[tokio::test]
async fn add_ignored_user_id_preserves_other_settings() {
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

    let target = Uuid::now_v7();
    let (changed, ids) = User::add_ignored_user_id(&client, user.id, target)
        .await
        .expect("add ignored user id");
    assert!(changed);
    assert_eq!(ids, vec![target]);

    let refreshed = User::get(&client, user.id)
        .await
        .expect("get user")
        .expect("user");
    assert_eq!(refreshed.settings["theme"], json!("dark"));
    assert_eq!(
        refreshed.settings["ignored_user_ids"],
        json!([target.to_string()])
    );
}

#[tokio::test]
async fn add_ignored_user_id_reports_already_present_without_duplication() {
    let (client, _test_db) = setup_db().await;

    let target = Uuid::now_v7();
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-dup".to_string(),
            username: "ignore_dup_user".to_string(),
            settings: json!({"ignored_user_ids": [target.to_string()]}),
        },
    )
    .await
    .expect("failed to create user");

    let (changed, ids) = User::add_ignored_user_id(&client, user.id, target)
        .await
        .expect("re-add ignored user id");
    assert!(!changed);
    assert_eq!(ids, vec![target]);

    let ignored = User::ignored_user_ids(&client, user.id)
        .await
        .expect("read ignored user ids");
    assert_eq!(ignored, vec![target]);
}

#[tokio::test]
async fn remove_ignored_user_id_updates_settings() {
    let (client, _test_db) = setup_db().await;

    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-remove".to_string(),
            username: "ignore_remove_user".to_string(),
            settings: json!({
                "ignored_user_ids": [alice.to_string(), bob.to_string()]
            }),
        },
    )
    .await
    .expect("failed to create user");

    let (changed, ids) = User::remove_ignored_user_id(&client, user.id, bob)
        .await
        .expect("remove ignored user id");
    assert!(changed);
    assert_eq!(ids, vec![alice]);

    let refreshed = User::get(&client, user.id)
        .await
        .expect("get user")
        .expect("user");
    assert_eq!(
        refreshed.settings["ignored_user_ids"],
        json!([alice.to_string()])
    );
}

#[tokio::test]
async fn remove_ignored_user_id_reports_missing_entry() {
    let (client, _test_db) = setup_db().await;

    let alice = Uuid::now_v7();
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-ignore-missing".to_string(),
            username: "ignore_missing_user".to_string(),
            settings: json!({"ignored_user_ids": [alice.to_string()]}),
        },
    )
    .await
    .expect("failed to create user");

    let absent = Uuid::now_v7();
    let (changed, ids) = User::remove_ignored_user_id(&client, user.id, absent)
        .await
        .expect("remove missing ignored user id");
    assert!(!changed);
    assert_eq!(ids, vec![alice]);
}

#[tokio::test]
async fn ignored_user_ids_require_existing_user() {
    let (client, _test_db) = setup_db().await;
    let missing_user_id = Uuid::now_v7();

    let err = User::ignored_user_ids(&client, missing_user_id)
        .await
        .expect_err("expected missing user error");
    assert!(err.to_string().to_ascii_lowercase().contains("not found"));
}

#[tokio::test]
async fn list_prompt_profile_context_by_ids_reads_profile_fields_from_settings() {
    let (client, _test_db) = setup_db().await;

    let alice = User::create(
        &client,
        UserParams {
            fingerprint: "fp-prompt-alice".to_string(),
            username: "alice".to_string(),
            settings: json!({
                "bio": "shipping rust + postgres",
                "country": " pl ",
                "timezone": " Europe/Warsaw ",
            }),
        },
    )
    .await
    .expect("create alice");
    let bob = User::create(
        &client,
        UserParams {
            fingerprint: "fp-prompt-bob".to_string(),
            username: "bob".to_string(),
            settings: json!({}),
        },
    )
    .await
    .expect("create bob");

    let contexts = User::list_prompt_profile_context_by_ids(&client, &[alice.id, bob.id])
        .await
        .expect("list prompt profile context");

    assert_eq!(
        contexts.get(&alice.id),
        Some(&PromptProfileContext {
            username: "alice".to_string(),
            bio: "shipping rust + postgres".to_string(),
            country: Some("PL".to_string()),
            timezone: Some("Europe/Warsaw".to_string()),
            is_bot: false,
        })
    );
    assert_eq!(
        contexts.get(&bob.id),
        Some(&PromptProfileContext {
            username: "bob".to_string(),
            bio: String::new(),
            country: None,
            timezone: None,
            is_bot: false,
        })
    );
}
