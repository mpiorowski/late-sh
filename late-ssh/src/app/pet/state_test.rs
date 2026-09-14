use std::time::{Duration, Instant};

use late_core::models::pet::{PetCompanion, PetMood, PetSpecies};
use late_core::test_utils::create_test_user;
use ratatui::layout::Rect;

use super::{
    ASLEEP_AFTER, Ambient, CHATTY_FOR, Look, MoodSignals, PROUD_FOR, PURR_FOR, Perch,
    PetFrameInputs, PetState, PetTick, PetTravel, SULK_FOR, mood_at,
};
use crate::app::pet::ui::{Neighbours, WatchSide};
use crate::test_helpers::new_test_db;

fn awake(at: Instant) -> Ambient {
    Ambient {
        last_input: at,
        music_playing: false,
    }
}

#[test]
fn the_reading_runs_in_precedence_and_every_window_closes() {
    let t0 = Instant::now();
    let signals = MoodSignals {
        petted: Some(t0),
        won: Some(t0),
        lost: Some(t0),
        spoke: Some(t0),
    };
    // Everything at once: the click wins, then each window closes in turn.
    assert_eq!(mood_at(signals, awake(t0), t0), PetMood::Purring);
    assert_eq!(mood_at(signals, awake(t0), t0 + PURR_FOR), PetMood::Proud);
    // The pet is an optimist: the loss is forgotten before the win is.
    assert!(SULK_FOR < PROUD_FOR);
    assert_eq!(
        mood_at(
            MoodSignals {
                won: None,
                ..signals
            },
            awake(t0),
            t0 + PURR_FOR
        ),
        PetMood::Sulking
    );
    // The sulk and the chat windows are the same length, so a message
    // sent mid-sulk is what shows once the sulk ends.
    assert_eq!(
        mood_at(
            MoodSignals {
                won: None,
                spoke: Some(t0 + SULK_FOR / 2),
                ..signals
            },
            awake(t0 + SULK_FOR),
            t0 + SULK_FOR
        ),
        PetMood::Chatty
    );
    // Quiet, keys still moving: awake. Music on: vibing. Keys gone: asleep,
    // radio or not.
    let quiet = t0 + PROUD_FOR.max(CHATTY_FOR);
    assert_eq!(mood_at(signals, awake(quiet), quiet), PetMood::Idle);
    assert_eq!(
        mood_at(
            signals,
            Ambient {
                last_input: quiet,
                music_playing: true
            },
            quiet
        ),
        PetMood::Vibing
    );
    assert_eq!(
        mood_at(
            signals,
            Ambient {
                last_input: quiet,
                music_playing: true
            },
            quiet + ASLEEP_AFTER
        ),
        PetMood::Asleep
    );
    // A blank slate is simply awake.
    assert_eq!(
        mood_at(MoodSignals::default(), awake(t0), t0),
        PetMood::Idle
    );
}

async fn fresh_state(handle: &str) -> PetState {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, handle).await;
    let svc = super::super::svc::PetService::new(test_db.db.clone());
    let pet = svc.ensure_pet(user.id).await.expect("ensure pet");
    PetState::new(user.id, svc, pet)
}

fn frame(position: (usize, usize)) -> PetFrameInputs {
    PetFrameInputs {
        travel: PetTravel { x: 40, y: 5 },
        zone: Rect::new(10, 20, 48, 8),
        neighbours: Neighbours::default(),
        position,
    }
}

fn tick(
    wall_tick: usize,
    now: Instant,
    frame: Option<PetFrameInputs>,
    cursor: Option<(u16, u16)>,
) -> PetTick {
    PetTick {
        wall_tick,
        now,
        ambient: awake(now),
        frame,
        cursor,
        persist: false,
    }
}

