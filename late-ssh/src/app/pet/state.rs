//! The rules of a pet growing up, pure: no clock, no I/O. `PetService`
//! loads a `Growth` from the locked row, applies one of these, and stores
//! it; a session's mirror reads the same type to draw and never writes.
//!
//! Nothing here goes down. Days together and treats eaten are counted,
//! never lost; a stage is entered and never left; a trick and a bond day
//! are kept. Only the owner's own `reset` starts a pet over.
//!
//! The days make an egg ready to hatch, and treats eaten make a hatched pet
//! ready for each stage after (`need_for`). Its owner's pick enters the
//! stage: the animal at the hatch, then one of two drawings of a part
//! (`Form`).

use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::pet::{
    Fed, Form, PetCompanion, PetGrowthWrite, Pick, Stage, TreatKind, Trick,
};
use uuid::Uuid;

use crate::app::activity::event::GameFamily;
use crate::app::common::primitives::Screen;

/// Separate UTC days of practice that teach a trick.
pub const TRICK_DAYS: i32 = 3;

/// Active seconds in one door, in a UTC day, that earn the day's treat.
pub const DOOR_TREAT_SECONDS: u64 = 600;

/// Minutes a pet that went calling stays with its host.
pub const STAY_MINUTES: i64 = 60;

/// When a stay that is still running at `now` began, at the earliest.
pub fn stays_since(now: DateTime<Utc>) -> DateTime<Utc> {
    now - chrono::Duration::minutes(STAY_MINUTES)
}

/// The minutes a stay that began at `since` still has at `now`, the one
/// under way counted whole. `None` once it is over.
pub fn stay_left(since: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    let seconds = (since + chrono::Duration::minutes(STAY_MINUTES) - now).num_seconds();
    match seconds > 0 {
        true => Some((seconds + 59) / 60),
        false => None,
    }
}

/// Days together the hatch asks for. The day the pet arrives counts as the
/// first, so an egg hatches from the day after.
pub const HATCH_DAYS: i32 = 2;

/// What a stage waits for, or what is still to come of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Days together: the hatch.
    Days(i32),
    /// Treats eaten: every stage after the hatch.
    Treats(i32),
}

/// What entering `stage` asks for, counted from the egg. Treats are one a
/// UTC day at most, so an elder is sixty days of play at the soonest; an
/// owner who never plays keeps a baby, and loses nothing by it.
pub fn need_for(stage: Stage) -> Need {
    match stage {
        Stage::Egg => Need::Days(0),
        Stage::Baby => Need::Days(HATCH_DAYS),
        Stage::Young => Need::Treats(5),
        Stage::Adult => Need::Treats(20),
        Stage::Elder => Need::Treats(60),
    }
}

/// The kind of treat a win in `family` earns, when it is the day's first.
/// Lateania's win is a mob kill, so it earns by its minutes only
/// (`DOOR_TREAT_SECONDS`).
pub fn treat_for_win(family: GameFamily) -> Option<TreatKind> {
    match family {
        GameFamily::ArcadeDaily => Some(TreatKind::Puzzle),
        GameFamily::ArcadeScore => Some(TreatKind::Arcade),
        GameFamily::Match => Some(TreatKind::Match),
        GameFamily::Table => Some(TreatKind::Table),
        GameFamily::Door => Some(TreatKind::Door),
        GameFamily::Lateania => None,
    }
}

/// Whether `screen` is a door: a game the owner goes into, whose minutes
/// count toward the door treat. The Games hub is the hallway, not a door.
pub fn is_door(screen: Screen) -> bool {
    match screen {
        Screen::Lateania
        | Screen::Rebels
        | Screen::Nethack
        | Screen::Dcss
        | Screen::Brogue
        | Screen::Dopewars
        | Screen::Bashquest
        | Screen::Codekeep
        | Screen::Zork
        | Screen::Usurper
        | Screen::GreenDragon
        | Screen::Darkroom
        | Screen::Realm => true,
        Screen::Dashboard
        | Screen::Arcade
        | Screen::Games
        | Screen::Artboard
        | Screen::Profiles
        | Screen::Leaderboard
        | Screen::Calendars
        | Screen::Clubhouse
        | Screen::Nightcap
        | Screen::City
        | Screen::DailyMatch
        | Screen::HouseTable
        | Screen::Scratchpad => false,
    }
}

/// The title a pet goes by, after its name, on the day it ate a `kind`
/// treat.
pub fn treat_title(kind: TreatKind) -> &'static str {
    match kind {
        TreatKind::Puzzle => "the bookish",
        TreatKind::Arcade => "the quick",
        TreatKind::Match => "the cunning",
        TreatKind::Table => "the lucky",
        TreatKind::Door => "the brave",
    }
}

