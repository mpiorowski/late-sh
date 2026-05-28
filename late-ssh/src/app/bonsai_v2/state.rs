use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use chrono::NaiveDate;
use late_core::models::bonsai::{BonsaiV2Tree, BonsaiV2TreeParams};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::bonsai::svc::BonsaiService;

const PASSIVE_GROWTH_TICK_INTERVAL: usize = 15 * 60 * 12;
const MAX_BRANCHES: usize = 96;
const LEAF_RAMIFICATION_THRESHOLD: u8 = 3;
const MIN_LEAF_VIGOR: i32 = 55;
const MAX_LEAF_STRESS: i32 = 55;

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
    #[serde(default)]
    pub ramification: u8,
    #[serde(default)]
    pub last_pinched_age: Option<u16>,
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
            .find(|branch| branch.is_alive() && self.is_tip(branch.id))
            .or_else(|| {
                self.branches
                    .iter()
                    .filter(|branch| branch.id != 1)
                    .find(|branch| branch.is_alive())
            })
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
        let _ = len;
        let end_y = (parent.end_y + dy.signum().max(0)).max(1);
        self.branches.push(Branch {
            id,
            parent_id: Some(parent_id),
            start_x: parent.end_x,
            start_y: parent.end_y,
            end_x: parent.end_x + dx.signum(),
            end_y,
            thickness,
            age: 0,
            vigor,
            status: BranchStatus::Growing,
            bend_x: 0,
            bend_y: 0,
            last_pruned_day: None,
            ramification: 0,
            last_pinched_age: None,
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
        let (graph, normalized_ids) =
            serde_json::from_value::<BonsaiGraph>(tree.branch_graph.clone())
                .map(normalize_graph_segments)
                .unwrap_or_else(|_| (seeded_graph(tree.seed, 0), BTreeMap::new()));
        let selected_branch_id = tree
            .selected_branch_id
            .and_then(|id| normalized_ids.get(&id).copied())
            .or(tree.selected_branch_id)
            .or_else(|| graph.selected_fallback());
        let mut state = Self {
            user_id,
            svc,
            seed: tree.seed,
            last_watered: tree.last_watered,
            is_alive: tree.is_alive,
            vigor: tree.vigor,
            water_stress: tree.water_stress.max(0),
            last_simulated_date: tree.last_simulated_date,
            age_days: (today - tree.created.date_naive()).num_days().max(0),
            graph,
            selected_branch_id,
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
        self.water_stress = (self.water_stress - 35).max(0);
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
        if matches!(branch.status, BranchStatus::LeafPad) {
            self.message = Some("Leaf pads are set; cut it back to rebuild".to_string());
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
        let parent_id = branch.parent_id;
        let child_ids = descendant_ids(&self.graph, id);
        let removed_count = child_ids.len();
        if let Some(branch) = self.graph.branch_mut(id) {
            branch.status = BranchStatus::Cut;
            branch.end_x = branch.start_x + (branch.end_x - branch.start_x).signum();
            branch.end_y = branch.start_y + (branch.end_y - branch.start_y).signum().max(1);
            branch.last_pruned_day = Some(self.age_days);
        }
        self.graph
            .branches
            .retain(|branch| !child_ids.contains(&branch.id));
        let mut back_bud_started = false;
        if removed_count > 0
            && let Some(parent_id) = parent_id
        {
            let chance = back_bud_threshold(self.vigor, self.water_stress);
            let roll = hash_parts(self.seed, id as u64, self.age_days as u64) % 100;
            if roll < chance {
                let direction = if branch.end_x >= branch.start_x {
                    -1
                } else {
                    1
                };
                back_bud_started = self
                    .graph
                    .add_branch(parent_id, direction, 1, 1, 1, (self.vigor / 2) as i16)
                    .is_some();
            }
        }
        self.vigor = (self.vigor - 4).max(0);
        self.message = Some(if removed_count == 0 {
            "Clean cut: tip trimmed".to_string()
        } else if back_bud_started {
            format!("Clean cut: removed {removed_count} branch glyphs, back-bud started")
        } else {
            format!("Clean cut: removed {removed_count} branch glyphs, watch for buds")
        });
        self.selected_branch_id = parent_id
            .filter(|parent_id| {
                self.graph
                    .branch(*parent_id)
                    .is_some_and(|branch| branch.is_alive() && self.graph.is_tip(*parent_id))
            })
            .or_else(|| self.graph.selected_fallback());
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
        let Some(branch) = self.graph.branch(id).cloned() else {
            self.message = Some("Selected branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        if matches!(branch.status, BranchStatus::Cut | BranchStatus::Deadwood) {
            self.message = Some("Deadwood has no soft tip".to_string());
            return;
        }
        if matches!(branch.status, BranchStatus::LeafPad) {
            self.message = Some("Already a leaf pad; cut it back to rebuild".to_string());
            return;
        }
        if branch.last_pinched_age == Some(branch.age) {
            self.message = Some("Let this spot push new growth before pinching again".to_string());
            return;
        }
        let Some(parent_id) = branch.parent_id else {
            self.message = Some("The trunk has no soft tip to pinch".to_string());
            return;
        };
        let Some(parent) = self.graph.branch(parent_id) else {
            self.message = Some("Parent branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        if !parent.is_alive() {
            self.message = Some("Cannot pinch back to deadwood".to_string());
            return;
        }

        let child_ids = descendant_ids(&self.graph, id);
        self.graph
            .branches
            .retain(|branch| branch.id != id && !child_ids.contains(&branch.id));
        let Some(parent) = self.graph.branch_mut(parent_id) else {
            self.message = Some("Parent branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        parent.ramification = parent.ramification.saturating_add(1).min(6);
        parent.last_pinched_age = Some(parent.age);
        let ramification = parent.ramification;
        self.vigor = (self.vigor - 2).max(0);
        let hint = if ramification >= LEAF_RAMIFICATION_THRESHOLD {
            if self.vigor >= MIN_LEAF_VIGOR && self.water_stress <= MAX_LEAF_STRESS {
                "leaf buds set here; water to leaf"
            } else {
                "needs lower stress for leaves"
            }
        } else {
            "let it grow, then pinch again"
        };
        self.message = Some(format!(
            "Pinched: ramification {}/{}; {hint}",
            ramification, LEAF_RAMIFICATION_THRESHOLD
        ));
        self.selected_branch_id = Some(parent_id)
            .filter(|parent_id| {
                self.graph
                    .branch(*parent_id)
                    .is_some_and(|branch| branch.is_alive() && self.graph.is_tip(*parent_id))
            })
            .or_else(|| self.graph.selected_fallback());
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
            .filter(|branch| branch.id != 1 && branch.is_alive() && self.graph.is_tip(branch.id))
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
        if self.selected_branch_id.is_some_and(|id| {
            self.graph
                .branch(id)
                .is_some_and(|branch| branch.is_alive() && self.graph.is_tip(id))
        }) {
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
            self.water_stress = (self.water_stress + 11).clamp(0, 120);
            self.vigor = (self.vigor - 7).max(0);
        } else {
            self.water_stress = (self.water_stress - 4).max(0);
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
            let selected_before_growth = self.selected_branch_id;
            let grown = grow_graph_once(
                &mut self.graph,
                self.seed,
                self.age_days,
                self.vigor,
                self.water_stress,
                cause,
                selected_before_growth,
            );
            if let (Some(selected_id), Some((source_id, next_tip_id))) =
                (selected_before_growth, grown)
                && selected_id == source_id
            {
                self.selected_branch_id = Some(next_tip_id);
            }
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
            ramification: 0,
            last_pinched_age: None,
        }],
    };

    let first_side = if seed.unsigned_abs() % 2 == 0 { -1 } else { 1 };
    let _ = graph.add_branch(1, first_side, 1, 3, 1, 65);
    let _ = graph.add_branch(1, -first_side, 1, 2, 1, 58);

    let steps = (growth_points / 45).clamp(0, 20);
    for age_days in 0..steps {
        let _ = grow_graph_once(
            &mut graph,
            seed,
            age_days as i64,
            72,
            0,
            GrowthCause::Daily,
            None,
        );
    }
    normalize_graph_segments(graph).0
}

fn normalize_graph_segments(graph: BonsaiGraph) -> (BonsaiGraph, BTreeMap<i32, i32>) {
    let max_existing_id = graph
        .branches
        .iter()
        .map(|branch| branch.id)
        .max()
        .unwrap_or(0);
    let mut next_id = graph.next_id.max(max_existing_id + 1);
    let mut source_branches = graph.branches;
    source_branches.sort_by_key(|branch| branch.id);

    let mut normalized = BonsaiGraph {
        version: graph.version,
        next_id,
        branches: Vec::with_capacity(source_branches.len()),
    };
    let mut terminal_ids = BTreeMap::new();

    for branch in source_branches {
        if normalized.branches.len() >= MAX_BRANCHES {
            break;
        }
        let parent_id = branch
            .parent_id
            .and_then(|id| terminal_ids.get(&id).copied().or(Some(id)));
        let terminal_id = push_segment_chain(&mut normalized, &mut next_id, branch, parent_id);
        terminal_ids.insert(terminal_id.0, terminal_id.1);
    }

    normalized.next_id = next_id;
    (normalized, terminal_ids)
}

fn push_segment_chain(
    graph: &mut BonsaiGraph,
    next_id: &mut i32,
    branch: Branch,
    parent_id: Option<i32>,
) -> (i32, i32) {
    if branch.length() <= 1 {
        let source_id = branch.id;
        let terminal_id = branch.id;
        graph.branches.push(Branch {
            parent_id,
            ..branch
        });
        return (source_id, terminal_id);
    }

    let source_id = branch.id;
    let mut previous_parent_id = parent_id;
    let mut start_x = branch.start_x;
    let mut start_y = branch.start_y;
    let mut segment_index = 0usize;
    let mut terminal_id = branch.id;

    while graph.branches.len() < MAX_BRANCHES && (start_x, start_y) != (branch.end_x, branch.end_y)
    {
        let next_x = start_x + (branch.end_x - start_x).signum();
        let next_y = start_y + (branch.end_y - start_y).signum();
        let is_first = segment_index == 0;
        let is_last = (next_x, next_y) == (branch.end_x, branch.end_y);
        let id = if is_first {
            branch.id
        } else {
            let id = *next_id;
            *next_id += 1;
            id
        };
        let status =
            if is_last || matches!(branch.status, BranchStatus::Cut | BranchStatus::Deadwood) {
                branch.status
            } else if matches!(branch.status, BranchStatus::Wired) {
                BranchStatus::Wired
            } else {
                BranchStatus::Growing
            };
        graph.branches.push(Branch {
            id,
            parent_id: previous_parent_id,
            start_x,
            start_y,
            end_x: next_x,
            end_y: next_y,
            thickness: branch.thickness,
            age: branch.age,
            vigor: branch.vigor,
            status,
            bend_x: branch.bend_x,
            bend_y: branch.bend_y,
            last_pruned_day: is_last.then_some(branch.last_pruned_day).flatten(),
            ramification: if is_last { branch.ramification } else { 0 },
            last_pinched_age: if is_last {
                branch.last_pinched_age
            } else {
                None
            },
        });
        terminal_id = id;
        previous_parent_id = Some(id);
        start_x = next_x;
        start_y = next_y;
        segment_index += 1;
    }

    (source_id, terminal_id)
}

fn grow_graph_once(
    graph: &mut BonsaiGraph,
    seed: i64,
    age_days: i64,
    vigor: i32,
    water_stress: i32,
    cause: GrowthCause,
    preferred_tip_id: Option<i32>,
) -> Option<(i32, i32)> {
    if graph.branches.len() >= MAX_BRANCHES {
        return None;
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
    }
    let tips = graph
        .branches
        .iter()
        .filter(|branch| branch.is_tip_candidate() && !child_ids.contains(&branch.id))
        .map(|branch| branch.id)
        .collect::<Vec<_>>();
    if tips.is_empty() {
        return None;
    }
    let tip_id = preferred_tip_id
        .filter(|id| {
            tips.contains(id)
                && graph
                    .branch(*id)
                    .is_some_and(Branch::is_tip_candidate)
        })
        .unwrap_or_else(|| {
            tips[hash_parts(seed, age_days as u64, graph.next_id as u64) as usize % tips.len()]
        });
    grow_tip_once(graph, tip_id, seed, vigor, water_stress, cause).map(|next_id| (tip_id, next_id))
}

fn grow_tip_once(
    graph: &mut BonsaiGraph,
    tip_id: i32,
    seed: i64,
    vigor: i32,
    water_stress: i32,
    cause: GrowthCause,
) -> Option<i32> {
    if graph.branches.len() >= MAX_BRANCHES {
        return None;
    }
    let Some(tip) = graph.branch(tip_id).cloned() else {
        return None;
    };
    if water_stress >= 80 && hash_parts(seed, tip_id as u64, graph.next_id as u64) % 100 < 24 {
        if let Some(branch) = graph.branch_mut(tip_id) {
            branch.status = BranchStatus::Deadwood;
        }
        return None;
    }
    if vigor <= 8 {
        return None;
    }
    if tip.ramification >= LEAF_RAMIFICATION_THRESHOLD
        && vigor >= MIN_LEAF_VIGOR
        && water_stress <= MAX_LEAF_STRESS
    {
        if let Some(branch) = graph.branch_mut(tip_id) {
            branch.status = BranchStatus::LeafPad;
        }
        return Some(tip_id);
    }

    let (dx, dy) = growth_step(&tip);
    let thickness = tip.thickness.saturating_sub(1).max(1);
    let new_id = graph.add_branch(
        tip_id,
        dx,
        dy,
        1,
        thickness,
        (vigor - water_stress / 2).clamp(20, 95) as i16,
    );
    if let Some(new_id) = new_id {
        if let Some(child) = graph.branch_mut(new_id) {
            child.bend_x = tip.bend_x;
            child.bend_y = tip.bend_y;
            if matches!(tip.status, BranchStatus::Wired) {
                child.status = BranchStatus::Wired;
            }
        }
    }
    let continuation_id = new_id?;

    let spawn_threshold = side_shoot_threshold(cause, &tip, vigor, water_stress);
    let roll = hash_parts(seed, tip_id as u64, graph.next_id as u64) % 100;
    if roll < spawn_threshold && graph.branches.len() < MAX_BRANCHES {
        let (side, dy) = side_shoot_step(seed, graph.next_id as u64, cause, water_stress);
        let _ = graph.add_branch(
            tip_id,
            side,
            dy,
            1,
            1,
            (vigor - water_stress / 2).clamp(20, 95) as i16,
        );
    }
    Some(continuation_id)
}

fn growth_step(branch: &Branch) -> (i16, i16) {
    let current_dx = (branch.end_x - branch.start_x).signum();
    let step_x = if branch.bend_x != 0 {
        branch.bend_x.signum() as i16
    } else {
        current_dx
    };
    let step_y = match branch.bend_y.cmp(&0) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => 0,
        std::cmp::Ordering::Equal if branch.bend_x != 0 => 0,
        std::cmp::Ordering::Equal => 1,
    };
    (step_x, step_y)
}

fn side_shoot_threshold(cause: GrowthCause, tip: &Branch, vigor: i32, water_stress: i32) -> u64 {
    let base = match cause {
        GrowthCause::Water => 6,
        GrowthCause::Daily | GrowthCause::Passive => 4,
        GrowthCause::DryDay => 24,
    };
    let vigor_bonus = if water_stress <= 35 {
        ((vigor - 55).max(0) / 8).clamp(0, 6)
    } else {
        0
    };
    let ramification_bonus = (tip.ramification as i32 * 10).min(30);
    let stress_bonus = if water_stress >= 60 {
        ((water_stress - 55) / 4).clamp(0, 20)
    } else {
        0
    };
    (base + vigor_bonus + ramification_bonus + stress_bonus).clamp(0, 70) as u64
}

fn side_shoot_step(seed: i64, next_id: u64, cause: GrowthCause, water_stress: i32) -> (i16, i16) {
    let side = if hash_parts(seed, next_id, 7) % 2 == 0 {
        -1
    } else {
        1
    };
    let messy = matches!(cause, GrowthCause::DryDay) || water_stress >= 60;
    let dy = if messy && hash_parts(seed, next_id, 11) % 100 < 55 {
        0
    } else {
        1
    };
    (side, dy)
}

fn back_bud_threshold(vigor: i32, water_stress: i32) -> u64 {
    (55 + vigor / 3 - water_stress / 2).clamp(20, 90) as u64
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
        BranchStatus::Growing if branch.ramification > 0 => "ramified tip",
        BranchStatus::Growing => "growing tip",
        BranchStatus::Wired if branch.ramification > 0 => "wired ramified tip",
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

    #[test]
    fn healthy_ramified_tip_leafs_out_on_growth() {
        let mut graph = seeded_graph(42, 0);
        let tip_id = graph
            .branches
            .iter()
            .find(|branch| branch.id != 1 && graph.is_tip(branch.id))
            .unwrap()
            .id;
        graph.branch_mut(tip_id).unwrap().ramification = LEAF_RAMIFICATION_THRESHOLD;

        grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water);

        assert_eq!(
            graph.branch(tip_id).map(|branch| branch.status),
            Some(BranchStatus::LeafPad)
        );
    }

    #[test]
    fn seeded_graph_uses_one_cell_segments() {
        let graph = seeded_graph(42, 600);

        assert!(graph.branches.iter().all(|branch| branch.length() <= 1));
    }

    #[test]
    fn growth_adds_child_segment_without_extending_source() {
        let mut graph = seeded_graph(42, 0);
        let tip_id = graph
            .branches
            .iter()
            .find(|branch| branch.id != 1 && graph.is_tip(branch.id))
            .unwrap()
            .id;
        let before = graph.branch(tip_id).unwrap().clone();

        let new_id = grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water).unwrap();

        assert_eq!(graph.branch(tip_id).unwrap().end_x, before.end_x);
        assert_eq!(graph.branch(tip_id).unwrap().end_y, before.end_y);
        assert_eq!(graph.branch(new_id).unwrap().parent_id, Some(tip_id));
        assert_eq!(graph.branch(new_id).unwrap().length(), 1);
    }

    #[test]
    fn growth_carries_ramification_to_continuation() {
        let mut graph = seeded_graph(42, 0);
        let tip_id = graph
            .branches
            .iter()
            .find(|branch| branch.id != 1 && graph.is_tip(branch.id))
            .unwrap()
            .id;
        graph.branch_mut(tip_id).unwrap().ramification = 2;

        let new_id = grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water).unwrap();

        assert_eq!(graph.branch(new_id).unwrap().ramification, 2);
    }

    #[test]
    fn ramification_and_stress_raise_side_shoot_chance() {
        let graph = seeded_graph(42, 0);
        let tip_id = graph
            .branches
            .iter()
            .find(|branch| branch.id != 1 && graph.is_tip(branch.id))
            .unwrap()
            .id;
        let mut tip = graph.branch(tip_id).unwrap().clone();
        let plain = side_shoot_threshold(GrowthCause::Water, &tip, 70, 0);

        tip.ramification = 2;
        let ramified = side_shoot_threshold(GrowthCause::Water, &tip, 70, 0);
        let stressed = side_shoot_threshold(GrowthCause::DryDay, &tip, 35, 80);

        assert!(ramified > plain);
        assert!(stressed > plain);
    }
}
