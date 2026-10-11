use crate::{app::audio::youtube::YoutubeVideo, test_helpers::make_app};
use late_core::test_utils::{create_test_user, test_db};

fn video(video_id: &str, title: &str) -> YoutubeVideo {
    YoutubeVideo {
        video_id: video_id.to_string(),
        title: Some(title.to_string()),
        channel: Some("Music channel".to_string()),
        duration_ms: Some(60_000),
        is_stream: false,
    }
}

#[tokio::test]
async fn ctrl_y_copies_the_link_of_the_track_the_focus_points_at() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "booth-copy").await;
    let mut app = make_app(test.db.clone(), user.id, "booth-copy");
    let service = app.audio.service();
    service
        .submit_validated_video(user.id, video("aaaaaaaaaaa", "Playing"))
        .await
        .expect("start YouTube track");
    service
        .submit_validated_video(user.id, video("bbbbbbbbbbb", "Up next"))
        .await
        .expect("queue YouTube track");
    let playing = "https://www.youtube.com/watch?v=aaaaaaaaaaa";
    let up_next = "https://www.youtube.com/watch?v=bbbbbbbbbbb";
    app.booth_modal_state.open(true);

    // Submit focus copies the playing track and leaves the URL field alone.
    app.handle_input(b"i\x19");
    assert_eq!(app.booth_modal_state.submit_input(), "i");
    assert_eq!(app.pending_clipboard.as_deref(), Some(playing));

    // The lists copy their selected row; History keeps the playing track on
    // top, and the `/` filter does not swallow the copy.
    for (focus, keys, expected) in [
        ("queue", &b"\t"[..], up_next),
        ("history", &b"\t"[..], playing),
        ("history filter", &b"/"[..], playing),
    ] {
        app.handle_input(keys);
        app.pending_clipboard = None;
        app.handle_input(b"\x19");
        assert_eq!(app.pending_clipboard.as_deref(), Some(expected), "{focus}");
        assert!(app.booth_modal_state.is_open(), "{focus}");
    }
    assert!(app.booth_modal_state.history_filter_active());
}

#[tokio::test]
async fn ctrl_y_on_an_empty_queue_preserves_the_clipboard() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "booth-copy-empty").await;
    let mut app = make_app(test.db.clone(), user.id, "booth-copy-empty");
    app.pending_clipboard = Some("Earlier copy".to_string());
    app.booth_modal_state.open(false);

    app.handle_input(b"\x19");

    assert_eq!(app.pending_clipboard.as_deref(), Some("Earlier copy"));
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("No track to copy")
    );
}
