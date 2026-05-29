use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    bonsai_v2::state::{BonsaiV2State, Branch, BranchStatus},
    common::theme,
};

#[derive(Debug, Clone)]
pub(crate) struct RenderedBonsai {
    pub lines: Vec<String>,
    pub selected_cells: Vec<(usize, usize)>,
    pub occupied_cells: usize,
    cell_kinds: Vec<Vec<Option<CellKind>>>,
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    branch_id: Option<i32>,
    kind: CellKind,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CellKind {
    Branch,
    Deadwood,
    Pinched,
    NeedsPinch,
    Leaf,
    Pot,
}

pub(crate) fn draw_bonsai_inline(frame: &mut Frame, area: Rect, state: &BonsaiV2State, _beat: f32) {
    if area.height < 3 || area.width < 10 {
        return;
    }

    let footer_height = 1usize;
    let tree_height = (area.height as usize).saturating_sub(footer_height);
    let rendered = render_ascii(state, area.width as usize, tree_height, false);
    let mut lines = rendered_lines(state, &rendered, false);

    while lines.len() < tree_height {
        lines.insert(0, Line::from(""));
    }

    let status = if !state.is_alive {
        "rip".to_string()
    } else if state.water_stress >= 60 {
        "dry".to_string()
    } else if state.water_stress >= 25 {
        "watch".to_string()
    } else {
        "alive".to_string()
    };
    lines.push(
        Line::from(vec![
            Span::styled(
                format!("{}d", state.age_days),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::raw("  "),
            Span::styled(status, Style::default().fg(theme::AMBER_DIM())),
        ])
        .centered(),
    );

    frame.render_widget(Paragraph::new(lines), area);
}

pub(crate) fn render_tree_lines(
    state: &BonsaiV2State,
    width: usize,
    height: usize,
    show_selection: bool,
) -> Vec<Line<'static>> {
    let rendered = render_ascii(state, width, height, show_selection);
    rendered_lines(state, &rendered, show_selection)
}

pub(crate) fn render_ascii(
    state: &BonsaiV2State,
    width: usize,
    height: usize,
    show_selection: bool,
) -> RenderedBonsai {
    if width == 0 || height == 0 {
        return RenderedBonsai {
            lines: Vec::new(),
            selected_cells: Vec::new(),
            occupied_cells: 0,
            cell_kinds: Vec::new(),
        };
    }

    let pot = "[=======]";
    let pot_width = pot.chars().count();
    let mut grid = vec![vec![None; width]; height];
    let pot_y = height.saturating_sub(1);
    let origin_x = width / 2;
    let trunk_base_y = pot_y.saturating_sub(1);

    for branch in &state.graph.branches {
        plot_branch(
            &mut grid,
            &state.graph.branches,
            branch,
            origin_x as isize,
            trunk_base_y as isize,
        );
    }

    for branch in &state.graph.branches {
        if branch.is_alive() && state.graph.is_tip(branch.id) {
            plot_leaf_pad(
                &mut grid,
                branch,
                origin_x as isize,
                trunk_base_y as isize,
                state.seed,
                state.vigor,
                state.water_stress,
            );
        }
    }

    let pot_x = origin_x.saturating_sub(pot_width / 2);
    for (i, ch) in pot.chars().enumerate() {
        put(
            &mut grid,
            pot_x + i,
            pot_y,
            Cell {
                ch,
                branch_id: None,
                kind: CellKind::Pot,
            },
        );
    }

    let mut selected_cells = Vec::new();
    if show_selection && let Some(selected_id) = state.selected_branch_id {
        for (y, row) in grid.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                if cell.is_some_and(|cell| cell.branch_id == Some(selected_id)) {
                    selected_cells.push((x, y));
                }
            }
        }
    }

    let occupied_cells = grid
        .iter()
        .flatten()
        .filter(|cell| cell.is_some_and(|cell| cell.kind != CellKind::Pot))
        .count();

    let cell_kinds = grid
        .iter()
        .map(|row| row.iter().map(|cell| cell.map(|cell| cell.kind)).collect())
        .collect();

    let lines = grid
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| cell.map_or(' ', |cell| cell.ch))
                .collect::<String>()
        })
        .collect();

    RenderedBonsai {
        lines,
        selected_cells,
        occupied_cells,
        cell_kinds,
    }
}

