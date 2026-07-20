use super::*;

fn occupant(n: u128, name: &str) -> Occupant {
    Occupant {
        user_id: Uuid::from_u128(n),
        username: name.to_string(),
    }
}

fn state_with_lobby(tutorial: bool) -> State {
    State::new(
        Some(SharedLobby::with_seed(7)),
        Uuid::from_u128(1),
        "me".to_string(),
        tutorial,
    )
}

#[test]
fn refresh_seats_the_crowd_and_mirrors_own_position() {
    let mut state = state_with_lobby(false);
    state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
    state.refresh_snapshot();
    assert_eq!(state.headcount(), 2);
    // Own cell mirrors the assigned seat, not the spawn mat.
    let own = state.snapshot.find(Uuid::from_u128(1)).unwrap();
    assert_eq!(own.placement.position(), (state.player_x, state.player_y));
}

#[test]
fn first_refresh_does_not_announce_the_whole_room() {
    let mut state = state_with_lobby(false);
    state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
    assert!(state.door_events.is_empty());

    state.refresh_roster(vec![
        occupant(1, "me"),
        occupant(2, "alice"),
        occupant(3, "bob"),
    ]);
    assert_eq!(state.door_events.len(), 1);
    assert!(state.door_events[0].arrived);
    assert_eq!(state.door_events[0].username, "bob");
    assert!(state.door_glow());
}

#[test]
fn departures_use_the_last_known_name() {
    let mut state = state_with_lobby(false);
    state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
    state.refresh_snapshot();
    state.refresh_roster(vec![occupant(1, "me")]);
    assert_eq!(state.door_events.len(), 1);
    assert!(!state.door_events[0].arrived);
    assert_eq!(state.door_events[0].username, "alice");
}

#[test]
fn door_events_expire_with_the_clock() {
    let mut state = state_with_lobby(false);
    state.refresh_roster(vec![occupant(1, "me")]);
    state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
    assert_eq!(state.door_events.len(), 1);
    for _ in 0..=DOOR_EVENT_TICKS {
        state.tick(true);
    }
    assert!(state.door_events.is_empty());
}

#[test]
fn walking_moves_and_respects_walls() {
    let mut state = state_with_lobby(false);
    state.refresh_roster(vec![occupant(1, "me")]);
    state.refresh_snapshot();
    for _ in 0..80 {
        state.walk(0, 1);
    }
    assert_eq!(state.player_y, map::MAP_H - 2);
    let before = (state.player_x, state.player_y);
    state.walk(0, 1);
    assert_eq!((state.player_x, state.player_y), before);
}

#[test]
fn tutorial_runs_welcome_to_done() {
    let mut state = state_with_lobby(true);
    assert_eq!(state.tutorial, Tutorial::Pending);
    state.enter_screen();
    assert_eq!(state.tutorial, Tutorial::Welcome);
    assert_eq!((state.player_x, state.player_y), map::SPAWN);

    state.walk(0, -1);
    assert_eq!(state.tutorial, Tutorial::GoToBar);

    // Not at the bar yet: no transition.
    assert!(!state.tutorial_reached_bar());

    // Teleport next to the counter (test-only shortcut via the lobby).
    state.player_x = 28;
    state.player_y = 12;
    assert!(state.tutorial_reached_bar());
    assert_eq!(state.tutorial, Tutorial::BarLesson);
    // Only fires once.
    assert!(!state.tutorial_reached_bar());

    assert!(!state.tutorial_advance());
    assert_eq!(state.tutorial, Tutorial::SendOff);
    assert!(state.tutorial_advance());
    assert_eq!(state.tutorial, Tutorial::Done);
}

const BARTENDER: u128 = 9;

fn lounge_msg(n: u128, author: u128, created: chrono::DateTime<chrono::Utc>) -> ChatMessage {
    ChatMessage {
        id: Uuid::from_u128(n),
        created,
        updated: created,
        pinned: false,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(99),
        user_id: Uuid::from_u128(author),
        body: format!("line {n}"),
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
        state.bartender_banner_message_id(),
        Some(Uuid::from_u128(1)),
        "the oldest answer of the burst shows first"
    );

    // The pinned line survives the dwell window even with lines waiting.
    for _ in 0..BANNER_QUEUE_DWELL_TICKS - 1 {
        state.tick(true);
        state.update_bartender_banner(bartender, &tail, now);
    }
    assert_eq!(
        state.bartender_banner_message_id(),
        Some(Uuid::from_u128(1))
    );

    state.tick(true);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(
        state.bartender_banner_message_id(),
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
    assert_eq!(
        state.bartender_banner_message_id(),
        Some(Uuid::from_u128(1))
    );

    for _ in 0..BANNER_FULL_TICKS - 1 {
        state.tick(true);
        state.update_bartender_banner(bartender, &tail, now);
    }
    assert_eq!(
        state.bartender_banner_message_id(),
        Some(Uuid::from_u128(1)),
        "nothing queued: the line keeps the full reading window"
    );

    state.tick(true);
    state.update_bartender_banner(bartender, &tail, now);
    assert_eq!(state.bartender_banner_message_id(), None);
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
    assert_eq!(state.bartender_banner_message_id(), None);

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
        state.bartender_banner_message_id(),
        Some(Uuid::from_u128(4)),
        "three oldest of eleven dropped, the fourth heads the banner"
    );
}

#[test]
fn returning_user_spawns_seated_not_at_the_door() {
    let mut state = state_with_lobby(false);
    state.enter_screen();
    state.refresh_roster(vec![occupant(1, "me")]);
    state.refresh_snapshot();
    let own = state.snapshot.find(Uuid::from_u128(1)).unwrap();
    assert!(matches!(
        own.placement,
        super::super::lobby::Placement::Seated(_)
    ));
}
