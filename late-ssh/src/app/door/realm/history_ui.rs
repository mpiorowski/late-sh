//! The realm as it was: a day picked with the arrows, the map painted from
//! that day's closing board, and what happened on it beside.
//!
//! The map is the same `worldmap` painter the live board uses; what differs is
//! where the colours come from. A replayed day is coloured from its own
//! snapshot's roster (`DayBoard::players`), not the live one — a player who
//! withdrew is gone from `RealmGameState::players` entirely, and asking the
//! living game who owned a territory three weeks ago would get the wrong
//! answer or none.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::common::theme;
use crate::app::common::worldmap::{self, view::Viewport};

use super::log_ui::entry_line;
use super::map::{TerritoryId, WorldMap};
use super::map_ui::RAIL_WIDTH;
use super::resolver::{DayBoard, RealmGameState};
use super::state::{RealmState, palette_shades, shade_index};
use super::svc::ArchivedDay;

/// Everything a history frame draws from, gathered before drawing so the
/// drawing itself does not need a live session — which is also what makes it
/// testable without a database behind it.
pub(super) struct HistoryFrame<'a> {
    pub map: &'a WorldMap,
    /// Days with a board, oldest first.
    pub days: &'a [&'a ArchivedDay],
    /// The run is still being read in, so what is on screen is the oldest
    /// day *so far* and will be replaced by an older one.
    pub loading: bool,
    /// Which of them is on screen.
    pub index: usize,
    /// The live game, for naming people in the day's log lines. A player who
    /// has since withdrawn is not in it; the snapshot's own roster is what
    /// covers them for everything else on screen.
    pub live: &'a RealmGameState,
    pub view: Viewport,
}

pub fn draw(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let Some(map) = detail.map() else {
        return;
    };

    let days = realm.history_days();
    let Some((index, _)) = realm.history_position() else {
        draw_nothing_yet(frame, area);
        return;
    };

    let canvas = canvas_of(area);
    // Always the whole world, always the same frame. Walking the days is a
    // comparison — this day against the one before it — and a map that had
    // been panned or zoomed between two frames would be comparing two
    // different pictures.
    let view = Viewport::fitted(&map, canvas.width as i32, canvas.height as i32 * 2);

    render(
        frame,
        area,
        &HistoryFrame {
            map: &map,
            days: &days,
            index,
            live: &detail.state,
            loading: realm.history_loading(),
            view,
        },
    );
}

/// The map area, minus the row the slider lives on at the top.
fn canvas_of(area: Rect) -> Rect {
    let map_area = split(area).0;
    Rect::new(
        map_area.x,
        map_area.y + 1,
        map_area.width,
        map_area.height.saturating_sub(1),
    )
}

fn split(area: Rect) -> (Rect, Option<Rect>) {
    if area.width > RAIL_WIDTH + 40 {
        (
            Rect::new(area.x, area.y, area.width - RAIL_WIDTH - 1, area.height),
            Some(Rect::new(
                area.x + area.width - RAIL_WIDTH,
                area.y,
                RAIL_WIDTH,
                area.height,
            )),
        )
    } else {
        (area, None)
    }
}

pub(super) fn render(frame: &mut Frame, area: Rect, ctx: &HistoryFrame) {
    let Some(archived) = ctx.days.get(ctx.index) else {
        draw_nothing_yet(frame, area);
        return;
    };
    let Some(day_board) = archived.board.as_ref() else {
        draw_nothing_yet(frame, area);
        return;
    };
    let (map_area, rail_area) = split(area);
    // The slider sits on top, above the map rather than under it. This view
    // and the live board draw the same world in the same colours, and the one
    // thing that says which you are looking at should not be the last line on
    // the screen.
    let slider_area = Rect::new(map_area.x, map_area.y, map_area.width, 1);

    worldmap::view::paint(
        frame,
        canvas_of(area),
        ctx.map,
        &ctx.view,
        &worldmap::view::Paint {
            colors: &colors_of(day_board, ctx.live.ruleset.fort_max_level),
            selected: None,
            crosshair: false,
        },
    );
    draw_slider(
        frame,
        slider_area,
        ctx.index,
        ctx.days.len(),
        archived.day(),
        ctx.loading,
    );

    if let Some(rail) = rail_area {
        draw_rail(frame, rail, archived, day_board, ctx.live, ctx.map);
    }
}

