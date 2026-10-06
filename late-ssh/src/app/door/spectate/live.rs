//! A door game on the live strip (`app/live/`): somebody started a game the
//! house can watch, so the room sees who and where they are, and can hop
//! into the watch with a key. A drawn dungeon room sits in the picture
//! column; the words beside it are the door, where the player is and who
//! else is watching, and who is playing.

use chrono::{DateTime, Utc};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::{
    common::theme,
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{HintPart, PICTURE_COLS, PICTURE_ROWS, StripBody, key_hint_spans, truncate_chars},
    },
};

use super::proxy::LiveGame;
use super::state::{LiveGameKey, LiveRow, SpectateGame};
use super::ui::{duration_label, minutes_since};

/// A live door game as the live strip paints it.
#[derive(Clone, Debug)]
pub struct DoorGameStripView {
    pub key: LiveGameKey,
    pub entry: LiveGame,
}

/// Every live game on the watchable doors' rosters, stamped with when it
/// started: a game is news when it starts. The rosters come from the door
/// hosts, so every replica reads the same stamps. The viewer's own running
/// game is never offered: there is nothing to hop into.
pub(crate) fn candidates(live: &[LiveRow], own: Option<LiveGameKey>) -> Vec<LiveCandidate> {
    live.iter()
        .filter_map(|row| {
            let key = LiveGameKey::new(row.game, &row.entry.playname)?;
            if Some(key) == own {
                return None;
            }
            Some(LiveCandidate {
                source: LiveSource::DoorGame(key),
                updated: started_at(&row.entry)?,
                aimed_at: None,
            })
        })
        .collect()
}

fn started_at(entry: &LiveGame) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(i64::try_from(entry.started_unix).ok()?, 0)
}

/// One live game as the strip paints it. `None` once it ended.
pub(crate) fn view(live: &[LiveRow], key: LiveGameKey) -> Option<DoorGameStripView> {
    live.iter()
        .find(|row| row.game == key.game() && row.entry.playname == key.playname())
        .map(|row| DoorGameStripView {
            key,
            entry: row.entry.clone(),
        })
}

pub(crate) fn body(budget: usize, strip: &DoorGameStripView) -> StripBody {
    StripBody {
        picture: picture(strip.key.game()),
        words: word_rows(budget, strip),
        hint: key_hint_spans(
            budget,
            &[HintPart::Key("o"), HintPart::Text(" or click to watch")],
        ),
        glow: glow(),
    }
}

/// A game on the strip is being played for as long as it is offered.
pub(crate) fn glow() -> bool {
    true
}

/// `dcss mat · XL3 Lair:2`, after the rule label.
pub(crate) fn compact_spans(rest: u16, strip: &DoorGameStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!(
        "{} {} · ",
        strip.key.game().door_game().key(),
        strip.key.playname()
    );
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&whereabouts(&strip.entry), left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

/// Where the player is (`XL3 Lair:2`, read off the game's own screen by the
/// host), or how long they have been in until the host has read it.
fn whereabouts(entry: &LiveGame) -> String {
    match entry.status.is_empty() {
        true => format!("{} in", duration_label(minutes_since(entry.started_unix))),
        false => entry.status.clone(),
    }
}

/// The door's picture: the watched screen lives behind the key, never in
/// the strip, so each door gets a drawing of itself.
fn picture(game: SpectateGame) -> Vec<Line<'static>> {
    match game {
        SpectateGame::Dcss => dungeon_room(),
    }
}

/// A walled room with the player's `@` in it and a door out, framed like a
/// screen.
fn dungeon_room() -> Vec<Line<'static>> {
    const ROOM: [&str; 5] = [
        "    ###########    ",
        "    #.........#    ",
        "    #....@....+..  ",
        "    #.........#    ",
        "    ###########    ",
    ];
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let wall = Style::default().fg(theme::TEXT_DIM());
    let floor = Style::default().fg(theme::TEXT_FAINT());
    let player = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner)),
            frame,
        ))
    };
    let mut lines = vec![edge("╭", "╮")];
    for row in ROOM {
        let mut spans = vec![Span::styled("│".to_string(), frame)];
        spans.extend(row.chars().map(|ch| {
            let style = match ch {
                '@' => player,
                '#' | '+' => wall,
                _ => floor,
            };
            Span::styled(ch.to_string(), style)
        }));
        spans.push(Span::styled("│".to_string(), frame));
        lines.push(Line::from(spans));
    }
    lines.push(edge("╰", "╯"));
    lines
}

/// The words beside the picture, one entry per picture row: the door, where
/// the player is and who is watching, and who is playing.
fn word_rows(budget: usize, strip: &DoorGameStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(strip.key.game().label(), budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    let watching = match strip.entry.watchers {
        0 => String::new(),
        watchers => format!(" · {watchers} watching"),
    };
    rows[2] = vec![Span::styled(
        truncate_chars(&format!("{}{watching}", whereabouts(&strip.entry)), budget),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    rows[3] = player_spans(budget, strip);
    rows
}

/// `mat is playing`, the player in amber.
fn player_spans(budget: usize, strip: &DoorGameStripView) -> Vec<Span<'static>> {
    let name = truncate_chars(strip.key.playname(), budget);
    let left = budget.saturating_sub(name.chars().count());
    vec![
        Span::styled(
            name,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_chars(" is playing", left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

#[cfg(test)]
#[path = "live_test.rs"]
mod live_test;
