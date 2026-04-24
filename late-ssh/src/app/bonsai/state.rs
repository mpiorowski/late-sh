use chrono::NaiveDate;
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use late_core::models::bonsai::{MAX_GROWTH_POINTS, Tree};

use super::svc::BonsaiService;

pub(crate) const STAGE_GROWTH_POINTS: i32 = 100;
pub(crate) const WRONG_CUT_GROWTH_LOSS: i32 = 10;

/// How many ticks between passive growth grants (1 point per ~10 minutes at 15fps)
const GROWTH_TICK_INTERVAL: usize = 15 * 60 * 10; // 15fps * 600s = 9000 ticks

pub struct BonsaiState {
    pub user_id: Uuid,
    pub svc: BonsaiService,
    pub is_admin: bool,

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
    pub fn new(user_id: Uuid, svc: BonsaiService, tree: Tree, is_admin: bool) -> Self {
        let today = chrono::Utc::now().date_naive();
        let created_date = tree.created.date_naive();
        let age_days = (today - created_date).num_days().max(0);

        Self {
            user_id,
            svc,
            is_admin,
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
            if self.add_growth_locally(1) > 0 {
                self.svc.add_growth_task(self.user_id, 1);
            }
        }
    }

    /// Water the tree. Returns points granted (0 if already watered today or dead).
    pub fn water(&mut self) -> i32 {
        if !self.is_alive {
            return 0;
        }
        let today = BonsaiService::today();
        if !self.is_admin && self.last_watered == Some(today) {
            return 0; // Already watered
        }

        let bonus = if let Some(last) = self.last_watered {
            if (today - last).num_days() == 1 { 5 } else { 0 }
        } else {
            0
        };
        let gained = self.add_growth_locally(10 + bonus);
        self.last_watered = Some(today);
        self.watered_this_session = true;

        self.svc.water_task(self.user_id, self.is_admin);
        gained
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
        can_water_on(
            self.is_alive,
            self.last_watered,
            BonsaiService::today(),
            self.is_admin,
        )
    }

    /// Cut/prune the tree — drops one growth stage, changes visual variant.
    /// Returns true if cut happened.
    pub fn cut(&mut self) -> bool {
        if !self.is_alive || self.growth_points < STAGE_GROWTH_POINTS {
            return false;
        }
        let cost = STAGE_GROWTH_POINTS;
        self.growth_points -= cost;
        self.seed = OsRng.next_u64() as i64;
        self.svc.cut_task(self.user_id, self.seed, cost);
        true
    }

    pub(crate) fn cut_daily_branch(&mut self, branch_id: i32) {
        self.svc
            .cut_daily_branch_task(self.user_id, BonsaiService::today(), branch_id);
    }

    pub(crate) fn reset_daily_branches(&mut self) {
        self.svc
            .clear_daily_branches_task(self.user_id, BonsaiService::today());
    }

    pub(crate) fn punish_wrong_cut(&mut self) -> i32 {
        if !self.is_alive || self.growth_points <= 0 {
            return 0;
        }
        let loss = self.growth_points.min(WRONG_CUT_GROWTH_LOSS);
        self.growth_points -= loss;
        self.svc.lose_growth_task(self.user_id, loss);
        loss
    }

    fn add_growth_locally(&mut self, points: i32) -> i32 {
        if points <= 0 || self.growth_points >= MAX_GROWTH_POINTS {
            return 0;
        }
        let before = self.growth_points;
        self.growth_points = (self.growth_points + points).min(MAX_GROWTH_POINTS);
        self.growth_points - before
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
        0..=99 => Stage::Seed,
        100..=199 => Stage::Sprout,
        200..=299 => Stage::Sapling,
        300..=399 => Stage::Young,
        400..=499 => Stage::Mature,
        500..=599 => Stage::Ancient,
        _ => Stage::Blossom,
    }
}

fn days_since_watered_on(last_watered: Option<NaiveDate>, today: NaiveDate) -> Option<i64> {
    last_watered.map(|date| (today - date).num_days())
}

fn is_wilting_state(is_alive: bool, age_days: i64, days_since_watered: Option<i64>) -> bool {
    is_alive && days_since_watered.map_or(age_days >= 2, |days| days >= 2)
}

fn can_water_on(
    is_alive: bool,
    last_watered: Option<NaiveDate>,
    today: NaiveDate,
    is_admin: bool,
) -> bool {
    is_alive && (is_admin || last_watered != Some(today))
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
    Seed,    // 0-99 pts
    Sprout,  // 100-199 pts
    Sapling, // 200-299 pts
    Young,   // 300-399 pts
    Mature,  // 400-499 pts
    Ancient, // 500-599 pts
    Blossom, // 600-700 pts
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
            Stage::Sapling => "\u{1f331}", // 🌱
            Stage::Young => "\u{1f332}",   // 🌲
            Stage::Mature => "\u{1f333}",  // 🌳
            Stage::Ancient => "\u{1f338}", // 🌸
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
            (true, 99, Stage::Seed),
            (true, 100, Stage::Sprout),
            (true, 199, Stage::Sprout),
            (true, 200, Stage::Sapling),
            (true, 299, Stage::Sapling),
            (true, 300, Stage::Young),
            (true, 399, Stage::Young),
            (true, 400, Stage::Mature),
            (true, 499, Stage::Mature),
            (true, 500, Stage::Ancient),
            (true, 599, Stage::Ancient),
            (true, 600, Stage::Blossom),
            (true, 700, Stage::Blossom),
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
        assert!(can_water_on(true, None, today, false));

        assert_eq!(days_since_watered_on(Some(today), today), Some(0));
        assert!(!can_water_on(true, Some(today), today, false));
        assert!(can_water_on(true, Some(today), today, true));

        assert_eq!(
            days_since_watered_on(Some(today - Duration::days(1)), today),
            Some(1)
        );
        assert!(can_water_on(
            true,
            Some(today - Duration::days(1)),
            today,
            false
        ));
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
