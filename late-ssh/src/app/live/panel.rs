//! The right sidebar's Live panel: what the house can watch right now, one
//! row per thing, four rows, under its `── live ──` rule. Today that is
//! every stream that has gone live (who, the title, how many are
//! watching), then the live games on the watchable doors (DCSS, NetHack,
//! Brogue): who, where they are (or how long they have been in until the
//! host has read it), and how many have the watch open. Newest on top in
//! each group: what just started is what the room is talking about. A deadchannel run or fight lands here as a new [`LivePanelRow`]
//! variant with its own row text and its own `LiveSource` for the click.
//!
//! A click on a row opens it the way the strip's click does
//! (`input::open_from_panel_click`), off the rect and the sources the draw
//! records in `LiveState::panel_hit`.

use std::cell::Cell;

use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::pick::LiveSource;
use super::ui::truncate_chars;
use crate::app::common::theme;
use crate::app::door::spectate::{
    state::{LiveGameKey, LiveRow},
    svc::OpenWatches,
    ui::duration_label,
};
use crate::app::stream::registry::LiveStreamView;
use uuid::Uuid;

/// Rows under the rule. The rule is the title, so this is all games.
pub(crate) const LIVE_PANEL_HEIGHT: u16 = 4;
pub(crate) const LIVE_PANEL_ROWS: usize = LIVE_PANEL_HEIGHT as usize;
/// The handle's column. Longer handles are cut with an ellipsis: the
/// place beside it is what tells one row from the next.
const HANDLE_COLS: usize = 7;
/// The on-air mark before a stream's title, the strip's.
const ON_AIR: &str = "\u{29bf} ";

/// One thing the house can watch, as the panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LivePanelRow {
    /// A stream that has gone live (`LiveStreamView::went_live_at` set).
    Stream {
        user_id: Uuid,
        username: String,
        title: String,
        went_live_at: DateTime<Utc>,
        /// Viewers on the watch page, as the registry counts them.
        watching: usize,
    },
    DoorGame {
        key: LiveGameKey,
        /// Where the player is, as the host read it off the game; empty
        /// until it has, and the row shows the time in instead.
        status: String,
        started_unix: u64,
        /// People with the game's watch open.
        watching: usize,
    },
}

impl LivePanelRow {
    /// What a click on the row opens.
    pub fn source(&self) -> LiveSource {
        match self {
            LivePanelRow::Stream { user_id, .. } => LiveSource::Stream(*user_id),
            LivePanelRow::DoorGame { key, .. } => LiveSource::DoorGame(*key),
        }
    }
}

/// What the panel draws this frame: its rows, and where it records the
/// click targets.
pub(crate) struct LivePanelProps<'a> {
    pub rows: &'a [LivePanelRow],
    /// Written by the draw: the body rect and the source on each of its
    /// rows (`None` for a blank slot or the `+N more` row).
    pub hit: &'a Cell<Option<(Rect, [Option<LiveSource>; LIVE_PANEL_ROWS])>>,
}

/// The panel's rows: the streams that have gone live, newest first (a
/// pending one has no stamp and is not listed, so the panel never points
/// at a black screen), then the watchable doors' live games across the
/// doors, newest first, with the open-watch count on each.
pub(crate) fn rows(
    streams: &[LiveStreamView],
    live: &[LiveRow],
    open_watches: &OpenWatches,
) -> Vec<LivePanelRow> {
    let mut rows: Vec<LivePanelRow> = streams
        .iter()
        .filter_map(|stream| {
            let went_live_at = stream.went_live_at?;
            Some(LivePanelRow::Stream {
                user_id: stream.user_id,
                username: stream.username.clone(),
                title: stream.title.clone(),
                went_live_at,
                watching: stream.watching,
            })
        })
        .collect();
    rows.sort_by_key(|row| match row {
        LivePanelRow::Stream { went_live_at, .. } => std::cmp::Reverse(*went_live_at),
        LivePanelRow::DoorGame { .. } => unreachable!("only streams so far"),
    });
    let mut games: Vec<LivePanelRow> = live
        .iter()
        .filter_map(|row| {
            let key = LiveGameKey::new(row.game, &row.entry.playname)?;
            Some(LivePanelRow::DoorGame {
                key,
                status: row.entry.status.clone(),
                started_unix: row.entry.started_unix,
                watching: open_watches.watchers_of(key),
            })
        })
        .collect();
    games.sort_by_key(|row| match row {
        LivePanelRow::DoorGame { started_unix, .. } => std::cmp::Reverse(*started_unix),
        LivePanelRow::Stream { .. } => unreachable!("only games here"),
    });
    rows.extend(games);
    rows
}

