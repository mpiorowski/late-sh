use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::aquarium_shield::AquariumShield;
use ratatui::layout::Rect;

use super::{AquariumCare, AquariumState, CareBar, CareOutcome, Fry};
use crate::app::hub::aquarium::creature::FRY_CREATURE;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
}

fn fed_on(d: u32, streak: i32) -> AquariumCare {
    AquariumCare {
        last_fed: Some(Utc.with_ymd_and_hms(2026, 9, d, 12, 0, 0).unwrap()),
        streak,
        shield: None,
        fry: None,
    }
}

#[test]
fn the_bar_counts_the_streak_in_green_and_the_unfed_days_in_red() {
    // Fed today, five days running: five green boxes.
    assert_eq!(fed_on(10, 5).bar_on(day(10)), CareBar::Streak(5));
    // The day a fry hatches the bar is full; the day after it starts over.
    assert_eq!(fed_on(10, 14).bar_on(day(10)), CareBar::Streak(14));
    assert_eq!(fed_on(10, 15).bar_on(day(10)), CareBar::Streak(1));
    // Not fed: one red box per unfed day, saturating at fourteen.
    assert_eq!(fed_on(10, 5).bar_on(day(11)), CareBar::Dry(1));
    assert_eq!(fed_on(1, 5).bar_on(day(15)), CareBar::Dry(14));
    assert_eq!(fed_on(1, 5).bar_on(day(30)), CareBar::Dry(14));
    // A tank never fed has nothing on the clock either way.
    let never = AquariumCare::new(None, None);
    assert_eq!(never.bar_on(day(10)), CareBar::Dry(0));
    assert!(never.hungry_on(day(10)));
}

#[test]
fn murk_sets_in_after_a_week_and_the_shield_holds_it_off() {
    let care = fed_on(1, 3);
    assert!(!care.murky_on(day(7)), "six unfed days: still clear");
    assert!(care.murky_on(day(8)), "seven unfed days: murky");
    assert!(care.hungry_on(day(8)));

    // A shield over the 2nd through the 20th: nothing counts, the fish
    // are minded, the water stays clear, the bar shows the minded state.
    let minded = AquariumCare {
        shield: Some(AquariumShield {
            starts_at: Utc.with_ymd_and_hms(2026, 9, 2, 0, 0, 0).unwrap(),
            ends_at: Utc.with_ymd_and_hms(2026, 9, 20, 0, 0, 0).unwrap(),
        }),
        ..fed_on(1, 3)
    };
    assert!(!minded.hungry_on(day(8)));
    assert!(!minded.murky_on(day(8)));
    assert_eq!(minded.bar_on(day(8)), CareBar::Minded);
    // The day after the shield lapses the clock resumes from one.
    assert!(minded.hungry_on(day(21)));
    assert_eq!(minded.bar_on(day(21)), CareBar::Dry(1));
}

#[test]
fn feeding_runs_the_streak_like_the_row_does() {
    let today = Utc::now().date_naive();
    let yesterday = today.pred_opt().unwrap();
    let mut care = AquariumCare {
        last_fed: Some(
            yesterday
                .and_hms_opt(23, 0, 0)
                .unwrap()
                .and_utc(),
        ),
        streak: 4,
        shield: None,
        fry: None,
    };
    assert_eq!(care.feed(), CareOutcome::Fed);
    assert_eq!(care.streak, 5, "yesterday was fed: the streak continues");
    assert!(!care.hungry_on(today));
    assert_eq!(care.feed(), CareOutcome::AlreadyFedToday);
    assert_eq!(care.streak, 5);

    let mut lapsed = AquariumCare {
        last_fed: Some(
            yesterday
                .pred_opt()
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_utc(),
        ),
        streak: 9,
        shield: None,
        fry: None,
    };
    assert_eq!(lapsed.feed(), CareOutcome::Fed);
    assert_eq!(lapsed.streak, 1, "a skipped day restarts the streak");
}

#[test]
fn a_fry_is_small_for_a_week() {
    let care = AquariumCare {
        fry: Some(Fry {
            creature: "clownfish".to_string(),
            born: day(10),
        }),
        ..fed_on(10, 14)
    };
    assert_eq!(care.fry_visible_on(day(10)), Some("clownfish"));
    assert_eq!(care.fry_visible_on(day(16)), Some("clownfish"));
    assert_eq!(care.fry_visible_on(day(17)), None, "grown on the seventh day");
}

#[test]
fn the_fry_takes_one_of_its_parents_places_in_the_water() {
    let mut sim = AquariumState::default_for_area(Rect::new(0, 0, 100, 20)).expect("sim");
    let fish = vec![("clownfish".to_string(), 3), ("anchovy".to_string(), 2)];
    sim.set_active_creatures(&fish, Some("clownfish"));

    let name_of = |sim: &AquariumState, def: usize| sim.definitions[def].name.clone();
    let count = |sim: &AquariumState, name: &str| {
        sim.entities
            .iter()
            .filter(|entity| name_of(sim, entity.def) == name)
            .count()
    };
    assert_eq!(count(&sim, FRY_CREATURE), 1);
    assert_eq!(count(&sim, "clownfish"), 2);
    assert_eq!(count(&sim, "anchovy"), 2);

    // The same population again respawns nothing.
    let before: Vec<(i32, i32)> = sim.entities.iter().map(|e| (e.x, e.y)).collect();
    sim.set_active_creatures(&fish, Some("clownfish"));
    let after: Vec<(i32, i32)> = sim.entities.iter().map(|e| (e.x, e.y)).collect();
    assert_eq!(before, after);

    // The fry grew up: three full clownfish, no hatchling.
    sim.set_active_creatures(&fish, None);
    assert_eq!(count(&sim, FRY_CREATURE), 0);
    assert_eq!(count(&sim, "clownfish"), 3);

    // A fry whose parent species left the water is not drawn.
    sim.set_active_creatures(&[("anchovy".to_string(), 2)], Some("clownfish"));
    assert_eq!(count(&sim, FRY_CREATURE), 0);
    assert_eq!(sim.entities.len(), 2);
}
