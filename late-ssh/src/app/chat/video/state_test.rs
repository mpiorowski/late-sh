use std::time::Instant;

use ratatui::layout::Rect;
use uuid::Uuid;

use super::*;
use crate::app::chat::ui::{ChatRowHit, ChatRowKind};
use crate::paired_clients::{PairControlMessage, VideoReport};

const URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

fn row(message_id: Option<Uuid>, kind: ChatRowKind) -> ChatRowHit {
    ChatRowHit { message_id, kind }
}

/// A message's header and body, a 4-row picture with its hint row, then
/// the next message's header.
fn rows_with_thumbnail(message_id: Uuid) -> Vec<ChatRowHit> {
    let mut rows = vec![
        row(Some(message_id), ChatRowKind::Header(Vec::new())),
        row(Some(message_id), ChatRowKind::Body),
    ];
    rows.extend((0..5).map(|_| row(Some(message_id), ChatRowKind::Image)));
    rows.push(row(None, ChatRowKind::None));
    rows.push(row(
        Some(Uuid::from_u128(99)),
        ChatRowKind::Header(Vec::new()),
    ));
    rows
}

#[test]
fn youtube_links_resolve_to_one_canonical_watch_link() {
    let canonical = Some(YoutubeLink {
        url: URL.to_string(),
        video_id: "dQw4w9WgXcQ".to_string(),
    });
    for body in [
        "look https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s",
        "(https://youtu.be/dQw4w9WgXcQ?si=share).",
        "https://youtube.com/shorts/dQw4w9WgXcQ!",
        "https://m.youtube.com/embed/dQw4w9WgXcQ",
    ] {
        assert_eq!(youtube_link_in_body(body), canonical, "{body}");
    }
    for body in [
        "https://www.youtube.com/@somechannel",
        "https://www.youtube.com/playlist?list=PL123",
        "https://youtu.be/tooshort",
        "https://notyoutube.com/watch?v=dQw4w9WgXcQ",
        "no link here",
    ] {
        assert_eq!(youtube_link_in_body(body), None, "{body}");
    }
}

#[test]
fn the_video_sits_over_the_picture_only_while_the_whole_thumbnail_shows() {
    let id = Uuid::from_u128(1);
    let area = Rect::new(30, 5, 80, 20);

    assert_eq!(
        video_rect(area, &rows_with_thumbnail(id), id, (43, 4)),
        Some(Rect::new(32, 7, 43, 4)),
        "two rows down, past the gutter and the space"
    );
    // Scrolled so the top picture row is off the pane.
    let clipped = rows_with_thumbnail(id).split_off(3);
    assert_eq!(video_rect(area, &clipped, id, (43, 4)), None);
    // A pane too narrow for the picture.
    let narrow = Rect::new(30, 5, 44, 20);
    assert_eq!(
        video_rect(narrow, &rows_with_thumbnail(id), id, (43, 4)),
        None
    );
    // Another message's thumbnail is not this video's.
    assert_eq!(
        video_rect(area, &rows_with_thumbnail(id), Uuid::from_u128(2), (43, 4)),
        None
    );
}

/// Drawn clear, it plays; covered, it parks and the CLI is asked for
/// nothing; clear again, it comes back under a fresh id three seconds
/// before where it was; off the screen, it closes.
#[test]
fn a_covered_video_parks_at_its_position_and_comes_back_from_it() {
    let now = Instant::now();
    let rect = Rect::new(2, 3, 43, 12);
    let video = InlineVideo::new(Uuid::from_u128(1), URL.to_string(), 7, now);

    video.set_drawn(Some(rect));
    video.set_clear(true);
    assert_eq!(video.step(), Step::Keep);
    assert_eq!(
        video.wanted(),
        Some(WantedPlay {
            play_id: 7,
            url: URL,
            rect,
            start_s: 0,
        })
    );

    let mut video = video;
    video.apply(
        &VideoReport::Playing {
            position_s: 83,
            duration_s: Some(724),
        },
        now,
    );
    video.set_clear(false);
    assert_eq!(video.sight(), Sight::Covered(rect));
    assert_eq!(video.step(), Step::Park);

    let video = video.parked();
    assert_eq!(video.play_id(), None);
    assert_eq!(video.wanted(), None);
    assert!(video.holds_rect(), "the rectangle stays the video's");
    assert_eq!(video.step(), Step::Keep, "still covered");

    video.set_clear(true);
    assert_eq!(video.step(), Step::Unpark);
    let video = video.unparked(8, now);
    assert_eq!(
        video.wanted(),
        Some(WantedPlay {
            play_id: 8,
            url: URL,
            rect,
            start_s: 80,
        })
    );
    assert_eq!(
        video.caption(now),
        "resuming from 1:23… · Enter pause · Esc stop"
    );

    video.begin_frame();
    assert_eq!(video.sight(), Sight::Gone);
    assert_eq!(video.step(), Step::Close);
}

/// A video watched to the end gives the picture back to the thumbnail,
/// is never parked, and plays again from the top on Enter.
#[test]
fn an_ended_video_stays_put_and_replays_from_the_top() {
    let now = Instant::now();
    let mut video = InlineVideo::new(Uuid::from_u128(1), URL.to_string(), 3, now);
    video.apply(
        &VideoReport::Playing {
            position_s: 700,
            duration_s: Some(724),
        },
        now,
    );
    assert_eq!(
        video.apply(&VideoReport::Ended, now),
        Some(WatchOutcome::Ended)
    );
    assert!(!video.holds_rect());
    assert_eq!(video.control(VideoControl::TogglePause), None);

    video.set_drawn(Some(Rect::new(2, 3, 43, 12)));
    assert_eq!(video.step(), Step::Keep, "covered, but nothing to park");
    assert_eq!(video.caption(now), "ended · Enter to watch again");

    let video = video.replayed(4, now);
    match video.control(VideoControl::TogglePause) {
        Some(PairControlMessage::VideoControl { play_id: 4, .. }) => {}
        other => panic!("expected the replay's control, got {other:?}"),
    }
    video.set_clear(true);
    assert_eq!(video.wanted().map(|wanted| wanted.start_s), Some(0));
}
