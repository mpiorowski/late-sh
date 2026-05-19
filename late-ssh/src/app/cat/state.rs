use chrono::{DateTime, Utc};
use late_core::models::cat::CatCompanion;
use uuid::Uuid;

use super::svc::CatService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatMood {
    Happy,
    Content,
    Bored,
    Hungry,
    Thirsty,
    Sad,
}

impl CatMood {
    pub fn label(self) -> &'static str {
        match self {
            CatMood::Happy => "happy",
            CatMood::Content => "content",
            CatMood::Bored => "bored",
            CatMood::Hungry => "hungry",
            CatMood::Thirsty => "thirsty",
            CatMood::Sad => "sad",
        }
    }

    pub fn eyes(self) -> &'static str {
        match self {
            CatMood::Happy => "^.^",
            CatMood::Content => "o.o",
            CatMood::Bored => "-.o",
            CatMood::Hungry => ">.>",
            CatMood::Thirsty => "o_o",
            CatMood::Sad => "T.T",
        }
    }

    pub fn from_hours(hours: i64) -> CatMood {
        match hours {
            ..=6 => CatMood::Happy,
            7..=12 => CatMood::Content,
            13..=24 => CatMood::Bored,
            25..=48 => CatMood::Hungry,
            49..=72 => CatMood::Thirsty,
            _ => CatMood::Sad,
        }
    }
}

pub struct CatState {
    pub user_id: Uuid,
    pub svc: CatService,

    pub last_fed: Option<DateTime<Utc>>,
    pub last_watered: Option<DateTime<Utc>>,
    pub last_played: Option<DateTime<Utc>>,

    pub action_feedback: Option<&'static str>,
    feedback_ticks: usize,
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl CatState {
    pub fn new(user_id: Uuid, svc: CatService, companion: CatCompanion) -> Self {
        Self {
            user_id,
            svc,
            last_fed: companion.last_fed,
            last_watered: companion.last_watered,
            last_played: companion.last_played,
            action_feedback: None,
            feedback_ticks: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.action_feedback.is_some() {
            self.feedback_ticks = self.feedback_ticks.saturating_sub(1);
            if self.feedback_ticks == 0 {
                self.action_feedback = None;
            }
        }
    }

    pub fn mood(&self) -> CatMood {
        CatMood::from_hours(self.hours_since_last_care())
    }

    pub fn feed(&mut self) {
        self.last_fed = Some(Utc::now());
        self.action_feedback = Some("fed!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.feed_task(self.user_id);
    }

    pub fn water(&mut self) {
        self.last_watered = Some(Utc::now());
        self.action_feedback = Some("watered!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.water_task(self.user_id);
    }

    pub fn play(&mut self) {
        self.last_played = Some(Utc::now());
        self.action_feedback = Some("played!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.play_task(self.user_id);
    }

    fn hours_since_last_care(&self) -> i64 {
        let now = Utc::now();
        let last = [self.last_fed, self.last_watered, self.last_played]
            .iter()
            .flatten()
            .max()
            .copied();
        last.map(|t| (now - t).num_hours()).unwrap_or(999)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mood_maps_hours_to_each_band() {
        assert_eq!(CatMood::from_hours(0), CatMood::Happy);
        assert_eq!(CatMood::from_hours(6), CatMood::Happy);
        assert_eq!(CatMood::from_hours(7), CatMood::Content);
        assert_eq!(CatMood::from_hours(12), CatMood::Content);
        assert_eq!(CatMood::from_hours(13), CatMood::Bored);
        assert_eq!(CatMood::from_hours(24), CatMood::Bored);
        assert_eq!(CatMood::from_hours(25), CatMood::Hungry);
        assert_eq!(CatMood::from_hours(48), CatMood::Hungry);
        assert_eq!(CatMood::from_hours(49), CatMood::Thirsty);
        assert_eq!(CatMood::from_hours(72), CatMood::Thirsty);
        assert_eq!(CatMood::from_hours(73), CatMood::Sad);
        assert_eq!(CatMood::from_hours(999), CatMood::Sad);
    }

    #[test]
    fn negative_hours_from_clock_skew_stay_happy_not_sad() {
        assert_eq!(CatMood::from_hours(-3), CatMood::Happy);
    }

    #[test]
    fn worst_mood_is_sad_never_dead() {
        // The cat never dies — the lowest mood it can reach is Sad.
        assert_eq!(CatMood::from_hours(i64::MAX), CatMood::Sad);
    }
}
