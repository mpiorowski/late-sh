//! The card that goes up when a realm ends.
//!
//! A conquest is weeks of play, and it used to end with the header wording
//! changing and the action keys quietly doing nothing. This says what
//! happened, in order: who took it, how, what everyone finished with, and
//! what the pot paid. It shows itself once when you open a finished game and
//! can be called back with `r`.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;

use super::map_ui::compact;
use super::resolver::RealmPlayerStatus;
use super::state::{RealmState, player_color};
use super::svc::{FinalPlace, final_table};

/// Rendered over the board, so the map stays visible around the edges — the
/// world you were just fighting over is the right backdrop for the result.
pub fn draw_results(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let width = CARD_WIDTH.min(area.width);
    let Some(lines) = results_lines(realm, width.saturating_sub(2) as usize) else {
        return;
    };
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = centered(width, height, area);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(" the realm is settled ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// How wide the card draws.
pub const CARD_WIDTH: u16 = 62;

/// The card's contents, written to `text_width`. Separate from the draw so
/// the widths can be checked without a terminal — a results screen that
/// clips the chips column would be worse than no results screen.
pub fn results_lines(realm: &RealmState, text_width: usize) -> Option<Vec<Line<'static>>> {
    let board = realm.board.as_ref()?;
    let detail = board.detail.as_ref()?;
    let state = &detail.state;
    let table = final_table(state, board.game_id, detail.winner_user_id);

    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let bright = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'static>> = vec![Line::raw("")];

    // The headline: who won, and by what.
    match detail.winner_user_id {
        Some(winner) => {
            let name = state
                .player(winner)
                .map(|p| p.username.clone())
                .unwrap_or_else(|| "someone".into());
            let held = state.territory_count(winner);
            let whole_map = detail
                .map()
                .is_some_and(|m| held as usize == m.territories.len());
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    name,
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    // Two ways to have won, and the card says which: the
                    // whole map, or the last one standing with unclaimed
                    // ground still lying about.
                    if whole_map {
                        " holds the whole world"
                    } else {
                        " is the last one standing"
                    },
                    bright,
                ),
            ]));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "  the realm dissolved — nobody was left to hold it",
                bright,
            )));
        }
    }
    lines.push(Line::from(Span::styled(
        format!(
            "  {} · {} days · {} of {} territories taken",
            detail.name,
            (detail.last_resolved_day - state.start_day).max(0),
            state.ownership.len(),
            detail.map().map(|m| m.territories.len()).unwrap_or(0),
        ),
        dim,
    )));
    lines.push(Line::raw(""));

    // The standings, in the order the pot was paid.
    lines.push(Line::from(Span::styled(
        format!(
            "  {:<3}{:<15}{:>6}{:>7}{:>9}  {}",
            "", "player", "land", "days", "chips", "ending"
        ),
        faint,
    )));
    for place in &table {
        lines.push(place_line(place, realm.user_id, state, text_width));
    }

    lines.push(Line::raw(""));
    let paid: i64 = table.iter().map(|p| p.chips).sum();
    lines.push(Line::from(Span::styled(
        if paid > 0 {
            format!("  pot paid: {} chips", compact(paid as u64))
        } else {
            "  no pot: a realm has to be played to pay".to_string()
        },
        Style::default().fg(theme::AMBER_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  esc close · r reopens it · the board stays open",
        faint,
    )));

    Some(lines)
}

