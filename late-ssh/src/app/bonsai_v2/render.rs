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
    let mut lines = render_preview_lines(state, area.width as usize, tree_height);

    while lines.len() < tree_height {
        lines.insert(0, Line::from(""));
    }

    let mut footer = if state.is_alive {
        let status = if state.water_stress >= 60 {
            "dry"
        } else if state.water_stress >= 25 {
            "watch"
        } else {
            "alive"
        };
        vec![
            Span::styled(
                format!("{}d", state.age_days),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(" · ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(status, Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" · ", Style::default().fg(theme::BORDER_DIM())),
        ]
    } else {
        vec![Span::styled(
            "rip",
            Style::default().fg(theme::TEXT_FAINT()),
        )]
    };
    if state.is_alive {
        footer.push(Span::styled(
            "w care",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ));
    }
    lines.push(Line::from(footer).centered());

    frame.render_widget(Paragraph::new(lines), area);
}

pub(crate) fn render_preview_lines(
    state: &BonsaiV2State,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let rendered = render_preview_ascii(state, width, height);
    rendered_lines(state, &rendered, false)
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

fn render_preview_ascii(state: &BonsaiV2State, width: usize, height: usize) -> RenderedBonsai {
    if width == 0 || height == 0 {
        return RenderedBonsai {
            lines: Vec::new(),
            selected_cells: Vec::new(),
            occupied_cells: 0,
            cell_kinds: Vec::new(),
        };
    }

    let pot_height = 1usize;
    let tree_height = height.saturating_sub(pot_height);
    if tree_height == 0 {
        return render_pot_only(width, height);
    }

    let samples = preview_samples(state);
    if samples.is_empty() {
        return render_pot_only(width, height);
    }

    let bounds = preview_bounds(&samples);
    let horizontal_radius = bounds.max_x.abs().max(bounds.min_x.abs()).max(1) as f32;
    let world_width = horizontal_radius * 2.0 + 1.0;
    let world_height = (bounds.max_y - bounds.min_y + 1).max(1) as f32;
    let scale = (world_width / width.max(1) as f32)
        .max(world_height / tree_height.max(1) as f32)
        .max(1.0);

    let mut preview = vec![vec![PreviewCell::default(); width]; tree_height];
    let origin_x = width.saturating_sub(1) as isize / 2;
    for sample in samples {
        let x =
            (origin_x + (sample.x as f32 / scale).round() as isize).clamp(0, width as isize - 1);
        let y = ((sample.y - bounds.min_y) as f32 / scale).round() as isize;
        let row = (tree_height as isize - 1 - y).clamp(0, tree_height as isize - 1);
        let Some(cell) = preview
            .get_mut(row as usize)
            .and_then(|row| row.get_mut(x as usize))
        else {
            continue;
        };
        cell.add(sample.kind);
    }

    let mut grid = vec![vec![None; width]; height];
    for (y, row) in preview.into_iter().enumerate() {
        for (x, cell) in row.into_iter().enumerate() {
            if let Some(kind) = cell.dominant_kind() {
                put(
                    &mut grid,
                    x,
                    y,
                    Cell {
                        ch: cell.preview_glyph(kind),
                        branch_id: None,
                        kind,
                    },
                );
            }
        }
    }

    draw_preview_pot(&mut grid, width, height - 1);
    rendered_from_grid(grid)
}

#[derive(Clone, Copy, Default)]
struct PreviewCell {
    branch: u16,
    deadwood: u16,
    pinched: u16,
    needs_pinch: u16,
    leaf: u16,
}

impl PreviewCell {
    fn add(&mut self, kind: CellKind) {
        match kind {
            CellKind::Branch => self.branch = self.branch.saturating_add(1),
            CellKind::Deadwood => self.deadwood = self.deadwood.saturating_add(1),
            CellKind::Pinched => self.pinched = self.pinched.saturating_add(1),
            CellKind::NeedsPinch => self.needs_pinch = self.needs_pinch.saturating_add(1),
            CellKind::Leaf => self.leaf = self.leaf.saturating_add(1),
            CellKind::Pot => {}
        }
    }

    fn total(self) -> u16 {
        self.branch + self.deadwood + self.pinched + self.needs_pinch + self.leaf
    }

    fn dominant_kind(self) -> Option<CellKind> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        if self.leaf >= self.branch
            && self.leaf >= self.deadwood
            && self.leaf >= self.pinched
            && self.leaf >= self.needs_pinch
        {
            Some(CellKind::Leaf)
        } else if self.needs_pinch >= self.branch
            && self.needs_pinch >= self.deadwood
            && self.needs_pinch >= self.pinched
        {
            Some(CellKind::NeedsPinch)
        } else if self.pinched >= self.branch && self.pinched >= self.deadwood {
            Some(CellKind::Pinched)
        } else if self.deadwood > self.branch {
            Some(CellKind::Deadwood)
        } else {
            Some(CellKind::Branch)
        }
    }

    fn preview_glyph(self, kind: CellKind) -> char {
        match kind {
            CellKind::Leaf if self.total() >= 5 => '#',
            CellKind::Leaf if self.total() >= 3 => '*',
            CellKind::Leaf => '@',
            CellKind::Deadwood => '\'',
            CellKind::Pinched => '+',
            CellKind::NeedsPinch => 'o',
            CellKind::Branch if self.total() >= 4 => '#',
            CellKind::Branch if self.total() >= 2 => '*',
            CellKind::Branch => '|',
            CellKind::Pot => '=',
        }
    }
}

#[derive(Clone, Copy)]
struct PreviewSample {
    x: isize,
    y: isize,
    kind: CellKind,
}

#[derive(Clone, Copy)]
struct PreviewBounds {
    min_x: isize,
    max_x: isize,
    min_y: isize,
    max_y: isize,
}

fn preview_samples(state: &BonsaiV2State) -> Vec<PreviewSample> {
    let mut samples = Vec::new();
    for branch in &state.graph.branches {
        if matches!(branch.status, BranchStatus::Cut) {
            continue;
        }
        let kind = match branch.status {
            BranchStatus::Deadwood => CellKind::Deadwood,
            BranchStatus::Pinched => CellKind::Pinched,
            BranchStatus::NeedsPinch => CellKind::NeedsPinch,
            BranchStatus::LeafPad => CellKind::Leaf,
            _ => CellKind::Branch,
        };
        let mut points = line_points(
            (branch.start_x as isize, branch.start_y as isize),
            (branch.end_x as isize, branch.end_y as isize),
        );
        if branch.parent_id.is_some() && points.len() > 1 {
            points.remove(0);
        }
        samples.extend(
            points
                .into_iter()
                .map(|(x, y)| PreviewSample { x, y, kind }),
        );

        if matches!(branch.status, BranchStatus::LeafPad) {
            samples.extend([(0, 0), (-1, 0), (1, 0), (0, 1), (0, -1)].into_iter().map(
                |(dx, dy)| PreviewSample {
                    x: branch.end_x as isize + dx,
                    y: branch.end_y as isize + dy,
                    kind: CellKind::Leaf,
                },
            ));
        }
    }
    samples
}

fn preview_bounds(samples: &[PreviewSample]) -> PreviewBounds {
    let mut bounds = PreviewBounds {
        min_x: 0,
        max_x: 0,
        min_y: 0,
        max_y: 0,
    };
    for sample in samples {
        bounds.min_x = bounds.min_x.min(sample.x);
        bounds.max_x = bounds.max_x.max(sample.x);
        bounds.min_y = bounds.min_y.min(sample.y);
        bounds.max_y = bounds.max_y.max(sample.y);
    }
    bounds
}

fn render_pot_only(width: usize, height: usize) -> RenderedBonsai {
    let mut grid = vec![vec![None; width]; height];
    if height > 0 {
        draw_preview_pot(&mut grid, width, height - 1);
    }
    rendered_from_grid(grid)
}

fn draw_preview_pot(grid: &mut [Vec<Option<Cell>>], width: usize, y: usize) {
    let pot = if width >= 9 {
        "[=====]"
    } else if width >= 5 {
        "[=]"
    } else {
        "="
    };
    let pot_width = pot.chars().count();
    let pot_x = width.saturating_sub(pot_width) / 2;
    for (i, ch) in pot.chars().enumerate() {
        put(
            grid,
            pot_x + i,
            y,
            Cell {
                ch,
                branch_id: None,
                kind: CellKind::Pot,
            },
        );
    }
}

fn rendered_from_grid(grid: Vec<Vec<Option<Cell>>>) -> RenderedBonsai {
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
        selected_cells: Vec::new(),
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
    let ch = branch_glyph(branch, branches);
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
            y,
            Cell {
                ch,
                branch_id: Some(branch.id),
                kind,
            },
        );
    }
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

