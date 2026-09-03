use crate::models::user::{User, UserParams};
use crate::test_utils::{TestDb, test_db};
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
            is_moderator boolean not null default false,
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
async fn add_friend_user_id_preserves_other_settings() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-friend-add".to_string(),
            username: "friend_add_user".to_string(),
            settings: json!({"theme": "late"}),
        },
    )
    .await
    .expect("failed to create user");

    let target = Uuid::now_v7();
    let (changed, ids) = User::add_friend_user_id(&client, user.id, target)
        .await
        .expect("add friend user id");
    assert!(changed);
    assert_eq!(ids, vec![target]);

    let refreshed = User::get(&client, user.id)
        .await
        .expect("get user")
        .expect("user");
    assert_eq!(refreshed.settings["theme"], json!("late"));
    assert_eq!(
        refreshed.settings["friend_user_ids"],
        json!([target.to_string()])
    );
}

#[tokio::test]
async fn friend_user_ids_are_private_and_removable() {
    let (client, _test_db) = setup_db().await;

    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-friend-remove".to_string(),
            username: "friend_remove_user".to_string(),
            settings: json!({"friend_user_ids": [alice.to_string(), bob.to_string()]}),
        },
    )
    .await
    .expect("failed to create user");

    let (changed, ids) = User::remove_friend_user_id(&client, user.id, bob)
        .await
        .expect("remove friend user id");
    assert!(changed);
    assert_eq!(ids, vec![alice]);

    let friends = User::friend_user_ids(&client, user.id)
        .await
        .expect("read friend user ids");
    assert_eq!(friends, vec![alice]);
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
async fn friend_and_ignored_user_ids_reads_both_lists_from_one_row() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let make = async |fingerprint: &str, username: &str| {
        User::create(
            &client,
            UserParams {
                fingerprint: fingerprint.to_string(),
                username: username.to_string(),
                settings: json!({}),
            },
        )
        .await
        .expect("create user")
    };
    let owner = make("combined-lists-owner", "combined_owner").await;
    let friend = make("combined-lists-friend", "combined_friend").await;
    let ignored = make("combined-lists-ignored", "combined_ignored").await;

    User::add_friend_user_id(&client, owner.id, friend.id)
        .await
        .expect("add friend");
    User::add_ignored_user_id(&client, owner.id, ignored.id)
        .await
        .expect("add ignored");

    let (friends, ignores) = User::friend_and_ignored_user_ids(&client, owner.id)
        .await
        .expect("combined lists");

    assert_eq!(friends, vec![friend.id]);
    assert_eq!(ignores, vec![ignored.id]);
    assert_eq!(
        friends,
        User::friend_user_ids(&client, owner.id)
            .await
            .expect("friends"),
        "the combined read must agree with the single-list helper"
    );
    assert_eq!(
        ignores,
        User::ignored_user_ids(&client, owner.id)
            .await
            .expect("ignores"),
        "the combined read must agree with the single-list helper"
    );
}

async fn create_first_contact_user(
    client: &deadpool_postgres::Client,
    settings: serde_json::Value,
) -> User {
    User::create(
        client,
        UserParams {
            fingerprint: format!("fp-first-contact-{}", Uuid::now_v7()),
            username: "haunted".to_string(),
            settings,
        },
    )
    .await
    .expect("failed to create user")
}

