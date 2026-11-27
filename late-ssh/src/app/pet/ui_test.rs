use std::cell::Cell;
use std::time::Instant;

use late_core::models::pet::{
    Form, PetCompanion, PetMood, PetSpecies, Pick, TreatKind, Trick, VisitKind,
};
use late_core::test_utils::create_test_user;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::{
    LEG_TICKS, Neighbours, PET_BOX_MIN_ROWS, PetPose, PetView, Portrait, STROLL_TICKS, WATCH_TICKS,
    WatchSide, WatchTarget, draw_pet_box, frame_changed, guest_label, pick_name, portrait_lines,
    portrait_with_guests, status_line, still_rows,
};
use crate::app::pet::session::{
    Ambient, Body, Look, PET_WIDTH, Perch, PetFrameInputs, PetState, PetTick, PetTravel,
};
use crate::test_helpers::new_test_db;

const WIDE: PetTravel = PetTravel { x: 40, y: 0 };
/// A box with nothing beside it.
const ALONE: Neighbours = Neighbours {
    tank: None,
    bonsai: None,
};
/// A tank against the right edge and nothing else.
const GLASS: Neighbours = Neighbours {
    tank: Some(WatchSide::Right),
    bonsai: None,
};

/// A grown pet, drawn with the first of each pick.
const ADULT: Form = Form::Adult {
    top: Pick::One,
    feet: Pick::One,
};

/// Every form a pet can take: an egg, a baby, and each stage after with
/// every pick made on the way to it. Sixteen.
fn all_forms() -> Vec<Form> {
    let mut forms = vec![Form::Egg, Form::Baby];
    for top in Pick::ALL {
        forms.push(Form::Young { top });
        for feet in Pick::ALL {
            forms.push(Form::Adult { top, feet });
            for face in Pick::ALL {
                forms.push(Form::Elder { top, feet, face });
            }
        }
    }
    forms
}

/// A grown cat in `mood`, with nothing going on.
fn body(mood: PetMood) -> Body {
    Body {
        species: PetSpecies::Cat,
        form: ADULT,
        mood,
        ready: false,
        carries: None,
        moment: None,
    }
}

#[test]
fn a_sleeping_pet_never_pays_a_frame() {
    // Asleep: curled on the floor, no blink, a limp tail: the box is fully
    // static, so no tick may report a change.
    for tick in 0..500 {
        assert!(!frame_changed(
            body(PetMood::Asleep),
            ALONE,
            None,
            tick,
            WIDE
        ));
    }
}

#[test]
fn an_awake_pet_changes_on_blink_edges_and_skips_still_ticks() {
    // Blink turns on at tick % 64 == 0 and off at tick % 64 == 3; both edges
    // repaint regardless of where the stroll is.
    assert!(frame_changed(body(PetMood::Idle), ALONE, None, 64, WIDE));
    assert!(frame_changed(body(PetMood::Idle), ALONE, None, 67, WIDE));

    // The gate only pays for ticks where the art moves: across a whole blink
    // period a strolling pet must have both changed and clean ticks.
    let changed_ticks = (1..=64)
        .filter(|&tick| frame_changed(body(PetMood::Idle), ALONE, None, tick, WIDE))
        .count();
    assert!(changed_ticks > 0, "an awake pet animates");
    assert!(changed_ticks < 64, "an awake pet still has clean ticks");

    // Sulking: parked, but awake, so the blink edges and one slow tail
    // flick still repaint and nothing else does.
    let sulk_ticks = (1..=64)
        .filter(|&tick| frame_changed(body(PetMood::Sulking), ALONE, None, tick, WIDE))
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
        body(PetMood::Idle),
        ALONE,
        None,
        64,
        PetTravel { x: 0, y: 0 }
    ));
}

