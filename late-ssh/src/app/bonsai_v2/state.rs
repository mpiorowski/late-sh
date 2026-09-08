use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::bonsai::{BonsaiV2Tree, BonsaiV2TreeParams};
use late_core::models::bonsai_decay_protection::BonsaiDecayProtection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::bonsai::svc::BonsaiService;

const MAX_GROWTH_WAVE_TIPS: usize = 6;
const LEAF_RAMIFICATION_THRESHOLD: u8 = 3;
const ROOT_BRANCH_ID: i32 = 1;

/// The pot. A Dynamic Bonsai lives in one fixed canvas that every surface
/// (care modal, sidebar panel, profile hero, share snippet) shows 1:1, so
/// the tree is never scaled or cropped anywhere. Growth stops at the edge;
/// density does not: forks, buds, and pads keep filling the box. The size
/// is the sidebar panel's: 21 columns, 12 tree rows over the pot row.
pub(crate) const CANVAS_WIDTH: usize = 21;
pub(crate) const CANVAS_HEIGHT: usize = 13;
/// A leaf pad reaches this far sideways and up from its tip, so tips stop
/// short of the canvas edge by that much and pads never leave the box.
const PAD_REACH_X: i16 = 2;
const PAD_REACH_Y: i16 = 1;
const TIP_MAX_ABS_X: i16 = (CANVAS_WIDTH as i16) / 2 - PAD_REACH_X;
/// Graph y=0 is the trunk base on the row above the pot, so the tallest
/// tip row is the canvas height minus the pot row, the base row, and the
/// pad's reach.
const TIP_MAX_Y: i16 = CANVAS_HEIGHT as i16 - 2 - PAD_REACH_Y;
/// The branch cap is the pot itself: one segment per tip cell plus the
/// root, so the only way a tree stops growing is a pot with no open cell
/// left, and a cut always opens one. Every branch-adding path still
/// checks it, as the last line of defense behind the openness check.
const MAX_BRANCHES: usize =
    (2 * TIP_MAX_ABS_X as usize + 1) * TIP_MAX_Y as usize + 1;

