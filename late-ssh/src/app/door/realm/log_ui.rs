//! The realm log view: the latest resolved day (from the game state) plus
//! archived days paged in from `realm_days`, newest first.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::common::theme;

use super::map::WorldMap;
use super::resolver::{DayResult, FizzleReason, LogEntry, RealmGameState, TakerRoll, WinKind};
use super::state::RealmState;

pub fn draw_log_view(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let Some(map) = detail.map() else {
        return;
    };
    let state = &detail.state;

    let mut lines: Vec<Line> = Vec::new();
    let mut seen_days: Vec<&DayResult> = Vec::new();
    if let Some(last) = state.last_day.as_ref() {
        seen_days.push(last);
    }
    for day in board.logs.iter().map(|archived| &archived.result) {
        // The archive also holds the last day; skip the duplicate.
        if state.last_day.as_ref().map(|d| d.day) != Some(day.day) {
            seen_days.push(day);
        }
    }

    if seen_days.is_empty() {
        lines.push(Line::from(Span::styled(
            "nothing has happened yet — take a territory and it lands here",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for day in &seen_days {
        lines.push(Line::from(Span::styled(
            format!("── day {} ──", day.day),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        if day.entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "  a quiet day; nobody moved",
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
        for entry in &day.entries {
            lines.extend(entry_lines(entry, state, &map));
        }
        lines.push(Line::from(""));
    }
    if !board.logs_exhausted {
        lines.push(Line::from(Span::styled(
            "  older days load as you scroll",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    let scroll = board.log_scroll.min(lines.len().saturating_sub(1));
    // Log lines name two players and a territory, so they outrun a narrow
    // pane regularly; wrapping keeps the tail rather than clipping it.
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, area);
}

fn name_of(state: &RealmGameState, user_id: Uuid) -> String {
    state
        .player(user_id)
        .map(|p| p.username.clone())
        .unwrap_or_else(|| "someone".into())
}

fn territory_name(map: &WorldMap, id: u16) -> String {
    map.territory(id)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| format!("#{id}"))
}

/// How the strike reached its target, for the log line. A plain border move
/// says nothing; anything further says how far it had to go.
fn reach_note(hops: u32, by_sea: bool) -> String {
    match (hops, by_sea) {
        (0..=1, false) => String::new(),
        (_, true) => " from over the water".to_string(),
        (h, false) => format!(" from {h} away"),
    }
}

/// The lines for one log entry: one, except a night's fight, which has a
/// line per taker. For the map rail's "latest" strip as well as the log view.
pub fn entry_lines(entry: &LogEntry, state: &RealmGameState, map: &WorldMap) -> Vec<Line<'static>> {
    match entry {
        LogEntry::Fought {
            target,
            holder,
            taken_by,
            takers,
            bounced,
            ..
        } => takers
            .iter()
            .map(|roll| fought_line(state, map, *target, *holder, *taken_by, *bounced, roll))
            .collect(),
        _ => vec![entry_line(entry, state, map)],
    }
}

/// "spoon → Micronesia · 3 pts · 42% · taken from mat": one taker's roll at
/// a night.
pub(super) fn fought_line(
    state: &RealmGameState,
    map: &WorldMap,
    target: u16,
    holder: Option<Uuid>,
    taken_by: Option<Uuid>,
    bounced: bool,
    roll: &TakerRoll,
) -> Line<'static> {
    let (verdict, style) = match (taken_by == Some(roll.user_id), roll.success, bounced) {
        (true, _, _) => (
            match holder {
                Some(holder) => format!("taken from {}", name_of(state, holder)),
                None => "taken".to_string(),
            },
            Style::default().fg(theme::SUCCESS()),
        ),
        (false, true, true) => ("bounced".to_string(), Style::default().fg(theme::AMBER())),
        (false, true, false) => ("outfought".to_string(), Style::default().fg(theme::ERROR())),
        (false, false, _) => ("held".to_string(), Style::default().fg(theme::ERROR())),
    };
    let points = match roll.points {
        1 => "1 pt".to_string(),
        n => format!("{n} pts"),
    };
    Line::from(vec![
        Span::styled(
            format!("  {} ", name_of(state, roll.user_id)),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            format!(
                "\u{2192} {} \u{b7} {points} \u{b7} {:.0}% \u{b7} ",
                territory_name(map, target),
                roll.probability * 100.0
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(verdict, style),
    ])
}

fn entry_line(entry: &LogEntry, state: &RealmGameState, map: &WorldMap) -> Line<'static> {
    let ok = Style::default().fg(theme::SUCCESS());
    let bad = Style::default().fg(theme::ERROR());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    match entry {
        LogEntry::Claimed {
            user_id,
            target,
            probability,
            success,
            hops,
            by_sea,
        } => {
            let jump_note = reach_note(*hops, *by_sea);
            let (verdict, style) = if *success {
                ("claimed", ok)
            } else {
                ("failed to claim", bad)
            };
            Line::from(vec![
                Span::styled(format!("  {} ", name_of(state, *user_id)), text),
                Span::styled(
                    format!(
                        "{verdict} {}{jump_note} ({:.0}%)",
                        territory_name(map, *target),
                        probability * 100.0
                    ),
                    style,
                ),
            ])
        }
        LogEntry::Attacked {
            user_id,
            defender_id,
            target,
            probability,
            success,
            hops,
            by_sea,
        } => {
            let jump_note = reach_note(*hops, *by_sea);
            let (verdict, style) = if *success {
                ("took", ok)
            } else {
                ("failed to take", bad)
            };
            Line::from(vec![
                Span::styled(format!("  {} ", name_of(state, *user_id)), text),
                Span::styled(
                    format!(
                        "{verdict} {} from {}{jump_note} ({:.0}%)",
                        territory_name(map, *target),
                        name_of(state, *defender_id),
                        probability * 100.0
                    ),
                    style,
                ),
            ])
        }
        LogEntry::Fizzled {
            user_id,
            action,
            reason,
        } => {
            let why = match reason {
                FizzleReason::TargetOwned => "someone got there first",
                FizzleReason::AlreadyYours => "it was already theirs",
                FizzleReason::TargetFree => "the defender was already gone",
                FizzleReason::NotAdjacent => "no route to it",
                FizzleReason::PlayerEliminated => "they had already fallen",
                FizzleReason::UnknownTerritory => "no such place",
                FizzleReason::TargetProtected => "it was under protection by nightfall",
                FizzleReason::NoFront => "nothing of theirs bordered it by nightfall",
            };
            Line::from(vec![
                Span::styled(format!("  {} ", name_of(state, *user_id)), text),
                Span::styled(
                    format!(
                        "wasted a move on {} — {why}",
                        territory_name(map, action.target())
                    ),
                    dim,
                ),
            ])
        }
        LogEntry::Fortified {
            user_id,
            target,
            level,
        } => Line::from(vec![
            Span::styled(format!("  {} ", name_of(state, *user_id)), text),
            Span::styled(
                format!("dug in at {} (level {level})", territory_name(map, *target)),
                dim,
            ),
        ]),
        LogEntry::Spawned { user_id, target } => Line::from(vec![
            Span::styled(format!("  {} ", name_of(state, *user_id)), text),
            Span::styled(format!("arrived in {}", territory_name(map, *target)), dim),
        ]),
        LogEntry::Left { user_id, freed } => Line::from(Span::styled(
            format!(
                "  {} withdrew ({} territories freed)",
                name_of(state, *user_id),
                freed.len()
            ),
            dim,
        )),
        LogEntry::Kicked {
            user_id,
            freed_territories,
            ..
        } => Line::from(Span::styled(
            format!(
                "  {} went silent and was removed ({} territories freed)",
                name_of(state, *user_id),
                freed_territories
            ),
            dim,
        )),
        LogEntry::Eliminated {
            user_id,
            by_user_id,
        } => Line::from(Span::styled(
            format!(
                "  {} was wiped off the map by {}",
                name_of(state, *user_id),
                name_of(state, *by_user_id)
            ),
            bad,
        )),
        LogEntry::Won { user_id, kind } => Line::from(Span::styled(
            match kind {
                WinKind::LastStanding => {
                    format!("  ★ {} rules the realm", name_of(state, *user_id))
                }
                WinKind::Dominated | WinKind::SeasonOver => {
                    format!("  ★ {} {}", name_of(state, *user_id), kind.phrase())
                }
            },
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        LogEntry::Fought { .. } => unreachable!("entry_lines draws a fight, a line per taker"),
    }
}
