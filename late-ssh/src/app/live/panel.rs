//! The right sidebar's Live panel: what the house can watch or read right
//! now, one row per thing, four rows, under its `── live ──` rule. Three
//! kinds today: a stream that has gone live (who, the title, how many are
//! watching), a live game on a watchable door (which game, where its
//! player is or how long they have been in, how many have the watch open;
//! who is playing is the watch's to say, a handle is long and the row is
//! not), and a News share still fresh (the title, and a dot while it is
//! unread), which is the one piece of news the room has no other way to
//! catch up on. Every
//! row says what it is in its first column, `stream`, the game's name, or
//! `news`, each kind in its own color, so a glance tells the rows apart.
//! A deadchannel run or fight lands here as a new [`LivePanelRow`] variant
//! with its own row text and its own `LiveSource` for the click.
//!
//! The four rows are shared by the floor rule (`arrange`): every kind with
//! something gets one row first, then the leftover goes by priority,
//! streams before games before news, and what does not fit folds into a
//! `+N more` row. So two streams never push the games off the panel, and a
//! stream and a game never push the one fresh link off it. The rows that
//! made it are then shown grouped, streams on top, games, news last,
//! newest first within a kind: the floor rule decides what is on the
//! panel, not where.
//!
//! Every row ends in its key, `s1` to `s4`, drawn the way the music panel
//! draws `v1` to `v5`: `s` then the digit opens it from the keyboard
//! (`input::open_from_prefix`), and a click on the row
//! opens it the way the strip's click does (`input::open_from_panel_click`),
//! both off the sources the draw records in `LiveState::panel_hit`.

use std::cell::Cell;

use chrono::{DateTime, Duration, Utc};
use late_core::models::article::ArticleFeedItem;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use super::pick::LiveSource;
use super::ui::truncate_chars;
use crate::app::chat::news::state::{ReadCursor, is_unread_at};
use crate::app::common::theme;
use crate::app::door::spectate::{
    state::{LiveGameKey, LiveRow},
    svc::OpenWatches,
    ui::duration_label,
};
use crate::app::stream::registry::LiveStreamView;

/// Rows under the rule. The rule is the title, so this is all rows.
pub(crate) const LIVE_PANEL_HEIGHT: u16 = 4;
pub(crate) const LIVE_PANEL_ROWS: usize = LIVE_PANEL_HEIGHT as usize;

/// Where the panel drew this frame and the source on each of its rows,
/// render-recorded for the click that opens one.
pub(crate) type LivePanelHit = Cell<Option<(Rect, [Option<LiveSource>; LIVE_PANEL_ROWS])>>;
/// How long a News share stays on the panel: reading it is still possible
/// long after, but after an hour it is the News room's, not the room's.
pub(crate) const NEWS_LIFETIME: Duration = Duration::hours(1);
/// The row's key at the right edge, `s1`, and the space before it.
const KEY_COLS: usize = 3;
/// The first column: what the row is. `stream`, the game's name
/// (`nethack` fills it), or `news`.
const KIND_COLS: usize = 7;
const STREAM_KIND: &str = "stream";
const NEWS_KIND: &str = "news";

/// One thing the house can watch or read, as the panel lists it.
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
    /// A News article shared within [`NEWS_LIFETIME`].
    News {
        article_id: Uuid,
        title: String,
        /// Newer than the reader's News cursor: the badge's rule, so the
        /// dot clears when the News room is visited, as the badge does.
        unread: bool,
    },
}

impl LivePanelRow {
    /// The kind's place on the panel: streams on top, games, news last.
    /// Also the floor rule's priority for the leftover rows.
    fn rank(&self) -> usize {
        match self {
            LivePanelRow::Stream { .. } => 0,
            LivePanelRow::DoorGame { .. } => 1,
            LivePanelRow::News { .. } => 2,
        }
    }

    /// What a click on the row opens.
    pub fn source(&self) -> LiveSource {
        match self {
            LivePanelRow::Stream { user_id, .. } => LiveSource::Stream(*user_id),
            LivePanelRow::DoorGame { key, .. } => LiveSource::DoorGame(*key),
            LivePanelRow::News { article_id, .. } => LiveSource::NewsArticle(*article_id),
        }
    }
}

/// What the panel draws this frame: its rows, in panel order, and where it
/// records the click targets.
pub(crate) struct LivePanelProps<'a> {
    pub rows: &'a [LivePanelRow],
    /// Written by the draw: the body rect and the source on each of its
    /// rows (`None` for a blank slot or the `+N more` row).
    pub hit: &'a LivePanelHit,
    pub now: DateTime<Utc>,
}