/// The treat a pet goes by on `today`: the one it ate today. Yesterday's
/// is gone at UTC midnight.
pub fn fed_today(fed: Option<Fed>, today: NaiveDate) -> Option<TreatKind> {
    match fed {
        Some(fed) if fed.on == today => Some(fed.kind),
        Some(_) | None => None,
    }
}

/// What a pet calling on another brings back, from a closed set: by the
/// treat the two share today, else by the caller's own. `caller` and `host`
/// are the treat each pet ate today.
pub fn call_line(
    caller: Option<TreatKind>,
    host: Option<TreatKind>,
    host_stage: Stage,
) -> &'static str {
    match host_stage {
        Stage::Egg => return "it kept the egg warm",
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => {}
    }
    match (caller, caller == host) {
        (None, _) => "they sat in the sun together",
        (Some(TreatKind::Puzzle), true) => "they worked a puzzle out together",
        (Some(TreatKind::Arcade), true) => "they raced laps round the room",
        (Some(TreatKind::Match), true) => "they played a long game and called it a draw",
        (Some(TreatKind::Table), true) => "they split a lucky find",
        (Some(TreatKind::Door), true) => "they went exploring and came back muddy",
        (Some(TreatKind::Puzzle), false) => "it read over their shoulder",
        (Some(TreatKind::Arcade), false) => "it ran rings round them",
        (Some(TreatKind::Match), false) => "it won at staring",
        (Some(TreatKind::Table), false) => "it came back with a found coin",
        (Some(TreatKind::Door), false) => "it came back with a story",
    }
}

/// A day that counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayCounted {
    /// Whole UTC days with no key between the last counted day and this
    /// one: 0 for the day after, and for a first day.
    pub days_away: i64,
}

/// What a day's practice came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Practiced {
    /// One more day of it, `days` so far.
    Progress {
        days: i32,
    },
    Learned,
    /// It already practiced this UTC day.
    AlreadyToday,
    /// It knows this one.
    Known,
    /// An egg learns nothing.
    Egg,
}

/// The growth of one pet: the row's columns, typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Growth {
    pub days_together: i32,
    pub last_day: Option<NaiveDate>,
    pub form: Form,
    pub bond_days: i32,
    pub last_bond_day: Option<NaiveDate>,
    pub tricks: Vec<Trick>,
    pub learning: Option<Trick>,
    pub learning_days: i32,
    pub last_practice: Option<NaiveDate>,
    pub treats_eaten: i32,
}

impl Growth {
    pub fn from_row(row: &PetCompanion) -> Self {
        Self {
            days_together: row.days_together,
            last_day: row.last_day,
            form: row.form(),
            bond_days: row.bond_days,
            last_bond_day: row.last_bond_day,
            tricks: row.tricks(),
            learning: row.learning(),
            learning_days: row.learning_days,
            last_practice: row.last_practice,
            treats_eaten: row.treats_eaten,
        }
    }

    pub fn to_write(&self, user_id: Uuid) -> PetGrowthWrite {
        PetGrowthWrite {
            user_id,
            days_together: self.days_together,
            last_day: self.last_day,
            form: self.form,
            bond_days: self.bond_days,
            last_bond_day: self.last_bond_day,
            tricks: self.tricks.clone(),
            learning: self.learning,
            learning_days: self.learning_days,
            last_practice: self.last_practice,
            treats_eaten: self.treats_eaten,
        }
    }

    pub fn stage(&self) -> Stage {
        self.form.stage()
    }

    /// Count `today` as a day together, once. `None` when the day is
    /// already counted, or when the row holds a later day (a clock that
    /// stepped back never uncounts one). The days enter no stage: they
    /// make an egg ready, and its owner's pick does the rest.
    pub fn count_day(&mut self, today: NaiveDate) -> Option<DayCounted> {
        let days_away = match self.last_day {
            Some(last) if last >= today => return None,
            Some(last) => (today - last).num_days() - 1,
            None => 0,
        };
        self.days_together += 1;
        self.last_day = Some(today);
        Some(DayCounted { days_away })
    }

    /// Count a treat eaten. Only a hatched pet is given one.
    pub fn eat(&mut self) {
        self.treats_eaten += 1;
    }

    /// Whether it has what the stage after this one asks for.
    fn has_what_it_needs(&self) -> bool {
        match self.still_needs() {
            Some((_, Need::Days(0) | Need::Treats(0))) => true,
            Some((_, Need::Days(_) | Need::Treats(_))) | None => false,
        }
    }

