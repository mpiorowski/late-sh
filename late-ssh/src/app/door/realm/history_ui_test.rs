use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::{HistoryFrame, render};
use crate::app::common::worldmap::data::map_by_id;
use crate::app::common::worldmap::view::Viewport;
use crate::app::door::realm::resolver::{
    DayBoard, DayPlayer, DayResult, LogEntry, RealmGameState, STATE_VERSION, TakerRoll, WinKind,
};
use crate::app::door::realm::rulesets::STANDARD;
use crate::app::door::realm::state::palette_shades;
use crate::app::door::realm::svc::ArchivedDay;

/// The live game the rail names people from. Empty of players on purpose:
/// everything the history draws about *who* comes from the day's own board,
/// and this proves it.
fn live_state() -> RealmGameState {
    RealmGameState {
        version: STATE_VERSION,
        revision: 1,
        ruleset: (&STANDARD).into(),
        players: Vec::new(),
        ownership: Default::default(),
        forts: Default::default(),
        last_day: None,
        start_player_count: 0,
        start_day: 0,
        news: None,
        last_night: None,
    }
}

fn player(n: u128, name: &str, color: u8) -> DayPlayer {
    DayPlayer {
        user_id: Uuid::from_u128(n),
        username: name.to_string(),
        color: Some(color),
    }
}

fn archived(day: i32, board: Option<DayBoard>, entries: Vec<LogEntry>) -> ArchivedDay {
    ArchivedDay {
        result: DayResult {
            day,
            seed: 1,
            entries,
        },
        board,
        board_open: None,
    }
}

/// The snapshot answers "who held this" on its own, which is the whole
/// reason it carries a roster: a player who withdrew is gone from the live
/// game, and the history still has to know their name and their colour.
#[test]
fn a_board_answers_for_itself() {
    let board = DayBoard {
        players: vec![player(1, "alice", 0), player(2, "bob", 3)],
        owned: vec![(10, 0), (11, 0), (12, 1)],
        forts: vec![(11, 2)],
    };
    assert_eq!(board.owner(10).map(|p| p.username.as_str()), Some("alice"));
    assert_eq!(board.owner(12).map(|p| p.username.as_str()), Some("bob"));
    assert_eq!(board.owner(99), None, "unowned ground names nobody");
    assert_eq!(board.fort_level(11), 2);
    assert_eq!(board.fort_level(10), 0);

    let standings = board.standings();
    assert_eq!(standings[0].0.username, "alice");
    assert_eq!(standings[0].1, 2);
    assert_eq!(standings[1].1, 1, "busiest first");
    assert!(!board.is_empty());
    assert!(DayBoard::default().is_empty());
}

/// A slot that no longer exists in the live roster must not silently become
/// somebody else. This is the failure the self-describing snapshot exists to
/// prevent, so it is worth its own test.
#[test]
fn a_slot_past_the_roster_names_nobody() {
    let board = DayBoard {
        players: vec![player(1, "alice", 0)],
        owned: vec![(10, 0), (11, 7)],
        forts: Vec::new(),
    };
    assert_eq!(board.owner(10).map(|p| p.username.as_str()), Some("alice"));
    assert_eq!(board.owner(11), None);
}

fn draw_days(days: &[ArchivedDay], selected: Option<i32>, w: u16, h: u16) -> (String, Vec<Color>) {
    draw_run(days, selected, w, h, false, None)
}

