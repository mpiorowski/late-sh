use std::sync::Arc;

use late_core::models::presence::{ClubhouseStand, PresenceRecord, Spot};

use super::*;
use crate::app::clubhouse::crowd::Placement;

const NOW: i64 = 1_000_000;
const ME: u128 = 1;

fn other(n: u128, spot: Spot, since_ms: i64) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::from_u128(1_000 + n),
        user_id: Uuid::from_u128(n),
        username: format!("user{n:03}"),
        clubhouse: ClubhouseStand {
            spot,
            since_ms,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: None,
        street: None,
    }
}

/// A session logging in as `ME` while `records` are already on the wire.
fn session(records: Vec<PresenceRecord>, tutorial: bool) -> State {
    State::new(
        Uuid::from_u128(1_000 + ME),
        Uuid::from_u128(ME),
        "me".to_string(),
        tutorial,
        Arc::new(records),
        DrunkMap::new(),
        NOW,
    )
}

fn state_with_lobby(tutorial: bool) -> State {
    session(Vec::new(), tutorial)
}

fn placement(state: &State, n: u128) -> Option<Placement> {
    state.crowd.find(Uuid::from_u128(n)).map(|p| p.placement)
}

#[test]
fn a_session_sits_down_the_moment_it_logs_in() {
    let state = session(vec![other(2, Spot::Walking { x: 30, y: 12 }, 0)], false);
    assert_eq!(state.headcount(), 2);
    assert!(
        matches!(placement(&state, ME), Some(Placement::Seated(_))),
        "a returning user spawns seated, not at the door"
    );
    // Own cell mirrors the picked seat, not the spawn mat.
    let own = placement(&state, ME).unwrap();
    assert_eq!(own.position(), (state.player_x, state.player_y));
    assert!(matches!(state.stand().spot, Spot::Seat { .. }));
}

#[test]
fn the_room_at_login_is_not_announced_and_later_arrivals_are() {
    let alice = other(2, Spot::Seat { index: 3 }, 0);
    let mut state = session(vec![alice.clone()], false);
    assert!(state.door_events.is_empty());

    let bob = other(3, Spot::Seat { index: 5 }, 0);
    state.set_records(Arc::new(vec![alice, bob]), NOW);
    assert_eq!(state.door_events.len(), 1);
    assert!(state.door_events[0].arrived);
    assert_eq!(state.door_events[0].username, "user003");
    assert!(state.door_glow());
}

#[test]
fn departures_use_the_last_known_name() {
    let mut state = session(vec![other(2, Spot::Seat { index: 3 }, 0)], false);
    state.set_records(Arc::new(Vec::new()), NOW);
    assert_eq!(state.door_events.len(), 1);
    assert!(!state.door_events[0].arrived);
    assert_eq!(state.door_events[0].username, "user002");
}

#[test]
fn door_events_expire_with_the_clock() {
    let mut state = session(Vec::new(), false);
    state.set_records(Arc::new(vec![other(2, Spot::Seat { index: 3 }, 0)]), NOW);
    assert_eq!(state.door_events.len(), 1);
    // The clock is wall-driven: a sparse tick jumps straight to the wall
    // tick it is given, and expiry follows wall time, not call count.
    state.tick(DOOR_EVENT_TICKS - 1);
    assert_eq!(state.door_events.len(), 1);
    state.tick(DOOR_EVENT_TICKS);
    assert!(state.door_events.is_empty());
}

#[test]
fn walking_moves_and_respects_walls() {
    let mut state = session(Vec::new(), true);
    state.enter_screen(NOW);
    assert_eq!((state.player_x, state.player_y), map::SPAWN);
    for _ in 0..80 {
        state.walk(0, -1, NOW);
    }
    let blocked = (state.player_x, state.player_y);
    assert!(blocked.1 < map::SPAWN.1, "never moved off the mat");
    assert!(!map::walkable(blocked.0, blocked.1 - 1));
    state.walk(0, -1, NOW);
    assert_eq!((state.player_x, state.player_y), blocked);
    assert_eq!(
        state.stand().spot,
        Spot::Walking {
            x: blocked.0,
            y: blocked.1
        },
        "the room sees the walker where they stopped"
    );
}

