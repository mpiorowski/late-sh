//! Drawing `/map`: the world on the left, who is on it down the right.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::common::theme;
use crate::app::common::worldmap::view::{self, Paint};

use super::state::{HEAT, MapMode, UserMapState, heat_label_for};

/// The modal, centred in the area it is given.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

/// How many country rows fit in the list column, once the legend and its
/// heading have taken their share. Shared with the click handling, which has
/// to agree with the draw about which row is where.
fn list_rows(area: Rect) -> usize {
    area.height.saturating_sub(3) as usize
}

/// Width of the country list. Enough for a long country name, a count, and
/// the gap between them.
const LIST_WIDTH: u16 = 30;

pub fn draw(frame: &mut Frame, area: Rect, state: &mut UserMapState) {
    // Nearly the whole screen: a world map in a small box is a smudge.
    let width = area.width.saturating_sub(4).max(20);
    let height = area.height.saturating_sub(2).max(10);
    let rect = centered_rect(width, height, area);
    frame.render_widget(Clear, rect);

    let total = state.total();
    let title = if total == 0 {
        format!(" where everyone is · {} ", state.mode().title())
    } else {
        format!(
            " where everyone is · {} · {} ",
            state.mode().title(),
            state.mode().noun(total)
        )
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height < 2 {
        return;
    }

    let footer = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    let (map_area, list_area) = if body.width > LIST_WIDTH + 30 {
        (
            Rect::new(body.x, body.y, body.width - LIST_WIDTH - 1, body.height),
            Some(Rect::new(
                body.x + body.width - LIST_WIDTH,
                body.y,
                LIST_WIDTH,
                body.height,
            )),
        )
    } else {
        (body, None)
    };

    if let Some(map) = state.map() {
        let px_w = map_area.width as i32;
        let px_h = map_area.height as i32 * 2;
        let viewport = state.sync_view(px_w, px_h);
        view::paint(
            frame,
            map_area,
            &map,
            &viewport,
            &Paint {
                colors: &state.colors(),
                selected: state.selected(),
                // Nothing is being aimed at here: the cursor is the country
                // list, and the map answers to it.
                crosshair: false,
            },
        );
    }

    // A click has to be able to work out what it hit, and only the draw
    // knows where anything ended up.
    let rows = list_area.map(list_rows).unwrap_or(0);
    state.record_geometry(map_area, list_area, rows);
    if let Some(list_area) = list_area {
        draw_list(frame, list_area, state);
    }

    // When the whole world is on screen the arrows are correctly a no-op, and
    // a footer that still advertises them is how "panning is broken" gets
    // reported. Say what is actually true instead.
    let fits = match (state.map(), state.view()) {
        (Some(map), Some(view)) => {
            !view.can_pan_vertically(&map) && !view.can_pan_horizontally(&map)
        }
        _ => false,
    };
    let hints = if fits {
        "tab mode · + zoom in · j/k country · r refresh · esc close"
    } else {
        "tab mode · arrows pan · +/- zoom · j/k country · f fit · esc close"
    };
    // The tab strip goes on the same line as the hints: it is how anybody
    // finds out the other two slices exist, and a modal this size has no
    // room to spend a whole row saying so.
    let mut footer_spans: Vec<Span<'static>> = Vec::new();
    for mode in MapMode::ALL {
        let current = mode == state.mode();
        footer_spans.push(Span::styled(
            format!("{} ", mode.tab_label()),
            Style::default().fg(if current {
                theme::AMBER_GLOW()
            } else {
                theme::TEXT_FAINT()
            }),
        ));
    }
    footer_spans.push(Span::styled(
        format!("· {hints}"),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(Paragraph::new(Line::from(footer_spans)), footer);
}

fn draw_list(frame: &mut Frame, area: Rect, state: &UserMapState) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(error) = state.error() {
        lines.push(Line::from(Span::styled(
            format!(" {error}"),
            Style::default().fg(theme::ERROR()),
        )));
    } else if state.loading() && state.tallies().is_empty() {
        lines.push(Line::from(Span::styled(
            " counting…",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else if state.tallies().is_empty() {
        // Written to the list's width: this column is 30 columns and a line
        // that overflows it is silently cut, which is how a helpful message
        // becomes a puzzling one.
        let first = match state.mode() {
            MapMode::Online => "nobody online has said",
            MapMode::Recent => "nobody here this month",
            MapMode::Everyone => "nobody has said",
        };
        let second = match state.mode() {
            MapMode::Recent => "has said where they are.",
            _ => "where they are.",
        };
        for chunk in [
            first,
            second,
            "",
            "it is a field in /settings —",
            "this map only shows people",
            "who filled it in.",
        ] {
            lines.push(Line::from(Span::styled(
                format!(" {chunk}"),
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
    }

    // The list scrolls with the cursor, keeping it in view without moving
    // when it does not have to.
    let rows = list_rows(area);
    let tallies = state.tallies();
    let start = state.first_visible_row();
    let floors = state.floors();
    for (index, tally) in tallies.iter().enumerate().skip(start).take(rows) {
        let selected = index == state.cursor();
        let (r, g, b) = HEAT[state.heat_index(tally.count)];
        let name = if tally.name.chars().count() > 18 {
            let short: String = tally.name.chars().take(17).collect();
            format!("{short}…")
        } else {
            tally.name.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { " ▸" } else { "  " },
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                "█ ",
                Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
            ),
            Span::styled(
                format!("{name:<19}"),
                Style::default().fg(if selected {
                    theme::TEXT_BRIGHT()
                } else {
                    theme::TEXT()
                }),
            ),
            Span::styled(
                format!("{:>3}", tally.count),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        // A country the map has no shape for still counts, and says so, so
        // the numbers beside the map add up to the numbers on it.
        if tally.territory.is_none() {
            lines.push(Line::from(Span::styled(
                "     (not on the map)",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " people per country",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    // Only the rungs the data actually uses: with four people on the server a
    // five-step legend is four lies and a fact. Wrapped by width rather than
    // trusted to fit, because at four digits it does not and the clipped end
    // is the top rung — the one somebody is squinting at the map to find.
    let mut legend: Vec<Span<'static>> = vec![Span::raw(" ")];
    let mut used = 1usize;
    for (rung, _) in floors.iter().enumerate() {
        let (r, g, b) = HEAT[rung.min(HEAT.len() - 1)];
        let label = format!("{} ", heat_label_for(&floors, rung));
        let width = label.chars().count() + 1;
        if used + width > area.width as usize && legend.len() > 1 {
            lines.push(Line::from(std::mem::take(&mut legend)));
            legend.push(Span::raw(" "));
            used = 1;
        }
        legend.push(Span::styled(
            "█",
            Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
        ));
        legend.push(Span::styled(
            label,
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        used += width;
    }
    lines.push(Line::from(legend));

    frame.render_widget(Paragraph::new(lines), area);
}