fn place_line(
    place: &FinalPlace,
    viewer: uuid::Uuid,
    state: &super::resolver::RealmGameState,
    text_width: usize,
) -> Line<'static> {
    let (r, g, b) = player_color(state, place.user_id);
    let you = place.user_id == viewer;
    let medal = match place.place {
        Some(1) => "1st",
        Some(2) => "2nd",
        Some(3) => "3rd",
        _ => "  ",
    };
    let name_style = if you {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let ending = if !place.note.is_empty() {
        place.note.to_string()
    } else {
        match place.status {
            RealmPlayerStatus::Alive => "stood to the end".to_string(),
            RealmPlayerStatus::Eliminated => "conquered".to_string(),
            RealmPlayerStatus::Left => "withdrew".to_string(),
            RealmPlayerStatus::Kicked => "went quiet".to_string(),
        }
    };
    let chips = if place.chips > 0 {
        format!("{:>9}", compact(place.chips as u64))
    } else {
        format!("{:>9}", "—")
    };
    // The columns up to here are fixed; the ending takes whatever is left.
    // Measured rather than counted, because a glyph's width is not something
    // to be confident about from a source file.
    let mut spans = vec![
        Span::styled(format!("  {medal:<3}"), Style::default().fg(theme::AMBER())),
        Span::styled("■ ", Style::default().fg(Color::Rgb(r, g, b))),
        Span::styled(
            format!(
                "{:<14}",
                truncate(
                    &format!("{}{}", place.username, if you { " (you)" } else { "" }),
                    13
                )
            ),
            name_style,
        ),
        Span::styled(
            format!("{:>6}{:>7}", place.territories, place.active_days),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            chips,
            if place.chips > 0 {
                Style::default().fg(theme::SUCCESS())
            } else {
                Style::default().fg(theme::TEXT_FAINT())
            },
        ),
    ];
    let used: usize = spans
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let room = text_width.saturating_sub(used + 2);
    spans.push(Span::styled(
        format!("  {}", truncate(&ending, room)),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    Line::from(spans)
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::lobby::realm::resolver::{RealmGameState, RealmPlayer};
    use crate::app::lobby::realm::rulesets::STANDARD;
    use std::collections::BTreeMap;
    use unicode_width::UnicodeWidthStr;
    use uuid::Uuid;

    fn state_with(players: &[(u128, &str, RealmPlayerStatus, u16)]) -> RealmGameState {
        RealmGameState {
            version: crate::app::lobby::realm::resolver::STATE_VERSION,
            revision: 2,
            ruleset: (&STANDARD).into(),
            players: players
                .iter()
                .map(|(n, name, status, held)| RealmPlayer {
                    user_id: Uuid::from_u128(*n),
                    username: name.to_string(),
                    status: *status,
                    joined_day: 20_000,
                    exit_day: (*status != RealmPlayerStatus::Alive).then_some(20_010),
                    exit_territories: *held,
                    actions_day: 20_010,
                    actions_used: 0,
                    actions_allowance: 0,
                    last_action_day: Some(20_010),
                    active_days: 12,
                    color: None,
                    joined_claimed: 0,
                })
                .collect(),
            ownership: BTreeMap::new(),
            forts: BTreeMap::new(),
            last_day: None,
            start_player_count: players.len() as u8,
            start_day: 20_000,
        }
    }

    /// The card is the last thing a player sees after weeks of play; a
    /// clipped chips column there would be the worst place for one.
    #[test]
    fn the_results_card_fits_its_own_box() {
        let text_width = (CARD_WIDTH - 2) as usize;
        let state = state_with(&[
            (
                1,
                "a-very-long-username-here",
                RealmPlayerStatus::Alive,
                180,
            ),
            (2, "another-long-one", RealmPlayerStatus::Eliminated, 44),
            (3, "kicked-player-name", RealmPlayerStatus::Kicked, 9),
            (4, "left-early", RealmPlayerStatus::Left, 2),
        ]);
        let table = final_table(&state, Uuid::from_u128(7), Some(Uuid::from_u128(1)));
        for place in &table {
            let line = place_line(place, Uuid::from_u128(1), &state, text_width);
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(
                text.width() <= text_width,
                "{text:?} is {} wide, past {text_width}",
                text.width()
            );
        }
    }

    #[test]
    fn everyone_is_on_the_card_whether_they_were_paid_or_not() {
        let state = state_with(&[
            (1, "winner", RealmPlayerStatus::Alive, 180),
            (2, "fought", RealmPlayerStatus::Eliminated, 44),
            (3, "quit", RealmPlayerStatus::Left, 2),
        ]);
        let table = final_table(&state, Uuid::from_u128(7), Some(Uuid::from_u128(1)));
        assert_eq!(table.len(), 3, "a result nobody is missing from");
        // The winner leads and is paid; the one who walked out is listed and
        // is not, with the reason on the row rather than left to inference.
        assert_eq!(table[0].username, "winner");
        assert!(table[0].chips > 0);
        let quit = table.iter().find(|p| p.username == "quit").expect("quit");
        assert_eq!(quit.chips, 0);
        assert_eq!(quit.note, "withdrew");
    }
}