#[test]
fn a_seat_lost_to_an_earlier_claim_is_picked_again() {
    let mut state = session(Vec::new(), false);
    let Spot::Seat { index } = state.stand().spot else {
        panic!("seated at login");
    };
    // Someone on another replica took the same seat a moment earlier.
    let alice = other(2, Spot::Seat { index }, NOW - 1);

    state.set_records(Arc::new(vec![alice]), NOW + 10);

    assert_eq!(placement(&state, 2), Some(Placement::Seated(usize::from(index))));
    assert!(
        matches!(state.stand().spot, Spot::Seat { index: mine } if mine != index),
        "picked another seat: {:?}",
        state.stand().spot
    );
    assert!(matches!(placement(&state, ME), Some(Placement::Seated(_))));
}

#[test]
fn a_full_house_waits_at_the_door_and_takes_the_first_seat_that_frees() {
    let mut full: Vec<PresenceRecord> = (0..map::SEATS.len())
        .map(|i| other(10 + i as u128, Spot::Seat { index: i as u16 }, 0))
        .chain((0..map::STANDING_SPOTS.len()).map(|i| {
            other(100 + i as u128, Spot::Standing { index: i as u16 }, 0)
        }))
        .collect();
    let mut state = session(full.clone(), false);
    assert_eq!(state.stand().spot, Spot::Door);
    assert!(matches!(placement(&state, ME), Some(Placement::Door(_))));

    let freed = full.remove(4).clubhouse.spot;
    state.set_records(Arc::new(full), NOW + 10);

    assert_eq!(state.stand().spot, freed);
    assert_eq!(placement(&state, ME), Some(Placement::Seated(4)));
}

#[test]
fn a_second_device_joins_the_first_and_follows_its_moves() {
    let mut laptop = other(ME, Spot::Seat { index: 7 }, NOW - 5_000);
    laptop.session_id = Uuid::from_u128(77);
    let mut phone = session(vec![laptop.clone()], false);
    assert_eq!(phone.stand().spot, Spot::Seat { index: 7 });

    laptop.clubhouse.spot = Spot::Walking { x: 30, y: 12 };
    laptop.clubhouse.since_ms = NOW + 100;
    phone.set_records(Arc::new(vec![laptop]), NOW + 200);

    assert_eq!(phone.stand().spot, Spot::Walking { x: 30, y: 12 });
    assert_eq!((phone.player_x, phone.player_y), (30, 12));
}

#[test]
fn tutorial_tours_every_page_then_comes_home() {
    let mut state = state_with_lobby(true);
    assert_eq!(state.tutorial, Tutorial::Pending);
    assert_eq!(state.tutorial_forced_step(), None);
    state.enter_screen(NOW);
    assert_eq!(state.tutorial, Tutorial::Welcome);
    assert_eq!((state.player_x, state.player_y), map::SPAWN);

    // Every stop forces exactly one input: a page digit whose screen
    // advances the route, Enter on the two mid-route interlude boxes (the
    // music on Home, the lobby on The Arcade), or the Ctrl+F chord into Zen.
    for (step, next_stage) in [
        (TourStep::Page(b'1', Screen::Dashboard), Tutorial::VisitChat),
        (TourStep::Enter, Tutorial::VisitMusic),
        (TourStep::Page(b'2', Screen::Arcade), Tutorial::VisitArcade),
        (TourStep::Enter, Tutorial::VisitLobby),
        (TourStep::Page(b'3', Screen::Games), Tutorial::VisitGames),
        (
            TourStep::Page(b'4', Screen::Artboard),
            Tutorial::VisitArtboard,
        ),
        (
            TourStep::Page(b'5', Screen::Profiles),
            Tutorial::VisitDirectory,
        ),
        (
            TourStep::Page(b'6', Screen::Leaderboard),
            Tutorial::VisitLeaderboard,
        ),
        (TourStep::Zen, Tutorial::VisitZen),
        (
            TourStep::Page(b'0', Screen::Clubhouse),
            Tutorial::Homecoming,
        ),
    ] {
        assert_eq!(state.tutorial_forced_step(), Some(step));
        match step {
            TourStep::Page(_, screen) => {
                // A wrong page never advances a stop; the route waits for
                // its page.
                let wrong = if screen == Screen::Games {
                    Screen::Artboard
                } else {
                    Screen::Games
                };
                state.tutorial_screen_entered(wrong);
                state.tutorial_screen_entered(screen);
            }
            TourStep::Zen => {
                // The tavern is not the way into Zen: the stop waits for it.
                state.tutorial_screen_entered(Screen::Clubhouse);
                state.tutorial_screen_entered(Screen::Zen);
            }
            // The interludes advance without finishing the tour.
            TourStep::Enter => assert!(!state.tutorial_advance()),
        }
        assert_eq!(state.tutorial, next_stage);
    }

    // The homecoming box forces Enter, and it finishes the tour.
    assert_eq!(state.tutorial_forced_step(), Some(TourStep::Enter));
    assert!(state.tutorial_advance());
    assert_eq!(state.tutorial, Tutorial::Done);
    assert_eq!(state.tutorial_forced_step(), None);
}

