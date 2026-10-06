//! The wasteland map and the ascent. Rendering only.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::common::theme;

use super::model::{Expedition, Game};
use super::space::{self, Space};
use super::state::State;
use super::world_data::Tile;

/// The masked viewport, centred on the wanderer.
pub fn draw_world(frame: &mut Frame, area: Rect, state: &State) {
    let (Some(game), Some(trip)) = (
        state.game(),
        state.game().and_then(|game| game.expedition.as_ref()),
    ) else {
        return;
    };
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(area);
    frame.render_widget(Paragraph::new(status_line(game, trip)), rows[0]);
    frame.render_widget(Paragraph::new(map_lines(trip, rows[1])), rows[1]);
}

fn status_line(game: &Game, trip: &Expedition) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("hp {}/{}  ", trip.hp, game.max_health()),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            format!("water {}/{}  ", trip.water, game.max_water()),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            format!("meat {}  ", trip.carrying(super::data::Resource::CuredMeat)),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            format!("{} from home", trip.distance()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

/// The map, cropped to what fits and to what has been seen.
fn map_lines(trip: &Expedition, area: Rect) -> Vec<Line<'static>> {
    let width = area.width as i32;
    let height = area.height as i32;
    let left = trip.x - width / 2;
    let top = trip.y - height / 2;
    let mut lines = Vec::new();
    for row in 0..height {
        let y = top + row;
        let mut spans = Vec::new();
        for column in 0..width {
            let x = left + column;
            if x == trip.x && y == trip.y {
                spans.push(Span::styled(
                    "@".to_string(),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ));
                continue;
            }
            if !trip.map.seen(x, y) {
                spans.push(Span::raw(" "));
                continue;
            }
            let tile = trip.map.tile(x, y);
            let style = match tile {
                Tile::Village => Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
                Tile::Forest | Tile::Field | Tile::Barrens => {
                    Style::default().fg(theme::TEXT_FAINT())
                }
                Tile::Road => Style::default().fg(theme::TEXT_DIM()),
                _ if trip.map.visited(x, y) => Style::default().fg(theme::TEXT_DIM()),
                _ => Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            };
            spans.push(Span::styled(tile.glyph().to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// The ascent: the ship, the rocks, the hull and how far up it is.
pub fn draw_space(frame: &mut Frame, area: Rect, flight: &Space) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}  ", flight.layer()),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}km  ", flight.altitude),
                Style::default().fg(theme::TEXT()),
            ),
            Span::styled(
                format!("hull {}/{}", flight.hull, flight.max_hull),
                Style::default().fg(theme::TEXT()),
            ),
        ])),
        rows[0],
    );

    let board = rows[1];
    let width = (space::WIDTH as u16).min(board.width) as usize;
    let height = (space::HEIGHT as u16).min(board.height) as usize;
    let mut grid = vec![vec![' '; width]; height];
    for asteroid in &flight.asteroids {
        let (x, y) = (asteroid.x.round() as isize, asteroid.y.round() as isize);
        if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
            grid[y as usize][x as usize] = asteroid.glyph;
        }
    }
    let (sx, sy) = (
        flight.ship_x.round() as isize,
        flight.ship_y.round() as isize,
    );
    if sx >= 0 && sy >= 0 && (sx as usize) < width && (sy as usize) < height {
        grid[sy as usize][sx as usize] = '@';
    }
    let lines: Vec<Line<'static>> = grid
        .into_iter()
        .map(|row| {
            Line::from(Span::styled(
                row.into_iter().collect::<String>(),
                Style::default().fg(theme::TEXT()),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), board);
}
