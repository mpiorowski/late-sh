//! A stream on the live strip (`app/live/`): somebody went live, so the room
//! sees who and what, and can walk into the stream's room with a key. A drawn screen
//! with the on-air mark sits in the picture column; the words beside it are
//! the title, the room and how many are watching, and who went live.

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
};

use super::registry::LiveStreamView;

/// Rows of the drawn screen, centred in the picture column's rows. Odd, so
/// the mark has a middle row inside the frame.
const SCREEN_ROWS: usize = 7;
/// The mark in the middle of the drawn screen.
const ON_AIR: &str = "⦿ LIVE";

/// A stream as the live strip paints it.
#[derive(Clone, Debug)]
pub struct StreamStripView {
    pub stream: LiveStreamView,
}

/// Every stream that has gone live, stamped with when its media first
/// flowed: a stream is news when it starts. A pending stream has no stamp
/// and is never offered, so the strip never points at a black screen; a
/// refresh through grace keeps the stamp, so it does not join again.
pub(crate) fn candidates(streams: &[LiveStreamView]) -> Vec<LiveCandidate> {
    streams
        .iter()
        .filter_map(|stream| {
            stream.went_live_at.map(|went_live_at| LiveCandidate {
                source: LiveSource::Stream(stream.user_id),
                updated: went_live_at,
                aimed_at: None,
            })
        })
        .collect()
}

/// One streamer's stream as the strip paints it. `None` once it ended.
pub(crate) fn view(streams: &[LiveStreamView], streamer_id: Uuid) -> Option<StreamStripView> {
    streams
        .iter()
        .find(|stream| stream.user_id == streamer_id && stream.went_live_at.is_some())
        .map(|stream| StreamStripView {
            stream: stream.clone(),
        })
}

pub(crate) fn body(budget: usize, strip: &StreamStripView) -> StripBody {
    StripBody {
        picture: screen_lines(),
        words: word_rows(budget, strip),
        hint: key_hint_spans(
            budget,
            &[HintPart::Key("o"), HintPart::Text(" or click for the room")],
        ),
        glow: glow(),
    }
}

/// A stream on the strip is on air for as long as it is offered.
pub(crate) fn glow() -> bool {
    true
}

/// `stream mat · Title`, after the rule label.
pub(crate) fn compact_spans(rest: u16, strip: &StreamStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!("stream {} · ", strip.stream.username);
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&title(strip), left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

/// A drawn screen with the on-air mark in the middle: the stream's picture
/// lives on the watch page, never in the terminal.
fn screen_lines() -> Vec<Line<'static>> {
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner)),
            frame,
        ))
    };
    let mark_width = ON_AIR.chars().count();
    let lead = (inner - mark_width) / 2;
    let mut lines = vec![edge("╭", "╮")];
    for row in 1..SCREEN_ROWS - 1 {
        let middle = if row == SCREEN_ROWS / 2 {
            vec![
                Span::styled("│".to_string(), frame),
                Span::raw(" ".repeat(lead)),
                Span::styled(
                    ON_AIR.to_string(),
                    Style::default()
                        .fg(theme::ERROR())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" ".repeat(inner - mark_width - lead)),
                Span::styled("│".to_string(), frame),
            ]
        } else {
            vec![
                Span::styled("│".to_string(), frame),
                Span::raw(" ".repeat(inner)),
                Span::styled("│".to_string(), frame),
            ]
        };
        lines.push(Line::from(middle));
    }
    lines.push(edge("╰", "╯"));
    lines
}

/// The words beside the picture, one entry per picture row: the title, how
/// many are watching, and who went live.
fn word_rows(budget: usize, strip: &StreamStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(&title(strip), budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    rows[2] = vec![Span::styled(
        truncate_chars(&format!("{} watching", strip.stream.watching), budget),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    rows[3] = event_spans(budget, strip);
    rows
}

fn title(strip: &StreamStripView) -> String {
    if strip.stream.title.is_empty() {
        "a stream".to_string()
    } else {
        strip.stream.title.clone()
    }
}

/// `mat went live`, the streamer in amber.
fn event_spans(budget: usize, strip: &StreamStripView) -> Vec<Span<'static>> {
    let name = truncate_chars(&strip.stream.username, budget);
    let left = budget.saturating_sub(name.chars().count());
    vec![
        Span::styled(
            name,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_chars(" went live", left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

#[cfg(test)]
#[path = "live_test.rs"]
mod live_test;
