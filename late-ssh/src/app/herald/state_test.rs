use chrono::{NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::artboard::Snapshot;
use late_core::models::herald::HeraldKind;

use super::state::{
    BOARD_MIN_INK, BlueskyMark, BoardCandidate, BoardFound, HeraldCommand, Look, Picture, Warning,
    Words, alt_text, bluesky_mark, ink_cells, mastodon_warning, parse_herald_command, post_due_day,
    shown_day, status_text,
};

/// The shortest limit among the networks: Bluesky's 300 graphemes
/// (Mastodon allows 500). The words differ between networks only by the
/// mark, so every variant fits the tightest.
const TEXT_LIMIT: usize = 300;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

fn board() -> BoardCandidate {
    BoardCandidate {
        key: Snapshot::daily_key(day(22)),
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
}

/// The picture knows its route, its page, its subject, and its words:
/// under the tightest network's limit, no bare `@`, the page linked, four
/// tags; and it round-trips through the row.
#[test]
fn the_words_fit_and_never_mention_anyone() {
    let picture = Picture::Board(board());
    let stored = serde_json::to_value(&picture).expect("serialize");
    assert_eq!(
        serde_json::from_value::<Picture>(stored).expect("deserialize"),
        picture
    );
    assert_eq!(picture.kind(), HeraldKind::Artboard);
    assert_eq!(picture.subject(), "daily:2026-10-22");
    assert_eq!(
        picture.picture_path(),
        "/gallery/daily:2026-10-22/image.png"
    );
    assert_eq!(picture.page_path(), "/gallery");
    let text = status_text(&picture, "https://late.sh", Words::Plain);
    assert!(text.chars().count() < TEXT_LIMIT, "{text}");
    assert!(text.contains("22 October 2026"));
    assert!(
        !text.contains('@'),
        "a bare @ would ping a stranger: {text}"
    );
    assert!(text.contains("https://late.sh/gallery\n"), "{text}");
    assert_eq!(text.matches('#').count(), 4);
    let alt = alt_text(&picture, &Look::Unseen);
    assert!(alt.starts_with("ASCII art: "), "{alt}");
    assert!(alt.contains("22 October 2026"));
    assert!(!alt.contains('@'));
}

fn seen(warning: Option<Warning>) -> Look {
    Look::Seen {
        alt: "A burning refinery across the top.".to_string(),
        warning,
        reason: String::new(),
    }
}

/// Every look, and the mark each network gives it: Mastodon's content
/// warning (any blurs), Bluesky's self-label and words. The whole table,
/// so a moved mark shows here.
#[test]
fn each_look_maps_to_each_networks_mark() {
    let marked = |first_line| Words::Marked { first_line };
    let looks = [
        seen(None),
        seen(Some(Warning::Nudity)),
        seen(Some(Warning::Suggestive)),
        seen(Some(Warning::Explicit)),
        seen(Some(Warning::Gore)),
        seen(Some(Warning::Offensive)),
        Look::Unseen,
    ];
    let marks: Vec<(&str, Option<&str>, BlueskyMark)> = looks
        .iter()
        .map(|look| (look.as_str(), mastodon_warning(look), bluesky_mark(look)))
        .collect();
    let bluesky = |label, words| BlueskyMark { label, words };
    assert_eq!(
        marks,
        vec![
            ("clean", None, bluesky(None, Words::Plain)),
            (
                "nudity",
                Some("nsfw: nudity"),
                bluesky(Some("nudity"), marked(None))
            ),
            (
                "suggestive",
                Some("nsfw: suggestive"),
                bluesky(Some("sexual"), marked(None))
            ),
            (
                "explicit",
                Some("nsfw: explicit"),
                bluesky(Some("porn"), marked(None))
            ),
            (
                "gore",
                Some("cw: gore"),
                bluesky(Some("graphic-media"), marked(None))
            ),
            (
                "offensive",
                Some("cw: offensive"),
                bluesky(None, marked(Some("cw: offensive")))
            ),
            (
                "unseen",
                Some("cw: not reviewed"),
                bluesky(None, marked(Some("cw: not reviewed")))
            ),
        ]
    );
}

/// A marked post swaps its last tag for `#nsfw`, keeps four, puts the
/// first line on top, and still fits Bluesky's limit.
#[test]
fn marked_words_keep_four_tags_and_fit() {
    let picture = Picture::Board(board());
    let plain = status_text(&picture, "https://late.sh", Words::Plain);
    assert!(
        plain.ends_with("#ASCIIart #MastoArt #tildeverse #terminal"),
        "{plain}"
    );

    let labelled = status_text(
        &picture,
        "https://late.sh",
        Words::Marked { first_line: None },
    );
    assert!(labelled.starts_with("The Artboard yesterday"), "{labelled}");
    assert!(
        labelled.ends_with("#ASCIIart #MastoArt #tildeverse #nsfw"),
        "{labelled}"
    );

    let unlabelled = status_text(
        &picture,
        "https://late.sh",
        Words::Marked {
            first_line: Some("cw: not reviewed"),
        },
    );
    assert!(
        unlabelled.starts_with("cw: not reviewed\n\nThe Artboard"),
        "{unlabelled}"
    );
    assert!(unlabelled.ends_with("#nsfw"), "{unlabelled}");
    for text in [&plain, &labelled, &unlabelled] {
        assert!(text.chars().count() < TEXT_LIMIT, "{text}");
        assert_eq!(text.matches('#').count(), 4, "{text}");
    }
}

/// The look round-trips through the row; `Unseen` is the `"unseen"`
/// migration 255 wrote on the rows picked before it. A seen picture's alt
/// is the model's, prefixed.
#[test]
fn the_look_round_trips_and_writes_the_alt() {
    assert_eq!(
        serde_json::to_value(Look::Unseen).expect("serialize"),
        serde_json::json!("unseen")
    );
    let look = seen(Some(Warning::Gore));
    let stored = serde_json::to_value(&look).expect("serialize");
    assert_eq!(
        serde_json::from_value::<Look>(stored).expect("deserialize"),
        look
    );
    assert_eq!(
        alt_text(&Picture::Board(board()), &look),
        "ASCII art: A burning refinery across the top."
    );
}

/// A board is a post at the ink bar and not under it; a missing one
/// says so.
#[test]
fn the_board_is_a_post_at_the_ink_bar() {
    let look = |ink| BoardFound {
        key: Snapshot::daily_key(day(22)),
        shown: day(22),
        ink,
    };
    assert_eq!(
        look(Some(BOARD_MIN_INK)).picture(),
        Some(Picture::Board(board()))
    );
    assert_eq!(look(Some(BOARD_MIN_INK - 1)).picture(), None);
    assert_eq!(look(None).picture(), None);
    assert_eq!(
        look(Some(7)).describe(),
        "daily:2026-10-22 has 7 drawn cells, needs 200"
    );
    assert_eq!(look(None).describe(), "no board archived for 2026-10-22");
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

#[test]
fn herald_commands_parse_and_everything_else_falls_through() {
    assert_eq!(
        parse_herald_command("/herald on"),
        Some(Some(HeraldCommand::On))
    );
    assert_eq!(
        parse_herald_command("  /herald off  "),
        Some(Some(HeraldCommand::Off))
    );
    assert_eq!(
        parse_herald_command("/herald status"),
        Some(Some(HeraldCommand::Status))
    );
    assert_eq!(
        parse_herald_command("/herald preview"),
        Some(Some(HeraldCommand::Preview))
    );
    assert_eq!(
        parse_herald_command("/herald post"),
        Some(Some(HeraldCommand::Post))
    );
    // Bare, or junk after it, is a usage banner, not a chat line.
    assert_eq!(parse_herald_command("/herald"), Some(None));
    assert_eq!(parse_herald_command("/herald print"), Some(None));
    assert_eq!(parse_herald_command("/herald on now"), Some(None));
    // Other words and other commands fall through to chat.
    assert_eq!(parse_herald_command("/heraldry"), None);
    assert_eq!(parse_herald_command("the herald posted"), None);
}
