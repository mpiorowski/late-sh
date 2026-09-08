use super::*;

fn test_bonsai_service() -> BonsaiService {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("test db");
    let (tx, _) = tokio::sync::broadcast::channel(1);
    BonsaiService::new(db, tx)
}

fn state_for_graph(graph: BonsaiGraph, selected_branch_id: Option<i32>) -> BonsaiState {
    let today = BonsaiService::today();
    BonsaiState {
        user_id: Uuid::nil(),
        svc: test_bonsai_service(),
        seed: 42,
        planted_at: Utc::now(),
        last_watered: None,
        is_alive: true,
        vigor: 70,
        water_stress: 0,
        last_simulated_date: today,
        age_days: 0,
        graph,
        selected_branch_id,
        mode: BonsaiMode::Inspect,
        message: None,
        state_revision: 0,
        decay_protection: None,
    }
}

fn graph_with_two_editable_tips() -> BonsaiGraph {
    let mut graph = seeded_graph(42);
    graph
        .add_branch(ROOT_BRANCH_ID, -1, 1, 1, 1, 65)
        .expect("left tip");
    graph
        .add_branch(ROOT_BRANCH_ID, 1, 1, 1, 1, 65)
        .expect("right tip");
    graph
}

fn graph_with_two_isolated_tips() -> BonsaiGraph {
    let mut graph = seeded_graph(42);
    let left_1 = graph
        .add_branch(ROOT_BRANCH_ID, -1, 1, 1, 1, 65)
        .expect("left child");
    let left_2 = graph.add_branch(left_1, -1, 1, 1, 1, 65).expect("left mid");
    graph.add_branch(left_2, -1, 1, 1, 1, 65).expect("left tip");
    let right_1 = graph
        .add_branch(ROOT_BRANCH_ID, 1, 1, 1, 1, 65)
        .expect("right child");
    let right_2 = graph
        .add_branch(right_1, 1, 1, 1, 1, 65)
        .expect("right mid");
    graph
        .add_branch(right_2, 1, 1, 1, 1, 65)
        .expect("right tip");
    graph
}

fn first_editable_tip(graph: &BonsaiGraph) -> i32 {
    graph
        .branches
        .iter()
        .find(|branch| branch.id != ROOT_BRANCH_ID && graph.is_tip(branch.id))
        .expect("editable tip")
        .id
}

fn test_branch(id: i32, parent_id: Option<i32>, start: (i16, i16), end: (i16, i16)) -> Branch {
    Branch {
        id,
        parent_id,
        start_x: start.0,
        start_y: start.1,
        end_x: end.0,
        end_y: end.1,
        thickness: 1,
        age: 0,
        vigor: 70,
        status: BranchStatus::Growing,
        bend_x: 0,
        bend_y: 0,
        last_pruned_day: None,
        ramification: 0,
        last_pinched_age: None,
    }
}

#[test]
fn badge_score_ladder_uses_doubled_thresholds() {
    assert_eq!(badge_glyph_for_score(16), "·");
    assert_eq!(badge_glyph_for_score(17), "⚘");
    assert_eq!(badge_glyph_for_score(40), "⚘");
    assert_eq!(badge_glyph_for_score(41), "🌱");
    assert_eq!(badge_glyph_for_score(80), "🌱");
    assert_eq!(badge_glyph_for_score(81), "🌲");
    assert_eq!(badge_glyph_for_score(150), "🌲");
    assert_eq!(badge_glyph_for_score(151), "🌳");
    assert_eq!(badge_glyph_for_score(240), "🌳");
    assert_eq!(badge_glyph_for_score(241), "🌸");
    assert_eq!(badge_glyph_for_score(360), "🌸");
    assert_eq!(badge_glyph_for_score(361), "🌼");
}

#[test]
fn growth_target_allows_adjacent_unrelated_branch() {
    let mut graph = BonsaiGraph {
        version: 1,
        next_id: 4,
        branches: vec![
            test_branch(ROOT_BRANCH_ID, None, (0, 0), (0, 0)),
            test_branch(2, Some(ROOT_BRANCH_ID), (0, 0), (0, 1)),
            test_branch(3, Some(ROOT_BRANCH_ID), (2, 2), (1, 2)),
        ],
    };

    let grown = grow_tip_once(&mut graph, 2, 42, 75, 0, GrowthCause::Water);

    assert!(grown.is_some());
    assert_eq!(graph.branches.len(), 4);
}

