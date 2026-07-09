//! Right-sidebar Activity panel: online headcount plus the rolling feed of
//! recent `ActivityEvent`s. Unlike the other sidebar panels this one is
//! flexible — it renders at `ACTIVITY_PANEL_MIN_HEIGHT` or taller, absorbing
//! whatever rows the rail has left over.

use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::event::ActivityEvent;
use crate::app::common::theme;

/// Smallest useful panel: header + a few event rows. Used as the panel's
/// footprint when the sidebar decides which panels fit.
pub(crate) const ACTIVITY_PANEL_MIN_HEIGHT: u16 = 5;

const ACTIVE_FRIEND_MARKER: &str = "★";
const ACTIVE_FRIEND_NAME_LIMIT: usize = 4;

pub(crate) struct ActivityPanelProps<'a> {
    pub events: &'a VecDeque<ActivityEvent>,
    pub online_count: usize,
    pub active_friend_names: &'a [String],
    /// Mouse-wheel scroll offset. `0` shows the newest event first; larger
    /// values reveal older events.
    pub scroll: u16,
    /// Free-running frame counter driving the horizontal marquee on event
    /// rows too long for the rail.
    pub marquee_tick: usize,
}

/// Event rows available in a panel of `height` rows: one row goes to the
/// header, one to the active-friends row when present.
pub(crate) fn visible_event_rows(height: u16, has_active_friends: bool) -> usize {
    (height as usize)
        .saturating_sub(1)
        .saturating_sub(has_active_friends as usize)
}

pub(crate) fn draw_activity_inline(
    frame: &mut Frame,
    area: Rect,
    props: &ActivityPanelProps<'_>,
    rect_slot: Option<&std::cell::Cell<Option<Rect>>>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if let Some(slot) = rect_slot {
        slot.set(Some(area));
    }

    let row = |offset: u16| Rect {
        x: area.x,
        y: area.y + offset,
        width: area.width,
        height: 1,
    };

    // Header: dim italic label + green presence dot + bold count.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "online",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled("● ", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                props.online_count.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" here", Style::default().fg(theme::TEXT_DIM())),
        ])),
        row(0),
    );

    let mut next_row = 1u16;
    if !props.active_friend_names.is_empty() && area.height > 1 {
        draw_active_friends_row(frame, row(1), props.active_friend_names);
        next_row = 2;
    }

    let visible = visible_event_rows(area.height, !props.active_friend_names.is_empty());
    // Clamp the scroll offset to the events beyond the visible window so a
    // buffer trim can't strand the view past the end.
    let max_offset = props.events.len().saturating_sub(visible);
    let offset = (props.scroll as usize).min(max_offset);

    let mut events = props.events.iter().rev().skip(offset);
    let mut drawn = 0;
    for offset_row in next_row..area.height {
        let Some(event) = events.next() else { break };
        frame.render_widget(
            Paragraph::new(event_line(event, area.width as usize, props.marquee_tick)),
            row(offset_row),
        );
        drawn += 1;
    }
    if drawn == 0 && next_row < area.height {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "the room is quiet",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            row(next_row),
        );
    }
}

/// One row per event. The action shares the row with the name and timestamp;
/// when it's too long to fit, it scrolls horizontally so the whole message is
/// readable without wrapping.
fn event_line(event: &ActivityEvent, body_w: usize, marquee_tick: usize) -> Line<'static> {
    let elapsed = event.at.elapsed().as_secs();
    let ago = if elapsed < 60 {
        format!("{}s", elapsed)
    } else if elapsed < 3600 {
        format!("{}m", elapsed / 60)
    } else {
        format!("{}h", elapsed / 3600)
    };
    let user = truncate(&event.username, 12);
    let user_part = format!("@{}", user);
    // Columns the action gets, sharing the row with the name and timestamp.
    let action_w = body_w.saturating_sub(user_part.chars().count() + ago.chars().count() + 4);
    let action = marquee_text(&event.action, action_w, marquee_tick);
    Line::from(vec![
        Span::styled(user_part, Style::default().fg(theme::TEXT())),
        Span::raw("  "),
        Span::styled(action, Style::default().fg(theme::TEXT_DIM())),
        Span::raw("  "),
        Span::styled(ago, Style::default().fg(theme::TEXT_FAINT())),
    ])
}

fn draw_active_friends_row(frame: &mut Frame, row: Rect, active_friend_names: &[String]) {
    let names = compact_friend_names(active_friend_names, row.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                ACTIVE_FRIEND_MARKER,
                Style::default()
                    .fg(theme::BADGE_GOLD())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                names,
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        row,
    );
}

fn compact_friend_names(names: &[String], width: usize) -> String {
    let mut pieces: Vec<String> = names
        .iter()
        .take(ACTIVE_FRIEND_NAME_LIMIT)
        .map(|name| format!("@{}", truncate(name, 10)))
        .collect();
    if names.len() > ACTIVE_FRIEND_NAME_LIMIT {
        pieces.push(format!("+{}", names.len() - ACTIVE_FRIEND_NAME_LIMIT));
    }
    truncate(
        &pieces.join(" "),
        width.saturating_sub(ACTIVE_FRIEND_MARKER.chars().count() + 1),
    )
}

fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max - 1).collect();
    out.push('…');
    out
}

/// Render `text` into a `width`-column window. Text that fits is returned
/// unchanged; longer text scrolls back and forth so the whole thing can be
/// read in place. `tick` advances once per world tick (~66ms); the window
/// holds briefly at each end before reversing so both edges stay readable.
fn marquee_text(text: &str, width: usize, tick: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if width == 0 || chars.len() <= width {
        return text.to_string();
    }
    let travel = chars.len() - width; // furthest left the window can scroll
    let hold = 20; // ticks paused at each extreme (~1.3s) before reversing
    let step = 3; // ticks per column of movement
    let sweep = travel * step;
    let period = 2 * hold + 2 * sweep;
    let t = tick % period;
    let offset = if t < hold {
        0
    } else if t < hold + sweep {
        (t - hold) / step
    } else if t < 2 * hold + sweep {
        travel
    } else {
        travel - (t - 2 * hold - sweep) / step
    }
    .min(travel);
    chars[offset..offset + width].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_event_rows_reserves_header_and_friends_rows() {
        assert_eq!(visible_event_rows(5, false), 4);
        assert_eq!(visible_event_rows(5, true), 3);
        assert_eq!(visible_event_rows(12, false), 11);
        assert_eq!(visible_event_rows(1, false), 0);
        assert_eq!(visible_event_rows(0, true), 0);
    }

    #[test]
    fn marquee_returns_text_that_fits_unchanged() {
        assert_eq!(marquee_text("short", 10, 42), "short");
    }

    #[test]
    fn marquee_holds_at_start_then_scrolls() {
        // 8 chars in a 5-col window: travel 3, hold 20, step 3.
        assert_eq!(marquee_text("abcdefgh", 5, 0), "abcde");
        assert_eq!(marquee_text("abcdefgh", 5, 19), "abcde");
        assert_eq!(marquee_text("abcdefgh", 5, 23), "bcdef");
    }

    #[test]
    fn compact_friend_names_caps_and_counts_overflow() {
        let names: Vec<String> = (1..=6).map(|i| format!("friend{i}")).collect();
        let compact = compact_friend_names(&names, 200);
        assert!(compact.contains("@friend1"));
        assert!(compact.contains("@friend4"));
        assert!(!compact.contains("@friend5"));
        assert!(compact.ends_with("+2"));
    }
}
