use chrono::{Duration, NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::artboard::Snapshot;
use late_core::models::herald::{HeraldKind, HeraldPicked, HeraldSlot};
use late_core::models::leaderboard::{ArcadeGame, DailyPuzzle, ScoreGame};
use late_core::models::showcase::{AuthoredShowcase, Showcase};
use uuid::Uuid;

use super::state::{
    ALT_MAX_CHARS, ANNOUNCEMENT_MAX_CHARS, AnnouncementCandidate, AnnouncementFound, ArtDay,
    ArtFound, BOARD_MIN_INK, BoardCandidate, BoardFound, BoardInk, Brief, Choice, Crop, Found,
    HeraldCommand, Image, LinkCard, Look, PROJECT_MIN_WORDS, PROJECT_POST_MAX_CHARS,
    PaperCandidate, Picture, PieceCandidate, ProjectCandidate, ProjectFound, ScoreLine,
    ScoreboardCandidate, Shape, Warning, WordsAlone, WordsFound, WordsKind, art_day, drawn_area,
    ink_cells, parse_herald_command, post_due_day, project_exclusions, scoreboard_week, shown_day,
    status_text, words_schedule,
};

/// The shortest limit among the networks: Bluesky's 300 graphemes
/// (Mastodon allows 500). Every network gets the same words, so every kind
/// fits the tightest.
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
                plays: "61 plays".to_string(),
            },
            ScoreLine {
                game: "Lateris".to_string(),
                plays: "1,234 plays".to_string(),
            },
        ],
    }
}

fn paper() -> PaperCandidate {
    PaperCandidate {
        edition: day(24),
        most_played: Some("Sudoku, 3 games finished".to_string()),
        yesterday: Some("Booth: 2 tracks queued; Arcade: 3 games finished, all Sudoku".to_string()),
        reading: Some("Someone shared a fine link.".to_string()),
        outside: Some("The world turned.".to_string()),
    }
}

fn announcement() -> AnnouncementCandidate {
    AnnouncementCandidate {
        edition: day(24),
        text: "Server updates no longer kick you out.\n\nOn the late CLI, an idle session moves to the new server by itself.".to_string(),
        brief: Brief::Model,
        summary: Some("The world turned.".to_string()),
    }
}

fn project() -> ProjectCandidate {
    ProjectCandidate {
        id: Uuid::from_u128(9),
        title: "tinyclock".to_string(),
        url: "https://example.com/tinyclock".to_string(),
        artist: "maker".to_string(),
        text: "A clock for your terminal that fits in one line.".to_string(),
    }
}

/// One of every kind, for the tables below.
fn every_kind() -> [Picture; 6] {
    [
        Picture::Board(board()),
        Picture::Piece(piece()),
        Picture::Scoreboard(scoreboard()),
        Picture::Paper(paper()),
        Picture::Announcement(announcement()),
        Picture::Project(project()),
    ]
}

/// Each slot is due at its own hour and catches up after it.
#[test]
fn each_post_is_due_at_its_own_hour_and_catches_up_after_it() {
    let at = |d, h, m, s| Utc.with_ymd_and_hms(2026, 10, d, h, m, s).unwrap();
    use HeraldSlot::{Art, Words};
    assert_eq!(post_due_day(at(23, 14, 59, 59), Words), day(22));
    assert_eq!(post_due_day(at(23, 15, 0, 0), Words), day(23));
    assert_eq!(
        post_due_day(at(23, 15, 0, 0), Art),
        day(22),
        "the words are out, the art is not due"
    );
    assert_eq!(post_due_day(at(23, 19, 59, 59), Art), day(22));
    assert_eq!(post_due_day(at(23, 20, 0, 0), Art), day(23));
    // A replica back at 02:00 owes the 23rd each post, not the 24th.
    assert_eq!(post_due_day(at(24, 2, 0, 0), Words), day(23));
    assert_eq!(post_due_day(at(24, 2, 0, 0), Art), day(23));
    // The post dated the 23rd shows the 22nd's board.
    assert_eq!(shown_day(day(23)), day(22));
}