#[test]
fn growth_target_blocks_crossing_branch() {
    let mut crossing_tip = test_branch(2, Some(ROOT_BRANCH_ID), (0, 0), (0, 1));
    crossing_tip.bend_x = 1;
    crossing_tip.bend_y = 1;
    let mut graph = BonsaiGraph {
        version: 1,
        next_id: 4,
        branches: vec![
            test_branch(ROOT_BRANCH_ID, None, (0, 0), (0, 0)),
            crossing_tip,
            test_branch(3, Some(ROOT_BRANCH_ID), (0, 2), (1, 1)),
        ],
    };

    let grown = grow_tip_once(&mut graph, 2, 42, 75, 0, GrowthCause::Water);

    // The wired step (0,1) -> (1,2) would cross branch 3, so it is refused;
    // the tip may still take one of its natural fallbacks instead.
    let crossing_taken = graph
        .branches
        .iter()
        .any(|branch| (branch.end_x, branch.end_y) == (1, 2));
    assert!(!crossing_taken);
    if let Some(new_id) = grown {
        let new_branch = graph.branch(new_id).expect("grown branch");
        assert_ne!((new_branch.end_x, new_branch.end_y), (1, 2));
    }
}

#[test]
fn same_source_forks_can_grow_adjacent_cells() {
    let mut graph = seeded_graph(42);
    let vertical = graph
        .add_branch(ROOT_BRANCH_ID, 0, 1, 1, 1, 65)
        .expect("vertical child");
    let side = graph
        .add_branch(ROOT_BRANCH_ID, -1, 1, 1, 1, 65)
        .expect("same-source side child");

    assert_eq!(graph.branch(vertical).map(|branch| branch.end_x), Some(0));
    assert_eq!(graph.branch(side).map(|branch| branch.end_x), Some(-1));
}

#[test]
fn pruning_finds_descendants_for_clean_removal() {
    let graph = graph_with_two_isolated_tips();
    let selected = graph
        .branches
        .iter()
        .find(|branch| branch.id != ROOT_BRANCH_ID)
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
fn seeded_graph_starts_as_one_locked_root_segment() {
    let graph = seeded_graph(42);
    assert_eq!(graph.branches.len(), 1);
    assert_eq!(graph.next_id, 2);
    let trunk = graph.branch(ROOT_BRANCH_ID).expect("trunk");
    assert_eq!((trunk.start_x, trunk.start_y), (0, 0));
    assert_eq!((trunk.end_x, trunk.end_y), (0, 0));
    assert_eq!(trunk.status, BranchStatus::Growing);

    let mut state = state_for_graph(graph, Some(ROOT_BRANCH_ID));
    let rendered = crate::app::bonsai::render::render_ascii(&state, 9, 4, false);
    assert_eq!(rendered.occupied_cells, 1);

    state.prune_selected();
    assert_eq!(state.graph.branches.len(), 1);
    assert_eq!(
        state.message.as_deref(),
        Some("Hard trunk cuts are disabled")
    );
    state.split_selected();
    assert_eq!(
        state
            .graph
            .branch(ROOT_BRANCH_ID)
            .and_then(|branch| branch.last_pruned_day),
        None
    );
    assert_eq!(state.message.as_deref(), Some("The trunk will not split"));
    state.pinch_selected();
    let trunk = state.graph.branch(ROOT_BRANCH_ID).expect("trunk");
    assert_eq!(trunk.status, BranchStatus::Growing);
    assert_eq!(trunk.ramification, 0);
    assert_eq!(state.message.as_deref(), Some("The trunk will not pinch"));
}

#[tokio::test]
async fn respawn_resets_age_anchor_and_advances_revision() {
    let old_planted_at = Utc::now() - chrono::Duration::days(12);
    let mut state = state_for_graph(graph_with_two_isolated_tips(), None);
    state.planted_at = old_planted_at;
    state.age_days = 12;
    state.state_revision = 7;

    state.respawn();

    assert_eq!(state.age_days, 0);
    assert!(state.planted_at > old_planted_at);
    assert_eq!(state.state_revision, 8);
}

#[test]
fn root_growth_ignores_split_marker_and_creates_one_branch() {
    let mut graph = seeded_graph(42);
    graph.branch_mut(ROOT_BRANCH_ID).unwrap().last_pruned_day = Some(0);

    let new_id = grow_tip_once(&mut graph, ROOT_BRANCH_ID, 42, 75, 0, GrowthCause::Water)
        .expect("root growth");

    assert_eq!(graph.child_ids(ROOT_BRANCH_ID), vec![new_id]);
    let child = graph.branch(new_id).expect("first branch");
    assert_eq!((child.start_x, child.start_y), (0, 0));
    assert_eq!((child.end_x, child.end_y), (0, 1));
    assert_eq!(
        graph
            .branch(ROOT_BRANCH_ID)
            .and_then(|branch| branch.last_pruned_day),
        None
    );
}

#[test]
fn pinched_tip_waits_then_needs_pinching() {
    let mut graph = graph_with_two_editable_tips();
    let tip_id = first_editable_tip(&graph);
    graph.branch_mut(tip_id).unwrap().status = BranchStatus::Pinched;

    let grown = grow_graph_once(&mut graph, 42, 0, 75, 0, GrowthCause::Water, Some(tip_id));

    assert!(grown.iter().all(|(source_id, _)| *source_id != tip_id));
    assert_eq!(
        graph.branch(tip_id).map(|branch| branch.status),
        Some(BranchStatus::NeedsPinch)
    );
}

#[test]
fn grown_graphs_use_one_cell_segments() {
    let mut graph = seeded_graph(42);
    for age_days in 0..12 {
        let _ = grow_graph_once(&mut graph, 42, age_days, 90, 0, GrowthCause::Water, None);
    }

    assert!(graph.branches.len() > 1);
    assert!(graph.branches.iter().all(|branch| branch.length() <= 1));
}

#[test]
fn growth_adds_child_segment_without_extending_source() {
    let mut graph = graph_with_two_editable_tips();
    let tip_id = first_editable_tip(&graph);
    let before = graph.branch(tip_id).unwrap().clone();

    let new_id = grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water).unwrap();

    assert_eq!(graph.branch(tip_id).unwrap().end_x, before.end_x);
    assert_eq!(graph.branch(tip_id).unwrap().end_y, before.end_y);
    assert_eq!(graph.branch(new_id).unwrap().parent_id, Some(tip_id));
    assert_eq!(graph.branch(new_id).unwrap().length(), 1);
}

