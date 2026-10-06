//! A booth track on the live strip (`app/live/`): somebody put a track in
//! the YouTube queue, so the room sees what it is and who brought it, and
//! can tune in with a key. The thumbnail sits in the picture column,
//! painted as symbols picked for the viewer's terminal (`render_picture`), a
//! drawn screen until it has loaded; the words beside it are the title, the
//! channel and length, and who queued it and where it stands.

use anyhow::Result;
use image::RgbaImage;
use late_core::models::user::AudioSource;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::app::{
    audio::{
        svc::{QueueItemView, QueueSnapshot},
        thumbnail::THUMBNAIL_ROWS,
    },
    common::theme,
    files::inline_image::{InlineImageRenderSettings, render_rgba_preview},
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{HintPart, PICTURE_COLS, PICTURE_ROWS, StripBody, key_hint_spans, truncate_chars},
    },
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
    /// The thumbnail as this session's terminal paints it; `None` until it
    /// has loaded, or when it could not be fetched.
    pub picture: Option<Vec<Line<'static>>>,
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

/// A thumbnail as symbols, the picture column wide and `THUMBNAIL_ROWS`
/// tall. The symbols are picked for the viewer's terminal, so this is
/// rendered per session, once per thumbnail, never per frame.
pub(crate) fn render_picture(
    thumbnail: &RgbaImage,
    settings: InlineImageRenderSettings,
) -> Result<Vec<Line<'static>>> {
    render_rgba_preview(
        thumbnail,
        u32::from(PICTURE_COLS),
        u32::from(THUMBNAIL_ROWS),
        settings,
    )
}

/// One track as the strip paints it. `None` once it left the booth.
pub(crate) fn view(
    snapshot: &QueueSnapshot,
    item_id: Uuid,
    listening_on: AudioSource,
    picture: Option<Vec<Line<'static>>>,
) -> Option<TrackStripView> {
    let listening = match listening_on {
        AudioSource::Youtube => true,
        AudioSource::Radio => false,
    };
    if let Some(item) = snapshot.current.as_ref().filter(|item| item.id == item_id) {
        return Some(TrackStripView {
            item: item.clone(),
            place: TrackPlace::Playing,
            picture,
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
        picture,
        listening,
    })
}

pub(crate) fn body(budget: usize, track: &TrackStripView) -> StripBody {
    StripBody {
        picture: match &track.picture {
            Some(picture) => picture.clone(),
            None => screen_lines(),
        },
        words: word_rows(budget, track),
        hint: hint_spans(budget, track),
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

/// A drawn screen with a play mark, the size a thumbnail takes, for a track
/// whose thumbnail has not loaded or could not be fetched.
fn screen_lines() -> Vec<Line<'static>> {
    const SCREEN_ROWS: usize = THUMBNAIL_ROWS as usize;
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
/// channel and length, and who queued it and where it stands.
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
    rows
}

/// How to tune in, or to open the booth for a viewer already on YouTube.
fn hint_spans(budget: usize, track: &TrackStripView) -> Vec<Span<'static>> {
    let action = if track.listening {
        " or click for the booth"
    } else {
        " or click to tune in"
    };
    key_hint_spans(budget, &[HintPart::Key("o"), HintPart::Text(action)])
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
