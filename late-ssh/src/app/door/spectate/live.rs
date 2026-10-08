//! A door game on the live strip (`app/live/`): somebody started a game the
//! house can watch, so the room sees who and where they are, and can hop
//! into the watch with a key. A drawing of the door's own dungeon sits in
//! the picture column (a crawl room, a NetHack room, a Brogue cavern); the
//! words beside it are the door, where the player is and who else is
//! watching, and who is playing.

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
    /// People with this game's watch open (`LiveGamesService::watchers_of`).
    pub watching: usize,
}

/// Every live game on the watchable doors' rosters, stamped with when it
/// started: a game is news when it starts. The rosters come from the door
/// hosts, so every replica reads the same stamps. The viewer's own games are
/// offered like anyone else's: a live surface never filters the viewer out.
pub(crate) fn candidates(live: &[LiveRow]) -> Vec<LiveCandidate> {
    live.iter()
        .filter_map(|row| {
            Some(LiveCandidate {
                source: LiveSource::DoorGame(row.key),
                updated: started_at(&row.entry)?,
                aimed_at: None,
            })
        })
        .collect()
}

fn started_at(entry: &LiveGame) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(i64::try_from(entry.started_unix).ok()?, 0)
}

/// One live game as the strip paints it, `watching` the people with its
/// watch open. `None` once it ended.
pub(crate) fn view(
    live: &[LiveRow],
    key: LiveGameKey,
    watching: usize,
) -> Option<DoorGameStripView> {
    live.iter()
        .find(|row| row.key == key)
        .map(|row| DoorGameStripView {
            key,
            entry: row.entry.clone(),
            watching,
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
/// the strip, so each door gets a drawing of itself in its own dungeon's
/// vocabulary.
fn picture(game: SpectateGame) -> Vec<Line<'static>> {
    match game {
        SpectateGame::Dcss => framed(&DCSS_ROOM, dcss_glyph),
        SpectateGame::Nethack => framed(&NETHACK_ROOM, nethack_glyph),
        SpectateGame::Brogue => framed(&BROGUE_CAVE, brogue_glyph),
    }
}

/// A walled crawl room with the player's `@` in it and a door out.
const DCSS_ROOM: [&str; 5] = [
    "    ###########    ",
    "    #.........#    ",
    "    #....@....+..  ",
    "    #.........#    ",
    "    ###########    ",
];

fn dcss_glyph(ch: char) -> Style {
    match ch {
        '@' => player_style(),
        '#' | '+' => Style::default().fg(theme::TEXT_DIM()),
        _ => Style::default().fg(theme::TEXT_FAINT()),
    }
}

/// A NetHack room: `-` and `|` walls, the player beside their little dog, a
/// fountain, gold, the downstairs, and a door onto a corridor.
const NETHACK_ROOM: [&str; 5] = [
    "  -----------      ",
    "  |...{.....|      ",
    "  |.@.d.....+####  ",
    "  |.....$..>|   #  ",
    "  -----------      ",
];

fn nethack_glyph(ch: char) -> Style {
    match ch {
        '@' => player_style(),
        'd' => Style::default().fg(theme::TEXT_BRIGHT()),
        '{' => Style::default().fg(theme::CHAT_AUTHOR()),
        '$' => Style::default().fg(theme::BADGE_GOLD()),
        '+' => Style::default().fg(theme::AMBER_DIM()),
        '>' => Style::default().fg(theme::TEXT()),
        '-' | '|' => Style::default().fg(theme::TEXT_DIM()),
        _ => Style::default().fg(theme::TEXT_FAINT()),
    }
}

/// A Brogue cavern: ragged walls, grass, deep water, a potion, and the
/// stairs down.
const BROGUE_CAVE: [&str; 5] = [
    "   ####   ######   ",
    "  ##\"\"\"####..!.##  ",
    "  #\"\"@\"\"\"...~~~.#  ",
    "  ##..\"\"...~~~~>#  ",
    "   #############   ",
];

fn brogue_glyph(ch: char) -> Style {
    match ch {
        '@' => player_style(),
        '"' => Style::default().fg(theme::BONSAI_LEAF()),
        '~' => Style::default().fg(theme::CHAT_AUTHOR()),
        '!' => Style::default().fg(theme::MENTION()),
        '>' => Style::default().fg(theme::TEXT()),
        '#' => Style::default().fg(theme::TEXT_DIM()),
        _ => Style::default().fg(theme::TEXT_FAINT()),
    }
}

/// The player's `@`, the one lit glyph in every picture.
fn player_style() -> Style {
    Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD)
}

/// `rows` framed like a screen, each glyph styled by `glyph`.
fn framed(rows: &[&str], glyph: fn(char) -> Style) -> Vec<Line<'static>> {
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner)),
            frame,
        ))
    };
    let mut lines = vec![edge("╭", "╮")];
    for row in rows {
        let mut spans = vec![Span::styled("│".to_string(), frame)];
        spans.extend(
            row.chars()
                .map(|ch| Span::styled(ch.to_string(), glyph(ch))),
        );
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
    let watching = match strip.watching {
        0 => String::new(),
        watching => format!(" · {watching} watching"),
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
