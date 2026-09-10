use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    bonsai::state::{BonsaiState, Branch, BranchStatus, CANVAS_HEIGHT, CANVAS_WIDTH},
    common::theme,
};

/// The preview block: one fixed size for the sidebar and the profile, the
/// sidebar's width, so both show the same picture of the tree and the
/// scale is the same in each. Twelve tree rows over the pot row.
pub(crate) const PREVIEW_WIDTH: usize = 21;
pub(crate) const PREVIEW_HEIGHT: usize = 13;

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

pub(crate) fn draw_bonsai_inline(
    frame: &mut Frame,
    area: Rect,
    state: &BonsaiState,
    wall_tick: usize,
) {
    if area.height < 3 || area.width < 10 {
        return;
    }

    let footer_height = 1usize;
    let tree_height = (area.height as usize).saturating_sub(footer_height);
    if (area.width as usize) < PREVIEW_WIDTH || tree_height < PREVIEW_HEIGHT {
        return;
    }
    let mut lines = render_preview_lines(state);
    apply_sway(&mut lines, wall_tick);
    center_lines(&mut lines, area.width as usize, PREVIEW_WIDTH);

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

/// Small idle sway off the shared wall tick: shift the upper lines
/// horizontally by up to one column, fading to none at the pot. Applied
/// to finished lines, so any selection highlighting embedded in them
/// moves with the art. A left shift trims one leading blank; a line with
/// no leading blank (tree at the edge) just skips that step. A wall_tick
/// of 0 (a static surface) lands on sin(0) and never moves.
pub(crate) fn apply_sway(lines: &mut [Line<'static>], wall_tick: usize) {
    let count = lines.len();
    if count < 2 {
        return;
    }
    let sway_base = (wall_tick as f64 * 0.132).sin(); // ~3s period at 66ms ticks
    for (i, line) in lines.iter_mut().enumerate() {
        let line_factor = 1.0 - (i as f64 / (count - 1) as f64);
        let offset = (sway_base * line_factor).round() as i32;
        if offset > 0 {
            line.spans.insert(0, Span::raw(" "));
        } else if offset < 0
            && let Some(first) = line.spans.first_mut()
            && let Some(rest) = first.content.strip_prefix(' ')
        {
            first.content = rest.to_string().into();
        }
    }
}

/// The tree at its one true size: the whole canvas, pot on the last row,
/// trunk rooted at the center column. The care modal draws exactly this
/// block; the preview below is a scaled reading of it.
pub(crate) fn canvas_lines(state: &BonsaiState, show_selection: bool) -> Vec<Line<'static>> {
    render_tree_lines(state, CANVAS_WIDTH, CANVAS_HEIGHT, show_selection)
}

/// The preview: the true canvas fitted into the fixed `PREVIEW_WIDTH` x
/// `PREVIEW_HEIGHT` block for the sidebar and the profile. It never
/// invents anything. When the tree fits, this is the modal's own glyphs,
/// trimmed around the trunk. When it does not, one integer scale factor
/// is applied to both axes, so the block keeps the modal's proportions;
/// bare rows (trunk and branch only, no foliage) are dropped from the pot
/// upward only as far as needed to stop the height forcing a larger
/// factor than the width already does, so a long trunk is what gets
/// shortened and the crown keeps its detail. A cell that gathers one
/// sample keeps that sample's glyph; a cell that gathers several takes
/// its dominant kind, foliage as density glyphs (`@`, `*`, `#`),
/// structure as its commonest glyph.
pub(crate) fn render_preview_lines(state: &BonsaiState) -> Vec<Line<'static>> {
    let rendered = render_preview_ascii(state);
    rendered_lines(state, &rendered, false)
}

/// Lead each line with blanks so a `block_width`-wide block sits centered
/// in a wider area. A block already wider than the area is left alone.
pub(crate) fn center_lines(lines: &mut [Line<'static>], width: usize, block_width: usize) {
    let left = width.saturating_sub(block_width) / 2;
    if left == 0 {
        return;
    }
    for line in lines.iter_mut() {
        line.spans.insert(0, Span::raw(" ".repeat(left)));
    }
}

/// The one bonsai renderer: the graph plotted 1:1 into a
/// `width` x `height` grid, the pot on the last row and the trunk rooted
/// at the horizontal center. Nothing is scaled; whatever falls outside
/// the grid is cut off, which the growth rules make impossible at the
/// canvas size (`canvas_lines`).
pub(crate) fn render_tree_lines(
    state: &BonsaiState,
    width: usize,
    height: usize,
    show_selection: bool,
) -> Vec<Line<'static>> {
    let rendered = render_ascii(state, width, height, show_selection);
    rendered_lines(state, &rendered, show_selection)
}

