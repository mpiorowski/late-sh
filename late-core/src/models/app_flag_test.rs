use crate::models::app_flag::{AppFlag, AppFlags};
use crate::test_utils::test_db;

#[tokio::test]
async fn the_seed_loads_and_a_set_flips_one_switch() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    // Migration 171's seed (kill switch on, fuse unlit) plus 173's (paper
    // and its outside page both on) and 174's (the gallery on).
    let flags = AppFlags::load(&client).await.expect("load");
    assert_eq!(
        flags,
        AppFlags {
            haunt_enabled: true,
            haunt_live: false,
            paper_enabled: true,
            paper_outside_enabled: true,
            artboard_gallery_enabled: true,
        }
    );

    AppFlags::set(&client, AppFlag::HauntLive, true)
        .await
        .expect("set");
    AppFlags::set(&client, AppFlag::HauntEnabled, false)
        .await
        .expect("set");
    assert_eq!(
        AppFlags::load(&client).await.expect("load"),
        AppFlags {
            haunt_enabled: false,
            haunt_live: true,
            paper_enabled: true,
            paper_outside_enabled: true,
            artboard_gallery_enabled: true,
        }
    );
}

#[tokio::test]
async fn a_missing_row_is_an_error_not_a_default() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    client
        .execute(
            "DELETE FROM app_flags WHERE key = $1",
            &[&AppFlag::HauntLive.key()],
        )
        .await
        .expect("delete");

    let error = AppFlags::load(&client).await.expect_err("load must fail");
    assert!(
        error.to_string().contains("haunt_live has no row"),
        "{error}"
    );
    let error = AppFlags::set(&client, AppFlag::HauntLive, true)
        .await
        .expect_err("set must fail");
    assert!(
        error.to_string().contains("haunt_live has no row"),
        "{error}"
    );
}