pub(crate) fn draw_live_inline(frame: &mut Frame, area: Rect, props: &LivePanelProps<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock after the unix epoch")
        .as_secs();
    let lines = panel_lines(area.width, props.rows, now_unix);
    frame.render_widget(Paragraph::new(lines), area);
    props.hit.set(Some((area, hit_sources(props.rows))));
}

/// The source behind each of the four slots: a row's where one is drawn,
/// none under `+N more`, a blank slot, or `nobody playing`.
fn hit_sources(rows: &[LivePanelRow]) -> [Option<LiveSource>; LIVE_PANEL_ROWS] {
    let mut sources = [None; LIVE_PANEL_ROWS];
    for (slot, row) in rows.iter().take(shown_rows(rows.len())).enumerate() {
        sources[slot] = Some(row.source());
    }
    sources
}

/// How many rows draw as rows: all of them when they fit, else one fewer
/// than the slots, the last slot saying how many more there are.
fn shown_rows(count: usize) -> usize {
    if count <= LIVE_PANEL_ROWS {
        count
    } else {
        LIVE_PANEL_ROWS - 1
    }
}

/// The four slots: a row per live thing, `+N more` in the last slot when
/// they do not all fit, `nobody playing` alone when there is nothing, and
/// blank slots after, so the panel never changes shape.
fn panel_lines(width: u16, rows: &[LivePanelRow], now_unix: u64) -> Vec<Line<'static>> {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut lines = Vec::with_capacity(LIVE_PANEL_ROWS);
    if rows.is_empty() {
        lines.push(Line::from(Span::styled("nobody playing", faint)));
    }
    let shown = shown_rows(rows.len());
    for row in rows.iter().take(shown) {
        lines.push(row_line(usize::from(width), row, now_unix));
    }
    if rows.len() > shown {
        lines.push(Line::from(Span::styled(
            format!("+{} more", rows.len() - shown),
            faint,
        )));
    }
    while lines.len() < LIVE_PANEL_ROWS {
        lines.push(Line::default());
    }
    lines
}

/// `mat     XL3 Lair:2 ·3`, `mat     ⦿ late night  ·3`: the handle in its
/// column, where they are (or `12m in`) or the stream's title behind the
/// on-air mark, and the watchers at the right edge when there are any. The
/// middle takes whatever the handle and the count leave.
fn row_line(width: usize, row: &LivePanelRow, now_unix: u64) -> Line<'static> {
    let (handle, middle, watching) = match row {
        LivePanelRow::Stream {
            username,
            title,
            watching,
            ..
        } => (username.as_str(), format!("{ON_AIR}{title}"), *watching),
        LivePanelRow::DoorGame {
            key,
            status,
            started_unix,
            watching,
        } => {
            let place = match status.is_empty() {
                true => format!(
                    "{} in",
                    duration_label(now_unix.saturating_sub(*started_unix) / 60)
                ),
                false => status.clone(),
            };
            (key.playname(), place, *watching)
        }
    };
    let handle = truncate_chars(handle, HANDLE_COLS);
    let eyes = match watching {
        0 => String::new(),
        n => format!(" \u{b7}{n}"),
    };
    let room = width.saturating_sub(HANDLE_COLS + 1 + eyes.chars().count());
    let middle = truncate_chars(&middle, room);
    Line::from(vec![
        Span::styled(
            format!("{handle:<HANDLE_COLS$} "),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            format!("{middle:<room$}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(eyes, Style::default().fg(theme::AMBER_DIM())),
    ])
}

#[cfg(test)]
#[path = "panel_test.rs"]
mod panel_test;
