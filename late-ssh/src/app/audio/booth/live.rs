//! A booth track on the live strip (`app/live/`): somebody put a track in
//! the YouTube queue, so the room sees what it is and who brought it, and
//! can tune in with a key. The thumbnail sits in the picture column, a
//! drawn screen until it has loaded; the words beside it are the title, the
//! channel and length, and who queued it and where it stands.

use image::Rgba;
use late_core::models::user::AudioSource;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::app::{
    audio::svc::{QueueItemView, QueueSnapshot},
    common::theme,
    games::pool_core::canvas::Rgb,
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{PICTURE_COLS, PICTURE_ROWS, StripBody, truncate_chars},
    },
    lobby::house::image_render::img_to_lines,
};

use super::ui::format_queue_duration;

/// Where a track stands in the booth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackPlace {
    Playing,
    UpNext,
    /// Tracks ahead of it in the queue, at least one.
    Behind(usize),
}

/// A booth track as the live strip paints it.
#[derive(Clone, Debug)]
pub struct TrackStripView {
    pub item: QueueItemView,
    pub place: TrackPlace,
    /// Whether the viewer is on the YouTube source already: the key then
    /// opens the booth instead of tuning in.
    pub listening: bool,
}

/// Every track in the booth, playing or waiting, stamped with when it was
/// queued: a track is news when somebody brings it.
pub(crate) fn candidates(snapshot: &QueueSnapshot) -> Vec<LiveCandidate> {
    snapshot
        .current
        .iter()
        .chain(snapshot.queue.iter())
        .map(|item| LiveCandidate {
            source: LiveSource::BoothTrack(item.id),
            updated: item.queued_at,
            aimed_at: None,
        })
        .collect()
}

/// One track as the strip paints it. `None` once it left the booth.
pub(crate) fn view(
    snapshot: &QueueSnapshot,
    item_id: Uuid,
    listening_on: AudioSource,
) -> Option<TrackStripView> {
    let listening = match listening_on {
        AudioSource::Youtube => true,
        AudioSource::Radio | AudioSource::Icecast => false,
    };
    if let Some(item) = snapshot.current.as_ref().filter(|item| item.id == item_id) {
        return Some(TrackStripView {
            item: item.clone(),
            place: TrackPlace::Playing,
            listening,
        });
    }
    let ahead = snapshot.queue.iter().position(|item| item.id == item_id)?;
    let place = match ahead {
        0 => TrackPlace::UpNext,
        ahead => TrackPlace::Behind(ahead),
    };
    Some(TrackStripView {
        item: snapshot.queue[ahead].clone(),
        place,
        listening,
    })
}

pub(crate) fn body(budget: usize, track: &TrackStripView, background: Rgb) -> StripBody {
    StripBody {
        picture: picture(track, background),
        words: word_rows(budget, track),
        glow: glow(track),
    }
}

/// The label is lit while the track is the one playing.
pub(crate) fn glow(track: &TrackStripView) -> bool {
    match track.place {
        TrackPlace::Playing => true,
        TrackPlace::UpNext | TrackPlace::Behind(_) => false,
    }
}

/// `booth mat · Song Title`, after the rule label.
pub(crate) fn compact_spans(rest: u16, track: &TrackStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!("booth {} · ", submitter(track));
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&title(track), left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

fn picture(track: &TrackStripView, background: Rgb) -> Vec<Line<'static>> {
    match &track.item.thumbnail {
        Some(thumbnail) => {
            let [r, g, b] = background;
            img_to_lines(thumbnail, None, Rgba([r, g, b, 255]))
        }
        None => screen_lines(),
    }
}

/// A drawn screen with a play mark, the size a thumbnail takes, for a track
/// whose thumbnail has not loaded or could not be fetched.
fn screen_lines() -> Vec<Line<'static>> {
    const SCREEN_ROWS: usize = 6;
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner)),
            frame,
        ))
    };
    let mut lines = vec![edge("╭", "╮")];
    for row in 1..SCREEN_ROWS - 1 {
        let middle = if row == SCREEN_ROWS / 2 {
            let lead = (inner - 1) / 2;
            vec![
                Span::styled("│".to_string(), frame),
                Span::raw(" ".repeat(lead)),
                Span::styled(
                    "▶".to_string(),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" ".repeat(inner - 1 - lead)),
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

/// The words beside the picture, one entry per picture row: the title, the
/// channel and length, who queued it and where it stands, then how to tune
/// in.
fn word_rows(budget: usize, track: &TrackStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(&title(track), budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    rows[2] = vec![Span::styled(
        truncate_chars(&about(&track.item), budget),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    rows[3] = event_spans(budget, track);
    rows[6] = vec![Span::styled(
        truncate_chars(
            if track.listening {
                "o or click for the booth"
            } else {
                "o or click to tune in"
            },
            budget,
        ),
        Style::default().fg(theme::TEXT_FAINT()),
    )];
    rows
}

fn title(track: &TrackStripView) -> String {
    match &track.item.title {
        Some(title) => title.clone(),
        None => "a track".to_string(),
    }
}

fn submitter(track: &TrackStripView) -> String {
    if track.item.submitter.is_empty() {
        "somebody".to_string()
    } else {
        track.item.submitter.clone()
    }
}

/// `Channel · 3:45`, either half alone when the other is unknown.
fn about(item: &QueueItemView) -> String {
    let length = format_queue_duration(item);
    match (&item.channel, length.is_empty()) {
        (Some(channel), false) => format!("{channel} · {length}"),
        (Some(channel), true) => channel.clone(),
        (None, false) => length,
        (None, true) => "YouTube".to_string(),
    }
}

/// `mat queued it · up next`, the submitter in amber.
fn event_spans(budget: usize, track: &TrackStripView) -> Vec<Span<'static>> {
    let standing = match track.place {
        TrackPlace::Playing => " put it on · playing now".to_string(),
        TrackPlace::UpNext => " queued it · up next".to_string(),
        TrackPlace::Behind(ahead) => format!(" queued it · #{} in line", ahead + 1),
    };
    let name = truncate_chars(&submitter(track), budget);
    let left = budget.saturating_sub(name.chars().count());
    vec![
        Span::styled(
            name,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_chars(&standing, left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

#[cfg(test)]
#[path = "live_test.rs"]
mod live_test;
