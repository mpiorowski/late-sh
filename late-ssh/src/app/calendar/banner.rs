//! Home's banner: today's board posts in one row across the top of the page
//! (`zen/ui.rs::draw_board_day`). Every post the Calendars page shows on the
//! viewer's today is on it for the whole day: an all-day post by its title,
//! a timed one with its start in the viewer's zone (its end, when it began
//! on an earlier day), dim once it is over. It is one row whatever the day
//! holds: the posts share its width (`day_spans`). Nothing else on Home
//! shows a board post. Personal events are never on it; they are the
//! viewer's and live on screen 7 alone.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{CalendarEvent, EventTiming};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

use crate::app::common::{primitives::truncate_cells, theme};

/// What the row opens with: the board's mark.
const LEAD: &str = "\u{1F4C5} ";
const SEPARATOR: &str = " · ";
/// The fewest columns a title is cut down to: four letters and the
/// ellipsis. A day too crowded for that folds its last posts into `+N`.
const MIN_TITLE_COLS: usize = 5;

/// One of today's board posts as the banner lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayPost {
    pub title: String,
    pub when: DayWhen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DayWhen {
    /// The post covers the viewer's whole today: an all-day post, or a
    /// timed one that began before today and ends after it. The row says
    /// its title alone.
    AllDay,
    /// A timed post that starts today: its start as a clock time in the
    /// viewer's zone (`21:00`), and whether it has ended.
    At { time: String, over: bool },
    /// A timed post that began on an earlier day and ends today: its end
    /// as a clock time in the viewer's zone, and whether it has ended. The
    /// row says `(until 02:00)`, so last night's start never reads as
    /// tonight's.
    Until { time: String, over: bool },
}

/// Today's board posts for a viewer in `tz` (`CalendarEvent::on_day`): the
/// ones covering the whole day first, then the timed ones, each kind in the
/// order given (soonest first). Empty on a day the board has nothing.
pub(super) fn day_posts(board: &[CalendarEvent], tz: Tz, now: DateTime<Utc>) -> Vec<DayPost> {
    let today = now.with_timezone(&tz).date_naive();
    let tomorrow = today.succ_opt().unwrap();
    let clock = |at: DateTime<Utc>| at.with_timezone(&tz).format("%H:%M").to_string();
    let mut posts: Vec<DayPost> = board
        .iter()
        .filter(|event| event.on_day(now, tz))
        .map(|event| {
            let (first, end_exclusive) = event.timing.dates(tz);
            let over = event.over(now);
            let when = match (&event.timing, first < today, end_exclusive > tomorrow) {
                (EventTiming::AllDay { .. }, _, _) | (EventTiming::Timed { .. }, true, true) => {
                    DayWhen::AllDay
                }
                (EventTiming::Timed { .. }, false, _) => DayWhen::At {
                    time: clock(event.starts_at),
                    over,
                },
                (EventTiming::Timed { .. }, true, false) => DayWhen::Until {
                    time: clock(event.ends_at),
                    over,
                },
            };
            DayPost {
                title: event.title.clone(),
                when,
            }
        })
        .collect();
    // Stable: each kind keeps the order it arrived in.
    posts.sort_by_key(|post| match post.when {
        DayWhen::AllDay => 0,
        DayWhen::At { .. } | DayWhen::Until { .. } => 1,
    });
    posts
}

/// The row in `width` columns: `📅 Hackathon · Movie night (21:00)`.
///
/// Every post gets an equal share of the columns the titles have. A title
/// shorter than its share hands the rest to the longer ones; a longer one
/// is cut with an ellipsis. The time (`(21:00)`, `(until 02:00)`) is never
/// cut. When the titles cannot
/// all keep `MIN_TITLE_COLS`, the last posts fold into a `+N` at the end,
/// as few of them as it takes.
pub(crate) fn day_spans(posts: &[DayPost], width: usize) -> Vec<Span<'static>> {
    let tails: Vec<String> = posts
        .iter()
        .map(|post| match &post.when {
            DayWhen::AllDay => String::new(),
            DayWhen::At { time, .. } => format!(" ({time})"),
            DayWhen::Until { time, .. } => format!(" (until {time})"),
        })
        .collect();
    let titles: Vec<usize> = posts.iter().map(|post| post.title.width()).collect();
    let folded = |shown: usize| match posts.len() - shown {
        0 => String::new(),
        more => format!("+{more}"),
    };
    // The columns that are not a title's, with `shown` posts on the row.
    let fixed = |shown: usize| {
        let tails: usize = tails[..shown].iter().map(|tail| tail.width()).sum();
        let folded = match folded(shown).width() {
            0 => 0,
            columns => SEPARATOR.width() + columns,
        };
        LEAD.width() + tails + SEPARATOR.width() * shown.saturating_sub(1) + folded
    };
    let shortest = |shown: usize| -> usize {
        titles[..shown]
            .iter()
            .map(|title| (*title).min(MIN_TITLE_COLS))
            .sum()
    };
    // One post is the floor: Home is never narrow enough to lose it.
    let shown = (1..=posts.len())
        .rev()
        .find(|shown| fixed(*shown) + shortest(*shown) <= width)
        .unwrap_or(posts.len().min(1));

    // Shortest title first: what it does not need goes back in the pot for
    // the rest, so the last and longest takes whatever is left.
    let mut order: Vec<usize> = (0..shown).collect();
    order.sort_by_key(|post| titles[*post]);
    let mut left = width.saturating_sub(fixed(shown));
    let mut rooms = vec![0; shown];
    for (done, post) in order.into_iter().enumerate() {
        rooms[post] = titles[post].min(left / (shown - done));
        left -= rooms[post];
    }

    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut spans = vec![Span::raw(LEAD)];
    for (index, post) in posts.iter().take(shown).enumerate() {
        if index > 0 {
            spans.push(Span::styled(SEPARATOR, faint));
        }
        let style = match post.when {
            DayWhen::At { over: true, .. } | DayWhen::Until { over: true, .. } => {
                Style::default().fg(theme::AMBER_DIM())
            }
            DayWhen::AllDay
            | DayWhen::At { over: false, .. }
            | DayWhen::Until { over: false, .. } => Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        };
        spans.push(Span::styled(
            format!(
                "{}{}",
                truncate_cells(&post.title, rooms[index]),
                tails[index]
            ),
            style,
        ));
    }
    if shown < posts.len() {
        spans.push(Span::styled(SEPARATOR, faint));
        spans.push(Span::styled(folded(shown), faint));
    }
    spans
}
