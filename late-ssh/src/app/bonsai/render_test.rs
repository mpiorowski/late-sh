use super::*;
use crate::app::bonsai::svc::BonsaiService;
use crate::app::bonsai_v2::state::BonsaiGraph;
use uuid::Uuid;

fn test_bonsai_service() -> BonsaiService {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("test db");
    let (tx, _) = tokio::sync::broadcast::channel(1);
    BonsaiService::new(db, tx)
}

fn state_with_branches(branches: Vec<Branch>) -> BonsaiV2State {
    let mut state = BonsaiV2State::fallback(Uuid::nil(), test_bonsai_service(), 42);
    state.graph = BonsaiGraph {
        version: 1,
        next_id: branches
            .iter()
            .map(|branch| branch.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
        branches,
    };
    state
}

fn branch(id: i32, parent_id: Option<i32>, start: (i16, i16), end: (i16, i16)) -> Branch {
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
fn child_segments_do_not_redraw_parent_joint() {
    let mut grid = vec![vec![None; 8]; 4];
    let root = branch(1, None, (0, 0), (1, 0));
    let child = branch(2, Some(1), (1, 0), (2, 0));
    let branches = vec![root.clone(), child.clone()];

    plot_branch(&mut grid, &branches, &root, 2, 2);
    plot_branch(&mut grid, &branches, &child, 2, 2);

    let occupied = grid.iter().flatten().filter(|cell| cell.is_some()).count();
    assert_eq!(occupied, 3);
    assert_eq!(grid[2][3].map(|cell| cell.branch_id), Some(Some(1)));
    assert_eq!(grid[2][4].map(|cell| cell.branch_id), Some(Some(2)));
}

#[test]
fn horizontal_child_after_rising_diagonal_uses_upper_horizontal_glyph() {
    let mut grid = vec![vec![None; 8]; 5];
    let parent = branch(1, None, (0, 0), (1, 1));
    let child = branch(2, Some(1), (1, 1), (2, 1));
    let branches = vec![parent.clone(), child.clone()];

    plot_branch(&mut grid, &branches, &parent, 2, 3);
    plot_branch(&mut grid, &branches, &child, 2, 3);

    assert_eq!(grid[2][3].map(|cell| cell.branch_id), Some(Some(1)));
    assert_eq!(grid[2][4].map(|cell| cell.branch_id), Some(Some(2)));
    assert_eq!(grid[2][4].map(|cell| cell.ch), Some('¯'));
    assert_eq!(grid[1][4].map(|cell| cell.branch_id), None);
}

#[test]
fn the_canvas_is_the_one_size_with_the_pot_on_the_last_row() {
    let trunk = branch(1, None, (0, 0), (0, 4));
    let mut left = branch(2, Some(1), (0, 4), (-7, 8));
    left.status = BranchStatus::LeafPad;
    let state = state_with_branches(vec![trunk, left]);

    let rendered = render_ascii(&state, CANVAS_WIDTH, CANVAS_HEIGHT, false);

    assert_eq!(rendered.lines.len(), CANVAS_HEIGHT);
    assert!(
        rendered
            .lines
            .iter()
            .all(|line| line.chars().count() == CANVAS_WIDTH)
    );
    // The trunk roots on the row above the pot, at the center column.
    assert_eq!(
        rendered.lines[CANVAS_HEIGHT - 2].chars().nth(CANVAS_WIDTH / 2),
        Some('|')
    );
    assert!(rendered.lines[CANVAS_HEIGHT - 1].contains("[=======]"));
    assert!(rendered.selected_cells.is_empty());
}

/// A tree that fits the preview box is the modal's own glyphs: nothing
/// merged, nothing invented, the pot on the last row.
#[test]
fn preview_keeps_exact_glyphs_when_the_tree_fits() {
    let trunk = branch(1, None, (0, 0), (0, 3));
    let mut left = branch(2, Some(1), (0, 3), (-3, 6));
    left.status = BranchStatus::LeafPad;
    let state = state_with_branches(vec![trunk, left]);

    let rendered = render_preview_ascii(&state);

    assert_eq!(rendered.lines.len(), PREVIEW_HEIGHT);
    assert!(rendered.lines[PREVIEW_HEIGHT - 1].contains("[=====]"));
    assert_eq!(
        rendered.lines[PREVIEW_HEIGHT - 2].chars().nth(PREVIEW_WIDTH / 2),
        Some('|')
    );
    let joined = rendered.lines.join("\n");
    assert!(joined.contains('\\'), "expected the real diagonal glyph: {joined}");
}

/// A tree wider than the block is scaled by one factor on both axes, so
/// it keeps the modal's proportions, and the pot stays on the last row.
#[test]
fn preview_scales_a_wide_tree_uniformly() {
    let trunk = branch(1, None, (0, 0), (0, 2));
    let mut left = branch(2, Some(1), (0, 2), (-20, 5));
    left.status = BranchStatus::LeafPad;
    let mut right = branch(3, Some(1), (0, 2), (20, 5));
    right.status = BranchStatus::LeafPad;
    let state = state_with_branches(vec![trunk, left, right]);

    let rendered = render_preview_ascii(&state);

    assert_eq!(rendered.lines.len(), PREVIEW_HEIGHT);
    assert!(
        rendered
            .lines
            .iter()
            .all(|line| line.chars().count() == PREVIEW_WIDTH)
    );
    assert!(rendered.lines[PREVIEW_HEIGHT - 1].contains("[=====]"));
    assert!(rendered.occupied_cells > 0);
    // Six tree rows at scale 2 land on three, not stretched to fill twelve.
    let tree_rows = rendered.lines[..PREVIEW_HEIGHT - 1]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .count();
    assert!(tree_rows <= 4, "expected a squat tree, got {tree_rows} rows");
}

#[test]
fn horizontal_run_after_rising_diagonal_keeps_upper_glyph() {
    let mut grid = vec![vec![None; 9]; 5];
    let parent = branch(1, None, (0, 0), (1, 1));
    let child = branch(2, Some(1), (1, 1), (2, 1));
    let grandchild = branch(3, Some(2), (2, 1), (3, 1));
    let branches = vec![parent.clone(), child.clone(), grandchild.clone()];

    plot_branch(&mut grid, &branches, &parent, 2, 3);
    plot_branch(&mut grid, &branches, &child, 2, 3);
    plot_branch(&mut grid, &branches, &grandchild, 2, 3);

    assert_eq!(grid[2][4].map(|cell| cell.ch), Some('¯'));
    assert_eq!(grid[2][5].map(|cell| cell.ch), Some('¯'));
}

#[test]
fn diagonal_glyphs_follow_actual_slope() {
    let right_up = branch(1, None, (0, 0), (1, 1));
    let left_down = branch(2, None, (0, 1), (-1, 0));
    let left_up = branch(3, None, (0, 0), (-1, 1));
    let right_down = branch(4, None, (0, 1), (1, 0));

    assert_eq!(branch_glyph(&right_up, &[]), '/');
    assert_eq!(branch_glyph(&left_down, &[]), '/');
    assert_eq!(branch_glyph(&left_up, &[]), '\\');
    assert_eq!(branch_glyph(&right_down, &[]), '\\');
}