#[test]
fn bar_glows_after_homecoming_until_the_pour_is_claimed() {
    let mut state = state_with_lobby(true);
    state.enter_screen(NOW);
    // Mid-tour: nothing pours at a distance, and the bar does not glow yet.
    assert!(!state.welcome_pour_due());
    assert!(!state.bar_glow());
    state.tutorial_screen_entered(Screen::Dashboard);
    assert!(!state.tutorial_advance()); // the music interlude
    state.tutorial_screen_entered(Screen::Arcade);
    assert!(!state.tutorial_advance()); // the lobby interlude
    for screen in [
        Screen::Games,
        Screen::Artboard,
        Screen::Profiles,
        Screen::Leaderboard,
        Screen::Zen,
        Screen::Clubhouse,
    ] {
        state.tutorial_screen_entered(screen);
    }
    assert_eq!(state.tutorial, Tutorial::Homecoming);
    assert!(state.bar_glow());
    assert!(state.tutorial_advance());
    // Done, pour unclaimed: the glow keeps pointing at the treasure.
    assert!(state.bar_glow());

    // Teleport to the edge of the bar's approach apron (test-only
    // shortcut): three rows off the counter is close enough to pour.
    state.player_x = 28;
    state.player_y = 15;
    assert!(state.welcome_pour_due());
    // Only fires once per session, and claiming kills the glow.
    assert!(!state.welcome_pour_due());
    assert!(!state.bar_glow());
}

#[test]
fn returning_users_never_pour_or_glow() {
    let mut state = state_with_lobby(false);
    state.enter_screen(NOW);
    assert_eq!(state.tutorial, Tutorial::Off);
    state.player_x = 28;
    state.player_y = 12;
    assert!(!state.welcome_pour_due());
    assert!(!state.bar_glow());
}

const BARTENDER: u128 = 9;

fn lounge_msg(n: u128, author: u128, created: chrono::DateTime<chrono::Utc>) -> ChatMessage {
    ChatMessage {
        id: Uuid::from_u128(n),
        created,
        updated: created,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(99),
        user_id: Uuid::from_u128(author),
        body: format!("line {n}"),
    }
}

/// The #lounge message currently in the banner. These tests only feed real
/// lounge lines, so a local line surfacing here is a bug in the code under test.
fn banner_id(state: &State) -> Option<Uuid> {
    match state.bartender_banner_line() {
        None => None,
        Some(BannerLine::Lounge(id)) => Some(*id),
        Some(BannerLine::Local(line)) => panic!("unexpected local banner line: {line}"),
    }
}

#[test]
fn bartender_banner_queues_a_burst_and_plays_it_in_order() {
    let mut state = state_with_lobby(false);
    let now = chrono::Utc::now();
    let bartender = Some(Uuid::from_u128(BARTENDER));
    // Newest-first tail: three answers in a burst, a patron line mixed in.
    let tail = vec![
        lounge_msg(3, BARTENDER, now),
        lounge_msg(4, 2, now - chrono::Duration::milliseconds(500)),
        lounge_msg(2, BARTENDER, now - chrono::Duration::seconds(1)),
        lounge_msg(1, BARTENDER, now - chrono::Duration::seconds(2)),
    ];
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(
        banner_id(&state),
        Some(Uuid::from_u128(1)),
        "the oldest answer of the burst shows first"
    );

    // The pinned line survives the dwell window even with lines waiting.
    state.tick(BANNER_QUEUE_DWELL_TICKS - 1);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(banner_id(&state), Some(Uuid::from_u128(1)));

    state.tick(BANNER_QUEUE_DWELL_TICKS);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(
        banner_id(&state),
        Some(Uuid::from_u128(2)),
        "dwell elapsed with a queue waiting: next answer takes the banner"
    );
}

