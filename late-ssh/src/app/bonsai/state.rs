use chrono::NaiveDate;
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use late_core::models::bonsai::Tree;

use super::svc::BonsaiService;

/// How many ticks between passive growth grants (1 point per ~10 minutes at 15fps)
const GROWTH_TICK_INTERVAL: usize = 15 * 60 * 10; // 15fps * 600s = 9000 ticks

pub struct BonsaiState {
    pub user_id: Uuid,
    pub svc: BonsaiService,

    // Cached tree state (refreshed on water/respawn)
    pub growth_points: i32,
    pub last_watered: Option<NaiveDate>,
    pub seed: i64,
    pub is_alive: bool,
    pub age_days: i64,
    pub created_date: NaiveDate,

    // Tick counter for passive growth
    ticks_since_growth: usize,

    // Whether water was pressed this session (for UI feedback)
    pub watered_this_session: bool,
}

impl BonsaiState {
    pub fn new(user_id: Uuid, svc: BonsaiService, tree: Tree) -> Self {
        let today = chrono::Utc::now().date_naive();
        let created_date = tree.created.date_naive();
        let age_days = (today - created_date).num_days().max(0);

        Self {
            user_id,
            svc,
            growth_points: tree.growth_points,
            last_watered: tree.last_watered,
            seed: tree.seed,
            is_alive: tree.is_alive,
            age_days,
            created_date,
            ticks_since_growth: 0,
            watered_this_session: false,
        }
    }

    pub fn tick(&mut self) {
        if !self.is_alive {
            return;
        }

        // Check death during live session (not just on login)
        let reference_date = self.last_watered.unwrap_or(self.created_date);
        if should_die(reference_date, BonsaiService::today()) {
            self.is_alive = false;
            // Fire-and-forget: the next login will also detect this and record the graveyard entry
            return;
        }

        self.ticks_since_growth += 1;
        if self.ticks_since_growth >= GROWTH_TICK_INTERVAL {
            self.ticks_since_growth = 0;
            self.growth_points += 1;
            self.svc.add_growth_task(self.user_id, 1);
        }
    }

    /// Water the tree. Returns true if watering is valid (alive + not yet watered today).
    pub fn water(&mut self) -> bool {
        if !self.is_alive {
            return false;
        }
        let today = BonsaiService::today();
        if self.last_watered == Some(today) {
            return false; // Already watered
        }

        // Optimistic update
        let bonus = if let Some(last) = self.last_watered {
            if (today - last).num_days() == 1 { 5 } else { 0 }
        } else {
            0
        };
        self.growth_points += 10 + bonus;
        self.last_watered = Some(today);
        self.watered_this_session = true;

        self.svc.water_task(self.user_id);
        true
    }

    /// Respawn a dead tree
    pub fn respawn(&mut self) {
        if self.is_alive {
            return;
        }
        self.is_alive = true;
        self.growth_points = 0;
        self.last_watered = None;
        self.seed = OsRng.next_u64() as i64;
        self.created_date = chrono::Utc::now().date_naive();
        self.age_days = 0;
        self.watered_this_session = false;
        self.svc.respawn_task(self.user_id);
    }

    /// Growth stage based on total growth points
    pub fn stage(&self) -> Stage {
        stage_for(self.is_alive, self.growth_points)
    }

    /// Days since last watered (None if never watered)
    pub fn days_since_watered(&self) -> Option<i64> {
        days_since_watered_on(self.last_watered, BonsaiService::today())
    }

    /// Whether the tree is currently wilting (2+ days without water)
    pub fn is_wilting(&self) -> bool {
        is_wilting_state(self.is_alive, self.age_days, self.days_since_watered())
    }

    /// Can water right now?
    pub fn can_water(&self) -> bool {
        can_water_on(self.is_alive, self.last_watered, BonsaiService::today())
    }

    /// Cut/prune the tree — costs 20% growth points, changes visual variant.
    /// Returns true if cut happened.
    pub fn cut(&mut self) -> bool {
        if !self.is_alive || self.growth_points < 10 {
            return false;
        }
        let cost = (self.growth_points as f64 * 0.2).ceil() as i32;
        self.growth_points -= cost;
        self.seed = OsRng.next_u64() as i64;
        self.svc.cut_task(self.user_id, self.seed, cost);
        true
    }

    /// ASCII snippet for sharing (plain text, no ANSI)
    pub fn share_snippet(&self) -> String {
        let art = share_art(self.stage(), self.seed);
        let label = share_label(self.is_alive, self.age_days);
        format!("{art}\n{label}")
    }
}

fn should_die(reference_date: NaiveDate, today: NaiveDate) -> bool {
    (today - reference_date).num_days() >= 7
}

pub fn stage_for(is_alive: bool, growth_points: i32) -> Stage {
    if !is_alive {
        return Stage::Dead;
    }
    match growth_points {
        0..=9 => Stage::Seed,
        10..=29 => Stage::Sprout,
        30..=69 => Stage::Sapling,
        70..=139 => Stage::Young,
        140..=279 => Stage::Mature,
        280..=499 => Stage::Ancient,
        _ => Stage::Blossom,
    }
}

