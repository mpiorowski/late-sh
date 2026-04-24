use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
};

use chrono::NaiveDate;
use late_core::models::bonsai::DailyCare;

use super::state::Stage;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum CareMode {
    Water,
    Prune,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct BranchTarget {
    pub id: i32,
    pub x: usize,
    pub y: usize,
    pub glyph: char,
}

#[derive(Debug, Clone)]
pub(crate) struct BonsaiCareState {
    pub date: NaiveDate,
    pub watered: bool,
    pub cut_branch_ids: BTreeSet<i32>,
    pub branch_goal: usize,
    pub cursor: usize,
    pub mode: CareMode,
    pub water_animation_ticks: u8,
    pub message: Option<String>,
}

impl BonsaiCareState {
    pub fn from_daily(care: DailyCare, seed: i64) -> Self {
        let branch_goal = if care.branch_goal > 0 {
            care.branch_goal as usize
        } else {
            branch_goal_for(seed, care.care_date)
        };
        Self {
            date: care.care_date,
            watered: care.watered,
            cut_branch_ids: care.cut_branch_ids.into_iter().collect(),
            branch_goal,
            cursor: 0,
            mode: CareMode::Water,
            water_animation_ticks: 0,
            message: None,
        }
    }

    pub fn fallback(date: NaiveDate, seed: i64) -> Self {
        Self {
            date,
            watered: false,
            cut_branch_ids: BTreeSet::new(),
            branch_goal: branch_goal_for(seed, date),
            cursor: 0,
            mode: CareMode::Water,
            water_animation_ticks: 0,
            message: None,
        }
    }

    pub fn tick(&mut self) {
        self.water_animation_ticks = self.water_animation_ticks.saturating_sub(1);
    }

    pub fn move_cursor(&mut self, delta: isize, target_count: usize) {
        if target_count == 0 {
            self.cursor = 0;
            return;
        }
        let current = self.cursor.min(target_count - 1) as isize;
        self.cursor = (current + delta).rem_euclid(target_count as isize) as usize;
    }

    pub fn mark_watered(&mut self) {
        self.watered = true;
        self.water_animation_ticks = 18;
        self.message = Some("Water soaked in".to_string());
    }

    pub fn reset_branch_cuts(&mut self) {
        self.cut_branch_ids.clear();
        self.cursor = 0;
        self.mode = CareMode::Prune;
    }

    pub fn cut_selected(&mut self, targets: &[BranchTarget]) -> Option<i32> {
        let target = targets.get(self.cursor.min(targets.len().saturating_sub(1)))?;
        if self.cut_branch_ids.insert(target.id) {
            self.message = Some("Clean cut".to_string());
            Some(target.id)
        } else {
            self.message = Some("Already trimmed".to_string());
            None
        }
    }

    pub fn branches_done(&self) -> usize {
        self.cut_branch_ids.len().min(self.branch_goal)
    }

    pub fn all_branches_cut(&self) -> bool {
        self.branches_done() >= self.branch_goal
    }
}

pub(crate) fn branch_goal_for(seed: i64, date: NaiveDate) -> usize {
    3 + (hash_parts(seed, date, 0) as usize % 2)
}

pub(crate) fn branch_targets_for(
    _stage: Stage,
    seed: i64,
    date: NaiveDate,
    art: &[&str],
    goal: usize,
) -> Vec<BranchTarget> {
    let mut candidates = Vec::new();
    let rows: Vec<Vec<char>> = art.iter().map(|line| line.chars().collect()).collect();
    for (y, chars) in rows.iter().enumerate() {
        let line: String = chars.iter().collect();
        if line.contains("[===") {
            continue;
        }
        for (x, ch) in chars.iter().copied().enumerate() {
            if ch != ' ' {
                continue;
            }
            if let Some(glyph) = overgrowth_glyph(&rows, x, y) {
                candidates.push((x, y, glyph));
            }
        }
    }

    if candidates.is_empty() {
        for (y, chars) in rows.iter().enumerate() {
            let line: String = chars.iter().collect();
            if line.contains("[===") {
                continue;
            }
            for (x, ch) in chars.iter().copied().enumerate() {
                if matches!(ch, '/' | '\\' | '|' | '_' | '~') {
                    candidates.push((x, y, ch));
                }
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    let mut targets = Vec::new();
    let goal = goal.min(candidates.len()).max(1);
    let mut used = BTreeSet::new();
    for id in 0..goal {
        let mut idx = hash_parts(seed, date, id as u64 + 1) as usize % candidates.len();
        while used.contains(&idx) {
            idx = (idx + 1) % candidates.len();
        }
        used.insert(idx);
        let (x, y, glyph) = candidates[idx];
        targets.push(BranchTarget {
            id: id as i32,
            x,
            y,
            glyph,
        });
    }
    targets
}

fn overgrowth_glyph(rows: &[Vec<char>], x: usize, y: usize) -> Option<char> {
    let left = x
        .checked_sub(1)
        .and_then(|lx| rows.get(y).and_then(|row| row.get(lx)))
        .copied();
    let right = rows.get(y).and_then(|row| row.get(x + 1)).copied();
    let below = rows.get(y + 1).and_then(|row| row.get(x)).copied();
    let above = y
        .checked_sub(1)
        .and_then(|ay| rows.get(ay).and_then(|row| row.get(x)))
        .copied();

    if is_tree_char(left) {
        Some('\\')
    } else if is_tree_char(right) {
        Some('/')
    } else if is_tree_char(below) {
        Some('|')
    } else if is_tree_char(above) {
        Some('|')
    } else {
        None
    }
}

fn is_tree_char(ch: Option<char>) -> bool {
    ch.is_some_and(|ch| {
        matches!(
            ch,
            '/' | '\\' | '|' | '_' | '~' | '@' | '#' | '*' | '.' | ','
        )
    })
}

fn hash_parts(seed: i64, date: NaiveDate, salt: u64) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    date.hash(&mut hasher);
    salt.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_goal_is_daily_three_or_four() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 24).unwrap();
        let goal = branch_goal_for(42, date);
        assert!((3..=4).contains(&goal));
        assert_eq!(goal, branch_goal_for(42, date));
    }

    #[test]
    fn cut_selected_records_branch_once() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 24).unwrap();
        let mut state = BonsaiCareState::fallback(date, 42);
        let targets = [BranchTarget {
            id: 7,
            x: 1,
            y: 1,
            glyph: '/',
        }];

        assert_eq!(state.cut_selected(&targets), Some(7));
        assert_eq!(state.cut_selected(&targets), None);
        assert_eq!(state.branches_done(), 1);
    }
}