#[test]
fn downward_wire_grows_a_drooping_segment() {
    let mut graph = graph_with_two_isolated_tips();
    let tip_id = first_editable_tip(&graph);
    let tip_before = graph.branch(tip_id).unwrap().clone();
    graph.branch_mut(tip_id).unwrap().bend_y = -1;

    let new_id = grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water).unwrap();
    let child = graph.branch(new_id).unwrap();

    assert_eq!(child.end_y, tip_before.end_y - 1);
    assert!(child.end_y >= 1);
}

#[test]
fn growth_wave_advances_multiple_tips() {
    let mut graph = graph_with_two_editable_tips();
    let before = graph.branches.len();

    let grown = grow_graph_once(&mut graph, 42, 0, 75, 0, GrowthCause::Water, None);

    assert!(grown.len() >= 2);
    assert!(graph.branches.len() >= before + grown.len());
}

#[test]
fn growth_wave_prioritizes_pending_split_tips() {
    let mut graph = graph_with_two_isolated_tips();
    let extra_tip_id = first_editable_tip(&graph);
    let preferred_tip_id = graph
        .branches
        .iter()
        .find(|branch| {
            branch.id != extra_tip_id && branch.id != ROOT_BRANCH_ID && graph.is_tip(branch.id)
        })
        .unwrap()
        .id;
    graph.branch_mut(extra_tip_id).unwrap().last_pruned_day = Some(0);

    let grown = grow_graph_once(
        &mut graph,
        42,
        0,
        20,
        20,
        GrowthCause::Daily,
        Some(preferred_tip_id),
    );

    assert!(
        grown
            .iter()
            .any(|(source_id, _)| *source_id == extra_tip_id)
    );
    assert!(
        graph
            .child_ids(extra_tip_id)
            .iter()
            .filter(|id| graph.branch(**id).is_some_and(Branch::is_tip_candidate))
            .count()
            >= 2
    );
}

#[test]
fn marked_tip_splits_on_next_growth() {
    let mut graph = graph_with_two_isolated_tips();
    let tip_id = first_editable_tip(&graph);
    graph.branch_mut(tip_id).unwrap().last_pruned_day = Some(0);

    grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water).unwrap();

    assert_eq!(graph.child_ids(tip_id).len(), 2);
    assert_eq!(graph.branch(tip_id).unwrap().last_pruned_day, None);
}

