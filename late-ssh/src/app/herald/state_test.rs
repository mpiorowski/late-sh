use chrono::{NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::artboard::Snapshot;
use late_core::models::herald::HeraldKind;
use uuid::Uuid;

use super::state::{
    ALT_MAX_CHARS, BOARD_MIN_INK, BlueskyMark, BoardCandidate, BoardFound, BoardInk, Crop, Found,
    HeraldCommand, Look, MastodonPost, PaperCandidate, Picture, PieceCandidate, ScoreLine,
    ScoreboardCandidate, Warning, Words, alt_text, bluesky_mark, drawn_area, ink_cells,
    mastodon_post, note_text, parse_herald_command, picked_pieces, piece_window_start,
    post_due_day, scoreboard_week, shown_day, status_text,
};

/// The shortest limit among the networks: Bluesky's 300 graphemes
/// (Mastodon allows 500). The words differ between networks only by the
/// mark, so every variant fits the tightest.
const TEXT_LIMIT: usize = 300;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

const CROP: Crop = Crop {
    x: 3,
    y: 1,
    width: 40,
    height: 12,
};

fn board() -> BoardCandidate {
    BoardCandidate {
        key: Snapshot::daily_key(day(22)),
        shown: day(22),
        crop: CROP,
    }
}

fn piece() -> PieceCandidate {
    PieceCandidate {
        id: Uuid::from_u128(7),
        title: "harbour at dusk".to_string(),
        artist: "painter".to_string(),
    }
}

fn scoreboard() -> ScoreboardCandidate {
    ScoreboardCandidate {
        last: day(24),
        lines: vec![
            ScoreLine {
                game: "Sudoku".to_string(),
                player: "alice".to_string(),
                best: "6 wins".to_string(),
            },
            ScoreLine {
                game: "Lateris".to_string(),
                player: "bob".to_string(),
                best: "12,340".to_string(),
            },
        ],
    }
}

fn paper() -> PaperCandidate {
    PaperCandidate {
        edition: day(24),
        outside: Some("The world turned.".to_string()),
        reading: None,
    }
}

/// One of every kind, for the tables below.
fn every_kind() -> [Picture; 4] {
    [
        Picture::Board(board()),
        Picture::Piece(piece()),
        Picture::Scoreboard(scoreboard()),
        Picture::Paper(paper()),
    ]
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

/// Every kind knows its kind, subject, route and page, and round-trips
/// through the row. The whole table, so a moved route shows here.
#[test]
fn each_kind_names_its_subject_route_and_page() {
    let named: Vec<(HeraldKind, String, String, Option<String>)> = every_kind()
        .iter()
        .map(|picture| {
            let stored = serde_json::to_value(picture).expect("serialize");
            assert_eq!(
                &serde_json::from_value::<Picture>(stored).expect("deserialize"),
                picture
            );
            (
                picture.kind(),
                picture.subject(),
                picture.picture_path(),
                picture.page_path(),
            )
        })
        .collect();
    let piece = "00000000-0000-0000-0000-000000000007";
    assert_eq!(
        named,
        vec![
            (
                HeraldKind::Artboard,
                "daily:2026-10-22".to_string(),
                "/gallery/daily:2026-10-22/image.png?crop=3,1,40,12".to_string(),
                Some("/gallery".to_string()),
            ),
            (
                HeraldKind::Piece,
                format!("piece:{piece}"),
                format!("/gallery/pieces/{piece}/image.png"),
                Some(format!("/gallery/pieces/{piece}")),
            ),
            (
                HeraldKind::Scoreboard,
                "week:2026-10-24".to_string(),
                "/games/week/2026-10-24/image.png".to_string(),
                None,
            ),
            (
                HeraldKind::Paper,
                "paper:2026-10-24".to_string(),
                "/paper/2026-10-24/image.png".to_string(),
                Some("/paper/2026-10-24".to_string()),
            ),
        ]
    );
}

/// Every kind's words, plain and marked, and the Mastodon note: under the
/// tightest network's limit, no bare `@` (a member is `~name`), the page
/// linked (the front page for the note and the scoreboard), the kind's
/// tags.
#[test]
fn the_words_fit_and_never_mention_anyone() {
    for picture in every_kind() {
        let texts = [
            status_text(&picture, "https://late.sh", Words::Plain),
            status_text(
                &picture,
                "https://late.sh",
                Words::Marked {
                    first_line: Some("cw: not reviewed"),
                },
            ),
            note_text(&picture, "https://late.sh"),
        ];
        for text in &texts {
            assert!(text.chars().count() < TEXT_LIMIT, "{text}");
            assert!(
                !text.contains('@'),
                "a bare @ would ping a stranger: {text}"
            );
        }
        let page = match picture.page_path() {
            Some(path) => format!("https://late.sh{path}\n"),
            None => "https://late.sh\n".to_string(),
        };
        assert!(texts[0].contains(&page), "{}", texts[0]);
        assert!(texts[2].contains("\n\nhttps://late.sh\n\n"), "{}", texts[2]);
        assert!(texts[2].ends_with("#tildeverse #terminal"), "{}", texts[2]);
        assert!(!texts[2].contains("#MastoArt"), "{}", texts[2]);
    }
    let tags = |picture: Picture| {
        let text = status_text(&picture, "https://late.sh", Words::Plain);
        text.rsplit("\n\n").next().expect("tags").to_string()
    };
    assert_eq!(
        tags(Picture::Board(board())),
        "#ASCIIart #MastoArt #tildeverse #terminal"
    );
    assert_eq!(
        tags(Picture::Piece(piece())),
        "#ASCIIart #MastoArt #tildeverse #terminal"
    );
    assert_eq!(
        tags(Picture::Scoreboard(scoreboard())),
        "#puzzles #sudoku #tildeverse #terminal"
    );
    assert_eq!(
        tags(Picture::Paper(paper())),
        "#tildeverse #terminal #smolweb"
    );
    let credited = status_text(&Picture::Piece(piece()), "https://late.sh", Words::Plain);
    assert!(
        credited.starts_with("\"harbour at dusk\" by ~painter"),
        "{credited}"
    );
}

fn seen(warning: Option<Warning>) -> Look {
    Look::Seen {
        alt: "A burning refinery across the top.".to_string(),
        warning,
        reason: String::new(),
    }
}

/// Every look, and what each network makes of it: Mastodon the picture
/// only when clean (Fosstodon bans the rest behind a warning too), a note
/// behind the warning otherwise; Bluesky's self-label and words. The
/// whole table, so a moved mark shows here.
#[test]
fn each_look_maps_to_each_networks_mark() {
    let marked = |first_line| Words::Marked { first_line };
    let note = |warning| MastodonPost::Note { warning };
    let looks = [
        seen(None),
        seen(Some(Warning::Nudity)),
        seen(Some(Warning::Suggestive)),
        seen(Some(Warning::Explicit)),
        seen(Some(Warning::Gore)),
        seen(Some(Warning::Offensive)),
        Look::Unseen,
        Look::Drawn,
    ];
    let marks: Vec<(&str, MastodonPost, BlueskyMark)> = looks
        .iter()
        .map(|look| (look.as_str(), mastodon_post(look), bluesky_mark(look)))
        .collect();
    let bluesky = |label, words| BlueskyMark { label, words };
    assert_eq!(
        marks,
        vec![
            ("clean", MastodonPost::Picture, bluesky(None, Words::Plain)),
            (
                "nudity",
                note("nsfw: nudity"),
                bluesky(Some("nudity"), marked(None))
            ),
            (
                "suggestive",
                note("nsfw: suggestive"),
                bluesky(Some("sexual"), marked(None))
            ),
            (
                "explicit",
                note("nsfw: explicit"),
                bluesky(Some("porn"), marked(None))
            ),
            (
                "gore",
                note("cw: gore"),
                bluesky(Some("graphic-media"), marked(None))
            ),
            (
                "offensive",
                note("cw: offensive"),
                bluesky(None, marked(Some("cw: offensive")))
            ),
            (
                "unseen",
                note("cw: not reviewed"),
                bluesky(None, marked(Some("cw: not reviewed")))
            ),
            ("drawn", MastodonPost::Picture, bluesky(None, Words::Plain)),
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
/// is the model's, prefixed; an unseen one's says what it is; a card's is
/// its own words, cut at the cap.
#[test]
fn the_look_round_trips_and_writes_the_alt() {
    assert_eq!(
        serde_json::to_value(Look::Unseen).expect("serialize"),
        serde_json::json!("unseen")
    );
    for look in [seen(Some(Warning::Gore)), Look::Drawn] {
        let stored = serde_json::to_value(&look).expect("serialize");
        assert_eq!(
            serde_json::from_value::<Look>(stored).expect("deserialize"),
            look
        );
    }
    let gore = seen(Some(Warning::Gore));
    let alts: Vec<String> = [
        (Picture::Board(board()), gore.clone()),
        (Picture::Piece(piece()), gore),
        (Picture::Board(board()), Look::Unseen),
        (Picture::Piece(piece()), Look::Unseen),
        (Picture::Scoreboard(scoreboard()), Look::Drawn),
        (Picture::Paper(paper()), Look::Drawn),
    ]
    .iter()
    .map(|(picture, look)| alt_text(picture, look))
    .collect();
    assert_eq!(
        alts,
        vec![
            "ASCII art: A burning refinery across the top.",
            "ASCII art: A burning refinery across the top.",
            "ASCII art: the late.sh Artboard, a shared terminal canvas, as it stood at the end of 22 October 2026 UTC. Coloured text glyphs on a dark background.",
            "ASCII art: \"harbour at dusk\", a piece hung in the late.sh gallery. Coloured text glyphs on a dark background.",
            "A scoreboard in green and grey terminal text: the week's best in every late.sh game, 18 October to 24 October 2026. Sudoku: ~alice, 6 wins; Lateris: ~bob, 12,340.",
            "The front page of the Late Edition, late.sh's daily paper, for 24 October 2026, in terminal text. Outside: The world turned.",
        ]
    );
    let long = Picture::Paper(PaperCandidate {
        reading: Some("word ".repeat(400)),
        ..paper()
    });
    let alt = alt_text(&long, &Look::Drawn);
    assert_eq!(alt.chars().count(), ALT_MAX_CHARS);
    assert!(alt.ends_with('…'));
}

fn found_board(cells: Option<usize>) -> BoardFound {
    BoardFound {
        key: Snapshot::daily_key(day(22)),
        shown: day(22),
        ink: cells.map(|cells| BoardInk {
            cells,
            area: Some(CROP),
        }),
    }
}

/// A board is a post at the ink bar and not under it, cropped to what is
/// drawn; a missing one says so.
#[test]
fn the_board_is_a_post_at_the_ink_bar() {
    assert_eq!(
        found_board(Some(BOARD_MIN_INK)).picture(),
        Some(Picture::Board(board()))
    );
    assert_eq!(found_board(Some(BOARD_MIN_INK - 1)).picture(), None);
    assert_eq!(found_board(None).picture(), None);
    assert_eq!(
        found_board(Some(7)).describe(),
        "daily:2026-10-22 has 7 drawn cells, needs 200"
    );
    assert_eq!(
        found_board(None).describe(),
        "no board archived for 2026-10-22"
    );
}

/// The rotation: the scoreboard on its Sunday, else a waiting piece, else
/// the board with ink, else the paper, else nothing.
#[test]
fn the_rotation_takes_the_first_kind_that_has_something() {
    let found = |sunday: bool, waiting: bool, ink: Option<usize>, printed: bool| Found {
        scoreboard: sunday.then(scoreboard),
        piece: waiting.then(piece),
        board: found_board(ink),
        paper: printed.then(paper),
    };
    let bar = Some(BOARD_MIN_INK);
    let picks: Vec<Option<HeraldKind>> = [
        found(true, true, bar, true),
        found(false, true, bar, true),
        found(false, false, bar, true),
        found(false, false, Some(3), true),
        found(false, false, None, false),
    ]
    .iter()
    .map(|found| found.picture().map(|picture| picture.kind()))
    .collect();
    assert_eq!(
        picks,
        vec![
            Some(HeraldKind::Scoreboard),
            Some(HeraldKind::Piece),
            Some(HeraldKind::Artboard),
            Some(HeraldKind::Paper),
            None,
        ]
    );
    assert_eq!(
        found(false, false, None, false).describe(),
        "no piece waiting, no board archived for 2026-10-22, no edition printed"
    );
}

/// The scoreboard falls on Sundays and shows the week to Saturday; a
/// piece waits a week; the picked pieces read back from their subjects,
/// and a subject that is not a piece's is refused.
#[test]
fn the_calendar_of_the_kinds() {
    let week: Vec<Option<NaiveDate>> = (19..=25).map(|d| scoreboard_week(day(d))).collect();
    assert_eq!(
        week,
        vec![None, None, None, None, None, None, Some(day(24))],
        "19 October 2026 is a Monday"
    );
    assert_eq!(piece_window_start(day(25)), day(18));
    let id = Uuid::from_u128(7);
    assert_eq!(
        picked_pieces(&[format!("piece:{id}")]).expect("parse"),
        vec![id]
    );
    assert!(picked_pieces(&["daily:2026-10-22".to_string()]).is_err());
    assert!(picked_pieces(&["piece:nope".to_string()]).is_err());
}

/// The crop is the drawn cells with a margin, kept inside the board; a
/// wide glyph's second cell counts; a blank board has none.
#[test]
fn the_crop_is_the_drawn_area_with_a_margin() {
    let mut canvas = Canvas::with_size(80, 24);
    assert_eq!(drawn_area(&canvas), None);
    canvas.set(Pos { x: 10, y: 5 }, '#');
    canvas.put_glyph(Pos { x: 30, y: 9 }, '界');
    canvas.set(Pos { x: 20, y: 7 }, ' ');
    assert_eq!(
        drawn_area(&canvas),
        Some(Crop {
            x: 8,
            y: 3,
            width: 26,
            height: 9,
        })
    );
    canvas.set(Pos { x: 0, y: 0 }, '#');
    canvas.set(Pos { x: 79, y: 23 }, '#');
    assert_eq!(
        drawn_area(&canvas),
        Some(Crop {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }),
        "the margin stops at the board's edge"
    );
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
