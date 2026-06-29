use crate::helpers::{
    assert_render_not_contains_for, make_app, new_test_db, render_plain, wait_for_render_contains,
};
use late_core::test_utils::create_test_user;
use tokio::time::Duration;

#[tokio::test]
async fn splash_screen_renders_selected_hint_with_existing_copy() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tip-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tip-session");

    app.show_splash_for_tests("Type /help in chat for a list of available chat commands");

    wait_for_render_contains(&mut app, "take a break, grab a coffee").await;
    wait_for_render_contains(
        &mut app,
        "Type /help in chat for a list of available chat commands",
    )
    .await;
}

#[tokio::test]
async fn splash_hint_disappears_after_splash_is_skipped() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tip-dismiss-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tip-dismiss-session");
    let tip = "Use m, - and = to mute, quiet, or louden the music";

    app.show_splash_for_tests(tip);
    wait_for_render_contains(&mut app, tip).await;

    app.handle_input(b"\x1b");

    assert_render_not_contains_for(&mut app, tip, Duration::from_millis(150)).await;
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains(tip),
        "tip should be gone once splash is dismissed"
    );
}
