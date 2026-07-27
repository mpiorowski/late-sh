use crate::test_helpers::{make_app, new_test_db, render_plain, wait_for_render_contains};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn splash_screen_renders_selected_hint_with_existing_copy_and_dismisses() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "splash-tip-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "splash-tip-session");
    let existing_copy = "take a break, grab a coffee";

    app.show_splash_for_tests("Type /help in chat for a list of available chat commands");
    // This test covers composition, not the typewriter's wall-clock duration.
    // Put the next tick on the completed-copy frame instead of polling through
    // every intermediate character.
    app.splash_ticks = existing_copy.len().saturating_sub(1);

    wait_for_render_contains(&mut app, existing_copy).await;
    wait_for_render_contains(
        &mut app,
        "Type /help in chat for a list of available chat commands",
    )
    .await;

    app.handle_input(b"\x1b");
    assert!(
        !app.show_splash,
        "Esc should dismiss the splash immediately"
    );
    let plain = render_plain(&mut app);
    assert!(
        !plain.contains("Type /help in chat for a list of available chat commands"),
        "tip should be gone once splash is dismissed"
    );
}
