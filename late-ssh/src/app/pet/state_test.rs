use chrono::NaiveDate;
use late_core::models::pet::{Fed, Form, Pick, Stage, TreatKind, Trick};

use super::{
    DayCounted, Growth, Practiced, STAY_MINUTES, TRICK_DAYS, call_line, fed_today, hatch_wait,
    stay_left, stays_since, treat_for_win, treat_title,
};
use crate::app::activity::event::GameFamily;

fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1).expect("a date") + chrono::Duration::days(n)
}

fn egg() -> Growth {
    Growth {
        days_together: 0,
        last_day: None,
        form: Form::Egg,
        bond_days: 0,
        last_bond_day: None,
        tricks: Vec::new(),
        learning: None,
        learning_days: 0,
        last_practice: None,
    }
}

/// Count one day together on each of `days`.
fn live(growth: &mut Growth, days: std::ops::Range<i64>) {
    for n in days {
        growth.count_day(day(n));
    }
}

#[test]
fn a_day_counts_once_and_never_backwards() {
    let mut growth = egg();
    assert_eq!(
        growth.count_day(day(0)),
        Some(DayCounted { days_away: 0 }),
        "the first day"
    );
    assert_eq!(growth.count_day(day(0)), None, "once a day");
    assert_eq!(growth.count_day(day(1)), Some(DayCounted { days_away: 0 }));
    assert_eq!(
        growth.count_day(day(6)),
        Some(DayCounted { days_away: 4 }),
        "four whole days with no key"
    );
    assert_eq!(growth.count_day(day(3)), None, "a clock that stepped back");
    assert_eq!((growth.days_together, growth.last_day), (3, Some(day(6))));
}

#[test]
fn an_egg_to_an_elder_by_its_days_and_its_owners_picks() {
    let mut growth = egg();
    let (one, two) = (Pick::One, Pick::Two);

    assert_eq!(growth.days_to_hatch(), Some(2), "before its first day");
    assert_eq!(hatch_wait(2), "in 2 days");
    live(&mut growth, 0..1);
    assert!(!growth.ready_to_hatch(), "the arrival day");
    assert!(!growth.hatch(), "an egg without its days stays an egg");
    assert_eq!(growth.days_to_next(), Some(1));
    assert_eq!(growth.days_to_hatch(), Some(1));
    assert_eq!(hatch_wait(1), "tomorrow");
    live(&mut growth, 1..5);
    assert!(growth.ready_to_hatch());
    assert_eq!(growth.days_to_hatch(), None, "a ready egg waits on a pick");
    assert_eq!(growth.stage(), Stage::Egg, "days never hatch an egg");
    assert_eq!(
        growth.offers(),
        None,
        "an egg is offered animals, not drawings"
    );
    assert_eq!(growth.grow(one), None);
    assert!(growth.hatch());
    assert!(!growth.hatch(), "once");
    assert_eq!(growth.days_to_next(), Some(2));

    assert_eq!(growth.offers(), None, "a baby without its days");
    assert_eq!(growth.grow(two), None);
    live(&mut growth, 5..40);
    assert_eq!(growth.stage(), Stage::Baby, "days never enter a stage");
    assert_eq!(
        growth.offers(),
        Some([Form::Young { top: one }, Form::Young { top: two }]),
        "the top, drawn each way"
    );
    assert_eq!(growth.grow(two), Some(Stage::Young));

    // It has the days for an adult already: the next pick waits at once.
    assert_eq!(
        growth.offers(),
        Some([
            Form::Adult {
                top: two,
                feet: one
            },
            Form::Adult {
                top: two,
                feet: two
            }
        ]),
        "the feet, under the top it kept"
    );
    assert_eq!(growth.grow(one), Some(Stage::Adult));
    assert_eq!(growth.offers(), None, "an elder is sixty days off");
    assert_eq!(growth.days_to_next(), Some(60));

    live(&mut growth, 40..100);
    assert_eq!(growth.grow(two), Some(Stage::Elder));
    assert_eq!(
        growth,
        Growth {
            days_together: 100,
            last_day: Some(day(99)),
            form: Form::Elder {
                top: two,
                feet: one,
                face: two
            },
            bond_days: 0,
            last_bond_day: None,
            tricks: Vec::new(),
            learning: None,
            learning_days: 0,
            last_practice: None,
        }
    );
    live(&mut growth, 100..120);
    assert_eq!(growth.offers(), None, "an elder is fully grown");
    assert_eq!(growth.grow(one), None);
    assert_eq!(growth.days_to_next(), None);
}

#[test]
fn a_reset_starts_over_as_an_egg_on_its_first_day() {
    let mut growth = egg();
    live(&mut growth, 0..10);
    growth.hatch();
    growth.bond(day(9));
    assert_eq!(growth.grow(Pick::Two), Some(Stage::Young));
    for n in 0..TRICK_DAYS as i64 {
        growth.practice(Trick::Spin, day(10 + n));
    }
    growth.practice(Trick::Bow, day(20));
    assert_eq!(growth.tricks, vec![Trick::Spin]);

    growth.reset();
    assert_eq!(growth, egg());
    assert_eq!(
        growth.count_day(day(21)),
        Some(DayCounted { days_away: 0 }),
        "its first day again, and it missed nobody"
    );
}

