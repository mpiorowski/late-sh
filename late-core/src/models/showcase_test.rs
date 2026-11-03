use chrono::{Duration, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::server_ban::{ServerBan, ServerBanActivation};
use super::showcase::{Showcase, ShowcaseParams};
use crate::test_utils::{create_test_user, test_db};

async fn project(client: &Client, user_id: Uuid, title: &str) -> Uuid {
    Showcase::create_by_user_id(
        client,
        user_id,
        ShowcaseParams {
            user_id,
            title: title.to_string(),
            url: "https://example.com".to_string(),
            description: "A thing.".to_string(),
            tags: Vec::new(),
        },
    )
    .await
    .expect("create project")
    .id
}

/// The herald draws only from authors seen since the bound and not banned,
/// leaves out what it was told to, names the author, and stops at the limit.
#[tokio::test]
async fn the_draw_is_active_unbanned_authors_minus_the_excluded() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let client = db.get().await.expect("db client");
    let active_since = Utc::now() - Duration::days(7);

    let active = create_test_user(db, "active-author").await;
    let first = project(&client, active.id, "first").await;
    let second = project(&client, active.id, "second").await;

    let gone = create_test_user(db, "gone-author").await;
    project(&client, gone.id, "dusty").await;
    client
        .execute(
            "UPDATE users SET last_seen = $2 WHERE id = $1",
            &[&gone.id, &(Utc::now() - Duration::days(8))],
        )
        .await
        .expect("age the author");

    let banned = create_test_user(db, "banned-author").await;
    project(&client, banned.id, "banned").await;
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: banned.id,
            fingerprint: None,
            ip_address: None,
            snapshot_username: None,
            actor_user_id: active.id,
            reason: "test",
            expires_at: None,
        },
    )
    .await
    .expect("ban");

    let mut drawn: Vec<Uuid> = Showcase::random_active(&client, active_since, &[], 10)
        .await
        .expect("draw")
        .into_iter()
        .map(|authored| {
            assert_eq!(authored.username, active.username);
            authored.showcase.id
        })
        .collect();
    drawn.sort();
    let mut both = vec![first, second];
    both.sort();
    assert_eq!(drawn, both, "only the active, unbanned author's projects");

    let left = Showcase::random_active(&client, active_since, &[first], 10)
        .await
        .expect("draw");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].showcase.id, second, "the excluded stays out");

    assert_eq!(
        Showcase::random_active(&client, active_since, &[], 1)
            .await
            .expect("draw")
            .len(),
        1,
        "the limit holds"
    );
}
