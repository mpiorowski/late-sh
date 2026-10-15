//! The spawn on the composer: one glyph and one line in the placeholder
//! of #lounge's composer, for a runner's session only, and the glyph
//! alone in the composer's title while they type. Pure: the chat's
//! composer block draws what `line` hands it.

use late_core::models::deadchannel_spawn::Spawn;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use uuid::Uuid;

use super::state::{composer_text, glyph};
use crate::app::common::theme;

/// What the composer draws for the lounge: the glyph, the words, and
/// whether the `/hit` key follows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoungeLine {
    pub glyph: char,
    pub text: String,
    pub hit_key: bool,
}

/// The line for `viewer`'s composer: the session's own `word` while it
/// stands, else what the spawn says.
pub(crate) fn line(spawn: &Spawn, word: Option<&str>, viewer: Uuid) -> LoungeLine {
    match word {
        Some(word) => LoungeLine {
            glyph: glyph(spawn.kind),
            text: word.to_string(),
            hit_key: false,
        },
        None => {
            let said = composer_text(spawn, viewer);
            LoungeLine {
                glyph: glyph(spawn.kind),
                text: said.text,
                hit_key: said.hit_key,
            }
        }
    }
}

/// The placeholder row: the glyph lit, the words dim, `/hit` lit after
/// them when the line carries the key.
pub(crate) fn placeholder(line: &LoungeLine) -> Line<'static> {
    let lit = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut spans = vec![
        Span::styled(line.glyph.to_string(), lit),
        Span::styled(" ", dim),
        Span::styled(line.text.clone(), dim),
    ];
    if line.hit_key {
        spans.push(Span::styled(" ", dim));
        spans.push(Span::styled("/hit", lit));
    }
    Line::from(spans)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
