use crate::models::door_rc::*;
use crate::test_utils::{create_test_user, test_db};

#[test]
fn content_cap_and_nul_are_rejected() {
    assert!(content_acceptable("OPTIONS=color,autopickup\n"));
    assert!(content_acceptable(""));
    assert!(!content_acceptable(&"x".repeat(MAX_RC_BYTES + 1)));
    assert!(!content_acceptable("OPTIONS=co\0lor"));
}

#[test]
fn game_keys_round_trip() {
    for game in [DoorRcGame::Nethack, DoorRcGame::Dcss] {
        assert_eq!(DoorRcGame::from_key(game.as_key()), Some(game));
    }
    assert_eq!(DoorRcGame::from_key("brogue"), None);
}

#[tokio::test]
async fn upsert_get_replace_and_clear_round_trip() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "door-rc-crud").await;
    let client = test_db.db.get().await.expect("db client");

    assert_eq!(
        DoorRc::get(&client, user.id, DoorRcGame::Nethack)
            .await
            .expect("get empty"),
        None
    );

    DoorRc::upsert(&client, user.id, DoorRcGame::Nethack, "OPTIONS=color\n")
        .await
        .expect("insert");
    // A replace overwrites in place (one row per user+game).
    DoorRc::upsert(
        &client,
        user.id,
        DoorRcGame::Nethack,
        "OPTIONS=autopickup\n",
    )
    .await
    .expect("replace");
    assert_eq!(
        DoorRc::get(&client, user.id, DoorRcGame::Nethack)
            .await
            .expect("get"),
        Some("OPTIONS=autopickup\n".to_string())
    );

    DoorRc::upsert(&client, user.id, DoorRcGame::Dcss, "show_more = false\n")
        .await
        .expect("insert dcss");
    let mut all = DoorRc::list_for_user(&client, user.id).await.expect("list");
    all.sort_by_key(|(game, _)| game.as_key());
    assert_eq!(
        all,
        vec![
            (DoorRcGame::Dcss, "show_more = false\n".to_string()),
            (DoorRcGame::Nethack, "OPTIONS=autopickup\n".to_string()),
        ]
    );

    DoorRc::clear(&client, user.id, DoorRcGame::Nethack)
        .await
        .expect("clear");
    assert_eq!(
        DoorRc::get(&client, user.id, DoorRcGame::Nethack)
            .await
            .expect("get cleared"),
        None
    );
    // Clearing one game leaves the other untouched.
    assert_eq!(
        DoorRc::get(&client, user.id, DoorRcGame::Dcss)
            .await
            .expect("get dcss"),
        Some("show_more = false\n".to_string())
    );
}