#[test]
fn a_calm_pet_beside_the_tank_strolls_twenty_minutes_and_watches_five() {
    let watch_tank = PetPose::Watch(WatchTarget::Tank, WatchSide::Right);
    let pose = |tick| PetPose::for_frame(body(PetMood::Idle), GLASS, None, tick);
    assert_eq!(pose(0), PetPose::Stroll);
    assert_eq!(pose(STROLL_TICKS - 1), PetPose::Stroll);
    assert_eq!(pose(STROLL_TICKS), watch_tank);
    assert_eq!(pose(STROLL_TICKS + WATCH_TICKS - 1), watch_tank);
    assert_eq!(
        pose(STROLL_TICKS + WATCH_TICKS),
        PetPose::Stroll,
        "and round again"
    );
    // The tank is all it has, so it takes the second window of the round
    // too: five minutes at the glass in every twenty-five, whatever else
    // the page holds.
    assert_eq!(pose(LEG_TICKS + STROLL_TICKS), watch_tank);
    assert_eq!(pose(2 * LEG_TICKS + STROLL_TICKS), watch_tank);
    // Roughly the minutes on the label, at the 66ms wall tick.
    assert_eq!((STROLL_TICKS * 66 + 30_000) / 60_000, 20);
    assert_eq!((WATCH_TICKS * 66 + 30_000) / 60_000, 5);
    // Vibing and chatty watch too; a wound up pet paces instead, a sulking
    // one sulks, a sleeping one sleeps, and no tank means no watching at
    // all.
    assert_eq!(
        PetPose::for_frame(body(PetMood::Vibing), GLASS, None, STROLL_TICKS),
        watch_tank
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Chatty), GLASS, None, STROLL_TICKS),
        watch_tank
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Chatty), GLASS, None, STROLL_TICKS - 1),
        PetPose::Stroll,
        "and it keeps the same twenty/five cycle"
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Proud), GLASS, None, STROLL_TICKS),
        PetPose::Stroll
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Purring), GLASS, None, STROLL_TICKS),
        PetPose::Stroll
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Sulking), GLASS, None, STROLL_TICKS),
        PetPose::Sulk
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Asleep), GLASS, None, STROLL_TICKS),
        PetPose::Sleep
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Idle), ALONE, None, STROLL_TICKS),
        PetPose::Stroll
    );
    // A perch overrides everything: the pet is where the state put it.
    let perch = Perch {
        x: 4,
        y: 0,
        look: Look::Left,
    };
    assert_eq!(
        PetPose::for_frame(body(PetMood::Idle), GLASS, Some(perch), STROLL_TICKS),
        PetPose::At(perch)
    );
}

#[test]
fn a_pet_between_the_tank_and_the_bonsai_alternates_them() {
    // Both beside it: the round is two legs, the tank on the first watch
    // window and the bonsai on the second, so each gets five minutes in
    // fifty and neither is ever watched twice running.
    let between = Neighbours {
        tank: Some(WatchSide::Below),
        bonsai: Some(WatchSide::Left),
    };
    let pose = |tick| PetPose::for_frame(body(PetMood::Idle), between, None, tick);
    assert_eq!(pose(0), PetPose::Stroll);
    assert_eq!(
        pose(STROLL_TICKS),
        PetPose::Watch(WatchTarget::Tank, WatchSide::Below)
    );
    assert_eq!(pose(LEG_TICKS), PetPose::Stroll, "off for another stroll");
    assert_eq!(
        pose(LEG_TICKS + STROLL_TICKS),
        PetPose::Watch(WatchTarget::Bonsai, WatchSide::Left)
    );
    assert_eq!(
        pose(2 * LEG_TICKS + STROLL_TICKS),
        PetPose::Watch(WatchTarget::Tank, WatchSide::Below),
        "and round again from the top"
    );
    // A bonsai on its own takes both windows, the way a tank on its own
    // does.
    let tree = Neighbours {
        tank: None,
        bonsai: Some(WatchSide::Above),
    };
    let watch_tree = PetPose::Watch(WatchTarget::Bonsai, WatchSide::Above);
    assert_eq!(
        PetPose::for_frame(body(PetMood::Idle), tree, None, STROLL_TICKS),
        watch_tree
    );
    assert_eq!(
        PetPose::for_frame(body(PetMood::Idle), tree, None, LEG_TICKS + STROLL_TICKS),
        watch_tree
    );
}

#[test]
fn the_bonsai_gets_a_quieter_beat_than_the_tank() {
    // Watching either one holds the pet still, but the tank's gasp is the
    // fish's doing: a tree gives it less to react to, so it pays fewer
    // frames over the same window.
    let tree = Neighbours {
        tank: None,
        bonsai: Some(WatchSide::Right),
    };
    let window = STROLL_TICKS + 1..=STROLL_TICKS + 480;
    let at_tree = window
        .clone()
        .filter(|&t| frame_changed(body(PetMood::Idle), tree, None, t, WIDE))
        .count();
    let at_glass = window
        .filter(|&t| frame_changed(body(PetMood::Idle), GLASS, None, t, WIDE))
        .count();
    assert!(at_tree > 0, "it still blinks and sniffs");
    assert!(at_tree < at_glass, "{at_tree} vs {at_glass}");
}