fn branch_glyph(branch: &Branch, branches: &[Branch]) -> char {
    if matches!(branch.status, BranchStatus::Deadwood) {
        return '`';
    }
    let dx = branch.end_x - branch.start_x;
    let dy = branch.end_y - branch.start_y;
    if dy == 0 && dx != 0 {
        if horizontal_branch_uses_upper_glyph(branch, branches) {
            '¯'
        } else {
            '_'
        }
    } else if dx.abs() <= dy.abs() / 2 {
        '|'
    } else if dx.signum() == dy.signum() {
        '/'
    } else {
        '\\'
    }
}

fn horizontal_branch_uses_upper_glyph(branch: &Branch, branches: &[Branch]) -> bool {
    let mut current = branch;
    let mut remaining_hops = branches.len();
    while remaining_hops > 0 {
        remaining_hops -= 1;
        let Some(parent) = current
            .parent_id
            .and_then(|parent_id| branches.iter().find(|candidate| candidate.id == parent_id))
        else {
            return false;
        };
        if branch_rises(parent) {
            return true;
        }
        if parent.end_y == parent.start_y && parent.end_x != parent.start_x {
            current = parent;
            continue;
        }
        return false;
    }
    false
}

fn branch_rises(branch: &Branch) -> bool {
    branch.end_y > branch.start_y && branch.end_x != branch.start_x
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
#[path = "render_test.rs"]
mod render_test;
