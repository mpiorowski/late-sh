use chrono::{DateTime, Utc};
use late_core::models::goldfish::{GoldfishBowl, MAX_FRIENDS};
use uuid::Uuid;

use super::svc::GoldfishService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldfishMood {
    Happy,
    Content,
    Bored,
    Hungry,
    Dirty,
    Sad,
}

impl GoldfishMood {
    pub fn label(self) -> &'static str {
        match self {
            GoldfishMood::Happy => "happy",
            GoldfishMood::Content => "content",
            GoldfishMood::Bored => "bored",
            GoldfishMood::Hungry => "hungry",
            GoldfishMood::Dirty => "dirty water",
            GoldfishMood::Sad => "sad",
        }
    }

    pub fn eye(self) -> char {
        match self {
            GoldfishMood::Happy | GoldfishMood::Content => '°',
            GoldfishMood::Bored => '-',
            GoldfishMood::Hungry => '>',
            GoldfishMood::Dirty => 'o',
            GoldfishMood::Sad => '\'',
        }
    }
}

pub struct GoldfishState {
    pub user_id: Uuid,
    pub svc: GoldfishService,

    pub last_fed: Option<DateTime<Utc>>,
    pub last_decorated: Option<DateTime<Utc>>,
    pub last_lit: Option<DateTime<Utc>>,
    pub last_water_changed: Option<DateTime<Utc>>,
    pub friend_count: i32,

    pub action_feedback: Option<&'static str>,
    feedback_ticks: usize,
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl GoldfishState {
    pub fn new(user_id: Uuid, svc: GoldfishService, bowl: GoldfishBowl) -> Self {
        Self {
            user_id,
            svc,
            last_fed: bowl.last_fed,
            last_decorated: bowl.last_decorated,
            last_lit: bowl.last_lit,
            last_water_changed: bowl.last_water_changed,
            friend_count: bowl.friend_count,
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

    pub fn mood(&self) -> GoldfishMood {
        let hours = self.hours_since_last_care();
        match hours {
            0..=6 => GoldfishMood::Happy,
            7..=12 => GoldfishMood::Content,
            13..=24 => GoldfishMood::Bored,
            25..=48 => GoldfishMood::Hungry,
            49..=72 => GoldfishMood::Dirty,
            _ => GoldfishMood::Sad,
        }
    }

    pub fn feed(&mut self) {
        self.last_fed = Some(Utc::now());
        self.action_feedback = Some("fed!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.feed_task(self.user_id);
    }

    pub fn decorate(&mut self) {
        self.last_decorated = Some(Utc::now());
        self.action_feedback = Some("decorated!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.decorate_task(self.user_id);
    }

    pub fn light(&mut self) {
        self.last_lit = Some(Utc::now());
        self.action_feedback = Some("lights adjusted!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.light_task(self.user_id);
    }

    pub fn change_water(&mut self) {
        self.last_water_changed = Some(Utc::now());
        self.action_feedback = Some("water changed!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.change_water_task(self.user_id);
    }

    pub fn add_friend(&mut self) {
        if self.friend_count >= MAX_FRIENDS {
            self.action_feedback = Some("bowl is full!");
            self.feedback_ticks = FEEDBACK_TICKS;
            return;
        }
        self.friend_count += 1;
        self.action_feedback = Some("new friend!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.add_friend_task(self.user_id);
    }

    fn hours_since_last_care(&self) -> i64 {
        let now = Utc::now();
        let last = [
            self.last_fed,
            self.last_decorated,
            self.last_lit,
            self.last_water_changed,
        ]
        .iter()
        .flatten()
        .max()
        .copied();
        last.map(|t| (now - t).num_hours()).unwrap_or(999)
    }
}
