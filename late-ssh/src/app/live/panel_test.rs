use super::*;
use crate::app::door::spectate::state::SpectateGame;
use chrono::TimeZone;

const NOW: u64 = 1_790_000_000;
/// The rail's usable width: 24 columns less the separator, its padding
/// and the right inset.
const WIDTH: u16 = 21;

fn game(playname: &str, status: &str, minutes_in: u64, watching: usize) -> LivePanelRow {
    LivePanelRow::DoorGame {
        key: LiveGameKey::new(SpectateGame::Dcss, playname).expect("a handle"),
        status: status.to_string(),
        started_unix: NOW - minutes_in * 60,
        watching,
    }
}

fn texts(rows: &[LivePanelRow]) -> Vec<String> {
    panel_lines(WIDTH, rows, NOW)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

/// Four slots always: the handle in its column, where they are, and the
/// watchers at the edge when there are any; the time in until the host has
/// read a place; blank slots after.
#[test]
fn a_row_names_who_where_and_how_many_watch_in_the_rails_width() {
    let rows = [
        game("mat", "XL3 Lair:2", 12, 3),
        game("eggy", "", 12, 0),
        game("longhandle", "Depth 4", 125, 12),
    ];

    assert_eq!(
        texts(&rows),
        vec![
            "mat     XL3 Lair:2 ·3".to_string(),
            "eggy    12m in       ".to_string(),
            "longha… Depth 4   ·12".to_string(),
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

    assert_eq!(texts(&rows)[0], "mat     XL27 Zi… ·100");
}

/// More games than slots: the last slot counts the rest, and a click there
/// opens nothing.
#[test]
fn more_games_than_slots_fold_the_rest_into_the_last_row() {
    let rows: Vec<LivePanelRow> = ["ava", "bob", "cid", "dee", "eve", "fay"]
        .into_iter()
        .map(|name| game(name, "XL1 D:1", 1, 0))
        .collect();

    let lines = texts(&rows);
    assert_eq!(lines[2], "cid     XL1 D:1      ");
    assert_eq!(lines[3], "+3 more");
    let sources = hit_sources(&rows);
    assert_eq!(sources[2], Some(rows[2].source()));
    assert_eq!(sources[3], None);
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

/// Streams lead, newest first, and a pending one (no media yet) is not
/// listed; the games follow, newest first across the doors. A stream's row
/// is the streamer, the title behind the on-air mark, and its viewers.
#[test]
fn streams_lead_the_rows_newest_first_and_a_pending_one_is_not_listed() {
    let later = Utc.timestamp_opt(NOW as i64, 0).unwrap();
    let earlier = Utc.timestamp_opt(NOW as i64 - 600, 0).unwrap();
    let streams = [
        stream("dax", "late night coding", Some(later)),
        stream("mira", "pending", None),
        stream("tom", "synth jam", Some(earlier)),
    ];
    let door_game = |game, playname: &str, started_unix, status: &str| {
        crate::app::door::spectate::state::LiveRow {
            game,
            entry: crate::app::door::spectate::proxy::LiveGame {
                playname: playname.to_string(),
                started_unix,
                watchers: 0,
                status: status.to_string(),
            },
        }
    };
    // The hub's order: DCSS before NetHack. The newer NetHack game leads.
    let games = [
        door_game(SpectateGame::Dcss, "mat", NOW - 720, "XL3 Lair:2"),
        door_game(SpectateGame::Nethack, "eggy", NOW - 60, "Xp1 Dlvl:1"),
    ];
    let open_watches = OpenWatches::new();
    let _watching = open_watches.open(
        LiveGameKey::new(SpectateGame::Dcss, "mat").expect("a handle"),
        Uuid::now_v7(),
    );

    let rows = rows(&streams, &games, &open_watches);

    assert_eq!(
        texts(&rows),
        vec![
            "dax     \u{29bf} late ni… ·3".to_string(),
            "tom     \u{29bf} synth j… ·3".to_string(),
            "eggy    Xp1 Dlvl:1   ".to_string(),
            "mat     XL3 Lair:2 ·1".to_string(),
        ]
    );
    assert_eq!(rows[0].source(), LiveSource::Stream(streams[0].user_id));
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
