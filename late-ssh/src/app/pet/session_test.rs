use std::time::{Duration, Instant};

use late_core::models::pet::{
    Fed, Form, PetCompanion, PetMood, PetSpecies, Pick, Stage, TreatKind, Trick, VisitKind,
};
use late_core::test_utils::{TestDb, create_test_user};
use ratatui::layout::Rect;
use uuid::Uuid;

use super::{
    ASLEEP_AFTER, Ambient, CHATTY_FOR, Clicked, Look, Moment, MoodSignals, PROUD_FOR, PURR_FOR,
    Perch, PetFrameInputs, PetState, PetTick, PetTravel, SULK_FOR, Toy, mood_at,
};
use crate::app::activity::event::GameFamily;
use crate::app::common::primitives::Screen;
use crate::app::pet::state::Need;
use crate::app::pet::svc::{PetCommand, PetOutcome, PetService};
use crate::app::pet::ui::{Neighbours, WatchSide};
use crate::test_helpers::new_test_db;

/// A grown pet, drawn with the second top and the first feet.
const ADULT: Form = Form::Adult {
    top: Pick::Two,
    feet: Pick::One,
};

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
    let svc = super::super::svc::PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let pet = svc.bootstrap(user.id).await.expect("bootstrap");
    PetState::new(
        user.id,
        svc,
        Some(crate::test_helpers::pet_grown_to(pet, ADULT)),
    )
}

fn ticked(state: &mut PetState, input: PetTick) -> bool {
    state.tick(input).changed
}

fn frame(position: (usize, usize)) -> PetFrameInputs {
    PetFrameInputs {
        travel: PetTravel { x: 40, y: 5 },
        zone: Rect::new(10, 20, 48, 8),
        neighbours: Neighbours::default(),
        position,
        home: (0, 0),
        rows: [None; 2],
        treat: None,
    }
}

/// Tick with the cursor outside the box until the pet is back on the
/// stroll, asserting it walks there: never more than a cell per axis per
/// animation edge. Returns the last perch it held, which must be right
/// next to `home` so handing over to the stroll is not a jump either.
fn walk_home(state: &mut PetState, mut wall: usize, now: Instant) -> Perch {
    let mut last = state.perch().expect("perched before the cursor left");
    for _ in 0..100 {
        wall += 2;
        state.tick(tick(wall, now, Some(frame((last.x, last.y))), Some((0, 0))));
        let Some(next) = state.perch() else {
            return last;
        };
        assert!(next.x.abs_diff(last.x) <= 1 && next.y.abs_diff(last.y) <= 1);
        last = next;
    }
    panic!("the pet never got back to the stroll");
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
        out: true,
        today: chrono::Utc::now().date_naive(),
        day_age: Duration::from_secs(3600),
    }
}

#[tokio::test]
async fn the_pet_walks_after_the_cursor_and_lets_go_when_it_leaves() {
    let mut state = fresh_state("pet-follows").await;
    assert_eq!(state.species, PetSpecies::Cat);
    let now = Instant::now();
    // The first tick wakes the stored (asleep) pet; after that a quiet
    // tick has nothing to report.
    assert!(ticked(&mut state, tick(0, now, None, None)), "the wake-up");
    assert!(
        !ticked(&mut state, tick(0, now, None, None)),
        "nothing to report"
    );
    assert_eq!(state.perch(), None);

    // The cursor lands far to the right inside the box: the pet turns and
    // walks a cell per animation edge from where it stood.
    let far_right = Some((10 + 3 + 30, 20 + 1 + 4));
    assert!(ticked(
        &mut state,
        tick(2, now, Some(frame((0, 0))), far_right)
    ));
    assert_eq!(
        state.perch(),
        Some(Perch {
            x: 1,
            y: 1,
            look: Look::Right
        })
    );
    // A sparse tick covers the same ground as the edges it skipped.
    assert!(ticked(
        &mut state,
        tick(12, now, Some(frame((1, 1))), far_right)
    ));
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
        !ticked(
            &mut state,
            tick(wall + 2, now, Some(frame((30, 4))), far_right)
        ),
        "a parked pet reports nothing"
    );

    // The cursor leaves the box: the pet walks back to where the stroll
    // is, a cell at a time, and only then does the stroll take over. It
    // must not teleport there.
    let last = walk_home(&mut state, wall + 2, now);
    assert!(last.x <= 1 && last.y <= 1, "let go right beside the stroll");
    assert_eq!(last.look, Look::Left, "facing the way it walked");
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
    // way back once it is gone. Nothing pins a purring pet.
    let later = t0 + SULK_FOR + ASLEEP_AFTER + Duration::from_secs(1);
    state.note_petted(later);
    assert!(ticked(
        &mut state,
        tick(6, later, Some(frame((7, 2))), inside)
    ));
    assert_eq!(state.mood(), PetMood::Purring);
    assert!(state.perch().is_some(), "following the click");
    walk_home(&mut state, 6, later);
    assert_eq!(state.perch(), None, "cursor gone: back on the stroll");
    // The purr ends on its own.
    assert!(ticked(
        &mut state,
        tick(10, later + PURR_FOR, Some(frame((7, 2))), None)
    ));
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
    let svc = super::super::svc::PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let stored = svc.bootstrap(user.id).await.expect("bootstrap");
    assert_eq!(stored.row.mood(), PetMood::Asleep);
    let mut state = PetState::new(user.id, svc, Some(stored));

    let now = Instant::now();
    assert!(
        ticked(&mut state, tick(0, now, None, None)),
        "waking up is a change"
    );
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

