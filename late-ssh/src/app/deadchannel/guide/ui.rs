//! The guide over the street: one framed box in the city's own palette,
//! the sections under neon headings, each block drawn its own way (the
//! loop as a chain, the prize boxed, keys as chips in a grid, rules as
//! bullets), scrolled by the state. Pure.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};

use super::data::{self, Figure, Key, SECTIONS};
use super::state::State;
use crate::app::common::markdown::wrap_spans;
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{INK, INK_BRIGHT, INK_DIM, glow, ink, lit};

/// The box: as wide as the frame allows up to this, and as tall.
const MAX_WIDTH: u16 = 84;
const MAX_HEIGHT: u16 = 40;
/// The warm ground a key chip sits on, a step up from the night.
const CHIP_BG: Color = Color::Rgb(58, 44, 16);
/// Columns between two cells of a grid.
const GAP: usize = 3;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let popup = centered_rect(area, MAX_WIDTH, MAX_HEIGHT);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(" the street, explained ", lit(Neon::Amber)))
        .borders(Borders::ALL)
        .border_style(glow(Neon::Amber))
        .padding(Padding::new(2, 2, 1, 0))
        .style(ink(INK));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let [body, keys] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);

    let lines = body_lines(usize::from(body.width));
    state.record_page(lines.len() as u16, body.height);
    frame.render_widget(Paragraph::new(lines).scroll((state.scroll(), 0)), body);

    let dim_text = ink(INK_DIM);
    let mut footer = Vec::new();
    footer.extend(chip("j/k"));
    footer.push(Span::styled(" scroll   ", dim_text));
    footer.extend(chip("Esc"));
    footer.push(Span::styled(" close   ", dim_text));
    footer.extend(chip("?"));
    footer.push(Span::styled(
        " opens this again, any time on the street",
        dim_text,
    ));
    frame.render_widget(
        Paragraph::new(vec![Line::default(), Line::from(footer)]),
        keys,
    );
}

/// The copy as rows `width` wide: a neon heading per section, its blocks
/// under it, a blank between blocks and between sections. Already
/// wrapped, so the row count is what the scroll holds against.
pub(crate) fn body_lines(width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (i, section) in SECTIONS.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(heading(section.title, section.neon, width));
        let mut last_rule = false;
        for block in section.blocks {
            let rule = matches!(block, data::Block::Rule(_));
            // Bullets run together; every other block breathes.
            if !(rule && last_rule) {
                lines.push(Line::default());
            }
            last_rule = rule;
            match block {
                data::Block::Loop(stages) => lines.extend(loop_lines(stages, width)),
                data::Block::Prize { title, lines: copy } => {
                    lines.extend(prize_lines(title, copy, width))
                }
                data::Block::Keys(keys) => lines.extend(key_lines(keys, width)),
                data::Block::Figures(figures) => lines.extend(figure_lines(figures, width)),
                data::Block::Rule(text) => lines.extend(rule_lines(text, width)),
            }
        }
    }
    lines
}

/// `▚ the street ──────`, the rule running to the edge.
fn heading(title: &'static str, neon: Neon, width: usize) -> Line<'static> {
    let used = 2 + title.chars().count() + 1;
    Line::from(vec![
        Span::styled("▚ ", glow(neon)),
        Span::styled(title, lit(neon)),
        Span::styled(" ", ink(INK)),
        Span::styled("─".repeat(width.saturating_sub(used)), glow(neon)),
    ])
}

/// The stages bright, joined by amber arrows, wrapped to the width.
fn loop_lines(stages: &[&'static str], width: usize) -> Vec<Line<'static>> {
    let stage = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    for (i, text) in stages.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ▸ ", lit(Neon::Amber)));
        }
        spans.push(Span::styled(*text, stage));
    }
    indented(&spans, Span::raw("  "), "  ", width)
}

/// A rounded box in amber, its title in the top edge, the lines inside
/// wrapped and padded to the box.
fn prize_lines(title: &'static str, copy: &[&'static str], width: usize) -> Vec<Line<'static>> {
    let edge = glow(Neon::Amber);
    let text = ink(INK_BRIGHT);
    let strong = lit(Neon::Amber);
    // Two columns of indent, a border and a space on each side.
    let inside = width.saturating_sub(2 + 4).max(1);
    let rows: Vec<Vec<Span<'static>>> = copy
        .iter()
        .flat_map(|line| wrap_spans(&marked(line, text, strong), inside, inside))
        .collect();
    let content = rows.iter().map(|row| spans_width(row)).max().unwrap_or(0);
    let inner = content.max(title.chars().count() + 3);

    let mut lines = Vec::new();
    let top_fill = (inner + 2).saturating_sub(title.chars().count() + 3);
    lines.push(Line::from(vec![
        Span::styled("  ╭─ ", edge),
        Span::styled(title, lit(Neon::Amber)),
        Span::styled(format!(" {}╮", "─".repeat(top_fill)), edge),
    ]));
    for row in rows {
        let pad = inner.saturating_sub(spans_width(&row));
        let mut spans = vec![Span::styled("  │ ", edge)];
        spans.extend(row);
        spans.push(Span::styled(" ".repeat(pad), text));
        spans.push(Span::styled(" │", edge));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        format!("  ╰{}╯", "─".repeat(inner + 2)),
        edge,
    )));
    lines
}

