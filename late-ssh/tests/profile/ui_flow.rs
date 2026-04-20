use late_core::models::profile::{Profile, ProfileParams};

use super::helpers::{make_app, new_test_db, render_plain, wait_for_render_contains};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn profile_page_opens_and_closes_settings_modal() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "modal-open-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "modal-open-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Press Enter or e to edit profile settings").await;

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Tune your identity").await;

    app.handle_input(&[0x1B]);
    wait_for_render_contains(&mut app, "Press Enter or e to edit profile settings").await;
}

#[tokio::test]
async fn profile_page_renders_saved_country_timezone_and_bio() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "profile-summary-it").await;
    let client = test_db.db.get().await.expect("db client");

    Profile::update(
        &client,
        user.id,
        ProfileParams {
            username: "profile-summary-it".to_string(),
            bio: "hello from late\nsecond line".to_string(),
            country: Some("PL".to_string()),
            timezone: Some("Europe/Warsaw".to_string()),
            notify_kinds: vec!["dms".to_string()],
            notify_bell: true,
            notify_cooldown_mins: 5,
            theme_id: Some("late".to_string()),
            enable_background_color: false,
        },
    )
    .await
    .expect("update profile");

    let mut app = make_app(test_db.db.clone(), user.id, "profile-summary-flow-it");
    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Europe/Warsaw").await;

    let plain = render_plain(&mut app);
    assert!(
        plain.contains("Poland"),
        "profile page should show country:\n{plain}"
    );
    assert!(
        plain.contains("hello from late"),
        "profile page should show bio:\n{plain}"
    );
    assert!(
        plain.contains("Press Enter or e to edit profile settings"),
        "profile page should expose edit action:\n{plain}"
    );
}