/// Every kind knows its kind, subject, route and page, and round-trips
/// through the row. The whole table, so a moved route shows here.
#[test]
fn each_kind_names_its_subject_route_and_page() {
    let named: Vec<(HeraldKind, String, Option<String>, String)> = every_kind()
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
                match picture.shape() {
                    Shape::Image(image) => Some(image.path()),
                    Shape::Words(_) => None,
                },
                picture.page("https://late.sh"),
            )
        })
        .collect();
    let piece = "00000000-0000-0000-0000-000000000007";
    let project = "00000000-0000-0000-0000-000000000009";
    assert_eq!(
        named,
        vec![
            (
                HeraldKind::Artboard,
                "daily:2026-10-22".to_string(),
                Some("/gallery/daily:2026-10-22/image.png?crop=3,1,40,12".to_string()),
                "https://late.sh/gallery".to_string(),
            ),
            (
                HeraldKind::Piece,
                format!("piece:{piece}"),
                Some(format!("/gallery/pieces/{piece}/image.png")),
                format!("https://late.sh/gallery/pieces/{piece}"),
            ),
            (
                HeraldKind::Scoreboard,
                "week:2026-10-24".to_string(),
                Some("/games/week/2026-10-24/image.png".to_string()),
                "https://late.sh".to_string(),
            ),
            (
                HeraldKind::Paper,
                "paper:2026-10-24".to_string(),
                Some("/paper/2026-10-24/image.png".to_string()),
                "https://late.sh/paper/2026-10-24".to_string(),
            ),
            (
                HeraldKind::Announcement,
                "announcement:2026-10-24".to_string(),
                None,
                "https://late.sh/paper/2026-10-24".to_string(),
            ),
            (
                HeraldKind::Project,
                format!("project:{project}"),
                None,
                "https://example.com/tinyclock".to_string(),
            ),
        ]
    );
}

/// Every kind's words: under the tightest network's limit, no bare `@` (a
/// member is `~name`), the page linked, the kind's tags.
#[test]
fn the_words_fit_and_never_mention_anyone() {
    for picture in every_kind() {
        let text = status_text(&picture, "https://late.sh");
        assert!(text.chars().count() < TEXT_LIMIT, "{text}");
        assert!(
            !text.contains('@'),
            "a bare @ would ping a stranger: {text}"
        );
        assert!(
            text.contains(&format!("{}\n", picture.page("https://late.sh"))),
            "{text}"
        );
    }
    let tags = |picture: Picture| {
        let text = status_text(&picture, "https://late.sh");
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
        "#tildeverse #terminal #TUI #smolweb"
    );
    assert_eq!(
        tags(Picture::Announcement(announcement())),
        "#tildeverse #terminal #TUI"
    );
    assert_eq!(tags(Picture::Project(project())), "#tildeverse #terminal");
    assert_eq!(
        status_text(&Picture::Announcement(announcement()), "https://late.sh"),
        "Server updates no longer kick you out.\n\nOn the late CLI, an idle session moves to the new server by itself.\n\nFull notes: https://late.sh/paper/2026-10-24\n\n#tildeverse #terminal #TUI"
    );
    assert!(
        status_text(&Picture::Paper(paper()), "https://late.sh")
            .contains("\n\nFull edition: https://late.sh/paper/2026-10-24\n\n"),
    );
    assert_eq!(
        status_text(&Picture::Project(project()), "https://late.sh"),
        "\"tinyclock\" by ~maker, from late.sh.\n\nA clock for your terminal that fits in one line.\n\nhttps://example.com/tinyclock\n\n#tildeverse #terminal"
    );
    assert_eq!(
        announcement().link_card("https://late.sh"),
        LinkCard {
            uri: "https://late.sh/paper/2026-10-24".to_string(),
            title: "The Late Edition, 24 October 2026".to_string(),
            description: "The world turned.".to_string(),
        }
    );
    assert_eq!(
        AnnouncementCandidate {
            summary: None,
            ..announcement()
        }
        .link_card("https://late.sh")
        .description,
        "late.sh's daily paper."
    );
    assert_eq!(
        WordsAlone::Project(&project()).link_card("https://late.sh"),
        LinkCard {
            uri: "https://example.com/tinyclock".to_string(),
            title: "tinyclock".to_string(),
            description: "A clock for your terminal that fits in one line.".to_string(),
        }
    );
    let credited = status_text(&Picture::Piece(piece()), "https://late.sh");
    assert!(
        credited.starts_with("\"harbour at dusk\" by ~painter"),
        "{credited}"
    );
}