pub(crate) fn render_ascii(
    state: &BonsaiState,
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
    let mut grid = plot_tree(state, width, height);
    let pot_y = height.saturating_sub(1);
    let origin_x = width / 2;

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

/// The tree's cells (branches, then leaf pads) plotted into a grid with
/// the trunk base on the row above the last one, at the center column.
/// The last row is left for the pot.
fn plot_tree(state: &BonsaiState, width: usize, height: usize) -> Vec<Vec<Option<Cell>>> {
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

    // A pad is foliage whether or not a bud has grown out of it, so a
    // budding pad reads as a shoot poking out of leaves, not as a pad
    // that vanished.
    for branch in &state.graph.branches {
        if branch.is_alive() {
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
    grid
}

fn render_preview_ascii(state: &BonsaiState) -> RenderedBonsai {
    render_fitted_ascii(state, PREVIEW_WIDTH, PREVIEW_HEIGHT)
}

fn render_fitted_ascii(state: &BonsaiState, width: usize, height: usize) -> RenderedBonsai {
    let width = width.max(3);
    let height = height.max(2);
    let tree_height = height - 1;

    let full = plot_tree(state, CANVAS_WIDTH, CANVAS_HEIGHT);
    let origin_x = (CANVAS_WIDTH / 2) as isize;

    // The rows that hold anything, top to bottom, above the pot row.
    let mut rows = (0..CANVAS_HEIGHT - 1)
        .filter(|y| full[*y].iter().any(Option::is_some))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return render_pot_only(width, height);
    }

    let half = rows
        .iter()
        .flat_map(|y| {
            full[*y]
                .iter()
                .enumerate()
                .filter(|(_, cell)| cell.is_some())
                .map(|(x, _)| (x as isize - origin_x).abs())
        })
        .max()
        .unwrap_or(0) as usize;
    let width_scale = (2 * half + 1).div_ceil(width).max(1);

    // Shorten bare structure only as far as the width's factor needs:
    // while the height would force a larger factor, drop the lowest bare
    // row, the trunk base (the last occupied row) always staying.
    while rows.len().div_ceil(tree_height) > width_scale {
        let base = rows[rows.len() - 1];
        let Some(bare_index) = rows
            .iter()
            .rposition(|y| *y != base && row_is_bare(&full[*y]))
        else {
            break;
        };
        rows.remove(bare_index);
    }

    // One factor for both axes, so the block keeps the modal's shape.
    let scale = width_scale.max(rows.len().div_ceil(tree_height).max(1));
    let (sx, sy) = (scale, scale);

    let out_origin = (width / 2) as isize;
    let mut acc = vec![vec![PreviewCell::default(); width]; tree_height];
    for (index, y) in rows.iter().enumerate() {
        let from_bottom = (rows.len() - 1 - index) / sy;
        let out_y = tree_height.saturating_sub(1).saturating_sub(from_bottom);
        for (x, cell) in full[*y].iter().enumerate() {
            let Some(cell) = cell else {
                continue;
            };
            let offset = x as isize - origin_x;
            let out_x = (out_origin + (offset as f32 / sx as f32).round() as isize)
                .clamp(0, width as isize - 1) as usize;
            acc[out_y][out_x].add(*cell);
        }
    }

    let mut grid = vec![vec![None; width]; height];
    for (y, row) in acc.into_iter().enumerate() {
        for (x, cell) in row.into_iter().enumerate() {
            if let Some((ch, kind)) = cell.resolve() {
                put(
                    &mut grid,
                    x,
                    y,
                    Cell {
                        ch,
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

/// A row with structure only: trunk, branches, deadwood, no foliage and
/// nothing mid-pinch.
fn row_is_bare(row: &[Option<Cell>]) -> bool {
    row.iter()
        .flatten()
        .all(|cell| matches!(cell.kind, CellKind::Branch | CellKind::Deadwood))
}

/// What one preview cell gathered from the true canvas.
#[derive(Clone, Default)]
struct PreviewCell {
    total: u16,
    leaf: u16,
    needs_pinch: u16,
    pinched: u16,
    deadwood: u16,
    branch: u16,
    glyphs: std::collections::BTreeMap<char, u16>,
    first: Option<(char, CellKind)>,
}

impl PreviewCell {
    fn add(&mut self, cell: Cell) {
        self.total = self.total.saturating_add(1);
        match cell.kind {
            CellKind::Leaf => self.leaf += 1,
            CellKind::NeedsPinch => self.needs_pinch += 1,
            CellKind::Pinched => self.pinched += 1,
            CellKind::Deadwood => self.deadwood += 1,
            CellKind::Branch => self.branch += 1,
            CellKind::Pot => {}
        }
        *self.glyphs.entry(cell.ch).or_insert(0) += 1;
        if self.first.is_none() {
            self.first = Some((cell.ch, cell.kind));
        }
    }

    fn resolve(&self) -> Option<(char, CellKind)> {
        let (first_ch, first_kind) = self.first?;
        if self.total == 1 {
            return Some((first_ch, first_kind));
        }
        let kind = if self.leaf >= self.needs_pinch
            && self.leaf >= self.pinched
            && self.leaf >= self.deadwood
            && self.leaf >= self.branch
        {
            CellKind::Leaf
        } else if self.needs_pinch >= self.pinched
            && self.needs_pinch >= self.deadwood
            && self.needs_pinch >= self.branch
        {
            CellKind::NeedsPinch
        } else if self.pinched >= self.deadwood && self.pinched >= self.branch {
            CellKind::Pinched
        } else if self.deadwood > self.branch {
            CellKind::Deadwood
        } else {
            CellKind::Branch
        };
        let ch = match kind {
            CellKind::Leaf if self.total >= 5 => '#',
            CellKind::Leaf if self.total >= 3 => '*',
            CellKind::Leaf => '@',
            CellKind::NeedsPinch => 'o',
            CellKind::Pinched => '+',
            CellKind::Deadwood => '\'',
            CellKind::Branch | CellKind::Pot => self
                .glyphs
                .iter()
                .max_by_key(|(ch, count)| (**count, std::cmp::Reverse(**ch)))
                .map(|(ch, _)| *ch)
                .unwrap_or(first_ch),
        };
        Some((ch, kind))
    }
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
    state: &BonsaiState,
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
                        style = style
                            .patch(theme::selection_style())
                            .add_modifier(Modifier::BOLD);
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

fn color_for_cell(kind: Option<CellKind>, state: &BonsaiState) -> ratatui::style::Color {
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
