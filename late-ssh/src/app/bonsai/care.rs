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
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub mode: CareMode,
    pub water_animation_ticks: u8,
    pub message: Option<String>,
}

impl BonsaiCareState {
    pub(crate) fn from_daily(care: DailyCare, seed: i64, stage: Stage) -> Self {
        let branch_goal = if care.branch_goal > 0 {
            care.branch_goal as usize
        } else {
            branch_goal_for(stage, seed, care.care_date)
        };
        Self {
            date: care.care_date,
            watered: care.watered,
            cut_branch_ids: care.cut_branch_ids.into_iter().collect(),
            branch_goal,
            cursor_x: 0,
            cursor_y: 0,
            mode: CareMode::Water,
            water_animation_ticks: 0,
            message: None,
        }
    }

    pub(crate) fn fallback(date: NaiveDate, seed: i64, stage: Stage) -> Self {
        Self {
            date,
            watered: false,
            cut_branch_ids: BTreeSet::new(),
            branch_goal: branch_goal_for(stage, seed, date),
            cursor_x: 0,
            cursor_y: 0,
            mode: CareMode::Water,
            water_animation_ticks: 0,
            message: None,
        }
    }

    pub(crate) fn tick(&mut self) {
        self.water_animation_ticks = self.water_animation_ticks.saturating_sub(1);
    }

    pub(crate) fn set_cursor(&mut self, x: usize, y: usize) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    pub(crate) fn move_cursor(&mut self, dx: isize, dy: isize, width: usize, height: usize) {
        if width == 0 || height == 0 {
            self.cursor_x = 0;
            self.cursor_y = 0;
            return;
        }
        let max_x = width.saturating_sub(1) as isize;
        let max_y = height.saturating_sub(1) as isize;
        self.cursor_x = (self.cursor_x as isize + dx).clamp(0, max_x) as usize;
        self.cursor_y = (self.cursor_y as isize + dy).clamp(0, max_y) as usize;
    }

    pub(crate) fn mark_watered(&mut self) {
        self.watered = true;
        self.water_animation_ticks = 18;
        self.message = Some("Water soaked in".to_string());
    }

    pub(crate) fn reset_branch_cuts(&mut self) {
        self.cut_branch_ids.clear();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.mode = CareMode::Prune;
    }

    pub(crate) fn reset_for_respawn(&mut self, seed: i64) {
        self.watered = false;
        self.cut_branch_ids.clear();
        self.branch_goal = branch_goal_for(Stage::Seed, seed, self.date);
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.mode = CareMode::Water;
        self.water_animation_ticks = 0;
    }

    pub(crate) fn cut_at_cursor(&mut self, targets: &[BranchTarget]) -> Option<i32> {
        let Some(target) = targets
            .iter()
            .find(|target| target.x == self.cursor_x && target.y == self.cursor_y)
        else {
            self.message = Some("No wrong branch here".to_string());
            return None;
        };
        if self.cut_branch_ids.insert(target.id) {
            self.message = Some("Clean cut".to_string());
            Some(target.id)
        } else {
            self.message = Some("Already trimmed".to_string());
            None
        }
    }

    pub(crate) fn branches_done(&self) -> usize {
        self.cut_branch_ids.len().min(self.branch_goal)
    }

    pub(crate) fn all_branches_cut(&self) -> bool {
        self.branches_done() >= self.branch_goal
    }
}

pub(crate) fn branch_goal_for(stage: Stage, seed: i64, date: NaiveDate) -> usize {
    let (min, spread) = match stage {
        Stage::Dead => (0, 0),
        Stage::Seed | Stage::Sprout => (1, 1),
        Stage::Sapling => (2, 1),
        Stage::Young | Stage::Mature => (3, 1),
        Stage::Ancient | Stage::Blossom => (4, 1),
    };
    if spread == 0 {
        min
    } else {
        min + (hash_parts(seed, date, 0) as usize % (spread + 1))
    }
}

pub(crate) fn branch_targets_for(
    _stage: Stage,
    seed: i64,
    date: NaiveDate,
    art: &[impl AsRef<str>],
    goal: usize,
) -> Vec<BranchTarget> {
    let mut candidates = Vec::new();
    let rows: Vec<Vec<char>> = art
        .iter()
        .map(|line| line.as_ref().chars().collect())
        .collect();
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
    } else if is_tree_char(below) || is_tree_char(above) {
        Some('|')
    } else {
        None
    }
}

fn is_tree_char(ch: Option<char>) -> bool {
    ch.is_some_and(|ch| {
        matches!(
            ch,
            '/' | '\\' | '|' | '_' | '~' | '@' | '#' | '*' | '.' | ',' | '\'' | 'o' | 'O'
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
#[path = "care_test.rs"]
mod care_test;
