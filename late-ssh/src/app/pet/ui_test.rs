use std::cell::Cell;
use std::time::Instant;

use late_core::models::pet::{PetMood, PetSpecies};
use late_core::test_utils::create_test_user;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::{
    PET_BOX_MIN_ROWS, PetPose, PetView, STROLL_TICKS, WATCH_TICKS, WatchSide, draw_pet_box,
    frame_changed,
};
use crate::app::pet::state::{
    Ambient, Look, PET_WIDTH, Perch, PetFrameInputs, PetState, PetTick, PetTravel,
};
use crate::test_helpers::new_test_db;

const WIDE: PetTravel = PetTravel { x: 40, y: 0 };

#[test]
fn a_sleeping_pet_never_pays_a_frame() {
    // Asleep: curled on the floor, no blink, a limp tail: the box is fully
    // static, so no tick may report a change.
    for tick in 0..500 {
        assert!(!frame_changed(PetMood::Asleep, None, None, tick, WIDE));
    }
}

#[test]
fn an_awake_pet_changes_on_blink_edges_and_skips_still_ticks() {
    // Blink turns on at tick % 64 == 0 and off at tick % 64 == 3; both edges
    // repaint regardless of where the stroll is.
    assert!(frame_changed(PetMood::Idle, None, None, 64, WIDE));
    assert!(frame_changed(PetMood::Idle, None, None, 67, WIDE));

    // The gate only pays for ticks where the art moves: across a whole blink
    // period a strolling pet must have both changed and clean ticks.
    let changed_ticks = (1..=64)
        .filter(|&tick| frame_changed(PetMood::Idle, None, None, tick, WIDE))
        .count();
    assert!(changed_ticks > 0, "an awake pet animates");
    assert!(changed_ticks < 64, "an awake pet still has clean ticks");

    // Sulking: parked, but awake, so the blink edges and one slow tail
    // flick still repaint and nothing else does.
    let sulk_ticks = (1..=64)
        .filter(|&tick| frame_changed(PetMood::Sulking, None, None, tick, WIDE))
        .count();
    assert_eq!(
        sulk_ticks, 4,
        "a sulking pet only blinks and flicks its tail"
    );
}

#[test]
fn zero_travel_still_blinks() {
    // A box too small to roam still has blink and tail edges.
    assert!(frame_changed(
        PetMood::Idle,
        None,
        None,
        64,
        PetTravel { x: 0, y: 0 }
    ));
}

#[test]
fn a_calm_pet_beside_the_tank_strolls_twenty_minutes_and_watches_five() {
    let glass = Some(WatchSide::Right);
    let pose = |tick| PetPose::for_frame(PetMood::Idle, glass, None, tick);
    assert_eq!(pose(0), PetPose::Stroll);
    assert_eq!(pose(STROLL_TICKS - 1), PetPose::Stroll);
    assert_eq!(pose(STROLL_TICKS), PetPose::Watch(WatchSide::Right));
    assert_eq!(
        pose(STROLL_TICKS + WATCH_TICKS - 1),
        PetPose::Watch(WatchSide::Right)
    );
    assert_eq!(
        pose(STROLL_TICKS + WATCH_TICKS),
        PetPose::Stroll,
        "and round again"
    );
    // Roughly the minutes on the label, at the 66ms wall tick.
    assert_eq!((STROLL_TICKS * 66 + 30_000) / 60_000, 20);
    assert_eq!((WATCH_TICKS * 66 + 30_000) / 60_000, 5);
    // Vibing and chatty watch too; a wound up pet paces instead, a sulking
    // one sulks, a sleeping one sleeps, and no tank means no watching at
    // all.
    assert_eq!(
        PetPose::for_frame(PetMood::Vibing, glass, None, STROLL_TICKS),
        PetPose::Watch(WatchSide::Right)
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Chatty, glass, None, STROLL_TICKS),
        PetPose::Watch(WatchSide::Right)
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Chatty, glass, None, STROLL_TICKS - 1),
        PetPose::Stroll,
        "and it keeps the same twenty/five cycle"
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Proud, glass, None, STROLL_TICKS),
        PetPose::Stroll
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Purring, glass, None, STROLL_TICKS),
        PetPose::Stroll
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Sulking, glass, None, STROLL_TICKS),
        PetPose::Sulk
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Asleep, glass, None, STROLL_TICKS),
        PetPose::Sleep
    );
    assert_eq!(
        PetPose::for_frame(PetMood::Idle, None, None, STROLL_TICKS),
        PetPose::Stroll
    );
    // A perch overrides everything: the pet is where the state put it.
    let perch = Perch {
        x: 4,
        y: 0,
        look: Look::Left,
    };
    assert_eq!(
        PetPose::for_frame(PetMood::Idle, glass, Some(perch), STROLL_TICKS),
        PetPose::At(perch)
    );
}