/// An account with its pet (an egg, out), the service its sessions share,
/// and the database kept alive for the answers.
async fn owner(handle: &str) -> (TestDb, PetService, Uuid) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, handle).await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    svc.bootstrap(user.id).await.expect("bootstrap");
    (test_db, svc, user.id)
}

async fn session(svc: &PetService, user_id: Uuid) -> PetState {
    let pet = svc.bootstrap(user_id).await.expect("bootstrap");
    PetState::new(user_id, svc.clone(), Some(pet))
}

/// A tick `since_key` after the owner's last key, with the pet out.
fn tick_after_key(wall_tick: usize, key: Instant) -> PetTick {
    PetTick {
        ambient: awake(key),
        ..tick(wall_tick, key + Duration::from_millis(5), None, None)
    }
}

/// Tick until `ready` holds: the service answers from a task of its own.
/// Every tick is the same key, so the waiting presses nothing.
async fn tick_until(
    state: &mut PetState,
    key: Instant,
    label: &str,
    ready: impl Fn(&PetState) -> bool,
) -> Option<crate::app::common::primitives::Banner> {
    let mut banner = None;
    let mut wall = state.animation_ticks();
    for _ in 0..500 {
        wall += 1;
        let ticked = state.tick(tick_after_key(wall, key));
        banner = ticked.banner.or(banner);
        if ready(state) {
            return banner;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for {label}");
}

#[tokio::test]
async fn the_first_key_of_a_day_counts_it_and_a_long_silence_gets_a_welcome() {
    let (_test_db, svc, user_id) = owner("pet-state-day").await;
    let today = chrono::Utc::now().date_naive();
    // Hatched four days ago, and not a key since: three whole days away.
    for days_ago in [5, 4] {
        svc.act(
            user_id,
            PetCommand::CountDay,
            today - chrono::Duration::days(days_ago),
        )
        .await
        .expect("an earlier day");
    }
    svc.act(
        user_id,
        PetCommand::Hatch(PetSpecies::Cat),
        today - chrono::Duration::days(4),
    )
    .await
    .expect("hatch");
    let mut state = session(&svc, user_id).await;
    let boot = Instant::now();

    // Connecting is not a key: nothing is asked, so nothing can answer.
    state.tick(tick_after_key(1, boot));
    assert_eq!(
        (state.growth().stage(), state.growth().days_together),
        (Stage::Baby, 2)
    );

    let key = boot + Duration::from_secs(1);
    let banner = tick_until(&mut state, key, "the day to count", |state| {
        state.growth().days_together == 3
    })
    .await
    .expect("a welcome back");
    assert_eq!(banner.message, "cat is glad you are back (3 days away)");
    assert!(matches!(
        state.moment(),
        Some((Moment::Welcome { days_away: 3 }, _))
    ));
    assert_eq!(state.growth().last_day, Some(today));
    assert_eq!(
        state.growth().still_needs(),
        Some((Stage::Young, Need::Treats(5))),
        "days never grow a hatched pet: young is five treats ahead"
    );
}

/// An egg on its first day has nothing to show for a pat but the chips, so
/// the pat names the wait.
#[tokio::test]
async fn a_pat_tells_a_day_old_egg_when_it_hatches() {
    let (_test_db, svc, user_id) = owner("pet-state-warm").await;
    let today = chrono::Utc::now().date_naive();
    svc.act(user_id, PetCommand::CountDay, today)
        .await
        .expect("count the day");
    let mut state = session(&svc, user_id).await;
    let key = Instant::now();
    state.tick(tick_after_key(1, key));

    assert_eq!(state.click(key, today, false), Clicked::Done);
    assert!(state.petted_on(today));
    assert_eq!(
        state.petted_line(),
        "The egg is warm (+100 chips): it hatches tomorrow"
    );
}

#[tokio::test]
async fn a_ready_egg_asks_for_the_care_box_then_a_click_pats_and_gives_what_it_earned() {
    let (_test_db, svc, user_id) = owner("pet-state-click").await;
    let today = chrono::Utc::now().date_naive();
    for days_ago in [1, 0] {
        svc.act(
            user_id,
            PetCommand::CountDay,
            today - chrono::Duration::days(days_ago),
        )
        .await
        .expect("count a day");
    }
    let mut state = session(&svc, user_id).await;
    let key = Instant::now();
    state.tick(tick_after_key(1, key));

    // A toy thrown at an egg lands nowhere, and an egg eats nothing.
    state.throw(Toy::Ball, 4, 2);
    assert_eq!(state.plaything(), None);

    // A ready egg is not hatched by the click: what comes out of it is
    // picked in the care box.
    assert!(state.ready_to_hatch());
    assert_eq!(state.display_name(), "egg");
    assert_eq!(state.click(key, today, false), Clicked::OpenCare);
    assert_eq!(state.growth().stage(), Stage::Egg);
    state.hatch(PetSpecies::Dog);
    let banner = tick_until(&mut state, key, "the hatch", |state| {
        state.growth().stage() == Stage::Baby
    })
    .await
    .expect("the hatch says so");
    assert_eq!(banner.message, "The egg hatched: a baby dog");
    assert!(matches!(state.moment(), Some((Moment::Hatched, _))));
    assert_eq!(state.animal(), Some(PetSpecies::Dog));

    // The next click is the day's pat.
    assert!(!state.petted_on(today));
    assert_eq!(state.click(key, today, false), Clicked::Done);
    assert!(state.petted_on(today));
    assert_eq!(state.petted_line(), "Petted (+100 chips)");

    // The day's first win earns the treat, of its family's kind; a win in
    // another family the same day asks for nothing more, and the click
    // after the pat gives it.
    state.note_win(key, GameFamily::ArcadeDaily);
    state.note_win(key, GameFamily::Table);
    let banner = tick_until(&mut state, key, "the treat", |state| {
        state.waiting().is_some()
    })
    .await
    .expect("the treat says so");
    assert_eq!(
        banner.message,
        "A puzzle treat for dog: click it to give it"
    );
    assert_eq!(state.waiting(), Some(TreatKind::Puzzle));
    assert_eq!(state.title(), None, "a treat in the pouch names nobody");

    assert_eq!(state.click(key, today, false), Clicked::Done);
    let banner = tick_until(&mut state, key, "the treat eaten", |state| {
        state.waiting().is_none()
    })
    .await
    .expect("a treat says how far it took the pet");
    assert_eq!(
        banner.message,
        "dog ate the puzzle treat: young after 4 more treats"
    );
    assert_eq!(state.growth().treats_eaten, 1);
    assert!(matches!(
        state.moment(),
        Some((Moment::Eating(TreatKind::Puzzle), _))
    ));
    assert_eq!(
        (state.growth().bond_days, state.growth().last_bond_day),
        (1, Some(today))
    );
    // Eaten, it is what the pet goes by and carries until the day ends.
    assert_eq!(state.title(), Some("the bookish"));
    assert_eq!(state.body().carries, Some(TreatKind::Puzzle));
    state.tick(PetTick {
        today: today + chrono::Duration::days(1),
        ..tick_after_key(state.animation_ticks() + 1, key)
    });
    assert_eq!(state.title(), None, "gone with the day");
}

/// `/pet dog` and `/pet reset yes`: the row decides, and the mirror says
/// what came of it.
#[tokio::test]
async fn the_owner_changes_what_it_is_and_starts_it_over() {
    let (_test_db, svc, user_id) = owner("pet-state-orders").await;
    let today = chrono::Utc::now().date_naive();
    let mut state = session(&svc, user_id).await;
    let key = Instant::now();
    state.tick(tick_after_key(1, key));

    state.change_species(PetSpecies::Bird);
    // The answer frees the mirror for its owner's next command.
    let banner = tick_until(&mut state, key, "the refusal", |state| state.busy.is_none()).await;
    assert_eq!(
        banner.map(|banner| banner.message),
        Some("It is still an egg".to_string())
    );
    assert_eq!(state.animal(), None);

    for days_ago in [1, 0] {
        svc.act(
            user_id,
            PetCommand::CountDay,
            today - chrono::Duration::days(days_ago),
        )
        .await
        .expect("count a day");
    }
    svc.act(user_id, PetCommand::Hatch(PetSpecies::Cat), today)
        .await
        .expect("hatch");
    let mut state = session(&svc, user_id).await;
    state.tick(tick_after_key(1, key));
    assert_eq!(state.animal(), Some(PetSpecies::Cat));

    state.change_species(PetSpecies::Bird);
    let banner = tick_until(&mut state, key, "the bird", |state| {
        state.animal() == Some(PetSpecies::Bird)
    })
    .await
    .expect("it says what it became");
    assert_eq!(banner.message, "bird is a bird now");

    state.start_over();
    let banner = tick_until(&mut state, key, "the egg", |state| {
        state.growth().stage() == Stage::Egg
    })
    .await
    .expect("it says so");
    assert_eq!(banner.message, "Your pet is an egg again");
    assert_eq!((state.animal(), state.growth().days_together), (None, 0));
}

/// Treats never enter a stage: a pet with its treats asks for the care
/// box, and the drawing its owner picks there is the one it grows into.
#[tokio::test]
async fn a_pet_with_its_treats_asks_for_the_care_box_and_grows_as_its_owner_picks() {
    let (_test_db, svc, user_id) = owner("pet-state-grow").await;
    let today = chrono::Utc::now().date_naive();
    let ago = |days| today - chrono::Duration::days(days);
    for days_ago in (5..7).rev() {
        svc.act(user_id, PetCommand::CountDay, ago(days_ago))
            .await
            .expect("count a day");
    }
    svc.act(user_id, PetCommand::Hatch(PetSpecies::Cat), ago(5))
        .await
        .expect("hatch");
    for days_ago in (0..5).rev() {
        for command in [PetCommand::Earn(TreatKind::Puzzle), PetCommand::Give] {
            svc.act(user_id, command, ago(days_ago))
                .await
                .expect("a treat a day");
        }
    }
    let mut state = session(&svc, user_id).await;
    let key = Instant::now();
    state.tick(tick_after_key(1, key));

    assert_eq!(
        (state.growth().stage(), state.growth().treats_eaten),
        (Stage::Baby, 5),
        "five treats, still a baby"
    );
    assert_eq!(
        state.offers(),
        Some([
            Form::Young { top: Pick::One },
            Form::Young { top: Pick::Two }
        ])
    );
    assert_eq!(state.click(key, today, false), Clicked::OpenCare);
    assert!(
        !state.petted_on(today),
        "the click that opens the box pats nothing"
    );

    state.grow(Pick::Two);
    let banner = tick_until(&mut state, key, "the pick", |state| {
        state.growth().stage() == Stage::Young
    })
    .await
    .expect("growing up says so");
    assert_eq!(banner.message, "cat grew up: young");
    assert_eq!(state.portrait().form, Form::Young { top: Pick::Two });
    assert_eq!(state.offers(), None, "an adult is fifteen treats off");

    // With nothing to pick, a click is the pet's again.
    assert_eq!(state.click(key, today, false), Clicked::Done);
    assert!(state.petted_on(today));
}

/// A tick `later` than the real UTC clock, with the pet out: a stay is
/// timed on that clock, whatever the session's own.
fn tick_at_clock(wall_tick: usize, key: Instant, later: Duration) -> PetTick {
    let utc = chrono::Utc::now();
    PetTick {
        today: utc.date_naive(),
        day_age: Duration::from_secs(u64::from(chrono::Timelike::num_seconds_from_midnight(&utc)))
            + later,
        ..tick_after_key(wall_tick, key)
    }
}

/// A pet sent calling stays with its host for an hour: the host's box has
/// a guest and the caller's tile says where its pet is, both until the
/// clock says the hour is up, with nobody asking the service.
#[tokio::test]
async fn a_pet_sent_calling_is_its_hosts_guest_for_an_hour() {
    let test_db = new_test_db().await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let host = create_test_user(&test_db.db, "pet-state-host").await;
    let caller = create_test_user(&test_db.db, "pet-state-caller").await;
    for user in [&host, &caller] {
        svc.bootstrap(user.id).await.expect("bootstrap");
    }
    let today = chrono::Utc::now().date_naive();
    for days_ago in [1, 0] {
        svc.act(
            caller.id,
            PetCommand::CountDay,
            today - chrono::Duration::days(days_ago),
        )
        .await
        .expect("count a day");
    }
    svc.act(caller.id, PetCommand::Hatch(PetSpecies::Bird), today)
        .await
        .expect("hatch");
    let key = Instant::now();
    let minutes = |n: u64| Duration::from_secs(n * 60);

    let mut sender = session(&svc, caller.id).await;
    sender.tick(tick_at_clock(1, key, Duration::ZERO));
    assert_eq!(sender.calling(), None);
    assert!(sender.guests().is_empty());
    sender.visit(host.id, VisitKind::Call);
    let banner = tick_until(&mut sender, key, "the call", |state| !state.visiting)
        .await
        .expect("the call says how it went");
    assert_eq!(
        banner.message,
        "the bird went to stay with the egg for an hour: it kept the egg warm"
    );

    // The caller's own pet stays in its box and says where it is, counting
    // down on the clock.
    let host_name = host.username.as_str();
    let mut sender = session(&svc, caller.id).await;
    sender.tick(tick_at_clock(1, key, Duration::ZERO));
    assert_eq!(sender.calling(), Some((host_name, 60)));
    assert!(sender.tick(tick_at_clock(2, key, minutes(30))).changed);
    assert_eq!(sender.calling(), Some((host_name, 30)));
    assert!(
        !sender.tick(tick_at_clock(3, key, minutes(30))).changed,
        "the same minute pays no frame"
    );
    assert!(sender.tick(tick_at_clock(4, key, minutes(60))).changed);
    assert_eq!(sender.calling(), None, "home again");

    // The host has it as a guest, drawn as it is, and sees who sent it.
    let mut home = session(&svc, host.id).await;
    home.tick(tick_at_clock(1, key, Duration::ZERO));
    let guests = home.guests();
    assert_eq!(guests.len(), 1);
    assert_eq!(
        (
            guests[0].visitor_id,
            guests[0].username.as_str(),
            guests[0].species,
            guests[0].form
        ),
        (
            caller.id,
            caller.username.as_str(),
            PetSpecies::Bird,
            Form::Baby
        )
    );
    assert_eq!(home.calling(), None);
    assert_eq!(
        super::visit_line(&home.visitors()[0], &home.display_name()),
        format!("{}'s pet came to stay with egg", caller.username)
    );

    // A guest blinks: a box on screen pays a frame on that edge and on no
    // other tick, and a box nobody sees pays none.
    home.tick(tick_at_clock(62, key, Duration::ZERO));
    assert!(
        !home
            .tick(PetTick {
                frame: Some(frame((0, 0))),
                ..tick_at_clock(63, key, Duration::ZERO)
            })
            .changed,
        "a still tick"
    );
    assert!(
        home.tick(PetTick {
            frame: Some(frame((0, 0))),
            ..tick_at_clock(64, key, Duration::ZERO)
        })
        .changed,
        "its eyes close"
    );
    home.tick(tick_at_clock(127, key, Duration::ZERO));
    assert!(
        !home.tick(tick_at_clock(128, key, Duration::ZERO)).changed,
        "no box drawn, no frame"
    );

    assert!(!home.tick(tick_at_clock(129, key, minutes(59))).changed);
    assert_eq!(home.guests().len(), 1, "a minute to go");
    assert!(home.tick(tick_at_clock(130, key, minutes(60))).changed);
    assert!(home.guests().is_empty(), "the hour is up");
}

#[tokio::test]
async fn ten_active_minutes_in_one_door_earn_the_days_treat() {
    let (_test_db, svc, user_id) = owner("pet-state-door").await;
    let mut state = session(&svc, user_id).await;
    let key = Instant::now();
    state.tick(tick_after_key(1, key));

    // Not a door; and two doors do not add up to one.
    state.note_active_seconds(Screen::Dashboard, 900.0);
    state.note_active_seconds(Screen::Nethack, 300.0);
    state.note_active_seconds(Screen::Dcss, 400.0);
    // The pat is this session's next ask: when its answer is in, an ask
    // for a treat raised before it would be too.
    state.pat(key, chrono::Utc::now().date_naive());
    let db_pet = || async { svc.bootstrap(user_id).await.expect("reload") };
    crate::test_helpers::wait_until(
        || async { db_pet().await.row.last_petted.is_some() },
        "the pat to land",
    )
    .await;
    assert!(db_pet().await.waiting.is_none(), "nothing earned yet");

    state.note_active_seconds(Screen::Nethack, 300.0);
    tick_until(&mut state, key, "the door treat", |state| {
        state.waiting() == Some(TreatKind::Door)
    })
    .await;
}

/// A pet brought back out counts the day. The mirror asks only once the
/// switch is written: a count that found the pet still away would be
/// refused and not asked again that day.
#[tokio::test]
async fn a_pet_brought_back_out_counts_its_day() {
    let (test_db, svc, user_id) = owner("pet-state-out").await;
    let client = test_db.db.get().await.expect("db client");
    PetCompanion::set_enabled(&client, user_id, false)
        .await
        .expect("put away");
    let mut state = session(&svc, user_id).await;
    let boot = Instant::now();
    state.tick(PetTick {
        out: false,
        ..tick_after_key(1, boot)
    });
    assert_eq!(state.growth().days_together, 0);

    state.set_enabled(true);
    let key = boot + Duration::from_secs(1);
    tick_until(&mut state, key, "the day to count", |state| {
        state.growth().days_together == 1
    })
    .await;
    assert_eq!(
        state.growth().last_day,
        Some(chrono::Utc::now().date_naive())
    );
}

/// A reload and an answer race each other: whichever arrives second, the
/// mirror keeps the newer row.
#[tokio::test]
async fn an_older_reload_never_overwrites_a_newer_answer() {
    let (_test_db, svc, user_id) = owner("pet-state-order").await;
    let mut state = session(&svc, user_id).await;
    let older = svc.bootstrap(user_id).await.expect("the row as it was");
    let today = chrono::Utc::now().date_naive();
    let acted = svc
        .act(user_id, PetCommand::CountDay, today)
        .await
        .expect("count the day");
    assert!(acted.pet.row.revision > older.row.revision);

    state
        .outcome_tx
        .send(PetOutcome::Acted {
            pet: Box::new(acted.pet),
            command: PetCommand::CountDay,
            did: acted.did,
        })
        .expect("the answer");
    state
        .outcome_tx
        .send(PetOutcome::Reloaded {
            pet: Box::new(older),
        })
        .expect("the stale reload");
    let key = Instant::now();
    state.tick(tick_after_key(1, key));

    assert_eq!(
        (state.growth().days_together, state.growth().last_day),
        (1, Some(today))
    );
}

/// The portrait other surfaces draw (the Arcade's corner, the tavern's
/// heel) is the mirror's pet: its species and stage, its mood, what it ate
/// today carried, its tricks.
#[tokio::test]
async fn the_portrait_is_the_pet_as_the_mirror_holds_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-portrait-it").await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let mut pet =
        crate::test_helpers::pet_grown_to(svc.bootstrap(user.id).await.expect("bootstrap"), ADULT);
    pet.row.species = PetSpecies::Bird.as_str().to_string();
    pet.row.mood = PetMood::Proud.as_str().to_string();
    let now = Instant::now();
    let today = chrono::Utc::now().date_naive();
    pet.fed = Some(Fed {
        kind: TreatKind::Door,
        on: today,
    });
    pet.row.tricks = vec![
        Trick::Bow.as_str().to_string(),
        Trick::Hop.as_str().to_string(),
    ];
    let mut state = PetState::new(user.id, svc, Some(pet));
    assert_eq!(
        state.portrait().carries,
        None,
        "no day is known before a tick"
    );
    // The tick reads the mood too: a win keeps it proud.
    state.note_win(now, GameFamily::Lateania);
    state.tick(tick(1, now, None, None));

    let portrait = state.portrait();
    assert_eq!(
        (
            portrait.species,
            portrait.form,
            portrait.mood,
            portrait.carries,
            portrait.tricks
        ),
        (
            PetSpecies::Bird,
            ADULT,
            PetMood::Proud,
            Some(TreatKind::Door),
            &[Trick::Bow, Trick::Hop][..]
        )
    );
}
