//! A board post on the house's surfaces. A timed post: an hour before it
//! starts the live strip (`app/live/`) shows what it is, who posted it and
//! how many are in; when it starts it goes up again, lit; the Live panel
//! holds a row for it from the hour before until it ends. A drawn calendar
//! page sits in the strip's picture column; the words beside it are the
//! title, how long until it starts or that it is on, the count, and who
//! posted it. An all-day post never touches the strip or the panel: it
//! takes the #lounge topic row for its day. Personal events reach none of
//! this; they are the viewer's and live on screen 7 alone.

use chrono::{DateTime, Datelike, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{CalendarEvent, EventTiming, STRIP_LEAD};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{HintPart, PICTURE_COLS, PICTURE_ROWS, StripBody, key_hint_spans, truncate_chars},
    },
    pot::state::lead_time,
};

/// An event as the live strip paints it.
#[derive(Clone, Debug)]
pub struct EventStripView {
    pub event: CalendarEvent,
    pub tz: Tz,
    pub now: DateTime<Utc>,
}

/// Every timed post inside [`STRIP_LEAD`] of its start or already on.
/// Stamped an hour before its start, and again at the start, so it joins the
/// queue twice: once as a heads-up, once as the thing happening. Both stamps
/// are the row's own instant, shared by every replica.
pub(crate) fn candidates(live: &[CalendarEvent], now: DateTime<Utc>) -> Vec<LiveCandidate> {
    live.iter()
        .filter(|event| event.timed() && event.live(now))
        .map(|event| LiveCandidate {
            source: LiveSource::BoardEvent(event.id),
            updated: if event.started(now) {
                event.starts_at
            } else {
                event.starts_at - STRIP_LEAD
            },
            aimed_at: None,
        })
        .collect()
}

/// One event as the strip paints it. `None` once it is over or gone.
pub(crate) fn view(
    live: &[CalendarEvent],
    id: Uuid,
    tz: Tz,
    now: DateTime<Utc>,
) -> Option<EventStripView> {
    live.iter()
        .find(|event| event.id == id && event.timed() && event.live(now))
        .map(|event| EventStripView {
            event: event.clone(),
            tz,
            now,
        })
}

pub(crate) fn body(budget: usize, strip: &EventStripView) -> StripBody {
    StripBody {
        picture: page_lines(strip),
        words: word_rows(budget, strip),
        hint: key_hint_spans(
            budget,
            &[
                HintPart::Key("o"),
                HintPart::Text(" or click for the board"),
            ],
        ),
        glow: glow(strip),
    }
}

/// Lit once the event is on.
pub(crate) fn glow(strip: &EventStripView) -> bool {
    strip.event.started(strip.now)
}

/// A timed post as the Live panel lists it: the start as a clock time in
/// the viewer's zone, since the panel holds the row from an hour before
/// until the end and a countdown would need a tick to stay true.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelEvent {
    pub id: Uuid,
    pub title: String,
    /// `21:00`, in the viewer's zone.
    pub time: String,
    pub going: i64,
}

/// The panel's rows for the live posts, in the order given (soonest first).
pub(crate) fn panel_events(live: &[CalendarEvent], tz: Tz) -> Vec<PanelEvent> {
    live.iter()
        .filter(|event| event.timed())
        .map(|event| PanelEvent {
            id: event.id,
            title: event.title.clone(),
            time: event
                .starts_at
                .with_timezone(&tz)
                .format("%H:%M")
                .to_string(),
            going: event.going,
        })
        .collect()
}

/// The #lounge topic row on a day the board has all-day posts:
/// `📅 Hackathon · Community call`. The row exists only on the day, so it
/// never says when. `None` on any other day.
pub(crate) fn day_text(live: &[CalendarEvent], now: DateTime<Utc>) -> Option<String> {
    let titles: Vec<&str> = live
        .iter()
        .filter(|event| !event.timed() && event.live(now))
        .map(|event| event.title.as_str())
        .collect();
    if titles.is_empty() {
        return None;
    }
    Some(format!("\u{1F4C5} {}", titles.join(" · ")))
}

/// `event mat · Movie night · in 58m`, after the rule label.
pub(crate) fn compact_spans(rest: u16, strip: &EventStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!("event {} · ", strip.event.creator_name);
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    let tail = format!(" · {}", when(strip));
    let title_room = left.saturating_sub(tail.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&strip.event.title, title_room),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            truncate_chars(&tail, left.saturating_sub(title_room)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]
}

/// `in 58m`, `in 1h`, or `on now`.
pub(crate) fn when(strip: &EventStripView) -> String {
    when_at(&strip.event, strip.now)
}

pub(crate) fn when_at(event: &CalendarEvent, now: DateTime<Utc>) -> String {
    if event.started(now) {
        "on now".to_string()
    } else {
        format!(
            "in {}",
            lead_time((event.starts_at - now).num_seconds().max(60))
        )
    }
}

/// A calendar page: the month on top, the day large, the weekday and the
/// start time under it.
fn page_lines(strip: &EventStripView) -> Vec<Line<'static>> {
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner)),
            frame,
        ))
    };
    let centered = |text: String, style: Style| {
        let text = truncate_chars(&text, inner);
        let lead = (inner - text.chars().count()) / 2;
        Line::from(vec![
            Span::styled("│".to_string(), frame),
            Span::raw(" ".repeat(lead)),
            Span::styled(text.clone(), style),
            Span::raw(" ".repeat(inner - lead - text.chars().count())),
            Span::styled("│".to_string(), frame),
        ])
    };
    let start = strip.event.starts_at.with_timezone(&strip.tz);
    let time = match strip.event.timing {
        EventTiming::AllDay { .. } => "all day".to_string(),
        EventTiming::Timed { .. } => start.format("%H:%M %Z").to_string(),
    };
    vec![
        edge("╭", "╮"),
        centered(
            start.format("%b").to_string().to_uppercase(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        centered(String::new(), frame),
        centered(
            start.day().to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        centered(
            start.format("%A").to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        centered(time, Style::default().fg(theme::TEXT())),
        edge("╰", "╯"),
    ]
}

/// The words beside the page, one entry per picture row: the title, when
/// and how many, who posted it.
fn word_rows(budget: usize, strip: &EventStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(&strip.event.title, budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    let going = match strip.event.going {
        0 => " · nobody's in yet".to_string(),
        n => format!(" · {n} in"),
    };
    rows[2] = vec![
        Span::styled(
            when(strip),
            if glow(strip) {
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            },
        ),
        Span::styled(
            truncate_chars(&going, budget.saturating_sub(when(strip).chars().count())),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    rows[3] = poster_spans(budget, strip);
    rows
}

/// `mat posted it`, the poster in amber.
fn poster_spans(budget: usize, strip: &EventStripView) -> Vec<Span<'static>> {
    let name = truncate_chars(&strip.event.creator_name, budget);
    let left = budget.saturating_sub(name.chars().count());
    vec![
        Span::styled(
            name,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_chars(" posted it", left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}
