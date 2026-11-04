//! Home's lobby tile (daily correspondence games): passive, all interaction
//! lives in the modal (`Ctrl+G`).

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
    svc::{DailyChallengeItem, DailyFinishedItem, DailyMatchItem},
};
use crate::app::common::primitives::format_relative_time_short;

/// Inputs for the tile's counts row, bundled so the pure line builder is easy
/// to drive from tests.
pub(crate) struct DailyPanelProps {
    pub open_count: usize,
    pub lobby_glow: bool,
    pub entry_count: usize,
    pub entry_cap: usize,
}

/// The Home lobby tile: the latest tables on the board, newest first, over
/// the `N open · entries/cap` row, with the Lobby's two keys on the last row
/// (`COMPACT_FOOTER_KEYS`). What waits on you is not this tile's
/// job: the attention guest hosted on it takes the tile over while it has
/// something (`../../zen/CONTEXT.md`, the attention guest).
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
    DailyPanelProps {
        open_count: state.lobby().len(),
        lobby_glow,
        entry_count: state.entry_count(),
        entry_cap: state.entry_cap(),
    }
}

/// Feed rows for as many tables as fit above the footer (a quiet board says
/// so in one faint row), then `1 open · 1/10`, then the keys on the last row.
fn daily_compact_lines(
    width: u16,
    height: u16,
    feed: &[DailyFeedRow],
    props: &DailyPanelProps,
) -> Vec<Line<'static>> {
    let room = (height as usize).saturating_sub(2);
    let mut lines = Vec::with_capacity(room + 2);
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
    // Flush with the rows above it, the keys under it as the music dock
    // names its own.
    lines.push(status_line(width, props));
    lines.push(compact_keys_line(width));
    lines
}

/// The two keys the Home tile's last row names, as the arrange footer
/// spells them: `^G` opens the Lobby, the backtick toggles you through the
/// games waiting on you and back to chat.
pub(crate) const COMPACT_FOOTER_KEYS: [(&str, &str); 2] = [("^G", "open"), ("`", "toggle")];

/// `^G open  ` toggle`, trimmed from the right a whole key at a time on a
/// tile too narrow for both.
fn compact_keys_line(width: u16) -> Line<'static> {
    let mut keep = COMPACT_FOOTER_KEYS.len();
    while keep > 0
        && crate::app::statusline::bar::keys_line(&COMPACT_FOOTER_KEYS[..keep]).width()
            > width as usize
    {
        keep -= 1;
    }
    crate::app::statusline::bar::keys_line(&COMPACT_FOOTER_KEYS[..keep])
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

/// `2 open · 1/4`: open lobby challenges and your entry usage in one row.
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
