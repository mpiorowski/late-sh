
use crate::test_helpers::{make_app, new_test_db};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn renders_non_empty_frames_when_input_and_ticks_are_processed() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "app-smoke").await;
    let mut app = make_app(test_db.db.clone(), user.id, "smoke-token");

    app.handle_input(b"4");
    app.handle_input(b"q");
    app.handle_input(b"3");
    app.handle_input(b"ihello\r");
    app.handle_input(b"n");
    app.tick();

    let bytes = app.render().expect("render");
    assert!(!bytes.is_empty(), "render output should not be empty");

    app.tick();
    let bytes2 = app.render().expect("second render");
    assert!(
        !bytes2.is_empty(),
        "second render output should not be empty"
    );
}
