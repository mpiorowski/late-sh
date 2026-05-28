use std::{
    collections::{BTreeSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use chrono::NaiveDate;
use late_core::models::bonsai::{BonsaiV2Tree, BonsaiV2TreeParams};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::bonsai::svc::BonsaiService;

const PASSIVE_GROWTH_TICK_INTERVAL: usize = 15 * 60 * 12;
const MAX_BRANCHES: usize = 96;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum BonsaiV2Mode {
    Inspect,
    Wire,
}

impl BonsaiV2Mode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Wire => "wire",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "wire" => Self::Wire,
            _ => Self::Inspect,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BranchStatus {
    Growing,
    Wired,
    Cut,
    Deadwood,
    LeafPad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Branch {
    pub id: i32,
    pub parent_id: Option<i32>,
    pub start_x: i16,
    pub start_y: i16,
    pub end_x: i16,
    pub end_y: i16,
    pub thickness: u8,
    pub age: u16,
    pub vigor: i16,
    pub status: BranchStatus,
    pub bend_x: i8,
    pub bend_y: i8,
    pub last_pruned_day: Option<i64>,
}

impl Branch {
    pub(crate) fn is_alive(&self) -> bool {
        !matches!(self.status, BranchStatus::Cut | BranchStatus::Deadwood)
    }

    pub(crate) fn is_tip_candidate(&self) -> bool {
        matches!(self.status, BranchStatus::Growing | BranchStatus::Wired)
    }

    pub(crate) fn length(&self) -> i16 {
        (self.end_x - self.start_x)
            .abs()
            .max((self.end_y - self.start_y).abs())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BonsaiGraph {
    pub version: u16,
    pub next_id: i32,
    pub branches: Vec<Branch>,
}

impl BonsaiGraph {
    fn selected_fallback(&self) -> Option<i32> {
        self.branches
            .iter()
            .filter(|branch| branch.id != 1)
            .find(|branch| branch.is_alive())
            .or_else(|| self.branches.iter().find(|branch| branch.is_alive()))
            .map(|branch| branch.id)
    }

    pub(crate) fn branch(&self, id: i32) -> Option<&Branch> {
        self.branches.iter().find(|branch| branch.id == id)
    }

    fn branch_mut(&mut self, id: i32) -> Option<&mut Branch> {
        self.branches.iter_mut().find(|branch| branch.id == id)
    }

    pub(crate) fn child_ids(&self, id: i32) -> Vec<i32> {
        self.branches
            .iter()
            .filter(|branch| branch.parent_id == Some(id))
            .map(|branch| branch.id)
            .collect()
    }

    pub(crate) fn is_tip(&self, id: i32) -> bool {
        !self
            .branches
            .iter()
            .any(|branch| branch.parent_id == Some(id) && branch.is_alive())
    }

    fn add_branch(
        &mut self,
        parent_id: i32,
        dx: i16,
        dy: i16,
        len: i16,
        thickness: u8,
        vigor: i16,
    ) -> Option<i32> {
        if self.branches.len() >= MAX_BRANCHES {
            return None;
        }
        let parent = self.branch(parent_id)?.clone();
        let id = self.next_id;
        self.next_id += 1;
        let len = len.clamp(1, 5);
        let end_y = (parent.end_y + dy.signum().max(0) * len).max(1);
        self.branches.push(Branch {
            id,
            parent_id: Some(parent_id),
            start_x: parent.end_x,
            start_y: parent.end_y,
            end_x: parent.end_x + dx.signum() * len,
            end_y,
            thickness,
            age: 0,
            vigor,
            status: BranchStatus::Growing,
            bend_x: 0,
            bend_y: 0,
            last_pruned_day: None,
        });
        Some(id)
    }
}

#[derive(Clone)]
pub(crate) struct BonsaiV2State {
    pub user_id: Uuid,
    pub svc: BonsaiService,
    pub seed: i64,
    pub last_watered: Option<NaiveDate>,
    pub is_alive: bool,
    pub vigor: i32,
    pub water_stress: i32,
    pub last_simulated_date: NaiveDate,
    pub age_days: i64,
    pub graph: BonsaiGraph,
    pub selected_branch_id: Option<i32>,
    pub mode: BonsaiV2Mode,
    pub message: Option<String>,
    ticks_since_growth: usize,
}

impl BonsaiV2State {
    pub(crate) fn new(user_id: Uuid, svc: BonsaiService, tree: BonsaiV2Tree) -> Self {
        let today = BonsaiService::today();
        let graph = serde_json::from_value::<BonsaiGraph>(tree.branch_graph.clone())
            .unwrap_or_else(|_| seeded_graph(tree.seed, 0));
        let mut state = Self {
            user_id,
            svc,
            seed: tree.seed,
            last_watered: tree.last_watered,
            is_alive: tree.is_alive,
            vigor: tree.vigor,
            water_stress: tree.water_stress,
            last_simulated_date: tree.last_simulated_date,
            age_days: (today - tree.created.date_naive()).num_days().max(0),
            graph,
            selected_branch_id: tree.selected_branch_id.or_else(|| {
                serde_json::from_value::<BonsaiGraph>(tree.branch_graph)
                    .ok()
                    .and_then(|graph| graph.selected_fallback())
            }),
            mode: BonsaiV2Mode::from_str(&tree.mode),
            message: None,
            ticks_since_growth: 0,
        };
        state.ensure_selection();
        if state.apply_elapsed_days(today) {
            state.persist();
        }
        state
    }

    pub(crate) fn fallback(user_id: Uuid, svc: BonsaiService, seed: i64) -> Self {
        let today = BonsaiService::today();
        let graph = seeded_graph(seed, 0);
        let selected_branch_id = graph.selected_fallback();
        Self {
            user_id,
            svc,
            seed,
            last_watered: None,
            is_alive: true,
            vigor: 70,
            water_stress: 0,
            last_simulated_date: today,
            age_days: 0,
            graph,
            selected_branch_id,
            mode: BonsaiV2Mode::Inspect,
            message: Some("Bonsai V2 is not persisted yet".to_string()),
            ticks_since_growth: 0,
        }
    }

    pub(crate) fn tick(&mut self) {
        if !self.is_alive {
            return;
        }
        self.ticks_since_growth += 1;
        if self.ticks_since_growth < PASSIVE_GROWTH_TICK_INTERVAL {
            return;
        }
        self.ticks_since_growth = 0;
        if self.vigor >= 50 {
            self.grow_once(GrowthCause::Passive);
            self.message = Some("A tip crept outward".to_string());
            self.persist();
        }
    }

    pub(crate) fn water(&mut self) -> bool {
        self.water_inner(false)
    }

    pub(crate) fn admin_water(&mut self) -> bool {
        self.water_inner(true)
    }

    fn water_inner(&mut self, allow_repeat: bool) -> bool {
        let today = BonsaiService::today();
        if !self.is_alive {
            self.respawn();
            return true;
        }
        let water_day = if allow_repeat && self.last_simulated_date > today {
            self.last_simulated_date
        } else {
            today
        };
        let already_watered = self.last_watered == Some(water_day);
        if already_watered && !allow_repeat {
            self.message = Some("Already watered today".to_string());
            return false;
        }
        self.last_watered = Some(water_day);
        if self.last_simulated_date < water_day {
            self.last_simulated_date = water_day;
        }
        self.water_stress = self.water_stress.saturating_sub(35);
        self.vigor = (self.vigor + 18).min(100);
        self.grow_once(GrowthCause::Water);
        self.grow_once(GrowthCause::Water);
        self.message = Some(if already_watered {
            "Admin watered again: vigor pushed new growth".to_string()
        } else {
            "Watered: vigor pushed new growth".to_string()
        });
        self.persist();
        true
    }

    pub(crate) fn admin_advance_days(&mut self, days: usize) {
        if !self.is_alive {
            self.message = Some("Dead trees need water before fast-forward".to_string());
            return;
        }

        let days = days.clamp(1, 30);
        let mut simulated_day = self.last_simulated_date;
        let mut applied = 0usize;
        for _ in 0..days {
            if !self.is_alive {
                break;
            }
            let Some(next_day) = simulated_day.succ_opt() else {
                break;
            };
            simulated_day = next_day;
            self.simulate_day(simulated_day);
            applied += 1;
        }

        if applied == 0 {
            self.message = Some("Admin time could not advance".to_string());
            return;
        }

        self.last_simulated_date = simulated_day;
        self.ensure_selection();
        let suffix = if applied == 1 { "" } else { "s" };
        let outcome = if !self.is_alive {
            "; tree dried out"
        } else if self.water_stress >= 60 {
            "; dry stress rising"
        } else {
            ""
        };
        self.message = Some(format!(
            "Admin time: +{applied} simulated day{suffix}{outcome}"
        ));
        self.persist();
    }

    pub(crate) fn respawn(&mut self) {
        let today = BonsaiService::today();
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.graph = seeded_graph(self.seed, 0);
        self.selected_branch_id = self.graph.selected_fallback();
        self.last_watered = None;
        self.is_alive = true;
        self.vigor = 70;
        self.water_stress = 0;
        self.last_simulated_date = today;
        self.age_days = 0;
        self.mode = BonsaiV2Mode::Inspect;
        self.message = Some("New living graph planted".to_string());
        self.persist();
    }

    pub(crate) fn cycle_selection(&mut self, delta: isize) {
        self.ensure_selection();
        let ids = self.selectable_branch_ids();
        if ids.is_empty() {
            self.selected_branch_id = None;
            return;
        }
        let current = self
            .selected_branch_id
            .and_then(|id| ids.iter().position(|candidate| *candidate == id))
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(ids.len() as isize) as usize;
        self.selected_branch_id = Some(ids[next]);
        if let Some(branch) = self.selected_branch() {
            self.message = Some(format!(
                "Selected branch {}: {}",
                branch.id,
                branch_label(branch)
            ));
        }
        self.persist();
    }

    pub(crate) fn bend_selected(&mut self, dx: i8, dy: i8) {
        let Some(id) = self.selected_branch_id else {
            self.message = Some("No branch selected".to_string());
            return;
        };
        if id == 1 {
            self.message = Some("The trunk remembers, but it will not wire".to_string());
            return;
        }
        if !self.graph.is_tip(id) {
            self.message = Some("Wire a live tip; prune structure branches first".to_string());
            return;
        }
        let Some(branch) = self.graph.branch_mut(id) else {
            self.message = Some("Selected branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        if matches!(branch.status, BranchStatus::Cut | BranchStatus::Deadwood) {
            self.message = Some("Deadwood will not bend".to_string());
            return;
        }
        branch.status = BranchStatus::Wired;
        branch.bend_x = (branch.bend_x + dx).clamp(-3, 3);
        branch.bend_y = (branch.bend_y + dy).clamp(-2, 3);
        let direction = wire_direction_label(branch.bend_x, branch.bend_y);
        self.mode = BonsaiV2Mode::Wire;
        self.message = Some(format!("Wire set: future growth will lean {direction}"));
        self.persist();
    }

    pub(crate) fn prune_selected(&mut self) {
        let Some(id) = self.selected_branch_id else {
            self.message = Some("No branch selected".to_string());
            return;
        };
        if id == 1 {
            self.message = Some("Hard trunk cuts are disabled in V2 preview".to_string());
            return;
        }
        let Some(branch) = self.graph.branch(id).cloned() else {
            self.message = Some("Selected branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        if matches!(branch.status, BranchStatus::Cut | BranchStatus::Deadwood) {
            self.message = Some("Already cut".to_string());
            return;
        }
        let child_ids = descendant_ids(&self.graph, id);
        if let Some(branch) = self.graph.branch_mut(id) {
            branch.status = BranchStatus::Cut;
            branch.end_x = branch.start_x + (branch.end_x - branch.start_x).signum();
            branch.end_y = branch.start_y + (branch.end_y - branch.start_y).signum().max(1);
            branch.last_pruned_day = Some(self.age_days);
        }
        for child_id in child_ids {
            if let Some(child) = self.graph.branch_mut(child_id) {
                child.status = BranchStatus::Deadwood;
                child.vigor = 0;
            }
        }
        if let Some(parent_id) = branch.parent_id {
            let direction = if branch.end_x >= branch.start_x {
                -1
            } else {
                1
            };
            let _ = self
                .graph
                .add_branch(parent_id, direction, 1, 2, 1, (self.vigor / 2) as i16);
        }
        self.vigor = (self.vigor - 4).max(0);
        self.message = Some("Clean cut: back-bud started near the parent".to_string());
        self.ensure_selection();
        self.persist();
    }

    pub(crate) fn pinch_selected(&mut self) {
        let Some(id) = self.selected_branch_id else {
            self.message = Some("No branch selected".to_string());
            return;
        };
        if !self.graph.is_tip(id) {
            self.message = Some("Pinch only the current tip".to_string());
            return;
        }
        let Some(branch) = self.graph.branch_mut(id) else {
            self.message = Some("Selected branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        if matches!(branch.status, BranchStatus::Cut | BranchStatus::Deadwood) {
            self.message = Some("Deadwood has no soft tip".to_string());
            return;
        }
        branch.status = BranchStatus::LeafPad;
        branch.vigor = (branch.vigor - 8).max(10);
        let _ = self.graph.add_branch(id, -1, 1, 1, 1, 35);
        let _ = self.graph.add_branch(id, 1, 1, 1, 1, 35);
        self.message = Some("Pinched: compact pads will form here".to_string());
        self.persist();
    }

    pub(crate) fn share_snippet(&self) -> String {
        let rendered = super::render::render_ascii(self, 72, 24, false);
        let label = if self.is_alive {
            format!(
                "ADMIRE my living graph (Day {}, {} cells)",
                self.age_days, rendered.occupied_cells
            )
        } else {
            "ADMIRE my living graph [RIP]".to_string()
        };
        format!(
            "{}\n{}",
            rendered
                .lines
                .iter()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n"),
            label
        )
    }

    pub(crate) fn selected_branch(&self) -> Option<&Branch> {
        self.selected_branch_id.and_then(|id| self.graph.branch(id))
    }

    pub(crate) fn badge_glyph(&self) -> String {
        badge_glyph_for_graph(&self.graph, self.is_alive, self.vigor, self.water_stress)
    }

    fn selectable_branch_ids(&self) -> Vec<i32> {
        let mut ids = self
            .graph
            .branches
            .iter()
            .filter(|branch| branch.id != 1 && branch.is_alive())
            .map(|branch| branch.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            ids = self
                .graph
                .branches
                .iter()
                .filter(|branch| branch.is_alive())
                .map(|branch| branch.id)
                .collect();
        }
        ids.sort();
        ids
    }

    fn ensure_selection(&mut self) {
        if self
            .selected_branch_id
            .is_some_and(|id| self.graph.branch(id).is_some_and(Branch::is_alive))
        {
            return;
        }
        self.selected_branch_id = self.graph.selected_fallback();
    }

    fn apply_elapsed_days(&mut self, today: NaiveDate) -> bool {
        if self.last_simulated_date >= today {
            return false;
        }
        let days = (today - self.last_simulated_date).num_days().clamp(0, 21);
        if days == 0 {
            self.last_simulated_date = today;
            return true;
        }
        let mut simulated_day = self.last_simulated_date;
        for _ in 0..days {
            if !self.is_alive {
                break;
            }
            if let Some(next_day) = simulated_day.succ_opt() {
                simulated_day = next_day;
                self.simulate_day(simulated_day);
            }
        }
        self.last_simulated_date = today;
        true
    }

    fn simulate_day(&mut self, day: NaiveDate) {
        if !self.is_alive {
            return;
        }
        self.age_days += 1;
        let dry = self
            .last_watered
            .is_none_or(|last| (day - last).num_days() >= 1);
        if dry {
            self.water_stress = (self.water_stress + 11).min(120);
            self.vigor = (self.vigor - 7).max(0);
        } else {
            self.water_stress = self.water_stress.saturating_sub(4);
            self.vigor = (self.vigor + 2).min(100);
        }
        self.grow_once(if dry {
            GrowthCause::DryDay
        } else {
            GrowthCause::Daily
        });
        if self.water_stress >= 100 && self.vigor == 0 {
            self.is_alive = false;
            self.kill_weak_tips();
        }
    }

    fn grow_once(&mut self, cause: GrowthCause) {
        if self.is_alive {
            grow_graph_once(
                &mut self.graph,
                self.seed,
                self.age_days,
                self.vigor,
                self.water_stress,
                cause,
            );
        }
    }

    fn kill_weak_tips(&mut self) {
        for branch in &mut self.graph.branches {
            if branch.vigor <= 20 && branch.id != 1 {
                branch.status = BranchStatus::Deadwood;
            }
        }
    }

    fn persist(&self) {
        let branch_graph =
            serde_json::to_value(&self.graph).unwrap_or_else(|_| serde_json::json!({}));
        self.svc.save_v2_task(BonsaiV2TreeParams {
            user_id: self.user_id,
            seed: self.seed,
            last_watered: self.last_watered,
            is_alive: self.is_alive,
            vigor: self.vigor,
            water_stress: self.water_stress,
            last_simulated_date: self.last_simulated_date,
            branch_graph,
            selected_branch_id: self.selected_branch_id,
            mode: self.mode.as_str().to_string(),
            badge_glyph: self.badge_glyph(),
        });
    }
}

#[derive(Debug, Clone, Copy)]
enum GrowthCause {
    Daily,
    DryDay,
    Passive,
    Water,
}

pub(crate) fn seeded_graph_value(seed: i64, growth_points: i32) -> serde_json::Value {
    serde_json::to_value(seeded_graph(seed, growth_points))
        .unwrap_or_else(|_| serde_json::json!({}))
}

pub(crate) fn seeded_badge_glyph(seed: i64, growth_points: i32, is_alive: bool) -> String {
    badge_glyph_for_graph(&seeded_graph(seed, growth_points), is_alive, 70, 0)
}

fn seeded_graph(seed: i64, growth_points: i32) -> BonsaiGraph {
    let mut graph = BonsaiGraph {
        version: 1,
        next_id: 2,
        branches: vec![Branch {
            id: 1,
            parent_id: None,
            start_x: 0,
            start_y: 0,
            end_x: 0,
            end_y: 4,
            thickness: 2,
            age: 6,
            vigor: 80,
            status: BranchStatus::Growing,
            bend_x: 0,
            bend_y: 0,
            last_pruned_day: None,
        }],
    };

    let first_side = if seed.unsigned_abs() % 2 == 0 { -1 } else { 1 };
    let _ = graph.add_branch(1, first_side, 1, 3, 1, 65);
    let _ = graph.add_branch(1, -first_side, 1, 2, 1, 58);

    let steps = (growth_points / 45).clamp(0, 20);
    for age_days in 0..steps {
        grow_graph_once(&mut graph, seed, age_days as i64, 72, 0, GrowthCause::Daily);
    }
    for branch in &mut graph.branches {
        if branch.is_tip_candidate() && branch.length() >= 3 {
            branch.status = BranchStatus::LeafPad;
        }
    }
    graph
}

fn grow_graph_once(
    graph: &mut BonsaiGraph,
    seed: i64,
    age_days: i64,
    vigor: i32,
    water_stress: i32,
    cause: GrowthCause,
) {
    if graph.branches.len() >= MAX_BRANCHES {
        return;
    }
    let live_ids = graph
        .branches
        .iter()
        .filter(|branch| branch.is_alive())
        .map(|branch| branch.id)
        .collect::<BTreeSet<_>>();
    let mut child_ids = BTreeSet::new();
    for branch in &graph.branches {
        if let Some(parent_id) = branch.parent_id
            && live_ids.contains(&parent_id)
            && branch.is_alive()
        {
            child_ids.insert(parent_id);
        }
    }
    for branch in &mut graph.branches {
        branch.age = branch.age.saturating_add(1);
        branch.vigor =
            (branch.vigor + (vigor / 20) as i16 - (water_stress / 30) as i16).clamp(0, 100);
    }
    let tips = graph
        .branches
        .iter()
        .filter(|branch| branch.is_tip_candidate() && !child_ids.contains(&branch.id))
        .map(|branch| branch.id)
        .collect::<Vec<_>>();
    if tips.is_empty() {
        return;
    }
    let tip_id =
        tips[hash_parts(seed, age_days as u64, graph.next_id as u64) as usize % tips.len()];
    grow_tip_once(graph, tip_id, seed, vigor, water_stress, cause);
}

fn grow_tip_once(
    graph: &mut BonsaiGraph,
    tip_id: i32,
    seed: i64,
    vigor: i32,
    water_stress: i32,
    cause: GrowthCause,
) {
    if graph.branches.len() >= MAX_BRANCHES {
        return;
    }
    let Some(tip) = graph.branch(tip_id).cloned() else {
        return;
    };
    let branch_vigor = tip.vigor as i32 + vigor - water_stress;
    if branch_vigor <= 18 {
        if let Some(branch) = graph.branch_mut(tip_id) {
            branch.status = BranchStatus::Deadwood;
        }
        return;
    }

    if tip.length() < max_tip_length(cause, vigor, water_stress) {
        if let Some(branch) = graph.branch_mut(tip_id) {
            let dx = (branch.end_x - branch.start_x + branch.bend_x as i16).clamp(-3, 3);
            let step_x = dx.signum();
            let raw_y = branch.end_y - branch.start_y + branch.bend_y as i16;
            let step_y = raw_y.clamp(0, 3).signum().max(1);
            branch.end_x = branch.end_x.saturating_add(step_x);
            branch.end_y = branch.end_y.saturating_add(step_y);
        }
    } else if let Some(branch) = graph.branch_mut(tip_id) {
        branch.status = BranchStatus::LeafPad;
    }

    let spawn_threshold = match cause {
        GrowthCause::Water => 44,
        GrowthCause::Daily | GrowthCause::Passive => 58,
        GrowthCause::DryDay => 36,
    };
    let roll = hash_parts(seed, tip_id as u64, graph.next_id as u64) % 100;
    if roll < spawn_threshold && graph.branches.len() < MAX_BRANCHES {
        let side = if hash_parts(seed, graph.next_id as u64, 7) % 2 == 0 {
            -1
        } else {
            1
        };
        let dy = if matches!(cause, GrowthCause::DryDay) {
            0
        } else {
            1
        };
        let len = if matches!(cause, GrowthCause::Water) {
            2
        } else {
            1
        };
        let _ = graph.add_branch(
            tip_id,
            side,
            dy,
            len,
            1,
            (vigor - water_stress / 2).clamp(20, 95) as i16,
        );
    }
}

pub(crate) fn badge_glyph_for_graph(
    graph: &BonsaiGraph,
    is_alive: bool,
    vigor: i32,
    water_stress: i32,
) -> String {
    if !is_alive {
        return String::new();
    }
    let raw_cells = graph
        .branches
        .iter()
        .filter(|branch| branch.is_alive())
        .map(|branch| branch.length().max(1) as i32 + leaf_weight(branch))
        .sum::<i32>();
    let health = if water_stress >= 90 {
        35
    } else if water_stress >= 60 {
        65
    } else if water_stress >= 25 {
        85
    } else if vigor >= 75 {
        110
    } else {
        100
    };
    let score = raw_cells * health / 100;
    match score {
        0..=8 => "·",
        9..=20 => "⚘",
        21..=40 => "🌱",
        41..=75 => "🌲",
        76..=120 => "🌳",
        121..=180 => "🌸",
        _ => "🌼",
    }
    .to_string()
}

fn leaf_weight(branch: &Branch) -> i32 {
    match branch.status {
        BranchStatus::LeafPad => 8,
        BranchStatus::Growing | BranchStatus::Wired => 3,
        BranchStatus::Cut | BranchStatus::Deadwood => 0,
    }
}

fn max_tip_length(cause: GrowthCause, vigor: i32, stress: i32) -> i16 {
    let base = match cause {
        GrowthCause::Water => 5,
        GrowthCause::Daily | GrowthCause::Passive => 4,
        GrowthCause::DryDay => 7,
    };
    (base + vigor / 40 + stress / 35).clamp(2, 8) as i16
}

fn descendant_ids(graph: &BonsaiGraph, id: i32) -> Vec<i32> {
    let mut seen = BTreeSet::new();
    let mut stack = graph.child_ids(id);
    while let Some(child_id) = stack.pop() {
        if !seen.insert(child_id) {
            continue;
        }
        stack.extend(graph.child_ids(child_id));
    }
    seen.into_iter().collect()
}

pub(crate) fn branch_label(branch: &Branch) -> &'static str {
    match branch.status {
        BranchStatus::Growing => "growing tip",
        BranchStatus::Wired => "wired tip",
        BranchStatus::Cut => "cut scar",
        BranchStatus::Deadwood => "deadwood",
        BranchStatus::LeafPad => "leaf pad",
    }
}

fn wire_direction_label(bend_x: i8, bend_y: i8) -> &'static str {
    match (bend_x.signum(), bend_y.signum()) {
        (-1, 1) => "up-left",
        (0, 1) => "up",
        (1, 1) => "up-right",
        (-1, 0) => "left",
        (1, 0) => "right",
        (-1, -1) => "low-left",
        (0, -1) => "lower",
        (1, -1) => "low-right",
        _ => "straight",
    }
}

fn hash_parts(seed: i64, a: u64, b: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    a.hash(&mut hasher);
    b.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_graph_scales_with_legacy_growth() {
        let small = seeded_graph(42, 0);
        let larger = seeded_graph(42, 600);

        assert!(larger.branches.len() > small.branches.len());
        assert_ne!(
            badge_glyph_for_graph(&small, true, 70, 0),
            badge_glyph_for_graph(&larger, true, 70, 0)
        );
    }

    #[test]
    fn pruning_marks_descendants_deadwood_and_adds_back_bud() {
        let graph = seeded_graph(42, 200);
        let selected = graph
            .branches
            .iter()
            .find(|branch| branch.id != 1)
            .unwrap()
            .id;
        let before = graph.branches.len();
        let child_ids = descendant_ids(&graph, selected);

        assert!(before > 0);
        assert!(child_ids.iter().all(|id| *id != selected));
        assert_eq!(
            graph.branch(selected).map(|branch| branch.is_alive()),
            Some(true)
        );
    }
}