fn authored(title: &str, url: &str) -> AuthoredShowcase {
    let now = Utc::now();
    AuthoredShowcase {
        showcase: Showcase {
            id: Uuid::from_u128(9),
            created: now,
            updated: now,
            user_id: Uuid::from_u128(1),
            title: title.to_string(),
            url: url.to_string(),
            description: "A clock.".to_string(),
            tags: vec!["rust".to_string()],
        },
        username: "maker".to_string(),
    }
}

/// A project is a candidate only with a link a network makes live and room for
/// words: no other scheme, no credentials, no bare host, no space, and no URL
/// so long it eats the post. A URL's stray whitespace at the ends is the
/// editor's, and trimmed.
#[test]
fn a_project_needs_a_real_link_and_room_for_words() {
    let found = ProjectFound::of(authored("tinyclock", "  https://example.com/tinyclock "))
        .expect("eligible");
    assert_eq!(found.url, "https://example.com/tinyclock");
    for url in [
        "ftp://example.com/x",
        "javascript:alert(1)",
        "https://user:pass@example.com",
        "http://localhost:8080",
        "https://example.com/a b",
        "not a url",
    ] {
        assert_eq!(ProjectFound::of(authored("tinyclock", url)), None, "{url}");
    }
    let long = format!("https://example.com/{}", "x".repeat(300));
    assert_eq!(ProjectFound::of(authored("tinyclock", &long)), None);
    let longest_kept = format!("https://example.com/{}", "x".repeat(120));
    let found = ProjectFound::of(authored("tinyclock", &longest_kept)).expect("still has room");
    assert!(found.words_limit() >= PROJECT_MIN_WORDS);
}

/// The model's words go out plain, dead and cut to the room the post has, so a
/// whole post stays under the limit however long the title and the link run; a
/// member's title cannot mention, tag or link either.
#[test]
fn a_projects_words_are_plain_dead_and_fit_the_post() {
    let title = format!("@you #tag https://x.y {}", "t".repeat(100));
    let url = format!("https://example.com/{}", "x".repeat(80));
    let found = ProjectFound::of(authored(&title, &url)).expect("eligible");
    let candidate = ProjectCandidate::of(
        &found,
        &format!(
            "- See @you at #here https://x.y \u{2014} {}",
            "word ".repeat(100)
        ),
    );
    assert_eq!(candidate.text.chars().count(), found.words_limit());
    assert!(candidate.text.ends_with('…'), "{}", candidate.text);
    assert!(
        candidate
            .text
            .starts_with("See you at here https: //x.y, word"),
        "{}",
        candidate.text
    );

    let text = status_text(&Picture::Project(candidate), "https://late.sh");
    assert_eq!(text.chars().count(), PROJECT_POST_MAX_CHARS);
    assert!(text.chars().count() < TEXT_LIMIT, "{text}");
    let (words, tags) = text.rsplit_once("\n\n").expect("the tags close the post");
    assert!(!words.contains(['@', '#']), "{text}");
    assert_eq!(words.matches("://").count(), 1, "the link alone: {text}");
    assert_eq!(
        super::bluesky::facets(&text).len(),
        1 + tags.split(' ').count(),
        "the link and the tags: {text}"
    );
}

fn picked(day: NaiveDate, id: u128) -> HeraldPicked {
    HeraldPicked {
        day,
        subject: format!("project:{}", Uuid::from_u128(id)),
    }
}

/// The cooldown's picks are what the draw leaves out; a pick from Monday on
/// means the week is paid and nothing is owed. 26 October 2026 is a Monday.
#[test]
fn the_draw_leaves_out_the_cooldown_and_a_week_is_paid_once() {
    let last_week = day(26) - Duration::days(7);
    assert_eq!(
        project_exclusions(day(26), &[]).expect("read"),
        Some(Vec::new())
    );
    assert_eq!(
        project_exclusions(day(26), &[picked(last_week, 1), picked(day(25), 2)]).expect("read"),
        Some(vec![Uuid::from_u128(1), Uuid::from_u128(2)]),
        "Sunday is last week's"
    );
    assert_eq!(
        project_exclusions(day(28), &[picked(last_week, 1), picked(day(26), 2)]).expect("read"),
        None,
        "Monday's project pays through Friday"
    );
    assert!(
        project_exclusions(
            day(26),
            &[HeraldPicked {
                day: last_week,
                subject: "piece:1".to_string(),
            }]
        )
        .is_err(),
        "a subject only `Picture::subject` writes"
    );
}

