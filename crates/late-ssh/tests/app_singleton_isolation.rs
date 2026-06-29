//! App integration test for basic app isolation.

mod helpers;

use helpers::{make_app, new_test_db};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn separate_apps_render_independently() {
    let test_db = new_test_db().await;
    let user_a = create_test_user(&test_db.db, "singleton-a").await;
    let user_b = create_test_user(&test_db.db, "singleton-b").await;

    let mut app_a = make_app(test_db.db.clone(), user_a.id, "singleton-iso-a");
    let mut app_b = make_app(test_db.db.clone(), user_b.id, "singleton-iso-b");

    app_a.tick();
    app_b.tick();
    let _ = app_a.render().expect("app_a renders");
    let _ = app_b.render().expect("app_b renders");
}