/// Keys as chips beside what they do, in as many columns as fit.
fn key_lines(keys: &[Key], width: usize) -> Vec<Line<'static>> {
    let key_width = keys.iter().map(|k| k.key.chars().count() + 2).max().unwrap_or(0);
    let text = ink(INK);
    let cells = keys
        .iter()
        .map(|k| {
            let mut head = chip(k.key);
            head.push(Span::styled(
                " ".repeat(key_width - (k.key.chars().count() + 2) + 1),
                text,
            ));
            (head, vec![Span::styled(k.does, text)])
        })
        .collect();
    grid(cells, key_width + 1, width)
}

/// The day's numbers, each bright beside what it means.
fn figure_lines(figures: &[Figure], width: usize) -> Vec<Line<'static>> {
    let value_width = figures.iter().map(|f| f.value.chars().count()).max().unwrap_or(0);
    let number = lit(Neon::Amber);
    let text = ink(INK);
    let cells = figures
        .iter()
        .map(|f| {
            let head = vec![Span::styled(
                format!("{:<w$}  ", f.value, w = value_width),
                number,
            )];
            (head, vec![Span::styled(f.means, text)])
        })
        .collect();
    grid(cells, value_width + 2, width)
}

/// Cells of a fixed-width head and a tail, laid out row by row in as many
/// columns as the widest cell allows. One column wraps each tail under
/// itself, past the head, so a narrow frame loses nothing.
fn grid(
    cells: Vec<(Vec<Span<'static>>, Vec<Span<'static>>)>,
    head_width: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let indent = 2;
    let cell_width = cells
        .iter()
        .map(|(_, tail)| head_width + spans_width(tail))
        .max()
        .unwrap_or(0);
    let columns = ((width.saturating_sub(indent) + GAP) / (cell_width + GAP)).max(1);
    let mut lines = Vec::new();
    if columns == 1 {
        let tail_width = width.saturating_sub(indent + head_width).max(1);
        for (head, tail) in cells {
            for (i, row) in wrap_spans(&tail, tail_width, tail_width)
                .into_iter()
                .enumerate()
            {
                let mut spans = vec![Span::raw(" ".repeat(indent))];
                match i {
                    0 => spans.extend(head.clone()),
                    _ => spans.push(Span::raw(" ".repeat(head_width))),
                }
                spans.extend(row);
                lines.push(Line::from(spans));
            }
        }
        return lines;
    }
    for row in cells.chunks(columns) {
        let mut spans = vec![Span::raw(" ".repeat(indent))];
        for (i, (head, tail)) in row.iter().enumerate() {
            spans.extend(head.clone());
            spans.extend(tail.clone());
            if i + 1 < row.len() {
                let used = head_width + spans_width(tail);
                spans.push(Span::raw(" ".repeat(cell_width - used + GAP)));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// A bullet, wrapped with a hanging indent under its first word.
fn rule_lines(text: &'static str, width: usize) -> Vec<Line<'static>> {
    let spans = marked(text, ink(INK), ink(INK_BRIGHT).add_modifier(Modifier::BOLD));
    indented(&spans, Span::styled("  · ", glow(Neon::Amber)), "    ", width)
}

/// Wrap `spans` under a first-row prefix and a continuation indent of the
/// same width (a bullet, then the blank under it).
fn indented(
    spans: &[Span<'static>],
    first: Span<'static>,
    rest: &'static str,
    width: usize,
) -> Vec<Line<'static>> {
    let body = width.saturating_sub(rest.len()).max(1);
    wrap_spans(spans, body, body)
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let prefix = match i {
                0 => first.clone(),
                _ => Span::raw(rest),
            };
            let mut line = vec![prefix];
            line.extend(row);
            Line::from(line)
        })
        .collect()
}

/// A key as a chip: the key bold in amber on a warm ground, a space of
/// ground either side.
fn chip(key: &'static str) -> Vec<Span<'static>> {
    let ground = Style::default().bg(CHIP_BG);
    vec![
        Span::styled(" ", ground),
        Span::styled(key, lit(Neon::Amber).bg(CHIP_BG)),
        Span::styled(" ", ground),
    ]
}

/// The copy's two marks read into spans: `` `key` `` as a chip (inline,
/// so no ground padding: a wrap must not split a chip at its own space),
/// `*strong*` in `strong`, the rest in `plain`.
fn marked(text: &'static str, plain: Style, strong: Style) -> Vec<Span<'static>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Plain,
        Key,
        Strong,
    }
    let style = |mark: Mark| match mark {
        Mark::Plain => plain,
        Mark::Key => lit(Neon::Amber).bg(CHIP_BG),
        Mark::Strong => strong,
    };
    let mut spans = Vec::new();
    let mut mark = Mark::Plain;
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        let next = match (ch, mark) {
            ('`', Mark::Plain) => Mark::Key,
            ('`', Mark::Key) => Mark::Plain,
            ('*', Mark::Plain) => Mark::Strong,
            ('*', Mark::Strong) => Mark::Plain,
            _ => continue,
        };
        if i > start {
            spans.push(Span::styled(&text[start..i], style(mark)));
        }
        mark = next;
        start = i + ch.len_utf8();
    }
    if start < text.len() {
        spans.push(Span::styled(&text[start..], style(mark)));
    }
    spans
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