/// A piece's title is the one thing in the words a member typed, and it
/// reaches the post dead: no mention, no tag, no link, on either network.
/// Mastodon reads all three out of the text; Bluesky makes links and tags
/// only from the facets, so every facet is the page or the kind's tags.
#[test]
fn a_title_cannot_mention_tag_or_link() {
    let titled = |title: &str| {
        Picture::Piece(PieceCandidate {
            title: title.to_string(),
            ..piece()
        })
    };
    for title in [
        "@you@mastodon.social hi",
        "#spam ＃tag ＠you",
        "see https://x.y or magnet:?xt",
    ] {
        let text = status_text(&titled(title), "https://late.sh");
        let (words, tags) = text.rsplit_once("\n\n").expect("the tags close the post");
        assert!(!words.contains(['@', '＠', '#', '＃']), "{text}");
        assert_eq!(words.matches("://").count(), 1, "the page alone: {text}");
        assert!(!words.contains("magnet:?"), "{text}");
        assert_eq!(
            super::bluesky::facets(&text).len(),
            1 + tags.split(' ').count(),
            "the page and the tags: {text}"
        );
    }
    let credited = status_text(
        &titled("@you@mastodon.social see https://x.y"),
        "https://late.sh",
    );
    assert!(
        credited.starts_with("\"youmastodon.social see https: //x.y\" by ~painter"),
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

/// A post goes out only clean or drawn: anything flagged, or that nobody could
/// check, stays off every network. The whole table, so a moved rule shows here.
#[test]
fn only_a_clean_look_or_a_card_goes_out() {
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
    let gates: Vec<(&str, bool)> = looks
        .iter()
        .map(|look| (look.as_str(), look.goes_out()))
        .collect();
    assert_eq!(
        gates,
        vec![
            ("clean", true),
            ("nudity", false),
            ("suggestive", false),
            ("explicit", false),
            ("gore", false),
            ("offensive", false),
            ("unseen", false),
            ("drawn", true),
        ]
    );
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
    let (the_board, the_piece, the_scoreboard, the_paper) =
        (board(), piece(), scoreboard(), paper());
    let alts: Vec<String> = [
        (Image::Board(&the_board), gore.clone()),
        (Image::Piece(&the_piece), gore),
        (Image::Board(&the_board), Look::Unseen),
        (Image::Piece(&the_piece), Look::Unseen),
        (Image::Scoreboard(&the_scoreboard), Look::Drawn),
        (Image::Paper(&the_paper), Look::Drawn),
    ]
    .iter()
    .map(|(image, look)| image.alt_text(look))
    .collect();
    assert_eq!(
        alts,
        vec![
            "ASCII art: A burning refinery across the top.",
            "ASCII art: A burning refinery across the top.",
            "ASCII art: the late.sh Artboard at the end of 22 October 2026 UTC. Coloured text on a dark background.",
            "ASCII art: \"harbour at dusk\", from the late.sh gallery. Coloured text on a dark background.",
            "A scoreboard in green and grey terminal text: everything played on late.sh, 18 October to 24 October 2026, game by game. Sudoku: 61 plays; Lateris: 1,234 plays.",
            "Front page of the Late Edition, late.sh's daily paper, 24 October 2026, in terminal text. Most played yesterday: Sudoku, 3 games finished. Yesterday on late.sh: Booth: 2 tracks queued; Arcade: 3 games finished, all Sudoku. What we were reading: Someone shared a fine link. Outside: The world turned.",
        ]
    );
    let long = PaperCandidate {
        reading: Some("word ".repeat(400)),
        ..paper()
    };
    let alt = Image::Paper(&long).alt_text(&Look::Drawn);
    assert_eq!(alt.chars().count(), ALT_MAX_CHARS);
    assert!(alt.ends_with('…'));
}

/// The paper's candidate carries the card's headline as the card writes
/// it (one game finished, or many) and each column's head; the headline
/// alone, with no column printed, is no edition.
#[test]
fn the_paper_carries_the_most_played_game_and_the_columns() {
    let sudoku = Some((ArcadeGame::Puzzle(DailyPuzzle::Sudoku), 3));
    assert_eq!(
        PaperCandidate::of(
            day(24),
            sudoku,
            Some("Booth: 2 tracks queued\n\nArcade: 3 games finished, all Sudoku"),
            Some("Someone shared a fine link.\nAnd another."),
            Some("The world turned."),
        ),
        Some(paper())
    );
    let one = PaperCandidate::of(
        day(24),
        Some((ArcadeGame::Score(ScoreGame::Snake), 1)),
        None,
        None,
        Some("The world turned."),
    )
    .expect("a column printed");
    assert_eq!(one.most_played.as_deref(), Some("Snake, 1 game finished"));
    let quiet = PaperCandidate::of(day(24), None, None, None, Some("The world turned."))
        .expect("a column printed");
    assert_eq!(quiet.most_played, None, "nobody played: no headline");
    assert_eq!(PaperCandidate::of(day(24), sudoku, None, None, None), None);
}

/// An announcement's words go out plain, dead and cut, whoever wrote
/// them: markdown markers dropped, a dash a comma, `@` and `#` gone, a
/// scheme broken but a time kept whole, at most `ANNOUNCEMENT_MAX_CHARS`
/// with an ellipsis; and under the page and the tags they fit the
/// tightest network.
#[test]
fn an_announcements_words_are_plain_dead_and_cut() {
    let note = "# Server updates no longer kick you out\n\n## Update your CLI\n\n- Run `curl -fsSL https://cli.late.sh/install.sh | bash`\n- Thanks @mat — see #lounge at 16:00\n";
    let cut = AnnouncementCandidate::of(day(24), note, Brief::Cut, None);
    assert_eq!(
        cut.text,
        "Server updates no longer kick you out\n\nUpdate your CLI\n\nRun `curl -fsSL https: //cli.late.sh/install.sh | bash`\nThanks mat, see lounge at 16:00"
    );
    let long = AnnouncementCandidate::of(day(24), &"word ".repeat(100), Brief::Model, None);
    assert_eq!(long.text.chars().count(), ANNOUNCEMENT_MAX_CHARS);
    assert!(long.text.ends_with('…'));
    let text = status_text(&Picture::Announcement(long), "https://late.sh");
    assert!(
        text.chars().count() < TEXT_LIMIT,
        "{}",
        text.chars().count()
    );
    assert!(
        text.ends_with(
            "\n\nFull notes: https://late.sh/paper/2026-10-24\n\n#tildeverse #terminal #TUI"
        ),
        "{text}"
    );
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

fn found_project() -> ProjectFound {
    ProjectFound::of(authored("tinyclock", "https://example.com/tinyclock")).expect("eligible")
}

/// The words slot over the week of Monday 19 October 2026: the kinds each day
/// would post, best first. A project while it is owed, Monday to Friday; the
/// paper on Wednesday and Saturday and behind Monday's project; the scoreboard
/// on Sunday, else the paper; the operator's word ahead of all of it; never art.
#[test]
fn the_words_slot_walks_the_days_schedule() {
    use HeraldKind::{Announcement, Paper, Project, Scoreboard};
    let found = |owed: bool, played: bool, printed: bool| {
        Found::Words(WordsFound {
            announcement: None,
            projects: match owed {
                true => vec![found_project()],
                false => Vec::new(),
            },
            scoreboard: played.then(scoreboard),
            paper: printed.then(paper),
        })
    };
    let week = |found: &Found| -> Vec<Vec<HeraldKind>> {
        (19..=25)
            .map(|d| {
                found
                    .choices(day(d))
                    .iter()
                    .map(|choice| match choice {
                        Choice::Picture(picture) => picture.kind(),
                        Choice::Announcement(_) => HeraldKind::Announcement,
                        Choice::Projects(_) => HeraldKind::Project,
                    })
                    .collect()
            })
            .collect()
    };

    assert_eq!(
        words_schedule(day(19)),
        &[WordsKind::Project, WordsKind::Paper]
    );
    assert_eq!(words_schedule(day(20)), &[WordsKind::Project]);
    assert_eq!(
        words_schedule(day(21)),
        &[WordsKind::Project, WordsKind::Paper]
    );
    assert_eq!(words_schedule(day(24)), &[WordsKind::Paper]);
    assert_eq!(
        words_schedule(day(25)),
        &[WordsKind::Scoreboard, WordsKind::Paper]
    );
    assert_eq!(
        week(&found(true, true, true)),
        vec![
            vec![Project, Paper],
            vec![Project],
            vec![Project, Paper],
            vec![Project],
            vec![Project],
            vec![Paper],
            vec![Scoreboard, Paper],
        ],
        "everything found: the schedule as written"
    );
    assert_eq!(
        week(&found(false, false, true)),
        vec![
            vec![Paper],
            vec![],
            vec![Paper],
            vec![],
            vec![],
            vec![Paper],
            vec![Paper],
        ],
        "no project owed, nobody played: the paper's days, and quiet otherwise"
    );
    assert_eq!(week(&found(false, false, false)), vec![Vec::new(); 7]);
    let spoke = Found::Words(WordsFound {
        announcement: Some(AnnouncementFound {
            edition: day(19),
            text: "A new late CLI is out.".to_string(),
            summary: None,
        }),
        projects: vec![found_project()],
        scoreboard: Some(scoreboard()),
        paper: Some(paper()),
    });
    let first: Vec<HeraldKind> = week(&spoke).into_iter().map(|kinds| kinds[0]).collect();
    assert_eq!(
        first,
        vec![Announcement; 7],
        "the operator's word jumps the schedule, whatever the day"
    );
    assert_eq!(
        found(false, false, false).describe(),
        "no announcement and nothing on the day's schedule (project, scoreboard, paper)"
    );
}

/// The art slot has one candidate a day and no fallback: the board on Monday,
/// the piece over the door the rest of the week. A bare board or an empty wall
/// is no post, and neither ever reaches for the other.
#[test]
fn the_art_slot_has_one_candidate_a_day() {
    let days: Vec<ArtDay> = (19..=25).map(|d| art_day(day(d))).collect();
    assert_eq!(
        days,
        vec![
            ArtDay::Board,
            ArtDay::Piece,
            ArtDay::Piece,
            ArtDay::Piece,
            ArtDay::Piece,
            ArtDay::Piece,
            ArtDay::Piece,
        ]
    );
    let kinds = |found: Found, d| -> Vec<HeraldKind> {
        found
            .choices(day(d))
            .iter()
            .map(|choice| match choice {
                Choice::Picture(picture) => picture.kind(),
                Choice::Announcement(_) | Choice::Projects(_) => panic!("art is a picture"),
            })
            .collect()
    };
    let inked = || Found::Art(ArtFound::Board(found_board(Some(BOARD_MIN_INK))));
    assert_eq!(kinds(inked(), 19), vec![HeraldKind::Artboard]);
    assert_eq!(
        kinds(Found::Art(ArtFound::Board(found_board(Some(3)))), 19),
        vec![]
    );
    assert_eq!(
        kinds(Found::Art(ArtFound::Piece(Some(piece()))), 20),
        vec![HeraldKind::Piece]
    );
    assert_eq!(kinds(Found::Art(ArtFound::Piece(None)), 20), vec![]);
    assert_eq!(
        Found::Art(ArtFound::Piece(None)).describe(),
        "no piece over the door"
    );
    assert_eq!(
        Found::Art(ArtFound::Board(found_board(Some(7)))).describe(),
        "daily:2026-10-22 has 7 drawn cells, needs 200"
    );
}

/// The scoreboard falls on Sundays and shows the week to Saturday.
#[test]
fn the_calendar_of_the_kinds() {
    let week: Vec<Option<NaiveDate>> = (19..=25).map(|d| scoreboard_week(day(d))).collect();
    assert_eq!(
        week,
        vec![None, None, None, None, None, None, Some(day(24))],
        "19 October 2026 is a Monday"
    );
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
        parse_herald_command("/herald preview words"),
        Some(Some(HeraldCommand::Preview(HeraldSlot::Words)))
    );
    assert_eq!(
        parse_herald_command("/herald preview art"),
        Some(Some(HeraldCommand::Preview(HeraldSlot::Art)))
    );
    assert_eq!(
        parse_herald_command("/herald post words"),
        Some(Some(HeraldCommand::Post(HeraldSlot::Words)))
    );
    assert_eq!(
        parse_herald_command("/herald post art"),
        Some(Some(HeraldCommand::Post(HeraldSlot::Art)))
    );
    // Bare, or junk after it, is a usage banner, not a chat line. Neither post
    // is the default: `post` and `preview` must name one.
    assert_eq!(parse_herald_command("/herald"), Some(None));
    assert_eq!(parse_herald_command("/herald post"), Some(None));
    assert_eq!(parse_herald_command("/herald preview"), Some(None));
    assert_eq!(parse_herald_command("/herald post paper"), Some(None));
    assert_eq!(parse_herald_command("/herald print"), Some(None));
    assert_eq!(parse_herald_command("/herald on now"), Some(None));
    // Other words and other commands fall through to chat.
    assert_eq!(parse_herald_command("/heraldry"), None);
    assert_eq!(parse_herald_command("the herald posted"), None);
}
