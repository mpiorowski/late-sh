//! The comeback moments: what the first return days hand a newcomer, and
//! what every account gets shown once. The tour is the first session; a
//! moment is one box on the first login of a later UTC day, in series, one
//! per day, each shown only when it has something to sell this session
//! (the radio to someone who cannot hear it). None is a stop: the
//! session lands where it would have,
//! the box sits over it. This file is the series, the gate that says which
//! moment a login is owed, and the box's open state; `svc.rs` opens it
//! after the splash and stamps the account, `input.rs` takes its two keys,
//! `ui.rs` draws it.

use chrono::NaiveDate;
use late_core::models::user::ComebackMark;

/// The moments, in the order the days hand them out. `pub` because
/// `metrics` counts each by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    /// The house has a radio, and over plain ssh you hear none of it: the
    /// `late` CLI and the listen page. Only to a session with no client
    /// paired; the CLI hears it already.
    HearIt,
    /// The big games the tour never reaches: the Games hub's doors and the
    /// Lobby's tables. To everyone once.
    BigGames,
    /// The People page the tour never reaches: the map, the work cards
    /// and the job shelf. To everyone once.
    People,
}

impl Moment {
    /// The first moment of the series.
    const FIRST: Moment = Moment::HearIt;
    /// The last moment of the series; a mark on it is a spent series.
    pub const LAST: Moment = Moment::People;

    /// The moment's number in the series, the form the account's mark
    /// stores (`ComebackMark::step`) and `/comeback N` takes.
    pub fn step(self) -> i32 {
        match self {
            Moment::HearIt => 1,
            Moment::BigGames => 2,
            Moment::People => 3,
        }
    }

    /// The moment with this number, `None` past the series.
    pub fn from_step(step: i32) -> Option<Self> {
        match step {
            1 => Some(Moment::HearIt),
            2 => Some(Moment::BigGames),
            3 => Some(Moment::People),
            _ => None,
        }
    }

    /// The moment after this one; `None` closes the series.
    fn next(self) -> Option<Self> {
        match self {
            Moment::HearIt => Some(Moment::BigGames),
            Moment::BigGames => Some(Moment::People),
            Moment::People => None,
        }
    }

    /// Whether the moment has anything to sell this session.
    fn has_something_to_say(self, standing: Standing) -> bool {
        match self {
            Moment::HearIt => !standing.hears,
            Moment::BigGames => true,
            Moment::People => true,
        }
    }
}

/// What the session already has of what the moments sell, read when the
/// opening sequence is over (the CLI pairs after the session stands, so
/// not at `App::new`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    /// A client is paired on the session's token: the radio can be heard.
    pub hears: bool,
}

/// How the session arrived. Only a login can owe a moment: a session with
/// the tour to walk owes none, and a drain handoff is not a login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    Login(Option<ComebackMark>),
    NotALogin,
}

/// Why the box opened. A login counts and stamps the account; `/comeback N`
/// does neither, it is a rerun like `/onboard`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    Login,
    Command,
}

/// What a login did with a moment. `pub` because `metrics` counts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beat {
    /// The box opened.
    Shown,
    /// Enter: the moment's own action.
    Taken,
    /// Esc: later.
    Skipped,
}

/// The moment a login is owed. The series starts on the first login of a
/// day after the account's creation day (the tour's day stays quiet) and
/// moves on at the first login of a day after the last moment was shown;
/// nothing twice in a day, nothing past the series, and a mark with a
/// step the series no longer has closes it. From where the series stands,
/// the first moment with something to say for this session is the one
/// owed; the ones with nothing to say are passed over, not spent.
pub fn due_moment(
    mark: Option<ComebackMark>,
    account_created: NaiveDate,
    today: NaiveDate,
    standing: Standing,
) -> Option<Moment> {
    let mut candidate = match mark {
        None => match account_created < today {
            true => Some(Moment::FIRST),
            false => None,
        },
        Some(mark) => match (mark.day < today, Moment::from_step(mark.step)) {
            (true, Some(last)) => last.next(),
            (true, None) | (false, _) => None,
        },
    };
    while let Some(moment) = candidate {
        if moment.has_something_to_say(standing) {
            return Some(moment);
        }
        candidate = moment.next();
    }
    None
}

/// `/comeback N`: `None` when the body is some other command, `Some(None)`
/// when it is `/comeback` without a moment's number.
pub(crate) fn parse_command(body: &str) -> Option<Option<Moment>> {
    let rest = body.trim().strip_prefix("/comeback")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    Some(match words.as_slice() {
        [step] => step.parse::<i32>().ok().and_then(Moment::from_step),
        [] | [_, _, ..] => None,
    })
}

/// The session's box: how it arrived, until the opening sequence is over
/// and the gate is read, and the moment open.
#[derive(Debug)]
pub struct State {
    arrival: Option<Arrival>,
    open: Option<(Moment, Start)>,
}

impl State {
    pub fn new(arrival: Arrival) -> Self {
        Self {
            arrival: Some(arrival),
            open: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The moment on screen.
    pub fn open(&self) -> Option<Moment> {
        self.open.map(|(moment, _)| moment)
    }

    pub(super) fn take_arrival(&mut self) -> Option<Arrival> {
        self.arrival.take()
    }

    pub(super) fn show(&mut self, moment: Moment, start: Start) {
        self.open = Some((moment, start));
    }

    pub(super) fn close(&mut self) -> Option<(Moment, Start)> {
        self.open.take()
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
