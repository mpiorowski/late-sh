use chrono::{TimeZone, Utc};

use super::*;

const STARTED: u64 = 1_790_000_000;

fn row(playname: &str, status: &str, watchers: usize) -> LiveRow {
    LiveRow {
        game: SpectateGame::Dcss,
        entry: LiveGame {
            playname: playname.to_string(),
            started_unix: STARTED,
            watchers,
            status: status.to_string(),
        },
    }
}

fn key(playname: &str) -> LiveGameKey {
    LiveGameKey::new(SpectateGame::Dcss, playname).expect("a handle")
}

fn text(spans: &[Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

/// A game joins the queue when it starts, stamped with the host's start
/// time, which every replica reads the same. The viewer's own running game
/// is never offered.
#[test]
fn every_live_game_but_your_own_is_offered_stamped_with_its_start() {
    let live = [row("mat", "XL3 Lair:2", 0), row("eggy", "", 1)];

    let offered = candidates(&live, &[key("eggy")]);

    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].source, LiveSource::DoorGame(key("mat")));
    assert_eq!(
        offered[0].updated,
        Utc.timestamp_opt(STARTED as i64, 0).unwrap()
    );
    assert_eq!(candidates(&live, &[]).len(), 2);
}

#[test]
fn a_game_no_longer_listed_has_no_view() {
    let live = [row("mat", "", 0)];

    assert!(view(&live, key("mat")).is_some());
    assert!(view(&live, key("eggy")).is_none());
}

#[test]
fn the_body_names_the_door_where_they_are_who_watches_and_the_key() {
    let live = [row("mat", "XL3 Lair:2", 2)];
    let strip = view(&live, key("mat")).expect("a listed game has a view");

    let body = body(60, &strip);
    let words: Vec<String> = body.words.iter().map(|spans| text(spans)).collect();

    assert_eq!(words[1], "DCSS");
    assert_eq!(words[2], "XL3 Lair:2 · 2 watching");
    assert_eq!(words[3], "mat is playing");
    assert_eq!(text(&body.hint), "o or click to watch");
    assert!(body.glow);
    assert_eq!(text(&compact_spans(60, &strip)), "dcss mat · XL3 Lair:2");
}

/// Until the host has read the game's HUD there is no place to name, so the
/// strip says how long they have been in; nobody watching says nothing.
#[test]
fn a_game_without_a_status_reads_its_time_in() {
    let live = [row("mat", "", 0)];
    let strip = view(&live, key("mat")).expect("a listed game has a view");

    let words: Vec<String> = body(60, &strip)
        .words
        .iter()
        .map(|spans| text(spans))
        .collect();

    assert!(words[2].ends_with(" in"), "{}", words[2]);
    assert!(!words[2].contains("watching"));
}

/// Each door draws its own dungeon, framed to exactly the picture column, and
/// carries its own name and status.
#[test]
fn every_door_draws_its_own_picture_in_the_picture_column() {
    let statuses = [
        (SpectateGame::Dcss, "XL3 Lair:2"),
        (SpectateGame::Nethack, "Xp3 Dlvl:4"),
        (SpectateGame::Brogue, "Depth 4"),
    ];
    let pictures: Vec<Vec<String>> = statuses
        .into_iter()
        .map(|(game, status)| {
            let live = [LiveRow {
                game,
                ..row("mat", status, 0)
            }];
            let key = LiveGameKey::new(game, "mat").expect("a handle");
            let strip = view(&live, key).expect("a listed game has a view");
            let body = body(60, &strip);
            let words: Vec<String> = body.words.iter().map(|spans| text(spans)).collect();
            assert_eq!(words[1], game.label());
            assert_eq!(words[2], status);
            body.picture.iter().map(|line| text(&line.spans)).collect()
        })
        .collect();

    for lines in &pictures {
        assert!(lines.len() <= usize::from(PICTURE_ROWS));
        for line in lines {
            assert_eq!(line.chars().count(), usize::from(PICTURE_COLS), "{line:?}");
        }
    }
    assert_ne!(pictures[0], pictures[1]);
    assert_ne!(pictures[0], pictures[2]);
    assert_ne!(pictures[1], pictures[2]);
}