#[test]
fn a_watching_pet_holds_still_but_still_blinks_and_gasps() {
    let start = STROLL_TICKS;
    // The walk to the glass repaints, then blink and gasp edges do.
    assert!(frame_changed(body(PetMood::Idle), GLASS, None, start, WIDE));
    let first_blink = (start..).find(|t| t % 64 == 0).unwrap();
    assert!(frame_changed(
        body(PetMood::Idle),
        GLASS,
        None,
        first_blink,
        WIDE
    ));
    let first_gasp = (start + 1..).find(|t| t % 96 == 0).unwrap();
    assert!(frame_changed(
        body(PetMood::Idle),
        GLASS,
        None,
        first_gasp,
        WIDE
    ));
    assert!(
        frame_changed(body(PetMood::Idle), GLASS, None, first_gasp + 6, WIDE),
        "the gasp ends after six ticks"
    );
    // No stroll steps: far fewer paid frames than a strolling pet.
    let window = start + 1..=start + 192;
    let watching = window
        .clone()
        .filter(|&t| frame_changed(body(PetMood::Idle), GLASS, None, t, WIDE))
        .count();
    let strolling = window
        .filter(|&t| frame_changed(body(PetMood::Idle), ALONE, None, t, WIDE))
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
    neighbours: Neighbours,
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
        out: true,
        today: chrono::Utc::now().date_naive(),
        day_age: std::time::Duration::from_secs(3600),
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
                neighbours,
            );
        })
        .unwrap();
    (
        pet_rect.get().expect("the box drew the pet"),
        frame_slot.get().expect("the box recorded its frame"),
    )
}

/// `p care` keeps its room on the status row: a tile too narrow for every
/// segment loses the ones before it, not the key, and only a tile without
/// room for the name and the key drops it (what fits is still shown).
#[tokio::test]
async fn the_status_row_keeps_room_for_its_key() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-status-key").await;
    let svc = crate::app::pet::svc::PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let pet = svc.bootstrap(user.id).await.expect("bootstrap");
    let mut state = PetState::new(
        user.id,
        svc,
        Some(crate::test_helpers::pet_grown_to(pet, ADULT)),
    );
    state.name = Some("Biscuit".to_string());

    let wide = status_line(&state, ALONE, 80).to_string();
    // The details (its age among them) are the care box's.
    assert!(wide.starts_with("Biscuit · adult · "), "{wide}");
    assert!(!wide.contains("day "), "{wide}");
    assert!(wide.ends_with(" · p care"), "{wide}");

    let narrow = status_line(&state, ALONE, 24).to_string();
    assert_eq!(narrow, "Biscuit · adult · p care");

    let name_and_key = status_line(&state, ALONE, 16).to_string();
    assert_eq!(name_and_key, "Biscuit · p care");

    let no_key = status_line(&state, ALONE, 15).to_string();
    assert_eq!(no_key, "Biscuit · adult");
}

