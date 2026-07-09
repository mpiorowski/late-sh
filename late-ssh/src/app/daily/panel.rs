//! Right-sidebar Lobby panel (daily correspondence games): passive, fixed
//! height, stable chrome. Slots render dashes when empty so the panel never
//! changes shape between states; all interaction lives in the modal
//! (`Ctrl+Q`).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

use super::state::DailyState;

/// Four match slots + status line (open count + entries) + key hints. The
/// panel has no title row of its own: the sidebar's labeled separator rule
/// (`── lobby ────`, glowing on your-turn) is the title.
pub(crate) const DAILY_PANEL_HEIGHT: u16 = 6;
const MATCH_SLOTS: usize = 4;

/// Inputs for the panel, bundled so the pure line builder is easy to drive
/// from tests.
pub(crate) struct DailyPanelProps {
    /// Sorted my-matches rows: your-turn first, then nearest deadline.
    pub matches: Vec<DailyPanelMatchRow>,
    pub open_count: usize,
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
        lobby_glow: state.lobby_glow(),
        entry_count: state.entry_count(),
        entry_cap: state.entry_cap(),
    };
    let lines = daily_panel_lines(area.width, &props);
    frame.render_widget(Paragraph::new(lines), area);
}

fn daily_panel_lines(width: u16, props: &DailyPanelProps) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(DAILY_PANEL_HEIGHT as usize);
    for slot in 0..MATCH_SLOTS {
        match props.matches.get(slot) {
            Some(row) => lines.push(match_line(width, row)),
            None => lines.push(empty_slot_line()),
        }
    }

    lines.push(status_line(width, props));
    lines.push(hints_line());
    lines
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

/// `2 open · 1/4` — open lobby challenges and your entry usage in one row.
/// The open count glows while there are unseen challenges; the entries part
/// stays faint so the liquidity signal carries the row.
fn status_line(width: u16, props: &DailyPanelProps) -> Line<'static> {
    let open_style = if props.lobby_glow {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else if props.open_count > 0 {
        Style::default().fg(theme::TEXT_DIM())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let open_text = format!("{} open", props.open_count);
    let entries_text = format!(" · {}/{}", props.entry_count, props.entry_cap);
    let budget = (width as usize).saturating_sub(open_text.chars().count());
    Line::from(vec![
        Span::styled(open_text, open_style),
        Span::styled(
            truncate_chars(&entries_text, budget),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn hints_line() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "ctrl+q",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
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
            lobby_glow: false,
            entry_count: 1,
            entry_cap: 4,
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
        let texts: Vec<String> = daily_panel_lines(21, &props)
            .iter()
            .map(line_text)
            .collect();
        assert!(texts[0].starts_with("► mira"));
        assert!(texts[0].trim_end().ends_with("your turn"));
        assert_eq!(texts[1].trim_end(), "  ─");
        assert_eq!(texts[2].trim_end(), "  ─");
        assert_eq!(texts[3].trim_end(), "  ─");
        assert_eq!(texts[4].trim_end(), "0 open · 1/4");
    }

    #[test]
    fn status_line_merges_open_count_and_entries() {
        let props = props_with(Vec::new(), 2);
        let texts: Vec<String> = daily_panel_lines(30, &props)
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(texts[4].trim_end(), "2 open · 1/4");
    }
}
