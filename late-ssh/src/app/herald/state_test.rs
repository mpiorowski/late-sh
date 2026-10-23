use chrono::{NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::herald::{HeraldKind, PosterCandidate};
use uuid::Uuid;

use super::state::{
    BOARD_MIN_INK, BoardCandidate, Picture, TEXT_LIMIT, alt_text, board_key, ink_cells,
    pick_picture, post_due_day, shown_day, status_text,
};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

fn poster(username: &str) -> PosterCandidate {
    PosterCandidate {
        user_id: Uuid::now_v7(),
        username: username.to_string(),
        slug: "w_heraldstate1".to_string(),
    }
}

fn board() -> BoardCandidate {
    BoardCandidate {
        key: board_key(day(22)),
        shown: day(22),
    }
}

#[test]
fn the_post_is_due_at_sixteen_utc_and_catches_up_after_it() {
    let before = Utc.with_ymd_and_hms(2026, 10, 23, 15, 59, 59).unwrap();
    assert_eq!(post_due_day(before), day(22), "still yesterday's post");
    let due = Utc.with_ymd_and_hms(2026, 10, 23, 16, 0, 0).unwrap();
    assert_eq!(post_due_day(due), day(23));
    // A replica back at 02:00 owes the 23rd its post, not the 24th.
    let late = Utc.with_ymd_and_hms(2026, 10, 24, 2, 0, 0).unwrap();
    assert_eq!(post_due_day(late), day(23));
    // The post dated the 23rd shows the 22nd's board.
    assert_eq!(shown_day(day(23)), day(22));
    assert_eq!(board_key(day(22)), "daily:2026-10-22");
}

/// A poster and the board alternate; whichever is missing, the other
/// goes; nothing goes when both are.
#[test]
fn the_rotation_alternates_and_falls_back() {
    let painter = poster("painter");
    let picks = |last| pick_picture(last, Some(painter.clone()), Some(board()));
    assert_eq!(picks(None), Some(Picture::Poster(painter.clone())));
    assert_eq!(
        picks(Some(HeraldKind::Artboard)),
        Some(Picture::Poster(painter.clone()))
    );
    assert_eq!(
        picks(Some(HeraldKind::Poster)),
        Some(Picture::Board(board()))
    );

    // A poster after a poster when the board is too bare.
    assert_eq!(
        pick_picture(Some(HeraldKind::Poster), Some(painter.clone()), None),
        Some(Picture::Poster(painter.clone()))
    );
    // The board after the board when no poster waits.
    assert_eq!(
        pick_picture(Some(HeraldKind::Artboard), None, Some(board())),
        Some(Picture::Board(board()))
    );
    assert_eq!(pick_picture(Some(HeraldKind::Artboard), None, None), None);
    assert_eq!(pick_picture(None, None, None), None);
}

/// Each picture knows its route, its page, its subject, and its words:
/// under the tightest network's limit, no bare `@`, the page linked, four
/// tags; and it round-trips through the row.
#[test]
fn the_words_fit_and_never_mention_anyone() {
    let painter = poster("a_long_username_of_thirty_chars");
    let picture = Picture::Poster(painter.clone());
    let stored = serde_json::to_value(&picture).expect("serialize");
    assert_eq!(
        serde_json::from_value::<Picture>(stored).expect("deserialize"),
        picture
    );
    assert_eq!(picture.kind(), HeraldKind::Poster);
    assert_eq!(picture.subject(), painter.user_id.to_string());
    assert_eq!(
        picture.picture_path(),
        "/profiles/w_heraldstate1/poster.png"
    );
    assert_eq!(picture.page_path(), "/profiles/w_heraldstate1");
    let text = status_text(&picture, "https://late.sh");
    assert!(text.chars().count() < TEXT_LIMIT, "{text}");
    assert!(text.contains("~a_long_username_of_thirty_chars"));
    assert!(
        !text.contains('@'),
        "a bare @ would ping a stranger: {text}"
    );
    assert!(text.contains("https://late.sh/profiles/w_heraldstate1"));
    assert_eq!(text.matches('#').count(), 4);
    let alt = alt_text(&picture);
    assert!(alt.contains("~a_long_username_of_thirty_chars"));
    assert!(!alt.contains('@'));

    let picture = Picture::Board(board());
    assert_eq!(picture.kind(), HeraldKind::Artboard);
    assert_eq!(picture.subject(), "daily:2026-10-22");
    assert_eq!(
        picture.picture_path(),
        "/gallery/daily:2026-10-22/image.png"
    );
    assert_eq!(picture.page_path(), "/gallery?key=daily:2026-10-22");
    let text = status_text(&picture, "https://late.sh");
    assert!(text.chars().count() < TEXT_LIMIT, "{text}");
    assert!(text.contains("22 October 2026"));
    assert!(text.contains("https://late.sh/gallery?key=daily:2026-10-22"));
    assert_eq!(text.matches('#').count(), 4);
    assert!(alt_text(&picture).contains("22 October 2026"));
}

#[test]
fn ink_counts_glyphs_and_ignores_whitespace() {
    let mut canvas = Canvas::with_size(80, 24);
    assert_eq!(ink_cells(&canvas), 0);
    for index in 0..BOARD_MIN_INK {
        canvas.set(
            Pos {
                x: index % 80,
                y: index / 80,
            },
            '#',
        );
    }
    canvas.set(Pos { x: 79, y: 23 }, ' ');
    assert_eq!(ink_cells(&canvas), BOARD_MIN_INK);
}
