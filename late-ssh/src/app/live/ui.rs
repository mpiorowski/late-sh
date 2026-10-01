//! The live strip's frame: one thing happening in the house, painted at the
//! top of the Home chat card for the room to see. A picture sits in a fixed
//! column with the words beside it, over a rule that parts the strip from
//! the chat. It is up only while something just happened (`state.rs`), so
//! it appearing is the news; it never draws an empty state. Two fixed forms,
//! picked by the card's size and never by what is shown (`fit_live_strip`):
//! the picture rows over the rule, or one row on a card too small for that.
//!
//! What goes in the frame is the source's own: a daily match paints its
//! board (`lobby/daily/live_strip.rs`), a booth track its thumbnail
//! (`audio/booth/live.rs`), a News article its ASCII art
//! (`chat/news/live.rs`), a stream a drawn screen (`stream/live.rs`).

use std::cell::Cell;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    audio::booth::live as booth_live,
    chat::news::live as news_live,
    common::theme,
    games::pool_core::canvas::Rgb,
    lobby::daily::{live_board::canvas_background, live_strip as match_strip},
    stream::live as stream_live,
};

use super::{pick::LiveSource, state::LiveStripView};

/// Rows of the picture column: tall enough for a chess board at one row a
/// rank. Anything shorter is centred in it.
pub(crate) const PICTURE_ROWS: u16 = 8;
/// The picture's column: wide enough for an eight-cell grid board at two
/// columns a cell.
pub(crate) const PICTURE_COLS: u16 = 21;
/// The picture rows plus the rule under them.
pub(crate) const LIVE_STRIP_HEIGHT: u16 = 1 + PICTURE_ROWS;
/// The one-row form: the rule label, then what is on.
pub(crate) const LIVE_STRIP_COMPACT_HEIGHT: u16 = 1;
/// Narrower than this and the words beside the picture have no room.
pub(crate) const MIN_FULL_WIDTH: u16 = 56;
/// Rows the messages keep under the full strip, more than the strip takes
/// so the chat stays the larger share of a small card; under that the card
/// takes the one-row form, and under `MIN_COMPACT_HEIGHT` nothing.
const MESSAGE_ROWS_UNDER_FULL: u16 = 10;
const MIN_COMPACT_HEIGHT: u16 = LIVE_STRIP_COMPACT_HEIGHT + 4;
/// Columns between the picture and the words.
pub(crate) const GAP: u16 = 2;
/// The words row the key hint sits on, under what the source says.
const HINT_ROW: usize = 6;

/// Where the strip is drawn. The #lounge card names its keys in the hint
/// row and parts the strip from the chat with the rule; a Zen tile has a
/// border and a title for both, and its title names the tile's keys, as
/// every Zen tile's does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StripHost {
    LoungeCard,
    ZenTile,
}

/// Which form the card fitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StripSize {
    Full,
    Compact,
}

/// What a source paints in the full form.
pub(crate) struct StripBody {
    /// Up to `PICTURE_ROWS` lines, each `PICTURE_COLS` wide at most.
    pub picture: Vec<Line<'static>>,
    /// One entry per picture row, each inside the budget it was given.
    /// The hint row (`HINT_ROW`) is left empty for `hint`.
    pub words: Vec<Vec<Span<'static>>>,
    /// The keys that act on the source (`key_hint_spans`), inside the same
    /// budget. Drawn on the #lounge card only (`StripHost`).
    pub hint: Vec<Span<'static>>,
    /// Whether the rule's label is lit.
    pub glow: bool,
}

/// Carve the strip off the top of a messages area: the form, its rect, and
/// what the messages keep. `None` when the area cannot spare a row.
pub(crate) fn fit_live_strip(area: Rect) -> Option<(StripSize, Rect, Rect)> {
    let size = if area.width >= MIN_FULL_WIDTH
        && area.height >= LIVE_STRIP_HEIGHT + MESSAGE_ROWS_UNDER_FULL
    {
        StripSize::Full
    } else if area.height >= MIN_COMPACT_HEIGHT {
        StripSize::Compact
    } else {
        return None;
    };
    let height = match size {
        StripSize::Full => LIVE_STRIP_HEIGHT,
        StripSize::Compact => LIVE_STRIP_COMPACT_HEIGHT,
    };
    let split = Layout::vertical([Constraint::Length(height), Constraint::Min(1)]).split(area);
    Some((size, split[0], split[1]))
}

