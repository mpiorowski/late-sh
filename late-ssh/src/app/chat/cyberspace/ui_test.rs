use crate::app::chat::cyberspace::api::{CircMessage, CsPost, CsReply};
use crate::app::chat::cyberspace::svc::CsThread;

use super::{room_lines, thread_lines};

/// The shape a markdown entry actually has: paragraphs, not pre-broken lines.
fn long_entry() -> CsThread {
    let paragraph = "word ".repeat(120);
    let post: CsPost = serde_json::from_str(&format!(
        r#"{{"postId":"p1","authorUsername":"mat","content":"{paragraph}"}}"#
    ))
    .expect("post");
    let reply: CsReply = serde_json::from_str(&format!(
        r#"{{"replyId":"r1","authorUsername":"laschii","content":"{paragraph}"}}"#
    ))
    .expect("reply");
    CsThread {
        post,
        replies: vec![reply],
    }
}

#[test]
fn a_long_entry_is_taller_than_the_pane_so_its_replies_can_be_scrolled_to() {
    let thread = long_entry();
    let width = 60;
    let viewport = 20;

    let lines = thread_lines(&thread, width, false);

    // Counting the entry's `\n`s says two lines and pins the scroll ceiling at
    // zero, which is what left the replies below an unscrollable screen.
    assert_eq!(
        thread.post.content.lines().count(),
        1,
        "the fixture must be the one-long-paragraph shape"
    );
    assert!(
        lines.len() > viewport,
        "a 600-word entry at width {width} must be taller than {viewport} rows, got {}",
        lines.len()
    );
    // Every row fits the pane, since the paragraph is laid out here rather
    // than by the widget: an over-wide row would be truncated, not wrapped.
    for line in &lines {
        assert!(
            line.width() <= width,
            "row wider than the pane: {:?}",
            line.width()
        );
    }
}

#[test]
fn wide_characters_wrap_by_display_width_not_char_count() {
    // Width-2 glyphs: a row budgeted by chars().count() would render at twice
    // the pane width and get truncated, dropping text off the right edge.
    let paragraph = "汉字宽度 ".repeat(60);
    let post: CsPost = serde_json::from_str(&format!(
        r#"{{"postId":"p1","authorUsername":"mat","content":"{paragraph}"}}"#
    ))
    .expect("post");
    let thread = CsThread {
        post,
        replies: Vec::new(),
    };
    let width = 60;

    let lines = thread_lines(&thread, width, false);

    for line in &lines {
        assert!(
            line.width() <= width,
            "row wider than the pane: {:?}",
            line.width()
        );
    }
}

#[test]
fn a_long_chat_message_wraps_inside_the_pane_instead_of_running_off_it() {
    let long = "a sentence that keeps going ".repeat(12);
    let message: CircMessage = serde_json::from_str(&format!(
        r#"{{"id":"m1","username":"tux_racer","content":"{long}","timestamp":0}}"#
    ))
    .expect("message");
    let width = 60;

    let rows = room_lines(std::slice::from_ref(&message), width);

    // One message, several rows: unwrapped it was one row that ran off the
    // right edge, and the scroll window counts rows, so it also lied about
    // how far back the conversation went.
    assert!(
        rows.len() > 1,
        "a {}-char message at width {width} must wrap",
        long.len()
    );
    for row in &rows {
        assert!(
            row.width() <= width,
            "row wider than the pane: {}",
            row.width()
        );
    }
    // The author is named once; continuations hang under it so the message
    // still reads as one message.
    let named: Vec<&ratatui::text::Line<'_>> = rows
        .iter()
        .filter(|row| {
            row.spans
                .iter()
                .any(|span| span.content.contains("tux_racer"))
        })
        .collect();
    assert_eq!(named.len(), 1);
}

#[test]
fn wide_glyphs_in_a_chat_message_wrap_by_display_width() {
    let message: CircMessage = serde_json::from_str(&format!(
        r#"{{"id":"m1","username":"mat","content":"{}","timestamp":0}}"#,
        "汉字宽度 ".repeat(30)
    ))
    .expect("message");
    let width = 50;

    for row in room_lines(std::slice::from_ref(&message), width) {
        assert!(
            row.width() <= width,
            "row wider than the pane: {}",
            row.width()
        );
    }
}

#[test]
fn a_narrower_pane_makes_the_same_entry_taller() {
    let thread = long_entry();
    let wide = thread_lines(&thread, 100, false).len();
    let narrow = thread_lines(&thread, 40, false).len();
    assert!(
        narrow > wide,
        "the scroll ceiling has to follow the pane width: {narrow} vs {wide}"
    );
}