/// Whether the once-per-UTC-day watering rule applies to this press.
/// `AdminBypass` is a temporary testing aid (2026-09-08): admins can water
/// Dynamic Bonsai repeatedly to watch growth waves land. The daily chips
/// are unaffected, since the classic path pays them once per day in the DB.
/// Remove once the Dynamic renderer has been evaluated.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum DailyWaterGate {
    Enforced,
    AdminBypass,
}

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
    Pinched,
    NeedsPinch,
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
            .filter(|branch| branch.id != ROOT_BRANCH_ID)
            .find(|branch| branch.is_alive() && self.is_tip(branch.id))
            .or_else(|| {
                self.branches
                    .iter()
                    .filter(|branch| branch.id != ROOT_BRANCH_ID)
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
        let target = branch_target(&parent, dx, dy);
        if !growth_target_is_open(self, parent_id, target) {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let _ = len;
        self.branches.push(Branch {
            id,
            parent_id: Some(parent_id),
            start_x: parent.end_x,
            start_y: parent.end_y,
            end_x: target.0,
            end_y: target.1,
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
    pub planted_at: DateTime<Utc>,
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
    state_revision: i64,

    /// The user's live Bonsai Decay Shield window, if any, consulted by
    /// `simulate_day` so a protected day adds no water stress and costs no
    /// vigor. Loaded at construction time (login, or profile view for
    /// `view_only`) and refreshed from the shop snapshot on tick; Dynamic
    /// Bonsai has no in-session re-simulation, so a purchase mid-session
    /// only takes visible effect from the next construction onward.
    pub decay_protection: Option<BonsaiDecayProtection>,
}

impl BonsaiV2State {
    pub(crate) fn new(
        user_id: Uuid,
        svc: BonsaiService,
        tree: BonsaiV2Tree,
        decay_protection: Option<BonsaiDecayProtection>,
    ) -> Self {
        let today = BonsaiService::today();
        let persisted_badge_glyph = tree.badge_glyph.clone();
        let (mut graph, normalized_ids) =
            serde_json::from_value::<BonsaiGraph>(tree.branch_graph.clone())
                .map(normalize_graph_segments)
                .unwrap_or_else(|_| (seeded_graph(tree.seed, 0), BTreeMap::new()));
        let repotted = repot_into_canvas(&mut graph);
        let selected_branch_id = tree
            .selected_branch_id
            .and_then(|id| normalized_ids.get(&id).copied())
            .or(tree.selected_branch_id)
            .or_else(|| graph.selected_fallback());
        let mut state = Self {
            user_id,
            svc,
            seed: tree.seed,
            planted_at: tree.planted_at,
            last_watered: tree.last_watered,
            is_alive: tree.is_alive,
            vigor: tree.vigor,
            water_stress: tree.water_stress.max(0),
            last_simulated_date: tree.last_simulated_date,
            age_days: simulated_age_days(tree.planted_at, tree.last_simulated_date),
            graph,
            selected_branch_id,
            mode: BonsaiV2Mode::from_str(&tree.mode),
            message: (repotted > 0).then(|| repot_message(repotted)),
            state_revision: tree.state_revision,
            decay_protection,
        };
        state.ensure_selection();
        let elapsed_changed = state.apply_elapsed_days(today);
        let badge_changed = state.badge_glyph() != persisted_badge_glyph;
        if elapsed_changed || badge_changed || repotted > 0 {
            state.persist();
        }
        state
    }

    /// Build a read-only state for rendering another user's tree (profile
    /// view). Catches elapsed days up in memory so the silhouette is accurate,
    /// but never persists, so viewing never mutates the owner's tree. Always
    /// renders standard 2D.
    pub(crate) fn view_only(
        user_id: Uuid,
        svc: BonsaiService,
        tree: BonsaiV2Tree,
        decay_protection: Option<BonsaiDecayProtection>,
    ) -> Self {
        let today = BonsaiService::today();
        let (mut graph, normalized_ids) =
            serde_json::from_value::<BonsaiGraph>(tree.branch_graph.clone())
                .map(normalize_graph_segments)
                .unwrap_or_else(|_| (seeded_graph(tree.seed, 0), BTreeMap::new()));
        let _ = repot_into_canvas(&mut graph);
        let selected_branch_id = tree
            .selected_branch_id
            .and_then(|id| normalized_ids.get(&id).copied())
            .or(tree.selected_branch_id)
            .or_else(|| graph.selected_fallback());
        let mut state = Self {
            user_id,
            svc,
            seed: tree.seed,
            planted_at: tree.planted_at,
            last_watered: tree.last_watered,
            is_alive: tree.is_alive,
            vigor: tree.vigor,
            water_stress: tree.water_stress.max(0),
            last_simulated_date: tree.last_simulated_date,
            age_days: simulated_age_days(tree.planted_at, tree.last_simulated_date),
            graph,
            selected_branch_id,
            mode: BonsaiV2Mode::from_str(&tree.mode),
            message: None,
            state_revision: tree.state_revision,
            decay_protection,
        };
        state.ensure_selection();
        // In-memory catch-up only; intentionally no `persist()` so a viewer
        // never writes to the viewed user's row.
        state.apply_elapsed_days(today);
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
            planted_at: Utc::now(),
            last_watered: None,
            is_alive: true,
            vigor: 70,
            water_stress: 0,
            last_simulated_date: today,
            age_days: 0,
            graph,
            selected_branch_id,
            mode: BonsaiV2Mode::Inspect,
            message: Some("Dynamic Bonsai is not persisted yet".to_string()),
            state_revision: 0,
            decay_protection: None,
        }
    }

    /// Water once per UTC day: a second press the same day is refused,
    /// unless the caller passes the admin bypass (a testing aid, see
    /// `DailyWaterGate`).
    pub(crate) fn water(&mut self, gate: DailyWaterGate) -> bool {
        let today = BonsaiService::today();
        if !self.is_alive {
            self.respawn();
            return true;
        }
        let refused = match gate {
            DailyWaterGate::Enforced => self.last_watered == Some(today),
            DailyWaterGate::AdminBypass => false,
        };
        if refused {
            self.message = Some("Already watered today".to_string());
            return false;
        }
        self.last_watered = Some(today);
        if self.last_simulated_date < today {
            self.last_simulated_date = today;
        }
        self.water_stress = (self.water_stress - 35).max(0);
        self.vigor = (self.vigor + 18).min(100);
        self.grow_once(GrowthCause::Water);
        self.message = Some("Watered: vigor pushed new growth".to_string());
        self.persist();
        true
    }

    pub(crate) fn respawn(&mut self) {
        let today = BonsaiService::today();
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.planted_at = Utc::now();
        self.graph = seeded_graph(self.seed, 0);
        self.selected_branch_id = self.graph.selected_fallback();
        self.last_watered = None;
        self.is_alive = true;
        self.vigor = 70;
        self.water_stress = 0;
        self.last_simulated_date = today;
        self.age_days = 0;
        self.mode = BonsaiV2Mode::Inspect;
        self.message = Some("New dynamic bonsai planted".to_string());
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
        self.message = None;
        self.persist();
    }

    pub(crate) fn bend_selected(&mut self, dx: i8, dy: i8) {
        let Some(id) = self.selected_branch_id else {
            self.message = Some("No branch selected".to_string());
            return;
        };
        if id == ROOT_BRANCH_ID {
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
        if matches!(
            branch.status,
            BranchStatus::Pinched | BranchStatus::NeedsPinch | BranchStatus::LeafPad
        ) {
            self.message = Some("Pinched and leaf branches will not wire".to_string());
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
        if id == ROOT_BRANCH_ID {
            self.message = Some("Hard trunk cuts are disabled".to_string());
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
        let removed_count = self.remove_branch_and_descendants(id);
        self.vigor = (self.vigor - 4).max(0);
        self.message = Some(clean_cut_message(removed_count));
        self.select_parent_tip_or_fallback(branch.parent_id);
        self.persist();
    }

    pub(crate) fn split_selected(&mut self) {
        let Some(id) = self.selected_branch_id else {
            self.message = Some("No branch selected".to_string());
            return;
        };
        if id == ROOT_BRANCH_ID {
            self.message = Some("The trunk will not split".to_string());
            return;
        }
        if !self.graph.is_tip(id) {
            self.message = Some("Split only a live tip".to_string());
            return;
        }
        let Some(branch) = self.graph.branch_mut(id) else {
            self.message = Some("Selected branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        match branch.status {
            BranchStatus::Growing | BranchStatus::Wired => {
                branch.last_pruned_day = Some(self.age_days);
                self.message = Some("Split marked: next growth forks if space is open".to_string());
                self.persist();
            }
            BranchStatus::Pinched | BranchStatus::NeedsPinch => {
                self.message = Some("Pinched branches stay compact; cut to rebuild".to_string());
            }
            BranchStatus::LeafPad => {
                self.message = Some("Leaf pads stay compact; cut to rebuild".to_string());
            }
            BranchStatus::Cut | BranchStatus::Deadwood => {
                self.message = Some("Deadwood will not split".to_string());
            }
        }
    }

    pub(crate) fn pinch_selected(&mut self) {
        let Some(id) = self.selected_branch_id else {
            self.message = Some("No branch selected".to_string());
            return;
        };
        if id == ROOT_BRANCH_ID {
            self.message = Some("The trunk will not pinch".to_string());
            return;
        }
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
        if matches!(branch.status, BranchStatus::Pinched) {
            self.message = Some("Let this pinch set before pinching again".to_string());
            return;
        }
        let Some(branch) = self.graph.branch_mut(id) else {
            self.message = Some("Selected branch vanished".to_string());
            self.ensure_selection();
            return;
        };
        branch.ramification = branch
            .ramification
            .saturating_add(1)
            .min(LEAF_RAMIFICATION_THRESHOLD);
        branch.last_pinched_age = Some(branch.age);
        branch.last_pruned_day = None;
        let ramification = branch.ramification;
        if ramification >= LEAF_RAMIFICATION_THRESHOLD {
            branch.status = BranchStatus::LeafPad;
        } else {
            branch.status = BranchStatus::Pinched;
        }
        self.vigor = (self.vigor - 2).max(0);
        let hint = if ramification >= LEAF_RAMIFICATION_THRESHOLD {
            "leaf pad set"
        } else {
            "wait for the next pinch color"
        };
        self.message = Some(format!(
            "Pinched: {}/{}; {hint}",
            ramification, LEAF_RAMIFICATION_THRESHOLD
        ));
        self.persist();
    }

    pub(crate) fn share_snippet(&self) -> String {
        let rendered = super::render::render_ascii(self, CANVAS_WIDTH, CANVAS_HEIGHT, false);
        let label = if self.is_alive {
            format!(
                "ADMIRE my Dynamic Bonsai (Day {}, {} cells)",
                self.age_days, rendered.occupied_cells
            )
        } else {
            "ADMIRE my Dynamic Bonsai [RIP]".to_string()
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
            .filter(|branch| {
                branch.id != ROOT_BRANCH_ID && branch.is_alive() && self.graph.is_tip(branch.id)
            })
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

    fn branch_is_alive_tip(&self, id: i32) -> bool {
        self.graph
            .branch(id)
            .is_some_and(|branch| branch.is_alive() && self.graph.is_tip(id))
    }

    fn select_parent_tip_or_fallback(&mut self, parent_id: Option<i32>) {
        self.selected_branch_id = parent_id
            .filter(|parent_id| self.branch_is_alive_tip(*parent_id))
            .or_else(|| self.graph.selected_fallback());
    }

    fn remove_branch_and_descendants(&mut self, id: i32) -> usize {
        let child_ids = descendant_ids(&self.graph, id);
        let removed_count = child_ids.len() + 1;
        self.graph
            .branches
            .retain(|branch| branch.id != id && !child_ids.contains(&branch.id));
        removed_count
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
        let protected = self
            .decay_protection
            .is_some_and(|protection| protection.covers_day(day));
        let dry = self
            .last_watered
            .is_none_or(|last| (day - last).num_days() >= 1);
        // A live Bonsai Decay Shield holds a dry day neutral: no stress rise,
        // no vigor loss, no death check. It never cancels the recovery a
        // watered day earns, so owning a shield can only ever help.
        match (dry, protected) {
            (true, false) => {
                self.water_stress = (self.water_stress + 11).clamp(0, 120);
                self.vigor = (self.vigor - 7).max(0);
            }
            (true, true) => {}
            (false, _) => {
                self.water_stress = (self.water_stress - 4).max(0);
                self.vigor = (self.vigor + 2).min(100);
            }
        }
        self.grow_once(if dry && !protected {
            GrowthCause::DryDay
        } else {
            GrowthCause::Daily
        });
        if !protected && self.water_stress >= 100 && self.vigor == 0 {
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
            if let Some(selected_id) = selected_before_growth
                && let Some((_, next_tip_id)) = grown
                    .iter()
                    .find(|(source_id, _)| *source_id == selected_id)
            {
                self.selected_branch_id = Some(*next_tip_id);
            }
        }
    }

    fn kill_weak_tips(&mut self) {
        for branch in &mut self.graph.branches {
            if branch.vigor <= 20 && branch.id != ROOT_BRANCH_ID {
                branch.status = BranchStatus::Deadwood;
            }
        }
    }

    fn persist(&mut self) {
        self.state_revision += 1;
        let branch_graph =
            serde_json::to_value(&self.graph).unwrap_or_else(|_| serde_json::json!({}));
        self.svc.save_v2_task(BonsaiV2TreeParams {
            user_id: self.user_id,
            seed: self.seed,
            planted_at: self.planted_at,
            last_watered: self.last_watered,
            is_alive: self.is_alive,
            vigor: self.vigor,
            water_stress: self.water_stress,
            last_simulated_date: self.last_simulated_date,
            branch_graph,
            selected_branch_id: self.selected_branch_id,
            mode: self.mode.as_str().to_string(),
            badge_glyph: self.badge_glyph(),
            state_revision: self.state_revision,
        });
    }
}

fn simulated_age_days(planted_at: DateTime<Utc>, last_simulated_date: NaiveDate) -> i64 {
    (last_simulated_date - planted_at.date_naive())
        .num_days()
        .max(0)
}

#[derive(Debug, Clone, Copy)]
enum GrowthCause {
    Daily,
    DryDay,
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
            id: ROOT_BRANCH_ID,
            parent_id: None,
            start_x: 0,
            start_y: 0,
            end_x: 0,
            end_y: 0,
            thickness: 2,
            age: 0,
            vigor: 80,
            status: BranchStatus::Growing,
            bend_x: 0,
            bend_y: 0,
            last_pruned_day: None,
            ramification: 0,
            last_pinched_age: None,
        }],
    };

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
) -> Vec<(i32, i32)> {
    if graph.branches.len() >= MAX_BRANCHES {
        return Vec::new();
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
        if matches!(branch.status, BranchStatus::Pinched) {
            branch.status = BranchStatus::NeedsPinch;
        }
    }
    let tips = graph
        .branches
        .iter()
        .filter(|branch| branch.is_tip_candidate() && !child_ids.contains(&branch.id))
        .map(|branch| branch.id)
        .collect::<Vec<_>>();
    let grown = grow_tips_once(
        graph,
        &tips,
        seed,
        age_days,
        vigor,
        water_stress,
        cause,
        preferred_tip_id,
    );
    // Budding runs whether or not any tip grew: a tree whose every end
    // has been pinched into a pad has no tips at all, and the buds are
    // what keep it alive as a game.
    bud_once(graph, seed, age_days, vigor, water_stress, cause);
    grown
}

fn grow_tips_once(
    graph: &mut BonsaiGraph,
    tips: &[i32],
    seed: i64,
    age_days: i64,
    vigor: i32,
    water_stress: i32,
    cause: GrowthCause,
    preferred_tip_id: Option<i32>,
) -> Vec<(i32, i32)> {
    if tips.is_empty() {
        return Vec::new();
    }
    let split_pending_tip_count = tips
        .iter()
        .filter(|id| {
            graph
                .branch(**id)
                .is_some_and(|branch| branch.last_pruned_day.is_some())
        })
        .count();
    let budget = growth_wave_budget(cause, vigor, water_stress, tips.len())
        .max(split_pending_tip_count)
        .min(tips.len());
    let tip_ids = growth_tip_order(graph, tips, seed, age_days, preferred_tip_id, budget);
    let mut grown = Vec::new();
    for tip_id in tip_ids {
        if graph.branches.len() >= MAX_BRANCHES {
            break;
        }
        if !graph
            .branch(tip_id)
            .is_some_and(|branch| branch.is_tip_candidate() && graph.is_tip(tip_id))
        {
            continue;
        }
        if let Some(next_id) = grow_tip_once(graph, tip_id, seed, vigor, water_stress, cause) {
            grown.push((tip_id, next_id));
        }
    }
    grown
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
    let tip = graph.branch(tip_id).cloned()?;
    if water_stress >= 80 && hash_parts(seed, tip_id as u64, graph.next_id as u64) % 100 < 24 {
        if let Some(branch) = graph.branch_mut(tip_id) {
            branch.status = BranchStatus::Deadwood;
        }
        return None;
    }
    if vigor <= 8 {
        return None;
    }
    if tip.last_pruned_day.is_some() {
        if tip_id != ROOT_BRANCH_ID {
            let split = split_tip_once(graph, tip_id, seed);
            if let Some(branch) = graph.branch_mut(tip_id) {
                branch.last_pruned_day = None;
            }
            return split.map(|(left_id, _)| left_id);
        }
        if let Some(branch) = graph.branch_mut(tip_id) {
            branch.last_pruned_day = None;
        }
    }

    // A wire is the player's explicit direction, so a wired tip extends
    // where it was told to. Everything else grows on a length budget:
    // once a run of single-file segments reaches its order's budget the
    // tip forks instead of extending, and if no fork is open it is done.
    // That is what keeps the tree inside its pot and turns growth into
    // density rather than height.
    let (order, run) = order_and_run(graph, tip_id);
    let wired =
        matches!(tip.status, BranchStatus::Wired) || tip.bend_x != 0 || tip.bend_y != 0;
    if !wired && run >= run_budget(order) {
        return split_tip_once(graph, tip_id, seed).map(|(first_id, _)| first_id);
    }
    let (dx, dy) = if order == 0 && !wired {
        trunk_step(seed, run)
    } else {
        growth_step(&tip)
    };
    let thickness = tip.thickness.saturating_sub(1).max(1);
    let new_id = graph.add_branch(
        tip_id,
        dx,
        dy,
        1,
        thickness,
        (vigor - water_stress / 2).clamp(20, 95) as i16,
    );
    if let Some(new_id) = new_id
        && let Some(child) = graph.branch_mut(new_id)
    {
        child.bend_x = tip.bend_x;
        child.bend_y = tip.bend_y;
        if matches!(tip.status, BranchStatus::Wired) {
            child.status = BranchStatus::Wired;
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

fn split_tip_once(graph: &mut BonsaiGraph, tip_id: i32, seed: i64) -> Option<(i32, i32)> {
    if graph.branches.len() + 2 > MAX_BRANCHES {
        return None;
    }
    let tip = graph.branch(tip_id)?.clone();
    if !matches!(tip.status, BranchStatus::Growing | BranchStatus::Wired) || !graph.is_tip(tip_id) {
        return None;
    }
    let (order, _) = order_and_run(graph, tip_id);
    let candidates = split_candidates(seed, tip_id, graph.next_id, order);
    if !split_targets_are_open(graph, tip_id, &tip, candidates) {
        return None;
    }

    let first_id = graph.add_branch(tip_id, candidates[0].0, candidates[0].1, 1, 1, tip.vigor)?;
    let second_id = graph.add_branch(tip_id, candidates[1].0, candidates[1].1, 1, 1, tip.vigor)?;
    Some((first_id, second_id))
}

fn split_targets_are_open(
    graph: &BonsaiGraph,
    tip_id: i32,
    tip: &Branch,
    targets: [(i16, i16); 2],
) -> bool {
    let mapped_targets = targets.map(|(dx, dy)| branch_target(tip, dx, dy));
    if mapped_targets[0] == mapped_targets[1]
        || points_are_adjacent(mapped_targets[0], mapped_targets[1])
    {
        return false;
    }
    targets.into_iter().all(|(dx, dy)| {
        let target = branch_target(tip, dx, dy);
        growth_target_is_open(graph, tip_id, target)
    })
}

/// The two directions a fork takes. The trunk's first fork always opens
/// upward on both sides; deeper forks mix a level arm with a rising one,
/// so the crown spreads sideways instead of stacking up.
fn split_candidates(seed: i64, tip_id: i32, next_id: i32, order: u8) -> [(i16, i16); 2] {
    let roll = hash_parts(seed, tip_id as u64, next_id as u64);
    let pair: [(i16, i16); 2] = match order {
        0 => [(-1, 1), (1, 1)],
        _ => match (roll / 2) % 3 {
            0 => [(-1, 1), (1, 1)],
            1 => [(-1, 0), (1, 1)],
            _ => [(-1, 1), (1, 0)],
        },
    };
    if roll.is_multiple_of(2) {
        pair
    } else {
        [pair[1], pair[0]]
    }
}

/// Where a segment sits in the tree: `order` counts the forks between it
/// and the trunk base (the trunk is order 0), `run` counts the single-file
/// segments since the last fork, this one included. The zero-length root
/// segment is not a cell and does not count.
fn order_and_run(graph: &BonsaiGraph, id: i32) -> (u8, u8) {
    let mut order = 0u8;
    let mut run = 0u8;
    let mut counting_run = true;
    let mut current = graph.branch(id);
    let mut remaining_hops = graph.branches.len();
    while let Some(branch) = current
        && remaining_hops > 0
    {
        remaining_hops -= 1;
        if counting_run && branch.length() > 0 {
            run = run.saturating_add(1);
        }
        let Some(parent_id) = branch.parent_id else {
            break;
        };
        let live_children = graph
            .branches
            .iter()
            .filter(|candidate| candidate.parent_id == Some(parent_id) && candidate.is_alive())
            .count();
        if live_children >= 2 {
            counting_run = false;
            order = order.saturating_add(1);
        }
        current = graph.branch(parent_id);
    }
    (order, run)
}

/// How many single-file cells a run may reach before it must fork. Each
/// division roughly halves what comes after it, the way ramification
/// works on a real bonsai, and the sum fits the canvas: trunk 3, first
/// arms 3, then 2, 2, and 1 from there on.
fn run_budget(order: u8) -> u8 {
    match order {
        0 | 1 => 3,
        2 | 3 => 2,
        _ => 1,
    }
}

/// The trunk's own movement: straight out of the pot, one lean to the
/// seed's side, then straight again, so it forks at three cells with a
/// slight S rather than as a post.
fn trunk_step(seed: i64, run: u8) -> (i16, i16) {
    let side: i16 = if hash_parts(seed, 3, 3).is_multiple_of(2) {
        -1
    } else {
        1
    };
    match run {
        1 => (side, 1),
        _ => (0, 1),
    }
}

/// Back-budding: a live segment behind the tips throws a new shoot from
/// its end, so foliage can form at every height instead of only where the
/// tree stopped growing. Two kinds of node bud: an interior segment with
/// exactly one live child (the bud leans away from that child, and no
/// node ever holds more than a fork), and a leaf pad with no shoot yet
/// (the pad keeps its foliage; the shoot pokes out of it and wants
/// pinching, which is the loop that keeps a finished-looking tree in
/// play). Watering buds up to two sites, a plain day one on a coin flip,
/// a dry day none; a weak or thirsty tree does not bud.
fn bud_once(
    graph: &mut BonsaiGraph,
    seed: i64,
    age_days: i64,
    vigor: i32,
    water_stress: i32,
    cause: GrowthCause,
) -> Vec<i32> {
    let attempts: usize = match cause {
        GrowthCause::Water => 2,
        GrowthCause::Daily if hash_parts(seed, age_days as u64, 13) % 100 < 50 => 1,
        GrowthCause::Daily | GrowthCause::DryDay => 0,
    };
    if attempts == 0 || vigor < 40 || water_stress >= 60 {
        return Vec::new();
    }
    let mut sites = graph
        .branches
        .iter()
        .filter(|branch| branch.is_alive() && branch.end_y >= 2)
        .filter_map(|branch| {
            let children = graph
                .branches
                .iter()
                .filter(|candidate| candidate.parent_id == Some(branch.id) && candidate.is_alive())
                .collect::<Vec<_>>();
            match (branch.status, children.as_slice()) {
                (BranchStatus::LeafPad, []) => Some((branch.id, 0)),
                (_, [child]) => Some((branch.id, (child.end_x - branch.end_x).signum())),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    sites.sort_by_key(|(id, _)| hash_parts(seed, age_days as u64, *id as u64));

    let mut budded = Vec::new();
    for (site_id, child_dx) in sites {
        if budded.len() >= attempts || graph.branches.len() >= MAX_BRANCHES {
            break;
        }
        let side = if child_dx != 0 {
            -child_dx
        } else if hash_parts(seed, site_id as u64, 17).is_multiple_of(2) {
            -1
        } else {
            1
        };
        let bud_vigor = (vigor - water_stress / 2).clamp(20, 95) as i16;
        let bud = [(side, 1), (side, 0), (-side, 1)]
            .into_iter()
            .find_map(|(dx, dy)| graph.add_branch(site_id, dx, dy, 1, 1, bud_vigor));
        if let Some(id) = bud {
            budded.push(id);
        }
    }
    budded
}

fn target_in_canvas(target: (i16, i16)) -> bool {
    target.0.abs() <= TIP_MAX_ABS_X && target.1 >= 1 && target.1 <= TIP_MAX_Y
}

/// Cuts every branch whose end lies outside the canvas, and everything
/// downstream of it. Trees planted before the fixed canvas (2026-09-08)
/// can be larger than the pot; this runs at load, and the tree keeps
/// growing under the pot's rules from what survived. Returns how many
/// glyphs were removed.
fn repot_into_canvas(graph: &mut BonsaiGraph) -> usize {
    let outside = graph
        .branches
        .iter()
        .filter(|branch| {
            branch.id != ROOT_BRANCH_ID && !target_in_canvas((branch.end_x, branch.end_y))
        })
        .map(|branch| branch.id)
        .collect::<Vec<_>>();
    if outside.is_empty() {
        return 0;
    }
    let mut doomed = BTreeSet::new();
    for id in outside {
        doomed.insert(id);
        doomed.extend(descendant_ids(graph, id));
    }
    graph.branches.retain(|branch| !doomed.contains(&branch.id));
    doomed.len()
}

fn repot_message(removed_count: usize) -> String {
    format!("Repotted: cut back {removed_count} glyphs to fit the new pot")
}

fn branch_target(parent: &Branch, dx: i16, dy: i16) -> (i16, i16) {
    (
        parent.end_x + dx.signum(),
        (parent.end_y + dy.signum()).max(1),
    )
}

fn growth_target_is_open(graph: &BonsaiGraph, parent_id: i32, target: (i16, i16)) -> bool {
    let Some(parent) = graph.branch(parent_id) else {
        return false;
    };
    let source = (parent.end_x, parent.end_y);
    if target == source || !target_in_canvas(target) {
        return false;
    }

    for branch in &graph.branches {
        if matches!(branch.status, BranchStatus::Cut) {
            continue;
        }
        for point in [
            (branch.start_x, branch.start_y),
            (branch.end_x, branch.end_y),
        ] {
            if point == source {
                continue;
            }
            if point == target {
                return false;
            }
        }
        if segments_cross_between_cells(
            source,
            target,
            (branch.start_x, branch.start_y),
            (branch.end_x, branch.end_y),
        ) {
            return false;
        }
    }
    true
}

fn segments_cross_between_cells(
    a_start: (i16, i16),
    a_end: (i16, i16),
    b_start: (i16, i16),
    b_end: (i16, i16),
) -> bool {
    if a_start == b_start || a_start == b_end || a_end == b_start || a_end == b_end {
        return false;
    }
    let a_dx = a_end.0 - a_start.0;
    let a_dy = a_end.1 - a_start.1;
    let b_dx = b_end.0 - b_start.0;
    let b_dy = b_end.1 - b_start.1;
    if a_dx.abs() != 1 || a_dy.abs() != 1 || b_dx.abs() != 1 || b_dy.abs() != 1 {
        return false;
    }
    let same_cell_box = a_start.0.min(a_end.0) == b_start.0.min(b_end.0)
        && a_start.0.max(a_end.0) == b_start.0.max(b_end.0)
        && a_start.1.min(a_end.1) == b_start.1.min(b_end.1)
        && a_start.1.max(a_end.1) == b_start.1.max(b_end.1);
    same_cell_box && a_dx.signum() * a_dy.signum() != b_dx.signum() * b_dy.signum()
}

fn points_are_adjacent(a: (i16, i16), b: (i16, i16)) -> bool {
    let dx = (a.0 - b.0).abs();
    let dy = (a.1 - b.1).abs();
    dx <= 1 && dy <= 1
}

fn growth_wave_budget(
    cause: GrowthCause,
    vigor: i32,
    water_stress: i32,
    tip_count: usize,
) -> usize {
    if tip_count == 0 || vigor <= 8 {
        return 0;
    }
    let base: usize = match cause {
        GrowthCause::Water => 4,
        GrowthCause::Daily => 3,
        GrowthCause::DryDay if water_stress >= 60 => 3,
        GrowthCause::DryDay => 2,
    };
    let vigor_bonus: usize = if vigor >= 85 {
        2
    } else if vigor >= 65 {
        1
    } else {
        0
    };
    let stress_penalty: usize = if water_stress >= 85 {
        2
    } else if water_stress >= 60 && !matches!(cause, GrowthCause::DryDay) {
        1
    } else {
        0
    };
    (base + vigor_bonus)
        .saturating_sub(stress_penalty)
        .clamp(1, MAX_GROWTH_WAVE_TIPS)
        .min(tip_count)
}

fn growth_tip_order(
    graph: &BonsaiGraph,
    tips: &[i32],
    seed: i64,
    age_days: i64,
    preferred_tip_id: Option<i32>,
    budget: usize,
) -> Vec<i32> {
    let mut ordered = tips
        .iter()
        .copied()
        .filter(|id| {
            graph
                .branch(*id)
                .is_some_and(|branch| branch.last_pruned_day.is_some())
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|id| {
        (
            graph.branch(*id).and_then(|branch| branch.last_pruned_day),
            *id,
        )
    });

    if let Some(preferred_tip_id) = preferred_tip_id
        && tips.contains(&preferred_tip_id)
        && !ordered.contains(&preferred_tip_id)
    {
        ordered.push(preferred_tip_id);
    }

    let mut remaining = tips
        .iter()
        .copied()
        .filter(|id| !ordered.contains(id))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|id| hash_parts(seed, age_days as u64, *id as u64));
    ordered.extend(remaining);
    ordered.truncate(budget);
    ordered
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
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal if branch.bend_x != 0 => 0,
        std::cmp::Ordering::Equal => 1,
    };
    (step_x, step_y)
}

fn side_shoot_threshold(cause: GrowthCause, _tip: &Branch, vigor: i32, water_stress: i32) -> u64 {
    let base = match cause {
        GrowthCause::Water => 6,
        GrowthCause::Daily => 4,
        GrowthCause::DryDay => 24,
    };
    let vigor_bonus = if water_stress <= 35 {
        ((vigor - 55).max(0) / 8).clamp(0, 6)
    } else {
        0
    };
    let stress_bonus = if water_stress >= 60 {
        ((water_stress - 55) / 4).clamp(0, 20)
    } else {
        0
    };
    (base + vigor_bonus + stress_bonus).clamp(0, 70) as u64
}

fn side_shoot_step(seed: i64, next_id: u64, cause: GrowthCause, water_stress: i32) -> (i16, i16) {
    let side = if hash_parts(seed, next_id, 7).is_multiple_of(2) {
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

fn clean_cut_message(removed_count: usize) -> String {
    if removed_count == 1 {
        "Clean cut: tip removed".to_string()
    } else {
        format!("Clean cut: removed {removed_count} branch glyphs")
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
    badge_glyph_for_score(score).to_string()
}

fn badge_glyph_for_score(score: i32) -> &'static str {
    match score {
        0..=16 => "·",
        17..=40 => "⚘",
        41..=80 => "🌱",
        81..=150 => "🌲",
        151..=240 => "🌳",
        241..=360 => "🌸",
        _ => "🌼",
    }
}

fn leaf_weight(branch: &Branch) -> i32 {
    match branch.status {
        BranchStatus::LeafPad => 8,
        BranchStatus::Growing | BranchStatus::Wired => 3,
        BranchStatus::Pinched | BranchStatus::NeedsPinch => 2,
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
        BranchStatus::Growing if branch.last_pruned_day.is_some() => "split marked",
        BranchStatus::Growing if branch.ramification > 0 => "pinch-trained tip",
        BranchStatus::Growing => "growing tip",
        BranchStatus::Wired if branch.last_pruned_day.is_some() => "wired split marked",
        BranchStatus::Wired if branch.ramification > 0 => "wired pinch-trained tip",
        BranchStatus::Wired => "wired tip",
        BranchStatus::Pinched => "pinched; waiting",
        BranchStatus::NeedsPinch => "ready to pinch",
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
#[path = "state_test.rs"]
mod state_test;
