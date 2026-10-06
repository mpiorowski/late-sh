use uuid::Uuid;

use crate::models::work_profile::{WorkProfile, WorkProfileParams, WorkStatus, WorkType};
use crate::test_utils::{create_test_user, test_db};

fn params(user_id: Uuid, slug: &str, status: WorkStatus, work_type: WorkType) -> WorkProfileParams {
    WorkProfileParams {
        user_id,
        slug: slug.to_string(),
        headline: "Rust backend engineer".to_string(),
        status,
        work_type,
        location: "EU remote".to_string(),
        contact: "work@example.com".to_string(),
        links: vec!["https://github.com/late-sh".to_string()],
        skills: vec!["Rust".to_string(), "PostgreSQL".to_string()],
        skills_tags: vec!["rust".to_string(), "postgres".to_string()],
        summary: "Building terminal software.".to_string(),
    }
}

#[tokio::test]
async fn typed_fields_round_trip_through_the_text_columns() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "work-typed").await;

    let created = WorkProfile::create_by_user_id(
        &client,
        user.id,
        params(
            user.id,
            "w_typedcard001",
            WorkStatus::Casual,
            WorkType::Freelance,
        ),
    )
    .await
    .expect("create");
    assert_eq!(created.status, WorkStatus::Casual);
    assert_eq!(created.work_type, WorkType::Freelance);
    assert_eq!(created.skills, vec!["Rust", "PostgreSQL"]);
    assert_eq!(created.skills_tags, vec!["rust", "postgres"]);

    // The column holds the enum's `as_str`, so the web and the matcher can
    // compare it as text without going through the model.
    let stored: (String, String) = client
        .query_one(
            "SELECT status, work_type FROM work_profiles WHERE id = $1",
            &[&created.id],
        )
        .await
        .map(|row| (row.get("status"), row.get("work_type")))
        .expect("stored row");
    assert_eq!(stored, ("casual".to_string(), "freelance".to_string()));

    let fetched = WorkProfile::find_by_slug(&client, "w_typedcard001")
        .await
        .expect("find")
        .expect("exists");
    assert_eq!(fetched.status, WorkStatus::Casual);
    assert_eq!(fetched.work_type, WorkType::Freelance);
}

#[tokio::test]
async fn index_order_is_open_then_casual_then_not_looking() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let closed = create_test_user(&test_db.db, "work-idx-closed").await;
    let open = create_test_user(&test_db.db, "work-idx-open").await;
    let casual = create_test_user(&test_db.db, "work-idx-casual").await;

    for (user, slug, status) in [
        (&closed, "w_indexorder01", WorkStatus::NotLooking),
        (&open, "w_indexorder02", WorkStatus::Open),
        (&casual, "w_indexorder03", WorkStatus::Casual),
    ] {
        WorkProfile::create_by_user_id(
            &client,
            user.id,
            params(user.id, slug, status, WorkType::Any),
        )
        .await
        .expect("create");
    }

    let index = WorkProfile::list_index(&client, 10).await.expect("index");
    let statuses: Vec<WorkStatus> = index.iter().map(|card| card.status).collect();
    assert_eq!(
        statuses,
        vec![WorkStatus::Open, WorkStatus::Casual, WorkStatus::NotLooking]
    );

    // The feed keeps its own order: whoever moved last, first.
    let recent = WorkProfile::list_recent(&client, 10).await.expect("recent");
    assert_eq!(recent[0].user_id, casual.id);
}

#[test]
fn enums_cycle_and_print() {
    assert_eq!(WorkStatus::NotLooking.next(), WorkStatus::Open);
    assert_eq!(WorkStatus::Open.prev(), WorkStatus::NotLooking);
    assert_eq!(WorkType::Any.next(), WorkType::FullTime);
    assert_eq!(WorkType::FullTime.prev(), WorkType::Any);
    for status in WorkStatus::ALL {
        assert_eq!(WorkStatus::from_db(status.as_str()), status);
    }
    for work_type in WorkType::ALL {
        assert_eq!(WorkType::from_db(work_type.as_str()), work_type);
    }
    // A `late-web` pod can decode rows before `late-ssh` has run migration
    // 192, when the column still holds free text: that reads as `Any`.
    assert_eq!(WorkType::from_db("contract, full-time"), WorkType::Any);
    assert_eq!(WorkType::Any.label(), "open to any");
    assert_eq!(WorkStatus::NotLooking.label(), "not looking");
}
