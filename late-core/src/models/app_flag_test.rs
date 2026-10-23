use super::AppFlag;
use crate::test_utils::test_db;

/// The herald's switch is seeded off and reads back whatever was last
/// written to its row.
#[tokio::test]
async fn the_herald_switch_is_seeded_off_and_flips() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    assert!(
        !AppFlag::HeraldEnabled
            .is_enabled(&client)
            .await
            .expect("read seeded flag")
    );

    AppFlag::HeraldEnabled
        .set(&client, true)
        .await
        .expect("flip on");
    assert!(
        AppFlag::HeraldEnabled
            .is_enabled(&client)
            .await
            .expect("read flipped flag")
    );
}