fn rendered_lines(
    state: &BonsaiV2State,
    rendered: &RenderedBonsai,
    show_selection: bool,
) -> Vec<Line<'static>> {
    rendered
        .lines
        .iter()
        .enumerate()
        .map(|(y, line)| {
            let spans = line
                .chars()
                .enumerate()
                .map(|(x, ch)| {
                    let selected = show_selection && rendered.selected_cells.contains(&(x, y));
                    let kind = rendered
                        .cell_kinds
                        .get(y)
                        .and_then(|row| row.get(x))
                        .copied()
                        .flatten();
                    let mut style = Style::default().fg(color_for_cell(kind, state));
                    if selected {
                        style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
                    }
                    Span::styled(ch.to_string(), style)
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn plot_branch(
    grid: &mut [Vec<Option<Cell>>],
    branches: &[Branch],
    branch: &Branch,
    origin_x: isize,
    origin_y: isize,
) {
    if matches!(branch.status, BranchStatus::Cut) {
        return;
    }
    let start = map_point(branch.start_x, branch.start_y, origin_x, origin_y);
    let end = map_point(branch.end_x, branch.end_y, origin_x, origin_y);
    let visual_offset = visual_offset(branches, branch);
    let ch = branch_glyph(branch);
    let kind = match branch.status {
        BranchStatus::Deadwood => CellKind::Deadwood,
        BranchStatus::Pinched => CellKind::Pinched,
        BranchStatus::NeedsPinch => CellKind::NeedsPinch,
        BranchStatus::LeafPad => CellKind::Leaf,
        _ => CellKind::Branch,
    };
    let mut points = line_points(start, end);
    if branch.parent_id.is_some() && points.len() > 1 {
        points.remove(0);
    }
    for (x, y) in points {
        put_signed(
            grid,
            x,
            y + visual_offset,
            Cell {
                ch,
                branch_id: Some(branch.id),
                kind,
            },
        );
    }
}

fn visual_offset(branches: &[Branch], branch: &Branch) -> isize {
    let is_horizontal = branch.end_y == branch.start_y && branch.end_x != branch.start_x;
    if !is_horizontal {
        return 0;
    }
    let Some(parent_id) = branch.parent_id else {
        return 0;
    };
    let Some(parent) = branches.iter().find(|candidate| candidate.id == parent_id) else {
        return 0;
    };
    let parent_rises = parent.end_y > parent.start_y && parent.end_x != parent.start_x;
    if parent_rises { -1 } else { 0 }
}

fn plot_leaf_pad(
    grid: &mut [Vec<Option<Cell>>],
    branch: &Branch,
    origin_x: isize,
    origin_y: isize,
    seed: i64,
    vigor: i32,
    stress: i32,
) {
    if matches!(branch.status, BranchStatus::Cut | BranchStatus::Deadwood) {
        return;
    }
    let (x, y) = map_point(branch.end_x, branch.end_y, origin_x, origin_y);
    let radius = match branch.status {
        BranchStatus::LeafPad if vigor >= 70 && stress < 35 => 2,
        BranchStatus::LeafPad => 1,
        _ => 0,
    };
    if radius == 0 {
        return;
    }
    let offsets = if radius == 2 {
        &[
            (0, 0),
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-2, 0),
            (2, 0),
        ][..]
    } else {
        &[(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)][..]
    };
    for (idx, (dx, dy)) in offsets.iter().copied().enumerate() {
        put_signed(
            grid,
            x + dx,
            y + dy,
            Cell {
                ch: leaf_glyph(seed, branch.id, idx, stress),
                branch_id: Some(branch.id),
                kind: CellKind::Leaf,
            },
        );
    }
}

fn branch_glyph(branch: &Branch) -> char {
    if matches!(branch.status, BranchStatus::Deadwood) {
        return '`';
    }
    let dx = branch.end_x - branch.start_x;
    let dy = branch.end_y - branch.start_y;
    if dy == 0 && dx != 0 {
        '_'
    } else if dx.abs() <= dy.abs() / 2 {
        '|'
    } else if dx > 0 {
        '/'
    } else if dx < 0 {
        '\\'
    } else {
        '_'
    }
}

fn leaf_glyph(seed: i64, branch_id: i32, idx: usize, stress: i32) -> char {
    if stress >= 60 {
        return match idx % 3 {
            0 => '.',
            1 => ',',
            _ => '\'',
        };
    }
    match ((seed.unsigned_abs() as usize) + branch_id as usize + idx) % 5 {
        0 => '@',
        1 => '#',
        2 => 'o',
        3 => '.',
        _ => '*',
    }
}

fn color_for_cell(kind: Option<CellKind>, state: &BonsaiV2State) -> ratatui::style::Color {
    match kind {
        Some(CellKind::Pot) => theme::TEXT_DIM(),
        Some(CellKind::Leaf) => {
            if state.water_stress >= 60 {
                theme::AMBER_DIM()
            } else {
                theme::BONSAI_CANOPY()
            }
        }
        Some(CellKind::Branch) => {
            if state.is_alive {
                theme::AMBER()
            } else {
                theme::TEXT_FAINT()
            }
        }
        Some(CellKind::Pinched) => theme::AMBER_GLOW(),
        Some(CellKind::NeedsPinch) => theme::BONSAI_SPROUT(),
        Some(CellKind::Deadwood) => theme::TEXT_FAINT(),
        _ => theme::TEXT_FAINT(),
    }
}

fn map_point(x: i16, y: i16, origin_x: isize, origin_y: isize) -> (isize, isize) {
    (origin_x + x as isize, origin_y - y as isize)
}

fn line_points(start: (isize, isize), end: (isize, isize)) -> Vec<(isize, isize)> {
    let (mut x0, mut y0) = start;
    let (x1, y1) = end;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut points = Vec::new();

    loop {
        points.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }

    points
}

fn put_signed(grid: &mut [Vec<Option<Cell>>], x: isize, y: isize, cell: Cell) {
    if x < 0 || y < 0 {
        return;
    }
    put(grid, x as usize, y as usize, cell);
}

fn put(grid: &mut [Vec<Option<Cell>>], x: usize, y: usize, cell: Cell) {
    let Some(row) = grid.get_mut(y) else {
        return;
    };
    let Some(slot) = row.get_mut(x) else {
        return;
    };
    if slot.is_some_and(|existing| existing.kind == CellKind::Pot) {
        return;
    }
    *slot = Some(cell);
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn horizontal_child_after_rising_diagonal_draws_one_row_higher() {
        let mut grid = vec![vec![None; 8]; 5];
        let parent = branch(1, None, (0, 0), (1, 1));
        let child = branch(2, Some(1), (1, 1), (2, 1));
        let branches = vec![parent.clone(), child.clone()];

        plot_branch(&mut grid, &branches, &parent, 2, 3);
        plot_branch(&mut grid, &branches, &child, 2, 3);

        assert_eq!(grid[2][3].map(|cell| cell.branch_id), Some(Some(1)));
        assert_eq!(grid[1][4].map(|cell| cell.branch_id), Some(Some(2)));
        assert_eq!(grid[2][4].map(|cell| cell.branch_id), None);
    }
}
