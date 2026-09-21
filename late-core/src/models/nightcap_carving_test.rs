use super::*;
use crate::test_utils::{create_test_user, test_db};

#[tokio::test]
async fn a_carving_names_its_carver_and_the_next_sitter_carves_over_it() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let first = create_test_user(&test_db.db, "carver_one").await;
    let second = create_test_user(&test_db.db, "carver_two").await;

    let carved = Carving::carve(&client, 2, first.id, "  was here, 3am  ")
        .await
        .expect("carve");
    assert_eq!(carved.stool, 2);
    assert_eq!(carved.user_id, Some(first.id));
    assert_eq!(carved.username.as_deref(), Some(first.username.as_str()));
    assert_eq!(carved.body, "was here, 3am");

    let over = Carving::carve(&client, 2, second.id, "and gone")
        .await
        .expect("carve over");
    assert_eq!(over.user_id, Some(second.id));

    let listed = Carving::list(&client).await.expect("list");
    assert_eq!(listed.len(), 1, "one line per stool, the newest wins");
    assert_eq!(listed[0].body, "and gone");
    assert_eq!(
        listed[0].username.as_deref(),
        Some(second.username.as_str())
    );
}

#[tokio::test]
async fn a_carving_is_one_trimmed_line_and_never_empty() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let carver = create_test_user(&test_db.db, "carver_lines").await;

    assert!(Carving::carve(&client, 0, carver.id, "   ").await.is_err());
    assert!(
        Carving::carve(&client, 6, carver.id, "off the end")
            .await
            .is_err()
    );

    let long = "x".repeat(CARVING_MAX_CHARS + 20);
    let carved = Carving::carve(&client, 0, carver.id, &format!("first line\nsecond {long}"))
        .await
        .expect("carve");
    assert_eq!(carved.body, "first line");

    let capped = Carving::carve(&client, 1, carver.id, &long)
        .await
        .expect("carve long");
    assert_eq!(capped.body.chars().count(), CARVING_MAX_CHARS);
}
