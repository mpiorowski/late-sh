use super::*;
use crate::app::door::spectate::{proxy::LiveGame, state::SpectateGame};
use chrono::TimeZone;
use late_core::models::article::Article;

const NOW_UNIX: u64 = 1_790_000_000;
/// The rail's usable width: 24 columns less the separator, its padding
/// and the right inset.
const WIDTH: u16 = 21;

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(NOW_UNIX as i64, 0).unwrap()
}

fn ago(minutes: i64) -> DateTime<Utc> {
    now() - Duration::minutes(minutes)
}

fn game(playname: &str, status: &str, minutes_in: u64, watching: usize) -> LivePanelRow {
    game_on(SpectateGame::Dcss, playname, status, minutes_in, watching)
}

fn game_on(
    game: SpectateGame,
    playname: &str,
    status: &str,
    minutes_in: u64,
    watching: usize,
) -> LivePanelRow {
    LivePanelRow::DoorGame {
        key: LiveGameKey::new(game, playname).expect("a handle"),
        status: status.to_string(),
        started_unix: NOW_UNIX - minutes_in * 60,
        watching,
    }
}

fn texts(rows: &[LivePanelRow]) -> Vec<String> {
    panel_lines(WIDTH, rows, now())
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

fn stream(username: &str, title: &str, went_live_at: Option<DateTime<Utc>>) -> LiveStreamView {
    LiveStreamView {
        user_id: Uuid::now_v7(),
        username: username.to_string(),
        title: title.to_string(),
        room_id: Uuid::now_v7(),
        voice_channel_id: Uuid::now_v7(),
        stream_id: format!("stream-{username}"),
        live: went_live_at.is_some(),
        went_live_at,
        watching: 3,
        watch_url: String::new(),
    }
}

fn door_game(game: SpectateGame, playname: &str, minutes_in: u64, status: &str) -> LiveRow {
    LiveRow {
        key: LiveGameKey::new(game, playname).expect("a handle"),
        entry: LiveGame {
            playname: playname.to_string(),
            started_unix: NOW_UNIX - minutes_in * 60,
            watchers: 0,
            status: status.to_string(),
        },
    }
}

fn article(shared_by: &str, title: &str, shared_at: DateTime<Utc>) -> ArticleFeedItem {
    ArticleFeedItem {
        article: Article {
            id: Uuid::now_v7(),
            created: shared_at,
            updated: shared_at,
            user_id: Uuid::now_v7(),
            url: format!("https://example.com/{title}"),
            title: title.to_string(),
            summary: String::new(),
            ascii_art: String::new(),
        },
        author_username: shared_by.to_string(),
    }
}

/// Four slots always: the game in the first column, where its player is,
/// the watchers when there are any, and the row's key at the right; the
/// time in until the host has read a place; blank slots after. The handle
/// is not on the row.
#[test]
fn a_row_names_what_where_and_how_many_watch_in_the_rails_width() {
    let rows = [
        game("mat", "XL3 Lair:2", 12, 3),
        game_on(SpectateGame::Nethack, "eggy", "", 12, 0),
        game_on(SpectateGame::Brogue, "longhandle", "Depth 4", 125, 12),
    ];

    assert_eq!(
        texts(&rows),
        vec![
            "dcss    XL3 La… ·3 s1".to_string(),
            "nethack 12m in     s2".to_string(),
            "brogue  Depth… ·12 s3".to_string(),
            String::new(),
        ]
    );
    for line in texts(&rows) {
        assert!(line.chars().count() <= usize::from(WIDTH), "{line:?}");
    }
}

/// A place too long for what the handle and the count leave is cut, never
/// wrapped or pushed out of the rail.
#[test]
fn a_long_place_is_cut_to_the_room_left() {
    let rows = [game("mat", "XL27 Zig:27 somewhere", 1, 100)];

    assert_eq!(texts(&rows)[0], "dcss    XL27… ·100 s1");
}

/// More rows than slots: the last slot counts the rest, and a click there
/// opens nothing.
#[test]
fn more_rows_than_slots_fold_the_rest_into_the_last_row() {
    let rows: Vec<LivePanelRow> = ["ava", "bob", "cid", "dee", "eve", "fay"]
        .into_iter()
        .map(|name| game(name, "XL1 D:1", 1, 0))
        .collect();

    let lines = texts(&rows);
    assert_eq!(lines[2], "dcss    XL1 D:1    s3");
    assert_eq!(lines[3], "+3 more");
    let sources = hit_sources(&rows);
    assert_eq!(sources[2], Some(rows[2].source()));
    assert_eq!(sources[3], None);
}

/// Each kind is newest first and the kinds are grouped, streams, games,
/// news; a pending stream (no media yet) is not listed, and a share older
/// than the lifetime is gone. A stream's row says `stream`, the streamer
/// and the title, and its viewers; a share's says `news`, the title, and
/// the dot while it is unread.
#[test]
fn each_kind_is_newest_first_and_stale_or_pending_things_are_not_listed() {
    let streams = [
        stream("dax", "late night coding", Some(ago(1))),
        stream("mira", "pending", None),
    ];
    let games = [
        door_game(SpectateGame::Dcss, "mat", 12, "XL3 Lair:2"),
        door_game(SpectateGame::Nethack, "eggy", 1, "Xp1 Dlvl:1"),
    ];
    let articles = [
        article("tom", "an old link", ago(61)),
        article("ann", "the freshest", ago(2)),
    ];
    let open_watches = OpenWatches::new();
    let _watching = open_watches.open(
        LiveGameKey::new(SpectateGame::Dcss, "mat").expect("a handle"),
        Uuid::now_v7(),
    );

    let rows = rows(&streams, &games, &articles, ReadCursor::Loaded(None), &open_watches, now());

    assert_eq!(
        texts(&rows),
        vec![
            "stream  dax la… ·3 s1".to_string(),
            "nethack Xp1 Dlvl:1 s2".to_string(),
            "dcss    XL3 La… ·1 s3".to_string(),
            "news    the fre… \u{25cf} s4".to_string(),
        ]
    );
    assert_eq!(rows[0].source(), LiveSource::Stream(streams[0].user_id));
    assert_eq!(
        rows[3].source(),
        LiveSource::NewsArticle(articles[1].article.id)
    );
}

/// The dot follows the News badge's rule: a share older than the reader's
/// cursor carries none, and nothing is marked until the cursor has
/// loaded, so the panel never guesses at what was read.
#[test]
fn a_share_is_dotted_while_unread_by_the_news_cursor() {
    let articles = [article("ann", "the freshest", ago(2))];
    let open_watches = OpenWatches::new();
    let text_at = |cursor: ReadCursor| {
        texts(&rows(&[], &[], &articles, cursor, &open_watches, now()))[0].clone()
    };

    assert_eq!(
        text_at(ReadCursor::Loaded(Some(ago(5)))),
        "news    the fre… \u{25cf} s1"
    );
    assert_eq!(
        text_at(ReadCursor::Loaded(Some(ago(1)))),
        "news    the fresh… s1"
    );
    assert_eq!(text_at(ReadCursor::Loading), "news    the fresh… s1");
}

/// The floor rule: every kind with something gets a row, the leftover goes
/// streams, then games, then news, the rest folds, and what made it shows
/// grouped by kind.
#[test]
fn every_kind_keeps_a_row_and_the_leftover_goes_by_priority() {
    let streams: Vec<LiveStreamView> = (0..4)
        .map(|n| stream(&format!("s{n}"), "show", Some(ago(n))))
        .collect();
    let games: Vec<LiveRow> = (0..4)
        .map(|n| door_game(SpectateGame::Dcss, &format!("game{n}"), n as u64, &format!("g{n}")))
        .collect();
    let articles: Vec<ArticleFeedItem> = (0..4)
        .map(|n| article("ann", &format!("n{n}"), ago(n)))
        .collect();
    let open_watches = OpenWatches::new();
    // The word after the kind: the streamer, the game's status, the
    // title; the count row's `+N`.
    let heads = |rows: &[LivePanelRow]| -> Vec<String> {
        texts(rows)
            .into_iter()
            .map(|line| {
                let mut words = line.split_whitespace();
                match words.next() {
                    Some(count) if count.starts_with('+') => count.to_string(),
                    Some(_) => words.next().unwrap_or("").to_string(),
                    None => String::new(),
                }
            })
            .collect()
    };

    // Four of everything: one of each, then the count.
    assert_eq!(
        heads(&rows(&streams, &games, &articles, ReadCursor::Loaded(None), &open_watches, now())),
        vec!["s0", "g0", "n0", "+9"]
    );
    // Two streams and two games against four shares: three live, one share.
    assert_eq!(
        heads(&rows(
            &streams[..2],
            &games[..2],
            &articles,
            ReadCursor::Loaded(None),
            &open_watches,
            now()
        )),
        vec!["s0", "g0", "n0", "+5"]
    );
    // One game and four shares: the game, then the shares fill the rest.
    assert_eq!(
        heads(&rows(&[], &games[..1], &articles, ReadCursor::Loaded(None), &open_watches, now())),
        vec!["g0", "n0", "n1", "+2"]
    );
    // Nothing live: four shares.
    assert_eq!(
        heads(&rows(&[], &[], &articles, ReadCursor::Loaded(None), &open_watches, now())),
        vec!["n0", "n1", "n2", "n3"]
    );
    // Two streams and one game, nothing to read: the leftover is a stream,
    // shown with the other stream, above the game.
    assert_eq!(
        heads(&rows(&streams[..2], &games[..1], &[], ReadCursor::Loaded(None), &open_watches, now())),
        vec!["s0", "s1", "g0", ""]
    );
}

#[test]
fn nobody_playing_says_so_in_the_first_slot_alone() {
    assert_eq!(
        texts(&[]),
        vec![
            "nobody playing".to_string(),
            String::new(),
            String::new(),
            String::new()
        ]
    );
    assert_eq!(hit_sources(&[]), [None; LIVE_PANEL_ROWS]);
}