#[test]
fn growth_keeps_ramification_on_cutback_spot() {
    let mut graph = graph_with_two_editable_tips();
    let tip_id = first_editable_tip(&graph);
    graph.branch_mut(tip_id).unwrap().ramification = 2;

    let new_id = grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water).unwrap();

    assert_eq!(graph.branch(tip_id).unwrap().ramification, 2);
    assert_eq!(graph.branch(new_id).unwrap().ramification, 0);
}

#[test]
fn stress_raises_side_shoot_chance() {
    let graph = graph_with_two_editable_tips();
    let tip_id = first_editable_tip(&graph);
    let tip = graph.branch(tip_id).unwrap().clone();
    let plain = side_shoot_threshold(GrowthCause::Water, &tip, 70, 0);
    let stressed = side_shoot_threshold(GrowthCause::DryDay, &tip, 35, 80);

    assert!(stressed > plain);
}

#[test]
fn simulate_day_holds_stress_and_vigor_steady_on_a_protected_day() {
    let mut state = state_for_graph(seeded_graph(42), None);
    state.water_stress = 50;
    state.vigor = 50;
    state.last_watered = None; // would otherwise count as a dry day
    let today = BonsaiService::today();
    state.decay_protection = Some(BonsaiDecayProtection {
        starts_at: Utc::now() - chrono::Duration::days(1),
        ends_at: Utc::now() + chrono::Duration::days(13),
    });

    state.simulate_day(today);

    assert_eq!(state.water_stress, 50);
    assert_eq!(state.vigor, 50);
    assert!(state.is_alive);
}

#[test]
fn simulate_day_applies_dry_day_penalty_without_protection() {
    let mut state = state_for_graph(seeded_graph(42), None);
    state.water_stress = 50;
    state.vigor = 50;
    state.last_watered = None;
    state.decay_protection = None;

    state.simulate_day(BonsaiService::today());

    assert_eq!(state.water_stress, 61);
    assert_eq!(state.vigor, 43);
}

#[test]
fn simulate_day_protection_does_not_extend_past_its_window() {
    let mut state = state_for_graph(seeded_graph(42), None);
    state.water_stress = 50;
    state.vigor = 50;
    state.last_watered = None;
    state.decay_protection = Some(BonsaiDecayProtection {
        starts_at: Utc::now() - chrono::Duration::days(20),
        ends_at: Utc::now() - chrono::Duration::days(1),
    });

    state.simulate_day(BonsaiService::today());

    // The window already lapsed, so today gets the normal dry-day penalty.
    assert_eq!(state.water_stress, 61);
    assert_eq!(state.vigor, 43);
}

#[test]
fn simulate_day_still_recovers_on_a_watered_day_under_protection() {
    let today = BonsaiService::today();
    let mut state = state_for_graph(seeded_graph(42), None);
    state.water_stress = 50;
    state.vigor = 50;
    state.last_watered = Some(today);
    state.decay_protection = Some(BonsaiDecayProtection {
        starts_at: Utc::now() - chrono::Duration::days(1),
        ends_at: Utc::now() + chrono::Duration::days(13),
    });

    state.simulate_day(today);

    // The shield neutralizes dry days; it must never cancel the recovery a
    // watered day earns, or buying it would leave a tended tree worse off.
    assert_eq!(state.water_stress, 46);
    assert_eq!(state.vigor, 52);
}

#[test]
fn simulate_day_protection_keeps_an_already_spent_tree_alive() {
    let mut state = state_for_graph(seeded_graph(42), None);
    state.water_stress = 100;
    state.vigor = 0;
    state.last_watered = None;
    state.decay_protection = Some(BonsaiDecayProtection {
        starts_at: Utc::now() - chrono::Duration::days(1),
        ends_at: Utc::now() + chrono::Duration::days(13),
    });

    state.simulate_day(BonsaiService::today());

    assert!(state.is_alive);
}

/// One watering a day, for everyone: the second press the same day is
/// refused and leaves the tree exactly as the first left it.
#[tokio::test]
async fn a_second_watering_the_same_day_is_refused() {
    let mut state = state_for_graph(seeded_graph(42), None);
    state.vigor = 50;
    state.water_stress = 40;

    assert!(state.water(DailyWaterGate::Enforced));
    let today = BonsaiService::today();
    assert_eq!(state.last_watered, Some(today));
    assert_eq!(state.vigor, 68);
    assert_eq!(state.water_stress, 5);
    let snapshot = |state: &BonsaiState| {
        (
            serde_json::to_value(&state.graph).expect("graph json"),
            state.state_revision,
            state.vigor,
            state.water_stress,
        )
    };
    let after_first = snapshot(&state);

    assert!(!state.water(DailyWaterGate::Enforced));
    assert_eq!(state.message.as_deref(), Some("Already watered today"));
    assert_eq!(snapshot(&state), after_first);
}