pub(crate) fn draw_live_strip(
    frame: &mut Frame,
    area: Rect,
    size: StripSize,
    strip: &LiveStripView<'_>,
    hit: &Cell<Option<(Rect, LiveSource)>>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if let Some(source) = strip.opens() {
        hit.set(Some((area, source)));
    }
    let lines = match size {
        StripSize::Full => live_strip_lines(
            area.width,
            strip,
            canvas_background(),
            StripHost::LoungeCard,
        ),
        StripSize::Compact => vec![live_strip_compact_line(area.width, strip)],
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// The form a Zen tile's inner area fits: the picture rows when it has
/// `PICTURE_ROWS` and the card's width, else the one row. A tile always
/// spares a row (`zen::layout::MIN_TILE_CELLS`).
pub(crate) fn fit_live_tile(area: Rect) -> StripSize {
    if area.width >= MIN_FULL_WIDTH && area.height >= PICTURE_ROWS {
        StripSize::Full
    } else {
        StripSize::Compact
    }
}

/// The strip in a Zen tile's inner area, centred top to bottom: the
/// picture rows with no hint and no rule, or the one row without its rule
/// label, since the tile's title already says what it is.
pub(crate) fn draw_live_tile(
    frame: &mut Frame,
    area: Rect,
    strip: &LiveStripView<'_>,
    hit: &Cell<Option<(Rect, LiveSource)>>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = match fit_live_tile(area) {
        StripSize::Full => {
            live_strip_lines(area.width, strip, canvas_background(), StripHost::ZenTile)
        }
        StripSize::Compact => vec![Line::from(compact_body_spans(area.width, strip))],
    };
    let height = (lines.len() as u16).min(area.height);
    let drawn = Rect::new(
        area.x,
        area.y + (area.height - height) / 2,
        area.width,
        height,
    );
    if let Some(source) = strip.opens() {
        hit.set(Some((drawn, source)));
    }
    frame.render_widget(Paragraph::new(lines), drawn);
}

/// The picture in its column with the words beside it. On the #lounge card
/// the hint row and the rule, which parts the strip from the chat below,
/// make it `LIVE_STRIP_HEIGHT` lines; in a Zen tile it is the
/// `PICTURE_ROWS` alone (`StripHost`). `background` is what a painted
/// picture blends into.
pub(crate) fn live_strip_lines(
    width: u16,
    strip: &LiveStripView<'_>,
    background: Rgb,
    host: StripHost,
) -> Vec<Line<'static>> {
    let budget = usize::from(width.saturating_sub(PICTURE_COLS + GAP));
    let body = match strip {
        LiveStripView::Match(strip) => match_strip::body(budget, strip, background),
        LiveStripView::Track(track) => booth_live::body(budget, track),
        LiveStripView::Article(article) => news_live::body(budget, article),
        LiveStripView::Stream(stream) => stream_live::body(budget, stream),
    };
    frame_lines(width, body, host)
}

/// A source's body in the frame: the picture centred in its column, the
/// words beside it, and on the #lounge card the hint among them and the
/// rule under both.
fn frame_lines(width: u16, body: StripBody, host: StripHost) -> Vec<Line<'static>> {
    let StripBody {
        mut picture,
        mut words,
        hint,
        glow,
    } = body;
    let mut lines = Vec::with_capacity(LIVE_STRIP_HEIGHT as usize);
    match host {
        StripHost::LoungeCard => words[HINT_ROW] = hint,
        StripHost::ZenTile => {}
    }

    picture.truncate(PICTURE_ROWS as usize);
    // Centre whatever the source drew in the fixed band.
    let top = (PICTURE_ROWS as usize).saturating_sub(picture.len()) / 2;
    let mut rows: Vec<Line<'static>> = (0..top).map(|_| Line::from("")).collect();
    rows.append(&mut picture);
    while rows.len() < PICTURE_ROWS as usize {
        rows.push(Line::from(""));
    }

    for (row, words) in rows.into_iter().zip(words) {
        let mut spans = clip_spans(row.spans, usize::from(PICTURE_COLS));
        let drawn: usize = spans.iter().map(|span| span.width()).sum();
        let pad = usize::from(PICTURE_COLS + GAP).saturating_sub(drawn);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.extend(words);
        lines.push(Line::from(spans));
    }
    match host {
        StripHost::LoungeCard => lines.push(rule_line(width, glow)),
        StripHost::ZenTile => {}
    }
    lines
}