#[tokio::test]
async fn the_pet_walks_after_the_cursor_and_lets_go_when_it_leaves() {
    let mut state = fresh_state("pet-follows").await;
    assert_eq!(state.species, PetSpecies::Cat);
    let now = Instant::now();
    // The first tick wakes the stored (asleep) pet; after that a quiet
    // tick has nothing to report.
    assert!(state.tick(tick(0, now, None, None)), "the wake-up");
    assert!(!state.tick(tick(0, now, None, None)), "nothing to report");
    assert_eq!(state.perch(), None);

    // The cursor lands far to the right inside the box: the pet turns and
    // walks a cell per animation edge from where it stood.
    let far_right = Some((10 + 3 + 30, 20 + 1 + 4));
    assert!(state.tick(tick(2, now, Some(frame((0, 0))), far_right)));
    assert_eq!(
        state.perch(),
        Some(Perch {
            x: 1,
            y: 1,
            look: Look::Right
        })
    );
    // A sparse tick covers the same ground as the edges it skipped.
    assert!(state.tick(tick(12, now, Some(frame((1, 1))), far_right)));
    assert_eq!(
        state.perch(),
        Some(Perch {
            x: 6,
            y: 4,
            look: Look::Right
        })
    );
    // Arrived: face under the cursor, looking up at it, and it stays put.
    let mut wall = 12;
    for _ in 0..40 {
        wall += 2;
        let at = state.perch().map(|perch| (perch.x, perch.y)).unwrap();
        state.tick(tick(wall, now, Some(frame(at)), far_right));
    }
    assert_eq!(
        state.perch(),
        Some(Perch {
            x: 30,
            y: 4,
            look: Look::Ahead
        })
    );
    assert!(
        !state.tick(tick(wall + 2, now, Some(frame((30, 4))), far_right)),
        "a parked pet reports nothing"
    );

    // The cursor leaves the box: the stroll takes over again.
    assert!(state.tick(tick(wall + 4, now, Some(frame((30, 4))), Some((0, 0)))));
    assert_eq!(state.perch(), None);
}

#[tokio::test]
async fn a_sulking_or_sleeping_pet_does_not_come_and_a_petted_one_is_not_pinned() {
    let mut state = fresh_state("pet-sulks").await;
    let t0 = Instant::now();
    let inside = Some((10 + 20, 20 + 3));

    state.note_loss(t0);
    state.tick(tick(2, t0, Some(frame((5, 5))), inside));
    assert_eq!(state.mood(), PetMood::Sulking);
    assert_eq!(state.perch(), None, "sulking: not coming");

    let asleep = PetTick {
        ambient: Ambient {
            last_input: t0,
            music_playing: false,
        },
        ..tick(4, t0 + SULK_FOR + ASLEEP_AFTER, Some(frame((5, 5))), inside)
    };
    state.tick(asleep);
    assert_eq!(state.mood(), PetMood::Asleep);
    assert_eq!(state.perch(), None, "asleep: not coming");

    // Petted (the click is a report inside the box), then the cursor
    // leaves: it purrs, follows while the cursor is there, and strolls the
    // moment it is gone. Nothing pins a purring pet.
    let later = t0 + SULK_FOR + ASLEEP_AFTER + Duration::from_secs(1);
    state.note_petted(later);
    assert!(state.tick(tick(6, later, Some(frame((7, 2))), inside)));
    assert_eq!(state.mood(), PetMood::Purring);
    assert!(state.perch().is_some(), "following the click");
    assert!(state.tick(tick(8, later, Some(frame((7, 2))), Some((0, 0)))));
    assert_eq!(state.perch(), None, "cursor gone: back on the stroll");
    // The purr ends on its own.
    assert!(state.tick(tick(10, later + PURR_FOR, Some(frame((7, 2))), None)));
    assert_eq!(state.mood(), PetMood::Idle);
}

#[tokio::test]
async fn a_tank_beside_the_box_rides_the_frame_inputs() {
    // The frame inputs carry the neighbour through unchanged: the pose is
    // the box's business, the tick only needs the zone and the position.
    let mut state = fresh_state("pet-frame").await;
    let now = Instant::now();
    let beside = PetFrameInputs {
        neighbours: Neighbours {
            tank: Some(WatchSide::Right),
            bonsai: None,
        },
        ..frame((3, 3))
    };
    state.tick(tick(2, now, Some(beside), None));
    assert_eq!(state.perch(), None);
}

#[tokio::test]
async fn an_owner_coming_back_wakes_the_stored_sleeping_pet_on_the_profile() {
    // The last session wrote `asleep` on its way out. A new session that
    // is active but quiet (no chat, win, loss, music, or petting) reads
    // idle, and that has to reach the row: otherwise the profile shows a
    // sleeping pet for an owner who is right there.
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-wakes").await;
    let svc = super::super::svc::PetService::new(test_db.db.clone());
    let stored = svc.ensure_pet(user.id).await.expect("ensure pet");
    assert_eq!(stored.mood(), PetMood::Asleep);
    let mut state = PetState::new(user.id, svc, stored);

    let now = Instant::now();
    let awake_and_quiet = PetTick {
        persist: true,
        ..tick(0, now, None, None)
    };
    assert!(state.tick(awake_and_quiet), "waking up is a change");
    assert_eq!(state.mood(), PetMood::Idle);

    let db = test_db.db.clone();
    crate::test_helpers::wait_until(
        || async {
            let client = db.get().await.expect("db client");
            PetCompanion::ensure(&client, user.id)
                .await
                .expect("reload")
                .mood()
                == PetMood::Idle
        },
        "the row says idle",
    )
    .await;
}
