//! Right-sidebar Lobby panel (daily correspondence games): passive, fixed
//! height, stable chrome. Slots render dashes when empty so the panel never
//! changes shape between states; all interaction lives in the modal
//! (`Ctrl+G`).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

use super::{
    state::DailyState,
    svc::{DailyChallengeItem, DailyFinishedItem, DailyMatchItem, DailyOutcome},
};
use crate::app::common::primitives::format_relative_time_short;

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
    let props = daily_panel_props(state, lobby_glow);
    let lines = daily_panel_lines(area.width, &props);
    frame.render_widget(Paragraph::new(lines), area);
}

/// The Home lobby tile's shape: only the games actually running, then one footer
/// row carrying the open count, your entries, and the two keys. No empty
/// slots, so the tile can be as short as its content. Top-aligned; rows
/// past the area are dropped, the footer never is.
/// The Home lobby tile: the latest tables on the board, newest first, over
/// the `N open · entries/cap` footer. What waits on you is not this tile's
/// job: the attention guest hosted on it takes the tile over while it has
/// something (`../attention/CONTEXT.md`).
pub(crate) fn draw_daily_compact(
    frame: &mut Frame,
    area: Rect,
    state: &DailyState,
    lobby_glow: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let props = daily_panel_props(state, lobby_glow);
    let feed = daily_feed_rows(state);
    let lines = daily_compact_lines(area.width, area.height, &feed, &props);
    frame.render_widget(Paragraph::new(lines), area);
}

/// One table in the Home tile's feed.
pub(crate) struct DailyFeedRow {
    pub game: &'static str,
    /// The challenger alone for an open challenge, `a·b` for a match.
    pub who: String,
    pub state: DailyFeedState,
    /// `2m`, `3h`: how long since the row's last change, computed by the
    /// caller so the line builder stays clock-free.
    pub age: String,
}

/// Where a table in the feed stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DailyFeedState {
    Open,
    Playing,
    Done,
}

/// Every table the snapshot knows, newest change first: open challenges by
/// their posting, matches by their last move, finished ones by their
/// finish. Finished tables stay only while a player has not seen the
/// result, which is as long as the snapshot carries them.
fn daily_feed_rows(state: &DailyState) -> Vec<DailyFeedRow> {
    let name = |username: &Option<String>| username.clone().unwrap_or_else(|| "player".to_string());
    let mut rows: Vec<(chrono::DateTime<chrono::Utc>, DailyFeedRow)> = Vec::new();
    for item in state.lobby() {
        let DailyChallengeItem {
            game,
            created,
            challenger_username,
            ..
        } = item;
        rows.push((
            *created,
            DailyFeedRow {
                game: game.label(),
                who: name(challenger_username),
                state: DailyFeedState::Open,
                age: format_relative_time_short(*created),
            },
        ));
    }
    for item in state.all_matches() {
        let DailyMatchItem {
            game,
            challenger_username,
            opponent_username,
            updated,
            ..
        } = item;
        rows.push((
            *updated,
            DailyFeedRow {
                game: game.label(),
                who: format!("{}·{}", name(challenger_username), name(opponent_username)),
                state: DailyFeedState::Playing,
                age: format_relative_time_short(*updated),
            },
        ));
    }
    for item in state.all_finished() {
        let DailyFinishedItem {
            game,
            challenger_username,
            opponent_username,
            finished_at,
            ..
        } = item;
        rows.push((
            *finished_at,
            DailyFeedRow {
                game: game.label(),
                who: format!("{}·{}", name(challenger_username), name(opponent_username)),
                state: DailyFeedState::Done,
                age: format_relative_time_short(*finished_at),
            },
        ));
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    rows.into_iter().map(|(_, row)| row).collect()
}

fn daily_panel_props(state: &DailyState, lobby_glow: bool) -> DailyPanelProps {
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
    DailyPanelProps {
        matches,
        open_count: lobby.len(),
        lobby_glow,
        entry_count: state.entry_count(),
        entry_cap: state.entry_cap(),
    }
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

/// Feed rows for as many tables as fit above the footer (a quiet board says
/// so in one faint row), then `1 open · 1/10`; the key is the title's.
fn daily_compact_lines(
    width: u16,
    height: u16,
    feed: &[DailyFeedRow],
    props: &DailyPanelProps,
) -> Vec<Line<'static>> {
    let room = (height as usize).saturating_sub(1);
    let mut lines = Vec::with_capacity(room + 1);
    if feed.is_empty() {
        if room > 0 {
            lines.push(Line::from(Span::styled(
                "no tables yet",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
    } else {
        for row in feed.iter().take(room) {
            lines.push(feed_line(width, row));
        }
    }
    // Flush with the rows above it; the keys are on the tile's title.
    lines.push(status_line(width, props));
    lines
}

/// `○ chess srw        2m` / `● pool srw·crk     5m` / `· chess crk·srw    1h`:
/// the state as a marker, the game dim, the names bright while the table is
/// live and faint once it is done, the age flush right.
fn feed_line(width: u16, row: &DailyFeedRow) -> Line<'static> {
    let (marker, marker_style, who_style) = match row.state {
        DailyFeedState::Open => (
            "○ ",
            Style::default().fg(theme::AMBER()),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        DailyFeedState::Playing => (
            "● ",
            Style::default().fg(theme::SUCCESS()),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        DailyFeedState::Done => (
            "· ",
            Style::default().fg(theme::TEXT_FAINT()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    };
    let age_w = row.age.chars().count();
    let game = format!("{} ", row.game);
    let fixed = 2 + game.chars().count() + age_w + 1;
    let who = truncate_chars(&row.who, (width as usize).saturating_sub(fixed));
    let pad = (width as usize)
        .saturating_sub(2 + game.chars().count() + who.chars().count() + age_w)
        .max(1);
    Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(game, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(who, who_style),
        Span::raw(" ".repeat(pad)),
        Span::styled(row.age.clone(), Style::default().fg(theme::TEXT_FAINT())),
    ])
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
    let waiting = match row.status {
        DailyPanelRowStatus::Waiting => true,
        DailyPanelRowStatus::YourTurn
        | DailyPanelRowStatus::Won
        | DailyPanelRowStatus::Lost
        | DailyPanelRowStatus::Draw => false,
    };
    let (marker, marker_style, name_style, status_style) = if waiting {
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
    let marker_w = 2;
    let status_w = status.chars().count();
    let name_budget = (width as usize).saturating_sub(marker_w + status_w + 1);
    let name = truncate_chars(&row.opponent, name_budget);
    let pad = (width as usize)
        .saturating_sub(marker_w + name.chars().count() + status_w)
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

/// `ctrl+g · \` toggle` — the two keys that front the Lobby: `ctrl+g` opens
/// the modal, and the backtick toggles you through the games waiting on you
/// (boards on your move, seated tables, unfinished dailies) and back to chat.
/// Constant chrome of the sidebar panel; the Home tile names them on its title.
fn hints_line() -> Line<'static> {
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let sep_style = Style::default().fg(theme::TEXT_FAINT());
    Line::from(vec![
        Span::styled("ctrl+g", key_style),
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