/// A day's territory colours, from that day's own roster. The live game is
/// not asked: a player who withdrew is gone from it, and their land would
/// come back either uncoloured or — worse — in somebody else's colour.
pub(super) fn colors_of(
    board: &DayBoard,
    fort_max: u8,
) -> std::collections::BTreeMap<TerritoryId, (u8, u8, u8)> {
    board
        .owned
        .iter()
        .filter_map(|(territory, slot)| {
            let player = board.players.get(*slot as usize)?;
            let shades = palette_shades(player.color.unwrap_or(*slot));
            Some((
                *territory,
                shades[shade_index(board.fort_level(*territory), fort_max)],
            ))
        })
        .collect()
}

/// The strip that says this is the past: what it is, which day, and where
/// that day falls in the run.
///
/// It leads with the word rather than the number. The history and the live
/// board draw the same world in the same colours, and somebody who tabs into
/// this view has to be able to tell at a glance that the map in front of them
/// is three weeks old.
fn draw_slider(frame: &mut Frame, area: Rect, index: usize, total: usize, day: i32, loading: bool) {
    // While the run is still coming in, the oldest day known is not the first
    // day of the game — say so, rather than letting a map that is about to be
    // replaced pass for the beginning. This is what "day 1" meant when it was
    // showing day 32 of 45.
    let label = if loading {
        format!(" history · reading the run in… ({total} days so far) ")
    } else {
        format!(
            " history · {} · day {} of {total} ",
            day_label(day),
            index + 1
        )
    };
    // The track is drawn between its keys. A slider whose ends are labelled
    // `[` and `]` says how to move it without spending a line on saying so.
    let track_width = area.width.saturating_sub(label.chars().count() as u16 + 4) as usize;
    let mut track = String::new();
    if track_width > 0 && total > 0 {
        // Where this day falls along the run, in cells.
        let at = if total == 1 {
            0
        } else {
            index * (track_width - 1) / (total - 1)
        };
        for cell in 0..track_width {
            track.push(if cell == at { '▮' } else { '─' });
        }
    }
    let key = Style::default().fg(theme::AMBER());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                label,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("[", key),
            Span::styled(track, Style::default().fg(theme::TEXT_DIM())),
            Span::styled("]", key),
        ])),
        area,
    );
}

/// A realm day as a date. The number is days since the epoch, which is the
/// right thing to compute with and no use at all to read.
fn day_label(day: i32) -> String {
    chrono::DateTime::from_timestamp(i64::from(day) * 86_400, 0)
        .map(|at| at.format("%-d %b").to_string())
        .unwrap_or_else(|| format!("day {day}"))
}

fn draw_nothing_yet(frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let lines = vec![
        Line::from(Span::styled(" nothing to walk through yet", dim)),
        Line::raw(""),
        Line::from(Span::styled(
            " a day appears here once it has closed —",
            dim,
        )),
        Line::from(Span::styled(
            " the board is kept as each day ends, just",
            dim,
        )),
        Line::from(Span::styled(" before everyone's points come back.", dim)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Who stood where on the day being shown, and what happened on it.
fn draw_rail(
    frame: &mut Frame,
    area: Rect,
    archived: &ArchivedDay,
    day_board: &DayBoard,
    live: &RealmGameState,
    map: &WorldMap,
) {
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        format!(" as {} ended", day_label(archived.day())),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(""));

    let total_land = map.territories.len().max(1);
    for (player, held) in day_board.standings() {
        let shades = palette_shades(player.color.unwrap_or(0));
        let (r, g, b) = shades[shades.len() / 2];
        let share = held as f64 / total_land as f64 * 100.0;
        lines.push(Line::from(vec![
            Span::styled(
                " █ ",
                Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
            ),
            Span::styled(format!("{:<14}", short(&player.username, 14)), text),
            Span::styled(format!("{held:>3}"), text),
            Span::styled(format!(" {share:>4.1}%"), faint),
        ]));
    }
    if day_board.players.is_empty() {
        lines.push(Line::from(Span::styled(" nobody was playing", dim)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(" what happened", faint)));
    if archived.result.entries.is_empty() {
        lines.push(Line::from(Span::styled(" a quiet day", dim)));
    }
    // The same renderer the log view uses, so a day reads the same in both
    // places.
    for entry in &archived.result.entries {
        lines.push(entry_line(entry, live, map));
    }

    frame.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

fn short(name: &str, width: usize) -> String {
    if name.chars().count() <= width {
        return name.to_string();
    }
    let kept: String = name.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
#[path = "history_ui_test.rs"]
mod history_ui_test;
