//! Drawing the map shelf: a title line, the world on the left, who is on it
//! down the right. The footer is the host page's row, its keys from [`hints`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::common::theme;
use crate::app::common::worldmap::view::{self, Paint};

use super::state::{CountryTally, HEAT, ListLine, MapMode, UserMapState, heat_label_for};

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
    if area.width == 0 || area.height < 2 {
        return;
    }
    let title = match (state.online(), state.total()) {
        // The online slice says how many of the connected it shows: the rest
        // never set a country, and without the "of" the gap to every other
        // online count reads as a bug.
        (Some(online), total) if online > 0 => format!(
            " where everyone is · {} · {total} of {online} {} set a country",
            state.mode().title(),
            match total {
                1 => "has",
                _ => "have",
            }
        ),
        (_, 0) => format!(" where everyone is · {}", state.mode().title()),
        (_, total) => format!(
            " where everyone is · {} · {}",
            state.mode().title(),
            state.mode().noun(total)
        ),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
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
}

/// The keys for the host's footer row. The three slices are not here: the
/// host draws them as filters on its section row, where they can be clicked.
pub fn hints(state: &UserMapState) -> &'static [(&'static str, &'static str)] {
    // When the whole world is on screen the arrows are correctly a no-op, and
    // a footer that still advertises them is how "panning is broken" gets
    // reported. Say what is actually true instead.
    let fits = match (state.map(), state.view()) {
        (Some(map), Some(view)) => {
            !view.can_pan_vertically(&map) && !view.can_pan_horizontally(&map)
        }
        _ => false,
    };
    if fits {
        &[
            ("m", "online/month/all"),
            ("+", "zoom in"),
            ("j/k", "country"),
            ("r", "refresh"),
        ]
    } else {
        &[
            ("m", "online/month/all"),
            ("arrows", "pan"),
            ("+/-", "zoom"),
            ("f", "fit"),
            ("j/k", "country"),
            ("r", "refresh"),
        ]
    }
}

/// A country's row: the cursor, its shade, its name and how many are there.
fn country_row(tally: &CountryTally, selected: bool, state: &UserMapState) -> Line<'static> {
    let (r, g, b) = HEAT[state.heat_index(tally.count)];
    let name = ellipsize(&tally.name, 18);
    Line::from(vec![
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
    ])
}

/// Cut to `max` characters, the last of them an ellipsis.
fn ellipsize(text: &str, max: usize) -> String {
    match text.chars().count() > max {
        true => {
            let short: String = text.chars().take(max - 1).collect();
            format!("{short}…")
        }
        false => text.to_string(),
    }
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
    let start = state.first_visible_line();
    let floors = state.floors();
    for line in state.list_lines().into_iter().skip(start).take(rows) {
        let tally = &tallies[line.tally()];
        lines.push(match line {
            ListLine::Country(index) => country_row(tally, index == state.cursor(), state),
            // A country the map has no shape for still counts, and says so,
            // so the numbers beside the map add up to the numbers on it.
            ListLine::NotOnMap(_) => Line::from(Span::styled(
                "     (not on the map)",
                Style::default().fg(theme::TEXT_FAINT()),
            )),
            ListLine::Person(_, person) => Line::from(Span::styled(
                format!(
                    "     {}",
                    ellipsize(&format!("@{}", tally.people[person]), 24)
                ),
                Style::default().fg(theme::TEXT_DIM()),
            )),
        });
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
