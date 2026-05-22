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
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    branch_id: Option<i32>,
    kind: CellKind,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CellKind {
    Branch,
    Deadwood,
    Leaf,
    Scar,
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
        };
    }

    let pot = "[=======]";
    let pot_width = pot.chars().count();
    let mut grid = vec![vec![None; width]; height];
    let pot_y = height.saturating_sub(1);
    let origin_x = width / 2;
    let trunk_base_y = pot_y.saturating_sub(1);

    for branch in &state.graph.branches {
        plot_branch(&mut grid, branch, origin_x as isize, trunk_base_y as isize);
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
                    let mut style = Style::default().fg(color_for_char(ch, state));
                    if selected {
                        style = style
                            .fg(theme::AMBER_GLOW())
                            .bg(theme::BG_SELECTION())
                            .add_modifier(Modifier::BOLD);
                    }
                    Span::styled(ch.to_string(), style)
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn plot_branch(grid: &mut [Vec<Option<Cell>>], branch: &Branch, origin_x: isize, origin_y: isize) {
    let start = map_point(branch.start_x, branch.start_y, origin_x, origin_y);
    let end = map_point(branch.end_x, branch.end_y, origin_x, origin_y);
    let ch = branch_glyph(branch);
    let kind = match branch.status {
        BranchStatus::Cut => CellKind::Scar,
        BranchStatus::Deadwood => CellKind::Deadwood,
        _ => CellKind::Branch,
    };
    for (x, y) in line_points(start, end) {
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
    if matches!(branch.status, BranchStatus::Cut) {
        put_signed(
            grid,
            end.0,
            end.1,
            Cell {
                ch: '\'',
                branch_id: Some(branch.id),
                kind: CellKind::Scar,
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
        BranchStatus::Growing | BranchStatus::Wired if branch.age >= 3 => 1,
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
    if dx.abs() <= dy.abs() / 2 {
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

fn color_for_char(ch: char, state: &BonsaiV2State) -> ratatui::style::Color {
    match ch {
        '[' | ']' | '=' => theme::TEXT_DIM(),
        '@' | '#' | 'o' | '*' | '.' | ',' | '\'' => {
            if state.water_stress >= 60 {
                theme::AMBER_DIM()
            } else {
                theme::BONSAI_CANOPY()
            }
        }
        '/' | '\\' | '|' | '_' | '~' => {
            if state.is_alive {
                theme::AMBER()
            } else {
                theme::TEXT_FAINT()
            }
        }
        '`' => theme::TEXT_FAINT(),
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