#[test]
fn a_stay_lasts_an_hour_and_counts_its_minutes_down() {
    let came = day(3).and_hms_opt(22, 30, 0).expect("a time").and_utc();
    let after = |seconds| came + chrono::Duration::seconds(seconds);
    assert_eq!(stay_left(came, came), Some(STAY_MINUTES));
    assert_eq!(stay_left(came, after(1)), Some(60), "the minute under way");
    assert_eq!(stay_left(came, after(60)), Some(59));
    assert_eq!(stay_left(came, after(59 * 60 + 59)), Some(1));
    assert_eq!(stay_left(came, after(60 * 60)), None, "the hour is up");
    assert_eq!(
        stay_left(came, after(2 * 60 * 60)),
        None,
        "across midnight, long gone"
    );
    // A stay still running at a moment began after this.
    assert_eq!(stay_left(stays_since(came), came), None);
    assert_eq!(
        stay_left(stays_since(came) + chrono::Duration::seconds(1), came),
        Some(1)
    );
}

#[test]
fn a_treat_names_the_pet_for_the_day_it_ate_it() {
    let fed = Some(Fed {
        kind: TreatKind::Arcade,
        on: day(3),
    });
    assert_eq!(fed_today(fed, day(3)), Some(TreatKind::Arcade));
    assert_eq!(fed_today(fed, day(4)), None, "gone at the day's end");
    assert_eq!(fed_today(None, day(3)), None);
    assert_eq!(
        TreatKind::ALL.map(treat_title),
        [
            "the bookish",
            "the quick",
            "the cunning",
            "the lucky",
            "the brave"
        ]
    );
}

#[test]
fn a_bond_day_is_stamped_once_a_day() {
    let mut growth = egg();
    assert!(growth.bond(day(0)));
    assert!(!growth.bond(day(0)));
    assert!(growth.bond(day(1)));
    assert_eq!((growth.bond_days, growth.last_bond_day), (2, Some(day(1))));
}

#[test]
fn a_trick_takes_separate_days_and_switching_starts_over() {
    let mut growth = egg();
    assert_eq!(growth.practice(Trick::Spin, day(0)), Practiced::Egg);
    growth.form = Form::Baby;

    assert_eq!(
        growth.practice(Trick::Spin, day(0)),
        Practiced::Progress { days: 1 }
    );
    assert_eq!(
        growth.practice(Trick::Spin, day(0)),
        Practiced::AlreadyToday
    );
    assert_eq!(
        growth.practice(Trick::Bow, day(1)),
        Practiced::Progress { days: 1 },
        "another trick starts over"
    );
    for n in 2..TRICK_DAYS as i64 {
        assert_eq!(
            growth.practice(Trick::Bow, day(n)),
            Practiced::Progress { days: n as i32 }
        );
    }
    assert_eq!(
        growth.practice(Trick::Bow, day(TRICK_DAYS as i64)),
        Practiced::Learned
    );
    assert_eq!(
        growth.practice(Trick::Bow, day(TRICK_DAYS as i64 + 1)),
        Practiced::Known
    );
    assert_eq!(
        (growth.tricks.clone(), growth.learning, growth.learning_days),
        (vec![Trick::Bow], None, 0)
    );
}

#[test]
fn every_family_but_lateania_earns_its_treat_by_a_win() {
    assert_eq!(
        treat_for_win(GameFamily::ArcadeDaily),
        Some(TreatKind::Puzzle)
    );
    assert_eq!(
        treat_for_win(GameFamily::ArcadeScore),
        Some(TreatKind::Arcade)
    );
    assert_eq!(treat_for_win(GameFamily::Match), Some(TreatKind::Match));
    assert_eq!(treat_for_win(GameFamily::Table), Some(TreatKind::Table));
    assert_eq!(treat_for_win(GameFamily::Door), Some(TreatKind::Door));
    assert_eq!(
        treat_for_win(GameFamily::Lateania),
        None,
        "a mob kill is not a win: its minutes earn the door treat"
    );
}

#[test]
fn a_call_brings_back_a_line_for_what_the_two_share() {
    let door = Some(TreatKind::Door);
    assert_eq!(
        call_line(door, door, Stage::Adult),
        "they went exploring and came back muddy"
    );
    assert_eq!(
        call_line(door, Some(TreatKind::Puzzle), Stage::Adult),
        "it came back with a story"
    );
    assert_eq!(
        call_line(None, None, Stage::Young),
        "they sat in the sun together",
        "two pets that ate nothing today share nothing in particular"
    );
    assert_eq!(call_line(door, door, Stage::Egg), "it kept the egg warm");
}