/// The admin bypass is a testing aid: a second press the same day waters
/// again and moves the tree, where the enforced gate would refuse it.
#[tokio::test]
async fn the_admin_bypass_waters_again_the_same_day() {
    let mut state = state_for_graph(seeded_graph(42), None);
    state.vigor = 50;
    state.water_stress = 40;

    assert!(state.water(DailyWaterGate::Enforced));
    assert_eq!(state.vigor, 68);
    assert_eq!(state.water_stress, 5);

    assert!(state.water(DailyWaterGate::AdminBypass));
    assert_eq!(state.vigor, 86);
    assert_eq!(state.water_stress, 0);
    assert_eq!(
        state.message.as_deref(),
        Some("Watered: vigor pushed new growth")
    );
}

/// A tip at its length budget forks instead of extending, so the tree
/// gains density rather than height.
#[test]
fn a_tip_at_its_run_budget_forks_instead_of_extending() {
    let mut graph = graph_with_two_isolated_tips();
    let tip_id = first_editable_tip(&graph);
    assert_eq!(order_and_run(&graph, tip_id), (1, 3));
    let extended = grow_tip_once(&mut graph, tip_id, 42, 75, 0, GrowthCause::Water)
        .expect("one more cell under budget");
    assert_eq!(order_and_run(&graph, extended), (1, 4));

    grow_tip_once(&mut graph, extended, 42, 75, 0, GrowthCause::Water).expect("fork");

    assert_eq!(graph.child_ids(extended).len(), 2);
}

/// Nothing grows past the canvas edge, whichever path adds the branch.
#[test]
fn growth_never_leaves_the_canvas() {
    let mut graph = seeded_graph(42);
    for age_days in 0..60 {
        let _ = grow_graph_once(&mut graph, 42, age_days, 90, 0, GrowthCause::Water, None);
    }

    assert!(graph.branches.len() > 10);
    assert!(
        graph
            .branches
            .iter()
            .all(|branch| { branch.end_x.abs() <= TIP_MAX_ABS_X && branch.end_y <= TIP_MAX_Y })
    );
}

/// A tree planted before the fixed canvas is cut back to the pot at load,
/// downstream branches included, and the trunk is never touched.
#[test]
fn repot_cuts_everything_outside_the_canvas() {
    let mut graph = seeded_graph(42);
    let inside = graph
        .add_branch(ROOT_BRANCH_ID, 1, 1, 1, 1, 65)
        .expect("inside");
    let outside_x = TIP_MAX_ABS_X + 7;
    graph
        .branches
        .push(test_branch(50, Some(inside), (1, 1), (outside_x, 3)));
    graph
        .branches
        .push(test_branch(51, Some(50), (outside_x, 3), (outside_x, 4)));
    graph.next_id = 52;

    let removed = repot_into_canvas(&mut graph);

    assert_eq!(removed, 2);
    assert!(graph.branch(ROOT_BRANCH_ID).is_some());
    assert!(graph.branch(inside).is_some());
    assert!(graph.branch(50).is_none());
    assert!(graph.branch(51).is_none());
}

/// A full tree still cycles its pinches: the wave's bookkeeping runs at
/// the cap even though nothing new can be added.
#[test]
fn a_pinch_still_sets_on_a_full_tree() {
    let mut graph = graph_with_two_editable_tips();
    let tip_id = first_editable_tip(&graph);
    graph.branch_mut(tip_id).unwrap().status = BranchStatus::Pinched;
    while graph.branches.len() < MAX_BRANCHES {
        let filler = graph.branches.len() as i32 + 100;
        graph
            .branches
            .push(test_branch(filler, None, (30, 20), (30, 20)));
    }

    let grown = grow_graph_once(&mut graph, 42, 0, 75, 0, GrowthCause::Water, None);

    assert!(grown.is_empty());
    assert_eq!(graph.branches.len(), MAX_BRANCHES);
    assert_eq!(
        graph.branch(tip_id).map(|branch| branch.status),
        Some(BranchStatus::NeedsPinch)
    );
}
