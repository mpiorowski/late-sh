mod helpers;

use helpers::make_app;
use late_core::db::{Db, DbConfig};
use uuid::Uuid;

#[tokio::test]
async fn renders_non_empty_frames_when_input_and_ticks_are_processed() {
    let db = Db::new(&DbConfig::default()).expect("db");
    let user_id = Uuid::now_v7();
    let mut app = make_app(db, user_id, "smoke-token");

    app.handle_input(b"2");
    app.handle_input(b"ihello\r");
    app.handle_input(b"3");
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