fn draw_run(
    days: &[ArchivedDay],
    selected: Option<i32>,
    w: u16,
    h: u16,
    loading: bool,
    replay_step: Option<usize>,
) -> (String, Vec<Color>) {
    let map = map_by_id("earth").expect("earth");
    // The view offers only days that kept a board, oldest first — the same
    // filter the live state applies.
    let shown: Vec<&ArchivedDay> = {
        let mut shown: Vec<&ArchivedDay> = days.iter().filter(|d| d.board.is_some()).collect();
        shown.sort_by_key(|d| d.day());
        shown
    };
    let index = selected
        .and_then(|day| shown.iter().position(|d| d.day() == day))
        // Same default as the view: the first day of the run.
        .unwrap_or(0);
    let live = live_state();
    let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
    terminal
        .draw(|frame| {
            let area = frame.area();
            let canvas_h = area.height.saturating_sub(1) as i32 * 2;
            render(
                frame,
                area,
                &HistoryFrame {
                    map: &map,
                    days: &shown,
                    index,
                    live: &live,
                    loading,
                    view: Viewport::fitted(&map, area.width as i32, canvas_h),
                    replay_step,
                },
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    let colors = buffer.content().iter().flat_map(|c| [c.fg, c.bg]).collect();
    (text, colors)
}

/// The point of the feature: the map is painted from the day being looked
/// at, in the colours that day was played in.
#[test]
fn the_map_is_painted_from_the_day_on_screen() {
    // Poland and New Zealand, held by different people on different days.
    let poland = crate::app::common::worldmap::data::map_by_id("earth")
        .unwrap()
        .territories
        .iter()
        .find(|t| t.iso == "PL")
        .unwrap()
        .id;
    let early = DayBoard {
        players: vec![player(1, "alice", 0), player(2, "bob", 4)],
        owned: vec![(poland, 0)],
        forts: Vec::new(),
    };
    let late = DayBoard {
        players: vec![player(1, "alice", 0), player(2, "bob", 4)],
        owned: vec![(poland, 1)],
        forts: Vec::new(),
    };
    let days = vec![
        archived(10, Some(early), Vec::new()),
        archived(11, Some(late), Vec::new()),
    ];

    let alice_shade = palette_shades(0)[1];
    let bob_shade = palette_shades(4)[1];
    let (text, colors) = draw_days(&days, Some(10), 160, 44);
    // Days are dates on screen: the number is days since the epoch, which is
    // the right thing to compute with and no use to read.
    assert!(text.contains("as 11 Jan ended"), "{text:.200}");
    assert!(
        text.contains("history · 11 Jan · day 1 of 2"),
        "the strip says what this is and where you are: {text:.200}"
    );
    assert!(
        colors.contains(&Color::Rgb(alice_shade.0, alice_shade.1, alice_shade.2)),
        "day 10 is alice's Poland"
    );

    let (text, colors) = draw_days(&days, Some(11), 160, 44);
    assert!(text.contains("as 12 Jan ended"));
    assert!(text.contains("day 2 of 2"));
    assert!(
        colors.contains(&Color::Rgb(bob_shade.0, bob_shade.1, bob_shade.2)),
        "day 11 is bob's Poland"
    );
}

/// Days archived before boards were kept have logs and no positions. They
/// are skipped rather than drawn as an empty world, which would read as
/// "everybody lost everything that day".
#[test]
fn days_without_a_board_are_not_offered() {
    let days = vec![
        archived(
            1,
            None,
            vec![LogEntry::Won {
                user_id: Uuid::nil(),
                kind: WinKind::LastStanding,
            }],
        ),
        archived(
            2,
            Some(DayBoard {
                players: vec![player(1, "alice", 0)],
                owned: vec![(10, 0)],
                forts: Vec::new(),
            }),
            Vec::new(),
        ),
    ];
    let (text, _) = draw_days(&days, None, 160, 44);
    assert!(
        text.contains("day 1 of 1"),
        "only the day with a board: {text:.200}"
    );
    assert!(text.contains("as 3 Jan ended"));
}

#[test]
fn an_empty_archive_says_so_instead_of_drawing_nothing() {
    let (text, _) = draw_days(&[], None, 120, 30);
    assert!(text.contains("nothing to walk through yet"), "{text:.300}");
}

/// The slider is clamped, not wrapped: the first and last days of a game are
/// meaningful places, and walking off either end should stay put rather than
/// teleport to the other one.
#[test]
fn the_slider_stops_at_both_ends() {
    use crate::app::door::realm::state::history_step_index;

    // Three days, standing on the middle one.
    assert_eq!(history_step_index(1, 3, -1), 0);
    assert_eq!(history_step_index(1, 3, 1), 2);
    // Off the ends.
    assert_eq!(history_step_index(0, 3, -1), 0);
    assert_eq!(history_step_index(2, 3, 1), 2);
    // A single day has nowhere to go.
    assert_eq!(history_step_index(0, 1, 1), 0);
    assert_eq!(history_step_index(0, 1, -1), 0);
}

/// The strip is the whole point of the view: it has to lead with what this
/// is, not with a number, and it has to be the first thing on the map rather
/// than the last.
#[test]
fn the_strip_says_what_you_are_looking_at_before_it_says_when() {
    let board = DayBoard {
        players: vec![player(1, "alice", 0)],
        owned: vec![(10, 0)],
        forts: Vec::new(),
    };
    let days = vec![archived(20_000, Some(board), Vec::new())];
    let (text, _) = draw_days(&days, None, 160, 44);

    // Row-major buffer: the first row is the first 160 characters.
    let first_row: String = text.chars().take(160).collect();
    assert!(
        first_row.trim_start().starts_with("history ·"),
        "the top row should announce the view: {first_row:?}"
    );
    assert!(
        first_row.contains("day 1 of 1"),
        "and say where in the run you are: {first_row:?}"
    );
    // The map begins under it, not over it.
    let second_row: String = text.chars().skip(160).take(160).collect();
    assert!(second_row.contains('▀'), "{second_row:?}");
}

/// The view opens on the first day of the run, not the last. The last day's
/// board is all but the live map — the one thing this view must not be taken
/// for — while day one is five spawns on an empty world and could not be
/// mistaken for anything else.
#[test]
fn it_opens_on_the_first_day() {
    let board_of = |owned: Vec<(u16, u8)>| DayBoard {
        players: vec![player(1, "alice", 0), player(2, "bob", 4)],
        owned,
        forts: Vec::new(),
    };
    let days = vec![
        archived(20_000, Some(board_of(vec![(10, 0), (11, 1)])), Vec::new()),
        archived(
            20_001,
            Some(board_of(vec![(10, 0), (11, 1), (12, 0)])),
            Vec::new(),
        ),
        archived(
            20_002,
            Some(board_of(vec![(10, 0), (11, 0), (12, 0)])),
            Vec::new(),
        ),
    ];

    // No day picked: the strip lands on the first.
    let (text, _) = draw_days(&days, None, 160, 44);
    assert!(text.contains("day 1 of 3"), "{text:.200}");
    assert!(text.contains("as 4 Oct ended"), "{text:.200}");
}

/// A run that is only half read in must not present its oldest known day as
/// the first day of the game. That is what "day 1" said while the map showed
/// day 32 of 45: the archive pages fourteen at a time, and the first page is
/// the *newest* fourteen.
#[test]
fn a_half_read_run_does_not_claim_to_be_at_the_beginning() {
    let board = DayBoard {
        players: vec![player(1, "alice", 0)],
        owned: vec![(10, 0)],
        forts: Vec::new(),
    };
    let days = vec![
        archived(20_030, Some(board.clone()), Vec::new()),
        archived(20_031, Some(board), Vec::new()),
    ];

    let (loading_text, _) = draw_run(&days, None, 160, 44, true, None);
    assert!(
        loading_text.contains("reading the run in"),
        "{loading_text:.200}"
    );
    assert!(
        !loading_text.contains("day 1 of"),
        "a partial run must not call its oldest day the first: {loading_text:.200}"
    );

    // Once it is all in, it says where you are.
    let (settled_text, _) = draw_run(&days, None, 160, 44, false, None);
    assert!(settled_text.contains("day 1 of 2"), "{settled_text:.200}");
}

/// The slider is drawn between its own keys, so the view explains itself
/// without spending a line on it — and there is nothing else to drive here,
/// no pan and no zoom, so those two brackets are the whole interface.
#[test]
fn the_track_is_drawn_between_the_keys_that_move_it() {
    let board = DayBoard {
        players: vec![player(1, "alice", 0)],
        owned: vec![(10, 0)],
        forts: Vec::new(),
    };
    let days = vec![
        archived(20_000, Some(board.clone()), Vec::new()),
        archived(20_001, Some(board), Vec::new()),
    ];
    let (text, _) = draw_days(&days, None, 160, 44);
    // The top row is the strip and then the rail beside it, so the track is
    // the part between the first `[` and the `]` that closes it.
    let first_row: String = text.chars().take(160).collect();
    let (_, after) = first_row.split_once('[').expect("an opening key");
    let (track, _) = after.split_once(']').expect("a closing key");
    assert!(
        track.contains('▮') && track.contains('─'),
        "the run should sit between the keys that move it: {first_row:?}"
    );
}

fn earth_id(iso: &str) -> u16 {
    map_by_id("earth")
        .unwrap()
        .territories
        .iter()
        .find(|t| t.iso == iso)
        .unwrap_or_else(|| panic!("{iso} on earth"))
        .id
}

fn fight(target: u16, holder: Option<u128>, taker: u128, from: u16, took: bool) -> LogEntry {
    LogEntry::Fought {
        target,
        holder: holder.map(Uuid::from_u128),
        taken_by: took.then(|| Uuid::from_u128(taker)),
        defence: 1.0,
        takers: vec![TakerRoll {
            user_id: Uuid::from_u128(taker),
            points: 3,
            from,
            force: 2.0,
            probability: 0.42,
            shown: 0.42,
            success: took,
        }],
        bounced: false,
    }
}

/// A night plays back over the board it opened on, one fight a step: the
/// step's target is lit, the land the take came from is lit in the taker's
/// colour, and the caption says how it went. `p` opens it on the first
/// fight, still.
#[test]
fn a_replayed_night_lights_each_fight_in_turn() {
    let (canada, usa, brazil, russia) = (
        earth_id("CA"),
        earth_id("US"),
        earth_id("BR"),
        earth_id("RU"),
    );
    // alice (rose) holds Canada and bob (green) Brazil as the night opens;
    // alice takes the free United States, bob fails on Russia.
    let players = vec![player(1, "alice", 0), player(2, "bob", 4)];
    let open = DayBoard {
        players: players.clone(),
        owned: vec![(canada, 0), (brazil, 1)],
        forts: Vec::new(),
    };
    let closed = DayBoard {
        players,
        owned: vec![(canada, 0), (usa, 0), (brazil, 1)],
        forts: Vec::new(),
    };
    let mut night = archived(
        20_000,
        Some(closed),
        vec![
            fight(usa, None, 1, canada, true),
            fight(russia, None, 2, brazil, false),
        ],
    );
    night.board_open = Some(open);
    let days = vec![night];
    let lit = Color::Rgb(
        super::REPLAY_TARGET.0,
        super::REPLAY_TARGET.1,
        super::REPLAY_TARGET.2,
    );
    let launched = |slot: u8| {
        let (r, g, b) = super::lit(palette_shades(slot)[0]);
        Color::Rgb(r, g, b)
    };
    // The caption rides the map's top row, under the slider; the rail lists
    // every fight of the night on every step, so the step is read there.
    let caption = |text: &str| text.chars().skip(160).take(160).collect::<String>();

    // Not replaying: the closing board, nothing lit.
    let (text, colors) = draw_run(&days, None, 160, 44, false, None);
    assert!(!colors.contains(&lit));
    assert!(!colors.contains(&launched(0)));
    assert!(text.contains("as 4 Oct ended"), "{text:.300}");

    // The first still: the United States lit, Canada lit in alice's colour.
    let (text, colors) = draw_run(&days, None, 160, 44, false, Some(0));
    assert!(colors.contains(&lit), "the target is lit");
    assert!(
        colors.contains(&launched(0)),
        "the take came from alice's land"
    );
    assert!(
        !colors.contains(&launched(4)),
        "bob's fight is not this step"
    );
    let first = caption(&text);
    assert!(first.contains("night · fight 1 of 2"), "{first}");
    assert!(first.contains("United States"), "{first}");
    assert!(first.contains("taken"), "{first}");
    assert!(
        text.contains("as the night of 4 Oct began"),
        "the rail reads from the opening board: {text:.400}"
    );

    // Step two: Russia, from bob's Brazil, held.
    let (text, colors) = draw_run(&days, None, 160, 44, false, Some(1));
    assert!(colors.contains(&lit));
    assert!(
        colors.contains(&launched(4)),
        "the take came from bob's land"
    );
    assert!(!colors.contains(&launched(0)), "alice's fight is past");
    let second = caption(&text);
    assert!(second.contains("night · fight 2 of 2"), "{second}");
    assert!(second.contains("Russia"), "{second}");
    assert!(second.contains("held"), "{second}");

    // A step past the end holds on the last fight.
    let (text, _) = draw_run(&days, None, 160, 44, false, Some(9));
    assert!(
        caption(&text).contains("fight 2 of 2"),
        "{}",
        caption(&text)
    );
}