/// The panel's rows in panel order: the floor rule over the streams that
/// have gone live (a pending one has no stamp and is not listed, so the
/// panel never points at a black screen), the watchable doors' live games
/// with the open-watch count on each, and the News shares younger than
/// [`NEWS_LIFETIME`] (unread against `read_cursor`), each kind newest
/// first, the kinds in `rank` order.
pub(crate) fn rows(
    streams: &[LiveStreamView],
    live: &[LiveRow],
    articles: &[ArticleFeedItem],
    read_cursor: &ReadCursor,
    open_watches: &OpenWatches,
    now: DateTime<Utc>,
) -> Vec<LivePanelRow> {
    let mut streams: Vec<(DateTime<Utc>, LivePanelRow)> = streams
        .iter()
        .filter_map(|stream| {
            let went_live_at = stream.went_live_at?;
            Some((
                went_live_at,
                LivePanelRow::Stream {
                    user_id: stream.user_id,
                    username: stream.username.clone(),
                    title: stream.title.clone(),
                    went_live_at,
                    watching: stream.watching,
                },
            ))
        })
        .collect();
    let mut games: Vec<(u64, LivePanelRow)> = live
        .iter()
        .map(|row| {
            (
                row.entry.started_unix,
                LivePanelRow::DoorGame {
                    key: row.key,
                    status: row.entry.status.clone(),
                    started_unix: row.entry.started_unix,
                    watching: open_watches.watchers_of(row.key),
                },
            )
        })
        .collect();
    let mut news: Vec<(DateTime<Utc>, LivePanelRow)> = articles
        .iter()
        .filter(|item| now.signed_duration_since(item.article.created) < NEWS_LIFETIME)
        .map(|item| {
            (
                item.article.created,
                LivePanelRow::News {
                    article_id: item.article.id,
                    title: item.article.title.clone(),
                    unread: is_unread_at(item, read_cursor),
                },
            )
        })
        .collect();
    streams.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    games.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    news.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    arrange([
        streams.into_iter().map(|(_, row)| row).collect(),
        games.into_iter().map(|(_, row)| row).collect(),
        news.into_iter().map(|(_, row)| row).collect(),
    ])
}

/// The floor rule. `kinds` is each kind's rows newest first, in `rank`
/// order. The first row of every kind that has one makes the panel, then
/// every kind's rest in rank order, as far as the slots go (the draw shows
/// the first four, three and a `+N more` when there are more): so the
/// floors always make the panel and the leftover slot goes to the
/// highest-ranked kind with a second row. The rows that made it come out
/// grouped by rank, streams on top, newest first within a kind (the sort
/// is stable); the rest follow in floor order, they only count.
fn arrange<const KINDS: usize>(kinds: [Vec<LivePanelRow>; KINDS]) -> Vec<LivePanelRow> {
    let mut arranged = Vec::with_capacity(kinds.iter().map(Vec::len).sum());
    let mut rests: Vec<std::vec::IntoIter<LivePanelRow>> = Vec::with_capacity(KINDS);
    for kind in kinds {
        let mut rows = kind.into_iter();
        if let Some(first) = rows.next() {
            arranged.push(first);
        }
        rests.push(rows);
    }
    for rest in rests {
        arranged.extend(rest);
    }
    let shown = shown_rows(arranged.len());
    arranged[..shown].sort_by_key(LivePanelRow::rank);
    arranged
}

pub(crate) fn draw_live_inline(frame: &mut Frame, area: Rect, props: &LivePanelProps<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = panel_lines(area.width, props.rows, props.now);
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
fn panel_lines(width: u16, rows: &[LivePanelRow], now: DateTime<Utc>) -> Vec<Line<'static>> {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut lines = Vec::with_capacity(LIVE_PANEL_ROWS);
    if rows.is_empty() {
        lines.push(Line::from(Span::styled("nobody playing", faint)));
    }
    let shown = shown_rows(rows.len());
    for (slot, row) in rows.iter().take(shown).enumerate() {
        lines.push(row_line(usize::from(width), slot + 1, row, now));
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

/// What sits at a row's right edge: the viewers for something being
/// watched, the unread dot for a link.
enum Edge {
    Watching(usize),
    Unread(bool),
}

/// The unread mark on a link's row.
const UNREAD: &str = "\u{25cf}";

/// `stream  dax late… ·3 s1`, `nethack Xp3 Dlvl:4 s2`, `news    a titl…
/// ● s3`: what it is, then the middle, then the edge, then the row's
/// key at the right, `s` and its number, in the music panel's key style.
/// The first column names the kind in the kind's color: `stream` in
/// amber, since someone is on air; the game's name, since which game it
/// is matters more than who is in it; `news` faint, since a link is not
/// happening. The middle is the streamer and the title, where the player
/// is (or `12m in`; the handle is left to the watch's header), or the
/// link's title. It takes whatever the first column and the edge leave.
fn row_line(width: usize, number: usize, row: &LivePanelRow, now: DateTime<Utc>) -> Line<'static> {
    let (kind, kind_style, middle, edge) = match row {
        LivePanelRow::Stream {
            username,
            title,
            watching,
            ..
        } => (
            STREAM_KIND,
            Style::default().fg(theme::AMBER()),
            format!("{username} {title}"),
            Edge::Watching(*watching),
        ),
        LivePanelRow::DoorGame {
            key,
            status,
            started_unix,
            watching,
        } => {
            let place = match status.is_empty() {
                true => {
                    let now_unix = u64::try_from(now.timestamp()).unwrap_or(0);
                    format!(
                        "{} in",
                        duration_label(now_unix.saturating_sub(*started_unix) / 60)
                    )
                }
                false => status.clone(),
            };
            (
                key.game().slug(),
                Style::default().fg(theme::TEXT_DIM()),
                place,
                Edge::Watching(*watching),
            )
        }
        LivePanelRow::News { title, unread, .. } => (
            NEWS_KIND,
            Style::default().fg(theme::TEXT_FAINT()),
            title.clone(),
            Edge::Unread(*unread),
        ),
    };
    let edge = match edge {
        Edge::Watching(0) => String::new(),
        Edge::Watching(n) => format!(" \u{b7}{n}"),
        Edge::Unread(true) => format!(" {UNREAD}"),
        Edge::Unread(false) => String::new(),
    };
    let room = width.saturating_sub(KIND_COLS + 1 + edge.chars().count() + KEY_COLS);
    let middle = truncate_chars(&middle, room);
    Line::from(vec![
        Span::styled(format!("{kind:<KIND_COLS$} "), kind_style),
        Span::styled(
            format!("{middle:<room$}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(edge, Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            format!(" s{number}"),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

#[cfg(test)]
#[path = "panel_test.rs"]
mod panel_test;