#[test]
fn a_watching_pet_holds_still_but_still_blinks_and_gasps() {
    let glass = Some(WatchSide::Right);
    let start = STROLL_TICKS;
    // The walk to the glass repaints, then blink and gasp edges do.
    assert!(frame_changed(PetMood::Idle, glass, None, start, WIDE));
    let first_blink = (start..).find(|t| t % 64 == 0).unwrap();
    assert!(frame_changed(PetMood::Idle, glass, None, first_blink, WIDE));
    let first_gasp = (start + 1..).find(|t| t % 96 == 0).unwrap();
    assert!(frame_changed(PetMood::Idle, glass, None, first_gasp, WIDE));
    assert!(
        frame_changed(PetMood::Idle, glass, None, first_gasp + 6, WIDE),
        "the gasp ends after six ticks"
    );
    // No stroll steps: far fewer paid frames than a strolling pet.
    let window = start + 1..=start + 192;
    let watching = window
        .clone()
        .filter(|&t| frame_changed(PetMood::Idle, glass, None, t, WIDE))
        .count();
    let strolling = window
        .filter(|&t| frame_changed(PetMood::Idle, None, None, t, WIDE))
        .count();
    assert!(
        watching > 0 && watching < strolling,
        "{watching} vs {strolling}"
    );
}

/// Draw the box at `tick` and report where the pet landed, plus the frame
/// inputs the draw recorded for the tick.
fn draw_at(
    state: &mut PetState,
    tick: usize,
    area: Rect,
    watching: Option<WatchSide>,
) -> (Rect, PetFrameInputs) {
    let now = Instant::now();
    state.tick(PetTick {
        wall_tick: tick,
        now,
        ambient: Ambient {
            last_input: now,
            music_playing: false,
        },
        frame: None,
        cursor: None,
        persist: false,
    });
    let pet_rect = Cell::new(None);
    let frame_slot = Cell::new(None);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            draw_pet_box(
                frame,
                area,
                &PetView {
                    state,
                    pet_rect_slot: Some(&pet_rect),
                    frame_slot: Some(&frame_slot),
                },
                watching,
            );
        })
        .unwrap();
    (
        pet_rect.get().expect("the box drew the pet"),
        frame_slot.get().expect("the box recorded its frame"),
    )
}

#[tokio::test]
async fn an_awake_pet_roams_the_whole_box_and_a_watching_one_sits_at_the_glass() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-roams-box").await;
    let svc = crate::app::pet::svc::PetService::new(test_db.db.clone());
    let pet = svc.ensure_pet(user.id).await.expect("ensure pet");
    let mut state = PetState::new(user.id, svc, pet);
    let area = Rect::new(0, 0, 60, 12);
    let floor_y = area.bottom() - PET_BOX_MIN_ROWS;

    // Over a long stroll the pet reaches the top and the far right of its
    // box, and comes back down to the floor; nothing pins it anywhere. The
    // frame it records is the zone it roamed and where it stood.
    let mut top = u16::MAX;
    let mut bottom = 0;
    let mut right = 0;
    for tick in (0..6000).step_by(7) {
        let (rect, frame) = draw_at(&mut state, tick, area, None);
        top = top.min(rect.y);
        bottom = bottom.max(rect.y);
        right = right.max(rect.right());
        assert!(
            rect.bottom() <= area.bottom(),
            "the pet stays inside its box"
        );
        assert_eq!(frame.zone, area);
        assert_eq!(
            frame.position,
            (usize::from(rect.x), usize::from(rect.y)),
            "the recorded position is where it drew"
        );
        assert_eq!(
            frame.travel,
            PetTravel {
                x: usize::from(area.width) - PET_WIDTH,
                y: usize::from(floor_y)
            }
        );
    }
    assert_eq!(top, area.y, "the stroll reaches the top of the box");
    assert_eq!(bottom, floor_y, "the stroll comes back to the floor");
    assert!(
        right > area.width / 2,
        "the stroll crosses into the right half (reached {right})"
    );

    // With a tank against the right edge the calm pet sits on the floor at
    // the edge and stays there, whatever the tick.
    let (at_glass, _) = draw_at(&mut state, STROLL_TICKS + 7, area, Some(WatchSide::Right));
    assert_eq!(at_glass.y, floor_y, "watching from the floor");
    assert_eq!(
        at_glass,
        draw_at(&mut state, STROLL_TICKS + 707, area, Some(WatchSide::Right)).0,
        "a watching pet does not wander"
    );
    assert_eq!(
        at_glass.right(),
        area.right(),
        "pressed against the tank's edge"
    );
}

#[tokio::test]
async fn every_species_draws_inside_the_same_eight_columns() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-species-art").await;
    let svc = crate::app::pet::svc::PetService::new(test_db.db.clone());
    let pet = svc.ensure_pet(user.id).await.expect("ensure pet");
    let mut state = PetState::new(user.id, svc, pet);
    let area = Rect::new(0, 0, 20, 3);
    for species in PetSpecies::ALL {
        state.species = species;
        let (rect, _) = draw_at(&mut state, 3, area, None);
        assert_eq!(rect.width, PET_WIDTH as u16, "{species:?}");
        assert_eq!(rect.height, PET_BOX_MIN_ROWS, "{species:?}");
    }
}
