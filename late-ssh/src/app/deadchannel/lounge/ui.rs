//! The spawn as a scene: three rows above #lounge's composer, for a
//! runner's session only. The hitters' portraits on the left, one seat
//! per hit the spawn needs, the lane a strike crosses, the spawn's face,
//! and the words beside it. Pure: the chat's dashboard card draws what
//! `scene` hands it, at the height `SCENE_HEIGHT` says.

use std::collections::HashMap;

use late_core::models::deadchannel_spawn::{Spawn, SpawnOutcome};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use uuid::Uuid;

use super::data::{EMPTY_SEAT, GHOST_PORTRAIT, STRIKE_FRAMES, face};
use super::state::{AFTERMATH_MS, STRIKE_MS, left_secs, scene_text};
use crate::app::common::theme;
use crate::app::deadchannel::runner::svc::RunnerEntry;
use crate::app::deadchannel::runner::ui::portrait_spans;

/// Rows the scene takes above the composer.
pub(crate) const SCENE_HEIGHT: u16 = 3;

/// One frame of the strike, ms: the half animation edge.
const STRIKE_FRAME_MS: i64 = 132;

/// What the dashboard card draws for the lounge: three rows, left to
/// right the seats, the lane, the face, the words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoungeScene {
    pub rows: [Line<'static>; SCENE_HEIGHT as usize],
}

/// The scene for `viewer` at `now_ms`: the hitters' looks off the
/// directory (`looks`), the session's own `word` in the detail row while
/// it stands, the strike for `STRIKE_MS` after the last hit, and the face
/// dissolving through the aftermath once the spawn ended.
pub(crate) fn scene(
    spawn: &Spawn,
    word: Option<&str>,
    viewer: Uuid,
    looks: &HashMap<Uuid, RunnerEntry>,
    now_ms: i64,
) -> LoungeScene {
    let lit = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let gap = Span::styled("  ", dim);

    let strike = spawn
        .hits
        .last()
        .filter(|hit| spawn.ended_at_ms.is_none() && now_ms < hit.at_ms + STRIKE_MS)
        .map(|hit| ((now_ms - hit.at_ms).max(0) / STRIKE_FRAME_MS) as usize % STRIKE_FRAMES.len());
    let landing = strike == Some(STRIKE_FRAMES.len() - 1);

    let seats: Vec<[Span<'static>; 3]> = (0..spawn.hits_needed.max(1) as usize)
        .map(|seat| match spawn.hits.get(seat) {
            Some(hit) => match looks.get(&hit.user_id) {
                Some(entry) => portrait_spans(&entry.look),
                None => GHOST_PORTRAIT.map(|row| Span::styled(row, dim)),
            },
            None => EMPTY_SEAT.map(|row| Span::styled(row, faint)),
        })
        .collect();

    let face_style = match spawn.outcome {
        None if landing => Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
        None => lit,
        Some(SpawnOutcome::Slain) => Style::default().fg(theme::AMBER_DIM()),
        Some(SpawnOutcome::Slipped) => faint,
    };
    let face_rows = face(spawn.kind).map(|row| dissolve(row, spawn.ended_at_ms, now_ms));

    let said = scene_text(spawn, viewer);
    let detail = match word {
        Some(word) => Span::styled(word.to_string(), text),
        None => Span::styled(said.detail, dim),
    };
    let mut key_row = Vec::new();
    if said.hit_key {
        key_row.push(Span::styled("/hit", lit));
        key_row.push(gap.clone());
    }
    if let Some(secs) = left_secs(spawn, now_ms) {
        key_row.push(Span::styled(
            format!("{}:{:02} left", secs / 60, secs % 60),
            dim,
        ));
    }
    let words: [Vec<Span<'static>>; 3] = [
        vec![Span::styled(said.headline, text)],
        vec![detail],
        key_row,
    ];

    let rows = std::array::from_fn(|row| {
        let mut spans = vec![Span::raw(" ")];
        for (seat, portrait) in seats.iter().enumerate() {
            if seat > 0 {
                spans.push(gap.clone());
            }
            spans.push(portrait[row].clone());
        }
        spans.push(gap.clone());
        let lane = match (row, strike) {
            (1, Some(frame)) => Span::styled(STRIKE_FRAMES[frame], lit),
            _ => Span::styled(" ".repeat(STRIKE_FRAMES[0].chars().count()), dim),
        };
        spans.push(lane);
        spans.push(gap.clone());
        spans.push(Span::styled(face_rows[row].clone(), face_style));
        spans.push(Span::styled("   ", dim));
        spans.extend(words[row].iter().cloned());
        Line::from(spans)
    });
    LoungeScene { rows }
}

/// A face row with its cells going to static: none while the spawn is
/// live, more of them the further into the aftermath, in a fixed order
/// per cell so a frame never puts a cell back.
fn dissolve(row: &str, ended_at_ms: Option<i64>, now_ms: i64) -> String {
    let Some(ended) = ended_at_ms else {
        return row.to_string();
    };
    let gone_pct = ((now_ms - ended).clamp(0, AFTERMATH_MS) * 100 / AFTERMATH_MS) as usize;
    row.chars()
        .enumerate()
        .map(|(col, cell)| {
            let order = (col * 37 + row.len() * 11) % 100;
            if order < gone_pct { ' ' } else { cell }
        })
        .collect()
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