#[tokio::test]
async fn first_contact_hit_claim_enforces_both_caps_in_the_row() {
    use crate::models::user::{FirstContactHitCaps, FirstContactHitClaim};
    let (client, _test_db) = setup_db().await;
    let user = create_first_contact_user(&client, json!({})).await;
    let caps = FirstContactHitCaps { daily: 2, total: 3 };
    let day1 = chrono::NaiveDate::from_ymd_opt(2026, 9, 2).unwrap();
    let day2 = day1.succ_opt().unwrap();

    // Two wins on day one, then the daily cap holds whatever the caller
    // believes about its own session.
    assert_eq!(
        User::claim_first_contact_glitch_burst(&client, user.id, day1, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Won { hits: 1 }
    );
    assert_eq!(
        User::claim_first_contact_glitch_burst(&client, user.id, day1, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Won { hits: 2 }
    );
    assert_eq!(
        User::claim_first_contact_glitch_burst(&client, user.id, day1, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Capped { hits: 2 }
    );

    // The day rolls inside the claim; the lifetime cap then closes it.
    assert_eq!(
        User::claim_first_contact_glitch_burst(&client, user.id, day2, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Won { hits: 3 }
    );
    assert_eq!(
        User::claim_first_contact_glitch_burst(&client, user.id, day2, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Capped { hits: 3 }
    );

    // The name counter is independent, and the mirror read matches.
    assert_eq!(
        User::claim_first_contact_name_hit(&client, user.id, day2, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Won { hits: 1 }
    );
    let settings = User::find_by_fingerprint(&client, &user.fingerprint)
        .await
        .unwrap()
        .unwrap()
        .settings;
    assert_eq!(
        crate::models::user::extract_first_contact_glitch_hits(&settings),
        3
    );
    assert_eq!(
        crate::models::user::extract_first_contact_name_hits(&settings),
        1
    );

    // Reset wipes the chain and the day counters, so the claim wins again.
    User::reset_first_contact(&client, user.id).await.unwrap();
    assert_eq!(
        User::claim_first_contact_glitch_burst(&client, user.id, day2, caps)
            .await
            .unwrap(),
        FirstContactHitClaim::Won { hits: 1 }
    );
}

#[tokio::test]
async fn first_contact_whisper_claim_wins_up_to_the_cap_a_gap_apart() {
    let (client, _test_db) = setup_db().await;
    let user = create_first_contact_user(&client, json!({})).await;
    let at = chrono::Utc::now();
    let gap = chrono::Duration::hours(24);
    let cap = 2;

    // The first delivery wins; a racing second device in the same window
    // loses, and so does the next evening's connect inside the gap.
    assert!(
        User::claim_first_contact_whisper(&client, user.id, at, gap, cap)
            .await
            .unwrap()
    );
    assert!(
        !User::claim_first_contact_whisper(&client, user.id, at, gap, cap)
            .await
            .unwrap()
    );
    assert!(
        !User::claim_first_contact_whisper(
            &client,
            user.id,
            at + chrono::Duration::hours(23),
            gap,
            cap
        )
        .await
        .unwrap()
    );

    // A day later the second one lands; the cap then closes the door.
    let later = at + chrono::Duration::hours(25);
    assert!(
        User::claim_first_contact_whisper(&client, user.id, later, gap, cap)
            .await
            .unwrap()
    );
    assert!(
        !User::claim_first_contact_whisper(
            &client,
            user.id,
            later + chrono::Duration::days(30),
            gap,
            cap
        )
        .await
        .unwrap()
    );
    let settings = User::find_by_fingerprint(&client, &user.fingerprint)
        .await
        .unwrap()
        .unwrap()
        .settings;
    assert_eq!(
        crate::models::user::extract_first_contact_whisper_hits(&settings),
        2
    );
    assert_eq!(
        crate::models::user::extract_first_contact_whisper_at(&settings)
            .map(|stamp| stamp.timestamp()),
        Some(later.timestamp())
    );

    // Reset wipes the counter and the stamp, so the door opens again.
    User::reset_first_contact(&client, user.id).await.unwrap();
    assert!(
        User::claim_first_contact_whisper(&client, user.id, at, gap, cap)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn first_contact_bio_screen_is_claimed_once_per_text_and_retried_when_stale() {
    use crate::models::user::{
        FirstContactBioScreen, FirstContactBioVerdict, extract_first_contact_bio_screen,
    };
    let (client, _test_db) = setup_db().await;
    let user = create_first_contact_user(&client, json!({})).await;
    let now = chrono::Utc::now();
    let retry_after = chrono::Duration::hours(24);

    // First claim on a text wins; a racing second claim on the same text
    // loses while the first is pending and fresh.
    assert!(
        User::claim_first_contact_bio_screen(&client, user.id, "h1", now, retry_after)
            .await
            .unwrap()
    );
    assert!(
        !User::claim_first_contact_bio_screen(&client, user.id, "h1", now, retry_after)
            .await
            .unwrap()
    );

    // A verdict lands only for the text on record.
    assert!(
        !User::set_first_contact_bio_verdict(
            &client,
            user.id,
            "h0",
            FirstContactBioVerdict::Passed,
            now
        )
        .await
        .unwrap()
    );
    assert!(
        User::set_first_contact_bio_verdict(
            &client,
            user.id,
            "h1",
            FirstContactBioVerdict::Failed,
            now
        )
        .await
        .unwrap()
    );
    let settings = User::find_by_fingerprint(&client, &user.fingerprint)
        .await
        .unwrap()
        .unwrap()
        .settings;
    assert_eq!(
        extract_first_contact_bio_screen(&settings),
        Some(FirstContactBioScreen {
            hash: "h1".to_string(),
            verdict: FirstContactBioVerdict::Failed,
            at: chrono::DateTime::parse_from_rfc3339(&now.to_rfc3339())
                .unwrap()
                .with_timezone(&chrono::Utc),
        })
    );

    // A fresh failure is not retried; a day later it is. A rewritten bio
    // (new hash) is screened at once.
    assert!(
        !User::claim_first_contact_bio_screen(&client, user.id, "h1", now, retry_after)
            .await
            .unwrap()
    );
    assert!(
        User::claim_first_contact_bio_screen(
            &client,
            user.id,
            "h1",
            now + chrono::Duration::hours(25),
            retry_after
        )
        .await
        .unwrap()
    );
    assert!(
        User::claim_first_contact_bio_screen(&client, user.id, "h2", now, retry_after)
            .await
            .unwrap()
    );

    // A pass is final for that text, however old.
    assert!(
        User::set_first_contact_bio_verdict(
            &client,
            user.id,
            "h2",
            FirstContactBioVerdict::Passed,
            now
        )
        .await
        .unwrap()
    );
    assert!(
        !User::claim_first_contact_bio_screen(
            &client,
            user.id,
            "h2",
            now + chrono::Duration::days(400),
            retry_after
        )
        .await
        .unwrap()
    );
}

#[test]
fn touched_settings_count_only_deliberate_keys() {
    use crate::models::user::count_touched_settings;
    assert_eq!(count_touched_settings(&json!({})), 0);
    // Keys every account gets written by default do not count.
    assert_eq!(
        count_touched_settings(&json!({
            "audio_source": "radio",
            "clubhouse_tutorial_done": true,
            "first_contact_glitch_hits": 3
        })),
        0
    );
    assert_eq!(
        count_touched_settings(&json!({
            "theme_id": "night",
            "country": "PL",
            "timezone": null
        })),
        2
    );
}

#[tokio::test]
async fn paper_shown_claim_wins_once_per_edition_and_only_moves_forward() {
    let (client, _test_db) = setup_db().await;
    let user = create_first_contact_user(&client, json!({})).await;
    let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 3).unwrap();
    let yesterday = today.pred_opt().unwrap();
    let tomorrow = today.succ_opt().unwrap();

    // The first login of the day wins the pop; a second device the same
    // day loses, and so does an older edition arriving late.
    assert!(
        User::claim_paper_shown(&client, user.id, today)
            .await
            .unwrap()
    );
    assert!(
        !User::claim_paper_shown(&client, user.id, today)
            .await
            .unwrap()
    );
    assert!(
        !User::claim_paper_shown(&client, user.id, yesterday)
            .await
            .unwrap()
    );
    // The next edition is a new claim.
    assert!(
        User::claim_paper_shown(&client, user.id, tomorrow)
            .await
            .unwrap()
    );
}