    /// Whether the egg has its days and waits on its owner to hatch it.
    pub fn ready_to_hatch(&self) -> bool {
        match self.form {
            Form::Egg => self.has_what_it_needs(),
            Form::Baby | Form::Young { .. } | Form::Adult { .. } | Form::Elder { .. } => false,
        }
    }

    /// The two forms a hatched pet with its treats can grow into, one per
    /// drawing of the part its next stage adds. `None` while it waits on
    /// its treats, for an egg (it hatches) and for an elder.
    pub fn offers(&self) -> Option<[Form; 2]> {
        match (
            self.has_what_it_needs(),
            self.form.grown(Pick::One),
            self.form.grown(Pick::Two),
        ) {
            (true, Some(one), Some(two)) => Some([one, two]),
            (true, _, _) | (false, _, _) => None,
        }
    }

    /// Hatch an egg that has its days. False for anything else. What comes
    /// out of it (cat, dog, bird) is the owner's pick, stored beside this.
    pub fn hatch(&mut self) -> bool {
        match self.ready_to_hatch() {
            true => {
                self.form = Form::Baby;
                true
            }
            false => false,
        }
    }

    /// Enter the next stage drawn with `pick`, for a hatched pet that has
    /// its treats. The stage entered, `None` for anything else.
    pub fn grow(&mut self, pick: Pick) -> Option<Stage> {
        let [one, two] = self.offers()?;
        self.form = match pick {
            Pick::One => one,
            Pick::Two => two,
        };
        Some(self.stage())
    }

    /// Start over: an egg on its first day, nothing learned. The one rule
    /// that takes anything away, and only its owner asks for it.
    pub fn reset(&mut self) {
        *self = Self {
            days_together: 0,
            last_day: None,
            form: Form::Egg,
            bond_days: 0,
            last_bond_day: None,
            tricks: Vec::new(),
            learning: None,
            learning_days: 0,
            last_practice: None,
            treats_eaten: 0,
        };
    }

    /// Stamp `today` as a bond day: the first treat or play of a UTC day.
    /// False when the day already has one.
    pub fn bond(&mut self, today: NaiveDate) -> bool {
        if self.last_bond_day == Some(today) {
            return false;
        }
        self.bond_days += 1;
        self.last_bond_day = Some(today);
        true
    }

    /// One day's practice of `trick`. Starting another trick starts over.
    pub fn practice(&mut self, trick: Trick, today: NaiveDate) -> Practiced {
        match self.form {
            Form::Egg => return Practiced::Egg,
            Form::Baby | Form::Young { .. } | Form::Adult { .. } | Form::Elder { .. } => {}
        }
        if self.tricks.contains(&trick) {
            return Practiced::Known;
        }
        if self.last_practice == Some(today) {
            return Practiced::AlreadyToday;
        }
        let days = match self.learning == Some(trick) {
            true => self.learning_days + 1,
            false => 1,
        };
        self.last_practice = Some(today);
        if days >= TRICK_DAYS {
            self.tricks.push(trick);
            self.learning = None;
            self.learning_days = 0;
            return Practiced::Learned;
        }
        self.learning = Some(trick);
        self.learning_days = days;
        Practiced::Progress { days }
    }

    /// The next stage and what is still to come of what it asks for: days
    /// for an egg, treats after. `None` for an elder. A count of 0 means it
    /// waits on its owner's pick.
    pub fn still_needs(&self) -> Option<(Stage, Need)> {
        let next = self.stage().next()?;
        let left = match need_for(next) {
            Need::Days(days) => Need::Days((days - self.days_together).max(0)),
            Need::Treats(treats) => Need::Treats((treats - self.treats_eaten).max(0)),
        };
        Some((next, left))
    }

    /// The days an egg still waits for. `None` once it has them, and for a
    /// hatched pet.
    pub fn days_to_hatch(&self) -> Option<i32> {
        match self.still_needs() {
            Some((_, Need::Days(days))) if days > 0 => Some(days),
            Some((_, Need::Days(_) | Need::Treats(_))) | None => None,
        }
    }
}

/// When an egg `days` short hatches, as its owner reads it: `tomorrow`,
/// `in 3 days`. The tile, the care box and the pat all say it this way.
pub fn hatch_wait(days: i32) -> String {
    match days {
        1 => "tomorrow".to_string(),
        days => format!("in {days} days"),
    }
}

/// What a hatched pet `treats` short of `next` waits for, as its owner
/// reads it: `young after 1 more treat`. The care box and the treat's
/// banner say it this way.
pub fn treat_wait(next: Stage, treats: i32) -> String {
    match treats {
        1 => format!("{} after 1 more treat", next.as_str()),
        treats => format!("{} after {treats} more treats", next.as_str()),
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