/// A picture row cut to its column, so nothing a source draws can run into
/// the gap or the words.
fn clip_spans(spans: Vec<Span<'static>>, max: usize) -> Vec<Span<'static>> {
    let mut left = max;
    let mut out = Vec::with_capacity(spans.len());
    for span in spans {
        if span.width() <= left {
            left -= span.width();
            out.push(span);
            continue;
        }
        let mut kept = String::new();
        for ch in span.content.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w > left {
                break;
            }
            left -= w;
            kept.push(ch);
        }
        out.push(Span::styled(kept, span.style));
        break;
    }
    out
}

/// `── live ────`, the label lit while the source says so.
fn rule_line(width: u16, glow: bool) -> Line<'static> {
    let label_style = if glow {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::ITALIC)
    };
    let label = "live";
    let used = 3 + label.chars().count() + 1;
    let trail = usize::from(width).saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled("── ".to_string(), Style::default().fg(theme::BORDER_DIM())),
        Span::styled(label.to_string(), label_style),
        Span::raw(" "),
        Span::styled("─".repeat(trail), Style::default().fg(theme::BORDER_DIM())),
    ])
}

/// One row: the rule label, then what the source says in a line.
pub(crate) fn live_strip_compact_line(width: u16, strip: &LiveStripView<'_>) -> Line<'static> {
    let glow = match strip {
        LiveStripView::Match(strip) => match_strip::glow(strip),
        LiveStripView::Track(track) => booth_live::glow(track),
        // A shared link is never happening right now (`news_live::body`).
        LiveStripView::Article(_) => false,
        LiveStripView::Stream(_) => stream_live::glow(),
    };
    let mut rule = rule_line(width, glow);
    rule.spans.truncate(3);
    let used: usize = rule.spans.iter().map(|span| span.width()).sum();
    let rest = width.saturating_sub(used as u16);
    let mut spans = rule.spans;
    spans.extend(compact_body_spans(rest, strip));
    Line::from(spans)
}

/// What the source says in a line, `rest` columns at most.
fn compact_body_spans(rest: u16, strip: &LiveStripView<'_>) -> Vec<Span<'static>> {
    match strip {
        LiveStripView::Match(strip) => match_strip::compact_spans(rest, strip),
        LiveStripView::Track(track) => booth_live::compact_spans(rest, track),
        LiveStripView::Article(article) => news_live::compact_spans(rest, article),
        LiveStripView::Stream(stream) => stream_live::compact_spans(rest, stream),
    }
}

/// Columns the status line's Live segment gives what the strip says in a
/// line: enough for the kind, who, and the start of what. Kept short on
/// purpose: a segment fits whole or is dropped, and one that grew wide
/// exactly when something went live would be the one dropped.
const STATUS_COLS: u16 = 20;

/// What the strip shows, as the status line's Live segment reads it
/// (`statusline/`): the source's own one-row form, cut to `STATUS_COLS`.
pub(crate) fn status_text(strip: &LiveStripView<'_>) -> String {
    compact_body_spans(STATUS_COLS, strip)
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// One piece of a body's key hint row: a key to press, or the words around it.
pub(crate) enum HintPart<'a> {
    Key(&'a str),
    Text(&'a str),
}

/// The key hint every body carries (`o read · r reply`): each key in
/// amber like the hint bars, the words faint. Too narrow for the whole hint,
/// it falls back to one faint truncated run.
pub(crate) fn key_hint_spans(budget: usize, parts: &[HintPart<'_>]) -> Vec<Span<'static>> {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let full: String = parts
        .iter()
        .map(|part| match part {
            HintPart::Key(text) | HintPart::Text(text) => *text,
        })
        .collect();
    if full.chars().count() > budget {
        return vec![Span::styled(truncate_chars(&full, budget), faint)];
    }
    parts
        .iter()
        .map(|part| match part {
            HintPart::Key(key) => Span::styled(
                key.to_string(),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
            HintPart::Text(text) => Span::styled(text.to_string(), faint),
        })
        .collect()
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
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
#[path = "ui_test.rs"]
mod ui_test;