#[test]
fn bartender_banner_holds_a_lone_line_for_the_full_window_then_clears() {
    let mut state = state_with_lobby(false);
    let now = chrono::Utc::now();
    let bartender = Some(Uuid::from_u128(BARTENDER));
    let tail = vec![lounge_msg(1, BARTENDER, now)];
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(banner_id(&state), Some(Uuid::from_u128(1)));

    state.tick(BANNER_FULL_TICKS - 1);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(
        banner_id(&state),
        Some(Uuid::from_u128(1)),
        "nothing queued: the line keeps the full reading window"
    );

    state.tick(BANNER_FULL_TICKS);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(banner_id(&state), None);
}

#[test]
fn bartender_banner_skips_stale_backlog_and_caps_the_queue() {
    let mut state = state_with_lobby(false);
    let now = chrono::Utc::now();
    let bartender = Some(Uuid::from_u128(BARTENDER));
    // A line from before the screen was open never enqueues.
    let stale = vec![lounge_msg(
        1,
        BARTENDER,
        now - chrono::Duration::seconds(60),
    )];
    state.update_bartender_banner(bartender, &stale, now);
    assert_eq!(banner_id(&state), None);

    // A flood wider than the cap drops the oldest answers.
    let mut state = state_with_lobby(false);
    let flood: Vec<ChatMessage> = (1..=BANNER_QUEUE_MAX as u128 + 3)
        .rev()
        .map(|n| {
            lounge_msg(
                n,
                BARTENDER,
                now - chrono::Duration::milliseconds(100 - n as i64),
            )
        })
        .collect();
    state.update_bartender_banner(bartender, &flood, now);
    assert_eq!(
        banner_id(&state),
        Some(Uuid::from_u128(4)),
        "three oldest of eleven dropped, the fourth heads the banner"
    );
}

#[test]
fn tutorial_welcome_takes_the_banner_ahead_of_a_queued_answer() {
    let mut state = state_with_lobby(true);
    let now = chrono::Utc::now();
    let bartender = Some(Uuid::from_u128(BARTENDER));
    let tail = vec![
        lounge_msg(2, BARTENDER, now),
        lounge_msg(1, BARTENDER, now - chrono::Duration::seconds(1)),
    ];
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(banner_id(&state), Some(Uuid::from_u128(1)));

    state.show_local_bartender_line("@me first one's on the house.".to_string());
    assert_eq!(
        state.bartender_banner_line(),
        Some(&BannerLine::Local(
            "@me first one's on the house.".to_string()
        )),
        "the welcome cuts the queue: it is why the newcomer walked to the bar"
    );

    // It holds its own dwell, then the queued answer resumes as usual.
    state.tick(1);
    state.update_bartender_banner(bartender, &tail, now);
    assert!(matches!(
        state.bartender_banner_line(),
        Some(BannerLine::Local(_))
    ));
    state.tick(BANNER_QUEUE_DWELL_TICKS);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(banner_id(&state), Some(Uuid::from_u128(2)));
}

#[test]
fn heading_out_back_puts_the_avatar_at_the_back_door_for_everyone() {
    let mut state = session(vec![other(2, Spot::Seat { index: 3 }, 0)], false);
    assert_ne!(state.nearby(), Some(map::Interactive::BackDoor));

    state.step_to_back_door(NOW);

    assert_eq!((state.player_x, state.player_y), map::BACK_DOOR_MAT);
    assert_eq!(state.nearby(), Some(map::Interactive::BackDoor));
    // The room sees it too: the stand this session publishes is at the
    // back door.
    let (x, y) = map::BACK_DOOR_MAT;
    assert_eq!(state.stand().spot, Spot::Walking { x, y });
}