fn days_since_watered_on(last_watered: Option<NaiveDate>, today: NaiveDate) -> Option<i64> {
    last_watered.map(|date| (today - date).num_days())
}

fn is_wilting_state(is_alive: bool, age_days: i64, days_since_watered: Option<i64>) -> bool {
    is_alive && days_since_watered.map_or(age_days >= 2, |days| days >= 2)
}

fn can_water_on(is_alive: bool, last_watered: Option<NaiveDate>, today: NaiveDate) -> bool {
    is_alive && last_watered != Some(today)
}

fn share_label(is_alive: bool, age_days: i64) -> String {
    if is_alive {
        format!("ADMIRE my tree (Day {age_days})")
    } else {
        "ADMIRE my tree [RIP]".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Dead,
    Seed,    // 0-9 pts
    Sprout,  // 10-29 pts (~1-3 days)
    Sapling, // 30-69 pts (~3-7 days)
    Young,   // 70-139 pts (~7-14 days)
    Mature,  // 140-279 pts (~14-28 days)
    Ancient, // 280-499 pts (~28-50 days)
    Blossom, // 500+ pts (~50+ days)
}

impl Stage {
    pub fn label(&self) -> &'static str {
        match self {
            Stage::Dead => "Dead",
            Stage::Seed => "Seed",
            Stage::Sprout => "Sprout",
            Stage::Sapling => "Sapling",
            Stage::Young => "Young Tree",
            Stage::Mature => "Mature",
            Stage::Ancient => "Ancient",
            Stage::Blossom => "Blossom",
        }
    }

    /// Small glyph for chat badge display
    pub fn glyph(&self) -> &'static str {
        match self {
            Stage::Dead => "",
            Stage::Seed => "\u{00b7}",     // ·
            Stage::Sprout => "\u{2698}",   // ⚘
            Stage::Sapling => "\u{2698}",  // ⚘
            Stage::Young => "\u{1f332}",   // 🌲
            Stage::Mature => "\u{1f333}",  // 🌳
            Stage::Ancient => "\u{1f333}", // 🌳
            Stage::Blossom => "\u{1f33C}", // 🌼
        }
    }
}

/// Compact ASCII art for clipboard sharing (no ANSI codes).
/// Derives from the same `tree_ascii` used by the UI so the two never drift.
fn share_art(stage: Stage, seed: i64) -> String {
    let lines = super::ui::tree_ascii(stage, seed, false);
    lines
        .iter()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn stage_thresholds_match_growth_ranges() {
        let cases = [
            (true, 0, Stage::Seed),
            (true, 9, Stage::Seed),
            (true, 10, Stage::Sprout),
            (true, 29, Stage::Sprout),
            (true, 30, Stage::Sapling),
            (true, 69, Stage::Sapling),
            (true, 70, Stage::Young),
            (true, 139, Stage::Young),
            (true, 140, Stage::Mature),
            (true, 279, Stage::Mature),
            (true, 280, Stage::Ancient),
            (true, 499, Stage::Ancient),
            (true, 500, Stage::Blossom),
            (false, 999, Stage::Dead),
        ];

        for (is_alive, growth_points, expected) in cases {
            assert_eq!(stage_for(is_alive, growth_points), expected);
        }
    }

    #[test]
    fn can_water_and_days_since_watered_track_today() {
        let today = BonsaiService::today();

        assert_eq!(days_since_watered_on(None, today), None);
        assert!(can_water_on(true, None, today));

        assert_eq!(days_since_watered_on(Some(today), today), Some(0));
        assert!(!can_water_on(true, Some(today), today));

        assert_eq!(
            days_since_watered_on(Some(today - Duration::days(1)), today),
            Some(1)
        );
        assert!(can_water_on(true, Some(today - Duration::days(1)), today));
    }

    #[test]
    fn is_wilting_depends_on_age_or_days_since_watered() {
        assert!(!is_wilting_state(true, 1, None));
        assert!(is_wilting_state(true, 2, None));
        assert!(!is_wilting_state(true, 10, Some(1)));
        assert!(is_wilting_state(true, 10, Some(2)));
        assert!(!is_wilting_state(false, 10, Some(5)));
    }

    #[test]
    fn should_die_after_seven_dry_days() {
        let today = BonsaiService::today();
        assert!(!should_die(today - Duration::days(6), today));
        assert!(should_die(today - Duration::days(7), today));
        assert!(should_die(today - Duration::days(20), today));
    }

    #[test]
    fn share_label_reflects_alive_and_dead_states() {
        assert_eq!(share_label(true, 12), "ADMIRE my tree (Day 12)");
        assert_eq!(share_label(false, 12), "ADMIRE my tree [RIP]");
    }
}
