//! Right-sidebar Daily Games panel: passive, fixed height, stable chrome.
//! Slots render dashes when empty so the panel never changes shape between
//! states; all interaction lives in the modal (`g`).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

use super::state::DailyState;

/// Title + four match slots + lobby line + entries line + key hints.
pub(crate) const DAILY_PANEL_HEIGHT: u16 = 8;
const MATCH_SLOTS: usize = 4;

/// Inputs for the panel, bundled so the pure line builder is easy to drive
/// from tests.
pub(crate) struct DailyPanelProps {
    /// Sorted my-matches rows: your-turn first, then nearest deadline.
    pub matches: Vec<DailyPanelMatchRow>,
    pub open_count: usize,
    /// Newest open challenge's poster, for the lobby activity line.
    pub latest_challenger: Option<String>,
    pub lobby_glow: bool,
    pub entry_count: usize,
    pub entry_cap: usize,
}

pub(crate) struct DailyPanelMatchRow {
    pub opponent: String,
    pub my_turn: bool,
}

pub(crate) fn draw_daily_inline(frame: &mut Frame, area: Rect, state: &DailyState) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let matches = state
        .my_matches()
        .iter()
        .map(|item| DailyPanelMatchRow {
            opponent: state
                .opponent_of(item)
                .1
                .unwrap_or_else(|| "player".to_string()),
            my_turn: state.my_turn(item),
        })
        .collect();
    let lobby = state.lobby();
    let props = DailyPanelProps {
        matches,
        open_count: lobby.len(),
        latest_challenger: lobby
            .last()
            .and_then(|challenge| challenge.challenger_username.clone()),
        lobby_glow: state.lobby_glow(),
        entry_count: state.entry_count(),
        entry_cap: state.entry_cap(),
    };
    let lines = daily_panel_lines(area.width, &props);
    frame.render_widget(Paragraph::new(lines), area);
}

fn daily_panel_lines(width: u16, props: &DailyPanelProps) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(DAILY_PANEL_HEIGHT as usize);
    let any_my_turn = props.matches.iter().any(|row| row.my_turn);
    lines.push(title_line(width, any_my_turn || props.lobby_glow));

    for slot in 0..MATCH_SLOTS {
        match props.matches.get(slot) {
            Some(row) => lines.push(match_line(width, row)),
            None => lines.push(empty_slot_line()),
        }
    }

    lines.push(lobby_line(width, props));
    lines.push(entries_line(props.entry_count, props.entry_cap));
    lines.push(hints_line());
    lines
}

/// `▌ daily games ────`, amber when something wants attention.
fn title_line(width: u16, active: bool) -> Line<'static> {
    let (bar_style, label_style) = if active {
        (
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            Style::default().fg(theme::BORDER_DIM()),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )
    };
    let label = "daily games";
    let used = 2 + label.chars().count() + 2;
    let dash_count = (width as usize).saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled("▌ ", bar_style),
        Span::styled(label.to_string(), label_style),
        Span::raw("  "),
        Span::styled(
            "─".repeat(dash_count),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ])
}

/// `► mira        your turn` / `  c0ld          waiting`.
fn match_line(width: u16, row: &DailyPanelMatchRow) -> Line<'static> {
    let (marker, marker_style, name_style, status, status_style) = if row.my_turn {
        (
            "► ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
            "your turn",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            "  ",
            Style::default().fg(theme::TEXT_FAINT()),
            Style::default().fg(theme::TEXT_DIM()),
            "waiting",
            Style::default().fg(theme::TEXT_FAINT()),
        )
    };
    let status_w = status.chars().count();
    let name_budget = (width as usize).saturating_sub(2 + status_w + 1);
    let name = truncate_chars(&row.opponent, name_budget);
    let pad = (width as usize)
        .saturating_sub(2 + name.chars().count() + status_w)
        .max(1);
    Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(name, name_style),
        Span::raw(" ".repeat(pad)),
        Span::styled(status.to_string(), status_style),
    ])
}

fn empty_slot_line() -> Line<'static> {
    Line::from(Span::styled(
        "  ─",
        Style::default().fg(theme::BORDER_DIM()),
    ))
}

/// `lobby: 2 open · c0ld`, glowing while there are unseen challenges.
fn lobby_line(width: u16, props: &DailyPanelProps) -> Line<'static> {
    let style = if props.lobby_glow {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else if props.open_count > 0 {
        Style::default().fg(theme::TEXT_DIM())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let mut text = format!("lobby: {} open", props.open_count);
    if let Some(name) = &props.latest_challenger {
        text.push_str(" · ");
        text.push_str(name);
    }
    Line::from(Span::styled(truncate_chars(&text, width as usize), style))
}

fn entries_line(entry_count: usize, entry_cap: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("entries {entry_count}/{entry_cap}"),
        Style::default().fg(theme::TEXT_FAINT()),
    ))
}

fn hints_line() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "g",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" games · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            "/challenge",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn props_with(matches: Vec<DailyPanelMatchRow>, open_count: usize) -> DailyPanelProps {
        DailyPanelProps {
            matches,
            open_count,
            latest_challenger: (open_count > 0).then(|| "c0ld".to_string()),
            lobby_glow: false,
            entry_count: 1,
            entry_cap: 5,
        }
    }

    #[test]
    fn panel_height_is_stable_across_states() {
        let empty = props_with(Vec::new(), 0);
        let busy = props_with(
            (0..6)
                .map(|i| DailyPanelMatchRow {
                    opponent: format!("player{i}"),
                    my_turn: i == 0,
                })
                .collect(),
            3,
        );
        for props in [&empty, &busy] {
            let lines = daily_panel_lines(21, props);
            assert_eq!(lines.len(), DAILY_PANEL_HEIGHT as usize);
        }
    }

    #[test]
    fn empty_slots_render_dashes() {
        let props = props_with(
            vec![DailyPanelMatchRow {
                opponent: "mira".to_string(),
                my_turn: true,
            }],
            0,
        );
        let texts: Vec<String> = daily_panel_lines(21, &props).iter().map(line_text).collect();
        assert!(texts[1].starts_with("► mira"));
        assert!(texts[1].trim_end().ends_with("your turn"));
        assert_eq!(texts[2].trim_end(), "  ─");
        assert_eq!(texts[3].trim_end(), "  ─");
        assert_eq!(texts[4].trim_end(), "  ─");
        assert!(texts[5].starts_with("lobby: 0 open"));
        assert!(texts[6].starts_with("entries 1/5"));
    }

    #[test]
    fn lobby_line_names_latest_challenger() {
        let props = props_with(Vec::new(), 2);
        let texts: Vec<String> = daily_panel_lines(30, &props).iter().map(line_text).collect();
        assert_eq!(texts[5].trim_end(), "lobby: 2 open · c0ld");
    }
}
