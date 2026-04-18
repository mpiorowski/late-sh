use late_core::models::profile::Profile;

use super::helpers::{make_app, new_test_db, render_plain, wait_for_render_contains, wait_until};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn welcome_modal_saves_profile_fields_and_profile_page_shows_them() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "modal-fields-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "modal-fields-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Press Enter or e to edit profile settings").await;

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Welcome / Profile").await;

    // Country row
    app.handle_input(b"jjjjjjjj");
    app.handle_input(b"\r");
    app.handle_input(b"pol");
    app.handle_input(b"\r");

    // Timezone row
    app.handle_input(b"j");
    app.handle_input(b"\r");
    app.handle_input(b"warsaw");
    app.handle_input(b"\r");

    // Bio row
    app.handle_input(b"j");
    app.handle_input(b"\r");
    app.handle_input(b"hello from late");
    app.handle_input(b"\x1b\r");
    app.handle_input(b"second line");
    app.handle_input(b"\r");

    // Save row
    app.handle_input(b"j");
    app.handle_input(b"\r");

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let profile = Profile::load(&client, user.id).await.expect("profile");
                profile.country.as_deref() == Some("PL")
                    && profile.timezone.as_deref() == Some("Europe/Warsaw")
                    && profile.bio == "hello from late\nsecond line"
            }
        },
        "profile country/timezone/bio to persist",
    )
    .await;

    let plain = render_plain(&mut app);
    assert!(plain.contains("Poland"), "profile page should show country:\n{plain}");
    assert!(
        plain.contains("Europe/Warsaw"),
        "profile page should show timezone:\n{plain}"
    );
    assert!(
        plain.contains("hello from late"),
        "profile page should show bio:\n{plain}"
    );
}

#[tokio::test]
async fn welcome_modal_normalizes_username_and_saves_notifications() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "modal-user-orig").await;
    let mut app = make_app(test_db.db.clone(), user.id, "modal-user-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "Press Enter or e to edit profile settings").await;

    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Welcome / Profile").await;

    // Username edit
    app.handle_input(b"\r");
    app.handle_input(b"\x15");
    app.handle_input(b"late night!!!");
    app.handle_input(b"\r");

    // Toggle DMs and Bell.
    app.handle_input(b"jjj");
    app.handle_input(b" ");
    app.handle_input(b"jjj");
    app.handle_input(b" ");

    // Save
    app.handle_input(b"jjjjj");
    app.handle_input(b"\r");

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let profile = Profile::load(&client, user.id).await.expect("profile");
                profile.username == "late_night"
                    && profile.notify_kinds == vec!["dms".to_string()]
                    && profile.notify_bell
            }
        },
        "normalized username and notifications to persist",
    )
    .await;

    let plain = render_plain(&mut app);
    assert!(
        plain.contains("late_night"),
        "profile page should show normalized username:\n{plain}"
    );
}
