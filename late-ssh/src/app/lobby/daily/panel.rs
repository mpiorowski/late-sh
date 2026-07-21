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

use super::{state::DailyState, svc::DailyOutcome};

/// Four match slots + status line (open count + entries) + key hints. The
/// panel has no title row of its own: the sidebar's labeled separator rule
/// (`── lobby ────`, glowing on your-turn) is the title.
pub(crate) const DAILY_PANEL_HEIGHT: u16 = 6;
/// The cap (`DAILY_MAX_ACTIVE_ENTRIES`, 10) now exceeds these four slots, so
/// the panel is a top-4 view, not a full mirror: it shows the most actionable
/// matches (your-turn first) and the modal shows the rest.
const MATCH_SLOTS: usize = 4;

/// Inputs for the panel, bundled so the pure line builder is easy to drive
/// from tests.
pub(crate) struct DailyPanelProps {
    /// Slot rows in display order: your-turn first (nearest deadline within),
    /// then unseen results, then waiting. Only the first four render; with the
    /// cap above four the tail (typically waiting rows) lives in the modal.
    pub matches: Vec<DailyPanelMatchRow>,
    pub open_count: usize,
    pub lobby_glow: bool,
    pub entry_count: usize,
    pub entry_cap: usize,
}

pub(crate) struct DailyPanelMatchRow {
    pub opponent: String,
    pub status: DailyPanelRowStatus,
}

/// What a slot row is telling the player. Won/Lost/Draw rows are unseen
/// results lingering until acknowledged in the modal.
pub(crate) enum DailyPanelRowStatus {
    YourTurn,
    Waiting,
    Won,
    Lost,
    Draw,
}

pub(crate) fn draw_daily_inline(
    frame: &mut Frame,
    area: Rect,
    state: &DailyState,
    lobby_glow: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let my_matches = state.my_matches();
    let (turn_rows, waiting_rows): (Vec<_>, Vec<_>) =
        my_matches.iter().partition(|item| state.my_turn(item));
    let match_row =
        |item: &&crate::app::lobby::daily::svc::DailyMatchItem, status| DailyPanelMatchRow {
            opponent: state
                .opponent_of(item)
                .1
                .unwrap_or_else(|| "player".to_string()),
            status,
        };
    // Actionable beats news beats waiting: your-turn rows, then unseen
    // results, then matches waiting on the opponent.
    let mut matches: Vec<DailyPanelMatchRow> = turn_rows
        .iter()
        .map(|item| match_row(item, DailyPanelRowStatus::YourTurn))
        .collect();
    matches.extend(state.my_finished().into_iter().map(|item| {
        DailyPanelMatchRow {
            opponent: item
                .opponent_of(state.user_id())
                .1
                .unwrap_or_else(|| "player".to_string()),
            status: match item.outcome_for(state.user_id()) {
                DailyOutcome::Won => DailyPanelRowStatus::Won,
                DailyOutcome::Lost => DailyPanelRowStatus::Lost,
                DailyOutcome::Draw => DailyPanelRowStatus::Draw,
            },
        }
    }));
    matches.extend(
        waiting_rows
            .iter()
            .map(|item| match_row(item, DailyPanelRowStatus::Waiting)),
    );
    let lobby = state.lobby();
    let props = DailyPanelProps {
        matches,
        open_count: lobby.len(),
        lobby_glow,
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

/// `► mira        your turn` / `  c0ld          waiting` / `► kal   you won`.
fn match_line(width: u16, row: &DailyPanelMatchRow) -> Line<'static> {
    // Everything but "waiting" is an attention row: glowing marker, bright
    // bold name, accent-colored status.
    let (marker_color, status, status_color) = match row.status {
        DailyPanelRowStatus::YourTurn => (theme::AMBER_GLOW(), "your turn", theme::AMBER()),
        DailyPanelRowStatus::Won => (theme::SUCCESS(), "you won", theme::SUCCESS()),
        DailyPanelRowStatus::Lost => (theme::ERROR(), "you lost", theme::ERROR()),
        DailyPanelRowStatus::Draw => (theme::AMBER(), "draw", theme::AMBER()),
        DailyPanelRowStatus::Waiting => (theme::TEXT_FAINT(), "waiting", theme::TEXT_FAINT()),
    };
    let (marker, marker_style, name_style, status_style) =
        if matches!(row.status, DailyPanelRowStatus::Waiting) {
            (
                "  ",
                Style::default().fg(marker_color),
                Style::default().fg(theme::TEXT_DIM()),
                Style::default().fg(status_color),
            )
        } else {
            (
                "► ",
                Style::default()
                    .fg(marker_color)
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
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

/// `ctrl+q · \` toggle` — the two keys that front the Lobby: `ctrl+q` opens
/// the modal, and the backtick toggles you through the games waiting on you
/// (boards on your move, seated tables, unfinished dailies) and back to chat.
/// Constant chrome, both keys always shown.
fn hints_line() -> Line<'static> {
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let sep_style = Style::default().fg(theme::TEXT_FAINT());
    Line::from(vec![
        Span::styled("ctrl+q", key_style),
        Span::styled(" · ", sep_style),
        Span::styled("`", key_style.fg(theme::AMBER())),
        Span::styled(" toggle", Style::default().fg(theme::TEXT_DIM())),
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
#[path = "panel_test.rs"]
mod panel_test;
