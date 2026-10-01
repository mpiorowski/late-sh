//! Per-frame inputs for the status bar, and the value each component paints.
//!
//! Everything here is a pure function of `StatusData`. In particular the clock
//! arrives pre-formatted: the draw path reads no wall clock, so a frame is
//! reproducible from its inputs and the bar builder stays testable without a
//! time source.

use late_core::models::statusline::{StatusComponent, StatusVariant};

use crate::app::common::primitives::thousands;

/// Everything the bar can show this frame, gathered once in `App::render`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StatusData<'a> {
    /// `HH:MM`, 24-hour.
    pub clock_24: &'a str,
    /// `H:MM am` / `H:MM pm`.
    pub clock_ampm: &'a str,
    /// Local hour, 0..=23, for the hour-dependent clock icon.
    pub hour: u32,
    pub chip_balance: i64,
    pub mentions_unread: i64,
    pub dms_unread: i64,
    /// Open raffle pot size. `None` before refresh or while the pot is closed.
    pub pot_size: Option<i64>,
    /// Compact time until the next draw, paired with `pot_size` when available.
    pub pot_draws_in: Option<&'a str>,
    /// Humans currently connected, bots excluded.
    pub online_count: usize,
    /// Daily correspondence matches waiting on this user's move.
    pub turns_waiting: usize,
    /// Display name of the audio source the user is listening to. Every
    /// source has one, which is why the station segment never reads inactive.
    pub station_name: &'a str,
    /// Live `Artist - Title` for that source, when the metadata feed has one.
    pub station_track: Option<&'a str>,
    pub quests_open_daily: usize,
    pub quests_open_weekly: usize,
    /// Owned companions (bonsai, tank, pet) still waiting on today's care.
    pub care_due: usize,
    /// The voice badge body, `channel [status]`; `None` when not in a room.
    pub voice: Option<&'a str>,
}

/// The hour hand matching `hour`, so the clock icon reads as the current time
/// rather than as generic decoration. Only the twelve on-the-hour faces are
/// used: half-hour faces would need the minute too, and swapping the glyph
/// twice an hour buys nothing at this size.
pub(crate) fn clock_icon(hour: u32) -> &'static str {
    const FACES: [&str; 12] = [
        "🕛", "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚",
    ];
    FACES[(hour % 12) as usize]
}

impl<'a> StatusData<'a> {
    /// The text this component paints, or `None` when it has nothing to say.
    ///
    /// `None` is what drives auto-hide: a component that returns `None` is
    /// *inactive*, and the caller decides whether that means hiding the
    /// segment or painting a zero.
    pub(crate) fn value(
        &self,
        component: StatusComponent,
        variant: Option<StatusVariant>,
    ) -> Option<String> {
        match component {
            // Built directly by `bar::shortcut_spans`: it is styled help copy,
            // not a value/label status reading.
            StatusComponent::Shortcuts => None,
            StatusComponent::Time => Some(
                match clock_format(variant) {
                    ClockFormat::AmPm => self.clock_ampm,
                    ClockFormat::H24 => self.clock_24,
                }
                .to_string(),
            ),
            StatusComponent::Chips => Some(self.chip_balance.to_string()),
            StatusComponent::Mentions => {
                let count = match counts_dms(variant) {
                    true => self.mentions_unread.saturating_add(self.dms_unread),
                    false => self.mentions_unread,
                };
                (count > 0).then(|| count.to_string())
            }
            StatusComponent::Pot => self.pot_size.map(|size| {
                let size = thousands(size);
                self.pot_draws_in
                    .filter(|draws_in| !draws_in.is_empty())
                    .map_or_else(|| size.clone(), |draws_in| format!("{size} · {draws_in}"))
            }),
            StatusComponent::Users => Some(self.online_count.to_string()),
            StatusComponent::Turns => {
                (self.turns_waiting > 0).then(|| self.turns_waiting.to_string())
            }
            StatusComponent::Station => Some(
                match shows_track(variant) {
                    // Track first, station as the fallback: a source with no
                    // live metadata still names itself rather than going blank.
                    true => self.station_track.unwrap_or(self.station_name),
                    false => self.station_name,
                }
                .to_string(),
            ),
            StatusComponent::Quests => {
                let count = match counts_weekly(variant) {
                    true => self
                        .quests_open_daily
                        .saturating_add(self.quests_open_weekly),
                    false => self.quests_open_daily,
                };
                (count > 0).then(|| count.to_string())
            }
            StatusComponent::Care => (self.care_due > 0).then(|| self.care_due.to_string()),
            StatusComponent::Voice => self.voice.map(str::to_string),
        }
    }
}

// Each dial is read through one exhaustive match, so a new `StatusVariant`
// breaks the build here instead of silently painting the default. An absent
// dial reads as the component's default. Another component's dial cannot reach
// these: `normalize_statusline_components` drops it at the boundary.

enum ClockFormat {
    H24,
    AmPm,
}

fn clock_format(variant: Option<StatusVariant>) -> ClockFormat {
    match variant {
        Some(StatusVariant::ClockAmPm) => ClockFormat::AmPm,
        Some(StatusVariant::Clock24) | None => ClockFormat::H24,
        Some(
            StatusVariant::MentionsOnly
            | StatusVariant::MentionsAndDms
            | StatusVariant::QuestsDaily
            | StatusVariant::QuestsDailyWeekly
            | StatusVariant::StationName
            | StatusVariant::StationTrack,
        ) => unreachable!("the clock carries a clock dial"),
    }
}

fn counts_dms(variant: Option<StatusVariant>) -> bool {
    match variant {
        Some(StatusVariant::MentionsAndDms) | None => true,
        Some(StatusVariant::MentionsOnly) => false,
        Some(
            StatusVariant::Clock24
            | StatusVariant::ClockAmPm
            | StatusVariant::QuestsDaily
            | StatusVariant::QuestsDailyWeekly
            | StatusVariant::StationName
            | StatusVariant::StationTrack,
        ) => unreachable!("mentions carry a mentions dial"),
    }
}

fn counts_weekly(variant: Option<StatusVariant>) -> bool {
    match variant {
        Some(StatusVariant::QuestsDailyWeekly) => true,
        Some(StatusVariant::QuestsDaily) | None => false,
        Some(
            StatusVariant::Clock24
            | StatusVariant::ClockAmPm
            | StatusVariant::MentionsOnly
            | StatusVariant::MentionsAndDms
            | StatusVariant::StationName
            | StatusVariant::StationTrack,
        ) => unreachable!("quests carry a quests dial"),
    }
}

fn shows_track(variant: Option<StatusVariant>) -> bool {
    match variant {
        Some(StatusVariant::StationTrack) => true,
        Some(StatusVariant::StationName) | None => false,
        Some(
            StatusVariant::Clock24
            | StatusVariant::ClockAmPm
            | StatusVariant::MentionsOnly
            | StatusVariant::MentionsAndDms
            | StatusVariant::QuestsDaily
            | StatusVariant::QuestsDailyWeekly,
        ) => unreachable!("the station carries a station dial"),
    }
}