#[tokio::test]
async fn an_awake_pet_roams_the_whole_box_and_a_watching_one_sits_at_the_glass() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-roams-box").await;
    let svc = crate::app::pet::svc::PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let pet = svc.bootstrap(user.id).await.expect("bootstrap");
    let mut state = PetState::new(
        user.id,
        svc,
        Some(crate::test_helpers::pet_grown_to(pet, ADULT)),
    );
    let area = Rect::new(0, 0, 60, 12);
    let floor_y = area.bottom() - PET_BOX_MIN_ROWS;

    // Over a long stroll the pet reaches the top and the far right of its
    // box, and comes back down to the floor; nothing pins it anywhere. The
    // frame it records is the zone it roamed and where it stood.
    let mut top = u16::MAX;
    let mut bottom = 0;
    let mut right = 0;
    for tick in (0..6000).step_by(7) {
        let (rect, frame) = draw_at(&mut state, tick, area, ALONE);
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
    let (at_glass, _) = draw_at(&mut state, STROLL_TICKS + 7, area, GLASS);
    assert_eq!(at_glass.y, floor_y, "watching from the floor");
    assert_eq!(
        at_glass,
        draw_at(&mut state, STROLL_TICKS + 707, area, GLASS).0,
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
    let svc = crate::app::pet::svc::PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let pet = svc.bootstrap(user.id).await.expect("bootstrap");
    let mut state = PetState::new(
        user.id,
        svc,
        Some(crate::test_helpers::pet_grown_to(pet, ADULT)),
    );
    let area = Rect::new(0, 0, 20, 3);
    for species in PetSpecies::ALL {
        state.species = species;
        let (rect, _) = draw_at(&mut state, 3, area, ALONE);
        assert_eq!(rect.width, PET_WIDTH as u16, "{species:?}");
        assert_eq!(rect.height, PET_BOX_MIN_ROWS, "{species:?}");
    }
}

fn portrait(species: PetSpecies, form: Form, carries: Option<TreatKind>) -> String {
    portrait_in(species, form, PetMood::Idle, carries)
}

fn portrait_in(
    species: PetSpecies,
    form: Form,
    mood: PetMood,
    carries: Option<TreatKind>,
) -> String {
    let lines = portrait_lines(
        Portrait {
            species,
            form,
            mood,
            carries,
            tricks: &[],
        },
        10,
    );
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The bodies, whole, idle, between blinks: the egg, the three babies,
/// and for each animal both drawings of every part, at the stage that
/// adds it and after.
#[test]
fn every_part_of_every_species_is_drawn() {
    let (one, two) = (Pick::One, Pick::Two);
    let young = |top| Form::Young { top };
    let adult = |pick| Form::Adult {
        top: pick,
        feet: pick,
    };
    let elder = |pick| Form::Elder {
        top: pick,
        feet: pick,
        face: pick,
    };
    for species in PetSpecies::ALL {
        assert_eq!(
            portrait(species, Form::Egg, None),
            "  ,-,  \n /   \\ \n \\___/ ",
            "{species:?}: an egg is an egg"
        );
    }
    let bodies: [(PetSpecies, Form, [&str; 3]); 21] = [
        (
            PetSpecies::Cat,
            Form::Baby,
            ["        ", "  ^ ^   ", " (o.o) ~"],
        ),
        (
            PetSpecies::Cat,
            young(one),
            [r" /\_/\  ", " (o.o) ~", "  >.<  "],
        ),
        (
            PetSpecies::Cat,
            young(two),
            [r" |\_/|  ", " (o.o) ~", "  >.<  "],
        ),
        (
            PetSpecies::Cat,
            adult(one),
            [r" /\_/\  ", "( o.o )~", " > . < "],
        ),
        (
            PetSpecies::Cat,
            adult(two),
            [r" |\_/|  ", "( o.o )~", " d . b "],
        ),
        (
            PetSpecies::Cat,
            elder(one),
            [r" /\_/\  ", "{ o.o }~", " > . < "],
        ),
        (
            PetSpecies::Cat,
            elder(two),
            [r" |\_/|  ", "{=o.o=}~", " d . b "],
        ),
        (
            PetSpecies::Dog,
            Form::Baby,
            ["        ", "  u u   ", " (o.o) ~"],
        ),
        (
            PetSpecies::Dog,
            young(one),
            [r" \,_,/  ", " (o.o) ~", r"  \./  "],
        ),
        (
            PetSpecies::Dog,
            young(two),
            [r" /|_|\  ", " (o.o) ~", r"  \./  "],
        ),
        (
            PetSpecies::Dog,
            adult(one),
            [r" \,_,/  ", "( o.o )~", r" \_._/ "],
        ),
        (
            PetSpecies::Dog,
            adult(two),
            [r" /|_|\  ", "( o.o )~", " w_._w "],
        ),
        (
            PetSpecies::Dog,
            elder(one),
            [r" \,_,/  ", "{ o.o }~", r" \_._/ "],
        ),
        (
            PetSpecies::Dog,
            elder(two),
            [r" /|_|\  ", "{:o.o:}~", " w_._w "],
        ),
        (
            PetSpecies::Bird,
            Form::Baby,
            ["        ", "   ,    ", " (ovo) ~"],
        ),
        (
            PetSpecies::Bird,
            young(one),
            [r"  \|/   ", " (ovo) ~", "  ^ ^  "],
        ),
        (
            PetSpecies::Bird,
            young(two),
            [" ^   ^  ", " (ovo) ~", "  ^ ^  "],
        ),
        (
            PetSpecies::Bird,
            adult(one),
            [r"  \|/   ", " (ovo) ~", r#"  " "  "#],
        ),
        (
            PetSpecies::Bird,
            adult(two),
            [" ^   ^  ", " (ovo) ~", "  J L  "],
        ),
        (
            PetSpecies::Bird,
            elder(one),
            [r"  \|/   ", r"/{ovo}\~", r#"  " "  "#],
        ),
        (
            PetSpecies::Bird,
            elder(two),
            [" ^   ^  ", r"\{ovo}/~", "  J L  "],
        ),
    ];
    for (species, form, body) in bodies {
        assert_eq!(
            portrait(species, form, None),
            body.join("\n"),
            "{species:?} {form:?}"
        );
    }
}

/// The picks are independent: each row of an elder is drawn by one pick
/// alone, so the three of them make eight elders a species, all different.
#[test]
fn three_picks_make_eight_elders_a_species() {
    let rows = |species, top, feet, face| -> Vec<String> {
        portrait(species, Form::Elder { top, feet, face }, None)
            .split('\n')
            .map(str::to_string)
            .collect()
    };
    let one = Pick::One;
    for species in PetSpecies::ALL {
        let mut seen = std::collections::HashSet::new();
        for top in Pick::ALL {
            for feet in Pick::ALL {
                for face in Pick::ALL {
                    let drawn = rows(species, top, feet, face);
                    assert_eq!(
                        drawn,
                        [
                            rows(species, top, one, one)[0].clone(),
                            rows(species, one, one, face)[1].clone(),
                            rows(species, one, feet, one)[2].clone(),
                        ],
                        "{species:?} {top:?} {feet:?} {face:?}"
                    );
                    seen.insert(drawn);
                }
            }
        }
        assert_eq!(seen.len(), 8, "{species:?}");
    }
}

/// The bird's beak sits between its eyes, so its face still reads its
/// mood; the cat's and the dog's mouths are under theirs.
#[test]
fn a_birds_eyes_change_around_its_beak() {
    let young = Form::Young { top: Pick::One };
    let face = |mood| {
        portrait_in(PetSpecies::Bird, young, mood, None)
            .split('\n')
            .nth(1)
            .expect("three rows")
            .to_string()
    };
    assert!(face(PetMood::Purring).contains("(^v^)"), "purring");
    assert!(face(PetMood::Proud).contains("(*v*)"), "proud");
    assert!(face(PetMood::Asleep).contains("(-v-)"), "asleep");
    assert!(
        portrait_in(PetSpecies::Cat, young, PetMood::Purring, None).contains("(^.^)"),
        "a cat's nose stays put"
    );
}

/// Nothing a body can show is wider than the box it walks in: the tail,
/// and the thing it carries on the floor beside its feet.
#[test]
fn no_body_outgrows_its_eight_columns() {
    use unicode_width::UnicodeWidthStr;
    for species in PetSpecies::ALL {
        for form in all_forms() {
            for carries in TreatKind::ALL.into_iter().map(Some).chain([None]) {
                let drawn = portrait(species, form, carries);
                let rows: Vec<&str> = drawn.split('\n').collect();
                assert_eq!(rows.len(), usize::from(PET_BOX_MIN_ROWS));
                for row in rows {
                    assert!(
                        row.width() <= PET_WIDTH,
                        "{species:?} {form:?} {carries:?}: {row:?}"
                    );
                }
            }
        }
    }
    assert!(
        portrait(
            PetSpecies::Cat,
            Form::Young { top: Pick::One },
            Some(TreatKind::Door)
        )
        .ends_with("  >.<  !"),
        "what it carries sits after its feet"
    );
}

/// The care box shows what an egg can hatch into and what a pet can grow
/// into as the bodies the tile will draw, and names each drawing.
#[test]
fn the_care_box_offers_the_bodies_the_tile_draws() {
    for species in PetSpecies::ALL {
        for form in all_forms() {
            let drawn = portrait(species, form, None);
            let rows: Vec<&str> = drawn.split('\n').collect();
            let still = still_rows(species, form);
            for (row, offered) in rows.iter().zip(&still) {
                assert!(
                    row.starts_with(offered.as_str()),
                    "{species:?} {form:?}: {offered:?} against {row:?}"
                );
            }
            match form {
                Form::Egg | Form::Baby => assert_eq!(pick_name(species, form), ""),
                Form::Young { .. } | Form::Adult { .. } | Form::Elder { .. } => {
                    assert!(
                        (1..=10).contains(&pick_name(species, form).len()),
                        "{species:?} {form:?}: a name that fits under its drawing"
                    );
                }
            }
        }
        // The two drawings of a part never share a name.
        for [one, two] in [
            [Pick::One, Pick::Two].map(|top| Form::Young { top }),
            [Pick::One, Pick::Two].map(|feet| Form::Adult {
                top: Pick::One,
                feet,
            }),
            [Pick::One, Pick::Two].map(|face| Form::Elder {
                top: Pick::One,
                feet: Pick::One,
                face,
            }),
        ] {
            assert_ne!(pick_name(species, one), pick_name(species, two));
        }
    }
}

/// The rows a box of `area` draws for `state` at `tick`, as text.
fn box_rows(state: &PetState, area: Rect) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            draw_pet_box(
                frame,
                area,
                &PetView {
                    state,
                    pet_rect_slot: None,
                    frame_slot: None,
                },
                ALONE,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// A pet that came calling stands in its host's box, on the right, under
/// its own look and over its name, and the host keeps the rest of the box
/// to itself. A box too short or too narrow for that draws the host alone.
#[tokio::test]
async fn a_guest_stands_in_its_hosts_box_over_its_name() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let svc = crate::app::pet::svc::PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let host = create_test_user(&test_db.db, "host").await;
    let today = chrono::Utc::now().date_naive();
    // Three callers, the last one a named dog with its ears up.
    let mut callers = Vec::new();
    for (handle, species) in [
        ("ann", PetSpecies::Cat),
        ("bob", PetSpecies::Bird),
        ("cy", PetSpecies::Dog),
    ] {
        let user = create_test_user(&test_db.db, handle).await;
        svc.bootstrap(user.id).await.expect("bootstrap");
        for days_ago in [1, 0] {
            svc.act(
                user.id,
                crate::app::pet::svc::PetCommand::CountDay,
                today - chrono::Duration::days(days_ago),
            )
            .await
            .expect("count a day");
        }
        svc.act(
            user.id,
            crate::app::pet::svc::PetCommand::Hatch(species),
            today,
        )
        .await
        .expect("hatch");
        callers.push(user);
    }
    PetCompanion::set_name(&client, callers[2].id, Some("Biscuit"))
        .await
        .expect("name");
    let pet = svc.bootstrap(host.id).await.expect("bootstrap");
    let alone = PetState::new(
        host.id,
        svc.clone(),
        Some(crate::test_helpers::pet_grown_to(pet, ADULT)),
    );
    let wide = Rect::new(0, 0, 60, 6);
    assert!(
        !box_rows(&alone, wide).join("\n").contains("ovo"),
        "nobody is over yet"
    );

    for caller in &callers {
        let mut sender = PetState::new(
            caller.id,
            svc.clone(),
            Some(svc.bootstrap(caller.id).await.expect("the caller")),
        );
        sender.visit(host.id, VisitKind::Call);
        for _ in 0..500 {
            if svc
                .bootstrap(caller.id)
                .await
                .expect("the caller")
                .calling
                .is_some()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    let pet = svc.bootstrap(host.id).await.expect("the host");
    assert_eq!(pet.guests.len(), 3);
    assert_eq!(
        pet.guests.iter().map(guest_label).collect::<Vec<_>>(),
        [
            format!("{}'s Biscuit", callers[2].username),
            format!("{}'s bird", callers[1].username),
            format!("{}'s cat", callers[0].username),
        ],
        "newest first, a pet with no name by what it is"
    );
    let mut state = PetState::new(
        host.id,
        svc,
        Some(crate::test_helpers::pet_grown_to(pet, ADULT)),
    );

    // Sixty columns hold the newest two, fourteen cells each, on the row
    // over their names; the host's floor is that row too.
    let (host_rect, frame) = draw_at(&mut state, 10, wide, ALONE);
    assert_eq!(frame.zone, Rect::new(0, 0, 32, 5));
    assert!(host_rect.right() <= 32 && host_rect.bottom() <= 5);
    let rows = box_rows(&state, wide);
    let strip: Vec<&str> = rows.iter().map(|row| &row[32..]).collect();
    assert_eq!(
        strip,
        [
            "                            ",
            "                            ",
            "                            ",
            "     u u            ,       ",
            "    (o.o) ~       (ovo) ~   ",
            &format!(
                "{:^14}{:^14}",
                format!("{}'s Biscuit", callers[2].username),
                format!("{}'s bird", callers[1].username)
            ),
        ],
        "{}",
        rows.join("\n")
    );
    let status = status_line(&state, ALONE, 80).to_string();
    assert!(status.contains(" · 3 guests · adult"), "{status}");
    // The caller's own row says where its pet is.
    let mut sender = PetState::new(
        callers[2].id,
        state.svc.clone(),
        Some(
            state
                .svc
                .bootstrap(callers[2].id)
                .await
                .expect("the caller"),
        ),
    );
    draw_at(&mut sender, 10, wide, ALONE);
    let status = status_line(&sender, ALONE, 80).to_string();
    assert!(
        status.contains(&format!(" · visiting {}, ", host.username))
            && status.contains(" min left"),
        "{status}"
    );

    // Half of forty columns holds one.
    let (_, frame) = draw_at(&mut state, 10, Rect::new(0, 0, 40, 6), ALONE);
    assert_eq!(frame.zone, Rect::new(0, 0, 26, 5));

    // A box of three rows has no row for names: the
    // guests stand on its floor beside the host, unnamed.
    let three_rows = Rect::new(0, 0, 60, 3);
    let (_, frame) = draw_at(&mut state, 10, three_rows, ALONE);
    assert_eq!(frame.zone, Rect::new(0, 0, 32, 3));
    let rows = box_rows(&state, three_rows);
    assert_eq!(
        rows.iter().map(|row| &row[32..]).collect::<Vec<_>>(),
        [
            "                            ",
            "     u u            ,       ",
            "    (o.o) ~       (ovo) ~   ",
        ]
    );

    // A box too narrow for a guest's cell draws the host alone in all of
    // it.
    let narrow = Rect::new(0, 0, 27, 6);
    let (_, frame) = draw_at(&mut state, 10, narrow, ALONE);
    assert_eq!(frame.zone, narrow);
    assert!(!box_rows(&state, narrow).join("\n").contains("u u"));

    // A profile stands them beside the host, as far as its width goes.
    let beside = |width| -> Vec<String> {
        portrait_with_guests(state.portrait(), state.guests(), 10, width)
            .iter()
            .map(|line| line.to_string())
            .collect()
    };
    assert_eq!(
        beside(24),
        [
            r" /\_/\            ",
            "( o.o )~    u u   ",
            " > . <     (o.o) ~"
        ],
        "a column of twenty four holds the host and one guest"
    );
    assert_eq!(beside(17), beside(8), "no room, no guest");
    assert_eq!(beside(8).len(), 3);
    assert!(
        beside(40)[2].ends_with("(ovo) ~"),
        "two at most: {:?}",
        beside(40)
    );
}

#[test]
fn a_profile_portrait_runs_through_the_tricks_it_knows() {
    let tricks = [Trick::Hop, Trick::Bow];
    let face = |tick| {
        let lines = portrait_lines(
            Portrait {
                species: PetSpecies::Cat,
                form: ADULT,
                mood: PetMood::Idle,
                carries: None,
                tricks: &tricks,
            },
            tick,
        );
        lines[1].spans[0].content.to_string()
    };
    assert!(face(5).contains("^.^"), "the first round hops");
    assert!(face(200).contains("o.o"), "then it stands a while");
    assert!(face(305).contains("u.u"), "the next round bows");
}

/// An egg sits where it is, and only moves once it has its days or while a
/// pat keeps it warm.
#[test]
fn an_egg_is_still_until_it_is_patted_or_ready_and_then_rocks() {
    let egg = Body {
        form: Form::Egg,
        ..body(PetMood::Idle)
    };
    let ready = Body { ready: true, ..egg };
    let patted = Body {
        mood: PetMood::Purring,
        ..egg
    };
    assert_eq!(
        PetPose::for_frame(egg, GLASS, None, STROLL_TICKS),
        PetPose::Nest
    );
    let frames = |body| {
        (1..=1000usize)
            .filter(|&tick| frame_changed(body, ALONE, None, tick, WIDE))
            .count()
    };
    assert_eq!(
        frames(egg),
        0,
        "an egg short of its days never pays a frame"
    );
    assert_eq!(
        frames(ready),
        1000 / 8,
        "a ready one rocks on eight-tick edges"
    );
    assert_eq!(
        frames(patted),
        1000 / 8,
        "so does one its owner just patted, for as long as it purrs"
    );
}
